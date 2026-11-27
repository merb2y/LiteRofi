//! Wallpaper Effects mode (`-show wallpaper-effects`, inspired by `JaKooLit/WallpaperEffects.sh`).
//! Decodes the current desktop wallpaper via Windows WIC, applies pure-Rust image effects
//! (Blur, Frosted Glass, Dim, B&W, Sepia, Vignette, Posterize, Solarize, Negate, Sharpen),
//! writes lossless BMP to cache, and applies it instantaneously — always preserving the original
//! wallpaper path so users can switch effects or restore the original at any time.

use crate::active_palette::ActivePalette;
use crate::wallpaper::{current_windows_wallpaper, set_windows_wallpaper};
use crate::{resolve_cache_dir, ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperEffect {
    Original,
    SoftBlur,
    HeavyBlur,
    DimDarken,
    BlackAndWhite,
    Sepia,
    Vignette,
    Posterize,
    Solarize,
    Negate,
    Sharpen,
}

struct EffectSpec {
    effect: WallpaperEffect,
    icon: &'static str,
    title: &'static str,
    desc: &'static str,
}

const EFFECTS: &[EffectSpec] = &[
    EffectSpec {
        effect: WallpaperEffect::Original,
        icon: "󰑐",
        title: "No Effects (Restore Original)",
        desc: "Revert desktop back to the unmodified original wallpaper",
    },
    EffectSpec {
        effect: WallpaperEffect::SoftBlur,
        icon: "󰂵",
        title: "Soft Blur",
        desc: "Smooth photographic blur (radius 8) for clean icon focus",
    },
    EffectSpec {
        effect: WallpaperEffect::HeavyBlur,
        icon: "󰖑",
        title: "Frosted Glass Blur",
        desc: "Deep 3-pass ambient blur (radius 20)",
    },
    EffectSpec {
        effect: WallpaperEffect::DimDarken,
        icon: "󰖔",
        title: "Dim / Darken (35%)",
        desc: "Subtle dark tint for high contrast desktop readability",
    },
    EffectSpec {
        effect: WallpaperEffect::BlackAndWhite,
        icon: "󰌁",
        title: "Black & White",
        desc: "High-contrast monochrome grayscale",
    },
    EffectSpec {
        effect: WallpaperEffect::Sepia,
        icon: "󰏘",
        title: "Sepia Tone",
        desc: "Warm vintage film palette",
    },
    EffectSpec {
        effect: WallpaperEffect::Vignette,
        icon: "󰄀",
        title: "Cinema Vignette",
        desc: "Soft radial edge darkening around center focus",
    },
    EffectSpec {
        effect: WallpaperEffect::Sharpen,
        icon: "󰃟",
        title: "Sharpen",
        desc: "Crisp edge enhancement filter",
    },
    EffectSpec {
        effect: WallpaperEffect::Posterize,
        icon: "󰉼",
        title: "Posterize",
        desc: "Stylized 5-level flat color art",
    },
    EffectSpec {
        effect: WallpaperEffect::Solarize,
        icon: "󰖙",
        title: "Solarize",
        desc: "Inverted highlight Sabattier effect",
    },
    EffectSpec {
        effect: WallpaperEffect::Negate,
        icon: "󰔎",
        title: "Negate (Invert)",
        desc: "Full RGB color inversion",
    },
];

/// Stateful wallpaper effects picker mode.
pub struct WallpaperEffectsMode {
    display: String,
    cache_dir: PathBuf,
    dry_run: bool,
    palette: Option<ActivePalette>,
    last_status: Option<String>,
    reload_hints: ReloadHints,
}

impl WallpaperEffectsMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "wallpaper-effects".to_string(),
            cache_dir: resolve_cache_dir(cfg).join("wallpaper-effects"),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
            last_status: None,
            reload_hints: ReloadHints::default(),
        }
    }

    fn original_marker_path(&self) -> PathBuf {
        self.cache_dir.join("original_path.txt")
    }

    fn resolve_original_wallpaper(&self) -> Option<PathBuf> {
        let cur = current_windows_wallpaper();
        if let Some(ref p) = cur {
            let is_our_effect = p.starts_with(&self.cache_dir);
            if !is_our_effect && p.is_file() {
                if !self.dry_run {
                    let _ = std::fs::create_dir_all(&self.cache_dir);
                    let _ =
                        std::fs::write(self.original_marker_path(), p.to_string_lossy().as_bytes());
                }
                return Some(p.clone());
            }
        }
        if let Ok(saved) = std::fs::read_to_string(self.original_marker_path()) {
            let p = PathBuf::from(saved.trim());
            if p.is_file() {
                return Some(p);
            }
        }
        cur.filter(|p| p.is_file())
    }

    fn apply_selected(&mut self, effect: WallpaperEffect) -> Result<(), String> {
        if self.dry_run {
            self.last_status = Some(format!("Dry-run applied: {effect:?}"));
            return Ok(());
        }
        let Some(orig) = self.resolve_original_wallpaper() else {
            return Err("Could not locate current Windows wallpaper".to_string());
        };
        if effect == WallpaperEffect::Original {
            set_windows_wallpaper(&orig)?;
            self.last_status = Some("Restored original wallpaper".to_string());
            return Ok(());
        }
        let (w, h, mut bgra) = decode_image_bgra(&orig)
            .ok_or_else(|| format!("Failed to decode wallpaper {}", orig.display()))?;
        apply_effect_bgra(&mut bgra, w, h, effect);
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|e| format!("Failed to create {}: {e}", self.cache_dir.display()))?;
        let out_path = self.cache_dir.join(format!("effect_{effect:?}.bmp"));
        write_bgra_as_bmp(&out_path, w, h, &bgra)?;
        set_windows_wallpaper(&out_path)?;
        self.last_status = Some(format!("Applied {effect:?} to wallpaper"));
        Ok(())
    }
}

impl RichMode for WallpaperEffectsMode {
    fn name(&self) -> &str {
        "wallpaper-effects"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some("󰸉 Wallpaper Effects".to_string())
    }

    fn message(&self) -> Option<String> {
        if let Some(status) = &self.last_status {
            return Some(status.clone());
        }
        Some(
            "Pure-Rust instant effects on active wallpaper · Enter: Apply · Shift+Enter: Apply & Keep Open"
                .to_string(),
        )
    }

    fn theme_snippets(&self) -> Vec<String> {
        vec!["element-icon { enabled: false; }".to_string()]
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn entries(&self) -> Vec<ListEntry> {
        EFFECTS
            .iter()
            .map(|e| ListEntry {
                text: format!("{}  {}   —   {}", e.icon, e.title, e.desc),
                meta: Some(format!("{} {}", e.title, e.desc)),
                ..ListEntry::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Accepted { index, .. } => {
                if let Some(spec) = EFFECTS.get(*index) {
                    if let Err(err) = self.apply_selected(spec.effect) {
                        self.last_status = Some(err);
                        return Ok(ModeStep::Reload);
                    }
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                if let Some(spec) = EFFECTS.get(*index) {
                    if let Err(err) = self.apply_selected(spec.effect) {
                        self.last_status = Some(err);
                    }
                }
                self.reload_hints = ReloadHints {
                    keep_selection: true,
                    new_selection: None,
                    keep_filter: true,
                };
                Ok(ModeStep::Reload)
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => Ok(ModeStep::Reload),
        }
    }
}

/// Applies the selected effect in-place on a `w × h` BGRA pixel buffer.
pub fn apply_effect_bgra(buf: &mut [u8], w: usize, h: usize, effect: WallpaperEffect) {
    if w == 0 || h == 0 || buf.len() < w * h * 4 {
        return;
    }
    match effect {
        WallpaperEffect::Original => {}
        WallpaperEffect::SoftBlur => {
            box_blur_bgra(buf, w, h, 8, 2);
        }
        WallpaperEffect::HeavyBlur => {
            box_blur_bgra(buf, w, h, 20, 3);
        }
        WallpaperEffect::DimDarken => {
            for px in buf.chunks_exact_mut(4) {
                px[0] = ((px[0] as u32 * 65) / 100) as u8;
                px[1] = ((px[1] as u32 * 65) / 100) as u8;
                px[2] = ((px[2] as u32 * 65) / 100) as u8;
            }
        }
        WallpaperEffect::BlackAndWhite => {
            for px in buf.chunks_exact_mut(4) {
                let lum =
                    (0.114 * px[0] as f32 + 0.587 * px[1] as f32 + 0.299 * px[2] as f32) / 255.0;
                // Mild S-curve contrast boost
                let c = ((lum - 0.5) * 1.25 + 0.5).clamp(0.0, 1.0);
                let v = (c * 255.0) as u8;
                px[0] = v;
                px[1] = v;
                px[2] = v;
            }
        }
        WallpaperEffect::Sepia => {
            for px in buf.chunks_exact_mut(4) {
                let b = px[0] as f32;
                let g = px[1] as f32;
                let r = px[2] as f32;
                let nr = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0) as u8;
                let ng = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0) as u8;
                let nb = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0) as u8;
                px[0] = nb;
                px[1] = ng;
                px[2] = nr;
            }
        }
        WallpaperEffect::Vignette => {
            let cx = w as f32 * 0.5;
            let cy = h as f32 * 0.5;
            let max_d2 = cx * cx + cy * cy;
            for y in 0..h {
                let dy = y as f32 - cy;
                let dy2 = dy * dy;
                for x in 0..w {
                    let dx = x as f32 - cx;
                    let norm = ((dx * dx + dy2) / max_d2).clamp(0.0, 1.0);
                    let factor = (1.0 - norm * norm * 0.72).clamp(0.18, 1.0);
                    let idx = (y * w + x) * 4;
                    buf[idx] = (buf[idx] as f32 * factor) as u8;
                    buf[idx + 1] = (buf[idx + 1] as f32 * factor) as u8;
                    buf[idx + 2] = (buf[idx + 2] as f32 * factor) as u8;
                }
            }
        }
        WallpaperEffect::Posterize => {
            let levels = 4.0f32;
            for px in buf.chunks_exact_mut(4) {
                for ch in px.iter_mut().take(3) {
                    let q = ((*ch as f32 / 255.0) * levels).round() / levels;
                    *ch = (q * 255.0).clamp(0.0, 255.0) as u8;
                }
            }
        }
        WallpaperEffect::Solarize => {
            for px in buf.chunks_exact_mut(4) {
                for ch in px.iter_mut().take(3) {
                    if *ch > 128 {
                        *ch = 255 - *ch;
                    }
                }
            }
        }
        WallpaperEffect::Negate => {
            for px in buf.chunks_exact_mut(4) {
                px[0] = 255 - px[0];
                px[1] = 255 - px[1];
                px[2] = 255 - px[2];
            }
        }
        WallpaperEffect::Sharpen => {
            let src = buf.to_vec();
            for y in 1..h.saturating_sub(1) {
                for x in 1..w.saturating_sub(1) {
                    let idx = (y * w + x) * 4;
                    let up = ((y - 1) * w + x) * 4;
                    let dn = ((y + 1) * w + x) * 4;
                    let lt = (y * w + (x - 1)) * 4;
                    let rt = (y * w + (x + 1)) * 4;
                    for c in 0..3 {
                        let v = 5 * src[idx + c] as i32
                            - src[up + c] as i32
                            - src[dn + c] as i32
                            - src[lt + c] as i32
                            - src[rt + c] as i32;
                        buf[idx + c] = v.clamp(0, 255) as u8;
                    }
                }
            }
        }
    }
}

/// Fast O(W*H) separable sliding-window box blur (repeated `passes` times approximates Gaussian).
fn box_blur_bgra(buf: &mut [u8], w: usize, h: usize, radius: usize, passes: usize) {
    if radius == 0 || w < 2 || h < 2 {
        return;
    }
    let mut tmp = vec![0u8; w * h * 4];
    let win = (radius * 2 + 1) as u32;
    for _ in 0..passes {
        // Horizontal pass: buf -> tmp
        for y in 0..h {
            let row = y * w;
            let mut sb = 0u32;
            let mut sg = 0u32;
            let mut sr = 0u32;
            for dx in -(radius as isize)..=(radius as isize) {
                let sx = dx.clamp(0, (w - 1) as isize) as usize;
                let idx = (row + sx) * 4;
                sb += buf[idx] as u32;
                sg += buf[idx + 1] as u32;
                sr += buf[idx + 2] as u32;
            }
            for x in 0..w {
                let dst = (row + x) * 4;
                tmp[dst] = (sb / win) as u8;
                tmp[dst + 1] = (sg / win) as u8;
                tmp[dst + 2] = (sr / win) as u8;
                tmp[dst + 3] = 255;

                let old_x = x.saturating_sub(radius);
                let new_x = (x + radius + 1).min(w - 1);
                let old_i = (row + old_x) * 4;
                let new_i = (row + new_x) * 4;
                sb = sb + buf[new_i] as u32 - buf[old_i] as u32;
                sg = sg + buf[new_i + 1] as u32 - buf[old_i + 1] as u32;
                sr = sr + buf[new_i + 2] as u32 - buf[old_i + 2] as u32;
            }
        }
        // Vertical pass: tmp -> buf
        for x in 0..w {
            let mut sb = 0u32;
            let mut sg = 0u32;
            let mut sr = 0u32;
            for dy in -(radius as isize)..=(radius as isize) {
                let sy = dy.clamp(0, (h - 1) as isize) as usize;
                let idx = (sy * w + x) * 4;
                sb += tmp[idx] as u32;
                sg += tmp[idx + 1] as u32;
                sr += tmp[idx + 2] as u32;
            }
            for y in 0..h {
                let dst = (y * w + x) * 4;
                buf[dst] = (sb / win) as u8;
                buf[dst + 1] = (sg / win) as u8;
                buf[dst + 2] = (sr / win) as u8;
                buf[dst + 3] = 255;

                let old_y = y.saturating_sub(radius);
                let new_y = (y + radius + 1).min(h - 1);
                let old_i = (old_y * w + x) * 4;
                let new_i = (new_y * w + x) * 4;
                sb = sb + tmp[new_i] as u32 - tmp[old_i] as u32;
                sg = sg + tmp[new_i + 1] as u32 - tmp[old_i + 1] as u32;
                sr = sr + tmp[new_i + 2] as u32 - tmp[old_i + 2] as u32;
            }
        }
    }
}

#[cfg(windows)]
fn decode_image_bgra(path: &Path) -> Option<(usize, usize, Vec<u8>)> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICBitmapSource,
        IWICFormatConverter, IWICImagingFactory, WICBitmapDitherTypeNone,
        WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnLoad,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };

    let wpath = HSTRING::from(path.as_os_str());
    unsafe {
        let com_ok = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let res = (|| -> Option<(usize, usize, Vec<u8>)> {
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let decoder = wic
                .CreateDecoderFromFilename(&wpath, None, GENERIC_READ, WICDecodeMetadataCacheOnLoad)
                .ok()?;
            let frame = decoder.GetFrame(0).ok()?;
            let mut w = 0u32;
            let mut h = 0u32;
            frame.GetSize(&mut w, &mut h).ok()?;
            if w == 0 || h == 0 || w > 8192 || h > 8192 {
                return None;
            }
            let src: IWICBitmapSource = frame.into();
            let conv: IWICFormatConverter = wic.CreateFormatConverter().ok()?;
            conv.Initialize(
                &src,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .ok()?;
            let conv_src: IWICBitmapSource = conv.into();
            let stride = w * 4;
            let mut pixels = vec![0u8; (stride as usize) * (h as usize)];
            conv_src
                .CopyPixels(std::ptr::null(), stride, &mut pixels)
                .ok()?;
            Some((w as usize, h as usize, pixels))
        })();
        if com_ok {
            CoUninitialize();
        }
        res
    }
}

#[cfg(not(windows))]
fn decode_image_bgra(_path: &Path) -> Option<(usize, usize, Vec<u8>)> {
    None
}

fn write_bgra_as_bmp(path: &Path, w: usize, h: usize, bgra: &[u8]) -> Result<(), String> {
    let row_bytes = w * 3;
    let pad = (4 - (row_bytes % 4)) % 4;
    let stride = row_bytes + pad;
    let img_size = stride * h;
    let file_size = 54 + img_size;

    let mut out = Vec::with_capacity(file_size);
    // BITMAPFILEHEADER (14 bytes)
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    // BITMAPINFOHEADER (40 bytes)
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes()); // bottom-up
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(img_size as u32).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    let zeros = [0u8; 3];
    for y in (0..h).rev() {
        let row_start = y * w * 4;
        for x in 0..w {
            let i = row_start + x * 4;
            out.push(bgra[i]);
            out.push(bgra[i + 1]);
            out.push(bgra[i + 2]);
        }
        if pad > 0 {
            out.extend_from_slice(&zeros[..pad]);
        }
    }
    std::fs::write(path, out).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallpaper_effects_transform_bgra_pixels() {
        let mut px = vec![
            100u8, 150, 200, 255, 50, 80, 120, 255, 10, 20, 30, 255, 220, 230, 240, 255,
        ];
        let orig = px.clone();
        apply_effect_bgra(&mut px, 2, 2, WallpaperEffect::Negate);
        assert_eq!(px[0], 255 - orig[0]);
        assert_eq!(px[2], 255 - orig[2]);

        let mut bw = orig.clone();
        apply_effect_bgra(&mut bw, 2, 2, WallpaperEffect::BlackAndWhite);
        assert_eq!(bw[0], bw[1]);
        assert_eq!(bw[1], bw[2]);
    }
}
