//! Native 60fps Direct2D Slanted-Parallelogram Wallpaper Carousel (`Alt+W`).
//!
//! Inspired by `diegoMalagrida/pocket-dots`'s frameless Quickshell wallpaper picker:
//! - Completely frameless, transparent per-pixel-alpha layered Win32 overlay.
//! - Horizontal 5-card slanted parallelogram strip (1 large focused 16:9 hero card in the center
//!   + 2 flanking slices on each side with progressive opacity fade-out).
//! - Images remain upright and undistorted inside the parallelogram clip mask (`object-fit: cover`).
//! - Smooth 60fps spring/lerp scrolling via Left/Right arrows, A/D, H/L, mouse wheel, or clicking
//!   flanking cards; `Enter` or clicking the center card applies immediately; `T` or `Shift+Enter`
//!   applies + extracts an Auto-Theme color palette; `Esc` or focus loss dismisses cleanly.

use crate::layout::Rect;
use crate::win32_icons::{draw_bitmap_cover_aligned, IconCache, WM_APP_ICON_READY};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use windows::core::{w, HSTRING, PCWSTR};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::{GENERIC_WRITE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1Geometry, ID2D1HwndRenderTarget,
    ID2D1PathGeometry, ID2D1RenderTarget, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LAYER_OPTIONS_NONE,
    D2D1_LAYER_PARAMETERS, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
};
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW,
    MonitorFromPoint, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
    IWICImagingFactory, WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
    GetWindowLongPtrW, LoadCursorW, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW,
    CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, IDC_ARROW, MSG, PM_REMOVE, SW_SHOW, WA_INACTIVE,
    WM_ACTIVATE, WM_APP, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_MOUSEWHEEL, WM_NCCREATE, WM_QUIT, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// Entry displayed in the wallpaper carousel.
#[derive(Debug, Clone)]
pub struct CarouselItem {
    pub title: String,
    pub path: PathBuf,
    pub is_current: bool,
}

/// Active source displayed in the wallpaper carousel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarouselSource {
    Local,
    Web,
    WallpaperEngine,
}

/// Action returned when the wallpaper carousel closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarouselOutcome {
    Cancelled,
    Apply(usize),
    ApplyWithTheme(usize),
    OpenInWallpaperEngine(usize),
    ToggleSource,
    SwitchToWallpaperEngine,
    RefreshWeb,
}

const CAROUSEL_HD_BUCKET_PX: u32 = 1024;
const WM_APP_MORE_ITEMS: u32 = WM_APP + 61;
const WM_APP_THUMB_READY: u32 = WM_APP + 62;

struct OffscreenSurface {
    hdc: HDC,
    hbmp: HBITMAP,
    prev: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
}

impl OffscreenSurface {
    unsafe fn new(w: i32, h: i32) -> Result<Self, String> {
        let w = w.max(1);
        let h = h.max(1);
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return Err("CreateCompatibleDC failed".to_string());
        }
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbmp = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| format!("CreateDIBSection failed: {e}"))?;
        let prev = SelectObject(hdc, HGDIOBJ(hbmp.0));
        Ok(Self {
            hdc,
            hbmp,
            prev,
            bits: bits as *mut u8,
            w,
            h,
        })
    }
}

impl Drop for OffscreenSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.prev);
            let _ = DeleteObject(HGDIOBJ(self.hbmp.0));
            let _ = DeleteDC(self.hdc);
        }
    }
}

#[derive(Clone, Copy)]
struct CardGeom {
    rel_d: f32,
    item_idx: usize,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    slant: f32,
    opacity: f32,
    focus: f32,
}

struct CarouselState {
    items: Vec<CarouselItem>,
    accent: (f32, f32, f32),
    source: CarouselSource,
    win_x: i32,
    win_y: i32,
    win_w_phys: i32,
    win_h_phys: i32,
    win_w_dip: f32,
    win_h_dip: f32,
    dpi_scale: f32,
    scroll_pos: f32,
    target_pos: f32,
    opened_at: Instant,
    last_frame: Instant,
    needs_redraw: bool,
    outcome: Option<CarouselOutcome>,
    d2d: ID2D1Factory,
    _dwrite: IDWriteFactory,
    text_fmt: IDWriteTextFormat,
    hwnd_rt: Option<ID2D1HwndRenderTarget>,
    dc_rt: Option<ID2D1DCRenderTarget>,
    surface: Option<OffscreenSurface>,
    icons: IconCache,
    last_cards: Vec<CardGeom>,
    more_req_tx: Option<mpsc::Sender<()>>,
    more_res_rx: Option<mpsc::Receiver<Vec<CarouselItem>>>,
    thumb_ready_rx: Option<mpsc::Receiver<Vec<PathBuf>>>,
    loading_more: bool,
    wheel_accum: i32,
}

fn wrap_index(idx: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    idx.rem_euclid(len as i32) as usize
}

fn smoothstep(t: f32) -> f32 {
    let c = t.clamp(0.0, 1.0);
    c * c * (3.0 - 2.0 * c)
}

fn compute_visible_cards(
    win_w: f32,
    win_h: f32,
    scroll_pos: f32,
    item_count: usize,
    source: CarouselSource,
) -> Vec<CardGeom> {
    if item_count == 0 {
        return Vec::new();
    }
    let center_idx = scroll_pos.round() as i32;
    let center_x = win_w * 0.5;
    let center_y = win_h * 0.46;

    // Wallpaper Engine Workshop hosts 192-256px square preview.gif assets for ~70% of items,
    // so use compact square-friendly slanted proportions there to prevent excessive zoom/blur.
    let (hero_h, hero_w, side_h, side_w, slant_ratio) = match source {
        CarouselSource::WallpaperEngine => {
            let hh = (win_h * 0.56).clamp(200.0, 320.0);
            let hw = (hh * 1.18).min(win_w * 0.28);
            let sh = hh * 0.64;
            let sw = (win_w * 0.115).clamp(100.0, 190.0);
            (hh, hw, sh, sw, 0.18f32)
        }
        CarouselSource::Local | CarouselSource::Web => {
            let hh = (win_h * 0.66).clamp(210.0, 440.0);
            let hw = (hh * (16.0 / 9.0)).min(win_w * 0.40);
            let sh = hh * 0.54;
            let sw = (win_w * 0.122).clamp(110.0, 220.0);
            (hh, hw, sh, sw, 0.24f32)
        }
    };
    let gap_center = 10.0;
    let gap_side = 8.0;

    let mut cards = Vec::with_capacity(7);
    let half_span: i32 = if item_count <= 1 {
        0
    } else if item_count <= 3 {
        1
    } else {
        3
    };

    for offset in -half_span..=half_span {
        let slot = center_idx + offset;
        let rel_d = slot as f32 - scroll_pos;
        let ad = rel_d.abs();
        if ad > 2.75 {
            continue;
        }
        let focus = smoothstep(1.0 - ad);
        let w = side_w + (hero_w - side_w) * focus;
        let h = side_h + (hero_h - side_h) * focus;
        let slant = h * slant_ratio;

        let sign = if rel_d >= 0.0 { 1.0 } else { -1.0 };
        let x_off = if ad <= 1.0 {
            ad * (hero_w * 0.5 + side_w * 0.5 + gap_center)
        } else {
            (hero_w * 0.5 + side_w * 0.5 + gap_center) + (ad - 1.0) * (side_w + gap_side)
        };

        let opacity = if ad <= 1.0 {
            1.0 - 0.18 * ad
        } else if ad <= 2.0 {
            0.82 - 0.42 * (ad - 1.0)
        } else {
            (0.40 * (1.0 - (ad - 2.0) / 0.75)).clamp(0.0, 0.40)
        };

        cards.push(CardGeom {
            rel_d,
            item_idx: wrap_index(slot, item_count),
            cx: center_x + sign * x_off,
            cy: center_y,
            w,
            h,
            slant,
            opacity,
            focus,
        });
    }

    // Painter's order: draw outer cards first, focused center card last (highest Z)
    cards.sort_by(|a, b| {
        b.rel_d
            .abs()
            .partial_cmp(&a.rel_d.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cards
}

unsafe fn make_parallelogram_geometry(
    d2d: &ID2D1Factory,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    slant: f32,
) -> Option<ID2D1PathGeometry> {
    let geom = d2d.CreatePathGeometry().ok()?;
    let sink = geom.Open().ok()?;
    let hw = w * 0.5;
    let hh = h * 0.5;
    let hs = slant * 0.5;
    let p_tl = D2D_POINT_2F {
        x: cx - hw + hs,
        y: cy - hh,
    };
    let p_tr = D2D_POINT_2F {
        x: cx + hw + hs,
        y: cy - hh,
    };
    let p_br = D2D_POINT_2F {
        x: cx + hw - hs,
        y: cy + hh,
    };
    let p_bl = D2D_POINT_2F {
        x: cx - hw - hs,
        y: cy + hh,
    };
    sink.BeginFigure(p_tl, D2D1_FIGURE_BEGIN_FILLED);
    sink.AddLine(p_tr);
    sink.AddLine(p_br);
    sink.AddLine(p_bl);
    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
    sink.Close().ok()?;
    Some(geom)
}

fn point_in_parallelogram(px: f32, py: f32, c: &CardGeom) -> bool {
    let hh = c.h * 0.5;
    if py < c.cy - hh || py > c.cy + hh {
        return false;
    }
    let t = (c.cy - py) / hh; // +1 at top, -1 at bottom
    let row_cx = c.cx + t * (c.slant * 0.5);
    let hw = c.w * 0.5;
    px >= row_cx - hw && px <= row_cx + hw
}

impl CarouselState {
    fn open_alpha(&self, force_opaque: bool) -> f32 {
        if force_opaque {
            return 1.0;
        }
        let ms = self.opened_at.elapsed().as_secs_f32() * 1000.0;
        smoothstep(ms / 150.0)
    }

    fn is_animating(&self) -> bool {
        let diff = (self.target_pos - self.scroll_pos).abs();
        let fading = self.opened_at.elapsed().as_secs_f32() < 0.16;
        diff >= 0.0015 || fading
    }

    fn paint(&mut self, hwnd: HWND, async_icons: bool, force_opaque: bool) -> Result<(), String> {
        let global_alpha = self.open_alpha(force_opaque);
        let cards = compute_visible_cards(
            self.win_w_dip,
            self.win_h_dip,
            self.scroll_pos,
            self.items.len(),
            self.source,
        );
        self.last_cards = cards.clone();
        self.needs_redraw = false;

        let active_item = wrap_index(self.target_pos.round() as i32, self.items.len());

        unsafe {
            let rt: ID2D1RenderTarget = if let Some(hrt) = &self.hwnd_rt {
                hrt.SetDpi(96.0 * self.dpi_scale, 96.0 * self.dpi_scale);
                hrt.clone().into()
            } else if let (Some(drt), Some(surf)) = (&self.dc_rt, &self.surface) {
                let bind_rect = RECT {
                    left: 0,
                    top: 0,
                    right: surf.w,
                    bottom: surf.h,
                };
                drt.BindDC(surf.hdc, &bind_rect)
                    .map_err(|e| format!("BindDC failed: {e}"))?;
                drt.SetDpi(96.0 * self.dpi_scale, 96.0 * self.dpi_scale);
                drt.clone().into()
            } else {
                return Err("no active D2D render target".to_string());
            };

            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            for card in &cards {
                let eff_alpha = (card.opacity * global_alpha).clamp(0.0, 1.0);
                if eff_alpha <= 0.01 {
                    continue;
                }

                // Soft symmetric drop shadow behind focused center card
                if card.focus > 0.2 {
                    for (expand, y_off, a) in [
                        (14.0f32, 8.0f32, 0.08f32),
                        (8.0, 5.0, 0.12),
                        (3.0, 2.5, 0.16),
                    ] {
                        if let Some(sh_geom) = make_parallelogram_geometry(
                            &self.d2d,
                            card.cx,
                            card.cy + y_off,
                            card.w + expand,
                            card.h + expand * 0.7,
                            card.slant,
                        ) {
                            if let Ok(sh_brush) = rt.CreateSolidColorBrush(
                                &D2D1_COLOR_F {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: a * card.focus * global_alpha,
                                },
                                None,
                            ) {
                                rt.FillGeometry(&sh_geom, &sh_brush, None);
                            }
                        }
                    }
                }

                let Some(path_geom) = make_parallelogram_geometry(
                    &self.d2d, card.cx, card.cy, card.w, card.h, card.slant,
                ) else {
                    continue;
                };
                let mask_geom: ID2D1Geometry = path_geom.clone().into();

                let bbox = Rect::new(
                    card.cx - (card.w + card.slant) * 0.5,
                    card.cy - card.h * 0.5,
                    card.w + card.slant,
                    card.h,
                );
                let bounds_d2d = D2D_RECT_F {
                    left: bbox.x,
                    top: bbox.y,
                    right: bbox.x + bbox.w,
                    bottom: bbox.y + bbox.h,
                };

                let lp = D2D1_LAYER_PARAMETERS {
                    contentBounds: bounds_d2d,
                    geometricMask: std::mem::ManuallyDrop::new(Some(mask_geom)),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity: eff_alpha,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                };
                rt.PushLayer(&lp, None);
                let mask_guard = std::mem::ManuallyDrop::into_inner(lp.geometricMask);

                // Dark fallback card fill while async thumbnail decodes
                if let Ok(bg_brush) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 0.11,
                        g: 0.13,
                        b: 0.18,
                        a: 0.88,
                    },
                    None,
                ) {
                    rt.FillRectangle(&bounds_d2d, &bg_brush);
                }

                if let Some(item) = self.items.get(card.item_idx) {
                    if !async_icons || item.path.is_file() {
                        let spec = item.path.to_string_lossy();
                        let bmp_opt = if async_icons {
                            self.icons
                                .get_cached_or_queue(&rt, &spec, CAROUSEL_HD_BUCKET_PX, hwnd)
                        } else {
                            self.icons.get_or_load(&rt, &spec, CAROUSEL_HD_BUCKET_PX)
                        };
                        if let Some(bmp) = bmp_opt {
                            draw_bitmap_cover_aligned(&rt, &bmp, bbox, 0.5, 0.5);
                        }
                    }
                }

                rt.PopLayer();
                drop(mask_guard);

                // Crisp contrasting border on the active center parallelogram
                if card.focus > 0.15 {
                    let (ar, ag, ab) = self.accent;
                    if let Ok(border_brush) = rt.CreateSolidColorBrush(
                        &D2D1_COLOR_F {
                            r: ar,
                            g: ag,
                            b: ab,
                            a: (0.92 * card.focus * global_alpha).clamp(0.0, 1.0),
                        },
                        None,
                    ) {
                        rt.DrawGeometry(&path_geom, &border_brush, 2.2, None);
                    }
                } else if let Ok(sub_brush) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: (0.16 * eff_alpha).clamp(0.0, 1.0),
                    },
                    None,
                ) {
                    rt.DrawGeometry(&path_geom, &sub_brush, 1.0, None);
                }
            }

            // Minimal floating caption pill beneath the center card
            if let Some(item) = self.items.get(active_item) {
                let loading_tag = if self.loading_more {
                    "  (Loading more…)"
                } else {
                    ""
                };
                let caption = match self.source {
                    CarouselSource::Web => {
                        format!(
                            "🌐 {} ({}/{}){}   ·   Enter: Apply   ·   T: +Theme   ·   R: New   ·   W: Local   ·   E: WE Discover",
                            item.title,
                            active_item + 1,
                            self.items.len(),
                            loading_tag
                        )
                    }
                    CarouselSource::WallpaperEngine => {
                        format!(
                            "⚡ {} ({}/{}){}   ·   Enter: Subscribe & Apply   ·   O: Open in WE   ·   T: +Theme   ·   R: New   ·   W: Web 4K",
                            item.title,
                            active_item + 1,
                            self.items.len(),
                            loading_tag
                        )
                    }
                    CarouselSource::Local => {
                        let star = if item.is_current { "★ " } else { "" };
                        format!(
                            "{star}{}   ·   Enter: Apply   ·   T: +Theme   ·   O: WE UI   ·   W: Web 4K   ·   E: WE Discover",
                            item.title
                        )
                    }
                };
                let pill_w = (self.win_w_dip * 0.56).clamp(480.0, 860.0);
                let pill_h = 34.0f32;
                let pill_x = (self.win_w_dip - pill_w) * 0.5;
                let pill_y = self.win_h_dip * 0.855;
                let pill_rect = D2D_RECT_F {
                    left: pill_x,
                    top: pill_y,
                    right: pill_x + pill_w,
                    bottom: pill_y + pill_h,
                };
                let rr = D2D1_ROUNDED_RECT {
                    rect: pill_rect,
                    radiusX: 17.0,
                    radiusY: 17.0,
                };
                if let Ok(pill_bg) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 0.06,
                        g: 0.08,
                        b: 0.12,
                        a: 0.78 * global_alpha,
                    },
                    None,
                ) {
                    rt.FillRoundedRectangle(&rr, &pill_bg);
                }
                if let Ok(pill_border) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 0.16 * global_alpha,
                    },
                    None,
                ) {
                    rt.DrawRoundedRectangle(&rr, &pill_border, 1.0, None);
                }
                if let Ok(txt_brush) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 0.94,
                        g: 0.96,
                        b: 1.0,
                        a: 0.96 * global_alpha,
                    },
                    None,
                ) {
                    let wide: Vec<u16> = caption.encode_utf16().collect();
                    rt.DrawText(
                        &wide,
                        &self.text_fmt,
                        &pill_rect,
                        &txt_brush,
                        windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_CLIP,
                        windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }

            // Pre-warm 2 offscreen neighbor cards in the background so fast scrolling never stalls
            if async_icons && self.items.len() > 5 {
                let c_idx = self.target_pos.round() as i32;
                for extra in [-3, 3] {
                    let idx = wrap_index(c_idx + extra, self.items.len());
                    if let Some(it) = self.items.get(idx) {
                        if it.path.is_file() {
                            let spec = it.path.to_string_lossy();
                            let _ = self.icons.get_cached_or_queue(
                                &rt,
                                &spec,
                                CAROUSEL_HD_BUCKET_PX,
                                hwnd,
                            );
                        }
                    }
                }
            }

            rt.EndDraw(None, None)
                .map_err(|e| format!("carousel EndDraw failed: {e}"))?;
        }
        Ok(())
    }

    fn step_animation(&mut self) -> bool {
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_frame)
            .as_secs_f32()
            .clamp(0.001, 0.05);
        self.last_frame = now;
        let diff = self.target_pos - self.scroll_pos;
        let fading = self.opened_at.elapsed().as_secs_f32() < 0.16;
        if diff.abs() < 0.0015 {
            self.scroll_pos = self.target_pos;
            return fading;
        }
        // Frame-rate independent exponential smoothing (silky at 60/120/144/240Hz)
        let factor = 1.0 - (-16.0 * dt).exp();
        self.scroll_pos += diff * factor;
        true
    }

    fn maybe_request_more(&mut self) {
        if !matches!(
            self.source,
            CarouselSource::Web | CarouselSource::WallpaperEngine
        ) || self.loading_more
            || self.items.len() < 4
            || self.target_pos < 0.0
        {
            return;
        }
        let pos = wrap_index(self.target_pos.round() as i32, self.items.len());
        if pos + 4 >= self.items.len() {
            if let Some(tx) = &self.more_req_tx {
                self.loading_more = true;
                let _ = tx.send(());
            }
        }
    }
}

fn current_monitor_rect_and_dpi() -> (RECT, f32) {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let rect = if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            mi.rcMonitor
        } else {
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            }
        };
        let mut dpix = 96u32;
        let mut dpiy = 96u32;
        let scale = if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpix, &mut dpiy).is_ok()
            && dpix >= 48
        {
            dpix as f32 / 96.0
        } else {
            1.0
        };
        (rect, scale)
    }
}

fn build_carousel_state(
    items: Vec<CarouselItem>,
    accent: (f32, f32, f32),
    offscreen: bool,
    source: CarouselSource,
    _has_wallpaper_engine: bool,
) -> Result<CarouselState, String> {
    let (mon, dpi_scale) = current_monitor_rect_and_dpi();
    let mon_w = (mon.right - mon.left).max(800);
    let mon_h = (mon.bottom - mon.top).max(600);
    let win_w_phys = ((mon_w as f32 * 0.88).round() as i32).clamp(760, mon_w);
    let win_h_phys = ((mon_h as f32 * 0.56).round() as i32).clamp(380, mon_h);
    let win_x = mon.left + (mon_w - win_w_phys) / 2;
    let win_y = mon.top + (mon_h - win_h_phys) / 2;
    let win_w_dip = win_w_phys as f32 / dpi_scale;
    let win_h_dip = win_h_phys as f32 / dpi_scale;

    unsafe {
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
            .map_err(|e| format!("D2D1CreateFactory failed: {e}"))?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
            .map_err(|e| format!("DWriteCreateFactory failed: {e}"))?;
        let text_fmt = dwrite
            .CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                12.5,
                w!("en-us"),
            )
            .map_err(|e| format!("CreateTextFormat failed: {e}"))?;
        let _ = text_fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        let _ = text_fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

        let (dc_rt, surface) = if offscreen {
            let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0 * dpi_scale,
                dpiY: 96.0 * dpi_scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: Default::default(),
            };
            let rt = d2d
                .CreateDCRenderTarget(&rt_props)
                .map_err(|e| format!("CreateDCRenderTarget failed: {e}"))?;
            let surf = OffscreenSurface::new(win_w_phys, win_h_phys)?;
            (Some(rt), Some(surf))
        } else {
            (None, None)
        };
        let icons = IconCache::new()?;
        let now = Instant::now();

        Ok(CarouselState {
            items,
            accent,
            source,
            win_x,
            win_y,
            win_w_phys,
            win_h_phys,
            win_w_dip,
            win_h_dip,
            dpi_scale,
            scroll_pos: 0.0,
            target_pos: 0.0,
            opened_at: now,
            last_frame: now,
            needs_redraw: true,
            outcome: None,
            d2d,
            _dwrite: dwrite,
            text_fmt,
            hwnd_rt: None,
            dc_rt,
            surface,
            icons,
            last_cards: Vec::new(),
            more_req_tx: None,
            more_res_rx: None,
            thumb_ready_rx: None,
            loading_more: false,
            wheel_accum: 0,
        })
    }
}

unsafe extern "system" fn carousel_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CarouselState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let st = &mut *ptr;

    match msg {
        WM_ERASEBKGND => LRESULT(1),
        m if m == WM_APP_ICON_READY => {
            if let Some(hrt) = &st.hwnd_rt {
                let rt: ID2D1RenderTarget = hrt.clone().into();
                if st.icons.drain_async_ready(&rt) {
                    st.needs_redraw = true;
                }
            }
            LRESULT(0)
        }
        m if m == WM_APP_MORE_ITEMS => {
            st.loading_more = false;
            if let Some(rx) = &st.more_res_rx {
                while let Ok(batch) = rx.try_recv() {
                    if !batch.is_empty() {
                        let old_len = st.items.len();
                        let base_round = st.scroll_pos.round();
                        let cur_idx = wrap_index(base_round as i32, old_len);
                        let frac = st.scroll_pos - base_round;
                        let delta = st.target_pos - st.scroll_pos;
                        st.items.extend(batch);
                        if old_len > 0 {
                            st.scroll_pos = cur_idx as f32 + frac;
                            st.target_pos = st.scroll_pos + delta;
                        }
                        st.needs_redraw = true;
                    }
                }
            }
            LRESULT(0)
        }
        m if m == WM_APP_THUMB_READY => {
            if let Some(rx) = &st.thumb_ready_rx {
                while let Ok(paths) = rx.try_recv() {
                    for p in paths {
                        st.icons.invalidate_path(&p.to_string_lossy());
                    }
                    st.needs_redraw = true;
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wparam.0 as u32;
            match vk {
                0x1B => {
                    // Escape
                    st.outcome = Some(CarouselOutcome::Cancelled);
                    let _ = DestroyWindow(hwnd);
                }
                0x25 | 0x41 | 0x48 => {
                    // Left / A / H
                    st.target_pos -= 1.0;
                    st.last_frame = Instant::now();
                    st.needs_redraw = true;
                }
                0x27 | 0x44 | 0x4C => {
                    // Right / D / L
                    st.target_pos += 1.0;
                    st.last_frame = Instant::now();
                    st.needs_redraw = true;
                    st.maybe_request_more();
                }
                0x0D => {
                    // Enter (Shift+Enter -> ApplyWithTheme)
                    let idx = wrap_index(st.target_pos.round() as i32, st.items.len());
                    let shift = (GetKeyState(i32::from(VK_SHIFT.0)) as u16 & 0x8000) != 0;
                    st.outcome = Some(if shift {
                        CarouselOutcome::ApplyWithTheme(idx)
                    } else {
                        CarouselOutcome::Apply(idx)
                    });
                    let _ = DestroyWindow(hwnd);
                }
                0x54 => {
                    // 'T' -> Apply + Auto-Theme
                    let idx = wrap_index(st.target_pos.round() as i32, st.items.len());
                    st.outcome = Some(CarouselOutcome::ApplyWithTheme(idx));
                    let _ = DestroyWindow(hwnd);
                }
                0x52 if matches!(
                    st.source,
                    CarouselSource::Web | CarouselSource::WallpaperEngine
                ) =>
                {
                    // 'R' -> Load a fresh random Wallhaven / Steam Workshop batch
                    st.outcome = Some(CarouselOutcome::RefreshWeb);
                    let _ = DestroyWindow(hwnd);
                }
                0x57 => {
                    // 'W' -> Toggle Local <-> Wallhaven 4K Web wallpapers
                    st.outcome = Some(CarouselOutcome::ToggleSource);
                    let _ = DestroyWindow(hwnd);
                }
                0x45 => {
                    // 'E' -> Jump to Wallpaper Engine Steam Workshop Discovery (or back to Local)
                    st.outcome = Some(CarouselOutcome::SwitchToWallpaperEngine);
                    let _ = DestroyWindow(hwnd);
                }
                0x4F => {
                    // 'O' -> Open in Wallpaper Engine UI / Workshop page
                    let idx = wrap_index(st.target_pos.round() as i32, st.items.len());
                    st.outcome = Some(CarouselOutcome::OpenInWallpaperEngine(idx));
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as u16) as i16;
            st.wheel_accum += delta as i32;
            while st.wheel_accum.abs() >= 120 {
                if st.wheel_accum >= 120 {
                    st.wheel_accum -= 120;
                    st.target_pos -= 1.0;
                } else {
                    st.wheel_accum += 120;
                    st.target_pos += 1.0;
                    st.maybe_request_more();
                }
                st.last_frame = Instant::now();
                st.needs_redraw = true;
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x_phys = (lparam.0 & 0xFFFF) as i16 as f32;
            let y_phys = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
            let px = x_phys / st.dpi_scale;
            let py = y_phys / st.dpi_scale;

            let mut hit: Option<CardGeom> = None;
            for card in st.last_cards.iter().rev() {
                if point_in_parallelogram(px, py, card) {
                    hit = Some(*card);
                    break;
                }
            }
            if let Some(card) = hit {
                if card.rel_d.abs() < 0.45 {
                    st.outcome = Some(CarouselOutcome::Apply(card.item_idx));
                    let _ = DestroyWindow(hwnd);
                } else {
                    st.target_pos = (st.scroll_pos + card.rel_d).round();
                    st.last_frame = Instant::now();
                    st.needs_redraw = true;
                    if card.rel_d > 0.0 {
                        st.maybe_request_more();
                    }
                }
            } else {
                st.outcome = Some(CarouselOutcome::Cancelled);
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE && st.outcome.is_none() {
                st.outcome = Some(CarouselOutcome::Cancelled);
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if st.outcome.is_none() {
                st.outcome = Some(CarouselOutcome::Cancelled);
            }
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Runs the interactive hardware-accelerated VSync slanted-parallelogram wallpaper carousel.
pub fn run_wallpaper_carousel(
    items: Vec<CarouselItem>,
    accent: (f32, f32, f32),
    source: CarouselSource,
    has_wallpaper_engine: bool,
) -> Result<CarouselOutcome, String> {
    run_wallpaper_carousel_paginated(
        items,
        accent,
        source,
        has_wallpaper_engine,
        None::<fn() -> Vec<CarouselItem>>,
    )
}

/// Runs the carousel with an optional background infinite-scroll page loader (`load_more`).
pub fn run_wallpaper_carousel_paginated<F>(
    items: Vec<CarouselItem>,
    accent: (f32, f32, f32),
    source: CarouselSource,
    has_wallpaper_engine: bool,
    load_more: Option<F>,
) -> Result<CarouselOutcome, String>
where
    F: FnMut() -> Vec<CarouselItem> + Send + 'static,
{
    run_wallpaper_carousel_streaming(
        items,
        accent,
        source,
        has_wallpaper_engine,
        load_more,
        None::<fn(Box<dyn Fn(Vec<PathBuf>) -> bool + Send + Sync>)>,
    )
}

/// Runs the carousel with optional background infinite scrolling (`load_more`) AND async thumbnail streaming (`start_thumb_prefetch`).
pub fn run_wallpaper_carousel_streaming<F, P>(
    items: Vec<CarouselItem>,
    accent: (f32, f32, f32),
    source: CarouselSource,
    has_wallpaper_engine: bool,
    load_more: Option<F>,
    start_thumb_prefetch: Option<P>,
) -> Result<CarouselOutcome, String>
where
    F: FnMut() -> Vec<CarouselItem> + Send + 'static,
    P: FnOnce(Box<dyn Fn(Vec<PathBuf>) -> bool + Send + Sync>) + Send + 'static,
{
    if items.is_empty() {
        return Ok(CarouselOutcome::Cancelled);
    }

    let com_ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let res = (|| -> Result<CarouselOutcome, String> {
        let mut state = Box::new(build_carousel_state(
            items,
            accent,
            false,
            source,
            has_wallpaper_engine,
        )?);

        unsafe {
            let hmod = GetModuleHandleW(PCWSTR::null())
                .map_err(|e| format!("GetModuleHandleW failed: {e}"))?;
            let cls = w!("LiteRofiWallpaperCarousel");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(carousel_wnd_proc),
                hInstance: hmod.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: cls,
                ..Default::default()
            };
            let _ = RegisterClassExW(&wc);

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                cls,
                w!("LiteRofi Wallpapers"),
                WS_POPUP,
                state.win_x,
                state.win_y,
                state.win_w_phys,
                state.win_h_phys,
                None,
                None,
                hmod,
                Some((&mut *state) as *mut CarouselState as *const std::ffi::c_void),
            )
            .map_err(|e| format!("CreateWindowExW(carousel) failed: {e}"))?;

            // Enable full-window per-pixel DWM transparency backed by hardware GPU swapchain
            let margins = MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

            let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0 * state.dpi_scale,
                dpiY: 96.0 * state.dpi_scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: Default::default(),
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U {
                    width: state.win_w_phys.max(1) as u32,
                    height: state.win_h_phys.max(1) as u32,
                },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE, // Hardware VSync (native monitor Hz)
            };
            let hrt = state
                .d2d
                .CreateHwndRenderTarget(&rt_props, &hwnd_props)
                .map_err(|e| format!("CreateHwndRenderTarget failed: {e}"))?;
            state.hwnd_rt = Some(hrt);

            if let Some(mut loader) = load_more {
                let (req_tx, req_rx) = mpsc::channel::<()>();
                let (res_tx, res_rx) = mpsc::channel::<Vec<CarouselItem>>();
                state.more_req_tx = Some(req_tx);
                state.more_res_rx = Some(res_rx);
                let hwnd_raw = hwnd.0 as isize;
                std::thread::spawn(move || {
                    while req_rx.recv().is_ok() {
                        let batch = loader();
                        if res_tx.send(batch).is_err() {
                            break;
                        }
                        let _ = PostMessageW(
                            HWND(hwnd_raw as *mut _),
                            WM_APP_MORE_ITEMS,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                });
            }

            if let Some(prefetch) = start_thumb_prefetch {
                let (t_tx, t_rx) = mpsc::channel::<Vec<PathBuf>>();
                state.thumb_ready_rx = Some(t_rx);
                let hwnd_raw = hwnd.0 as isize;
                let notify: Box<dyn Fn(Vec<PathBuf>) -> bool + Send + Sync> =
                    Box::new(move |paths: Vec<PathBuf>| {
                        if paths.is_empty() {
                            return true;
                        }
                        if t_tx.send(paths).is_err() {
                            return false;
                        }
                        let _ = PostMessageW(
                            HWND(hwnd_raw as *mut _),
                            WM_APP_THUMB_READY,
                            WPARAM(0),
                            LPARAM(0),
                        );
                        true
                    });
                let _ = std::thread::Builder::new()
                    .name("literofi-thumb-prefetch".to_string())
                    .spawn(move || prefetch(notify));
            }

            let _ = state.paint(hwnd, true, false);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);

            // VSync-driven render loop when animating, zero-CPU GetMessageW sleep when idle
            let mut msg = MSG::default();
            'outer: loop {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        break 'outer;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                if state.outcome.is_some() {
                    break;
                }
                if state.is_animating() || state.needs_redraw {
                    let _ = state.step_animation();
                    let _ = state.paint(hwnd, true, false);
                } else {
                    if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        break;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            Ok(state.outcome.unwrap_or(CarouselOutcome::Cancelled))
        }
    })();

    if com_ok {
        unsafe { CoUninitialize() };
    }
    res
}

/// Renders a single crisp offline PNG frame of the slanted wallpaper carousel for visual verification.
pub fn dump_wallpaper_carousel_png(
    items: Vec<CarouselItem>,
    accent: (f32, f32, f32),
    out_png: &Path,
) -> Result<(i32, i32), String> {
    let com_ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let res = (|| -> Result<(i32, i32), String> {
        let mut state = build_carousel_state(items, accent, true, CarouselSource::Local, false)?;
        state.paint(HWND::default(), false, true)?;
        let surf = state
            .surface
            .as_ref()
            .ok_or_else(|| "missing offscreen surface".to_string())?;
        let w = surf.w as u32;
        let h = surf.h as u32;
        let byte_len = (w as usize) * (h as usize) * 4;
        // SAFETY: `surface.bits` points to a valid top-down 32bpp BGRA DIBSection of size `w * h * 4`.
        let pixels = unsafe { std::slice::from_raw_parts(surf.bits, byte_len) };

        if let Some(parent) = out_png.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        save_bgra_png(out_png, w, h, pixels)?;
        Ok((surf.w, surf.h))
    })();
    if com_ok {
        unsafe { CoUninitialize() };
    }
    res
}

fn save_bgra_png(path: &Path, w: u32, h: u32, bgra: &[u8]) -> Result<(), String> {
    let wpath = HSTRING::from(path.as_os_str());
    unsafe {
        let wic: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("WIC factory failed: {e}"))?;
        let stream = wic
            .CreateStream()
            .map_err(|e| format!("CreateStream failed: {e}"))?;
        stream
            .InitializeFromFilename(&wpath, GENERIC_WRITE.0)
            .map_err(|e| format!("InitializeFromFilename failed: {e}"))?;
        let encoder = wic
            .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
            .map_err(|e| format!("CreateEncoder failed: {e}"))?;
        encoder
            .Initialize(&stream, WICBitmapEncoderNoCache)
            .map_err(|e| format!("encoder.Initialize failed: {e}"))?;
        let mut frame = None;
        let mut props = None;
        encoder
            .CreateNewFrame(&mut frame, &mut props)
            .map_err(|e| format!("CreateNewFrame failed: {e}"))?;
        let frame = frame.ok_or_else(|| "CreateNewFrame returned None".to_string())?;
        frame
            .Initialize(props.as_ref())
            .map_err(|e| format!("frame.Initialize failed: {e}"))?;
        frame
            .SetSize(w, h)
            .map_err(|e| format!("frame.SetSize failed: {e}"))?;
        let mut fmt = GUID_WICPixelFormat32bppBGRA;
        frame
            .SetPixelFormat(&mut fmt)
            .map_err(|e| format!("frame.SetPixelFormat failed: {e}"))?;
        frame
            .WritePixels(h, w * 4, bgra)
            .map_err(|e| format!("frame.WritePixels failed: {e}"))?;
        frame
            .Commit()
            .map_err(|e| format!("frame.Commit failed: {e}"))?;
        encoder
            .Commit()
            .map_err(|e| format!("encoder.Commit failed: {e}"))?;
        Ok(())
    }
}
