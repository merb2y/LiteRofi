//! Windows Shell, WIC, and HWND icon decoding into cached Direct2D bitmaps.
//! All Win32/COM/WIC `unsafe` icon logic is isolated here.

use crate::layout::Rect;
use literofi_theme::ImageScale;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HWND, LPARAM, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    ID2D1Bitmap, ID2D1RenderTarget, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{DeleteObject, HBITMAP};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource, IWICFormatConverter,
    IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapIgnoreAlpha,
    WICBitmapPaletteTypeMedianCut, WICBitmapUseAlpha, WICDecodeMetadataCacheOnLoad,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON,
    SHGFI_LARGEICON, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetClassLongPtrW, GetWindowThreadProcessId, PostMessageW, SendMessageTimeoutW,
    GCLP_HICON, GCLP_HICONSM, HICON, ICON_BIG, ICON_SMALL, SMTO_ABORTIFHUNG, WM_GETICON,
};

/// Custom window message posted by the background icon worker when decoded icons are ready.
pub const WM_APP_ICON_READY: u32 = 0x8001;

const MAX_CACHED_ICONS: usize = 256;

struct DecodedIconPixels {
    w: u32,
    h: u32,
    pixels: Vec<u8>,
}

type IconReqMsg = (String, u32, isize);
type IconResultMsg = ((String, u32), Option<DecodedIconPixels>);

/// Per-render-target cache of decoded [`ID2D1Bitmap`] icons backed by WIC and Windows Shell.
pub struct IconCache {
    wic: IWICImagingFactory,
    cache: HashMap<(String, u32), Option<ID2D1Bitmap>>,
    pending: HashSet<(String, u32)>,
    req_tx: Option<mpsc::Sender<IconReqMsg>>,
    res_rx: Option<mpsc::Receiver<IconResultMsg>>,
}

impl IconCache {
    pub fn new() -> Result<Self, String> {
        // SAFETY: Creates thread-local WIC imaging factory via documented CLSID after COM init.
        let wic: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
                .map_err(|e| format!("CoCreateInstance(WICImagingFactory) failed: {e}"))?;
        Ok(Self {
            wic,
            cache: HashMap::new(),
            pending: HashSet::new(),
            req_tx: None,
            res_rx: None,
        })
    }

    /// Drops cached render-target-bound bitmaps (required on `D2DERR_RECREATE_TARGET`).
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    fn ensure_worker(&mut self) {
        if self.req_tx.is_some() {
            return;
        }
        let (req_tx, req_rx) = mpsc::channel::<IconReqMsg>();
        let (res_tx, res_rx) = mpsc::channel::<IconResultMsg>();
        if std::thread::Builder::new()
            .name("literofi-icon-worker".to_string())
            .spawn(move || {
                // SAFETY: Initializes apartment-threaded COM on this dedicated background worker.
                let com_ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
                // SAFETY: Creates thread-local WIC factory after COM init.
                let wic_opt: Option<IWICImagingFactory> = unsafe {
                    CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()
                };
                while let Ok((spec, bucket, hwnd_raw)) = req_rx.recv() {
                    let px = wic_opt.as_ref().and_then(|wic| {
                        let out = extract_icon_pixels(wic, &spec, bucket);
                        if let (Some(dp), Some(data)) =
                            (icon_disk_cache_path(&spec, bucket), out.as_ref())
                        {
                            save_disk_cached_pixels(&dp, data.w, data.h, &data.pixels);
                        }
                        out
                    });
                    if res_tx.send(((spec, bucket), px)).is_err() {
                        break;
                    }
                    if hwnd_raw != 0 {
                        // SAFETY: Wakes the UI thread cleanly via async Win32 PostMessageW.
                        unsafe {
                            let _ = PostMessageW(
                                HWND(hwnd_raw as *mut _),
                                WM_APP_ICON_READY,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                    }
                }
                drop(wic_opt);
                if com_ok {
                    // SAFETY: Balanced with successful CoInitializeEx on this worker thread.
                    unsafe { CoUninitialize() };
                }
            })
            .is_ok()
        {
            self.req_tx = Some(req_tx);
            self.res_rx = Some(res_rx);
        }
    }

    /// Returns a cached RAM or disk-cached icon bitmap immediately without blocking the UI thread.
    /// On cache miss, queues background Shell/WIC extraction and returns `None` until ready.
    pub fn get_cached_or_queue(
        &mut self,
        rt: &ID2D1RenderTarget,
        spec: &str,
        size_px: u32,
        notify_hwnd: HWND,
    ) -> Option<ID2D1Bitmap> {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.starts_with("hwnd:") {
            return self.get_or_load(rt, trimmed, size_px);
        }
        let bucket = bucket_icon_size(size_px);
        let key = (trimmed.to_string(), bucket);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        if self.cache.len() >= MAX_CACHED_ICONS {
            self.cache.clear();
        }
        if let Some(dp) = icon_disk_cache_path(trimmed, bucket) {
            // SAFETY: Creates D2D bitmap from validated local PBGRA cache bytes for this `rt`.
            if let Some(bmp) = unsafe { load_disk_cached_bitmap(rt, &dp) } {
                self.cache.insert(key, Some(bmp.clone()));
                return Some(bmp);
            }
        }
        if self.pending.insert(key.clone()) {
            self.ensure_worker();
            if let Some(tx) = &self.req_tx {
                let _ = tx.send((key.0, key.1, notify_hwnd.0 as isize));
            }
        }
        None
    }

    /// Transfers any asynchronously decoded icon bitmaps onto `rt` and returns whether any arrived.
    pub fn drain_async_ready(&mut self, rt: &ID2D1RenderTarget) -> bool {
        let Some(rx) = &self.res_rx else {
            return false;
        };
        let mut any = false;
        while let Ok((key, maybe_px)) = rx.try_recv() {
            self.pending.remove(&key);
            if self.cache.len() >= MAX_CACHED_ICONS {
                self.cache.clear();
            }
            let bmp = maybe_px.and_then(|px| {
                // SAFETY: Creates D2D bitmap on `rt` from valid 32bpp PBGRA worker buffer.
                unsafe { create_d2d_bitmap_from_pbgra(rt, px.w, px.h, &px.pixels) }
            });
            self.cache.insert(key, bmp);
            any = true;
        }
        any
    }

    /// Resolves and caches a Direct2D bitmap for `spec` (`hwnd:<id>`, `shell:AppsFolder\<AUMID>`,
    /// direct image file, `.lnk`, `.exe`, or folder/file path).
    pub fn get_or_load(
        &mut self,
        rt: &ID2D1RenderTarget,
        spec: &str,
        size_px: u32,
    ) -> Option<ID2D1Bitmap> {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            return None;
        }
        let bucket = bucket_icon_size(size_px);
        let key = (trimmed.to_string(), bucket);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        if self.cache.len() >= MAX_CACHED_ICONS {
            self.cache.clear();
        }
        let disk_path = icon_disk_cache_path(trimmed, bucket);
        if let Some(dp) = disk_path.as_deref() {
            // SAFETY: Creates D2D bitmap from validated local PBGRA cache bytes for this `rt`.
            if let Some(bmp) = unsafe { load_disk_cached_bitmap(rt, dp) } {
                self.cache.insert(key, Some(bmp.clone()));
                return Some(bmp);
            }
        }
        let loaded = self.load_uncached(rt, trimmed, bucket, disk_path.as_deref());
        self.cache.insert(key, loaded.clone());
        loaded
    }

    /// Loads a full-resolution background image file via WIC (supports extensionless files
    /// such as Windows `TranscodedWallpaper` and `.current_wallpaper`), without falling back
    /// to generic Shell unknown-file icons.
    pub fn get_or_load_image(&mut self, rt: &ID2D1RenderTarget, spec: &str) -> Option<ID2D1Bitmap> {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            return None;
        }
        let key = (format!("bgimg:{trimmed}"), 0);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        if self.cache.len() >= MAX_CACHED_ICONS {
            self.cache.clear();
        }
        let path = Path::new(trimmed);
        let loaded = if path.is_file() {
            self.load_image_file(rt, path, None)
        } else {
            None
        };
        self.cache.insert(key, loaded.clone());
        loaded
    }

    fn load_uncached(
        &self,
        rt: &ID2D1RenderTarget,
        spec: &str,
        bucket_px: u32,
        save_path: Option<&Path>,
    ) -> Option<ID2D1Bitmap> {
        if let Some(hwnd_raw) = spec.strip_prefix("hwnd:") {
            let hwnd_val = hwnd_raw.trim().parse::<isize>().ok()?;
            return self.load_window_icon(rt, hwnd_val, bucket_px);
        }

        let path = Path::new(spec);
        if (is_image_file_ext(path) || path.extension().is_none()) && path.is_file() {
            if let Some(bmp) = self.load_image_file(rt, path, save_path) {
                return Some(bmp);
            }
        }

        if let Some(bmp) = self.load_shell_item_icon(rt, spec, bucket_px, save_path) {
            return Some(bmp);
        }

        if path.is_relative() && !spec.contains(':') && !spec.contains('\\') && !spec.contains('/')
        {
            if let Some(on_path) = find_on_path(spec) {
                let p_str = on_path.to_string_lossy();
                if let Some(bmp) = self.load_shell_item_icon(rt, &p_str, bucket_px, save_path) {
                    return Some(bmp);
                }
            }
        }

        self.load_shgetfileinfo_icon(rt, spec, save_path)
    }

    fn load_image_file(
        &self,
        rt: &ID2D1RenderTarget,
        path: &Path,
        save_path: Option<&Path>,
    ) -> Option<ID2D1Bitmap> {
        let wpath = HSTRING::from(path.as_os_str());
        // SAFETY: Valid HSTRING file path and WIC COM factory calls.
        unsafe {
            let decoder = self
                .wic
                .CreateDecoderFromFilename(&wpath, None, GENERIC_READ, WICDecodeMetadataCacheOnLoad)
                .ok()?;
            let frame = decoder.GetFrame(0).ok()?;
            let src: IWICBitmapSource = frame.into();
            self.wic_source_to_d2d(rt, &src, save_path)
        }
    }

    fn load_shell_item_icon(
        &self,
        rt: &ID2D1RenderTarget,
        parsing_name: &str,
        bucket_px: u32,
        save_path: Option<&Path>,
    ) -> Option<ID2D1Bitmap> {
        let wname = HSTRING::from(parsing_name);
        // SAFETY: Valid parsing name passed to SHCreateItemFromParsingName + DeleteObject on returned HBITMAP.
        unsafe {
            let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&wname, None).ok()?;
            let side = i32::try_from(bucket_px).unwrap_or(32).max(16);
            let hbmp: HBITMAP = factory
                .GetImage(
                    SIZE { cx: side, cy: side },
                    SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
                )
                .ok()?;
            let bmp = self.hbitmap_to_d2d(rt, hbmp, save_path);
            let _ = DeleteObject(hbmp);
            bmp
        }
    }

    fn load_shgetfileinfo_icon(
        &self,
        rt: &ID2D1RenderTarget,
        path_or_ext: &str,
        save_path: Option<&Path>,
    ) -> Option<ID2D1Bitmap> {
        let wpath = HSTRING::from(path_or_ext);
        let mut sfi = SHFILEINFOW::default();
        // SAFETY: Valid SHFILEINFOW buffer passed to SHGetFileInfoW; destroys owned HICON afterward.
        unsafe {
            let ok = SHGetFileInfoW(
                &wpath,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut sfi),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            );
            if ok == 0 || sfi.hIcon.0.is_null() {
                return None;
            }
            let bmp = self.hicon_to_d2d(rt, sfi.hIcon, save_path);
            let _ = DestroyIcon(sfi.hIcon);
            bmp
        }
    }

    fn load_window_icon(
        &self,
        rt: &ID2D1RenderTarget,
        hwnd_raw: isize,
        bucket_px: u32,
    ) -> Option<ID2D1Bitmap> {
        let hwnd = HWND(hwnd_raw as *mut _);
        // SAFETY: Queries non-owning window/class HICON without blocking (50ms timeout) and falls back to exe path.
        unsafe {
            for kind in [
                ICON_BIG as usize,
                2usize, /* ICON_SMALL2 */
                ICON_SMALL as usize,
            ] {
                let mut res = 0usize;
                let lr = SendMessageTimeoutW(
                    hwnd,
                    WM_GETICON,
                    WPARAM(kind),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG,
                    50,
                    Some(&mut res),
                );
                if lr.0 != 0 && res != 0 {
                    if let Some(bmp) = self.hicon_to_d2d(rt, HICON(res as *mut _), None) {
                        return Some(bmp);
                    }
                }
            }
            for idx in [GCLP_HICON, GCLP_HICONSM] {
                let ptr = GetClassLongPtrW(hwnd, idx);
                if ptr != 0 {
                    if let Some(bmp) = self.hicon_to_d2d(rt, HICON(ptr as *mut _), None) {
                        return Some(bmp);
                    }
                }
            }
            if let Some(exe) = query_window_exe_path(hwnd) {
                if let Some(bmp) = self.load_shell_item_icon(rt, &exe, bucket_px, None) {
                    return Some(bmp);
                }
                return self.load_shgetfileinfo_icon(rt, &exe, None);
            }
            None
        }
    }

    unsafe fn hbitmap_to_d2d(
        &self,
        rt: &ID2D1RenderTarget,
        hbmp: HBITMAP,
        save_path: Option<&Path>,
    ) -> Option<ID2D1Bitmap> {
        let wic_bmp = self
            .wic
            .CreateBitmapFromHBITMAP(hbmp, None, WICBitmapUseAlpha)
            .or_else(|_| {
                self.wic
                    .CreateBitmapFromHBITMAP(hbmp, None, WICBitmapIgnoreAlpha)
            })
            .ok()?;
        let src: IWICBitmapSource = wic_bmp.into();
        self.wic_source_to_d2d(rt, &src, save_path)
    }

    unsafe fn hicon_to_d2d(
        &self,
        rt: &ID2D1RenderTarget,
        hicon: HICON,
        save_path: Option<&Path>,
    ) -> Option<ID2D1Bitmap> {
        let wic_bmp = self.wic.CreateBitmapFromHICON(hicon).ok()?;
        let src: IWICBitmapSource = wic_bmp.into();
        self.wic_source_to_d2d(rt, &src, save_path)
    }

    unsafe fn wic_source_to_d2d(
        &self,
        rt: &ID2D1RenderTarget,
        src: &IWICBitmapSource,
        save_path: Option<&Path>,
    ) -> Option<ID2D1Bitmap> {
        if let Some(disk_path) = save_path {
            let decoded = wic_source_to_pbgra(&self.wic, src, 256)?;
            save_disk_cached_pixels(disk_path, decoded.w, decoded.h, &decoded.pixels);
            return create_d2d_bitmap_from_pbgra(rt, decoded.w, decoded.h, &decoded.pixels);
        }
        // Full-resolution path for `background-image: url(...)` wallpapers & uncached images:
        // do not downscale to 256px or reject >2048px wallpapers, and normalize DPI to 96.
        let conv: IWICFormatConverter = self.wic.CreateFormatConverter().ok()?;
        conv.Initialize(
            src,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeMedianCut,
        )
        .ok()?;
        let conv_src: IWICBitmapSource = conv.into();
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
        };
        rt.CreateBitmapFromWicBitmap(&conv_src, Some(&props)).ok()
    }
}

fn extract_icon_pixels(
    wic: &IWICImagingFactory,
    spec: &str,
    bucket_px: u32,
) -> Option<DecodedIconPixels> {
    if spec.starts_with("hwnd:") {
        return None;
    }
    let path = Path::new(spec);
    if is_image_file_ext(path) && path.is_file() {
        let wpath = HSTRING::from(path.as_os_str());
        // SAFETY: Valid HSTRING path passed to WIC CreateDecoderFromFilename.
        let decoded = unsafe {
            wic.CreateDecoderFromFilename(&wpath, None, GENERIC_READ, WICDecodeMetadataCacheOnLoad)
                .ok()
                .and_then(|d| d.GetFrame(0).ok())
                .and_then(|f| {
                    let src: IWICBitmapSource = f.into();
                    wic_source_to_pbgra(wic, &src, 256)
                })
        };
        if decoded.is_some() {
            return decoded;
        }
    }
    if let Some(px) = extract_shell_item_pixels(wic, spec, bucket_px) {
        return Some(px);
    }
    if path.is_relative() && !spec.contains(':') && !spec.contains('\\') && !spec.contains('/') {
        if let Some(on_path) = find_on_path(spec) {
            let p_str = on_path.to_string_lossy();
            if let Some(px) = extract_shell_item_pixels(wic, &p_str, bucket_px) {
                return Some(px);
            }
        }
    }
    let wpath = HSTRING::from(spec);
    let mut sfi = SHFILEINFOW::default();
    // SAFETY: Queries shell HICON and extracts PBGRA bytes before DestroyIcon.
    unsafe {
        let ok = SHGetFileInfoW(
            &wpath,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut sfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if ok == 0 || sfi.hIcon.0.is_null() {
            return None;
        }
        let px = wic
            .CreateBitmapFromHICON(sfi.hIcon)
            .ok()
            .and_then(|b| wic_source_to_pbgra(wic, &b.into(), 256));
        let _ = DestroyIcon(sfi.hIcon);
        px
    }
}

fn extract_shell_item_pixels(
    wic: &IWICImagingFactory,
    parsing_name: &str,
    bucket_px: u32,
) -> Option<DecodedIconPixels> {
    let wname = HSTRING::from(parsing_name);
    // SAFETY: Queries IShellItemImageFactory::GetImage and extracts PBGRA bytes before DeleteObject.
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&wname, None).ok()?;
        let side = i32::try_from(bucket_px).unwrap_or(32).max(16);
        let hbmp: HBITMAP = factory
            .GetImage(
                SIZE { cx: side, cy: side },
                SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
            )
            .ok()?;
        let wic_bmp = wic
            .CreateBitmapFromHBITMAP(hbmp, None, WICBitmapUseAlpha)
            .or_else(|_| wic.CreateBitmapFromHBITMAP(hbmp, None, WICBitmapIgnoreAlpha))
            .ok();
        let px = wic_bmp.and_then(|b| wic_source_to_pbgra(wic, &b.into(), 256));
        let _ = DeleteObject(hbmp);
        px
    }
}

fn downscale_pbgra_box(src: DecodedIconPixels, max_dim: u32) -> DecodedIconPixels {
    if src.w <= max_dim && src.h <= max_dim {
        return src;
    }
    let scale = f64::from(max_dim) / f64::from(src.w.max(src.h));
    let nw = ((f64::from(src.w) * scale).round() as u32).clamp(1, max_dim);
    let nh = ((f64::from(src.h) * scale).round() as u32).clamp(1, max_dim);
    let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
    for ny in 0..nh {
        let sy0 = (u64::from(ny) * u64::from(src.h) / u64::from(nh)) as u32;
        let sy1 = (((u64::from(ny) + 1) * u64::from(src.h) / u64::from(nh)) as u32)
            .max(sy0 + 1)
            .min(src.h);
        for nx in 0..nw {
            let sx0 = (u64::from(nx) * u64::from(src.w) / u64::from(nw)) as u32;
            let sx1 = (((u64::from(nx) + 1) * u64::from(src.w) / u64::from(nw)) as u32)
                .max(sx0 + 1)
                .min(src.w);
            let mut sb = 0u32;
            let mut sg = 0u32;
            let mut sr = 0u32;
            let mut sa = 0u32;
            let mut cnt = 0u32;
            for yy in sy0..sy1 {
                for xx in sx0..sx1 {
                    let idx = ((yy * src.w + xx) as usize) * 4;
                    sb += u32::from(src.pixels[idx]);
                    sg += u32::from(src.pixels[idx + 1]);
                    sr += u32::from(src.pixels[idx + 2]);
                    sa += u32::from(src.pixels[idx + 3]);
                    cnt += 1;
                }
            }
            let dst = ((ny * nw + nx) as usize) * 4;
            out[dst] = (sb / cnt) as u8;
            out[dst + 1] = (sg / cnt) as u8;
            out[dst + 2] = (sr / cnt) as u8;
            out[dst + 3] = (sa / cnt) as u8;
        }
    }
    DecodedIconPixels {
        w: nw,
        h: nh,
        pixels: out,
    }
}

fn normalize_icon_pixels(dec: DecodedIconPixels) -> DecodedIconPixels {
    // Only trim large Shell jumbo canvases (>= 64px) where a small legacy icon was stamped with
    // wide transparent margins; leave genuine small test/UI bitmaps untouched.
    if dec.w < 64 || dec.h < 64 {
        return dec;
    }
    let mut min_x = dec.w;
    let mut min_y = dec.h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut found = false;
    for y in 0..dec.h {
        for x in 0..dec.w {
            let idx = ((y * dec.w + x) as usize) * 4;
            if dec.pixels[idx + 3] > 8 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                found = true;
            }
        }
    }
    if !found {
        return dec;
    }
    let bw = max_x - min_x + 1;
    let bh = max_y - min_y + 1;
    if bw.max(bh) * 100 >= dec.w.max(dec.h) * 68 {
        return dec;
    }
    let span = bw.max(bh);
    let margin = (span / 16).max(1);
    let side = (span + margin * 2).min(dec.w.max(dec.h));
    let mut out = vec![0u8; (side as usize) * (side as usize) * 4];
    let dst_x0 = (side - bw) / 2;
    let dst_y0 = (side - bh) / 2;
    for dy in 0..bh {
        for dx in 0..bw {
            let s_idx = (((min_y + dy) * dec.w + (min_x + dx)) as usize) * 4;
            let d_idx = (((dst_y0 + dy) * side + (dst_x0 + dx)) as usize) * 4;
            out[d_idx..d_idx + 4].copy_from_slice(&dec.pixels[s_idx..s_idx + 4]);
        }
    }
    DecodedIconPixels {
        w: side,
        h: side,
        pixels: out,
    }
}

unsafe fn wic_source_to_pbgra(
    wic: &IWICImagingFactory,
    src: &IWICBitmapSource,
    max_dim: u32,
) -> Option<DecodedIconPixels> {
    let conv: IWICFormatConverter = wic.CreateFormatConverter().ok()?;
    conv.Initialize(
        src,
        &GUID_WICPixelFormat32bppPBGRA,
        WICBitmapDitherTypeNone,
        None,
        0.0,
        WICBitmapPaletteTypeMedianCut,
    )
    .ok()?;
    let conv_src: IWICBitmapSource = conv.into();
    let mut w = 0u32;
    let mut h = 0u32;
    conv_src.GetSize(&mut w, &mut h).ok()?;
    if w == 0 || h == 0 || w > 2048 || h > 2048 {
        return None;
    }
    let stride = w * 4;
    let mut pixels = vec![0u8; (stride * h) as usize];
    conv_src
        .CopyPixels(std::ptr::null(), stride, &mut pixels)
        .ok()?;
    let dec = normalize_icon_pixels(DecodedIconPixels { w, h, pixels });
    Some(downscale_pbgra_box(dec, max_dim))
}

unsafe fn create_d2d_bitmap_from_pbgra(
    rt: &ID2D1RenderTarget,
    w: u32,
    h: u32,
    pixels: &[u8],
) -> Option<ID2D1Bitmap> {
    let expected = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    if w == 0 || h == 0 || pixels.len() < expected {
        return None;
    }
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
    };
    rt.CreateBitmap(
        D2D_SIZE_U {
            width: w,
            height: h,
        },
        Some(pixels.as_ptr() as *const _),
        w * 4,
        &props,
    )
    .ok()
}

fn icon_cache_dir() -> Option<PathBuf> {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.trim().is_empty() {
            return Some(
                PathBuf::from(local)
                    .join("LiteRofi")
                    .join("cache")
                    .join("icons_v2"),
            );
        }
    }
    None
}

fn fnv1a64(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn icon_disk_cache_path(spec: &str, bucket_px: u32) -> Option<PathBuf> {
    if spec.starts_with("hwnd:") || bucket_px > 512 {
        return None;
    }
    let dir = icon_cache_dir()?;
    Some(dir.join(format!("{:016x}_{bucket_px}.bgra", fnv1a64(spec))))
}

unsafe fn load_disk_cached_bitmap(rt: &ID2D1RenderTarget, path: &Path) -> Option<ID2D1Bitmap> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 12 {
        return None;
    }
    let w = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
    let h = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    if !(1..=512).contains(&w) || !(1..=512).contains(&h) {
        return None;
    }
    let expected = 8usize + (w as usize) * (h as usize) * 4;
    if bytes.len() != expected {
        return None;
    }
    create_d2d_bitmap_from_pbgra(rt, w, h, &bytes[8..])
}

fn save_disk_cached_pixels(path: &Path, w: u32, h: u32, pixels: &[u8]) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let mut buf = Vec::with_capacity(8 + pixels.len());
    buf.extend_from_slice(&w.to_le_bytes());
    buf.extend_from_slice(&h.to_le_bytes());
    buf.extend_from_slice(pixels);
    let tmp = path.with_extension(format!("bgra.tmp.{}", std::process::id()));
    if std::fs::write(&tmp, &buf).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Draws `bmp` aspect-fitted inside `rect` with `(xalign, yalign)` in `0.0..=1.0`.
///
/// # Safety
/// `rt` must be inside an active `BeginDraw`..`EndDraw` pass and `bmp` must belong to `rt`.
pub unsafe fn draw_bitmap_fitted_aligned(
    rt: &ID2D1RenderTarget,
    bmp: &ID2D1Bitmap,
    rect: Rect,
    xalign: f32,
    yalign: f32,
) {
    if rect.w <= 1.0 || rect.h <= 1.0 {
        return;
    }
    let sz = bmp.GetSize();
    if sz.width <= 0.0 || sz.height <= 0.0 {
        return;
    }
    let scale = (rect.w / sz.width).min(rect.h / sz.height);
    let dw = (sz.width * scale).max(1.0);
    let dh = (sz.height * scale).max(1.0);
    let x = rect.x + (rect.w - dw) * xalign.clamp(0.0, 1.0);
    let y = rect.y + (rect.h - dh) * yalign.clamp(0.0, 1.0);
    let dst = D2D_RECT_F {
        left: x,
        top: y,
        right: x + dw,
        bottom: y + dh,
    };
    rt.DrawBitmap(
        bmp,
        Some(&dst),
        1.0,
        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
        None,
    );
}

/// Draws `bmp` centered and aspect-fitted inside `rect` using linear interpolation.
///
/// # Safety
/// `rt` must be inside an active `BeginDraw`..`EndDraw` pass and `bmp` must belong to `rt`.
pub unsafe fn draw_bitmap_fitted(rt: &ID2D1RenderTarget, bmp: &ID2D1Bitmap, rect: Rect) {
    draw_bitmap_fitted_aligned(rt, bmp, rect, 0.5, 0.5);
}

/// Maps common Freedesktop / GTK symbolic icon names used in `.rasi` `icon-*` widgets
/// to Windows 11 `Segoe Fluent Icons` / `Segoe MDL2 Assets` glyphs.
pub fn freedesktop_symbolic_glyph(name: &str) -> Option<char> {
    let key = name.trim().to_ascii_lowercase();
    let stripped = key.strip_suffix("-symbolic").unwrap_or(&key);
    match stripped {
        "search" | "system-search" | "edit-find" | "gtk-find" | "find" => Some('\u{E721}'),
        "go-previous" | "back" | "arrow-left" | "gtk-go-back" => Some('\u{E72B}'),
        "go-next" | "forward" | "arrow-right" | "gtk-go-forward" => Some('\u{E72A}'),
        "go-up" | "arrow-up" | "gtk-go-up" => Some('\u{E74A}'),
        "go-down" | "arrow-down" | "gtk-go-down" => Some('\u{E74B}'),
        "gtk-paste" | "edit-paste" | "paste" => Some('\u{E77F}'),
        "gtk-copy" | "edit-copy" | "copy" => Some('\u{E8C8}'),
        "input-keyboard" | "keyboard" => Some('\u{E765}'),
        "window-close" | "gtk-close" | "edit-clear" | "close" => Some('\u{E711}'),
        "view-refresh" | "gtk-refresh" | "refresh" => Some('\u{E72C}'),
        "folder" | "folder-open" | "system-file-manager" => Some('\u{E8B7}'),
        "user-home" | "go-home" | "home" => Some('\u{E80F}'),
        "preferences-system" | "emblem-system" | "settings" | "configure" => Some('\u{E713}'),
        "utilities-terminal" | "terminal" => Some('\u{E756}'),
        "applications-system" | "applications-other" | "view-app-grid" => Some('\u{F0E2}'),
        _ => None,
    }
}

/// Draws a background image bitmap into `rect` honoring Rofi's `ImageScale` (`both`, `width`, `height`, `none`).
///
/// # Safety
/// `rt` must be inside an active `BeginDraw`..`EndDraw` pass and `bmp` must belong to `rt`.
pub unsafe fn draw_bitmap_scaled(
    rt: &ID2D1RenderTarget,
    bmp: &ID2D1Bitmap,
    rect: Rect,
    scale: ImageScale,
) {
    if rect.w <= 1.0 || rect.h <= 1.0 {
        return;
    }
    let sz = bmp.GetSize();
    if sz.width <= 0.0 || sz.height <= 0.0 {
        return;
    }
    let clip = D2D_RECT_F {
        left: rect.x,
        top: rect.y,
        right: rect.x + rect.w,
        bottom: rect.y + rect.h,
    };
    let dst = match scale {
        ImageScale::Both => clip,
        ImageScale::Width => {
            let s = rect.w / sz.width;
            let dh = sz.height * s;
            let y = rect.y + (rect.h - dh) * 0.5;
            D2D_RECT_F {
                left: rect.x,
                top: y,
                right: rect.x + rect.w,
                bottom: y + dh,
            }
        }
        ImageScale::Height => {
            let s = rect.h / sz.height;
            let dw = sz.width * s;
            let x = rect.x + (rect.w - dw) * 0.5;
            D2D_RECT_F {
                left: x,
                top: rect.y,
                right: x + dw,
                bottom: rect.y + rect.h,
            }
        }
        ImageScale::None => {
            let x = rect.x + (rect.w - sz.width) * 0.5;
            let y = rect.y + (rect.h - sz.height) * 0.5;
            D2D_RECT_F {
                left: x,
                top: y,
                right: x + sz.width,
                bottom: y + sz.height,
            }
        }
    };
    rt.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    rt.DrawBitmap(
        bmp,
        Some(&dst),
        1.0,
        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
        None,
    );
    rt.PopAxisAlignedClip();
}

fn bucket_icon_size(px: u32) -> u32 {
    match px {
        0..=24 => 24,
        25..=32 => 32,
        33..=48 => 48,
        49..=64 => 64,
        65..=96 => 96,
        _ => 128,
    }
}

fn is_image_file_ext(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "bmp" | "ico" | "webp" | "gif"
        )
    })
}

fn find_on_path(cmd: &str) -> Option<PathBuf> {
    let path_os = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_os) {
        let p = dir.join(cmd);
        if p.is_file() {
            return Some(p);
        }
        if Path::new(cmd).extension().is_none() {
            let exe = dir.join(format!("{cmd}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

unsafe fn query_window_exe_path(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return None;
    }
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_WIN32,
        PWSTR(buf.as_mut_ptr()),
        &mut len,
    )
    .is_ok();
    let _ = CloseHandle(handle);
    if ok && len > 0 {
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    } else {
        None
    }
}
