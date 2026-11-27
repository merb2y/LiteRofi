//! Native Hardware-Accelerated Direct2D Game Launcher (`Alt+G`),
//! combining `Rolv-Apneseth/rofi-games` functionality (multi-launcher discovery,
//! 2:3 box-art shelf, recency sorting, `Shift+Enter` library/folder, `Shift+Del` reset,
//! live search) with a Hallmark Editorial Cinema Shelf layout:
//! - Single unified rounded surface (no card-inside-card, no tracked-caps labels,
//!   no middle-dot metadata joins, no emoji icons, no invented metrics).
//! - Top Hero Stage: edge-to-edge hero artwork of the focused game, official transparent
//!   logo (or single-weight display title), plain platform & real last-played text,
//!   and crisp primary (`Play`) + secondary (`Library` / `Folder`) actions.
//! - Mid hairline search & status strip with instant keyboard filtering.
//! - Bottom 2:3 box-art poster shelf with smooth 60/120Hz GPU scrolling.

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
    D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1Geometry, ID2D1HwndRenderTarget,
    ID2D1RenderTarget, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_GAMMA_2_2, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_FAR,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP,
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
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
    GetWindowLongPtrW, KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW,
    CS_DBLCLKS, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, IDC_ARROW, MSG, SW_SHOW, WA_INACTIVE,
    WM_ACTIVATE, WM_APP, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_PAINT, WM_TIMER,
    WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const WM_APP_GAMES_READY: u32 = WM_APP + 61;

const WIN_W_DIP: f32 = 1040.0;
const WIN_H_DIP: f32 = 576.0;
const WIN_RADIUS: f32 = 18.0;
const HERO_H: f32 = 286.0;
const BAR_H: f32 = 46.0;
const SHELF_Y: f32 = HERO_H + BAR_H;
const PAD_X: f32 = 39.0;
const CARD_W: f32 = 122.0;
const CARD_H: f32 = 180.0;
const CARD_GAP: f32 = 18.0;
const CARD_STRIDE: f32 = CARD_W + CARD_GAP;
const CARD_RADIUS: f32 = 10.0;
const ANIM_TIMER_ID: usize = 3;

/// Single game entry displayed by the Direct2D Game Launcher.
#[derive(Debug, Clone)]
pub struct GameCardItem {
    pub key: String,
    pub name: String,
    pub source: String,
    pub launch_uri: String,
    pub library_uri: Option<String>,
    pub folder_path: Option<PathBuf>,
    pub cover_path: Option<PathBuf>,
    pub hero_path: Option<PathBuf>,
    pub logo_path: Option<PathBuf>,
    pub last_played: Option<String>,
}

/// Declared color & typography tokens for the Game Launcher (derived from active theme or `widgets.toml`).
#[derive(Debug, Clone)]
pub struct GamesPalette {
    pub ground: (f32, f32, f32),
    pub surface: (f32, f32, f32),
    pub ink: (f32, f32, f32),
    pub ink_muted: (f32, f32, f32),
    pub accent: (f32, f32, f32),
    pub accent_ink: (f32, f32, f32),
    pub font_family: String,
    pub corner_radius: f32,
}

impl Default for GamesPalette {
    fn default() -> Self {
        Self {
            ground: (0.094, 0.102, 0.141),
            surface: (0.141, 0.157, 0.216),
            ink: (0.933, 0.945, 0.969),
            ink_muted: (0.635, 0.671, 0.753),
            accent: (0.949, 0.957, 0.973),
            accent_ink: (0.075, 0.086, 0.122),
            font_family: "Segoe UI".to_string(),
            corner_radius: WIN_RADIUS,
        }
    }
}

/// Outcome returned when the user acts inside the Game Launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GamesWidgetOutcome {
    Cancelled,
    Launch { key: String, uri: String },
    OpenLibrary { key: String, uri: String },
    OpenFolder { key: String, path: PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoverTarget {
    None,
    PlayBtn,
    SecondaryBtn,
    FolderBtn,
    Card(usize),
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
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
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

    fn save_png(&self, path: &Path) -> Result<(), String> {
        unsafe {
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| format!("WIC factory: {e}"))?;
            let stream = factory
                .CreateStream()
                .map_err(|e| format!("WIC stream: {e}"))?;
            let wide = HSTRING::from(path.as_os_str());
            stream
                .InitializeFromFilename(&wide, GENERIC_WRITE.0)
                .map_err(|e| format!("WIC file: {e}"))?;
            let encoder = factory
                .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
                .map_err(|e| format!("WIC encoder: {e}"))?;
            encoder
                .Initialize(&stream, WICBitmapEncoderNoCache)
                .map_err(|e| format!("WIC init: {e}"))?;
            let mut frame = None;
            encoder
                .CreateNewFrame(&mut frame, std::ptr::null_mut())
                .map_err(|e| format!("WIC frame: {e}"))?;
            let frame = frame.ok_or("WIC frame missing")?;
            frame
                .Initialize(None)
                .map_err(|e| format!("WIC frame init: {e}"))?;
            frame
                .SetSize(self.w as u32, self.h as u32)
                .map_err(|e| format!("WIC size: {e}"))?;
            let mut fmt = GUID_WICPixelFormat32bppBGRA;
            frame
                .SetPixelFormat(&mut fmt)
                .map_err(|e| format!("WIC fmt: {e}"))?;
            let stride = (self.w * 4) as u32;
            let bytes = std::slice::from_raw_parts(self.bits, (stride * self.h as u32) as usize);
            frame
                .WritePixels(self.h as u32, stride, bytes)
                .map_err(|e| format!("WIC write: {e}"))?;
            frame.Commit().map_err(|e| format!("WIC commit: {e}"))?;
            encoder.Commit().map_err(|e| format!("WIC commit: {e}"))?;
        }
        Ok(())
    }
}

impl Drop for OffscreenSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.hdc.is_invalid() {
                let _ = SelectObject(self.hdc, self.prev);
                let _ = DeleteObject(HGDIOBJ(self.hbmp.0));
                let _ = DeleteDC(self.hdc);
            }
        }
    }
}

type ResetAccessCallback = Box<dyn FnMut(&str)>;

struct GamesWidgetState {
    hwnd: HWND,
    dpi_scale: f32,
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    rt: Option<ID2D1HwndRenderTarget>,
    icons: IconCache,
    palette: GamesPalette,
    all_games: Vec<GameCardItem>,
    filtered: Vec<usize>,
    selected: usize,
    query: String,
    scroll_x: f32,
    target_scroll_x: f32,
    animating: bool,
    last_frame: Instant,
    hover: HoverTarget,
    opened_at: Instant,
    outcome: GamesWidgetOutcome,
    on_reset_access: Option<ResetAccessCallback>,
    refresh_rx: Option<mpsc::Receiver<Vec<GameCardItem>>>,
}

impl GamesWidgetState {
    fn focused_game(&self) -> Option<&GameCardItem> {
        let idx = *self.filtered.get(self.selected)?;
        self.all_games.get(idx)
    }

    fn rebuild_filter(&mut self, keep_key: Option<&str>) {
        let q = self.query.trim().to_ascii_lowercase();
        self.filtered.clear();
        for (i, g) in self.all_games.iter().enumerate() {
            if q.is_empty()
                || g.name.to_ascii_lowercase().contains(&q)
                || g.source.to_ascii_lowercase().contains(&q)
            {
                self.filtered.push(i);
            }
        }
        if let Some(k) = keep_key {
            if let Some(pos) = self
                .filtered
                .iter()
                .position(|&i| self.all_games[i].key == k)
            {
                self.selected = pos;
            } else {
                self.selected = 0;
            }
        } else if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
        self.update_target_scroll();
        self.scroll_x = self.target_scroll_x;
        self.prewarm_visible();
    }

    fn shelf_viewport_w(&self) -> f32 {
        WIN_W_DIP - PAD_X * 2.0
    }

    fn max_scroll_x(&self) -> f32 {
        let count = self.filtered.len();
        if count == 0 {
            return 0.0;
        }
        let total = count as f32 * CARD_W + count.saturating_sub(1) as f32 * CARD_GAP;
        (total - self.shelf_viewport_w()).max(0.0)
    }

    fn update_target_scroll(&mut self) {
        if self.filtered.is_empty() {
            self.target_scroll_x = 0.0;
            return;
        }
        let card_left = self.selected as f32 * CARD_STRIDE;
        let card_right = card_left + CARD_W;
        let vp = self.shelf_viewport_w();
        let mut target = self.target_scroll_x;
        let pad = 24.0;
        if card_left < target + pad {
            target = (card_left - pad).max(0.0);
        } else if card_right > target + vp - pad {
            target = (card_right - vp + pad).max(0.0);
        }
        self.target_scroll_x = target.clamp(0.0, self.max_scroll_x());
    }

    fn ensure_anim_timer(&mut self) {
        if !self.animating {
            self.animating = true;
            self.last_frame = Instant::now();
            unsafe {
                let _ = SetTimer(self.hwnd, ANIM_TIMER_ID, 8, None);
            }
        }
    }

    fn step_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let max_idx = self.filtered.len() - 1;
        let next = (self.selected as isize + delta).clamp(0, max_idx as isize) as usize;
        if next != self.selected {
            self.selected = next;
            self.update_target_scroll();
            self.prewarm_visible();
            self.ensure_anim_timer();
            self.paint();
        }
    }

    fn select_index(&mut self, idx: usize) {
        if idx < self.filtered.len() {
            self.selected = idx;
            self.update_target_scroll();
            self.prewarm_visible();
            self.ensure_anim_timer();
            self.paint();
        }
    }

    fn prewarm_visible(&mut self) {
        let Some(rt_hwnd) = self.rt.clone() else {
            return;
        };
        let rt: ID2D1RenderTarget = rt_hwnd.into();
        if let Some(g) = self.focused_game().cloned() {
            if let Some(p) = &g.hero_path {
                let _ = self.icons.get_or_load(&rt, &p.to_string_lossy(), 1024);
            }
            if let Some(p) = &g.logo_path {
                let _ = self.icons.get_or_load(&rt, &p.to_string_lossy(), 512);
            }
            if let Some(p) = &g.cover_path {
                let _ = self.icons.get_or_load(&rt, &p.to_string_lossy(), 512);
            }
        }
        let start = self.selected.saturating_sub(4);
        let end = (self.selected + 7).min(self.filtered.len());
        for pos in start..end {
            let idx = self.filtered[pos];
            if let Some(p) = &self.all_games[idx].cover_path {
                let _ = self.icons.get_or_load(&rt, &p.to_string_lossy(), 256);
            }
        }
    }

    fn play_btn_rect(&self) -> Rect {
        Rect {
            x: WIN_W_DIP - PAD_X - 116.0,
            y: HERO_H - 58.0,
            w: 116.0,
            h: 38.0,
        }
    }

    fn secondary_btn_rect(&self) -> Option<Rect> {
        let g = self.focused_game()?;
        if g.library_uri.is_some() {
            Some(Rect {
                x: self.play_btn_rect().x - 10.0 - 94.0,
                y: HERO_H - 58.0,
                w: 94.0,
                h: 38.0,
            })
        } else {
            None
        }
    }

    fn folder_btn_rect(&self) -> Option<Rect> {
        let g = self.focused_game()?;
        g.folder_path.as_ref()?;
        let right_edge = self
            .secondary_btn_rect()
            .map_or(self.play_btn_rect().x, |r| r.x);
        Some(Rect {
            x: right_edge - 10.0 - 86.0,
            y: HERO_H - 58.0,
            w: 86.0,
            h: 38.0,
        })
    }

    fn card_rect(&self, visible_pos: usize) -> Rect {
        let lift = if visible_pos == self.selected {
            6.0
        } else {
            0.0
        };
        Rect {
            x: PAD_X + visible_pos as f32 * CARD_STRIDE - self.scroll_x,
            y: SHELF_Y + 20.0 - lift,
            w: CARD_W,
            h: CARD_H,
        }
    }

    fn hit_test(&self, x: f32, y: f32) -> HoverTarget {
        if self.focused_game().is_some() {
            if point_in_rect(x, y, self.play_btn_rect()) {
                return HoverTarget::PlayBtn;
            }
            if let Some(r) = self.secondary_btn_rect() {
                if point_in_rect(x, y, r) {
                    return HoverTarget::SecondaryBtn;
                }
            }
            if let Some(r) = self.folder_btn_rect() {
                if point_in_rect(x, y, r) {
                    return HoverTarget::FolderBtn;
                }
            }
        }
        if y >= SHELF_Y && (PAD_X..=WIN_W_DIP - PAD_X).contains(&x) {
            for pos in 0..self.filtered.len() {
                let mut r = self.card_rect(pos);
                r.h += 28.0; // include title below poster
                if point_in_rect(x, y, r) {
                    return HoverTarget::Card(pos);
                }
            }
        }
        HoverTarget::None
    }

    fn trigger_launch(&mut self) {
        if let Some(g) = self.focused_game().cloned() {
            self.outcome = GamesWidgetOutcome::Launch {
                key: g.key,
                uri: g.launch_uri,
            };
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }

    fn trigger_secondary(&mut self) {
        if let Some(g) = self.focused_game().cloned() {
            if let Some(lib_uri) = g.library_uri {
                self.outcome = GamesWidgetOutcome::OpenLibrary {
                    key: g.key,
                    uri: lib_uri,
                };
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
            } else if let Some(path) = g.folder_path {
                self.outcome = GamesWidgetOutcome::OpenFolder { key: g.key, path };
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
            }
        }
    }

    fn trigger_folder(&mut self) {
        if let Some(g) = self.focused_game().cloned() {
            if let Some(path) = g.folder_path {
                self.outcome = GamesWidgetOutcome::OpenFolder { key: g.key, path };
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
            }
        }
    }

    fn trigger_reset_access(&mut self) {
        let Some(&real_idx) = self.filtered.get(self.selected) else {
            return;
        };
        let key = self.all_games[real_idx].key.clone();
        if let Some(cb) = self.on_reset_access.as_mut() {
            cb(&key);
        }
        self.all_games[real_idx].last_played = None;
        self.paint();
    }

    fn paint(&mut self) {
        let Some(rt_hwnd) = self.rt.clone() else {
            return;
        };
        let rt: ID2D1RenderTarget = rt_hwnd.into();
        let d2d = self.d2d.clone();
        let dwrite = self.dwrite.clone();
        let palette = self.palette.clone();
        let query = self.query.clone();
        let total = self.all_games.len();
        let filtered_len = self.filtered.len();
        let selected = self.selected;
        let scroll_x = self.scroll_x;
        let hover = self.hover;
        let play_rect = self.play_btn_rect();
        let sec_rect = self.secondary_btn_rect();
        let folder_rect = self.folder_btn_rect();
        let focused = self.focused_game().cloned();
        let visible_cards: Vec<(usize, GameCardItem)> = self
            .filtered
            .iter()
            .enumerate()
            .map(|(pos, &idx)| (pos, self.all_games[idx].clone()))
            .collect();

        unsafe {
            rt.BeginDraw();
            draw_games_scene(
                &rt,
                &d2d,
                &dwrite,
                &mut self.icons,
                &palette,
                focused.as_ref(),
                &visible_cards,
                selected,
                scroll_x,
                &query,
                total,
                filtered_len,
                hover,
                play_rect,
                sec_rect,
                folder_rect,
                false,
            );
            let _ = rt.EndDraw(None, None);
        }
    }
}

fn color_f(rgb: (f32, f32, f32), a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: rgb.0,
        g: rgb.1,
        b: rgb.2,
        a,
    }
}

fn point_in_rect(x: f32, y: f32, r: Rect) -> bool {
    x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_games_scene(
    rt: &ID2D1RenderTarget,
    d2d: &ID2D1Factory,
    dwrite: &IDWriteFactory,
    icons: &mut IconCache,
    palette: &GamesPalette,
    focused: Option<&GameCardItem>,
    visible_cards: &[(usize, GameCardItem)],
    selected: usize,
    scroll_x: f32,
    query: &str,
    total_games: usize,
    filtered_len: usize,
    hover: HoverTarget,
    play_rect: Rect,
    sec_rect: Option<Rect>,
    folder_rect: Option<Rect>,
    _sync_load: bool,
) {
    rt.Clear(Some(&D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }));
    rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);

    let font_name = if palette.font_family.trim().is_empty() {
        "Segoe UI"
    } else {
        palette.font_family.trim()
    };
    let win_radius = palette.corner_radius.clamp(0.0, 32.0);
    let full_rr = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: 1.0,
            top: 1.0,
            right: WIN_W_DIP - 1.0,
            bottom: WIN_H_DIP - 1.0,
        },
        radiusX: win_radius,
        radiusY: win_radius,
    };

    // Clip entire window to single rounded surface
    let clip_geom = d2d.CreateRoundedRectangleGeometry(&full_rr).ok();
    let clipped = if let Some(g) = &clip_geom {
        let mask: ID2D1Geometry = g.clone().into();
        let mut params = D2D1_LAYER_PARAMETERS {
            contentBounds: full_rr.rect,
            geometricMask: std::mem::ManuallyDrop::new(Some(mask)),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity: 1.0,
            opacityBrush: std::mem::ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS_NONE,
        };
        rt.PushLayer(&params, None);
        std::mem::ManuallyDrop::drop(&mut params.geometricMask);
        true
    } else {
        false
    };

    // Base ground fill
    if let Ok(ground_brush) = rt.CreateSolidColorBrush(&color_f(palette.ground, 0.98), None) {
        rt.FillRectangle(&full_rr.rect, &ground_brush);
    }

    // 1. HERO STAGE (0..HERO_H)
    let hero_rect = Rect {
        x: 0.0,
        y: 0.0,
        w: WIN_W_DIP,
        h: HERO_H,
    };
    let hero_d2d = D2D_RECT_F {
        left: 0.0,
        top: 0.0,
        right: WIN_W_DIP,
        bottom: HERO_H,
    };

    let mut drew_hero_art = false;
    if let Some(game) = focused {
        let art_path = game.hero_path.as_ref().or(game.cover_path.as_ref());
        if let Some(path) = art_path {
            let key = path.to_string_lossy();
            let bmp = icons.get_or_load(rt, &key, 1920);
            if let Some(bmp) = bmp {
                draw_bitmap_cover_aligned(rt, &bmp, hero_rect, 0.5, 0.32);
                drew_hero_art = true;
            }
        }
    }
    if !drew_hero_art {
        if let Ok(surf) = rt.CreateSolidColorBrush(&color_f(palette.surface, 1.0), None) {
            rt.FillRectangle(&hero_d2d, &surf);
        }
    }

    // Seamless vertical scrim: reaches full ground opacity well before HERO_H so there is zero seam
    let stops = [
        D2D1_GRADIENT_STOP {
            position: 0.0,
            color: color_f(palette.ground, if drew_hero_art { 0.10 } else { 0.04 }),
        },
        D2D1_GRADIENT_STOP {
            position: 0.44,
            color: color_f(palette.ground, if drew_hero_art { 0.48 } else { 0.36 }),
        },
        D2D1_GRADIENT_STOP {
            position: 0.78,
            color: color_f(palette.ground, 0.95),
        },
        D2D1_GRADIENT_STOP {
            position: 1.0,
            color: color_f(palette.ground, 1.0),
        },
    ];
    if let Ok(stop_col) =
        rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
    {
        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: D2D_POINT_2F { x: 0.0, y: 0.0 },
            endPoint: D2D_POINT_2F { x: 0.0, y: HERO_H },
        };
        if let Ok(grad) = rt.CreateLinearGradientBrush(&props, None, &stop_col) {
            rt.FillRectangle(&hero_d2d, &grad);
        }
    }

    // Left readability vignette for title/logo
    let h_stops = [
        D2D1_GRADIENT_STOP {
            position: 0.0,
            color: color_f(palette.ground, 0.58),
        },
        D2D1_GRADIENT_STOP {
            position: 0.55,
            color: color_f(palette.ground, 0.0),
        },
    ];
    if let Ok(stop_col) =
        rt.CreateGradientStopCollection(&h_stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
    {
        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: D2D_POINT_2F { x: 0.0, y: 0.0 },
            endPoint: D2D_POINT_2F {
                x: WIN_W_DIP * 0.62,
                y: 0.0,
            },
        };
        if let Ok(grad) = rt.CreateLinearGradientBrush(&props, None, &stop_col) {
            rt.FillRectangle(&hero_d2d, &grad);
        }
    }

    // Hero Left: Official Logo PNG or single-weight display title + platform/recency line
    if let Some(game) = focused {
        let mut drew_logo = false;
        if let Some(logo_path) = &game.logo_path {
            let key = logo_path.to_string_lossy();
            let bmp = icons.get_or_load(rt, &key, 512);
            if let Some(bmp) = bmp {
                let sz = bmp.GetSize();
                if sz.width > 1.0 && sz.height > 1.0 {
                    let max_w = 340.0_f32;
                    let max_h = 96.0_f32;
                    let scale = (max_w / sz.width).min(max_h / sz.height).min(1.0);
                    let dw = sz.width * scale;
                    let dh = sz.height * scale;
                    let dst = D2D_RECT_F {
                        left: PAD_X,
                        top: HERO_H - 48.0 - dh,
                        right: PAD_X + dw,
                        bottom: HERO_H - 48.0,
                    };
                    rt.DrawBitmap(
                        &bmp,
                        Some(&dst),
                        1.0,
                        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                        None,
                    );
                    drew_logo = true;
                }
            }
        }

        if !drew_logo {
            if let (Ok(fmt), Ok(brush)) = (
                make_text_format(
                    dwrite,
                    font_name,
                    28.0,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    DWRITE_PARAGRAPH_ALIGNMENT_FAR,
                    true,
                ),
                rt.CreateSolidColorBrush(&color_f(palette.ink, 1.0), None),
            ) {
                let title_w: Vec<u16> = game.name.encode_utf16().collect();
                let title_rect = D2D_RECT_F {
                    left: PAD_X,
                    top: HERO_H - 134.0,
                    right: WIN_W_DIP - 360.0,
                    bottom: HERO_H - 46.0,
                };
                rt.DrawText(
                    &title_w,
                    &fmt,
                    &title_rect,
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }

        // Plain metadata line: platform name + optional real last-played (space-separated, no middle dot)
        let meta_text = match &game.last_played {
            Some(lp) => format!("{}     {}", game.source, lp),
            None => game.source.clone(),
        };
        if let (Ok(fmt), Ok(brush)) = (
            make_text_format(
                dwrite,
                font_name,
                13.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                false,
            ),
            rt.CreateSolidColorBrush(&color_f(palette.ink_muted, 0.95), None),
        ) {
            let meta_w: Vec<u16> = meta_text.encode_utf16().collect();
            let meta_rect = D2D_RECT_F {
                left: PAD_X,
                top: HERO_H - 42.0,
                right: WIN_W_DIP - 360.0,
                bottom: HERO_H - 20.0,
            };
            rt.DrawText(
                &meta_w,
                &fmt,
                &meta_rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }

        // Hero Right: Primary Play button + secondary Library/Folder buttons
        let play_alpha = if hover == HoverTarget::PlayBtn {
            1.0
        } else {
            0.94
        };
        let play_rr = D2D1_ROUNDED_RECT {
            rect: rect_to_d2d(play_rect),
            radiusX: 10.0,
            radiusY: 10.0,
        };
        if let Ok(bg_b) = rt.CreateSolidColorBrush(&color_f(palette.accent, play_alpha), None) {
            rt.FillRoundedRectangle(&play_rr, &bg_b);
        }
        if let Ok(ink_b) = rt.CreateSolidColorBrush(&color_f(palette.accent_ink, 1.0), None) {
            // Crisp vector Play triangle
            draw_play_triangle(
                rt,
                d2d,
                &ink_b,
                play_rect.x + 28.0,
                play_rect.y + play_rect.h * 0.5,
                10.0,
            );
            if let Ok(fmt) = make_text_format(
                dwrite,
                font_name,
                13.5,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                false,
            ) {
                let txt: Vec<u16> = "Play".encode_utf16().collect();
                let r = D2D_RECT_F {
                    left: play_rect.x + 48.0,
                    top: play_rect.y,
                    right: play_rect.x + play_rect.w - 10.0,
                    bottom: play_rect.y + play_rect.h,
                };
                rt.DrawText(
                    &txt,
                    &fmt,
                    &r,
                    &ink_b,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }

        if let Some(sr) = sec_rect {
            draw_secondary_button(
                rt,
                dwrite,
                font_name,
                sr,
                "Library",
                hover == HoverTarget::SecondaryBtn,
                palette,
            );
        }
        if let Some(fr) = folder_rect {
            draw_secondary_button(
                rt,
                dwrite,
                font_name,
                fr,
                "Folder",
                hover == HoverTarget::FolderBtn,
                palette,
            );
        }
    } else if let (Ok(fmt), Ok(brush)) = (
        make_text_format(
            dwrite,
            font_name,
            22.0,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            false,
        ),
        rt.CreateSolidColorBrush(&color_f(palette.ink_muted, 1.0), None),
    ) {
        let msg = if total_games == 0 {
            "No installed games found"
        } else {
            "No matching games"
        };
        let wmsg: Vec<u16> = msg.encode_utf16().collect();
        let r = D2D_RECT_F {
            left: PAD_X,
            top: HERO_H - 90.0,
            right: WIN_W_DIP - PAD_X,
            bottom: HERO_H - 30.0,
        };
        rt.DrawText(
            &wmsg,
            &fmt,
            &r,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    // 2. INTEGRATED SEARCH & STATUS STRIP (HERO_H .. SHELF_Y)
    if let Ok(line_b) = rt.CreateSolidColorBrush(&color_f(palette.ink, 0.08), None) {
        rt.DrawLine(
            D2D_POINT_2F {
                x: PAD_X,
                y: SHELF_Y,
            },
            D2D_POINT_2F {
                x: WIN_W_DIP - PAD_X,
                y: SHELF_Y,
            },
            &line_b,
            1.0,
            None,
        );
    }

    // Vector magnifier icon (no emoji)
    let bar_mid_y = HERO_H + BAR_H * 0.5;
    let icon_color = if query.is_empty() {
        color_f(palette.ink_muted, 0.85)
    } else {
        color_f(palette.accent, 1.0)
    };
    if let Ok(ic_b) = rt.CreateSolidColorBrush(&icon_color, None) {
        let ellipse = D2D1_ELLIPSE {
            point: D2D_POINT_2F {
                x: PAD_X + 7.5,
                y: bar_mid_y - 1.5,
            },
            radiusX: 5.5,
            radiusY: 5.5,
        };
        rt.DrawEllipse(&ellipse, &ic_b, 1.6, None);
        rt.DrawLine(
            D2D_POINT_2F {
                x: PAD_X + 11.5,
                y: bar_mid_y + 2.5,
            },
            D2D_POINT_2F {
                x: PAD_X + 15.5,
                y: bar_mid_y + 6.5,
            },
            &ic_b,
            1.7,
            None,
        );
    }

    // Search text / placeholder
    let (search_text, search_col) = if query.is_empty() {
        (
            "Type to filter games".to_string(),
            color_f(palette.ink_muted, 0.72),
        )
    } else {
        (query.to_string(), color_f(palette.ink, 1.0))
    };
    if let (Ok(fmt), Ok(brush)) = (
        make_text_format(
            dwrite,
            font_name,
            13.5,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            false,
        ),
        rt.CreateSolidColorBrush(&search_col, None),
    ) {
        let wtxt: Vec<u16> = search_text.encode_utf16().collect();
        let r = D2D_RECT_F {
            left: PAD_X + 26.0,
            top: HERO_H,
            right: WIN_W_DIP * 0.54,
            bottom: SHELF_Y,
        };
        rt.DrawText(
            &wtxt,
            &fmt,
            &r,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    // Right status: real count + quiet shortcut hints (no middle dots)
    let count_str = if query.is_empty() {
        if total_games == 1 {
            "1 game".to_string()
        } else {
            format!("{total_games} games")
        }
    } else {
        format!("{filtered_len} of {total_games}")
    };
    let status_str =
        format!("{count_str}     Enter play     Shift+Enter library     Shift+Del reset");
    if let (Ok(fmt), Ok(brush)) = (
        make_text_format(
            dwrite,
            font_name,
            11.5,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            false,
        ),
        rt.CreateSolidColorBrush(&color_f(palette.ink_muted, 0.80), None),
    ) {
        let wtxt: Vec<u16> = status_str.encode_utf16().collect();
        let r = D2D_RECT_F {
            left: WIN_W_DIP * 0.46,
            top: HERO_H,
            right: WIN_W_DIP - PAD_X,
            bottom: SHELF_Y,
        };
        rt.DrawText(
            &wtxt,
            &fmt,
            &r,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    // 3. BOX-ART POSTER SHELF (SHELF_Y .. WIN_H_DIP)
    let shelf_clip = D2D_RECT_F {
        left: 12.0,
        top: SHELF_Y + 4.0,
        right: WIN_W_DIP - 12.0,
        bottom: WIN_H_DIP - 8.0,
    };
    rt.PushAxisAlignedClip(&shelf_clip, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);

    let title_fmt = make_text_format(
        dwrite,
        font_name,
        11.5,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
        false,
    )
    .ok();
    let fallback_fmt = make_text_format(
        dwrite,
        font_name,
        12.5,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_CENTER,
        DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
        true,
    )
    .ok();

    for (pos, game) in visible_cards {
        let pos = *pos;
        let x = PAD_X + pos as f32 * CARD_STRIDE - scroll_x;
        if x + CARD_W < PAD_X - 24.0 || x > WIN_W_DIP - PAD_X + 24.0 {
            continue;
        }
        let is_sel = pos == selected;
        let is_hov = hover == HoverTarget::Card(pos);
        let lift = if is_sel {
            6.0
        } else if is_hov {
            2.5
        } else {
            0.0
        };
        let card_rect = Rect {
            x,
            y: SHELF_Y + 20.0 - lift,
            w: CARD_W,
            h: CARD_H,
        };
        let card_rr = D2D1_ROUNDED_RECT {
            rect: rect_to_d2d(card_rect),
            radiusX: CARD_RADIUS,
            radiusY: CARD_RADIUS,
        };

        // Clip poster to rounded rect
        let c_geom = d2d.CreateRoundedRectangleGeometry(&card_rr).ok();
        let c_clipped = if let Some(g) = &c_geom {
            let mask: ID2D1Geometry = g.clone().into();
            let mut params = D2D1_LAYER_PARAMETERS {
                contentBounds: card_rr.rect,
                geometricMask: std::mem::ManuallyDrop::new(Some(mask)),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2::identity(),
                opacity: if is_sel || is_hov { 1.0 } else { 0.80 },
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            rt.PushLayer(&params, None);
            std::mem::ManuallyDrop::drop(&mut params.geometricMask);
            true
        } else {
            false
        };

        if let Ok(surf_b) = rt.CreateSolidColorBrush(&color_f(palette.surface, 1.0), None) {
            rt.FillRectangle(&card_rr.rect, &surf_b);
        }

        let mut drew_cover = false;
        let cover = game.cover_path.as_ref().or(game.hero_path.as_ref());
        if let Some(p) = cover {
            let key = p.to_string_lossy();
            let bmp = icons.get_or_load(rt, &key, 512);
            if let Some(bmp) = bmp {
                draw_bitmap_cover_aligned(rt, &bmp, card_rect, 0.5, 0.5);
                drew_cover = true;
            }
        }
        if !drew_cover {
            if let (Some(fmt), Ok(brush)) = (
                &fallback_fmt,
                rt.CreateSolidColorBrush(&color_f(palette.ink, 0.88), None),
            ) {
                let wtxt: Vec<u16> = game.name.encode_utf16().collect();
                let pad_r = D2D_RECT_F {
                    left: card_rect.x + 10.0,
                    top: card_rect.y + 16.0,
                    right: card_rect.x + card_rect.w - 10.0,
                    bottom: card_rect.y + card_rect.h - 16.0,
                };
                rt.DrawText(
                    &wtxt,
                    fmt,
                    &pad_r,
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }

        if c_clipped {
            rt.PopLayer();
        }

        // Instant focus ring on selected poster; subtle hairline on others
        if is_sel {
            if let Ok(acc_b) = rt.CreateSolidColorBrush(&color_f(palette.accent, 1.0), None) {
                rt.DrawRoundedRectangle(&card_rr, &acc_b, 2.5, None);
            }
        } else if let Ok(edge_b) = rt.CreateSolidColorBrush(
            &color_f(palette.ink, if is_hov { 0.30 } else { 0.12 }),
            None,
        ) {
            rt.DrawRoundedRectangle(&card_rr, &edge_b, 1.0, None);
        }

        // Single-line title below poster
        if let Some(fmt) = &title_fmt {
            let tcol = if is_sel {
                color_f(palette.ink, 1.0)
            } else {
                color_f(palette.ink_muted, 0.88)
            };
            if let Ok(tb) = rt.CreateSolidColorBrush(&tcol, None) {
                let wtxt: Vec<u16> = game.name.encode_utf16().collect();
                let tr = D2D_RECT_F {
                    left: card_rect.x + 1.0,
                    top: SHELF_Y + 20.0 + CARD_H + 8.0,
                    right: card_rect.x + CARD_W - 1.0,
                    bottom: WIN_H_DIP - 10.0,
                };
                rt.DrawText(
                    &wtxt,
                    fmt,
                    &tr,
                    &tb,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    rt.PopAxisAlignedClip();

    // Soft horizontal edge fades on the poster shelf so scrolling cards never slice abruptly
    let fade_w = 34.0_f32;
    let left_fade_stops = [
        D2D1_GRADIENT_STOP {
            position: 0.0,
            color: color_f(palette.ground, 0.98),
        },
        D2D1_GRADIENT_STOP {
            position: 1.0,
            color: color_f(palette.ground, 0.0),
        },
    ];
    if let Ok(sc) =
        rt.CreateGradientStopCollection(&left_fade_stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
    {
        let lp = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: D2D_POINT_2F { x: 1.0, y: 0.0 },
            endPoint: D2D_POINT_2F {
                x: 1.0 + fade_w,
                y: 0.0,
            },
        };
        if let Ok(lb) = rt.CreateLinearGradientBrush(&lp, None, &sc) {
            rt.FillRectangle(
                &D2D_RECT_F {
                    left: 1.0,
                    top: SHELF_Y + 4.0,
                    right: 1.0 + fade_w,
                    bottom: WIN_H_DIP - 1.0,
                },
                &lb,
            );
        }
    }
    let right_fade_stops = [
        D2D1_GRADIENT_STOP {
            position: 0.0,
            color: color_f(palette.ground, 0.0),
        },
        D2D1_GRADIENT_STOP {
            position: 1.0,
            color: color_f(palette.ground, 0.98),
        },
    ];
    if let Ok(sc) =
        rt.CreateGradientStopCollection(&right_fade_stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
    {
        let rp = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: D2D_POINT_2F {
                x: WIN_W_DIP - 1.0 - fade_w,
                y: 0.0,
            },
            endPoint: D2D_POINT_2F {
                x: WIN_W_DIP - 1.0,
                y: 0.0,
            },
        };
        if let Ok(rb) = rt.CreateLinearGradientBrush(&rp, None, &sc) {
            rt.FillRectangle(
                &D2D_RECT_F {
                    left: WIN_W_DIP - 1.0 - fade_w,
                    top: SHELF_Y + 4.0,
                    right: WIN_W_DIP - 1.0,
                    bottom: WIN_H_DIP - 1.0,
                },
                &rb,
            );
        }
    }

    if clipped {
        rt.PopLayer();
    }

    // Crisp outer window border
    if let Ok(border_b) = rt.CreateSolidColorBrush(&color_f(palette.ink, 0.14), None) {
        rt.DrawRoundedRectangle(&full_rr, &border_b, 1.2, None);
    }
}

unsafe fn draw_secondary_button(
    rt: &ID2D1RenderTarget,
    dwrite: &IDWriteFactory,
    font_name: &str,
    rect: Rect,
    label: &str,
    hovered: bool,
    palette: &GamesPalette,
) {
    let rr = D2D1_ROUNDED_RECT {
        rect: rect_to_d2d(rect),
        radiusX: 10.0,
        radiusY: 10.0,
    };
    let fill_a = if hovered { 0.72 } else { 0.44 };
    if let Ok(bg_b) = rt.CreateSolidColorBrush(&color_f(palette.ground, fill_a), None) {
        rt.FillRoundedRectangle(&rr, &bg_b);
    }
    let stroke_a = if hovered { 0.34 } else { 0.14 };
    if let Ok(st_b) = rt.CreateSolidColorBrush(&color_f(palette.ink, stroke_a), None) {
        rt.DrawRoundedRectangle(&rr, &st_b, 1.0, None);
    }
    if let (Ok(fmt), Ok(ink_b)) = (
        make_text_format(
            dwrite,
            font_name,
            13.0,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            false,
        ),
        rt.CreateSolidColorBrush(&color_f(palette.ink, 0.96), None),
    ) {
        let txt: Vec<u16> = label.encode_utf16().collect();
        rt.DrawText(
            &txt,
            &fmt,
            &rr.rect,
            &ink_b,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
}

unsafe fn draw_play_triangle(
    rt: &ID2D1RenderTarget,
    d2d: &ID2D1Factory,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    cx: f32,
    cy: f32,
    size: f32,
) {
    let Ok(path) = d2d.CreatePathGeometry() else {
        return;
    };
    let Ok(sink) = path.Open() else {
        return;
    };
    let h = size * 0.58;
    let w = size * 0.52;
    sink.BeginFigure(
        D2D_POINT_2F {
            x: cx - w * 0.65,
            y: cy - h,
        },
        D2D1_FIGURE_BEGIN_FILLED,
    );
    sink.AddLine(D2D_POINT_2F {
        x: cx + w * 0.95,
        y: cy,
    });
    sink.AddLine(D2D_POINT_2F {
        x: cx - w * 0.65,
        y: cy + h,
    });
    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
    let _ = sink.Close();
    rt.FillGeometry(&path, brush, None);
}

unsafe fn make_text_format(
    dwrite: &IDWriteFactory,
    font_name: &str,
    size: f32,
    weight: windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT,
    align: windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_ALIGNMENT,
    valign: windows::Win32::Graphics::DirectWrite::DWRITE_PARAGRAPH_ALIGNMENT,
    wrap: bool,
) -> windows::core::Result<IDWriteTextFormat> {
    let family = HSTRING::from(font_name);
    let fmt = dwrite.CreateTextFormat(
        &family,
        None,
        weight,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        size,
        w!("en-US"),
    )?;
    let _ = fmt.SetTextAlignment(align);
    let _ = fmt.SetParagraphAlignment(valign);
    let _ = fmt.SetWordWrapping(if wrap {
        DWRITE_WORD_WRAPPING_WRAP
    } else {
        DWRITE_WORD_WRAPPING_NO_WRAP
    });
    if !wrap {
        let trim = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            delimiter: 0,
            delimiterCount: 0,
        };
        if let Ok(ellipsis) = dwrite.CreateEllipsisTrimmingSign(&fmt) {
            let _ = fmt.SetTrimming(&trim, &ellipsis);
        }
    }
    Ok(fmt)
}

fn rect_to_d2d(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}

unsafe extern "system" fn games_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return LRESULT(1);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut GamesWidgetState;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *ptr;

    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            state.paint();
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE
                && state.opened_at.elapsed().as_millis() > 260
            {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_APP_ICON_READY => {
            state.paint();
            LRESULT(0)
        }
        WM_APP_GAMES_READY => {
            if let Some(rx) = &state.refresh_rx {
                if let Ok(fresh) = rx.try_recv() {
                    let keep = state.focused_game().map(|g| g.key.clone());
                    state.all_games = fresh;
                    state.rebuild_filter(keep.as_deref());
                    state.paint();
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == ANIM_TIMER_ID {
                let now = Instant::now();
                let dt = now
                    .duration_since(state.last_frame)
                    .as_secs_f32()
                    .clamp(0.001, 0.05);
                state.last_frame = now;
                let diff = state.target_scroll_x - state.scroll_x;
                if diff.abs() < 0.4 {
                    state.scroll_x = state.target_scroll_x;
                    state.animating = false;
                    let _ = KillTimer(hwnd, ANIM_TIMER_ID);
                } else {
                    let alpha = 1.0 - (-15.0 * dt).exp();
                    state.scroll_x += diff * alpha;
                }
                state.paint();
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as i16 as f32 / state.dpi_scale;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / state.dpi_scale;
            let next_hover = state.hit_test(x, y);
            if next_hover != state.hover {
                state.hover = next_hover;
                state.paint();
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as f32 / state.dpi_scale;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / state.dpi_scale;
            match state.hit_test(x, y) {
                HoverTarget::PlayBtn => state.trigger_launch(),
                HoverTarget::SecondaryBtn => state.trigger_secondary(),
                HoverTarget::FolderBtn => state.trigger_folder(),
                HoverTarget::Card(pos) => state.select_index(pos),
                HoverTarget::None => {}
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            let x = (lparam.0 & 0xFFFF) as i16 as f32 / state.dpi_scale;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32 / state.dpi_scale;
            if let HoverTarget::Card(pos) = state.hit_test(x, y) {
                state.select_index(pos);
                state.trigger_launch();
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xFFFF) as i16;
            if delta < 0 {
                state.step_selection(1);
            } else if delta > 0 {
                state.step_selection(-1);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wparam.0 as u32;
            let shift = (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;
            let ctrl = (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
            match vk {
                0x1B => {
                    // Escape: clear filter first, or close
                    if !state.query.is_empty() {
                        let keep = state.focused_game().map(|g| g.key.clone());
                        state.query.clear();
                        state.rebuild_filter(keep.as_deref());
                        state.paint();
                    } else {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                0x0D => {
                    // Enter / Shift+Enter
                    if shift {
                        state.trigger_secondary();
                    } else {
                        state.trigger_launch();
                    }
                }
                0x25 => state.step_selection(-1), // Left
                0x27 => state.step_selection(1),  // Right
                0x24 => state.select_index(0),    // Home
                0x23 => {
                    if !state.filtered.is_empty() {
                        state.select_index(state.filtered.len() - 1);
                    }
                }
                0x08 => {
                    // Backspace / Ctrl+Backspace
                    if !state.query.is_empty() {
                        let keep = state.focused_game().map(|g| g.key.clone());
                        if ctrl {
                            state.query.clear();
                        } else {
                            state.query.pop();
                        }
                        state.rebuild_filter(keep.as_deref());
                        state.paint();
                    }
                }
                0x2E if shift => {
                    // Shift+Delete: reset recency/access record (`rofi-games` parity)
                    state.trigger_reset_access();
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CHAR => {
            if let Some(ch) = char::from_u32(wparam.0 as u32) {
                if !ch.is_control() {
                    let keep = state.focused_game().map(|g| g.key.clone());
                    state.query.push(ch);
                    state.rebuild_filter(keep.as_deref());
                    state.paint();
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            if state.animating {
                let _ = KillTimer(hwnd, ANIM_TIMER_ID);
                state.animating = false;
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Runs the interactive Hallmark Direct2D Game Launcher window and returns the chosen action.
/// If `refresh_fn` is provided, it runs in a background thread after the window appears so
/// cached launches open instantaneously and still pick up newly installed/removed games.
pub fn run_games_widget<F, R>(
    games: Vec<GameCardItem>,
    palette: GamesPalette,
    on_reset_access: F,
    refresh_fn: Option<R>,
) -> Result<GamesWidgetOutcome, String>
where
    F: FnMut(&str) + 'static,
    R: FnOnce() -> Vec<GameCardItem> + Send + 'static,
{
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
            .map_err(|e| format!("D2D1CreateFactory: {e}"))?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
            .map_err(|e| format!("DWriteCreateFactory: {e}"))?;

        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let hmon = MonitorFromPoint(cursor, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(hmon, &mut mi);
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        let _ = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let dpi_scale = (dpi_x as f32 / 96.0).max(1.0);

        let win_w = (WIN_W_DIP * dpi_scale).round() as i32;
        let win_h = (WIN_H_DIP * dpi_scale).round() as i32;
        let mon = mi.rcWork;
        let x = mon.left + ((mon.right - mon.left) - win_w) / 2;
        let y = mon.top + ((mon.bottom - mon.top) - win_h) / 2;

        let hinstance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let class_name = w!("LiteRofiGamesShelfClass");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(games_wnd_proc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let filtered: Vec<usize> = (0..games.len()).collect();
        let mut state = Box::new(GamesWidgetState {
            hwnd: HWND::default(),
            dpi_scale,
            d2d: d2d.clone(),
            dwrite,
            rt: None,
            icons: IconCache::new()?,
            palette,
            all_games: games,
            filtered,
            selected: 0,
            query: String::new(),
            scroll_x: 0.0,
            target_scroll_x: 0.0,
            animating: false,
            last_frame: Instant::now(),
            hover: HoverTarget::None,
            opened_at: Instant::now(),
            outcome: GamesWidgetOutcome::Cancelled,
            on_reset_access: Some(Box::new(on_reset_access)),
            refresh_rx: None,
        });

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("LiteRofi Games"),
            WS_POPUP,
            x,
            y,
            win_w,
            win_h,
            None,
            None,
            hinstance,
            Some(&mut *state as *mut GamesWidgetState as *const core::ffi::c_void),
        )
        .map_err(|e| format!("CreateWindowExW: {e}"))?;

        state.hwnd = hwnd;

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
            dpiX: 96.0 * dpi_scale,
            dpiY: 96.0 * dpi_scale,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            ..Default::default()
        };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: win_w as u32,
                height: win_h as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        let rt = d2d
            .CreateHwndRenderTarget(&rt_props, &hwnd_props)
            .map_err(|e| format!("CreateHwndRenderTarget: {e}"))?;
        state.rt = Some(rt);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        state.paint();

        if let Some(bg_refresh) = refresh_fn {
            let (tx, rx) = mpsc::channel();
            state.refresh_rx = Some(rx);
            let hwnd_raw = hwnd.0 as usize;
            std::thread::spawn(move || {
                let fresh = bg_refresh();
                if tx.send(fresh).is_ok() {
                    let _ = PostMessageW(
                        HWND(hwnd_raw as *mut core::ffi::c_void),
                        WM_APP_GAMES_READY,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            });
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let outcome = state.outcome.clone();
        drop(state);
        drop(d2d);
        CoUninitialize();
        Ok(outcome)
    }
}

/// Renders the Game Launcher to a PNG file for automated visual verification (`-dump-png`).
pub fn dump_games_widget_png(
    games: Vec<GameCardItem>,
    palette: GamesPalette,
    out_path: &Path,
) -> Result<(i32, i32), String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| -> Result<(i32, i32), String> {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
                .map_err(|e| format!("D2D1CreateFactory: {e}"))?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)
                .map_err(|e| format!("DWriteCreateFactory: {e}"))?;

            let scale = 1.25_f32;
            let w = (WIN_W_DIP * scale).round() as i32;
            let h = (WIN_H_DIP * scale).round() as i32;
            let surf = OffscreenSurface::new(w, h)?;

            let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                ..Default::default()
            };
            let dc_rt: ID2D1DCRenderTarget = d2d
                .CreateDCRenderTarget(&rt_props)
                .map_err(|e| format!("CreateDCRenderTarget: {e}"))?;
            let rc = RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            };
            dc_rt
                .BindDC(surf.hdc, &rc)
                .map_err(|e| format!("BindDC: {e}"))?;

            let rt: ID2D1RenderTarget = dc_rt.into();
            let mut icons = IconCache::new()?;
            let visible_cards: Vec<(usize, GameCardItem)> =
                games.iter().cloned().enumerate().collect();
            let focused = games.first();
            let play_rect = Rect {
                x: WIN_W_DIP - PAD_X - 116.0,
                y: HERO_H - 58.0,
                w: 116.0,
                h: 38.0,
            };
            let sec_rect = focused.and_then(|g| {
                g.library_uri.as_ref().map(|_| Rect {
                    x: play_rect.x - 10.0 - 94.0,
                    y: HERO_H - 58.0,
                    w: 94.0,
                    h: 38.0,
                })
            });
            let folder_rect = focused.and_then(|g| {
                g.folder_path.as_ref().map(|_| {
                    let right_edge = sec_rect.map_or(play_rect.x, |r| r.x);
                    Rect {
                        x: right_edge - 10.0 - 86.0,
                        y: HERO_H - 58.0,
                        w: 86.0,
                        h: 38.0,
                    }
                })
            });

            rt.BeginDraw();
            draw_games_scene(
                &rt,
                &d2d,
                &dwrite,
                &mut icons,
                &palette,
                focused,
                &visible_cards,
                0,
                0.0,
                "",
                games.len(),
                games.len(),
                HoverTarget::None,
                play_rect,
                sec_rect,
                folder_rect,
                true,
            );
            rt.EndDraw(None, None)
                .map_err(|e| format!("EndDraw: {e}"))?;

            if let Some(parent) = out_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            surf.save_png(out_path)?;
            let _ = PCWSTR::null();
            Ok((w, h))
        })();
        CoUninitialize();
        result
    }
}
