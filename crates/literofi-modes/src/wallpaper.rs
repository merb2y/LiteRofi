//! Native Windows Wallpaper Switcher mode (`-show wallpaper`, `Alt+W`):
//! - Discovers wallpapers from `%USERPROFILE%\Pictures\Wallpapers`, `%APPDATA%\LiteRofi\wallpapers`,
//!   `C:\Windows\Web\Wallpaper`, and top-level `%USERPROFILE%\Pictures`.
//! - Renders a 16:9 rounded-card gallery using the async WIC thumbnail cache (`IconCache`).
//! - Selecting a wallpaper opens a full-bleed preview panel with frosted-glass actions:
//!   - **Set Wallpaper**: applies immediately via `SystemParametersInfoW(SPI_SETDESKWALLPAPER)`.
//!   - **Set + Auto-Theme Colors**: applies the wallpaper AND extracts a dominant color palette
//!     via WIC (Pywal-style) into LiteRofi's active color palette (`wallust/colors-rofi.rasi` &
//!     `launchers/*/shared/colors.rasi`).
//!   - **Open Folder**: reveals the image file in Windows Explorer.
//!   - **Back**: returns to the wallpaper gallery.

use crate::active_palette::ActivePalette;
use crate::color_picker::{apply_palette_to_root, ColorPalette};
use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct WallpaperEntry {
    pub name: String,
    pub folder_label: String,
    pub path: PathBuf,
    pub is_current: bool,
}

/// Interactive Windows wallpaper gallery + preview & auto-accent mode.
pub struct WallpaperMode {
    display: String,
    config_root: PathBuf,
    wallpapers: Vec<WallpaperEntry>,
    selected_wp: Option<usize>,
    dry_run: bool,
    palette: Option<ActivePalette>,
}

impl WallpaperMode {
    pub fn new(cfg: &Config) -> Self {
        let config_root = cfg
            .config_file
            .as_ref()
            .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
            .unwrap_or_else(default_config_root);
        Self {
            display: "wallpaper".to_string(),
            config_root,
            wallpapers: Vec::new(),
            selected_wp: None,
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
        }
    }

    fn active_preview(&self) -> Option<&WallpaperEntry> {
        self.selected_wp.and_then(|idx| self.wallpapers.get(idx))
    }

    fn preview_actions() -> [(&'static str, &'static str); 4] {
        [
            ("󰸉  Set Wallpaper", "set"),
            ("󰏘  Set + Auto-Theme", "set_theme"),
            ("󰉋  Open Folder", "folder"),
            ("󰁍  Back", "back"),
        ]
    }
}

impl RichMode for WallpaperMode {
    fn name(&self) -> &str {
        "wallpaper"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        self.selected_wp = None;
        self.wallpapers = discover_wallpapers(&self.config_root);
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        if self.active_preview().is_some() {
            Some("Wallpaper".to_string())
        } else {
            Some("󰸉  Wallpapers".to_string())
        }
    }

    fn message(&self) -> Option<String> {
        if let Some(wp) = self.active_preview() {
            let tag = if wp.is_current { "  ·  Active" } else { "" };
            return Some(format!("{}  ·  {}{}", wp.name, wp.folder_label, tag));
        }
        if self.wallpapers.is_empty() {
            return Some(
                "No wallpapers found. Add images to Pictures\\Wallpapers or AppData\\Roaming\\LiteRofi\\wallpapers."
                    .to_string(),
            );
        }
        Some(format!(
            "{} wallpapers  ·  Enter: Preview & Apply  ·  Esc: Close",
            self.wallpapers.len()
        ))
    }

    fn reload_hints(&self) -> ReloadHints {
        ReloadHints {
            keep_selection: false,
            new_selection: Some(0),
            keep_filter: false,
        }
    }

    fn theme_snippets(&self) -> Vec<String> {
        let Some(wp) = self.active_preview() else {
            return self
                .palette
                .as_ref()
                .map(|p| vec![p.to_wallpaper_rasi()])
                .unwrap_or_default();
        };

        let esc = wp.path.to_string_lossy().replace('\\', "/");
        let accent = self
            .palette
            .as_ref()
            .map(ActivePalette::accent_hex)
            .unwrap_or_else(|| "#89b4fa".to_string());
        let accent_fg = self
            .palette
            .as_ref()
            .map(ActivePalette::accent_fg_hex)
            .unwrap_or_else(|| "#1e1e2e".to_string());

        vec![format!(
            "configuration {{ show-icons: false; }}\n\
             window {{ width: 920px; height: 518px; location: center; anchor: center; border: 2px solid; border-radius: 16px; border-color: {accent}; background-image: url(\"{esc}\", width); }}\n\
             mainbox {{ children: [preview-spacer, preview-panel]; orientation: horizontal; padding: 0px; spacing: 0px; background-color: transparent; }}\n\
             preview-spacer {{ expand: true; background-color: transparent; }}\n\
             preview-panel {{ expand: false; width: 290px; orientation: vertical; background-color: rgba(12, 15, 22, 0.56); border: 0px 0px 0px 1px; border-color: rgba(255, 255, 255, 0.18); children: [message, listview]; }}\n\
             message {{ padding: 26px 20px 8px 20px; background-color: transparent; }}\n\
             textbox {{ font: \"Segoe UI Bold 11\"; text-color: #FFFFFF; horizontal-align: 0.5; }}\n\
             listview {{ layout: vertical; flow: vertical; columns: 1; lines: 4; spacing: 10px; padding: 18px 22px 24px 22px; fixed-height: true; scrollbar: false; background-color: transparent; }}\n\
             element {{ children: [element-text]; padding: 11px 14px; margin: 0px; orientation: horizontal; border: 1px solid; border-color: rgba(255, 255, 255, 0.22); background-color: rgba(255, 255, 255, 0.14); border-radius: 10px; text-color: #FFFFFF; }}\n\
             element selected.normal {{ background-color: {accent}; border-color: {accent}; text-color: {accent_fg}; border-radius: 10px; }}\n\
             element-text {{ horizontal-align: 0.5; vertical-align: 0.5; font: \"Segoe UI Semibold 11\"; text-color: inherit; }}\n\
             element-text selected.normal {{ horizontal-align: 0.5; text-color: {accent_fg}; }}"
        )]
    }

    fn entries(&self) -> Vec<ListEntry> {
        if self.active_preview().is_some() {
            return Self::preview_actions()
                .into_iter()
                .map(|(label, _)| ListEntry {
                    text: label.to_string(),
                    ..Default::default()
                })
                .collect();
        }
        self.wallpapers
            .iter()
            .map(|w| {
                let title = if w.is_current {
                    format!("★ {}", w.name)
                } else {
                    w.name.clone()
                };
                ListEntry {
                    text: title,
                    icon: Some(w.path.to_string_lossy().into_owned()),
                    meta: Some(w.folder_label.clone()),
                    ..Default::default()
                }
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Cancelled => {
                if self.selected_wp.is_some() {
                    self.selected_wp = None;
                    Ok(ModeStep::Reload)
                } else {
                    Ok(ModeStep::Exit(1))
                }
            }
            UiOutcome::Accepted { index, .. } => {
                if let Some(wp) = self.active_preview().cloned() {
                    let actions = Self::preview_actions();
                    let action = actions.get(*index).map(|(_, id)| *id).unwrap_or("back");
                    match action {
                        "set" => {
                            if !self.dry_run {
                                set_windows_wallpaper(&wp.path)?;
                            }
                            self.selected_wp = None;
                            return Ok(ModeStep::Exit(0));
                        }
                        "set_theme" => {
                            if !self.dry_run {
                                set_windows_wallpaper(&wp.path)?;
                                if let Some(pal) = extract_wallpaper_palette(&wp.path, &wp.name) {
                                    apply_palette_to_root(&self.config_root, &pal)?;
                                }
                            }
                            self.selected_wp = None;
                            return Ok(ModeStep::Exit(0));
                        }
                        "folder" => {
                            if !self.dry_run {
                                reveal_in_explorer(&wp.path);
                            }
                            self.selected_wp = None;
                            return Ok(ModeStep::Exit(0));
                        }
                        _ => {
                            self.selected_wp = None;
                            return Ok(ModeStep::Reload);
                        }
                    }
                }

                if *index < self.wallpapers.len() {
                    self.selected_wp = Some(*index);
                    return Ok(ModeStep::Reload);
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::ModeSwitch(idx) => {
                self.selected_wp = None;
                Ok(ModeStep::SwitchModeIndex(*idx))
            }
            _ => Ok(ModeStep::Reload),
        }
    }
}

fn default_config_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return PathBuf::from(appdata).join("LiteRofi");
        }
    }
    PathBuf::from(".")
}

fn is_wallpaper_ext(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "bmp"
        )
    })
}

fn clean_wallpaper_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Wallpaper");
    let words: Vec<String> = stem
        .replace(['_', '-'], " ")
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => {
                    let mut s = first.to_uppercase().to_string();
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            }
        })
        .collect();
    let joined = words.join(" ");
    if joined.is_empty() {
        "Wallpaper".to_string()
    } else {
        joined
    }
}

fn scan_dir_recursive(
    root: &Path,
    folder_label: &str,
    max_depth: usize,
    out: &mut Vec<WallpaperEntry>,
    seen: &mut HashSet<String>,
    current_norm: Option<&str>,
) {
    if !root.is_dir() {
        return;
    }
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        children.sort();
        for path in children {
            if path.is_dir() {
                if depth < max_depth {
                    let dir_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if !matches!(
                        dir_name.as_str(),
                        "screenshots" | "camera roll" | "saved pictures" | ".git" | "thumbnails"
                    ) {
                        stack.push((path, depth + 1));
                    }
                }
                continue;
            }
            if !is_wallpaper_ext(&path) {
                continue;
            }
            // Skip tiny icons/system preview thumbnails (< 64 KB, e.g. Windows Spotlight 280x175 img50.jpg)
            if path.metadata().map(|m| m.len()).unwrap_or(0) < 64 * 1024 {
                continue;
            }
            let key = path.to_string_lossy().to_ascii_lowercase();
            if !seen.insert(key.clone()) {
                continue;
            }
            let is_current = current_norm.is_some_and(|c| c == key);
            let sub_label = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .filter(|n| !n.eq_ignore_ascii_case("wallpaper"))
                .map(str::to_string)
                .unwrap_or_else(|| folder_label.to_string());
            out.push(WallpaperEntry {
                name: clean_wallpaper_name(&path),
                folder_label: sub_label,
                path,
                is_current,
            });
            if out.len() >= 180 {
                return;
            }
        }
    }
}

/// Discovers wallpapers from user folders and Windows built-in wallpapers.
pub fn discover_wallpapers(config_root: &Path) -> Vec<WallpaperEntry> {
    let current_path = current_windows_wallpaper();
    let current_norm = current_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_ascii_lowercase());

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    // 1. User's dedicated Wallpapers folders
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let user_wp = PathBuf::from(&profile).join("Pictures").join("Wallpapers");
        scan_dir_recursive(
            &user_wp,
            "Wallpapers",
            2,
            &mut out,
            &mut seen,
            current_norm.as_deref(),
        );
    }
    let appdata_wp = config_root.join("wallpapers");
    scan_dir_recursive(
        &appdata_wp,
        "LiteRofi",
        2,
        &mut out,
        &mut seen,
        current_norm.as_deref(),
    );

    let had_user_wallpapers = !out.is_empty();

    // 2. Windows built-in wallpapers (always rich out-of-the-box on Windows 10/11)
    let win_wp = PathBuf::from(r"C:\Windows\Web\Wallpaper");
    scan_dir_recursive(
        &win_wp,
        "Windows",
        3,
        &mut out,
        &mut seen,
        current_norm.as_deref(),
    );

    // 3. Top-level Pictures folder (depth 0 only) if user has no dedicated Wallpapers dir yet
    if !had_user_wallpapers {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let pics = PathBuf::from(profile).join("Pictures");
            scan_dir_recursive(
                &pics,
                "Pictures",
                0,
                &mut out,
                &mut seen,
                current_norm.as_deref(),
            );
        }
    }

    // Ensure the active Windows wallpaper appears even if it lives elsewhere
    if let Some(cur) = current_path.filter(|p| p.is_file() && is_wallpaper_ext(p)) {
        let key = cur.to_string_lossy().to_ascii_lowercase();
        if seen.insert(key) {
            out.insert(
                0,
                WallpaperEntry {
                    name: clean_wallpaper_name(&cur),
                    folder_label: "Current".to_string(),
                    path: cur,
                    is_current: true,
                },
            );
        }
    }

    // Float active wallpaper to index 0
    if let Some(pos) = out.iter().position(|w| w.is_current) {
        if pos > 0 {
            let active = out.remove(pos);
            out.insert(0, active);
        }
    }

    out
}

#[cfg(windows)]
pub fn current_windows_wallpaper() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_SZ,
        REG_VALUE_TYPE,
    };

    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Control Panel\\Desktop"),
            0,
            KEY_READ,
            &mut hkey,
        )
        .is_err()
        {
            return None;
        }
        let mut typ = REG_VALUE_TYPE::default();
        let mut len = 0u32;
        let ok = RegQueryValueExW(
            hkey,
            w!("WallPaper"),
            None,
            Some(&mut typ),
            None,
            Some(&mut len),
        )
        .is_ok()
            && typ == REG_SZ
            && len >= 2;
        if !ok {
            let _ = RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u16; (len as usize).div_ceil(2)];
        let got = RegQueryValueExW(
            hkey,
            w!("WallPaper"),
            None,
            None,
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut len),
        )
        .is_ok();
        let _ = RegCloseKey(hkey);
        if !got {
            return None;
        }
        let nul = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..nul]);
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }
}

#[cfg(not(windows))]
pub fn current_windows_wallpaper() -> Option<PathBuf> {
    None
}

/// Target display detected when launching the wallpaper carousel.
#[derive(Debug, Clone, Default)]
pub struct MonitorTarget {
    pub index: u32,
    pub device_id: Option<String>,
}

/// Detects the monitor currently under the mouse cursor via `IDesktopWallpaper`.
#[cfg(windows)]
pub fn detect_cursor_monitor_target() -> MonitorTarget {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }

    let com_ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let result = (|| -> Option<MonitorTarget> {
        let dw: IDesktopWallpaper =
            unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL) }.ok()?;
        let count = unsafe { dw.GetMonitorDevicePathCount() }.ok()?;
        for i in 0..count {
            let Ok(pw) = (unsafe { dw.GetMonitorDevicePathAt(i) }) else {
                continue;
            };
            let id_str = unsafe { pw.to_string() }.ok();
            let rect_res = unsafe { dw.GetMonitorRECT(windows::core::PCWSTR(pw.0)) };
            unsafe {
                CoTaskMemFree(Some(pw.0 as *const std::ffi::c_void));
            }
            if let (Some(dev_id), Ok(rect)) = (id_str, rect_res) {
                if pt.x >= rect.left && pt.x < rect.right && pt.y >= rect.top && pt.y < rect.bottom
                {
                    return Some(MonitorTarget {
                        index: i,
                        device_id: Some(dev_id),
                    });
                }
            }
        }
        None
    })();
    if com_ok {
        unsafe {
            CoUninitialize();
        }
    }
    result.unwrap_or_default()
}

#[cfg(not(windows))]
pub fn detect_cursor_monitor_target() -> MonitorTarget {
    MonitorTarget::default()
}

/// Applies a static wallpaper to a specific monitor (`IDesktopWallpaper::SetWallpaper`),
/// falling back to `SystemParametersInfoW` if per-monitor COM targeting is unavailable.
#[cfg(windows)]
pub fn set_windows_wallpaper_on_monitor(
    path: &Path,
    monitor: &MonitorTarget,
) -> Result<(), String> {
    if let Some(ref dev_id) = monitor.device_id {
        use windows::core::HSTRING;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper};

        let com_ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let set_res = (|| -> Result<(), String> {
            let dw: IDesktopWallpaper =
                unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL) }
                    .map_err(|e| format!("CoCreateInstance(DesktopWallpaper) failed: {e}"))?;
            let dev_h = HSTRING::from(dev_id.as_str());
            let path_h = HSTRING::from(path.as_os_str());
            unsafe { dw.SetWallpaper(&dev_h, &path_h) }
                .map_err(|e| format!("IDesktopWallpaper::SetWallpaper failed: {e}"))
        })();
        if com_ok {
            unsafe {
                CoUninitialize();
            }
        }
        if set_res.is_ok() {
            return Ok(());
        }
    }
    set_windows_wallpaper(path)
}

#[cfg(not(windows))]
pub fn set_windows_wallpaper_on_monitor(
    path: &Path,
    _monitor: &MonitorTarget,
) -> Result<(), String> {
    set_windows_wallpaper(path)
}

#[cfg(windows)]
pub fn set_windows_wallpaper(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 file path for `SPI_SETDESKWALLPAPER`.
    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(wide.as_ptr() as *mut std::ffi::c_void),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
        .map_err(|e| format!("SystemParametersInfoW(SPI_SETDESKWALLPAPER) failed: {e}"))
    }
}

#[cfg(not(windows))]
pub fn set_windows_wallpaper(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn reveal_in_explorer(path: &Path) {
    let arg = format!("/select,{}", path.display());
    let _ = Command::new("explorer.exe").arg(arg).spawn();
}

/// Extracts a Pywal-style dark/accent color palette from a wallpaper image via WIC.
#[cfg(windows)]
pub fn extract_wallpaper_palette(path: &Path, name: &str) -> Option<ColorPalette> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICBitmapScaler, IWICBitmapSource,
        IWICFormatConverter, IWICImagingFactory, WICBitmapDitherTypeNone,
        WICBitmapInterpolationModeFant, WICBitmapPaletteTypeMedianCut,
        WICDecodeMetadataCacheOnLoad,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };

    let wpath = HSTRING::from(path.as_os_str());
    let samples: Vec<(u8, u8, u8)> = unsafe {
        let com_ok = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let res = (|| -> Option<Vec<(u8, u8, u8)>> {
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let decoder = wic
                .CreateDecoderFromFilename(&wpath, None, GENERIC_READ, WICDecodeMetadataCacheOnLoad)
                .ok()?;
            let frame = decoder.GetFrame(0).ok()?;
            let src: IWICBitmapSource = frame.into();
            let scaler: IWICBitmapScaler = wic.CreateBitmapScaler().ok()?;
            scaler
                .Initialize(&src, 64, 36, WICBitmapInterpolationModeFant)
                .ok()?;
            let scaled: IWICBitmapSource = scaler.into();
            let conv: IWICFormatConverter = wic.CreateFormatConverter().ok()?;
            conv.Initialize(
                &scaled,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .ok()?;
            let conv_src: IWICBitmapSource = conv.into();
            let mut pixels = vec![0u8; 64 * 36 * 4];
            conv_src
                .CopyPixels(std::ptr::null(), 64 * 4, &mut pixels)
                .ok()?;
            let mut rgb = Vec::with_capacity(64 * 36);
            for px in pixels.as_chunks::<4>().0 {
                rgb.push((px[2], px[1], px[0]));
            }
            Some(rgb)
        })();
        if com_ok {
            CoUninitialize();
        }
        res?
    };

    Some(extract_palette_from_rgb_samples(&samples, name))
}

#[cfg(not(windows))]
pub fn extract_wallpaper_palette(_path: &Path, _name: &str) -> Option<ColorPalette> {
    None
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = f32::from(r) / 255.0;
    let gf = f32::from(g) / 255.0;
    let bf = f32::from(b) / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let d = max - min;
    let h = if d < 1e-5 {
        0.0
    } else if (max - rf).abs() < 1e-5 {
        60.0 * (((gf - bf) / d) % 6.0)
    } else if (max - gf).abs() < 1e-5 {
        60.0 * (((bf - rf) / d) + 2.0)
    } else {
        60.0 * (((rf - gf) / d) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max < 1e-5 { 0.0 } else { d / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn hex_rgb(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Builds a harmonious dark UI palette from sampled wallpaper RGB pixels.
pub fn extract_palette_from_rgb_samples(samples: &[(u8, u8, u8)], name: &str) -> ColorPalette {
    // Bin hues into 24 buckets weighted by saturation * mid-brightness to find dominant accent
    let mut buckets = [(0.0f32, 0.0f32, 0.0f32, 0.0f32); 24]; // (weight, r_sum, g_sum, b_sum)
    let mut avg_r = 0.0f32;
    let mut avg_g = 0.0f32;
    let mut avg_b = 0.0f32;

    for &(r, g, b) in samples {
        avg_r += f32::from(r);
        avg_g += f32::from(g);
        avg_b += f32::from(b);
        let (h, s, v) = rgb_to_hsv(r, g, b);
        if s > 0.18 && (0.20..=0.95).contains(&v) {
            let idx = ((h / 15.0) as usize).min(23);
            let w = s * s * (1.0 - (v - 0.62).abs());
            buckets[idx].0 += w;
            buckets[idx].1 += f32::from(r) * w;
            buckets[idx].2 += f32::from(g) * w;
            buckets[idx].3 += f32::from(b) * w;
        }
    }

    let n = (samples.len().max(1)) as f32;
    avg_r /= n;
    avg_g /= n;
    avg_b /= n;

    let best = buckets
        .iter()
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .copied()
        .unwrap_or((0.0, 0.0, 0.0, 0.0));

    let (acc_r, acc_g, acc_b) = if best.0 > 0.01 {
        let raw_r = (best.1 / best.0).round().clamp(0.0, 255.0) as u8;
        let raw_g = (best.2 / best.0).round().clamp(0.0, 255.0) as u8;
        let raw_b = (best.3 / best.0).round().clamp(0.0, 255.0) as u8;
        let (h, s, v) = rgb_to_hsv(raw_r, raw_g, raw_b);
        hsv_to_rgb(h, s.clamp(0.45, 0.85), v.clamp(0.62, 0.90))
    } else {
        (137, 180, 250) // Catppuccin blue fallback for monochrome wallpapers
    };

    // Deep tinted background derived from the wallpaper's average hue
    let (bg_h, bg_s, _) = rgb_to_hsv(avg_r as u8, avg_g as u8, avg_b as u8);
    let (bg_r, bg_g, bg_b) = hsv_to_rgb(bg_h, (bg_s * 0.45).clamp(0.08, 0.30), 0.13);
    let (alt_r, alt_g, alt_b) = hsv_to_rgb(bg_h, (bg_s * 0.42).clamp(0.10, 0.32), 0.20);
    let (acc_h, _, _) = rgb_to_hsv(acc_r, acc_g, acc_b);
    let (act_r, act_g, act_b) = hsv_to_rgb((acc_h + 40.0) % 360.0, 0.58, 0.82);

    ColorPalette {
        name: format!("Wallpaper ({name})"),
        bg: hex_rgb(bg_r, bg_g, bg_b),
        bg_alt: hex_rgb(alt_r, alt_g, alt_b),
        fg: "#eceff4".to_string(),
        selected: hex_rgb(acc_r, acc_g, acc_b),
        active: hex_rgb(act_r, act_g, act_b),
        urgent: "#f38ba8".to_string(),
        border: hex_rgb(acc_r, acc_g, acc_b),
        bg_rgba: format!("rgba({bg_r},{bg_g},{bg_b},0.90)"),
    }
}

/// A curated 4K/HD wallpaper discovered from the Wallhaven API.
#[derive(Debug, Clone)]
pub struct WebWallpaperEntry {
    pub id: String,
    pub title: String,
    pub thumb_path: PathBuf,
    pub full_url: String,
}

fn cache_subdir(sub: &str, temp_fallback: &str) -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.trim().is_empty() {
            return PathBuf::from(local)
                .join("LiteRofi")
                .join("cache")
                .join(sub);
        }
    }
    std::env::temp_dir().join(temp_fallback)
}

fn wallhaven_cache_dir() -> PathBuf {
    cache_subdir("wallhaven", "literofi-wallhaven")
}

/// Runs a single `curl.exe -Z --parallel-immediate` process to fetch all missing `(url, dst)` thumbnails.
fn download_urls_parallel_curl(pairs: &[(String, PathBuf)], max_time_secs: &str) {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let missing: Vec<&(String, PathBuf)> = pairs
        .iter()
        .filter(|(_, dst)| !(dst.is_file() && dst.metadata().map(|m| m.len()).unwrap_or(0) > 1024))
        .collect();
    if missing.is_empty() {
        return;
    }

    let mut cmd = Command::new("curl.exe");
    cmd.args([
        "-Z",
        "--parallel-immediate",
        "--parallel-max",
        "6",
        "-sL",
        "--max-time",
        max_time_secs,
        "-A",
        "Mozilla/5.0",
    ]);
    for (url, dst) in missing {
        cmd.arg("-o");
        cmd.arg(dst);
        cmd.arg(url);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let _ = cmd.status();
}

/// Generates a fresh 6-character alphanumeric Wallhaven random seed from system entropy.
pub fn new_wallhaven_seed() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut x = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(std::process::id()).wrapping_mul(0x517c_c1b7_2722_0a95);
    let mut out = String::with_capacity(6);
    for _ in 0..6 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push(CHARS[(x as usize) % CHARS.len()] as char);
    }
    out
}

/// Filter parameters for Wallhaven API search queries (`Alt+W -> W`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallhavenFilter {
    pub query: String,
    pub categories: String,
    pub sorting: String,
    pub top_range: String,
    pub atleast: String,
    pub ratios: String,
    pub color: String,
}

impl Default for WallhavenFilter {
    fn default() -> Self {
        Self {
            query: String::new(),
            categories: "111".to_string(),
            sorting: "random".to_string(),
            top_range: "1M".to_string(),
            atleast: "1920x1080".to_string(),
            ratios: String::new(),
            color: String::new(),
        }
    }
}

impl From<&literofi_core::widgets_config::WallpaperWidgetConfig> for WallhavenFilter {
    fn from(cfg: &literofi_core::widgets_config::WallpaperWidgetConfig) -> Self {
        Self {
            query: cfg.wallhaven_query.clone(),
            categories: cfg.wallhaven_categories.clone(),
            sorting: cfg.wallhaven_sorting.clone(),
            top_range: cfg.wallhaven_top_range.clone(),
            atleast: cfg.wallhaven_atleast.clone(),
            ratios: cfg.wallhaven_ratios.clone(),
            color: cfg.wallhaven_color.clone(),
        }
    }
}

fn percent_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Builds the Wallhaven v1 search API URL from `filter`, `seed`, and `page`.
pub fn build_wallhaven_url(filter: &WallhavenFilter, seed: &str, page: u32) -> String {
    let sorting = match filter.sorting.as_str() {
        "random" | "toplist" | "favorites" | "views" | "date_added" | "hot" => {
            filter.sorting.as_str()
        }
        _ => "random",
    };
    let categories = if filter.categories.len() == 3
        && filter.categories.bytes().all(|b| b == b'0' || b == b'1')
        && filter.categories != "000"
    {
        filter.categories.as_str()
    } else {
        "111"
    };

    let mut url = format!(
        "https://wallhaven.cc/api/v1/search?categories={categories}&purity=100&sorting={sorting}&page={page}"
    );
    if sorting == "random" && !seed.is_empty() {
        url.push_str(&format!("&seed={seed}"));
    }
    if sorting == "toplist" && !filter.top_range.is_empty() {
        url.push_str(&format!("&topRange={}", filter.top_range));
    }
    let q = filter.query.trim();
    if !q.is_empty() {
        url.push_str(&format!("&q={}", percent_encode_query(q)));
    }
    let atleast = filter.atleast.trim();
    if !atleast.is_empty() {
        url.push_str(&format!("&atleast={atleast}"));
    }
    let ratios = filter.ratios.trim();
    if !ratios.is_empty() {
        url.push_str(&format!("&ratios={ratios}"));
    }
    let color = filter.color.trim().trim_start_matches('#');
    if color.len() == 6 && color.chars().all(|c| c.is_ascii_hexdigit()) {
        url.push_str(&format!("&colors={}", color.to_ascii_lowercase()));
    }
    url
}

/// Fetches a page of SFW wallpapers from Wallhaven using `filter` and caches thumbnails in parallel.
pub fn fetch_wallhaven_page(
    filter: &WallhavenFilter,
    seed: &str,
    page: u32,
) -> Vec<WebWallpaperEntry> {
    fetch_wallhaven_page_limit(filter, seed, page, 12)
}

/// Fetches up to `limit` SFW wallpapers from Wallhaven in one parallel `curl.exe -Z` batch.
pub fn fetch_wallhaven_page_limit(
    filter: &WallhavenFilter,
    seed: &str,
    page: u32,
    limit: usize,
) -> Vec<WebWallpaperEntry> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let url = build_wallhaven_url(filter, seed, page);
    let mut cmd = Command::new("curl.exe");
    cmd.args(["-sL", "--max-time", "6", "-A", "LiteRofi/0.2", &url]);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let Ok(out) = cmd.output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let body = String::from_utf8_lossy(&out.stdout);
    let parsed: Vec<_> = parse_wallhaven_json(&body)
        .into_iter()
        .take(limit.max(1))
        .collect();
    if parsed.is_empty() {
        return Vec::new();
    }

    let cache_dir = wallhaven_cache_dir();
    let _ = std::fs::create_dir_all(&cache_dir);

    let pairs: Vec<(String, PathBuf)> = parsed
        .iter()
        .map(|(id, _, _, thumb_url)| (thumb_url.clone(), cache_dir.join(format!("{id}_thumb.jpg"))))
        .collect();
    download_urls_parallel_curl(&pairs, "6");

    parsed
        .into_iter()
        .zip(pairs)
        .filter_map(|((id, resolution, full_url, _), (_, thumb_path))| {
            if thumb_path.is_file() && thumb_path.metadata().map(|m| m.len()).unwrap_or(0) > 4096 {
                Some(WebWallpaperEntry {
                    title: format!("Wallhaven #{id} ({resolution})"),
                    id,
                    thumb_path,
                    full_url,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Downloads the full-resolution Wallhaven wallpaper into `%USERPROFILE%\Pictures\Wallpapers`.
pub fn download_wallhaven_full(entry: &WebWallpaperEntry) -> Result<PathBuf, String> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let ext = entry
        .full_url
        .rsplit('.')
        .next()
        .filter(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp"
            )
        })
        .unwrap_or("jpg");
    let wp_dir = std::env::var("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Pictures").join("Wallpapers"))
        .unwrap_or_else(|_| wallhaven_cache_dir());
    std::fs::create_dir_all(&wp_dir)
        .map_err(|e| format!("failed to create {}: {e}", wp_dir.display()))?;
    let dst = wp_dir.join(format!("wallhaven_{}.{ext}", entry.id));
    if dst.is_file() && dst.metadata().map(|m| m.len()).unwrap_or(0) > 32 * 1024 {
        return Ok(dst);
    }

    let mut cmd = Command::new("curl.exe");
    cmd.args([
        "-sL",
        "--max-time",
        "15",
        "-A",
        "LiteRofi/0.2",
        "-o",
        &dst.to_string_lossy(),
        &entry.full_url,
    ]);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let status = cmd
        .status()
        .map_err(|e| format!("curl.exe failed for Wallhaven download: {e}"))?;
    if !status.success() || !dst.is_file() {
        return Err(format!("failed to download {}", entry.full_url));
    }
    Ok(dst)
}

fn parse_wallhaven_json(body: &str) -> Vec<(String, String, String, String)> {
    // Wallhaven JSON objects each contain `"id":"...",..."resolution":"...",..."path":"...",..."large":"..."`
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(pos) = rest.find("\"id\":\"") {
        let after_id = &rest[pos + 6..];
        let Some(id_end) = after_id.find('"') else {
            break;
        };
        let id = after_id[..id_end].to_string();
        let chunk_end = after_id[id_end..]
            .find("\"id\":\"")
            .map(|p| id_end + p)
            .unwrap_or(after_id.len());
        let chunk = &after_id[id_end..chunk_end];

        let resolution =
            extract_json_str(chunk, "\"resolution\":\"").unwrap_or_else(|| "HD".into());
        let path = extract_json_str(chunk, "\"path\":\"").map(|s| s.replace("\\/", "/"));
        let thumb = extract_json_str(chunk, "\"large\":\"").map(|s| s.replace("\\/", "/"));

        if let (Some(full_url), Some(thumb_url)) = (path, thumb) {
            if !id.is_empty() && full_url.starts_with("http") && thumb_url.starts_with("http") {
                out.push((id, resolution, full_url, thumb_url));
            }
        }
        rest = &after_id[chunk_end..];
    }
    out
}

fn extract_json_str(haystack: &str, key: &str) -> Option<String> {
    let idx = haystack.find(key)?;
    let val = &haystack[idx + key.len()..];
    let end = val.find('"')?;
    Some(val[..end].to_string())
}

// ---------------------------------------------------------------------------
// Wallpaper Engine Integration (Steam Workshop + myprojects + defaultprojects)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WallpaperEngineItem {
    pub title: String,
    pub project_json: PathBuf,
    pub preview_path: PathBuf,
    pub kind: String,
}

fn push_unique_dir(roots: &mut Vec<PathBuf>, seen: &mut HashSet<String>, p: PathBuf) {
    if p.is_dir() {
        let key = p.to_string_lossy().to_ascii_lowercase();
        if seen.insert(key) {
            roots.push(p);
        }
    }
}

fn discover_steam_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Common drive/path candidates across Windows machines
    for drive in ['C', 'D', 'E', 'F', 'G'] {
        for rel in [
            "Program Files (x86)\\Steam",
            "Program Files\\Steam",
            "Steam",
            "SteamLibrary",
            "Games\\Steam",
            "Games\\SteamLibrary",
        ] {
            push_unique_dir(
                &mut roots,
                &mut seen,
                PathBuf::from(format!("{drive}:\\{rel}")),
            );
        }
    }

    // Parse Steam's libraryfolders.vdf from any discovered Steam root to catch custom libraries
    let mut extra: Vec<PathBuf> = Vec::new();
    for root in &roots {
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            for line in text.lines() {
                if let Some(p) = extract_vdf_path_value(line) {
                    extra.push(PathBuf::from(p));
                }
            }
        }
    }
    for p in extra {
        push_unique_dir(&mut roots, &mut seen, p);
    }

    roots
}

fn extract_vdf_path_value(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("\"path\"") {
        return None;
    }
    let after_key = trimmed.strip_prefix("\"path\"")?.trim();
    let val = after_key.strip_prefix('"')?;
    let end = val.find('"')?;
    Some(val[..end].replace("\\\\", "\\"))
}

/// Locates `wallpaper64.exe` (preferred) or `wallpaper32.exe` in installed Steam libraries.
pub fn find_wallpaper_engine_exe() -> Option<PathBuf> {
    for root in discover_steam_roots() {
        let dir = root
            .join("steamapps")
            .join("common")
            .join("wallpaper_engine");
        let exe64 = dir.join("wallpaper64.exe");
        if exe64.is_file() {
            return Some(exe64);
        }
        let exe32 = dir.join("wallpaper32.exe");
        if exe32.is_file() {
            return Some(exe32);
        }
    }
    None
}

/// Discovers installed Wallpaper Engine items (Workshop 431960 + myprojects + defaultprojects).
pub fn discover_wallpaper_engine_items() -> Vec<WallpaperEngineItem> {
    let mut scan_dirs: Vec<PathBuf> = Vec::new();
    for root in discover_steam_roots() {
        let ws = root
            .join("steamapps")
            .join("workshop")
            .join("content")
            .join("431960");
        if ws.is_dir() {
            scan_dirs.push(ws);
        }
        let we_dir = root
            .join("steamapps")
            .join("common")
            .join("wallpaper_engine");
        for sub in ["projects\\myprojects", "projects\\defaultprojects"] {
            let p = we_dir.join(sub);
            if p.is_dir() {
                scan_dirs.push(p);
            }
        }
    }

    let mut out: Vec<WallpaperEngineItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for parent in scan_dirs {
        let Ok(entries) = std::fs::read_dir(&parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let project_json = dir.join("project.json");
            if !project_json.is_file() {
                continue;
            }
            let key = project_json.to_string_lossy().to_ascii_lowercase();
            if !seen.insert(key) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&project_json) else {
                continue;
            };
            let Some((title, preview_rel, kind)) = parse_we_project_json(&text) else {
                continue;
            };
            let preview_path = resolve_we_preview(&dir, preview_rel.as_deref());
            let Some(preview_path) = preview_path else {
                continue;
            };
            let clean_title = if title.trim().is_empty() {
                dir.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Wallpaper")
                    .to_string()
            } else {
                title
            };
            out.push(WallpaperEngineItem {
                title: clean_title,
                project_json,
                preview_path,
                kind,
            });
        }
    }

    out.sort_by(|a, b| {
        a.title
            .to_ascii_lowercase()
            .cmp(&b.title.to_ascii_lowercase())
    });
    out
}

fn resolve_we_preview(dir: &Path, preview_rel: Option<&str>) -> Option<PathBuf> {
    if let Some(rel) = preview_rel {
        let p = dir.join(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    for cand in ["preview.jpg", "preview.png", "preview.gif", "preview.webp"] {
        let p = dir.join(cand);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn extract_json_field_loose(body: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let mut search = body;
    while let Some(pos) = search.find(&needle) {
        let after = search[pos + needle.len()..].trim_start();
        if let Some(after_colon) = after.strip_prefix(':') {
            let val_part = after_colon.trim_start();
            if let Some(str_start) = val_part.strip_prefix('"') {
                let mut out = String::new();
                let mut chars = str_start.chars();
                while let Some(ch) = chars.next() {
                    match ch {
                        '"' => return Some(out),
                        '\\' => {
                            if let Some(esc) = chars.next() {
                                match esc {
                                    '"' => out.push('"'),
                                    '\\' => out.push('\\'),
                                    '/' => out.push('/'),
                                    'n' => out.push(' '),
                                    't' => out.push(' '),
                                    other => out.push(other),
                                }
                            }
                        }
                        other => out.push(other),
                    }
                }
            }
        }
        search = &search[pos + needle.len()..];
    }
    None
}

fn parse_we_project_json(body: &str) -> Option<(String, Option<String>, String)> {
    let title = extract_json_field_loose(body, "title").unwrap_or_default();
    let preview = extract_json_field_loose(body, "preview");
    let kind = extract_json_field_loose(body, "type").unwrap_or_else(|| "scene".to_string());
    if title.is_empty() && preview.is_none() {
        return None;
    }
    Some((title, preview, kind))
}

fn is_wallpaper_engine_running() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        for proc_name in ["wallpaper64.exe", "wallpaper32.exe"] {
            let mut check = Command::new("tasklist.exe");
            check.args([
                "/FI",
                &format!("IMAGENAME eq {proc_name}"),
                "/NH",
                "/FO",
                "CSV",
            ]);
            check.creation_flags(0x0800_0000);
            let running = check
                .output()
                .ok()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .to_ascii_lowercase()
                        .contains(proc_name)
                })
                .unwrap_or(false);
            if running {
                return true;
            }
        }
    }
    false
}

/// Applies a Wallpaper Engine item via `wallpaper64.exe -control openWallpaper -file "<project.json>"`.
pub fn apply_wallpaper_engine_item(project_json: &Path) -> Result<(), String> {
    apply_wallpaper_engine_item_on_monitor(project_json, 0, false)
}

/// Applies a Wallpaper Engine item on `monitor_index` and optionally mutes Wallpaper Engine audio.
pub fn apply_wallpaper_engine_item_on_monitor(
    project_json: &Path,
    monitor_index: u32,
    mute_audio: bool,
) -> Result<(), String> {
    let exe = find_wallpaper_engine_exe()
        .ok_or_else(|| "Wallpaper Engine executable (wallpaper64.exe) not found".to_string())?;
    let mon_str = monitor_index.to_string();
    let path_str = project_json.to_string_lossy();
    let mut cmd = Command::new(&exe);
    cmd.args([
        "-control",
        "openWallpaper",
        "-file",
        &path_str,
        "-monitor",
        &mon_str,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let _ = cmd
        .status()
        .map_err(|e| format!("failed to launch {}: {e}", exe.display()))?;

    if mute_audio {
        let mut mute_cmd = Command::new(&exe);
        mute_cmd.args(["-control", "mute"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            mute_cmd.creation_flags(0x0800_0000);
        }
        let _ = mute_cmd.spawn();
    }
    Ok(())
}

/// Opens the Wallpaper Engine main browser/settings UI window.
pub fn open_wallpaper_engine_ui() -> Result<(), String> {
    let exe = find_wallpaper_engine_exe()
        .ok_or_else(|| "Wallpaper Engine executable (wallpaper64.exe) not found".to_string())?;
    let mut cmd = Command::new(&exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn()
        .map_err(|e| format!("failed to launch {}: {e}", exe.display()))?;
    Ok(())
}

/// If Wallpaper Engine is currently running, asks it to close its active overlay wallpaper
/// on `monitor_index` so a static Windows wallpaper on that display becomes immediately visible.
pub fn stop_wallpaper_engine_on_monitor_if_running(monitor_index: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if !is_wallpaper_engine_running() {
            return;
        }
        if let Some(exe) = find_wallpaper_engine_exe() {
            let mon_str = monitor_index.to_string();
            let mut cmd = Command::new(exe);
            cmd.args(["-control", "closeWallpaper", "-monitor", &mon_str]);
            cmd.creation_flags(0x0800_0000);
            let _ = cmd.spawn();
        }
    }
    #[cfg(not(windows))]
    let _ = monitor_index;
}

/// If Wallpaper Engine is currently running, asks it to close its active overlay wallpaper.
pub fn stop_wallpaper_engine_if_running() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if !is_wallpaper_engine_running() {
            return;
        }
        if let Some(exe) = find_wallpaper_engine_exe() {
            let mut cmd = Command::new(exe);
            cmd.args(["-control", "closeWallpaper"]);
            cmd.creation_flags(0x0800_0000);
            let _ = cmd.spawn();
        }
    }
}

/// Steam Workshop Wallpaper Engine discovery item (`appid=431960`).
#[derive(Debug, Clone)]
pub struct WorkshopWallpaperEntry {
    pub id: String,
    pub title: String,
    pub preview_url: String,
    pub hd_preview_url: Option<String>,
    pub thumb_path: PathBuf,
}

fn we_thumbs_cache_dir() -> PathBuf {
    cache_subdir("we_thumbs", "literofi-we-thumbs")
}

/// Collects numeric Steam Workshop IDs already subscribed/downloaded under `workshop\content\431960`.
pub fn subscribed_wallpaper_engine_ids() -> HashSet<String> {
    let mut ids = HashSet::new();
    for root in discover_steam_roots() {
        let ws = root
            .join("steamapps")
            .join("workshop")
            .join("content")
            .join("431960");
        let Ok(entries) = std::fs::read_dir(&ws) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
                        ids.insert(name.to_string());
                    }
                }
            }
        }
    }
    ids
}

/// Picks a pseudo-random starting page (`1..=10`) for Wallpaper Engine Steam Workshop discovery.
pub fn new_workshop_start_page() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    ((nanos ^ (nanos >> 17)) % 10) as u32 + 1
}

/// Search/tag filters for Wallpaper Engine Steam Workshop discovery (`Alt+W → E`).
#[derive(Debug, Clone)]
pub struct WorkshopFilter {
    pub query: String,
    pub genre: String,
    pub kind: String,
    pub sort: String,
    pub exclude_anime: bool,
}

impl Default for WorkshopFilter {
    fn default() -> Self {
        Self {
            query: String::new(),
            genre: String::new(),
            kind: String::new(),
            sort: "toprated".to_string(),
            exclude_anime: true,
        }
    }
}

impl From<&literofi_core::widgets_config::WallpaperWidgetConfig> for WorkshopFilter {
    fn from(cfg: &literofi_core::widgets_config::WallpaperWidgetConfig) -> Self {
        Self {
            query: cfg.we_query.clone(),
            genre: cfg.we_genre.clone(),
            kind: cfg.we_type.clone(),
            sort: cfg.we_sort.clone(),
            exclude_anime: cfg.we_exclude_anime,
        }
    }
}

/// Builds the Steam Workshop browse URL for Wallpaper Engine (`appid=431960`) with active filters.
pub fn build_workshop_url(filter: &WorkshopFilter, page: u32) -> String {
    let sort_param = match filter.sort.as_str() {
        "toprated" | "mostrecent" | "trend" => filter.sort.as_str(),
        _ => "toprated",
    };
    let mut url = format!(
        "https://steamcommunity.com/workshop/browse/?appid=431960&browsesort={sort_param}&section=readytouseitems&days=90&p={page}&requiredtags%5B%5D=Everyone&requiredtags%5B%5D=Approved&excludedtags%5B%5D=Mature&excludedtags%5B%5D=Questionable"
    );
    if filter.exclude_anime && !filter.genre.trim().eq_ignore_ascii_case("Anime") {
        url.push_str("&excludedtags%5B%5D=Anime");
    }
    let genre = filter.genre.trim();
    if !genre.is_empty() {
        url.push_str(&format!(
            "&requiredtags%5B%5D={}",
            percent_encode_query(genre)
        ));
    }
    let kind = filter.kind.trim();
    if !kind.is_empty() {
        url.push_str(&format!(
            "&requiredtags%5B%5D={}",
            percent_encode_query(kind)
        ));
    }
    let query = filter.query.trim();
    if !query.is_empty() {
        url.push_str(&format!("&searchtext={}", percent_encode_query(query)));
    }
    url
}

/// Parses `(publishedfileid, title, preview_url)` tuples from Steam Workshop SSR HTML/JSON.
pub fn parse_steam_workshop_ssr(body: &str) -> Vec<(String, String, String)> {
    let escaped = body.contains("\\\"publishedfileid\\\":\\\"");
    let marker = if escaped {
        "\\\"publishedfileid\\\":\\\""
    } else {
        "\"publishedfileid\":\""
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut search_from = 0usize;

    while let Some(rel) = body[search_from..].find(marker) {
        let id_start = search_from + rel + marker.len();
        let after_id = &body[id_start..];
        let id_end_delim = if escaped { "\\\"" } else { "\"" };
        let Some(id_end) = after_id.find(id_end_delim) else {
            break;
        };
        let id = after_id[..id_end].to_string();
        search_from = id_start + id_end + id_end_delim.len();

        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }

        let chunk_end = body[search_from..]
            .find(marker)
            .map(|p| (search_from + p).min(search_from + 4096))
            .unwrap_or((search_from + 4096).min(body.len()));
        let chunk = &body[search_from..chunk_end];

        let Some(preview_url) = extract_ssr_string_field(chunk, "preview_url", escaped) else {
            continue;
        };
        if !preview_url.starts_with("http") {
            continue;
        }
        let title = extract_ssr_string_field(chunk, "title", escaped)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("Workshop #{id}"));

        if seen.insert(id.clone()) {
            out.push((id, title, preview_url));
        }
    }

    out
}

fn extract_ssr_string_field(chunk: &str, field: &str, escaped: bool) -> Option<String> {
    let needle = if escaped {
        format!("\\\"{field}\\\":\\\"")
    } else {
        format!("\"{field}\":\"")
    };
    let idx = chunk.find(&needle)?;
    let rest = &chunk[idx + needle.len()..];
    let mut out = String::new();
    let bytes = rest.as_bytes();
    let mut i = 0usize;

    if escaped {
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                if i + 2 < bytes.len()
                    && bytes[i + 1] == b'\\'
                    && bytes[i + 2] == b'\\'
                    && i + 3 < bytes.len()
                    && bytes[i + 3] == b'"'
                {
                    out.push('"');
                    i += 4;
                    continue;
                }
                if i + 2 < bytes.len() && bytes[i + 1] == b'\\' && bytes[i + 2] == b'/' {
                    out.push('/');
                    i += 3;
                    continue;
                }
                if i + 2 < bytes.len() && bytes[i + 1] == b'\\' && bytes[i + 2] == b'\\' {
                    out.push('\\');
                    i += 3;
                    continue;
                }
                if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                    return Some(out);
                }
                if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    out.push('/');
                    i += 2;
                    continue;
                }
            }
            let ch = rest[i..].chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
        }
    } else {
        while i < bytes.len() {
            if bytes[i] == b'"' {
                return Some(out);
            }
            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                match bytes[i + 1] {
                    b'"' => {
                        out.push('"');
                        i += 2;
                        continue;
                    }
                    b'\\' => {
                        out.push('\\');
                        i += 2;
                        continue;
                    }
                    b'/' => {
                        out.push('/');
                        i += 2;
                        continue;
                    }
                    _ => {}
                }
            }
            let ch = rest[i..].chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    None
}

const STEAM_THUMB_RANGE_BYTES: usize = 196_608;

/// Truncates an animated GIF down to its first complete image frame + `0x3B` trailer
/// so partial HTTP Range downloads (`0..192KB`) render instantaneously via WIC as valid static GIFs.
fn truncate_gif_to_first_frame(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 14 || (!data.starts_with(b"GIF87a") && !data.starts_with(b"GIF89a")) {
        return None;
    }
    let packed = data[10];
    let mut pos = 13usize;
    if (packed & 0x80) != 0 {
        pos += 3 * (1usize << ((packed & 7) + 1));
    }
    while pos < data.len() {
        match data[pos] {
            0x21 => {
                pos += 2;
                while pos < data.len() && data[pos] != 0 {
                    pos += 1 + data[pos] as usize;
                }
                pos += 1;
            }
            0x2C => {
                if pos + 10 > data.len() {
                    return None;
                }
                let ipacked = data[pos + 9];
                pos += 10;
                if (ipacked & 0x80) != 0 {
                    pos += 3 * (1usize << ((ipacked & 7) + 1));
                }
                if pos >= data.len() {
                    return None;
                }
                pos += 1; // LZW minimum code size
                while pos < data.len() && data[pos] != 0 {
                    pos += 1 + data[pos] as usize;
                }
                pos += 1; // block terminator 0x00
                if pos <= data.len() {
                    let mut out = Vec::with_capacity(pos + 1);
                    out.extend_from_slice(&data[..pos]);
                    out.push(0x3B);
                    return Some(out);
                }
                return None;
            }
            _ => return None,
        }
    }
    None
}

/// Checks whether a downloaded non-GIF thumbnail byte buffer represents a complete image file
/// (either strictly smaller than `range_limit`, or carrying its format EOF marker).
fn is_complete_non_gif_image(data: &[u8], range_limit: usize) -> bool {
    if data.len() < 128 {
        return false;
    }
    let is_jpeg = data.starts_with(&[0xFF, 0xD8]);
    let is_png = data.starts_with(&[0x89, 0x50, 0x4E, 0x47]);
    let is_webp = data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP";
    if !is_jpeg && !is_png && !is_webp {
        return false;
    }
    if data.len() < range_limit {
        return true;
    }
    if is_jpeg {
        let tail = &data[data.len().saturating_sub(16)..];
        return tail.windows(2).any(|w| w == [0xFF, 0xD9]);
    }
    if is_png {
        let tail = &data[data.len().saturating_sub(32)..];
        return tail.windows(4).any(|w| w == b"IEND");
    }
    false
}

/// Maps `WorkshopFilter.sort` strings to Valve `EUGCQuery` integer values.
pub fn workshop_sort_to_eugc_query(sort: &str) -> i32 {
    match sort.trim() {
        "trend" => 3,                   // k_EUGCQuery_RankedByTrend
        "mostrecent" => 1,              // k_EUGCQuery_RankedByPublicationDate
        "totaluniquesubscribers" => 12, // k_EUGCQuery_RankedByTotalUniqueSubscriptions
        _ => 0,                         // k_EUGCQuery_RankedByVote ("toprated")
    }
}

type WorkshopMetaRow = (String, String, String, Option<String>);

#[cfg(windows)]
fn query_workshop_page_steam_api(
    filter: &WorkshopFilter,
    page: u32,
) -> Option<Vec<WorkshopMetaRow>> {
    use std::ffi::{c_char, c_void, CString};
    use std::time::Duration;
    use windows::core::{s, HSTRING};
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    #[repr(C, align(8))]
    struct AlignedBuf<const N: usize>([u8; N]);

    let dll_path = find_steam_api64_dll()?;
    std::env::set_var("SteamAppId", "431960");
    std::env::set_var("SteamGameId", "431960");

    let dll_h = HSTRING::from(dll_path.as_os_str());
    let hmod = unsafe { LoadLibraryW(&dll_h) }.ok()?;

    let res = unsafe {
        let init_safe = GetProcAddress(hmod, s!("SteamAPI_InitSafe"))
            .map(|p| std::mem::transmute::<_, extern "C" fn() -> bool>(p));
        let init_flat = GetProcAddress(hmod, s!("SteamAPI_InitFlat"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut u8) -> i32>(p));
        let shutdown = GetProcAddress(hmod, s!("SteamAPI_Shutdown"))
            .map(|p| std::mem::transmute::<_, extern "C" fn()>(p));
        let run_callbacks = GetProcAddress(hmod, s!("SteamAPI_RunCallbacks"))
            .map(|p| std::mem::transmute::<_, extern "C" fn()>(p));
        let get_ugc = GetProcAddress(hmod, s!("SteamAPI_SteamUGC_v020"))
            .map(|p| std::mem::transmute::<_, extern "C" fn() -> *mut c_void>(p));
        let get_utils = GetProcAddress(hmod, s!("SteamAPI_SteamUtils_v010"))
            .map(|p| std::mem::transmute::<_, extern "C" fn() -> *mut c_void>(p));

        let create_query = GetProcAddress(
            hmod,
            s!("SteamAPI_ISteamUGC_CreateQueryAllUGCRequestPage"),
        )
        .map(|p| {
            std::mem::transmute::<_, extern "C" fn(*mut c_void, i32, i32, u32, u32, u32) -> u64>(p)
        });
        let add_req_tag = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_AddRequiredTag")).map(|p| {
            std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, *const c_char) -> bool>(p)
        });
        let add_exc_tag = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_AddExcludedTag")).map(|p| {
            std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, *const c_char) -> bool>(p)
        });
        let set_search = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SetSearchText")).map(|p| {
            std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, *const c_char) -> bool>(p)
        });
        let set_trend_days = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SetRankedByTrendDays"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, u32) -> bool>(p));
        let set_allow_cache = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SetAllowCachedResponse"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, u32) -> bool>(p));
        let set_ret_desc = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SetReturnLongDescription"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, bool) -> bool>(p));
        let set_ret_meta = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SetReturnMetadata"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, bool) -> bool>(p));
        let set_ret_child = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SetReturnChildren"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, bool) -> bool>(p));
        let set_ret_add_prev =
            GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SetReturnAdditionalPreviews")).map(|p| {
                std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, bool) -> bool>(p)
            });
        let send_query = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SendQueryUGCRequest"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64) -> u64>(p));
        let get_ugc_res =
            GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_GetQueryUGCResult")).map(|p| {
                std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, u32, *mut u8) -> bool>(p)
            });
        let get_prev_url = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_GetQueryUGCPreviewURL"))
            .map(|p| {
                std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, u32, *mut u8, u32) -> bool>(
                    p,
                )
            });
        let get_num_add_prev = GetProcAddress(
            hmod,
            s!("SteamAPI_ISteamUGC_GetQueryUGCNumAdditionalPreviews"),
        )
        .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, u32) -> u32>(p));
        let get_add_prev =
            GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_GetQueryUGCAdditionalPreview")).map(|p| {
                std::mem::transmute::<
                    _,
                    extern "C" fn(
                        *mut c_void,
                        u64,
                        u32,
                        u32,
                        *mut u8,
                        u32,
                        *mut u8,
                        u32,
                        *mut i32,
                    ) -> bool,
                >(p)
            });
        let release_query = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_ReleaseQueryUGCRequest"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64) -> bool>(p));
        let is_done =
            GetProcAddress(hmod, s!("SteamAPI_ISteamUtils_IsAPICallCompleted")).map(|p| {
                std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, *mut bool) -> bool>(p)
            });
        let get_api_res =
            GetProcAddress(hmod, s!("SteamAPI_ISteamUtils_GetAPICallResult")).map(|p| {
                std::mem::transmute::<
                    _,
                    extern "C" fn(*mut c_void, u64, *mut u8, i32, i32, *mut bool) -> bool,
                >(p)
            });

        let init_ok = if let Some(f) = init_safe {
            f()
        } else if let Some(f) = init_flat {
            let mut err_buf = [0u8; 1024];
            f(err_buf.as_mut_ptr()) == 0
        } else {
            false
        };

        let mut parsed_items: Option<Vec<WorkshopMetaRow>> = None;
        if init_ok {
            if let (
                Some(get_ugc_fn),
                Some(get_utils_fn),
                Some(create_fn),
                Some(send_fn),
                Some(get_res_fn),
                Some(get_prev_fn),
                Some(rel_fn),
                Some(is_done_fn),
                Some(get_api_fn),
            ) = (
                get_ugc,
                get_utils,
                create_query,
                send_query,
                get_ugc_res,
                get_prev_url,
                release_query,
                is_done,
                get_api_res,
            ) {
                let ugc = get_ugc_fn();
                let utils = get_utils_fn();
                if !ugc.is_null() && !utils.is_null() {
                    let sort_code = workshop_sort_to_eugc_query(&filter.sort);
                    let qh = create_fn(ugc, sort_code, 0, 431960, 431960, page.max(1));
                    if qh != 0 && qh != u64::MAX {
                        if let Some(add_req) = add_req_tag {
                            for tag in ["Everyone", "Approved"] {
                                if let Ok(cs) = CString::new(tag) {
                                    let _ = add_req(ugc, qh, cs.as_ptr());
                                }
                            }
                            if !filter.genre.trim().is_empty() {
                                if let Ok(cs) = CString::new(filter.genre.trim()) {
                                    let _ = add_req(ugc, qh, cs.as_ptr());
                                }
                            }
                            if !filter.kind.trim().is_empty() {
                                if let Ok(cs) = CString::new(filter.kind.trim()) {
                                    let _ = add_req(ugc, qh, cs.as_ptr());
                                }
                            }
                        }
                        if let Some(add_exc) = add_exc_tag {
                            for tag in ["Mature", "Questionable"] {
                                if let Ok(cs) = CString::new(tag) {
                                    let _ = add_exc(ugc, qh, cs.as_ptr());
                                }
                            }
                            if filter.exclude_anime
                                && !filter.genre.trim().eq_ignore_ascii_case("anime")
                            {
                                if let Ok(cs) = CString::new("Anime") {
                                    let _ = add_exc(ugc, qh, cs.as_ptr());
                                }
                            }
                        }
                        if !filter.query.trim().is_empty() {
                            if let (Some(set_q), Ok(cs)) =
                                (set_search, CString::new(filter.query.trim()))
                            {
                                let _ = set_q(ugc, qh, cs.as_ptr());
                            }
                        }
                        if filter.sort.trim() == "trend" {
                            if let Some(set_td) = set_trend_days {
                                let _ = set_td(ugc, qh, 7);
                            }
                        }
                        if let Some(f) = set_ret_desc {
                            let _ = f(ugc, qh, false);
                        }
                        if let Some(f) = set_ret_meta {
                            let _ = f(ugc, qh, false);
                        }
                        if let Some(f) = set_ret_child {
                            let _ = f(ugc, qh, false);
                        }
                        if let Some(f) = set_ret_add_prev {
                            let _ = f(ugc, qh, true);
                        }
                        if let Some(f) = set_allow_cache {
                            let _ = f(ugc, qh, 180);
                        }

                        let call = send_fn(ugc, qh);
                        if call != 0 {
                            let mut failed = false;
                            let mut completed = false;
                            for _ in 0..400 {
                                if let Some(rc) = run_callbacks {
                                    rc();
                                }
                                if is_done_fn(utils, call, &mut failed) {
                                    completed = true;
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            if completed && !failed {
                                let mut cb_buf = AlignedBuf([0u8; 512]);
                                let mut cb_failed = false;
                                let mut got_cb = false;
                                for sz in [280i32, 288i32, 24i32] {
                                    if get_api_fn(
                                        utils,
                                        call,
                                        cb_buf.0.as_mut_ptr(),
                                        sz,
                                        3401,
                                        &mut cb_failed,
                                    ) {
                                        got_cb = true;
                                        break;
                                    }
                                }
                                if got_cb && !cb_failed {
                                    let e_result = i32::from_le_bytes([
                                        cb_buf.0[8],
                                        cb_buf.0[9],
                                        cb_buf.0[10],
                                        cb_buf.0[11],
                                    ]);
                                    let num_returned = u32::from_le_bytes([
                                        cb_buf.0[12],
                                        cb_buf.0[13],
                                        cb_buf.0[14],
                                        cb_buf.0[15],
                                    ]);
                                    if e_result == 1 && num_returned > 0 {
                                        let mut det_buf = AlignedBuf([0u8; 16384]);
                                        let mut prev_buf = [0u8; 1024];
                                        let mut items = Vec::with_capacity(num_returned as usize);
                                        for idx in 0..num_returned {
                                            det_buf.0.fill(0);
                                            prev_buf.fill(0);
                                            if !get_res_fn(ugc, qh, idx, det_buf.0.as_mut_ptr()) {
                                                continue;
                                            }
                                            let pub_id = u64::from_le_bytes([
                                                det_buf.0[0],
                                                det_buf.0[1],
                                                det_buf.0[2],
                                                det_buf.0[3],
                                                det_buf.0[4],
                                                det_buf.0[5],
                                                det_buf.0[6],
                                                det_buf.0[7],
                                            ]);
                                            if pub_id == 0 {
                                                continue;
                                            }
                                            let title_slice = &det_buf.0[24..24 + 129];
                                            let title_end = title_slice
                                                .iter()
                                                .position(|&b| b == 0)
                                                .unwrap_or(title_slice.len());
                                            let raw_title =
                                                String::from_utf8_lossy(&title_slice[..title_end])
                                                    .trim()
                                                    .to_string();
                                            let title = if raw_title.is_empty() {
                                                format!("Workshop #{pub_id}")
                                            } else {
                                                raw_title
                                            };
                                            if !get_prev_fn(
                                                ugc,
                                                qh,
                                                idx,
                                                prev_buf.as_mut_ptr(),
                                                prev_buf.len() as u32,
                                            ) {
                                                continue;
                                            }
                                            let prev_end = prev_buf
                                                .iter()
                                                .position(|&b| b == 0)
                                                .unwrap_or(prev_buf.len());
                                            let preview_url =
                                                String::from_utf8_lossy(&prev_buf[..prev_end])
                                                    .trim()
                                                    .to_string();
                                            if preview_url.starts_with("http") {
                                                let mut hd_img: Option<String> = None;
                                                let mut hd_yt: Option<String> = None;
                                                if let (Some(n_fn), Some(g_fn)) =
                                                    (get_num_add_prev, get_add_prev)
                                                {
                                                    let n_add = n_fn(ugc, qh, idx).min(16);
                                                    let mut ubuf = [0u8; 512];
                                                    let mut fbuf = [0u8; 256];
                                                    for j in 0..n_add {
                                                        ubuf.fill(0);
                                                        fbuf.fill(0);
                                                        let mut ptype = -1i32;
                                                        if g_fn(
                                                            ugc,
                                                            qh,
                                                            idx,
                                                            j,
                                                            ubuf.as_mut_ptr(),
                                                            ubuf.len() as u32,
                                                            fbuf.as_mut_ptr(),
                                                            fbuf.len() as u32,
                                                            &mut ptype,
                                                        ) {
                                                            let uend = ubuf
                                                                .iter()
                                                                .position(|&b| b == 0)
                                                                .unwrap_or(ubuf.len());
                                                            let val = String::from_utf8_lossy(
                                                                &ubuf[..uend],
                                                            )
                                                            .trim()
                                                            .to_string();
                                                            if ptype == 0
                                                                && val.starts_with("http")
                                                                && hd_img.is_none()
                                                            {
                                                                hd_img = Some(
                                                                    if val.contains('?') {
                                                                        val
                                                                    } else {
                                                                        format!(
                                                                        "{val}?imw=960&imh=540&ima=fit&impolicy=Letterbox"
                                                                    )
                                                                    },
                                                                );
                                                            } else if ptype == 1
                                                                && !val.is_empty()
                                                                && val.bytes().all(|b| {
                                                                    b.is_ascii_alphanumeric()
                                                                        || b == b'-'
                                                                        || b == b'_'
                                                                })
                                                                && hd_yt.is_none()
                                                            {
                                                                hd_yt = Some(format!(
                                                                    "https://i.ytimg.com/vi/{val}/hqdefault.jpg"
                                                                ));
                                                            }
                                                        }
                                                    }
                                                }
                                                items.push((
                                                    pub_id.to_string(),
                                                    title,
                                                    preview_url,
                                                    hd_img.or(hd_yt),
                                                ));
                                            }
                                        }
                                        if !items.is_empty() {
                                            parsed_items = Some(items);
                                        }
                                    }
                                }
                            }
                        }
                        let _ = rel_fn(ugc, qh);
                    }
                }
            }
            if let Some(shut_fn) = shutdown {
                shut_fn();
            }
        }
        let _ = FreeLibrary(hmod);
        parsed_items
    };

    res
}

/// Queries Wallpaper Engine Steam Workshop item metadata without blocking on thumbnail downloads.
/// Uses native `steam_api64.dll` (`ISteamUGC_v020`) first (~270ms / cached IPC) and falls back to SSR scraping.
pub fn query_workshop_page_metadata(
    filter: &WorkshopFilter,
    page: u32,
    exclude_ids: &HashSet<String>,
    limit: usize,
) -> Vec<WorkshopWallpaperEntry> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    #[cfg(windows)]
    let raw_items = query_workshop_page_steam_api(filter, page);
    #[cfg(not(windows))]
    let raw_items: Option<Vec<(String, String, String, Option<String>)>> = None;

    let candidates_all = if let Some(items) = raw_items {
        items
    } else {
        let url = build_workshop_url(filter, page);
        let mut cmd = Command::new("curl.exe");
        cmd.args(["-sL", "--max-time", "12", "-A", "Mozilla/5.0", &url]);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);

        let Ok(out) = cmd.output() else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        let body = String::from_utf8_lossy(&out.stdout);
        parse_steam_workshop_ssr(&body)
            .into_iter()
            .map(|(id, title, prev)| (id, title, prev, None))
            .collect()
    };

    let cache_dir = we_thumbs_cache_dir();
    candidates_all
        .into_iter()
        .filter(|(id, _, prev, _)| !exclude_ids.contains(id) && prev.trim().starts_with("http"))
        .take(limit.max(1))
        .map(
            |(id, title, preview_url, hd_preview_url)| WorkshopWallpaperEntry {
                thumb_path: cache_dir.join(format!("{id}.jpg")),
                id,
                title,
                preview_url,
                hd_preview_url,
            },
        )
        .collect()
}

/// Upgrades Workshop thumbnails whose existing preview is missing, a tiny GIF, or `< 45KB`
/// using crisp additional HD screenshots / YouTube thumbnails discovered via `ISteamUGC`.
pub fn download_workshop_hd_thumbs(items: &[WorkshopWallpaperEntry]) -> Vec<PathBuf> {
    use std::io::Read;
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let cache_dir = we_thumbs_cache_dir();
    let _ = std::fs::create_dir_all(&cache_dir);

    let mut seen = HashSet::new();
    let mut targets: Vec<(&str, PathBuf, PathBuf)> = Vec::new();
    for item in items {
        let Some(hd_url) = item.hd_preview_url.as_deref() else {
            continue;
        };
        if !seen.insert(item.thumb_path.clone()) {
            continue;
        }
        let mut needs_upgrade = !item.thumb_path.is_file();
        if !needs_upgrade {
            let meta_len = item.thumb_path.metadata().map(|m| m.len()).unwrap_or(0);
            if meta_len < 45 * 1024 {
                needs_upgrade = true;
            } else if let Ok(mut f) = std::fs::File::open(&item.thumb_path) {
                let mut hdr = [0u8; 3];
                if f.read_exact(&mut hdr).is_ok() && &hdr == b"GIF" {
                    needs_upgrade = true;
                }
            }
        }
        if !needs_upgrade {
            continue;
        }
        let hdpart = item.thumb_path.with_extension("hdpart");
        let _ = std::fs::remove_file(&hdpart);
        targets.push((hd_url, item.thumb_path.clone(), hdpart));
    }

    if targets.is_empty() {
        return Vec::new();
    }

    let mut cmd = Command::new("curl.exe");
    cmd.args([
        "-Z",
        "--parallel-immediate",
        "--parallel-max",
        "6",
        "-sL",
        "--max-time",
        "8",
        "-A",
        "Mozilla/5.0",
    ]);
    for (url, _, hdpart) in &targets {
        cmd.arg("-o");
        cmd.arg(hdpart);
        cmd.arg(url);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let _ = cmd.status();

    let mut upgraded = Vec::new();
    for (_, thumb_path, hdpart) in &targets {
        if let Ok(bytes) = std::fs::read(hdpart) {
            if bytes.len() > 4096
                && is_complete_non_gif_image(&bytes, usize::MAX)
                && std::fs::rename(hdpart, thumb_path).is_ok()
            {
                upgraded.push(thumb_path.clone());
                continue;
            }
        }
        let _ = std::fs::remove_file(hdpart);
    }
    upgraded
}

/// Downloads missing Steam Workshop thumbnails rapidly using raw `preview_url` + HTTP `Range: bytes=0-196607`
/// (bypassing Steam CDN's slow GIF re-encoder) and extracting valid single-frame GIF headers where applicable.
pub fn download_workshop_thumbs_fast(items: &[WorkshopWallpaperEntry]) -> Vec<PathBuf> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let cache_dir = we_thumbs_cache_dir();
    let _ = std::fs::create_dir_all(&cache_dir);

    let mut seen = HashSet::new();
    let mut missing: Vec<(&str, PathBuf, PathBuf)> = Vec::new();
    for item in items {
        let valid = item.thumb_path.is_file()
            && item.thumb_path.metadata().map(|m| m.len()).unwrap_or(0) > 256;
        if valid || !seen.insert(item.thumb_path.clone()) {
            continue;
        }
        let part = item.thumb_path.with_extension("part");
        let _ = std::fs::remove_file(&part);
        missing.push((item.preview_url.as_str(), item.thumb_path.clone(), part));
    }

    if missing.is_empty() {
        return Vec::new();
    }

    let mut cmd = Command::new("curl.exe");
    cmd.args([
        "-Z",
        "--parallel-immediate",
        "--parallel-max",
        "8",
        "-sL",
        "--max-time",
        "6",
        "-r",
        "0-196607",
        "-A",
        "Mozilla/5.0",
    ]);
    for (url, _, part) in &missing {
        cmd.arg("-o");
        cmd.arg(part);
        cmd.arg(url);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let _ = cmd.status();

    let mut ready = Vec::new();
    let mut needs_full: Vec<(&str, PathBuf, PathBuf)> = Vec::new();

    for (url, thumb_path, part) in &missing {
        if let Ok(bytes) = std::fs::read(part) {
            if bytes.starts_with(b"GIF") {
                if let Some(frame0) = truncate_gif_to_first_frame(&bytes) {
                    if frame0.len() > 256
                        && std::fs::write(part, &frame0).is_ok()
                        && std::fs::rename(part, thumb_path).is_ok()
                    {
                        ready.push(thumb_path.clone());
                        continue;
                    }
                }
                let _ = std::fs::remove_file(part);
                needs_full.push((*url, thumb_path.clone(), part.clone()));
            } else if is_complete_non_gif_image(&bytes, STEAM_THUMB_RANGE_BYTES) {
                if std::fs::rename(part, thumb_path).is_ok() {
                    ready.push(thumb_path.clone());
                } else {
                    let _ = std::fs::remove_file(part);
                }
            } else if bytes.len() >= STEAM_THUMB_RANGE_BYTES {
                let _ = std::fs::remove_file(part);
                needs_full.push((*url, thumb_path.clone(), part.clone()));
            } else {
                let _ = std::fs::remove_file(part);
            }
        } else {
            let _ = std::fs::remove_file(part);
        }
    }

    if !needs_full.is_empty() {
        let mut full_cmd = Command::new("curl.exe");
        full_cmd.args([
            "-Z",
            "--parallel-immediate",
            "--parallel-max",
            "6",
            "-sL",
            "--max-time",
            "8",
            "-A",
            "Mozilla/5.0",
        ]);
        for (url, _, part) in &needs_full {
            full_cmd.arg("-o");
            full_cmd.arg(part);
            full_cmd.arg(url);
        }
        #[cfg(windows)]
        full_cmd.creation_flags(0x0800_0000);
        let _ = full_cmd.status();

        for (_, thumb_path, part) in &needs_full {
            if let Ok(bytes) = std::fs::read(part) {
                if bytes.starts_with(b"GIF") {
                    if let Some(frame0) = truncate_gif_to_first_frame(&bytes) {
                        if frame0.len() > 256
                            && std::fs::write(part, &frame0).is_ok()
                            && std::fs::rename(part, thumb_path).is_ok()
                        {
                            ready.push(thumb_path.clone());
                            continue;
                        }
                    }
                }
                if bytes.len() > 1024 && std::fs::rename(part, thumb_path).is_ok() {
                    ready.push(thumb_path.clone());
                    continue;
                }
            }
            let _ = std::fs::remove_file(part);
        }
    }

    ready
}

/// Fetches a page of unsubscribed Wallpaper Engine items from Steam Workshop (`appid=431960`)
/// and caches their thumbnails rapidly.
pub fn fetch_workshop_page(
    filter: &WorkshopFilter,
    page: u32,
    exclude_ids: &HashSet<String>,
) -> Vec<WorkshopWallpaperEntry> {
    fetch_workshop_page_limit(filter, page, exclude_ids, 18)
}

/// Fetches up to `limit` unsubscribed Wallpaper Engine items from Steam Workshop (`appid=431960`).
pub fn fetch_workshop_page_limit(
    filter: &WorkshopFilter,
    page: u32,
    exclude_ids: &HashSet<String>,
    limit: usize,
) -> Vec<WorkshopWallpaperEntry> {
    let entries = query_workshop_page_metadata(filter, page, exclude_ids, limit);
    if entries.is_empty() {
        return Vec::new();
    }
    let _ = download_workshop_thumbs_fast(&entries);
    entries
        .into_iter()
        .filter(|e| {
            e.thumb_path.is_file() && e.thumb_path.metadata().map(|m| m.len()).unwrap_or(0) > 256
        })
        .collect()
}

/// Stable filter signature for Wallhaven discovery cache invalidation.
pub fn wallhaven_filter_key(filter: &WallhavenFilter) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        filter.query.trim(),
        filter.categories,
        filter.sorting,
        filter.top_range,
        filter.atleast,
        filter.ratios,
        filter.color
    )
}

/// Stable filter signature for Wallpaper Engine discovery cache invalidation.
pub fn workshop_filter_key(filter: &WorkshopFilter) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        filter.query.trim(),
        filter.genre.trim(),
        filter.kind.trim(),
        filter.sort,
        filter.exclude_anime
    )
}

fn clean_tsv_cell(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ")
}

fn serialize_discovery_cache(filter_key: &str, rows: &[(&str, &str, &str, &Path)]) -> String {
    let mut out = format!("#V1\t{}\n", clean_tsv_cell(filter_key));
    for (id, title, url, thumb_path) in rows {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            clean_tsv_cell(id),
            clean_tsv_cell(title),
            clean_tsv_cell(url),
            clean_tsv_cell(&thumb_path.to_string_lossy())
        ));
    }
    out
}

fn parse_discovery_cache(text: &str, expected_key: &str) -> Vec<(String, String, String, PathBuf)> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let expected_header = format!("#V1\t{}", clean_tsv_cell(expected_key));
    if header != expected_header {
        return Vec::new();
    }
    lines
        .filter_map(|line| {
            let mut cols = line.splitn(4, '\t');
            let id = cols.next()?.to_string();
            let title = cols.next()?.to_string();
            let url = cols.next()?.to_string();
            let thumb = PathBuf::from(cols.next()?);
            if id.is_empty() || !thumb.is_file() {
                return None;
            }
            Some((id, title, url, thumb))
        })
        .collect()
}

fn discovery_cache_path(filename: &str) -> PathBuf {
    cache_subdir("", "literofi-cache").join(filename)
}

/// Loads cached Wallhaven discovery items if `filter_key` matches and thumbnails exist on disk.
pub fn load_wallhaven_cache(filter_key: &str) -> Vec<WebWallpaperEntry> {
    let path = discovery_cache_path("wh_discover_cache.tsv");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_discovery_cache(&text, filter_key)
        .into_iter()
        .map(|(id, title, full_url, thumb_path)| WebWallpaperEntry {
            id,
            title,
            thumb_path,
            full_url,
        })
        .collect()
}

/// Saves Wallhaven discovery items keyed by `filter_key` for instant subsequent opens.
pub fn save_wallhaven_cache(filter_key: &str, items: &[WebWallpaperEntry]) {
    let path = discovery_cache_path("wh_discover_cache.tsv");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let rows: Vec<(&str, &str, &str, &Path)> = items
        .iter()
        .take(36)
        .map(|w| {
            (
                w.id.as_str(),
                w.title.as_str(),
                w.full_url.as_str(),
                w.thumb_path.as_path(),
            )
        })
        .collect();
    let _ = std::fs::write(path, serialize_discovery_cache(filter_key, &rows));
}

/// Loads cached Wallpaper Engine discovery items matching `filter_key` and excluding `exclude_ids`.
pub fn load_workshop_cache(
    filter_key: &str,
    exclude_ids: &HashSet<String>,
) -> Vec<WorkshopWallpaperEntry> {
    let path = discovery_cache_path("we_discover_cache.tsv");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_discovery_cache(&text, filter_key)
        .into_iter()
        .filter(|(id, _, _, _)| !exclude_ids.contains(id))
        .map(
            |(id, title, preview_url, thumb_path)| WorkshopWallpaperEntry {
                id,
                title,
                preview_url,
                hd_preview_url: None,
                thumb_path,
            },
        )
        .collect()
}

/// Saves Wallpaper Engine discovery items keyed by `filter_key` for instant subsequent opens.
pub fn save_workshop_cache(filter_key: &str, items: &[WorkshopWallpaperEntry]) {
    let path = discovery_cache_path("we_discover_cache.tsv");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let rows: Vec<(&str, &str, &str, &Path)> = items
        .iter()
        .take(36)
        .map(|w| {
            (
                w.id.as_str(),
                w.title.as_str(),
                w.preview_url.as_str(),
                w.thumb_path.as_path(),
            )
        })
        .collect();
    let _ = std::fs::write(path, serialize_discovery_cache(filter_key, &rows));
}

/// Opens a Steam Workshop item page directly in the Steam client so the user can preview/subscribe.
pub fn open_steam_workshop_item(published_file_id: &str) {
    let uri = format!("steam://url/CommunityFilePage/{published_file_id}");
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/C", "start", "", &uri]);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let _ = cmd.spawn();
}

fn find_workshop_project_json(published_file_id: u64) -> Option<PathBuf> {
    let id_str = published_file_id.to_string();
    for root in discover_steam_roots() {
        let pj = root
            .join("steamapps")
            .join("workshop")
            .join("content")
            .join("431960")
            .join(&id_str)
            .join("project.json");
        if pj.is_file() {
            return Some(pj);
        }
    }
    None
}

fn find_steam_api64_dll() -> Option<PathBuf> {
    if let Some(exe) = find_wallpaper_engine_exe() {
        if let Some(we_dir) = exe.parent() {
            let bin_dll = we_dir.join("bin").join("steam_api64.dll");
            if bin_dll.is_file() {
                return Some(bin_dll);
            }
            let direct = we_dir.join("steam_api64.dll");
            if direct.is_file() {
                return Some(direct);
            }
        }
    }
    None
}

/// Subscribes to a Wallpaper Engine Steam Workshop item via `steam_api64.dll` (`ISteamUGC::SubscribeItem`),
/// waits for Steam to finish downloading `project.json` (up to 60s), and applies it to `monitor_index`.
#[cfg(windows)]
pub fn subscribe_and_apply_workshop_item(
    published_file_id: u64,
    monitor_index: u32,
    mute_audio: bool,
) -> Result<(), String> {
    use std::ffi::c_void;
    use std::time::Duration;
    use windows::core::{s, HSTRING};
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    if let Some(existing_pj) = find_workshop_project_json(published_file_id) {
        return apply_wallpaper_engine_item_on_monitor(&existing_pj, monitor_index, mute_audio);
    }

    let Some(dll_path) = find_steam_api64_dll() else {
        open_steam_workshop_item(&published_file_id.to_string());
        return Ok(());
    };

    std::env::set_var("SteamAppId", "431960");
    std::env::set_var("SteamGameId", "431960");

    let dll_h = HSTRING::from(dll_path.as_os_str());
    let Ok(hmod) = (unsafe { LoadLibraryW(&dll_h) }) else {
        open_steam_workshop_item(&published_file_id.to_string());
        return Ok(());
    };

    unsafe {
        let init_safe = GetProcAddress(hmod, s!("SteamAPI_InitSafe"))
            .map(|p| std::mem::transmute::<_, extern "C" fn() -> bool>(p));
        let init_flat = GetProcAddress(hmod, s!("SteamAPI_InitFlat"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut u8) -> i32>(p));
        let shutdown = GetProcAddress(hmod, s!("SteamAPI_Shutdown"))
            .map(|p| std::mem::transmute::<_, extern "C" fn()>(p));
        let run_callbacks = GetProcAddress(hmod, s!("SteamAPI_RunCallbacks"))
            .map(|p| std::mem::transmute::<_, extern "C" fn()>(p));
        let get_ugc = GetProcAddress(hmod, s!("SteamAPI_SteamUGC_v020"))
            .map(|p| std::mem::transmute::<_, extern "C" fn() -> *mut c_void>(p));
        let subscribe = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_SubscribeItem"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64) -> u64>(p));
        let download = GetProcAddress(hmod, s!("SteamAPI_ISteamUGC_DownloadItem"))
            .map(|p| std::mem::transmute::<_, extern "C" fn(*mut c_void, u64, bool) -> bool>(p));

        let init_ok = if let Some(f) = init_safe {
            f()
        } else if let Some(f) = init_flat {
            let mut err_buf = [0u8; 1024];
            f(err_buf.as_mut_ptr()) == 0
        } else {
            false
        };

        let mut applied = false;
        if init_ok {
            if let (Some(get_ugc_fn), Some(sub_fn), Some(dl_fn)) = (get_ugc, subscribe, download) {
                let ugc = get_ugc_fn();
                if !ugc.is_null() {
                    let _ = sub_fn(ugc, published_file_id);
                    let _ = dl_fn(ugc, published_file_id, true);

                    for _ in 0..120 {
                        if let Some(rc_fn) = run_callbacks {
                            rc_fn();
                        }
                        if let Some(pj) = find_workshop_project_json(published_file_id) {
                            std::thread::sleep(Duration::from_millis(400));
                            let _ = apply_wallpaper_engine_item_on_monitor(
                                &pj,
                                monitor_index,
                                mute_audio,
                            );
                            applied = true;
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            }
            if let Some(shut_fn) = shutdown {
                shut_fn();
            }
        }
        let _ = FreeLibrary(hmod);

        if !applied {
            open_steam_workshop_item(&published_file_id.to_string());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn subscribe_and_apply_workshop_item(
    _published_file_id: u64,
    _monitor_index: u32,
    _mute_audio: bool,
) -> Result<(), String> {
    Ok(())
}

/// Spawns a detached short-lived `literofi.exe --we-subscribe <id> <monitor_idx>` helper process
/// so the carousel closes immediately while Steam downloads and applies the wallpaper.
pub fn spawn_we_subscribe_helper(published_file_id: &str, monitor_index: u32) {
    if let Ok(exe) = std::env::current_exe() {
        let mut cmd = Command::new(exe);
        cmd.args([
            "--we-subscribe",
            published_file_id,
            &monitor_index.to_string(),
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW_FLAG: u32 = 0x0800_0000;
            const DETACHED_PROCESS_FLAG: u32 = 0x0000_0008;
            cmd.creation_flags(CREATE_NO_WINDOW_FLAG | DETACHED_PROCESS_FLAG);
        }
        if cmd.spawn().is_err() {
            open_steam_workshop_item(published_file_id);
        }
    } else {
        open_steam_workshop_item(published_file_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_wallpaper_name_strips_extension_and_separators() {
        assert_eq!(
            clean_wallpaper_name(Path::new("tokyo_night-4k.png")),
            "Tokyo Night 4k"
        );
        assert_eq!(clean_wallpaper_name(Path::new("img0.jpg")), "Img0");
    }

    #[test]
    fn extract_palette_from_rgb_samples_builds_valid_hex_colors() {
        let samples = vec![(20, 40, 120), (45, 130, 235), (30, 80, 210), (15, 25, 60)];
        let pal = extract_palette_from_rgb_samples(&samples, "Ocean");
        assert!(pal.bg.starts_with('#') && pal.bg.len() == 7);
        assert!(pal.bg_alt.starts_with('#') && pal.bg_alt.len() == 7);
        assert!(pal.selected.starts_with('#') && pal.selected.len() == 7);
        assert!(pal.bg_rgba.starts_with("rgba("));
        assert_eq!(pal.fg, "#eceff4");
    }

    #[test]
    fn build_wallhaven_url_applies_all_filters() {
        let f = WallhavenFilter {
            query: "cyberpunk city".to_string(),
            categories: "110".to_string(),
            sorting: "toplist".to_string(),
            top_range: "1w".to_string(),
            atleast: "3840x2160".to_string(),
            ratios: "16x9".to_string(),
            color: "#424153".to_string(),
        };
        let url = build_wallhaven_url(&f, "abc123", 2);
        assert!(url.contains("categories=110"));
        assert!(url.contains("purity=100"));
        assert!(url.contains("sorting=toplist"));
        assert!(url.contains("topRange=1w"));
        assert!(!url.contains("seed="));
        assert!(url.contains("page=2"));
        assert!(url.contains("q=cyberpunk+city"));
        assert!(url.contains("atleast=3840x2160"));
        assert!(url.contains("ratios=16x9"));
        assert!(url.contains("colors=424153"));

        let rnd = WallhavenFilter::default();
        let rnd_url = build_wallhaven_url(&rnd, "xyz789", 1);
        assert!(rnd_url.contains("sorting=random"));
        assert!(rnd_url.contains("seed=xyz789"));
        assert!(!rnd_url.contains("topRange="));
    }

    #[test]
    fn parse_we_project_json_extracts_title_preview_and_type() {
        let sample = r#"{
            "file" : "scene.json",
            "preview" : "preview.jpg",
            "title" : "Razer Vice City",
            "type" : "scene"
        }"#;
        let (title, preview, kind) = parse_we_project_json(sample).unwrap();
        assert_eq!(title, "Razer Vice City");
        assert_eq!(preview.as_deref(), Some("preview.jpg"));
        assert_eq!(kind, "scene");
    }

    #[test]
    fn parse_steam_workshop_ssr_extracts_escaped_items() {
        let sample = r#"\"results\":[{\"publishedfileid\":\"3843147764\",\"creator\":\"76561199370851607\",\"consumer_appid\":431960,\"file_type\":0,\"preview_url\":\"https://images.steamusercontent.com/ugc/17365798859941903490/3825027A8C7D0947B8C951BAD8AE1B0E67F992DB/\",\"title\":\"Cyberpunk \\\"Neon\\\" City\"},{\"publishedfileid\":\"9988776655\",\"preview_url\":\"https://images.steamusercontent.com/ugc/123/456/\",\"title\":\"Quiet Forest\"}]"#;
        let items = parse_steam_workshop_ssr(sample);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "3843147764");
        assert_eq!(items[0].1, "Cyberpunk \"Neon\" City");
        assert_eq!(
            items[0].2,
            "https://images.steamusercontent.com/ugc/17365798859941903490/3825027A8C7D0947B8C951BAD8AE1B0E67F992DB/"
        );
        assert_eq!(items[1].0, "9988776655");
        assert_eq!(items[1].1, "Quiet Forest");
    }

    #[test]
    fn build_workshop_url_applies_filters_and_hides_anime_by_default() {
        let def = WorkshopFilter::default();
        let def_url = build_workshop_url(&def, 1);
        assert!(def_url.contains("appid=431960"));
        assert!(def_url.contains("browsesort=toprated"));
        assert!(def_url.contains("requiredtags%5B%5D=Everyone"));
        assert!(def_url.contains("excludedtags%5B%5D=Anime"));
        assert!(def_url.contains("excludedtags%5B%5D=Mature"));

        let custom = WorkshopFilter {
            query: "neon rain".to_string(),
            genre: "Cyberpunk".to_string(),
            kind: "Scene".to_string(),
            sort: "trend".to_string(),
            exclude_anime: true,
        };
        let custom_url = build_workshop_url(&custom, 3);
        assert!(custom_url.contains("browsesort=trend"));
        assert!(custom_url.contains("p=3"));
        assert!(custom_url.contains("requiredtags%5B%5D=Cyberpunk"));
        assert!(custom_url.contains("requiredtags%5B%5D=Scene"));
        assert!(custom_url.contains("searchtext=neon+rain"));

        let anime_genre = WorkshopFilter {
            genre: "Anime".to_string(),
            exclude_anime: true,
            ..Default::default()
        };
        assert!(!build_workshop_url(&anime_genre, 1).contains("excludedtags%5B%5D=Anime"));
    }

    #[test]
    fn workshop_cache_roundtrip_respects_filter_key() {
        let tmp = std::env::temp_dir().join("literofi_we_cache_test_thumb.jpg");
        let _ = std::fs::write(&tmp, b"fakeimg");
        let filter_a = WorkshopFilter::default();
        let filter_b = WorkshopFilter {
            genre: "Cyberpunk".to_string(),
            ..Default::default()
        };
        let key_a = workshop_filter_key(&filter_a);
        let key_b = workshop_filter_key(&filter_b);
        let raw = serialize_discovery_cache(
            &key_a,
            &[(
                "123456",
                "Test Scene",
                "https://example.com/123.jpg",
                tmp.as_path(),
            )],
        );
        let parsed_a = parse_discovery_cache(&raw, &key_a);
        assert_eq!(parsed_a.len(), 1);
        assert_eq!(parsed_a[0].0, "123456");
        assert!(parse_discovery_cache(&raw, &key_b).is_empty());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn truncate_gif_to_first_frame_extracts_valid_single_frame() {
        // Minimal GIF89a (1x1) with 2 frames + trailing garbage simulating Range truncation
        let two_frame_gif: Vec<u8> = vec![
            b'G', b'I', b'F', b'8', b'9', b'a', // header
            1, 0, 1, 0, 0x80, 0, 0, // LSD: 1x1, GCT flag, 2 colors (6 bytes)
            0, 0, 0, 255, 255, 255, // GCT
            0x21, 0xF9, 4, 0, 10, 0, 0, 0, // Graphic Control Extension for Frame 0
            0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, // Image Descriptor 0 (no LCT)
            2, // LZW min code size
            2, 0x4C, 0x01, 0x00, // LZW sub-block + block terminator
            // Frame 1 truncated mid-stream:
            0x21, 0xF9, 4, 0, 10, 0, 0, 0, 0x2C, 0, 0,
        ];
        let trimmed = truncate_gif_to_first_frame(&two_frame_gif).expect("frame 0 should parse");
        assert!(trimmed.ends_with(&[0x00, 0x3B]));
        assert_eq!(trimmed.len(), 43);
        assert!(truncate_gif_to_first_frame(b"NOTAGIF").is_none());
        assert!(truncate_gif_to_first_frame(&two_frame_gif[..30]).is_none());
    }

    #[test]
    fn is_complete_non_gif_image_detects_jpeg_png_truncation() {
        let mut small_jpg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        small_jpg.resize(512, 0xAA);
        assert!(is_complete_non_gif_image(&small_jpg, 196_608));

        let mut capped_jpg_incomplete = vec![0xFF, 0xD8, 0xFF, 0xE0];
        capped_jpg_incomplete.resize(196_608, 0x11);
        assert!(!is_complete_non_gif_image(&capped_jpg_incomplete, 196_608));

        let mut capped_jpg_complete = capped_jpg_incomplete.clone();
        let n = capped_jpg_complete.len();
        capped_jpg_complete[n - 2] = 0xFF;
        capped_jpg_complete[n - 1] = 0xD9;
        assert!(is_complete_non_gif_image(&capped_jpg_complete, 196_608));

        let mut html_err = b"<!DOCTYPE html><html>404</html>".to_vec();
        html_err.resize(256, b' ');
        assert!(!is_complete_non_gif_image(&html_err, 196_608));
    }

    #[test]
    fn workshop_sort_maps_to_expected_steam_ugc_query() {
        assert_eq!(workshop_sort_to_eugc_query("toprated"), 0);
        assert_eq!(workshop_sort_to_eugc_query("trend"), 3);
        assert_eq!(workshop_sort_to_eugc_query("mostrecent"), 1);
        assert_eq!(workshop_sort_to_eugc_query("totaluniquesubscribers"), 12);
    }
}
