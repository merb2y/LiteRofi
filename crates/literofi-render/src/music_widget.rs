//! Native Hardware-Accelerated Direct2D Compact Music Player (`Alt+M`),
//! inspired by `boubounokefalos/teerquaz-dotfiles`:
//! - Compact rounded dark card (~452x182 DIP) with a dynamic 1.5px accent border derived
//!   directly from the currently playing track's album artwork.
//! - Left: 118x118 rounded HD album artwork (`object-fit: cover`).
//! - Right: Track Title (bold), Artist, Album (accent-tinted), interactive live progress bar
//!   with `mm:ss` timestamps (click to seek), and compact media controls (`Prev`, filled accent
//!   `Play/Pause` circle, `Next`, `Stop`, `Refresh`).

use crate::layout::Rect;
use crate::win32_icons::{draw_bitmap_cover_aligned, IconCache, WM_APP_ICON_READY};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows::core::{w, HSTRING, PCWSTR};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::{GENERIC_WRITE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1Geometry, ID2D1HwndRenderTarget,
    ID2D1RenderTarget, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_CLIP,
    D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
    GetMonitorInfoW, MonitorFromPoint, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTOPRIMARY, PAINTSTRUCT,
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
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
    GetWindowLongPtrW, KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW,
    CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, IDC_ARROW, MSG, SW_SHOW, WA_INACTIVE, WM_ACTIVATE,
    WM_APP, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};

const WM_APP_MEDIA_READY: u32 = WM_APP + 55;
const TIMER_TICK_ID: usize = 2;
const WIN_W_DIP: f32 = 456.0;
const WIN_H_DIP: f32 = 186.0;

/// Track state rendered by the compact Direct2D Music Player.
#[derive(Debug, Clone)]
pub struct MusicCardData {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub cover_path: Option<PathBuf>,
    pub position_secs: u64,
    pub duration_secs: u64,
    pub volume: f32,
    pub muted: bool,
    pub bg: (f32, f32, f32),
    pub bg_alt: (f32, f32, f32),
    pub accent: (f32, f32, f32),
    pub font_family: String,
    pub corner_radius: f32,
}

/// Interactive command emitted by the Music Player widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MusicWidgetCommand {
    Previous,
    TogglePlayPause,
    Next,
    Stop,
    Refresh,
    Seek(u64),
    SetVolume(f32),
    ToggleMute,
}

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
                biHeight: -h,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum HitTarget {
    Close,
    Prev,
    PlayPause,
    Next,
    Mute,
    VolumeBar,
    ProgressBar,
}

struct WidgetLayout {
    close_btn: Rect,
    cover: Rect,
    title: Rect,
    artist: Rect,
    album: Rect,
    progress_track: Rect,
    progress_hit: Rect,
    time_left: Rect,
    time_right: Rect,
    btn_prev: Rect,
    btn_play: Rect,
    btn_next: Rect,
    btn_mute: Rect,
    vol_track: Rect,
    vol_hit: Rect,
}

fn compute_layout() -> WidgetLayout {
    let pad = 16.0;
    let header_h = 26.0;
    let cover_size = 122.0;
    let cover = Rect::new(pad, header_h + 10.0, cover_size, cover_size);
    let rx = cover.x + cover.w + 16.0;
    let rw = WIN_W_DIP - rx - pad;
    let vol_w = 88.0;
    let vol_x = rx + rw - vol_w;

    WidgetLayout {
        close_btn: Rect::new(WIN_W_DIP - 28.0, 5.0, 22.0, 20.0),
        cover,
        title: Rect::new(rx, cover.y + 1.0, rw, 22.0),
        artist: Rect::new(rx, cover.y + 23.0, rw, 18.0),
        album: Rect::new(rx, cover.y + 42.0, rw, 16.0),
        progress_track: Rect::new(rx, cover.y + 68.0, rw, 4.0),
        progress_hit: Rect::new(rx, cover.y + 60.0, rw, 20.0),
        time_left: Rect::new(rx, cover.y + 76.0, 48.0, 14.0),
        time_right: Rect::new(rx + rw - 48.0, cover.y + 76.0, 48.0, 14.0),
        btn_prev: Rect::new(rx + 2.0, cover.y + 92.0, 28.0, 28.0),
        btn_play: Rect::new(rx + 36.0, cover.y + 89.0, 34.0, 34.0),
        btn_next: Rect::new(rx + 76.0, cover.y + 92.0, 28.0, 28.0),
        btn_mute: Rect::new(vol_x - 28.0, cover.y + 93.0, 24.0, 26.0),
        vol_track: Rect::new(vol_x, cover.y + 104.0, vol_w, 4.0),
        vol_hit: Rect::new(vol_x - 4.0, cover.y + 94.0, vol_w + 8.0, 24.0),
    }
}

fn pt_in(x: f32, y: f32, r: Rect) -> bool {
    x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h
}

fn format_mmss(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
}

fn contrast_on(rgb: (f32, f32, f32)) -> D2D1_COLOR_F {
    let lum = 0.2126 * rgb.0 + 0.7152 * rgb.1 + 0.0722 * rgb.2;
    if lum > 0.42 {
        D2D1_COLOR_F {
            r: 0.07,
            g: 0.09,
            b: 0.12,
            a: 1.0,
        }
    } else {
        D2D1_COLOR_F {
            r: 0.98,
            g: 0.99,
            b: 1.0,
            a: 1.0,
        }
    }
}

struct MusicWidgetState {
    data: MusicCardData,
    layout: WidgetLayout,
    hover: Option<HitTarget>,
    dpi_scale: f32,
    win_x: i32,
    win_y: i32,
    win_w_phys: i32,
    win_h_phys: i32,
    opened_at: Instant,
    last_tick: Instant,
    tick_count: u32,
    d2d: ID2D1Factory,
    _dwrite: IDWriteFactory,
    fmt_header: IDWriteTextFormat,
    fmt_title: IDWriteTextFormat,
    fmt_artist: IDWriteTextFormat,
    fmt_album: IDWriteTextFormat,
    fmt_time_l: IDWriteTextFormat,
    fmt_time_r: IDWriteTextFormat,
    fmt_icon: IDWriteTextFormat,
    fmt_icon_sm: IDWriteTextFormat,
    hwnd_rt: Option<ID2D1HwndRenderTarget>,
    dc_rt: Option<ID2D1DCRenderTarget>,
    surface: Option<OffscreenSurface>,
    icons: IconCache,
    cmd_tx: Option<mpsc::Sender<MusicWidgetCommand>>,
    data_rx: Option<mpsc::Receiver<MusicCardData>>,
}

impl MusicWidgetState {
    fn hit_test(&self, x: f32, y: f32) -> Option<HitTarget> {
        let l = &self.layout;
        if pt_in(x, y, l.close_btn) {
            Some(HitTarget::Close)
        } else if pt_in(x, y, l.btn_play) {
            Some(HitTarget::PlayPause)
        } else if pt_in(x, y, l.btn_prev) {
            Some(HitTarget::Prev)
        } else if pt_in(x, y, l.btn_next) {
            Some(HitTarget::Next)
        } else if pt_in(x, y, l.btn_mute) {
            Some(HitTarget::Mute)
        } else if pt_in(x, y, l.vol_hit) {
            Some(HitTarget::VolumeBar)
        } else if pt_in(x, y, l.progress_hit) && self.data.duration_secs > 0 {
            Some(HitTarget::ProgressBar)
        } else {
            None
        }
    }

    fn send_cmd(&self, cmd: MusicWidgetCommand) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(cmd);
        }
    }

    fn paint(&mut self, hwnd: HWND, async_icons: bool) -> Result<(), String> {
        let l = &self.layout;
        let bg = self.data.bg;
        let bg_alt = self.data.bg_alt;
        let acc = self.data.accent;

        unsafe {
            let rt: ID2D1RenderTarget = if let Some(hrt) = &self.hwnd_rt {
                hrt.SetDpi(96.0 * self.dpi_scale, 96.0 * self.dpi_scale);
                hrt.clone().into()
            } else if let (Some(drt), Some(surf)) = (&self.dc_rt, &self.surface) {
                let bind = RECT {
                    left: 0,
                    top: 0,
                    right: surf.w,
                    bottom: surf.h,
                };
                drt.BindDC(surf.hdc, &bind)
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

            // Outer rounded dark card with dynamic cover-art accent border
            let card_rect = D2D_RECT_F {
                left: 2.0,
                top: 2.0,
                right: WIN_W_DIP - 2.0,
                bottom: WIN_H_DIP - 2.0,
            };
            let card_r = self.data.corner_radius.clamp(0.0, 24.0);
            let card_rr = D2D1_ROUNDED_RECT {
                rect: card_rect,
                radiusX: card_r,
                radiusY: card_r,
            };
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: bg.0,
                    g: bg.1,
                    b: bg.2,
                    a: 0.95,
                },
                None,
            ) {
                rt.FillRoundedRectangle(&card_rr, &b);
            }
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: acc.0,
                    g: acc.1,
                    b: acc.2,
                    a: 0.78,
                },
                None,
            ) {
                rt.DrawRoundedRectangle(&card_rr, &b, 1.5, None);
            }

            // Top subtle titlebar ("Music" + close X)
            let hdr_rect = D2D_RECT_F {
                left: 40.0,
                top: 5.0,
                right: WIN_W_DIP - 40.0,
                bottom: 24.0,
            };
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.78,
                    g: 0.82,
                    b: 0.88,
                    a: 0.85,
                },
                None,
            ) {
                let w_hdr: Vec<u16> = "Music".encode_utf16().collect();
                rt.DrawText(
                    &w_hdr,
                    &self.fmt_header,
                    &hdr_rect,
                    &b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            let close_d2d = d2d_rect(l.close_btn);
            let close_alpha = if self.hover == Some(HitTarget::Close) {
                0.95
            } else {
                0.55
            };
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.90,
                    g: 0.92,
                    b: 0.96,
                    a: close_alpha,
                },
                None,
            ) {
                let w_x: Vec<u16> = "×".encode_utf16().collect();
                rt.DrawText(
                    &w_x,
                    &self.fmt_header,
                    &close_d2d,
                    &b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // Left rounded square album artwork
            let cov_d2d = d2d_rect(l.cover);
            let cov_r = (self.data.corner_radius * 0.7).clamp(0.0, 10.0);
            let cov_rr = D2D1_ROUNDED_RECT {
                rect: cov_d2d,
                radiusX: cov_r,
                radiusY: cov_r,
            };
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: bg_alt.0,
                    g: bg_alt.1,
                    b: bg_alt.2,
                    a: 1.0,
                },
                None,
            ) {
                rt.FillRoundedRectangle(&cov_rr, &b);
            }
            if let Some(cover_path) = &self.data.cover_path {
                let spec = cover_path.to_string_lossy();
                let bmp_opt = if async_icons {
                    self.icons.get_cached_or_queue(&rt, &spec, 512, hwnd)
                } else {
                    self.icons.get_or_load(&rt, &spec, 512)
                };
                if let Some(bmp) = bmp_opt {
                    if let Ok(rr_geom) = self.d2d.CreateRoundedRectangleGeometry(&cov_rr) {
                        let mask: ID2D1Geometry = rr_geom.into();
                        let lp = D2D1_LAYER_PARAMETERS {
                            contentBounds: cov_d2d,
                            geometricMask: std::mem::ManuallyDrop::new(Some(mask)),
                            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                            maskTransform: Matrix3x2::identity(),
                            opacity: 1.0,
                            opacityBrush: std::mem::ManuallyDrop::new(None),
                            layerOptions: D2D1_LAYER_OPTIONS_NONE,
                        };
                        rt.PushLayer(&lp, None);
                        let guard = std::mem::ManuallyDrop::into_inner(lp.geometricMask);
                        draw_bitmap_cover_aligned(&rt, &bmp, l.cover, 0.5, 0.5);
                        rt.PopLayer();
                        drop(guard);
                    }
                }
            } else if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: acc.0,
                    g: acc.1,
                    b: acc.2,
                    a: 0.65,
                },
                None,
            ) {
                let w_note: Vec<u16> = "♪".encode_utf16().collect();
                rt.DrawText(
                    &w_note,
                    &self.fmt_title,
                    &cov_d2d,
                    &b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.12,
                },
                None,
            ) {
                rt.DrawRoundedRectangle(&cov_rr, &b, 1.0, None);
            }

            // Right side: Title, Artist, Album
            let title_text = if self.data.title.is_empty() {
                "Nothing Playing"
            } else {
                &self.data.title
            };
            let artist_text = if self.data.artist.is_empty() {
                "Open Spotify, YouTube Music or any player"
            } else {
                &self.data.artist
            };
            let album_text = if !self.data.album.is_empty() {
                self.data.album.as_str()
            } else if self.data.is_playing {
                "Now Playing"
            } else {
                "Paused"
            };

            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.97,
                    g: 0.98,
                    b: 1.0,
                    a: 1.0,
                },
                None,
            ) {
                let w_t: Vec<u16> = title_text.encode_utf16().collect();
                rt.DrawText(
                    &w_t,
                    &self.fmt_title,
                    &d2d_rect(l.title),
                    &b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.72,
                    g: 0.76,
                    b: 0.82,
                    a: 0.95,
                },
                None,
            ) {
                let w_a: Vec<u16> = artist_text.encode_utf16().collect();
                rt.DrawText(
                    &w_a,
                    &self.fmt_artist,
                    &d2d_rect(l.artist),
                    &b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: acc.0,
                    g: acc.1,
                    b: acc.2,
                    a: 0.95,
                },
                None,
            ) {
                let w_al: Vec<u16> = album_text.encode_utf16().collect();
                rt.DrawText(
                    &w_al,
                    &self.fmt_album,
                    &d2d_rect(l.album),
                    &b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // Progress bar track + filled accent portion + knob
            let frac = if self.data.duration_secs > 0 {
                (self.data.position_secs as f32 / self.data.duration_secs as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let track_rr = D2D1_ROUNDED_RECT {
                rect: d2d_rect(l.progress_track),
                radiusX: 2.0,
                radiusY: 2.0,
            };
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.16,
                },
                None,
            ) {
                rt.FillRoundedRectangle(&track_rr, &b);
            }
            let fill_w = (l.progress_track.w * frac).max(if frac > 0.0 { 4.0 } else { 0.0 });
            if fill_w > 0.5 {
                let fill_rr = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: l.progress_track.x,
                        top: l.progress_track.y,
                        right: l.progress_track.x + fill_w,
                        bottom: l.progress_track.y + l.progress_track.h,
                    },
                    radiusX: 2.0,
                    radiusY: 2.0,
                };
                if let Ok(b) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: acc.0,
                        g: acc.1,
                        b: acc.2,
                        a: 1.0,
                    },
                    None,
                ) {
                    rt.FillRoundedRectangle(&fill_rr, &b);
                    let knob_r = if self.hover == Some(HitTarget::ProgressBar) {
                        5.5
                    } else {
                        4.0
                    };
                    let knob = D2D1_ELLIPSE {
                        point: D2D_POINT_2F {
                            x: l.progress_track.x + fill_w,
                            y: l.progress_track.y + l.progress_track.h * 0.5,
                        },
                        radiusX: knob_r,
                        radiusY: knob_r,
                    };
                    rt.FillEllipse(&knob, &b);
                }
            }

            // Timestamps beneath progress bar
            if let Ok(b) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 0.65,
                    g: 0.69,
                    b: 0.76,
                    a: 0.90,
                },
                None,
            ) {
                let left_s = format_mmss(self.data.position_secs);
                let right_s = format_mmss(self.data.duration_secs);
                let w_l: Vec<u16> = left_s.encode_utf16().collect();
                let w_r: Vec<u16> = right_s.encode_utf16().collect();
                rt.DrawText(
                    &w_l,
                    &self.fmt_time_l,
                    &d2d_rect(l.time_left),
                    &b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                rt.DrawText(
                    &w_r,
                    &self.fmt_time_r,
                    &d2d_rect(l.time_right),
                    &b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // Transport buttons: Prev, filled accent Play/Pause circle, Next, Stop, Refresh
            let draw_small_btn = |r: Rect, glyph: &str, target: HitTarget, sm: bool| {
                let hovered = self.hover == Some(target);
                if hovered {
                    if let Ok(hb) = rt.CreateSolidColorBrush(
                        &D2D1_COLOR_F {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 0.12,
                        },
                        None,
                    ) {
                        let rr = D2D1_ROUNDED_RECT {
                            rect: d2d_rect(r),
                            radiusX: r.h * 0.5,
                            radiusY: r.h * 0.5,
                        };
                        rt.FillRoundedRectangle(&rr, &hb);
                    }
                }
                if let Ok(tb) = rt.CreateSolidColorBrush(
                    &D2D1_COLOR_F {
                        r: 0.90,
                        g: 0.93,
                        b: 0.98,
                        a: if hovered { 1.0 } else { 0.80 },
                    },
                    None,
                ) {
                    let wg: Vec<u16> = glyph.encode_utf16().collect();
                    let fmt = if sm {
                        &self.fmt_icon_sm
                    } else {
                        &self.fmt_icon
                    };
                    rt.DrawText(
                        &wg,
                        fmt,
                        &d2d_rect(r),
                        &tb,
                        D2D1_DRAW_TEXT_OPTIONS_CLIP,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            };

            draw_small_btn(l.btn_prev, "⏮", HitTarget::Prev, false);

            // Accent circular Play/Pause button
            let play_cx = l.btn_play.x + l.btn_play.w * 0.5;
            let play_cy = l.btn_play.y + l.btn_play.h * 0.5;
            let play_r = l.btn_play.w * 0.5
                + if self.hover == Some(HitTarget::PlayPause) {
                    1.0
                } else {
                    0.0
                };
            if let Ok(pb) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: acc.0,
                    g: acc.1,
                    b: acc.2,
                    a: 1.0,
                },
                None,
            ) {
                rt.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: D2D_POINT_2F {
                            x: play_cx,
                            y: play_cy,
                        },
                        radiusX: play_r,
                        radiusY: play_r,
                    },
                    &pb,
                );
            }
            if let Ok(fgb) = rt.CreateSolidColorBrush(&contrast_on(acc), None) {
                if self.data.is_playing {
                    // Pixel-perfect geometrically centered Pause bars (||)
                    let bar_w = 3.2;
                    let bar_h = 12.0;
                    let gap = 3.4;
                    let left_x = play_cx - gap * 0.5 - bar_w;
                    let right_x = play_cx + gap * 0.5;
                    let top_y = play_cy - bar_h * 0.5;
                    for bx in [left_x, right_x] {
                        rt.FillRoundedRectangle(
                            &D2D1_ROUNDED_RECT {
                                rect: D2D_RECT_F {
                                    left: bx,
                                    top: top_y,
                                    right: bx + bar_w,
                                    bottom: top_y + bar_h,
                                },
                                radiusX: 1.2,
                                radiusY: 1.2,
                            },
                            &fgb,
                        );
                    }
                } else if let Ok(geom) = self.d2d.CreatePathGeometry() {
                    // Pixel-perfect optically centered Play triangle (▶)
                    if let Ok(sink) = geom.Open() {
                        let ox = 1.2; // slight optical shift right so triangle looks dead-center
                        let tri_h = 12.4;
                        let tri_w = 10.6;
                        sink.BeginFigure(
                            D2D_POINT_2F {
                                x: play_cx - tri_w * 0.5 + ox,
                                y: play_cy - tri_h * 0.5,
                            },
                            D2D1_FIGURE_BEGIN_FILLED,
                        );
                        sink.AddLine(D2D_POINT_2F {
                            x: play_cx + tri_w * 0.5 + ox,
                            y: play_cy,
                        });
                        sink.AddLine(D2D_POINT_2F {
                            x: play_cx - tri_w * 0.5 + ox,
                            y: play_cy + tri_h * 0.5,
                        });
                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        if sink.Close().is_ok() {
                            rt.FillGeometry(&geom, &fgb, None);
                        }
                    }
                }
            }

            draw_small_btn(l.btn_next, "⏭", HitTarget::Next, false);

            // Right-aligned Mute button + interactive Volume Slider (click or mouse wheel)
            let mute_glyph = if self.data.muted || self.data.volume <= 0.005 {
                "🔇"
            } else {
                "🔊"
            };
            draw_small_btn(l.btn_mute, mute_glyph, HitTarget::Mute, true);

            let vol_rr = D2D1_ROUNDED_RECT {
                rect: d2d_rect(l.vol_track),
                radiusX: 2.0,
                radiusY: 2.0,
            };
            if let Ok(vb_bg) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.16,
                },
                None,
            ) {
                rt.FillRoundedRectangle(&vol_rr, &vb_bg);
            }
            let eff_vol = if self.data.muted {
                0.0
            } else {
                self.data.volume.clamp(0.0, 1.0)
            };
            let vol_fill_w = (l.vol_track.w * eff_vol).max(if eff_vol > 0.0 { 4.0 } else { 0.0 });
            if let Ok(vb_fg) = rt.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: acc.0,
                    g: acc.1,
                    b: acc.2,
                    a: if self.data.muted { 0.35 } else { 0.95 },
                },
                None,
            ) {
                if vol_fill_w > 0.5 {
                    rt.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left: l.vol_track.x,
                                top: l.vol_track.y,
                                right: l.vol_track.x + vol_fill_w,
                                bottom: l.vol_track.y + l.vol_track.h,
                            },
                            radiusX: 2.0,
                            radiusY: 2.0,
                        },
                        &vb_fg,
                    );
                }
                let knob_r = if self.hover == Some(HitTarget::VolumeBar) {
                    5.2
                } else {
                    3.8
                };
                rt.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: D2D_POINT_2F {
                            x: l.vol_track.x + vol_fill_w,
                            y: l.vol_track.y + l.vol_track.h * 0.5,
                        },
                        radiusX: knob_r,
                        radiusY: knob_r,
                    },
                    &vb_fg,
                );
            }

            rt.EndDraw(None, None)
                .map_err(|e| format!("music widget EndDraw failed: {e}"))?;
        }
        Ok(())
    }
}

fn d2d_rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
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

unsafe fn make_fmt(
    dw: &IDWriteFactory,
    font: PCWSTR,
    weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
    size: f32,
    align: windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT,
) -> Result<IDWriteTextFormat, String> {
    let f = dw
        .CreateTextFormat(
            font,
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("en-us"),
        )
        .map_err(|e| format!("CreateTextFormat failed: {e}"))?;
    let _ = f.SetTextAlignment(align);
    let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
    Ok(f)
}

fn build_widget_state(data: MusicCardData, offscreen: bool) -> Result<MusicWidgetState, String> {
    let (mon, dpi_scale) = current_monitor_rect_and_dpi();
    let win_w_phys = (WIN_W_DIP * dpi_scale).round() as i32;
    let win_h_phys = (WIN_H_DIP * dpi_scale).round() as i32;
    let mon_w = (mon.right - mon.left).max(640);
    let mon_h = (mon.bottom - mon.top).max(480);
    let win_x = mon.left + (mon_w - win_w_phys) / 2;
    let win_y = mon.top + (mon_h - win_h_phys) / 2;

    unsafe {
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
            .map_err(|e| format!("D2D1CreateFactory failed: {e}"))?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
            .map_err(|e| format!("DWriteCreateFactory failed: {e}"))?;

        let font_h = HSTRING::from(if data.font_family.trim().is_empty() {
            "Segoe UI"
        } else {
            data.font_family.trim()
        });
        let font_pcw = PCWSTR(font_h.as_ptr());

        let fmt_header = make_fmt(
            &dwrite,
            font_pcw,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            11.0,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        )?;
        let fmt_title = make_fmt(
            &dwrite,
            font_pcw,
            DWRITE_FONT_WEIGHT_BOLD,
            14.0,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        )?;
        let fmt_artist = make_fmt(
            &dwrite,
            font_pcw,
            DWRITE_FONT_WEIGHT_NORMAL,
            11.0,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        )?;
        let fmt_album = make_fmt(
            &dwrite,
            font_pcw,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            10.0,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        )?;
        let fmt_time_l = make_fmt(
            &dwrite,
            font_pcw,
            DWRITE_FONT_WEIGHT_NORMAL,
            9.5,
            DWRITE_TEXT_ALIGNMENT_LEADING,
        )?;
        let fmt_time_r = make_fmt(
            &dwrite,
            font_pcw,
            DWRITE_FONT_WEIGHT_NORMAL,
            9.5,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        )?;
        let fmt_icon = make_fmt(
            &dwrite,
            w!("Segoe UI Symbol"),
            DWRITE_FONT_WEIGHT_NORMAL,
            14.0,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        )?;
        let fmt_icon_sm = make_fmt(
            &dwrite,
            w!("Segoe UI Symbol"),
            DWRITE_FONT_WEIGHT_NORMAL,
            12.0,
            DWRITE_TEXT_ALIGNMENT_CENTER,
        )?;

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

        Ok(MusicWidgetState {
            data,
            layout: compute_layout(),
            hover: None,
            dpi_scale,
            win_x,
            win_y,
            win_w_phys,
            win_h_phys,
            opened_at: Instant::now(),
            last_tick: Instant::now(),
            tick_count: 0,
            d2d,
            _dwrite: dwrite,
            fmt_header,
            fmt_title,
            fmt_artist,
            fmt_album,
            fmt_time_l,
            fmt_time_r,
            fmt_icon,
            fmt_icon_sm,
            hwnd_rt: None,
            dc_rt,
            surface,
            icons,
            cmd_tx: None,
            data_rx: None,
        })
    }
}

unsafe extern "system" fn music_wnd_proc(
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

    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut MusicWidgetState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let st = &mut *ptr;

    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            let _ = st.paint(hwnd, true);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        m if m == WM_APP_ICON_READY => {
            if let Some(hrt) = &st.hwnd_rt {
                let rt: ID2D1RenderTarget = hrt.clone().into();
                if st.icons.drain_async_ready(&rt) {
                    let _ = st.paint(hwnd, true);
                }
            }
            LRESULT(0)
        }
        m if m == WM_APP_MEDIA_READY => {
            let mut updated = false;
            if let Some(rx) = &st.data_rx {
                while let Ok(new_data) = rx.try_recv() {
                    st.data = new_data;
                    st.last_tick = Instant::now();
                    updated = true;
                }
            }
            if updated {
                let _ = st.paint(hwnd, true);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_TICK_ID => {
            st.tick_count = st.tick_count.wrapping_add(1);
            if st.data.is_playing
                && st.data.duration_secs > 0
                && st.data.position_secs < st.data.duration_secs
                && st.last_tick.elapsed() >= Duration::from_millis(950)
            {
                st.data.position_secs += 1;
                st.last_tick = Instant::now();
                let _ = st.paint(hwnd, true);
            }
            if st.tick_count.is_multiple_of(3) {
                st.send_cmd(MusicWidgetCommand::Refresh);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as i16 as f32 / st.dpi_scale;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / st.dpi_scale;
            let next_hover = st.hit_test(x, y);
            if next_hover != st.hover {
                st.hover = next_hover;
                let _ = st.paint(hwnd, true);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as f32 / st.dpi_scale;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / st.dpi_scale;
            match st.hit_test(x, y) {
                Some(HitTarget::Close) => {
                    let _ = DestroyWindow(hwnd);
                }
                Some(HitTarget::Prev) => st.send_cmd(MusicWidgetCommand::Previous),
                Some(HitTarget::PlayPause) => {
                    st.data.is_playing = !st.data.is_playing;
                    let _ = st.paint(hwnd, true);
                    st.send_cmd(MusicWidgetCommand::TogglePlayPause);
                }
                Some(HitTarget::Next) => st.send_cmd(MusicWidgetCommand::Next),
                Some(HitTarget::Mute) => {
                    st.data.muted = !st.data.muted;
                    let _ = st.paint(hwnd, true);
                    st.send_cmd(MusicWidgetCommand::ToggleMute);
                }
                Some(HitTarget::VolumeBar) => {
                    let frac =
                        ((x - st.layout.vol_track.x) / st.layout.vol_track.w).clamp(0.0, 1.0);
                    st.data.volume = frac;
                    st.data.muted = false;
                    let _ = st.paint(hwnd, true);
                    st.send_cmd(MusicWidgetCommand::SetVolume(frac));
                }
                Some(HitTarget::ProgressBar) => {
                    let frac = ((x - st.layout.progress_track.x) / st.layout.progress_track.w)
                        .clamp(0.0, 1.0);
                    let target = (frac * st.data.duration_secs as f32).round() as u64;
                    st.data.position_secs = target;
                    st.last_tick = Instant::now();
                    let _ = st.paint(hwnd, true);
                    st.send_cmd(MusicWidgetCommand::Seek(target));
                }
                None => {}
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) as u16) as i16;
            let step = if delta > 0 { 0.04 } else { -0.04 };
            let next_vol = (st.data.volume + step).clamp(0.0, 1.0);
            st.data.volume = next_vol;
            st.data.muted = false;
            let _ = st.paint(hwnd, true);
            st.send_cmd(MusicWidgetCommand::SetVolume(next_vol));
            LRESULT(0)
        }
        WM_KEYDOWN => {
            match wparam.0 as u32 {
                0x1B => {
                    let _ = DestroyWindow(hwnd);
                }
                0x20 | 0x0D => {
                    // Space / Enter -> Toggle Play/Pause
                    st.data.is_playing = !st.data.is_playing;
                    let _ = st.paint(hwnd, true);
                    st.send_cmd(MusicWidgetCommand::TogglePlayPause);
                }
                0x25 | 0x41 => st.send_cmd(MusicWidgetCommand::Previous),
                0x27 | 0x44 => st.send_cmd(MusicWidgetCommand::Next),
                0x52 => st.send_cmd(MusicWidgetCommand::Refresh),
                _ => {}
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE
                && st.opened_at.elapsed() > Duration::from_millis(250)
            {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(hwnd, TIMER_TICK_ID);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Runs the interactive compact `teerquaz`-style Direct2D Music Player (`Alt+M`).
pub fn run_music_widget<F>(initial: MusicCardData, mut handler: F) -> Result<(), String>
where
    F: FnMut(MusicWidgetCommand) -> MusicCardData + Send + 'static,
{
    let com_ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let res = (|| -> Result<(), String> {
        let mut state = Box::new(build_widget_state(initial, false)?);
        let (cmd_tx, cmd_rx) = mpsc::channel::<MusicWidgetCommand>();
        let (data_tx, data_rx) = mpsc::channel::<MusicCardData>();
        state.cmd_tx = Some(cmd_tx);
        state.data_rx = Some(data_rx);

        unsafe {
            let hmod = GetModuleHandleW(PCWSTR::null())
                .map_err(|e| format!("GetModuleHandleW failed: {e}"))?;
            let cls = w!("LiteRofiMusicWidget");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(music_wnd_proc),
                hInstance: hmod.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: cls,
                ..Default::default()
            };
            let _ = RegisterClassExW(&wc);

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                cls,
                w!("LiteRofi Music"),
                WS_POPUP,
                state.win_x,
                state.win_y,
                state.win_w_phys,
                state.win_h_phys,
                None,
                None,
                hmod,
                Some((&mut *state) as *mut MusicWidgetState as *const std::ffi::c_void),
            )
            .map_err(|e| format!("CreateWindowExW(music) failed: {e}"))?;

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
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let hrt = state
                .d2d
                .CreateHwndRenderTarget(&rt_props, &hwnd_props)
                .map_err(|e| format!("CreateHwndRenderTarget(music) failed: {e}"))?;
            state.hwnd_rt = Some(hrt);

            // Spawn background SMTC command/refresh worker so UI never blocks
            let hwnd_raw = hwnd.0 as isize;
            let _worker = std::thread::spawn(move || {
                while let Ok(cmd) = cmd_rx.recv() {
                    let updated = handler(cmd);
                    if data_tx.send(updated).is_err() {
                        break;
                    }
                    let _ = PostMessageW(
                        HWND(hwnd_raw as *mut _),
                        WM_APP_MEDIA_READY,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            });

            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            let _ = state.paint(hwnd, true);
            // Trigger immediate async SMTC refresh right after window appears (0ms open!)
            state.send_cmd(MusicWidgetCommand::Refresh);
            let _ = SetTimer(hwnd, TIMER_TICK_ID, 1000, None);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            Ok(())
        }
    })();

    if com_ok {
        unsafe { CoUninitialize() };
    }
    res
}

/// Renders an offline PNG of the compact Music Player for visual verification.
pub fn dump_music_widget_png(data: MusicCardData, out_png: &Path) -> Result<(i32, i32), String> {
    let com_ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let res = (|| -> Result<(i32, i32), String> {
        let mut state = build_widget_state(data, true)?;
        state.paint(HWND::default(), false)?;
        let surf = state
            .surface
            .as_ref()
            .ok_or_else(|| "missing offscreen surface".to_string())?;
        let w = surf.w as u32;
        let h = surf.h as u32;
        let pixels = unsafe { std::slice::from_raw_parts(surf.bits, (w * h * 4) as usize) };
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
