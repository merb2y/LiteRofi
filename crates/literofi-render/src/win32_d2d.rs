//! Win32 windowing, Direct2D rendering, DirectWrite typography & metrics,
//! clipboard integration, and keyboard/mouse event loop.

use crate::font::parse_rofi_font;
use crate::layout::Rect;
use crate::pango::parse_pango_markup;
use crate::state::{LauncherState, ListEntry, UiOutcome};
use crate::tree::{effective_base_font, LaidOutTree, LaidOutWidget, RenderWidgetKind, WidgetStyle};
use crate::win32_icons::{
    draw_bitmap_cover_aligned, draw_bitmap_fitted_aligned, draw_bitmap_scaled,
    freedesktop_symbolic_glyph, IconCache, WM_APP_ICON_READY,
};
use literofi_core::config::Config;
use literofi_core::keyb::{parse_chord_list, KeyBindingAction};
use literofi_theme::{
    highlight_flags, DistanceMetrics, GradientDirection, HighlightColorStyle, ImageValue,
    LineStyle, MonitorGeometry, ThemeColor, ThemeSheet, WidgetQuery,
};
use std::cell::RefCell;
use std::rc::Rc;

use windows::core::{w, Interface, HSTRING, PCWSTR};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::{
    BOOL, COLORREF, D2DERR_RECREATE_TARGET, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE,
    WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
    D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Bitmap, ID2D1Brush, ID2D1DCRenderTarget, ID2D1Factory, ID2D1Geometry,
    ID2D1LinearGradientBrush, ID2D1PathGeometry, ID2D1RenderTarget, ID2D1SolidColorBrush,
    ID2D1StrokeStyle, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_FLAT,
    D2D1_COMBINE_MODE_EXCLUDE, D2D1_COMBINE_MODE_INTERSECT, D2D1_DASH_STYLE_DASH,
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_GAMMA_2_2,
    D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat,
    IDWriteTextLayout, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_LIGHT, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_FAR,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP,
};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC, GetMonitorInfoW,
    MonitorFromPoint, ReleaseDC, SelectObject, SetStretchBltMode, StretchBlt, ValidateRect,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HALFTONE, HBITMAP, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST, SRCCOPY,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, GetDpiForWindow, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetKeyState, ReleaseCapture, SetActiveWindow, SetCapture, SetFocus,
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_RWIN,
    VK_SHIFT, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsWindowVisible, LoadCursorW, PostQuitMessage, RegisterClassExW, SetCursor,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW,
    TranslateMessage, UpdateLayeredWindow, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA,
    HWND_TOPMOST, IDC_ARROW, IDC_HAND, IDC_IBEAM, MSG, SPI_GETDESKWALLPAPER, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, ULW_ALPHA,
    WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_NCDESTROY, WM_PAINT, WM_RBUTTONDOWN, WM_SIZE, WM_SYSKEYDOWN, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const WM_APP_WM_QUIT: u32 = 0;
const CF_UNICODETEXT_U32: u32 = 13;

/// Summary returned by [`run_render_self_test`].
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSelfTestReport {
    pub dpi: f64,
    pub char_height: f64,
    pub char_width: f64,
    pub window_width: i32,
    pub window_height: i32,
    pub frames_painted: usize,
    pub icons_loaded: usize,
}

struct ComGuard(bool);

impl ComGuard {
    fn init() -> Self {
        // SAFETY: Initializing single-threaded COM on the current UI thread.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        Self(hr.is_ok())
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: Paired with successful `CoInitializeEx` on this same thread.
            unsafe { CoUninitialize() };
        }
    }
}

struct LayeredSurface {
    hdc: HDC,
    hbmp: HBITMAP,
    old_bmp: HGDIOBJ,
    bits: *const u8,
    width: u32,
    height: u32,
}

impl LayeredSurface {
    fn ensure(slot: &mut Option<Self>, width: u32, height: u32) -> Result<HDC, String> {
        let w = width.max(1);
        let h = height.max(1);
        if let Some(s) = slot {
            if s.width == w && s.height == h {
                return Ok(s.hdc);
            }
        }
        *slot = None;
        // SAFETY: Allocates an offscreen memory DC and 32-bit top-down premultiplied DIBSection.
        unsafe {
            let hdc = CreateCompatibleDC(None);
            if hdc.0.is_null() {
                return Err("CreateCompatibleDC failed".to_string());
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbmp = match CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) => b,
                Err(e) => {
                    let _ = DeleteDC(hdc);
                    return Err(format!("CreateDIBSection failed: {e}"));
                }
            };
            let old_bmp = SelectObject(hdc, hbmp);
            *slot = Some(Self {
                hdc,
                hbmp,
                old_bmp,
                bits: bits as *const u8,
                width: w,
                height: h,
            });
            Ok(hdc)
        }
    }
}

impl Drop for LayeredSurface {
    fn drop(&mut self) {
        // SAFETY: Restores previous GDI bitmap selection and releases owned HBITMAP and HDC.
        unsafe {
            let _ = SelectObject(self.hdc, self.old_bmp);
            let _ = DeleteObject(self.hbmp);
            let _ = DeleteDC(self.hdc);
        }
    }
}

struct D2dResources {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    sys_fonts: Option<IDWriteFontCollection>,
    mono_font_family: HSTRING,
    nerd_font_family: Option<HSTRING>,
    fluent_icon_font: Option<HSTRING>,
    dashed_stroke: ID2D1StrokeStyle,
    dc_rt: Option<ID2D1DCRenderTarget>,
    surface: Option<LayeredSurface>,
    icons: Option<IconCache>,
}

impl D2dResources {
    fn new() -> Result<Self, String> {
        // SAFETY: Standard COM factory creation with valid enum parameters.
        let d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }
                .map_err(|e| format!("D2D1CreateFactory failed: {e}"))?;

        // SAFETY: DirectWrite shared factory creation returns a valid `IDWriteFactory`.
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
            .map_err(|e| format!("DWriteCreateFactory failed: {e}"))?;

        let mut sys_fonts: Option<IDWriteFontCollection> = None;
        // SAFETY: Queries installed system font collection without forcing re-enumeration.
        let _ = unsafe { dwrite.GetSystemFontCollection(&mut sys_fonts, false) };

        let find_first = |cands: &[&str]| -> Option<HSTRING> {
            let coll = sys_fonts.as_ref()?;
            for &cand in cands {
                let h = HSTRING::from(cand);
                let mut idx = 0u32;
                let mut exists = BOOL(0);
                // SAFETY: Valid HSTRING and out-pointers for `FindFamilyName`.
                if unsafe { coll.FindFamilyName(&h, &mut idx, &mut exists) }.is_ok()
                    && exists.as_bool()
                {
                    return Some(h);
                }
            }
            None
        };

        let mono_font_family = find_first(&[
            "JetBrainsMono NF",
            "JetBrainsMono NFM",
            "Cascadia Mono",
            "Cascadia Code",
            "Fira Code",
            "Consolas",
        ])
        .unwrap_or_else(|| HSTRING::from("Consolas"));

        // Prefer proportional Nerd Font variants (`* NFP` / `Symbols Nerd Font`) for PUA icons
        // so double-width icons carry true proportional advance width and never overlap text.
        let nerd_font_family = find_first(&[
            "Symbols Nerd Font",
            "JetBrainsMono Nerd Font Propo",
            "JetBrainsMono NFP",
            "CaskaydiaCove Nerd Font Propo",
            "CaskaydiaCove NFP",
            "FiraCode Nerd Font Propo",
            "FiraCode NFP",
            "JetBrainsMono Nerd Font",
            "JetBrainsMono NF",
            "JetBrainsMono NFM",
            "CaskaydiaCove Nerd Font",
            "CaskaydiaCove NF",
            "FiraCode Nerd Font",
            "FiraCode NF",
            "Symbols Nerd Font Mono",
        ]);

        let fluent_icon_font = find_first(&["Segoe Fluent Icons", "Segoe MDL2 Assets"]);

        let stroke_props = D2D1_STROKE_STYLE_PROPERTIES {
            startCap: D2D1_CAP_STYLE_FLAT,
            endCap: D2D1_CAP_STYLE_FLAT,
            dashCap: D2D1_CAP_STYLE_FLAT,
            dashStyle: D2D1_DASH_STYLE_DASH,
            ..Default::default()
        };
        // SAFETY: Valid stroke properties struct passed to `ID2D1Factory::CreateStrokeStyle`.
        let dashed_stroke = unsafe { d2d.CreateStrokeStyle(&stroke_props, None) }
            .map_err(|e| format!("CreateStrokeStyle failed: {e}"))?;

        Ok(Self {
            d2d,
            dwrite,
            sys_fonts,
            mono_font_family,
            nerd_font_family,
            fluent_icon_font,
            dashed_stroke,
            dc_rt: None,
            surface: None,
            icons: IconCache::new().ok(),
        })
    }

    fn ensure_dc_rt(
        &mut self,
        phys_w: u32,
        phys_h: u32,
        dpi: f64,
    ) -> Result<(ID2D1DCRenderTarget, HDC), String> {
        let w = phys_w.max(1);
        let h = phys_h.max(1);
        let dpi_f = dpi.max(96.0) as f32;
        let hdc = LayeredSurface::ensure(&mut self.surface, w, h)?;
        let rt = if let Some(existing) = &self.dc_rt {
            existing.clone()
        } else {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi_f,
                dpiY: dpi_f,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            // SAFETY: Valid D2D1 factory and premultiplied BGRA render target properties.
            let created = unsafe { self.d2d.CreateDCRenderTarget(&props) }
                .map_err(|e| format!("CreateDCRenderTarget failed: {e}"))?;
            // SAFETY: Grayscale text antialiasing prevents ClearType alpha fringing on layered surfaces.
            unsafe { created.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE) };
            self.dc_rt = Some(created.clone());
            created
        };
        let sub_rect = RECT {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
        };
        // SAFETY: `hdc` is a live 32bpp DIBSection DC matching `sub_rect`.
        unsafe {
            rt.SetDpi(dpi_f, dpi_f);
            rt.BindDC(hdc, &sub_rect)
                .map_err(|e| format!("ID2D1DCRenderTarget::BindDC failed: {e}"))?;
        }
        Ok((rt, hdc))
    }

    fn find_installed_family(&self, name: &str) -> Option<HSTRING> {
        let h = HSTRING::from(name);
        let Some(coll) = &self.sys_fonts else {
            return Some(h);
        };
        let mut idx = 0u32;
        let mut exists = BOOL(0);
        // SAFETY: Valid HSTRING and out-pointers for `FindFamilyName`.
        if unsafe { coll.FindFamilyName(&h, &mut idx, &mut exists) }.is_ok() && exists.as_bool() {
            Some(h)
        } else {
            None
        }
    }

    fn create_text_format(&self, font_spec: &str) -> Result<IDWriteTextFormat, String> {
        let parsed = parse_rofi_font(font_spec);
        let fam_trimmed = parsed.family.trim();
        let lower = fam_trimmed.to_ascii_lowercase();
        let base_family_h = if matches!(
            lower.as_str(),
            "sans" | "sans-serif" | "system-ui" | "ui-sans-serif" | ""
        ) {
            HSTRING::from("Segoe UI")
        } else if matches!(lower.as_str(), "serif" | "ui-serif") {
            HSTRING::from("Georgia")
        } else if matches!(lower.as_str(), "mono" | "monospace" | "ui-monospace") {
            self.mono_font_family.clone()
        } else if let Some(exact) = self.find_installed_family(fam_trimmed) {
            exact
        } else {
            let mut matched = None;
            if let Some(prefix) = lower
                .strip_suffix(" nerd font propo")
                .or_else(|| lower.strip_suffix(" nerd font mono"))
                .or_else(|| lower.strip_suffix(" nerd font"))
            {
                let compact = prefix.replace(' ', "");
                for cand in [
                    format!("{prefix} NF"),
                    format!("{compact} NF"),
                    format!("{compact} NFM"),
                    format!("{compact} NFP"),
                    prefix.to_string(),
                ] {
                    if let Some(h) = self.find_installed_family(&cand) {
                        matched = Some(h);
                        break;
                    }
                }
            }
            if matched.is_none() {
                if lower.contains("firacode") || lower.contains("fira code") {
                    matched = self
                        .find_installed_family("Fira Code")
                        .or_else(|| self.find_installed_family("Fira Mono"));
                } else if lower.contains("jetbrains") {
                    matched = self
                        .find_installed_family("JetBrains Mono")
                        .or_else(|| self.find_installed_family("JetBrainsMono NF"));
                } else if lower.contains("cascadia") || lower.contains("caskaydia") {
                    matched = self
                        .find_installed_family("Cascadia Mono")
                        .or_else(|| self.find_installed_family("Cascadia Code"));
                } else if lower.contains("ubuntu") && lower.contains("mono") {
                    matched = self
                        .find_installed_family("Ubuntu Sans Mono")
                        .or_else(|| self.find_installed_family("Ubuntu Mono"));
                } else if lower.contains("dejavu") && lower.contains("mono") {
                    matched = self.find_installed_family("DejaVu Sans Mono");
                } else if lower.contains("dejavu") && lower.contains("sans") {
                    matched = self.find_installed_family("DejaVu Sans");
                } else if lower.contains("noto") && lower.contains("sans") {
                    matched = self.find_installed_family("Noto Sans");
                }
            }
            matched.unwrap_or_else(|| {
                let wants_mono = [
                    "mono",
                    "code",
                    "nerd",
                    "iosevka",
                    "jetbrains",
                    "fira",
                    "cascadia",
                    "caskaydia",
                    "consolas",
                    "hack",
                    "courier",
                    "inconsolata",
                    "terminus",
                    "fantasque",
                    "menlo",
                    "monaco",
                    "agave",
                    "victor",
                    "fontawesome",
                    "font awesome",
                    "icomoon",
                    "feather",
                    "material",
                ]
                .iter()
                .any(|kw| lower.contains(kw));
                if wants_mono {
                    self.mono_font_family.clone()
                } else if lower.contains("serif") && !lower.contains("sans") {
                    HSTRING::from("Georgia")
                } else {
                    HSTRING::from("Segoe UI")
                }
            })
        };
        let locale_h = HSTRING::from("en-us");
        let requested_weight = parsed.weight.clamp(100, 950) as i32;
        let style = if parsed.italic {
            DWRITE_FONT_STYLE_ITALIC
        } else {
            DWRITE_FONT_STYLE_NORMAL
        };
        // Some Windows font installs register non-400/700 weights (e.g. `Fira Code Medium`,
        // `JetBrainsMono NF Medium`, `Cascadia Mono SemiBold`) as separate GDI family names.
        let mut family_h = base_family_h;
        let mut weight = DWRITE_FONT_WEIGHT(requested_weight);
        if requested_weight != 400 && requested_weight != 700 {
            let has_exact_weight = self.sys_fonts.as_ref().is_some_and(|coll| {
                let mut idx = 0u32;
                let mut exists = BOOL(0);
                // SAFETY: Valid HSTRING and out-pointers for `FindFamilyName` / `GetFontFamily`.
                unsafe {
                    if coll
                        .FindFamilyName(&family_h, &mut idx, &mut exists)
                        .is_ok()
                        && exists.as_bool()
                    {
                        if let Ok(fam) = coll.GetFontFamily(idx) {
                            if let Ok(fnt) =
                                fam.GetFirstMatchingFont(weight, DWRITE_FONT_STRETCH_NORMAL, style)
                            {
                                return (fnt.GetWeight().0 - requested_weight).abs() <= 50;
                            }
                        }
                    }
                    false
                }
            });
            if !has_exact_weight {
                let suffixes: &[&str] = match requested_weight {
                    ..=150 => &["Thin"],
                    151..=250 => &["ExtraLight", "UltraLight"],
                    251..=350 => &["Light", "Semilight"],
                    450..=550 => &["Medium"],
                    551..=650 => &["SemiBold", "Semibold", "DemiBold"],
                    750..=850 => &["ExtraBold", "UltraBold"],
                    _ => &["Black", "Heavy"],
                };
                let base_str = family_h.to_string_lossy();
                for sfx in suffixes {
                    if let Some(sub_h) = self.find_installed_family(&format!("{base_str} {sfx}")) {
                        family_h = sub_h;
                        weight = DWRITE_FONT_WEIGHT_NORMAL;
                        break;
                    }
                }
            }
        }
        let px_size = (parsed.size_pt * 96.0 / 72.0).max(8.0);
        // SAFETY: Valid `IDWriteFactory::CreateTextFormat` call with null-terminated HSTRINGs.
        let fmt = unsafe {
            self.dwrite.CreateTextFormat(
                &family_h,
                None,
                weight,
                style,
                DWRITE_FONT_STRETCH_NORMAL,
                px_size,
                &locale_h,
            )
        }
        .map_err(|e| format!("CreateTextFormat failed: {e}"))?;
        Ok(fmt)
    }

    fn measure_text_span(&self, font_spec: &str, text: &str) -> (f32, f32) {
        let Ok(fmt) = self.create_text_format(font_spec) else {
            return (text.chars().count() as f32 * 8.0, 16.0);
        };
        // SAFETY: Valid IDWriteTextFormat setter call.
        let _ = unsafe { fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP) };
        let sample = if text.is_empty() { "Mg" } else { text };
        let utf16: Vec<u16> = sample.encode_utf16().collect();
        // SAFETY: Valid UTF-16 buffer and text format pointer.
        let Ok(layout) = (unsafe { self.dwrite.CreateTextLayout(&utf16, &fmt, 8000.0, 400.0) })
        else {
            return (text.chars().count() as f32 * 8.0, 16.0);
        };
        if !text.is_empty() {
            // SAFETY: Valid IDWriteTextLayout and optional installed Nerd Font family HSTRING.
            unsafe {
                apply_nerd_font_pua_ranges(&layout, text, self.nerd_font_family.as_ref());
            }
        }
        let mut tm = DWRITE_TEXT_METRICS::default();
        // SAFETY: `tm` is properly zero-initialized output struct.
        if unsafe { layout.GetMetrics(&mut tm) }.is_err() {
            return (text.chars().count() as f32 * 8.0, 16.0);
        }
        let w = if text.is_empty() {
            0.0
        } else {
            tm.widthIncludingTrailingWhitespace.max(tm.width).ceil()
        };
        (w, tm.height.ceil().max(8.0))
    }

    fn measure_wrapped_height(&self, font_spec: &str, text: &str, max_w: f32) -> f32 {
        let (plain_owned, _) = if text.contains('<') || text.contains('&') {
            parse_pango_markup(text)
        } else {
            (String::new(), Vec::new())
        };
        let eff = if plain_owned.is_empty() {
            text
        } else {
            plain_owned.as_str()
        };
        let Ok(fmt) = self.create_text_format(font_spec) else {
            return 16.0;
        };
        // SAFETY: Valid IDWriteTextFormat setter call.
        let _ = unsafe { fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP) };
        let sample = if eff.is_empty() { "Mg" } else { eff };
        let utf16: Vec<u16> = sample.encode_utf16().collect();
        // SAFETY: Valid UTF-16 buffer and text format pointer.
        let Ok(layout) = (unsafe {
            self.dwrite
                .CreateTextLayout(&utf16, &fmt, max_w.max(16.0), 4000.0)
        }) else {
            return 16.0;
        };
        if !eff.is_empty() {
            // SAFETY: Valid IDWriteTextLayout and optional installed Nerd Font family HSTRING.
            unsafe {
                apply_nerd_font_pua_ranges(&layout, eff, self.nerd_font_family.as_ref());
            }
        }
        let mut tm = DWRITE_TEXT_METRICS::default();
        // SAFETY: `tm` is properly zero-initialized output struct.
        if unsafe { layout.GetMetrics(&mut tm) }.is_err() {
            return 16.0;
        }
        tm.height.ceil().max(8.0)
    }

    fn measure_sheet_font(
        &self,
        sheet: &ThemeSheet,
        cfg_font: &str,
        dpi: f64,
        mon: MonitorGeometry,
    ) -> DistanceMetrics {
        let base_font = effective_base_font(sheet, cfg_font);
        self.measure_font(&base_font, dpi, mon)
    }

    fn measure_font(&self, font_spec: &str, dpi: f64, mon: MonitorGeometry) -> DistanceMetrics {
        let fallback = DistanceMetrics {
            monitor_width: f64::from(mon.width),
            monitor_height: f64::from(mon.height),
            char_height: 16.0,
            char_width: 8.0,
            dpi,
        };
        let Ok(fmt) = self.create_text_format(font_spec) else {
            return fallback;
        };
        let sample = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let utf16: Vec<u16> = sample.encode_utf16().collect();
        // SAFETY: Valid UTF-16 slice and text format pointer.
        let Ok(layout) = (unsafe { self.dwrite.CreateTextLayout(&utf16, &fmt, 4000.0, 200.0) })
        else {
            return fallback;
        };
        let mut tm = DWRITE_TEXT_METRICS::default();
        // SAFETY: `tm` is properly zero-initialized output struct.
        if unsafe { layout.GetMetrics(&mut tm) }.is_err() {
            return fallback;
        }
        // Match Pango's typographic `hhea` line height (`~1.20 * emSize`, `textbox.c:1047`)
        // instead of DirectWrite's inflated `usWinAscent + usWinDescent` (`1.33 * emSize` on Segoe UI).
        let px_size = f64::from((parse_rofi_font(font_spec).size_pt * 96.0 / 72.0).max(8.0));
        let pango_cap = (px_size * 1.20).round().max(8.0);
        let char_h = f64::from(tm.height).round().min(pango_cap).max(8.0);
        let char_w = (f64::from(tm.width) / 62.0).max(4.0);
        DistanceMetrics {
            monitor_width: f64::from(mon.width),
            monitor_height: f64::from(mon.height),
            char_height: char_h,
            char_width: char_w,
            dpi,
        }
    }
}

fn color_to_d2d(c: ThemeColor) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r.clamp(0.0, 1.0) as f32,
        g: c.g.clamp(0.0, 1.0) as f32,
        b: c.b.clamp(0.0, 1.0) as f32,
        a: c.a.clamp(0.0, 1.0) as f32,
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

#[derive(Clone, Copy)]
struct MonitorMetrics {
    workarea_dip: Rect,
    mon_rect_dip: Rect,
    mon_dip: MonitorGeometry,
    phys_origin: (i32, i32),
    mon_phys_origin: (i32, i32),
    dpi: f64,
}

impl MonitorMetrics {
    fn bounds_dip(&self, fullscreen: bool) -> Rect {
        if fullscreen {
            self.mon_rect_dip
        } else {
            self.workarea_dip
        }
    }

    fn phys_origin_for(&self, fullscreen: bool) -> (i32, i32) {
        if fullscreen {
            self.mon_phys_origin
        } else {
            self.phys_origin
        }
    }
}

fn sheet_is_fullscreen(sheet: &ThemeSheet) -> bool {
    sheet.get_boolean(&WidgetQuery::new("window"), "fullscreen", false)
}

fn dip_rect_to_phys(wr: Rect, phys_origin: (i32, i32), dpi: f64) -> (i32, i32, i32, i32) {
    let scale = (dpi / 96.0).max(1.0);
    let x = phys_origin.0 + (f64::from(wr.x) * scale).round() as i32;
    let y = phys_origin.1 + (f64::from(wr.y) * scale).round() as i32;
    let w = (f64::from(wr.w) * scale).round().max(1.0) as i32;
    let h = (f64::from(wr.h) * scale).round().max(1.0) as i32;
    (x, y, w, h)
}

fn query_active_monitor() -> MonitorMetrics {
    let mut pt = POINT::default();
    // SAFETY: `pt` is stack-allocated POINT passed to Win32 `GetCursorPos`.
    let _ = unsafe { GetCursorPos(&mut pt) };
    // SAFETY: Valid `POINT` and flag passed to `MonitorFromPoint`.
    let hmon = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
    let mut dpi_x = 0u32;
    let mut dpi_y = 0u32;
    // SAFETY: `hmon` is a valid monitor handle and output pointers are valid stack locals.
    let dpi_ok =
        unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_ok();
    let dpi = if dpi_ok && dpi_x >= 96 {
        f64::from(dpi_x)
    } else {
        96.0
    };
    let scale = (dpi / 96.0).max(1.0);

    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `mi.cbSize` is properly initialized for `GetMonitorInfoW`.
    if unsafe { GetMonitorInfoW(hmon, &mut mi) }.as_bool() {
        let r = mi.rcWork;
        let w_dip = ((r.right - r.left) as f64 / scale).max(320.0) as f32;
        let h_dip = ((r.bottom - r.top) as f64 / scale).max(240.0) as f32;
        let mon_r = mi.rcMonitor;
        let mw_dip = ((mon_r.right - mon_r.left) as f64 / scale).max(320.0) as f32;
        let mh_dip = ((mon_r.bottom - mon_r.top) as f64 / scale).max(240.0) as f32;
        return MonitorMetrics {
            workarea_dip: Rect::new(0.0, 0.0, w_dip, h_dip),
            mon_rect_dip: Rect::new(0.0, 0.0, mw_dip, mh_dip),
            mon_dip: MonitorGeometry {
                width: mw_dip.round() as i32,
                height: mh_dip.round() as i32,
                monitor_id: 0,
            },
            phys_origin: (r.left, r.top),
            mon_phys_origin: (mon_r.left, mon_r.top),
            dpi,
        };
    }
    MonitorMetrics {
        workarea_dip: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        mon_rect_dip: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        mon_dip: MonitorGeometry {
            width: 1920,
            height: 1080,
            monitor_id: 0,
        },
        phys_origin: (0, 0),
        mon_phys_origin: (0, 0),
        dpi,
    }
}

fn register_window_class(hinst: HINSTANCE) -> Result<PCWSTR, String> {
    let class_name = w!("LiteRofiPopupWindow");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinst,
        // SAFETY: `LoadCursorW(None, IDC_ARROW)` loads a standard system cursor.
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: class_name,
        ..Default::default()
    };
    // SAFETY: `wc` is fully initialized with valid module handle and static class name.
    let atom = unsafe { RegisterClassExW(&wc) };
    if atom == 0 {
        // Class may already be registered in the same process (e.g., repeated self-tests).
        return Ok(class_name);
    }
    Ok(class_name)
}

unsafe fn set_window_cloaked(hwnd: HWND, cloaked: bool) {
    let val = BOOL::from(cloaked);
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_CLOAK,
        &val as *const _ as *const std::ffi::c_void,
        std::mem::size_of_val(&val) as u32,
    );
}

/// Reliably brings the topmost popup to the foreground on Windows 10/11 (same proven sequence
/// as DosyaBul: attaches foreground thread input, asserts HWND_TOPMOST + BringWindowToTop,
/// and unlocks Windows' foreground lock with neutral VK 0xE8 only if SetForegroundWindow fails).
unsafe fn activate_popup_window(hwnd: HWND) {
    let fg = GetForegroundWindow();
    let fg_tid = if !fg.0.is_null() {
        GetWindowThreadProcessId(fg, None)
    } else {
        0
    };
    let cur_tid = GetCurrentThreadId();
    let attached = fg_tid != 0
        && cur_tid != 0
        && fg_tid != cur_tid
        && AttachThreadInput(fg_tid, cur_tid, true).as_bool();

    let _ = ShowWindow(hwnd, SW_SHOW);
    set_window_cloaked(hwnd, false);
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
    );
    let _ = BringWindowToTop(hwnd);
    if !SetForegroundWindow(hwnd).as_bool() {
        // Neutral unassigned VK (0xE8) unlocks Windows' foreground lock without Alt/Win side effects.
        keybd_event(0xE8, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(0xE8, 0, KEYEVENTF_KEYUP, 0);
        let _ = SetForegroundWindow(hwnd);
    }
    let _ = SetActiveWindow(hwnd);
    let _ = SetFocus(hwnd);

    if attached {
        let _ = AttachThreadInput(fg_tid, cur_tid, false);
    }
}

struct WindowCtx {
    sheet: ThemeSheet,
    cfg: Config,
    state: LauncherState,
    metrics: DistanceMetrics,
    monitor: MonitorMetrics,
    sync_window_size: bool,
    async_icons: bool,
    quit_on_outcome: bool,
    suppress_deactivate: bool,
    shown_at: Option<std::time::Instant>,
    layout: LaidOutTree,
    res: D2dResources,
    outcome: Option<UiOutcome>,
    dragging_sb: bool,
    backdrop_blur: Option<(u32, u32, Vec<u8>)>,
}

fn should_blur_backdrop(layout: &LaidOutTree, cfg: &Config) -> Option<u32> {
    if cfg.blur_radius == 0 {
        return None;
    }
    let w = layout.widgets.first()?;
    let a = w.style.background.a;
    if a > 0.01 && a < 0.99 && w.style.background_image.is_none() {
        Some(cfg.blur_radius)
    } else {
        None
    }
}

/// Fast `O(w * h)` 3-pass separable box blur (Gaussian approximation) on BGRA pixels.
fn box_blur_bgra_in_place(buf: &mut [u8], w: usize, h: usize, radius: usize) {
    if w < 2 || h < 2 || radius == 0 || buf.len() < w * h * 4 {
        return;
    }
    let r = radius.min(w.max(h));
    let mut tmp = vec![0u8; w * h * 4];
    for _pass in 0..3 {
        // Horizontal pass: buf -> tmp
        let r_h = r.min(w - 1);
        let div_h = (2 * r_h + 1) as u32;
        for y in 0..h {
            let row = y * w * 4;
            let mut sb = 0u32;
            let mut sg = 0u32;
            let mut sr = 0u32;
            for dx in -(r_h as isize)..=(r_h as isize) {
                let sx = dx.clamp(0, (w - 1) as isize) as usize;
                let idx = row + sx * 4;
                sb += u32::from(buf[idx]);
                sg += u32::from(buf[idx + 1]);
                sr += u32::from(buf[idx + 2]);
            }
            for x in 0..w {
                let dst = row + x * 4;
                tmp[dst] = (sb / div_h) as u8;
                tmp[dst + 1] = (sg / div_h) as u8;
                tmp[dst + 2] = (sr / div_h) as u8;
                tmp[dst + 3] = 255;
                let left_x = (x as isize - r_h as isize).clamp(0, (w - 1) as isize) as usize;
                let right_x = (x as isize + r_h as isize + 1).clamp(0, (w - 1) as isize) as usize;
                let li = row + left_x * 4;
                let ri = row + right_x * 4;
                sb = sb + u32::from(buf[ri]) - u32::from(buf[li]);
                sg = sg + u32::from(buf[ri + 1]) - u32::from(buf[li + 1]);
                sr = sr + u32::from(buf[ri + 2]) - u32::from(buf[li + 2]);
            }
        }
        // Vertical pass: tmp -> buf
        let r_v = r.min(h - 1);
        let div_v = (2 * r_v + 1) as u32;
        for x in 0..w {
            let mut sb = 0u32;
            let mut sg = 0u32;
            let mut sr = 0u32;
            for dy in -(r_v as isize)..=(r_v as isize) {
                let sy = dy.clamp(0, (h - 1) as isize) as usize;
                let idx = (sy * w + x) * 4;
                sb += u32::from(tmp[idx]);
                sg += u32::from(tmp[idx + 1]);
                sr += u32::from(tmp[idx + 2]);
            }
            for y in 0..h {
                let dst = (y * w + x) * 4;
                buf[dst] = (sb / div_v) as u8;
                buf[dst + 1] = (sg / div_v) as u8;
                buf[dst + 2] = (sr / div_v) as u8;
                buf[dst + 3] = 255;
                let top_y = (y as isize - r_v as isize).clamp(0, (h - 1) as isize) as usize;
                let bot_y = (y as isize + r_v as isize + 1).clamp(0, (h - 1) as isize) as usize;
                let ti = (top_y * w + x) * 4;
                let bi = (bot_y * w + x) * 4;
                sb = sb + u32::from(tmp[bi]) - u32::from(tmp[ti]);
                sg = sg + u32::from(tmp[bi + 1]) - u32::from(tmp[ti + 1]);
                sr = sr + u32::from(tmp[bi + 2]) - u32::from(tmp[ti + 2]);
            }
        }
    }
}

/// Captures the live desktop behind `(phys_x, phys_y, phys_w, phys_h)` at 0.5x scale
/// and applies a fast 3-pass Gaussian-like blur for frosted-glass translucent themes.
unsafe fn capture_desktop_blur(
    phys_x: i32,
    phys_y: i32,
    phys_w: i32,
    phys_h: i32,
    radius: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    if phys_w < 4 || phys_h < 4 || radius == 0 {
        return None;
    }
    let bw = ((phys_w / 2).max(2)) as u32;
    let bh = ((phys_h / 2).max(2)) as u32;
    let screen_dc = GetDC(None);
    if screen_dc.0.is_null() {
        return None;
    }
    let mem_dc = CreateCompatibleDC(screen_dc);
    if mem_dc.0.is_null() {
        let _ = ReleaseDC(None, screen_dc);
        return None;
    }
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bw as i32,
            biHeight: -(bh as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbmp = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0);
    let Ok(hbmp) = hbmp else {
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        return None;
    };
    if bits_ptr.is_null() {
        let _ = DeleteObject(hbmp);
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        return None;
    }
    let prev = SelectObject(mem_dc, hbmp);
    let _ = SetStretchBltMode(mem_dc, HALFTONE);
    let ok = StretchBlt(
        mem_dc, 0, 0, bw as i32, bh as i32, screen_dc, phys_x, phys_y, phys_w, phys_h, SRCCOPY,
    )
    .as_bool();
    let _ = GdiFlush();
    let out = if ok {
        let byte_len = (bw as usize) * (bh as usize) * 4;
        let mut px = std::slice::from_raw_parts(bits_ptr as *const u8, byte_len).to_vec();
        box_blur_bgra_in_place(
            &mut px,
            bw as usize,
            bh as usize,
            (radius as usize / 2).clamp(1, 32),
        );
        Some((bw, bh, px))
    } else {
        None
    };
    let _ = SelectObject(mem_dc, prev);
    let _ = DeleteObject(hbmp);
    let _ = DeleteDC(mem_dc);
    let _ = ReleaseDC(None, screen_dc);
    out
}

unsafe fn paint_backdrop_blur(
    rt: &ID2D1RenderTarget,
    d2d: &ID2D1Factory,
    w: &LaidOutWidget,
    bw: u32,
    bh: u32,
    pixels: &[u8],
) {
    if w.paint_rect.w <= 1.0 || w.paint_rect.h <= 1.0 || pixels.len() < (bw * bh * 4) as usize {
        return;
    }
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
    };
    let Ok(bmp): Result<ID2D1Bitmap, _> = rt.CreateBitmap(
        D2D_SIZE_U {
            width: bw,
            height: bh,
        },
        Some(pixels.as_ptr() as *const std::ffi::c_void),
        bw * 4,
        &props,
    ) else {
        return;
    };
    let radii = clamp_corner_radii(w.paint_rect, w.style.border_radius);
    let any_radius = radii.iter().any(|&r| r > 0.25);
    let dst = d2d_rect(w.paint_rect);
    if any_radius {
        if let Some(geom) = make_rounded_geometry(d2d, w.paint_rect, radii) {
            let lp = D2D1_LAYER_PARAMETERS {
                contentBounds: dst,
                geometricMask: std::mem::ManuallyDrop::new(Some(geom)),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2::identity(),
                opacity: 1.0,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            rt.PushLayer(&lp, None);
            let guard = std::mem::ManuallyDrop::into_inner(lp.geometricMask);
            rt.DrawBitmap(
                &bmp,
                Some(&dst),
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            );
            rt.PopLayer();
            drop(guard);
            return;
        }
    }
    rt.PushAxisAlignedClip(&dst, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    rt.DrawBitmap(
        &bmp,
        Some(&dst),
        1.0,
        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
        None,
    );
    rt.PopAxisAlignedClip();
}

impl WindowCtx {
    fn relayout(&mut self, _hwnd: HWND) {
        let fs = sheet_is_fullscreen(&self.sheet);
        let bounds = self.monitor.bounds_dip(fs);
        let res = &self.res;
        let layout = LaidOutTree::build_with_measurers(
            &self.sheet,
            &self.state,
            bounds,
            &self.metrics,
            &self.cfg.font,
            self.cfg.location,
            self.cfg.x_offset,
            self.cfg.y_offset,
            self.cfg.sidebar_mode,
            self.cfg.show_icons,
            self.cfg.hide_icons,
            Some(&|f, t| res.measure_text_span(f, t)),
            Some(&|f, t, mw| res.measure_wrapped_height(f, t, mw)),
        );
        self.layout = layout;
        self.state.ensure_visible(self.layout.listview_page_size);
        // Rebuild cells with adjusted scroll_offset if needed
        let res = &self.res;
        let layout = LaidOutTree::build_with_measurers(
            &self.sheet,
            &self.state,
            bounds,
            &self.metrics,
            &self.cfg.font,
            self.cfg.location,
            self.cfg.x_offset,
            self.cfg.y_offset,
            self.cfg.sidebar_mode,
            self.cfg.show_icons,
            self.cfg.hide_icons,
            Some(&|f, t| res.measure_text_span(f, t)),
            Some(&|f, t, mw| res.measure_wrapped_height(f, t, mw)),
        );
        self.layout = layout;
    }

    fn capture_backdrop_if_needed(&mut self) {
        let Some(radius) = should_blur_backdrop(&self.layout, &self.cfg) else {
            self.backdrop_blur = None;
            return;
        };
        let fs = sheet_is_fullscreen(&self.sheet);
        let (phys_x, phys_y, phys_w, phys_h) = dip_rect_to_phys(
            self.layout.window_rect,
            self.monitor.phys_origin_for(fs),
            self.metrics.dpi,
        );
        // SAFETY: Captures screen DC behind the hidden/cloaked popup rect and releases all GDI handles.
        self.backdrop_blur =
            unsafe { capture_desktop_blur(phys_x, phys_y, phys_w, phys_h, radius) };
    }

    fn paint(&mut self, hwnd: HWND) -> Result<usize, String> {
        for attempt in 0..2 {
            match self.paint_once(hwnd) {
                Ok(n) => return Ok(n),
                Err(_) if attempt == 0 => {
                    self.res.dc_rt = None;
                    if let Some(icons) = &mut self.res.icons {
                        icons.clear();
                    }
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err("ID2D1DCRenderTarget::paint failed after recreate".to_string())
    }

    fn paint_once(&mut self, hwnd: HWND) -> Result<usize, String> {
        let fs = sheet_is_fullscreen(&self.sheet);
        let (phys_x, phys_y, phys_w, phys_h) = dip_rect_to_phys(
            self.layout.window_rect,
            self.monitor.phys_origin_for(fs),
            self.metrics.dpi,
        );
        let phys_w_u32 = phys_w.max(1) as u32;
        let phys_h_u32 = phys_h.max(1) as u32;
        let (dc_rt, hdc) = self
            .res
            .ensure_dc_rt(phys_w_u32, phys_h_u32, self.metrics.dpi)?;
        let rt: ID2D1RenderTarget = dc_rt
            .cast()
            .map_err(|e| format!("ID2D1RenderTarget cast failed: {e}"))?;

        let state = &self.state;
        let layout = &self.layout;
        let cfg = &self.cfg;
        let sheet = &self.sheet;
        let dpi_scale = (self.metrics.dpi / 96.0).max(1.0);
        let async_icons = self.async_icons;
        let mut icons_drawn = 0usize;

        // SAFETY: All D2D BeginDraw/EndDraw pairs and UpdateLayeredWindow calls operate on owned offscreen DC and HWND.
        unsafe {
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            if let (Some(root_w), Some((bw, bh, pixels))) =
                (layout.widgets.first(), self.backdrop_blur.as_ref())
            {
                paint_backdrop_blur(&rt, &self.res.d2d, root_w, *bw, *bh, pixels);
            }

            let paint_res = paint_node(
                0,
                &rt,
                &mut self.res,
                layout,
                state,
                cfg,
                sheet,
                dpi_scale,
                async_icons,
                hwnd,
                &mut icons_drawn,
            );
            let hr = rt.EndDraw(None, None);
            if let Err(err) = hr {
                if err.code() == D2DERR_RECREATE_TARGET {
                    self.res.dc_rt = None;
                    if let Some(icons) = &mut self.res.icons {
                        icons.clear();
                    }
                    return Err("D2DERR_RECREATE_TARGET".to_string());
                } else {
                    return Err(format!("ID2D1DCRenderTarget::EndDraw failed: {err}"));
                }
            }
            paint_res?;

            let dst_pt = POINT {
                x: phys_x,
                y: phys_y,
            };
            let dst_opt = self.sync_window_size.then_some(&dst_pt as *const POINT);
            let win_size = SIZE {
                cx: phys_w.max(1),
                cy: phys_h.max(1),
            };
            let src_pt = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                hwnd,
                None,
                dst_opt,
                Some(&win_size),
                hdc,
                Some(&src_pt),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
        Ok(icons_drawn)
    }
}

fn margin_inset(r: Rect, m: [f32; 4]) -> Rect {
    r.inset(m[0], m[1], m[2], m[3])
}

#[allow(clippy::too_many_arguments)]
unsafe fn paint_node(
    idx: usize,
    rt: &ID2D1RenderTarget,
    res: &mut D2dResources,
    layout: &LaidOutTree,
    state: &LauncherState,
    cfg: &Config,
    sheet: &ThemeSheet,
    dpi_scale: f64,
    async_icons: bool,
    hwnd: HWND,
    icons_drawn: &mut usize,
) -> Result<(), String> {
    let Some(w) = layout.widgets.get(idx) else {
        return Ok(());
    };
    if w.rect.w <= 0.0 || w.rect.h <= 0.0 || w.paint_rect.w <= 0.0 || w.paint_rect.h <= 0.0 {
        return Ok(());
    }

    paint_box_fill(rt, res, w.paint_rect, &w.style)?;

    let is_container = !w.children_indices.is_empty()
        || matches!(
            w.kind,
            RenderWidgetKind::ListView | RenderWidgetKind::ModeSwitcher
        );
    let radii = clamp_corner_radii(w.paint_rect, w.style.border_radius);
    let any_radius = radii.iter().any(|&r| r > 0.25);

    enum ClipKind {
        None,
        Axis,
        Layer(ID2D1Geometry),
    }

    let clip_kind = if !is_container {
        ClipKind::None
    } else if any_radius {
        let uniform = (radii[0] - radii[1]).abs() <= 0.25
            && (radii[1] - radii[2]).abs() <= 0.25
            && (radii[2] - radii[3]).abs() <= 0.25;
        let geom: Option<ID2D1Geometry> = if uniform {
            let rr = D2D1_ROUNDED_RECT {
                rect: d2d_rect(w.paint_rect),
                radiusX: radii[0],
                radiusY: radii[0],
            };
            res.d2d
                .CreateRoundedRectangleGeometry(&rr)
                .ok()
                .map(Into::into)
        } else {
            build_rounded_path(&res.d2d, w.paint_rect, radii).map(Into::into)
        };
        if let Some(geom) = geom {
            let layer_params = D2D1_LAYER_PARAMETERS {
                contentBounds: d2d_rect(w.paint_rect),
                geometricMask: std::mem::ManuallyDrop::new(Some(geom)),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2::identity(),
                opacity: 1.0,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            rt.PushLayer(&layer_params, None);
            let guard = std::mem::ManuallyDrop::into_inner(layer_params.geometricMask)
                .expect("geometricMask kept until PopLayer");
            ClipKind::Layer(guard)
        } else {
            rt.PushAxisAlignedClip(&d2d_rect(w.paint_rect), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            ClipKind::Axis
        }
    } else {
        rt.PushAxisAlignedClip(&d2d_rect(w.paint_rect), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        ClipKind::Axis
    };

    let inner_res = (|| -> Result<(), String> {
        match &w.kind {
            RenderWidgetKind::ListView => {
                for cell in &layout.cells {
                    let cell_paint = margin_inset(cell.rect, cell.row_style.margin);
                    paint_box_fill(rt, res, cell_paint, &cell.row_style)?;

                    let cell_radii = clamp_corner_radii(cell_paint, cell.row_style.border_radius);
                    let cell_clip_geom = if cell_radii.iter().any(|&r| r > 0.25) {
                        make_rounded_geometry(&res.d2d, cell_paint, cell_radii)
                    } else {
                        None
                    };
                    let cell_clip_layer = if let Some(geom) = cell_clip_geom {
                        let lp = D2D1_LAYER_PARAMETERS {
                            contentBounds: d2d_rect(cell_paint),
                            geometricMask: std::mem::ManuallyDrop::new(Some(geom)),
                            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                            maskTransform: Matrix3x2::identity(),
                            opacity: 1.0,
                            opacityBrush: std::mem::ManuallyDrop::new(None),
                            layerOptions: D2D1_LAYER_OPTIONS_NONE,
                        };
                        rt.PushLayer(&lp, None);
                        std::mem::ManuallyDrop::into_inner(lp.geometricMask)
                    } else {
                        rt.PushAxisAlignedClip(
                            &d2d_rect(cell_paint),
                            D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                        );
                        None
                    };

                    let cell_res = (|| -> Result<(), String> {
                        let entry = &state.entries[cell.entry_index];
                        if let Some(ico_box) = cell.icon_box_rect {
                            let ico_paint = margin_inset(ico_box, cell.icon_style.margin);
                            paint_box_fill(rt, res, ico_paint, &cell.icon_style)?;
                            if let Some(icon_r) = cell.icon_rect {
                                let size_px = (f64::from(icon_r.w.max(icon_r.h)) * dpi_scale)
                                    .round()
                                    .clamp(16.0, 512.0)
                                    as u32;
                                let bmp_opt = entry.icon.as_deref().and_then(|spec| {
                                    res.icons.as_mut().and_then(|cache| {
                                        if async_icons {
                                            cache.get_cached_or_queue(rt, spec, size_px, hwnd)
                                        } else {
                                            cache.get_or_load(rt, spec, size_px)
                                        }
                                    })
                                });
                                if let Some(bmp) = bmp_opt {
                                    let ico_radii =
                                        clamp_corner_radii(icon_r, cell.icon_style.border_radius);
                                    let ico_clip_layer = if ico_radii.iter().any(|&r| r > 0.25) {
                                        make_rounded_geometry(&res.d2d, icon_r, ico_radii).map(
                                            |geom| {
                                                let lp = D2D1_LAYER_PARAMETERS {
                                                    contentBounds: d2d_rect(icon_r),
                                                    geometricMask: std::mem::ManuallyDrop::new(
                                                        Some(geom),
                                                    ),
                                                    maskAntialiasMode:
                                                        D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                                                    maskTransform: Matrix3x2::identity(),
                                                    opacity: 1.0,
                                                    opacityBrush: std::mem::ManuallyDrop::new(None),
                                                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                                                };
                                                rt.PushLayer(&lp, None);
                                                std::mem::ManuallyDrop::into_inner(lp.geometricMask)
                                            },
                                        )
                                    } else {
                                        None
                                    };
                                    let is_rectangular_card =
                                        (icon_r.w - icon_r.h).abs() > icon_r.w.min(icon_r.h) * 0.20;
                                    if is_rectangular_card {
                                        draw_bitmap_cover_aligned(
                                            rt,
                                            &bmp,
                                            icon_r,
                                            cell.icon_style.horizontal_align,
                                            cell.icon_style.vertical_align,
                                        );
                                    } else {
                                        draw_bitmap_fitted_aligned(
                                            rt,
                                            &bmp,
                                            icon_r,
                                            cell.icon_style.horizontal_align,
                                            cell.icon_style.vertical_align,
                                        );
                                    }
                                    if let Some(guard) = ico_clip_layer {
                                        rt.PopLayer();
                                        drop(guard);
                                    }
                                    *icons_drawn += 1;
                                } else if let Some(spec) = entry.icon.as_deref() {
                                    if let Some(glyph) = freedesktop_symbolic_glyph(spec) {
                                        draw_fluent_glyph(
                                            rt,
                                            res,
                                            icon_r,
                                            glyph,
                                            cell.icon_style.foreground,
                                            cell.icon_style.horizontal_align,
                                            cell.icon_style.vertical_align,
                                        );
                                    }
                                }
                            }
                            paint_box_border(rt, res, ico_paint, &cell.icon_style)?;
                        }
                        if cell.text_box_rect.w > 1.0 && cell.text_box_rect.h > 1.0 {
                            let txt_paint =
                                margin_inset(cell.text_box_rect, cell.text_style.margin);
                            paint_box_fill(rt, res, txt_paint, &cell.text_style)?;
                            if cell.text_rect.w > 1.0 && cell.text_rect.h > 1.0 {
                                let label = entry.visible_label();
                                let spans = state.match_highlight_ranges(label);
                                let use_markup = state.markup_rows_override.unwrap_or(
                                    cfg.markup_rows
                                        || label.contains("<span")
                                        || label.contains("<i>")
                                        || label.contains("<b>"),
                                );
                                draw_styled_text_ex(
                                    rt,
                                    res,
                                    cell.text_rect,
                                    label,
                                    &cell.text_style,
                                    &spans,
                                    false,
                                    use_markup,
                                    true,
                                )?;
                            }
                            paint_box_border(rt, res, txt_paint, &cell.text_style)?;
                        }
                        Ok(())
                    })();

                    if let Some(geom) = cell_clip_layer {
                        rt.PopLayer();
                        drop(geom);
                    } else {
                        rt.PopAxisAlignedClip();
                    }
                    cell_res?;
                    paint_box_border(rt, res, cell_paint, &cell.row_style)?;
                }

                if let Some(track) = layout.scrollbar_track {
                    let sb_paint = margin_inset(track, layout.scrollbar_style.margin);
                    paint_box_fill(rt, res, sb_paint, &layout.scrollbar_style)?;
                    if let Some(thumb) = layout.scrollbar_thumb {
                        let q_lv = WidgetQuery::new("listview");
                        let q_sb = WidgetQuery::new("scrollbar").with_parent(&q_lv);
                        let handle_col = sheet.get_color(
                            &q_sb,
                            "handle-color",
                            ThemeColor::new(0.65, 0.65, 0.65, 0.9),
                        );
                        if handle_col.a > 0.0 {
                            if let Ok(b) = rt.CreateSolidColorBrush(&color_to_d2d(handle_col), None)
                            {
                                let b_iface: ID2D1Brush = b.cast().map_err(|e| e.to_string())?;
                                let h_radii = if layout
                                    .scrollbar_style
                                    .border_radius
                                    .iter()
                                    .any(|&r| r > 0.25)
                                {
                                    layout.scrollbar_style.border_radius
                                } else {
                                    [(thumb.w * 0.5).min(4.0); 4]
                                };
                                fill_shaped_rect(rt, &res.d2d, thumb, h_radii, &b_iface);
                            }
                        }
                    }
                    paint_box_border(rt, res, sb_paint, &layout.scrollbar_style)?;
                }
            }
            RenderWidgetKind::ModeSwitcher => {
                for btn in &layout.mode_buttons {
                    let btn_paint = margin_inset(btn.rect, btn.style.margin);
                    paint_box_fill(rt, res, btn_paint, &btn.style)?;
                    draw_styled_text_ex(
                        rt,
                        res,
                        btn_paint.inset(
                            btn.style.border[0] + btn.style.padding[0],
                            btn.style.border[1] + btn.style.padding[1],
                            btn.style.border[2] + btn.style.padding[2],
                            btn.style.border[3] + btn.style.padding[3],
                        ),
                        &btn.label,
                        &btn.style,
                        &[],
                        false,
                        false,
                        true,
                    )?;
                    paint_box_border(rt, res, btn_paint, &btn.style)?;
                }
            }
            _ => {
                paint_widget_content(rt, res, w, state, cfg, dpi_scale)?;
            }
        }

        for &child_idx in &w.children_indices {
            paint_node(
                child_idx,
                rt,
                res,
                layout,
                state,
                cfg,
                sheet,
                dpi_scale,
                async_icons,
                hwnd,
                icons_drawn,
            )?;
        }
        Ok(())
    })();

    match clip_kind {
        ClipKind::None => {}
        ClipKind::Axis => rt.PopAxisAlignedClip(),
        ClipKind::Layer(geom) => {
            rt.PopLayer();
            drop(geom);
        }
    }

    inner_res?;
    paint_box_border(rt, res, w.paint_rect, &w.style)?;
    Ok(())
}

fn clamp_corner_radii(rect: Rect, raw: [f32; 4]) -> [f32; 4] {
    let max_r = (rect.w.min(rect.h) * 0.5).max(0.0);
    [
        raw[0].clamp(0.0, max_r),
        raw[1].clamp(0.0, max_r),
        raw[2].clamp(0.0, max_r),
        raw[3].clamp(0.0, max_r),
    ]
}

unsafe fn make_rounded_geometry(
    d2d: &ID2D1Factory,
    rect: Rect,
    radii: [f32; 4],
) -> Option<ID2D1Geometry> {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return None;
    }
    let r = clamp_corner_radii(rect, radii);
    let all_zero = r.iter().all(|&v| v <= 0.25);
    let uniform =
        (r[0] - r[1]).abs() <= 0.25 && (r[1] - r[2]).abs() <= 0.25 && (r[2] - r[3]).abs() <= 0.25;
    if all_zero {
        d2d.CreateRectangleGeometry(&d2d_rect(rect))
            .ok()
            .map(Into::into)
    } else if uniform {
        let rr = D2D1_ROUNDED_RECT {
            rect: d2d_rect(rect),
            radiusX: r[0],
            radiusY: r[0],
        };
        d2d.CreateRoundedRectangleGeometry(&rr).ok().map(Into::into)
    } else {
        build_rounded_path(d2d, rect, r).map(Into::into)
    }
}

unsafe fn build_rounded_path(
    d2d: &ID2D1Factory,
    rect: Rect,
    radii: [f32; 4],
) -> Option<ID2D1PathGeometry> {
    let [tl, tr, br, bl] = clamp_corner_radii(rect, radii);
    let path = d2d.CreatePathGeometry().ok()?;
    let sink = path.Open().ok()?;
    let x = rect.x;
    let y = rect.y;
    let w = rect.w;
    let h = rect.h;

    sink.BeginFigure(D2D_POINT_2F { x: x + tl, y }, D2D1_FIGURE_BEGIN_FILLED);
    sink.AddLine(D2D_POINT_2F { x: x + w - tr, y });
    if tr > 0.25 {
        sink.AddArc(&D2D1_ARC_SEGMENT {
            point: D2D_POINT_2F {
                x: x + w,
                y: y + tr,
            },
            size: D2D_SIZE_F {
                width: tr,
                height: tr,
            },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        });
    }
    sink.AddLine(D2D_POINT_2F {
        x: x + w,
        y: y + h - br,
    });
    if br > 0.25 {
        sink.AddArc(&D2D1_ARC_SEGMENT {
            point: D2D_POINT_2F {
                x: x + w - br,
                y: y + h,
            },
            size: D2D_SIZE_F {
                width: br,
                height: br,
            },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        });
    }
    sink.AddLine(D2D_POINT_2F {
        x: x + bl,
        y: y + h,
    });
    if bl > 0.25 {
        sink.AddArc(&D2D1_ARC_SEGMENT {
            point: D2D_POINT_2F { x, y: y + h - bl },
            size: D2D_SIZE_F {
                width: bl,
                height: bl,
            },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        });
    }
    sink.AddLine(D2D_POINT_2F { x, y: y + tl });
    if tl > 0.25 {
        sink.AddArc(&D2D1_ARC_SEGMENT {
            point: D2D_POINT_2F { x: x + tl, y },
            size: D2D_SIZE_F {
                width: tl,
                height: tl,
            },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        });
    }
    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
    sink.Close().ok()?;
    Some(path)
}

unsafe fn fill_shaped_rect(
    rt: &ID2D1RenderTarget,
    d2d: &ID2D1Factory,
    rect: Rect,
    radii: [f32; 4],
    brush: &ID2D1Brush,
) {
    let all_zero = radii.iter().all(|&r| r <= 0.25);
    let uniform = (radii[0] - radii[1]).abs() <= 0.25
        && (radii[1] - radii[2]).abs() <= 0.25
        && (radii[2] - radii[3]).abs() <= 0.25;
    if all_zero {
        rt.FillRectangle(&d2d_rect(rect), brush);
    } else if uniform {
        let rr = D2D1_ROUNDED_RECT {
            rect: d2d_rect(rect),
            radiusX: radii[0],
            radiusY: radii[0],
        };
        rt.FillRoundedRectangle(&rr, brush);
    } else if let Some(path) = build_rounded_path(d2d, rect, radii) {
        rt.FillGeometry(&path, brush, None);
    } else {
        rt.FillRectangle(&d2d_rect(rect), brush);
    }
}

unsafe fn paint_box_fill(
    rt: &ID2D1RenderTarget,
    res: &mut D2dResources,
    rect: Rect,
    style: &WidgetStyle,
) -> Result<(), String> {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return Ok(());
    }
    let radii = clamp_corner_radii(rect, style.border_radius);
    let any_radius = radii.iter().any(|&r| r > 0.25);
    let uniform_radius = (radii[0] - radii[1]).abs() <= 0.25
        && (radii[1] - radii[2]).abs() <= 0.25
        && (radii[2] - radii[3]).abs() <= 0.25;

    if style.background.a > 0.0 {
        let brush: ID2D1SolidColorBrush = rt
            .CreateSolidColorBrush(&color_to_d2d(style.background), None)
            .map_err(|e| format!("CreateSolidColorBrush failed: {e}"))?;
        let b_iface: ID2D1Brush = brush.cast().map_err(|e| e.to_string())?;
        fill_shaped_rect(rt, &res.d2d, rect, radii, &b_iface);
    }

    if let Some(ImageValue::Url { url, scale }) = &style.background_image {
        let resolved = resolve_image_url_spec(url);
        if let Some(bmp) = res
            .icons
            .as_mut()
            .and_then(|cache| cache.get_or_load_image(rt, &resolved))
        {
            let mask_geom: Option<ID2D1Geometry> = if !any_radius {
                None
            } else if uniform_radius {
                let rr = D2D1_ROUNDED_RECT {
                    rect: d2d_rect(rect),
                    radiusX: radii[0],
                    radiusY: radii[0],
                };
                res.d2d
                    .CreateRoundedRectangleGeometry(&rr)
                    .ok()
                    .map(Into::into)
            } else {
                build_rounded_path(&res.d2d, rect, radii).map(Into::into)
            };
            if let Some(geom) = mask_geom {
                let layer_params = D2D1_LAYER_PARAMETERS {
                    contentBounds: d2d_rect(rect),
                    geometricMask: std::mem::ManuallyDrop::new(Some(geom)),
                    maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    maskTransform: Matrix3x2::identity(),
                    opacity: 1.0,
                    opacityBrush: std::mem::ManuallyDrop::new(None),
                    layerOptions: D2D1_LAYER_OPTIONS_NONE,
                };
                rt.PushLayer(&layer_params, None);
                draw_bitmap_scaled(rt, &bmp, rect, *scale);
                rt.PopLayer();
                let _ = std::mem::ManuallyDrop::into_inner(layer_params.geometricMask);
            } else {
                draw_bitmap_scaled(rt, &bmp, rect, *scale);
            }
        }
    } else if let Some(ImageValue::LinearGradient { dir, colors }) = &style.background_image {
        if colors.len() >= 2 {
            let stops: Vec<D2D1_GRADIENT_STOP> = colors
                .iter()
                .enumerate()
                .map(|(i, &c)| D2D1_GRADIENT_STOP {
                    position: i as f32 / (colors.len() - 1) as f32,
                    color: color_to_d2d(c),
                })
                .collect();
            if let Ok(stop_col) =
                rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
            {
                let (start, end) = match dir {
                    GradientDirection::Right => (
                        D2D_POINT_2F {
                            x: rect.x,
                            y: rect.y,
                        },
                        D2D_POINT_2F {
                            x: rect.x + rect.w,
                            y: rect.y,
                        },
                    ),
                    GradientDirection::Left => (
                        D2D_POINT_2F {
                            x: rect.x + rect.w,
                            y: rect.y,
                        },
                        D2D_POINT_2F {
                            x: rect.x,
                            y: rect.y,
                        },
                    ),
                    GradientDirection::Bottom => (
                        D2D_POINT_2F {
                            x: rect.x,
                            y: rect.y,
                        },
                        D2D_POINT_2F {
                            x: rect.x,
                            y: rect.y + rect.h,
                        },
                    ),
                    GradientDirection::Top => (
                        D2D_POINT_2F {
                            x: rect.x,
                            y: rect.y + rect.h,
                        },
                        D2D_POINT_2F {
                            x: rect.x,
                            y: rect.y,
                        },
                    ),
                    GradientDirection::Angle(deg) => {
                        let rad = (*deg as f32).to_radians();
                        let cx = rect.x + rect.w * 0.5;
                        let cy = rect.y + rect.h * 0.5;
                        let ox = rad.cos() * rect.w * 0.5;
                        let oy = rad.sin() * rect.h * 0.5;
                        (
                            D2D_POINT_2F {
                                x: cx - ox,
                                y: cy - oy,
                            },
                            D2D_POINT_2F {
                                x: cx + ox,
                                y: cy + oy,
                            },
                        )
                    }
                };
                let props =
                    windows::Win32::Graphics::Direct2D::D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: start,
                        endPoint: end,
                    };
                if let Ok(gbrush) = rt.CreateLinearGradientBrush(&props, None, &stop_col) {
                    let b_iface: ID2D1Brush = gbrush.cast().map_err(|e| e.to_string())?;
                    fill_shaped_rect(rt, &res.d2d, rect, radii, &b_iface);
                }
            }
        }
    }
    Ok(())
}

unsafe fn paint_box_border(
    rt: &ID2D1RenderTarget,
    res: &D2dResources,
    rect: Rect,
    style: &WidgetStyle,
) -> Result<(), String> {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return Ok(());
    }
    let radii = clamp_corner_radii(rect, style.border_radius);
    let any_radius = radii.iter().any(|&r| r > 0.25);
    let uniform_radius = (radii[0] - radii[1]).abs() <= 0.25
        && (radii[1] - radii[2]).abs() <= 0.25
        && (radii[2] - radii[3]).abs() <= 0.25;

    if style.border_color.a > 0.0 && style.border.iter().any(|&b| b > 0.0) {
        let brush: ID2D1SolidColorBrush = rt
            .CreateSolidColorBrush(&color_to_d2d(style.border_color), None)
            .map_err(|e| format!("CreateSolidColorBrush failed: {e}"))?;
        let uniform = style.border[0] == style.border[1]
            && style.border[1] == style.border[2]
            && style.border[2] == style.border[3];
        let any_dashed = style
            .border
            .iter()
            .zip(style.border_styles.iter())
            .any(|(&bw, &st)| bw > 0.0 && st == LineStyle::Dash);
        if uniform {
            let bw = style.border[0];
            let stroke = (style.border_styles[0] == LineStyle::Dash).then_some(&res.dashed_stroke);
            let inset = bw * 0.5;
            let inset_rect = rect.inset(inset, inset, inset, inset);
            let inset_radii = clamp_corner_radii(
                inset_rect,
                [
                    (radii[0] - inset).max(0.0),
                    (radii[1] - inset).max(0.0),
                    (radii[2] - inset).max(0.0),
                    (radii[3] - inset).max(0.0),
                ],
            );
            if !any_radius {
                rt.DrawRectangle(&d2d_rect(inset_rect), &brush, bw, stroke);
            } else if uniform_radius {
                let rr = D2D1_ROUNDED_RECT {
                    rect: d2d_rect(inset_rect),
                    radiusX: inset_radii[0],
                    radiusY: inset_radii[0],
                };
                rt.DrawRoundedRectangle(&rr, &brush, bw, stroke);
            } else if let Some(path) = build_rounded_path(&res.d2d, inset_rect, inset_radii) {
                rt.DrawGeometry(&path, &brush, bw, stroke);
            }
        } else if any_radius && !any_dashed {
            let [bt, br, bb, bl] = style.border;
            let active_edges =
                (bt > 0.0) as u8 + (br > 0.0) as u8 + (bb > 0.0) as u8 + (bl > 0.0) as u8;
            let drew_ring = if let Some(outer_geom) = make_rounded_geometry(&res.d2d, rect, radii) {
                if active_edges == 1 {
                    let edge_rect = if bt > 0.0 {
                        Rect::new(rect.x, rect.y, rect.w, bt.min(rect.h))
                    } else if br > 0.0 {
                        let bw = br.min(rect.w);
                        Rect::new(rect.x + rect.w - bw, rect.y, bw, rect.h)
                    } else if bb > 0.0 {
                        let bw = bb.min(rect.h);
                        Rect::new(rect.x, rect.y + rect.h - bw, rect.w, bw)
                    } else {
                        Rect::new(rect.x, rect.y, bl.min(rect.w), rect.h)
                    };
                    if let Ok(strip_geom) = res.d2d.CreateRectangleGeometry(&d2d_rect(edge_rect)) {
                        if let Ok(ring) = res.d2d.CreatePathGeometry() {
                            if let Ok(sink) = ring.Open() {
                                let ok = outer_geom
                                    .CombineWithGeometry(
                                        &strip_geom,
                                        D2D1_COMBINE_MODE_INTERSECT,
                                        None,
                                        0.25,
                                        &sink,
                                    )
                                    .is_ok()
                                    && sink.Close().is_ok();
                                if ok {
                                    rt.FillGeometry(&ring, &brush, None);
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    // Only push a 0-border side outward when both of its adjacent corners are
                    // square (e.g. open-bottom/open-top folder tabs), keeping smooth shifted-arc
                    // curves on opposite 2-sided rounded borders (`widget.c:270-360`).
                    let pad_out = 2.0f32;
                    let inner_rect = rect.inset(
                        if bt > 0.0 {
                            bt
                        } else if radii[0] <= 0.25 && radii[1] <= 0.25 {
                            -pad_out
                        } else {
                            0.0
                        },
                        if br > 0.0 {
                            br
                        } else if radii[1] <= 0.25 && radii[2] <= 0.25 {
                            -pad_out
                        } else {
                            0.0
                        },
                        if bb > 0.0 {
                            bb
                        } else if radii[2] <= 0.25 && radii[3] <= 0.25 {
                            -pad_out
                        } else {
                            0.0
                        },
                        if bl > 0.0 {
                            bl
                        } else if radii[3] <= 0.25 && radii[0] <= 0.25 {
                            -pad_out
                        } else {
                            0.0
                        },
                    );
                    let inner_radii = [
                        (radii[0] - bl.min(bt)).max(0.0),
                        (radii[1] - br.min(bt)).max(0.0),
                        (radii[2] - br.min(bb)).max(0.0),
                        (radii[3] - bl.min(bb)).max(0.0),
                    ];
                    if inner_rect.w <= 0.25 || inner_rect.h <= 0.25 {
                        rt.FillGeometry(&outer_geom, &brush, None);
                        true
                    } else if let Some(inner_geom) =
                        make_rounded_geometry(&res.d2d, inner_rect, inner_radii)
                    {
                        if let Ok(ring) = res.d2d.CreatePathGeometry() {
                            if let Ok(sink) = ring.Open() {
                                let ok = outer_geom
                                    .CombineWithGeometry(
                                        &inner_geom,
                                        D2D1_COMBINE_MODE_EXCLUDE,
                                        None,
                                        0.25,
                                        &sink,
                                    )
                                    .is_ok()
                                    && sink.Close().is_ok();
                                if ok {
                                    rt.FillGeometry(&ring, &brush, None);
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            } else {
                false
            };
            if !drew_ring {
                let b_iface: ID2D1Brush = brush.cast().map_err(|e| e.to_string())?;
                fill_shaped_rect(rt, &res.d2d, rect, radii, &b_iface);
            }
        } else {
            // Top, Right, Bottom, Left individual border edges (e.g. `border: 2px dash 0px 0px`)
            let edges = [
                (
                    D2D_POINT_2F {
                        x: rect.x,
                        y: rect.y + style.border[0] * 0.5,
                    },
                    D2D_POINT_2F {
                        x: rect.x + rect.w,
                        y: rect.y + style.border[0] * 0.5,
                    },
                    style.border[0],
                    style.border_styles[0],
                ),
                (
                    D2D_POINT_2F {
                        x: rect.x + rect.w - style.border[1] * 0.5,
                        y: rect.y,
                    },
                    D2D_POINT_2F {
                        x: rect.x + rect.w - style.border[1] * 0.5,
                        y: rect.y + rect.h,
                    },
                    style.border[1],
                    style.border_styles[1],
                ),
                (
                    D2D_POINT_2F {
                        x: rect.x,
                        y: rect.y + rect.h - style.border[2] * 0.5,
                    },
                    D2D_POINT_2F {
                        x: rect.x + rect.w,
                        y: rect.y + rect.h - style.border[2] * 0.5,
                    },
                    style.border[2],
                    style.border_styles[2],
                ),
                (
                    D2D_POINT_2F {
                        x: rect.x + style.border[3] * 0.5,
                        y: rect.y,
                    },
                    D2D_POINT_2F {
                        x: rect.x + style.border[3] * 0.5,
                        y: rect.y + rect.h,
                    },
                    style.border[3],
                    style.border_styles[3],
                ),
            ];
            for (p0, p1, bw, st) in edges {
                if bw > 0.0 {
                    let stroke = (st == LineStyle::Dash).then_some(&res.dashed_stroke);
                    rt.DrawLine(p0, p1, &brush, bw, stroke);
                }
            }
        }
    }
    let _ = ID2D1LinearGradientBrush::IID;
    Ok(())
}

unsafe fn draw_fluent_glyph(
    rt: &ID2D1RenderTarget,
    res: &D2dResources,
    rect: Rect,
    glyph: char,
    color: ThemeColor,
    xalign: f32,
    yalign: f32,
) {
    if rect.w <= 2.0 || rect.h <= 2.0 {
        return;
    }
    let Some(font_family) = &res.fluent_icon_font else {
        return;
    };
    // GTK symbolic icons (e.g. `search-symbolic`) include ~15% built-in viewBox margin.
    let side = rect.w.min(rect.h).max(8.0);
    let px_size = (side * 0.85).clamp(9.0, 128.0);
    let locale_h = HSTRING::from("en-us");
    let Ok(fmt) = res.dwrite.CreateTextFormat(
        font_family,
        None,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        px_size,
        &locale_h,
    ) else {
        return;
    };
    if xalign < 0.25 {
        let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
    } else if xalign > 0.75 {
        let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
    } else {
        let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
    }
    if yalign < 0.25 {
        let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
    } else if yalign > 0.75 {
        let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_FAR);
    } else {
        let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    }
    let mut buf = [0u16; 2];
    let utf16 = glyph.encode_utf16(&mut buf);
    let Ok(layout) = res.dwrite.CreateTextLayout(utf16, &fmt, rect.w, rect.h) else {
        return;
    };
    let Ok(brush) = rt.CreateSolidColorBrush(&color_to_d2d(color), None) else {
        return;
    };
    rt.DrawTextLayout(
        D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        },
        &layout,
        &brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
    );
}

unsafe fn paint_widget_content(
    rt: &ID2D1RenderTarget,
    res: &mut D2dResources,
    w: &LaidOutWidget,
    state: &LauncherState,
    _cfg: &Config,
    dpi_scale: f64,
) -> Result<(), String> {
    match &w.kind {
        RenderWidgetKind::Prompt => {
            draw_styled_text(rt, res, w.inner_rect, &state.prompt, &w.style, &[], false)?;
        }
        RenderWidgetKind::Entry => {
            let display = state.display_query();
            let show_caret = !(display.is_empty() && w.style.hide_cursor_on_empty);
            // Leave a minimal 1.5px left caret gutter only when a left-aligned caret is shown
            // so the vertical bar does not collide with the first glyph without inflating spacing.
            let left_gutter =
                if show_caret && w.style.horizontal_align < 0.25 && w.inner_rect.w > 12.0 {
                    1.5
                } else {
                    0.0
                };
            let text_rect = Rect::new(
                w.inner_rect.x + left_gutter,
                w.inner_rect.y,
                (w.inner_rect.w - left_gutter).max(0.0),
                w.inner_rect.h,
            );
            if display.is_empty() && !w.style.placeholder.is_empty() {
                let mut ph_style = w.style.clone();
                ph_style.foreground = w.style.placeholder_color;
                draw_styled_text(
                    rt,
                    res,
                    text_rect,
                    &w.style.placeholder,
                    &ph_style,
                    &[],
                    false,
                )?;
            } else {
                draw_styled_text(rt, res, text_rect, &display, &w.style, &[], false)?;
            }
            if show_caret {
                // Draw text caret at cursor_byte sized to typographic em-height (not inflated WinAscent)
                let caret_byte = if state.password {
                    state.query[..state.cursor_byte.min(state.query.len())]
                        .chars()
                        .count()
                } else {
                    state.cursor_byte.min(display.len())
                };
                let (caret_x, line_h) = measure_caret_geometry(
                    res,
                    &w.style.font,
                    w.inner_rect,
                    text_rect,
                    &display,
                    &w.style.placeholder,
                    caret_byte,
                    w.style.horizontal_align,
                );
                let em_px = (parse_rofi_font(&w.style.font).size_pt * 96.0 / 72.0).max(8.0);
                let target_h = line_h.min((em_px * 1.08).round());
                let caret_h = target_h.clamp(8.0, (w.inner_rect.h - 2.0).max(8.0));
                let valign = if w.style.vertical_align < 0.25 {
                    0.0
                } else if w.style.vertical_align > 0.75 {
                    1.0
                } else {
                    0.5
                };
                let top = w.inner_rect.y + (w.inner_rect.h - caret_h).max(0.0) * valign;
                let caret_col = if w.style.cursor_color.a > 0.0 {
                    w.style.cursor_color
                } else {
                    w.style.foreground
                };
                if let Ok(caret_brush) = rt.CreateSolidColorBrush(&color_to_d2d(caret_col), None) {
                    rt.DrawLine(
                        D2D_POINT_2F { x: caret_x, y: top },
                        D2D_POINT_2F {
                            x: caret_x,
                            y: top + caret_h,
                        },
                        &caret_brush,
                        w.style.cursor_width.clamp(1.0, 6.0),
                        None,
                    );
                }
            }
        }
        RenderWidgetKind::CaseIndicator => {
            draw_styled_text(
                rt,
                res,
                w.inner_rect,
                state.case_indicator_str(),
                &w.style,
                &[],
                false,
            )?;
        }
        RenderWidgetKind::NumFilteredRows => {
            let txt = state.filtered.len().to_string();
            draw_styled_text(rt, res, w.inner_rect, &txt, &w.style, &[], false)?;
        }
        RenderWidgetKind::NumRows => {
            let txt = state.entries.len().to_string();
            draw_styled_text(rt, res, w.inner_rect, &txt, &w.style, &[], false)?;
        }
        RenderWidgetKind::StaticTextbox(text) => {
            draw_styled_text(rt, res, w.inner_rect, text, &w.style, &[], false)?;
        }
        RenderWidgetKind::CurrentEntryTextbox => {
            let selected_lbl = state
                .filtered
                .get(state.selected)
                .map(|&idx| state.entries[idx].visible_label().to_string())
                .unwrap_or_default();
            if selected_lbl.is_empty() && !w.style.placeholder.is_empty() {
                let mut ph_style = w.style.clone();
                ph_style.foreground = w.style.placeholder_color;
                draw_styled_text_ex(
                    rt,
                    res,
                    w.inner_rect,
                    &w.style.placeholder,
                    &ph_style,
                    &[],
                    false,
                    false,
                    true,
                )?;
            } else if !selected_lbl.is_empty() {
                draw_styled_text_ex(
                    rt,
                    res,
                    w.inner_rect,
                    &selected_lbl,
                    &w.style,
                    &[],
                    false,
                    false,
                    true,
                )?;
            }
        }
        RenderWidgetKind::Message | RenderWidgetKind::MessageTextbox => {
            if let Some(msg) = &state.message {
                draw_styled_text_ex(rt, res, w.inner_rect, msg, &w.style, &[], true, true, false)?;
            }
        }
        RenderWidgetKind::Icon(spec) => {
            let resolved_spec = match spec.as_deref() {
                Some("current-entry" | "icon-current-entry") => state
                    .filtered
                    .get(state.selected)
                    .and_then(|&idx| state.entries[idx].icon.as_deref()),
                Some(s) if !s.is_empty() => Some(s),
                _ => None,
            };
            let size_px = (f64::from(w.inner_rect.w.max(w.inner_rect.h)) * dpi_scale)
                .round()
                .clamp(16.0, 1024.0) as u32;
            let bmp_opt = resolved_spec.and_then(|s| {
                res.icons
                    .as_mut()
                    .and_then(|cache| cache.get_or_load(rt, s, size_px))
            });
            if let Some(bmp) = bmp_opt {
                draw_bitmap_fitted_aligned(
                    rt,
                    &bmp,
                    w.inner_rect,
                    w.style.horizontal_align,
                    w.style.vertical_align,
                );
            } else if let Some(spec_name) = resolved_spec {
                if let Some(glyph) = freedesktop_symbolic_glyph(spec_name) {
                    draw_fluent_glyph(
                        rt,
                        res,
                        w.inner_rect,
                        glyph,
                        w.style.foreground,
                        w.style.horizontal_align,
                        w.style.vertical_align,
                    );
                }
            }
        }
        RenderWidgetKind::Overlay => {
            if let Some(ov) = &state.overlay {
                draw_styled_text_ex(rt, res, w.inner_rect, ov, &w.style, &[], false, false, true)?;
            }
        }
        RenderWidgetKind::Box | RenderWidgetKind::ListView | RenderWidgetKind::ModeSwitcher => {}
    }
    Ok(())
}

unsafe fn apply_nerd_font_pua_ranges(
    layout: &IDWriteTextLayout,
    text: &str,
    nf_family: Option<&HSTRING>,
) {
    let Some(nf_family) = nf_family else {
        return;
    };
    let mut u16_pos = 0u32;
    let mut run_start: Option<u32> = None;
    let mut run_len = 0u32;
    for ch in text.chars() {
        let ch_u16 = ch.len_utf16() as u32;
        let is_pua =
            ('\u{e000}'..='\u{f8ff}').contains(&ch) || ('\u{f0000}'..='\u{ffffd}').contains(&ch);
        if is_pua {
            if run_start.is_none() {
                run_start = Some(u16_pos);
                run_len = 0;
            }
            run_len += ch_u16;
        } else if let Some(start) = run_start.take() {
            let _ = layout.SetFontFamilyName(
                nf_family,
                DWRITE_TEXT_RANGE {
                    startPosition: start,
                    length: run_len,
                },
            );
        }
        u16_pos += ch_u16;
    }
    if let Some(start) = run_start {
        let _ = layout.SetFontFamilyName(
            nf_family,
            DWRITE_TEXT_RANGE {
                startPosition: start,
                length: run_len,
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn measure_caret_geometry(
    res: &D2dResources,
    font: &str,
    outer_rect: Rect,
    text_rect: Rect,
    display_text: &str,
    placeholder: &str,
    byte_pos: usize,
    horizontal_align: f32,
) -> (f32, f32) {
    let min_x = outer_rect.x + 0.75;
    let max_x = (outer_rect.x + outer_rect.w - 1.5).max(min_x);
    let align_factor = if horizontal_align < 0.25 {
        0.0
    } else if horizontal_align > 0.75 {
        1.0
    } else {
        0.5
    };
    let (_, line_h) = res.measure_text_span(font, "Mg");
    if display_text.is_empty() {
        if !placeholder.is_empty() {
            let (ph_w, ph_h) = res.measure_text_span(font, placeholder);
            let origin_x = text_rect.x + (text_rect.w - ph_w).max(0.0) * align_factor;
            let gap = if align_factor > 0.25 { 2.5 } else { 1.5 };
            return ((origin_x - gap).clamp(min_x, max_x), ph_h.max(line_h));
        }
        let x = text_rect.x + (text_rect.w - 2.0).max(0.0) * align_factor;
        return (x.clamp(min_x, max_x), line_h);
    }
    let (total_w, txt_h) = res.measure_text_span(font, display_text);
    let origin_x = text_rect.x + (text_rect.w - total_w).max(0.0) * align_factor;
    if byte_pos == 0 {
        return ((origin_x - 2.5).clamp(min_x, max_x), txt_h.max(line_h));
    }
    let mut end = byte_pos.min(display_text.len());
    while end > 0 && !display_text.is_char_boundary(end) {
        end -= 1;
    }
    let (prefix_w, _) = res.measure_text_span(font, &display_text[..end]);
    ((origin_x + prefix_w).clamp(min_x, max_x), txt_h.max(line_h))
}

unsafe fn draw_styled_text(
    rt: &ID2D1RenderTarget,
    res: &D2dResources,
    rect: Rect,
    text: &str,
    style: &WidgetStyle,
    highlight_byte_ranges: &[(usize, usize)],
    wrap: bool,
) -> Result<(), String> {
    draw_styled_text_ex(
        rt,
        res,
        rect,
        text,
        style,
        highlight_byte_ranges,
        wrap,
        false,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_styled_text_ex(
    rt: &ID2D1RenderTarget,
    res: &D2dResources,
    rect: Rect,
    text: &str,
    style: &WidgetStyle,
    highlight_byte_ranges: &[(usize, usize)],
    wrap: bool,
    markup: bool,
    ellipsize: bool,
) -> Result<(), String> {
    if rect.w <= 1.0 || rect.h <= 1.0 || text.is_empty() {
        return Ok(());
    }
    let (plain_owned, pango_spans) = if markup && (text.contains('<') || text.contains('&')) {
        parse_pango_markup(text)
    } else {
        (String::new(), Vec::new())
    };
    let effective_text = if pango_spans.is_empty() && plain_owned.is_empty() {
        text
    } else {
        plain_owned.as_str()
    };
    if effective_text.is_empty() {
        return Ok(());
    }

    let fmt = res.create_text_format(&style.font)?;
    if style.horizontal_align < 0.25 {
        let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
    } else if style.horizontal_align > 0.75 {
        let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
    } else {
        let _ = fmt.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
    }
    if style.vertical_align < 0.25 {
        let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
    } else if style.vertical_align > 0.75 {
        let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_FAR);
    } else {
        let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
    }
    let _ = fmt.SetWordWrapping(if wrap {
        DWRITE_WORD_WRAPPING_WRAP
    } else {
        DWRITE_WORD_WRAPPING_NO_WRAP
    });
    if ellipsize {
        let trimming = DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            delimiter: 0,
            delimiterCount: 0,
        };
        if let Ok(ellipsis) = res.dwrite.CreateEllipsisTrimmingSign(&fmt) {
            let _ = fmt.SetTrimming(&trimming, Some(&ellipsis));
        }
    }

    let utf16: Vec<u16> = effective_text.encode_utf16().collect();
    let layout: IDWriteTextLayout = res
        .dwrite
        .CreateTextLayout(&utf16, &fmt, rect.w, rect.h)
        .map_err(|e| format!("CreateTextLayout failed: {e}"))?;

    apply_nerd_font_pua_ranges(&layout, effective_text, res.nerd_font_family.as_ref());

    if !pango_spans.is_empty() {
        let base_pt = parse_rofi_font(&style.font).size_pt;
        for sp in &pango_spans {
            if sp.byte_start >= sp.byte_end || sp.byte_end > effective_text.len() {
                continue;
            }
            let u16_start = effective_text[..sp.byte_start].encode_utf16().count() as u32;
            let u16_len = effective_text[sp.byte_start..sp.byte_end]
                .encode_utf16()
                .count() as u32;
            let range = DWRITE_TEXT_RANGE {
                startPosition: u16_start,
                length: u16_len,
            };
            if sp.bold {
                let _ = layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range);
            } else if sp.light {
                let _ = layout.SetFontWeight(DWRITE_FONT_WEIGHT_LIGHT, range);
            }
            if sp.italic {
                let _ = layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range);
            }
            if sp.underline {
                let _ = layout.SetUnderline(true, range);
            }
            if sp.strikethrough {
                let _ = layout.SetStrikethrough(true, range);
            }
            if let Some(sc) = sp.size_scale {
                let _ = layout.SetFontSize((base_pt * sc * (96.0 / 72.0)).max(8.0), range);
            }
            if let Some(col) = sp.color {
                if let Ok(br) = rt.CreateSolidColorBrush(&color_to_d2d(col), None) {
                    let _ = layout.SetDrawingEffect(&br, range);
                }
            }
        }
    }

    if !highlight_byte_ranges.is_empty() && pango_spans.is_empty() {
        apply_highlight_ranges(
            rt,
            &layout,
            effective_text,
            highlight_byte_ranges,
            style.highlight,
        );
    }

    let fg_brush = rt
        .CreateSolidColorBrush(&color_to_d2d(style.foreground), None)
        .map_err(|e| format!("CreateSolidColorBrush failed: {e}"))?;
    rt.DrawTextLayout(
        D2D_POINT_2F {
            x: rect.x,
            y: rect.y,
        },
        &layout,
        &fg_brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
    );
    Ok(())
}

fn query_windows_wallpaper_path() -> Option<String> {
    let mut buf = [0u16; 1024];
    // SAFETY: Standard Win32 SystemParametersInfoW(SPI_GETDESKWALLPAPER) into UTF-16 buffer.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buf.len() as u32,
            Some(buf.as_mut_ptr() as *mut std::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    if !ok {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path = String::from_utf16_lossy(&buf[..len]);
    let trimmed = path.trim();
    if !trimmed.is_empty() && std::path::Path::new(trimmed).is_file() {
        return Some(trimmed.to_string());
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        let transcoded = std::path::PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Themes")
            .join("TranscodedWallpaper");
        if transcoded.is_file() {
            return Some(transcoded.to_string_lossy().into_owned());
        }
    }
    None
}

fn resolve_image_url_spec(url: &str) -> String {
    let expanded = expand_tilde_str(url);
    if std::path::Path::new(&expanded).is_file() {
        return expanded;
    }
    let normalized = url.trim().replace('\\', "/");
    if let Some(idx) = normalized.to_ascii_lowercase().find(".config/rofi/") {
        let sub = normalized[idx + ".config/rofi/".len()..].trim_start_matches('/');
        if !sub.is_empty() {
            if let Ok(appdata) = std::env::var("APPDATA") {
                let base = std::path::PathBuf::from(&appdata).join("LiteRofi");
                for root in [&base, &base.join("themes")] {
                    let cand = root.join(sub);
                    if cand.is_file() {
                        return cand.to_string_lossy().into_owned();
                    }
                }
            }
        }
    }
    let lower = url.to_ascii_lowercase();
    if lower.contains("wallpaper") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let base = std::path::PathBuf::from(&appdata).join("LiteRofi");
            for name in [
                ".current_wallpaper",
                ".current_wallpaper.jpg",
                ".current_wallpaper.png",
                ".current_wallpaper.webp",
                ".current_wallpaper.bmp",
            ] {
                let cand = base.join(name);
                if cand.is_file() {
                    return cand.to_string_lossy().into_owned();
                }
            }
        }
        if let Some(wp) = query_windows_wallpaper_path() {
            return wp;
        }
    }
    expanded
}

fn expand_tilde_str(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        for key in ["USERPROFILE", "HOME"] {
            if let Ok(home) = std::env::var(key) {
                if !home.trim().is_empty() {
                    return std::path::PathBuf::from(home)
                        .join(rest)
                        .to_string_lossy()
                        .into_owned();
                }
            }
        }
    }
    trimmed.to_string()
}

unsafe fn apply_highlight_ranges(
    rt: &ID2D1RenderTarget,
    layout: &IDWriteTextLayout,
    text: &str,
    byte_ranges: &[(usize, usize)],
    hl: HighlightColorStyle,
) {
    let hl_brush = if hl.style & highlight_flags::COLOR != 0 {
        rt.CreateSolidColorBrush(&color_to_d2d(hl.color), None).ok()
    } else {
        None
    };

    for &(bs, be) in byte_ranges {
        if bs >= be || be > text.len() || !text.is_char_boundary(bs) || !text.is_char_boundary(be) {
            continue;
        }
        let u16_start = text[..bs].encode_utf16().count() as u32;
        let u16_len = text[bs..be].encode_utf16().count() as u32;
        let range = DWRITE_TEXT_RANGE {
            startPosition: u16_start,
            length: u16_len,
        };
        if hl.style & highlight_flags::BOLD != 0 {
            let _ = layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range);
        }
        if hl.style & highlight_flags::ITALIC != 0 {
            let _ = layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range);
        }
        if hl.style & highlight_flags::UNDERLINE != 0 {
            let _ = layout.SetUnderline(true, range);
        }
        if hl.style & highlight_flags::STRIKETHROUGH != 0 {
            let _ = layout.SetStrikethrough(true, range);
        }
        if let Some(brush) = &hl_brush {
            let _ = layout.SetDrawingEffect(brush, range);
        }
    }
}

fn current_modifiers() -> (bool, bool, bool, bool) {
    // SAFETY: `GetKeyState` reads current thread key state safely with VK constants.
    unsafe {
        let ctrl = (GetKeyState(i32::from(VK_CONTROL.0)) & -0x8000i16) != 0;
        let alt = (GetKeyState(i32::from(VK_MENU.0)) & -0x8000i16) != 0;
        let shift = (GetKeyState(i32::from(VK_SHIFT.0)) & -0x8000i16) != 0;
        let win = (GetKeyState(i32::from(VK_LWIN.0)) & -0x8000i16) != 0
            || (GetKeyState(i32::from(VK_RWIN.0)) & -0x8000i16) != 0;
        (ctrl, alt, shift, win)
    }
}

fn vk_matches_rofi_key(vk: u16, name: &str) -> bool {
    match name.to_ascii_lowercase().as_str() {
        "return" | "kp_enter" | "enter" => vk == VK_RETURN.0,
        "escape" | "esc" => vk == VK_ESCAPE.0,
        "backspace" => vk == VK_BACK.0,
        "delete" => vk == VK_DELETE.0,
        "tab" => vk == VK_TAB.0,
        "iso_left_tab" => vk == VK_TAB.0,
        "up" => vk == VK_UP.0,
        "down" => vk == VK_DOWN.0,
        "left" => vk == VK_LEFT.0,
        "right" => vk == VK_RIGHT.0,
        "page_up" | "prior" => vk == VK_PRIOR.0,
        "page_down" | "next" => vk == VK_NEXT.0,
        "home" | "kp_home" => vk == VK_HOME.0,
        "end" | "kp_end" => vk == VK_END.0,
        "insert" => vk == VK_INSERT.0,
        "space" => vk == b' ' as u16,
        "grave" | "dead_grave" => vk == 0xC0, // VK_OEM_3
        "period" => vk == 0xBE,               // VK_OEM_PERIOD
        "equal" => vk == 0xBB,                // VK_OEM_PLUS
        "minus" => vk == 0xBD,                // VK_OEM_MINUS
        "bracketleft" => vk == 0xDB,          // VK_OEM_4
        s if s.len() == 1 => {
            let b = s.as_bytes()[0];
            if b.is_ascii_alphanumeric() {
                vk == u16::from(b.to_ascii_uppercase())
            } else {
                false
            }
        }
        _ => false,
    }
}

fn match_keybinding(cfg: &Config, vk: u16) -> Option<KeyBindingAction> {
    let (ctrl, alt, shift, win) = current_modifiers();
    // Prioritize custom actions & bindings from Config::keybindings
    for entry in cfg.keybindings.entries() {
        for chord in parse_chord_list(&entry.binding) {
            let want_shift =
                chord.modifiers.shift || chord.key.eq_ignore_ascii_case("iso_left_tab");
            if chord.modifiers.control == ctrl
                && chord.modifiers.alt == alt
                && want_shift == shift
                && chord.modifiers.super_key == win
                && vk_matches_rofi_key(vk, &chord.key)
            {
                return Some(entry.action);
            }
        }
    }
    // Standard fallback navigation keys regardless of subtle chord differences
    if !ctrl && !alt && !shift && !win {
        match vk {
            x if x == VK_UP.0 => return Some(KeyBindingAction::RowUp),
            x if x == VK_DOWN.0 => return Some(KeyBindingAction::RowDown),
            x if x == VK_LEFT.0 => return Some(KeyBindingAction::MoveCharBack),
            x if x == VK_RIGHT.0 => return Some(KeyBindingAction::MoveCharForward),
            x if x == VK_PRIOR.0 => return Some(KeyBindingAction::PagePrev),
            x if x == VK_NEXT.0 => return Some(KeyBindingAction::PageNext),
            x if x == VK_HOME.0 => return Some(KeyBindingAction::MoveFront),
            x if x == VK_END.0 => return Some(KeyBindingAction::MoveEnd),
            x if x == VK_BACK.0 => return Some(KeyBindingAction::RemoveCharBack),
            x if x == VK_DELETE.0 => return Some(KeyBindingAction::RemoveCharForward),
            x if x == VK_RETURN.0 => return Some(KeyBindingAction::AcceptEntry),
            x if x == VK_ESCAPE.0 => return Some(KeyBindingAction::Cancel),
            _ => {}
        }
    }
    None
}

fn read_clipboard_text(hwnd: HWND) -> Option<String> {
    // SAFETY: Standard Win32 OpenClipboard / GetClipboardData / CloseClipboard sequence.
    unsafe {
        if OpenClipboard(hwnd).is_err() {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT_U32).ok();
        let text = handle.and_then(|h| {
            let hglobal = windows::Win32::Foundation::HGLOBAL(h.0);
            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                return None;
            }
            let wstr = ptr as *const u16;
            let mut len = 0usize;
            while *wstr.add(len) != 0 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(wstr, len);
            let s = String::from_utf16_lossy(slice);
            let _ = GlobalUnlock(hglobal);
            Some(s)
        });
        let _ = CloseClipboard();
        text
    }
}

fn write_clipboard_text(hwnd: HWND, text: &str) {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = utf16.len() * std::mem::size_of::<u16>();
    // SAFETY: Standard Win32 clipboard allocation and ownership transfer on `SetClipboardData`.
    unsafe {
        if OpenClipboard(hwnd).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
            let dst = GlobalLock(hmem) as *mut u16;
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(utf16.as_ptr(), dst, utf16.len());
                let _ = GlobalUnlock(hmem);
                let handle = windows::Win32::Foundation::HANDLE(hmem.0);
                let _ = SetClipboardData(CF_UNICODETEXT_U32, handle);
            }
        }
        let _ = CloseClipboard();
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCCREATE {
        let cs = lparam.0 as *const CREATESTRUCTW;
        if !cs.is_null() {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
        }
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<WindowCtx>;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ctx_rc = &*ptr;

    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => LRESULT(0),
        WM_DPICHANGED => {
            let new_dpi = (wparam.0 & 0xFFFF).max(96) as f64;
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                let mon_m = query_active_monitor();
                ctx.monitor = mon_m;
                ctx.metrics =
                    ctx.res
                        .measure_sheet_font(&ctx.sheet, &ctx.cfg.font, new_dpi, mon_m.mon_dip);
                ctx.relayout(hwnd);
                let _ = ctx.paint(hwnd);
            }
            LRESULT(0)
        }
        m if m == WM_APP_ICON_READY => {
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                let (_, _, pw, ph) = dip_rect_to_phys(
                    ctx.layout.window_rect,
                    ctx.monitor.phys_origin,
                    ctx.metrics.dpi,
                );
                let dpi = ctx.metrics.dpi;
                if let Ok((dc_rt, _)) =
                    ctx.res
                        .ensure_dc_rt(pw.max(1) as u32, ph.max(1) as u32, dpi)
                {
                    if let Ok(rt) = dc_rt.cast::<ID2D1RenderTarget>() {
                        if ctx
                            .res
                            .icons
                            .as_mut()
                            .is_some_and(|cache| cache.drain_async_ready(&rt))
                            && IsWindowVisible(hwnd).as_bool()
                        {
                            let _ = ctx.paint(hwnd);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let _ = ValidateRect(hwnd, None);
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wparam.0 as u16;
            let mut should_quit = false;
            let mut handled = false;
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                let (ctrl, alt, shift, _) = current_modifiers();
                let action_opt = match_keybinding(&ctx.cfg, vk).or((alt
                    && !ctrl
                    && !shift
                    && vk == b'A' as u16)
                    .then_some(KeyBindingAction::AcceptAlt));
                if let Some(action) = action_opt {
                    handled = true;
                    match action {
                        KeyBindingAction::PastePrimary | KeyBindingAction::PasteSecondary => {
                            if let Some(clip) = read_clipboard_text(hwnd) {
                                ctx.state.insert_str(&clip);
                            }
                        }
                        KeyBindingAction::CopySecondary => {
                            let clip = ctx
                                .state
                                .filtered
                                .get(ctx.state.selected)
                                .map(|&i| ctx.state.entries[i].text.as_str())
                                .unwrap_or(&ctx.state.query)
                                .to_string();
                            write_clipboard_text(hwnd, &clip);
                        }
                        other => {
                            let cols = ctx.layout.listview_columns;
                            let lines = ctx.layout.listview_lines;
                            let no_entry =
                                ctx.layout.entry_rect.w <= 1.0 || ctx.layout.entry_rect.h <= 1.0;
                            let mapped = if cols > 1 && (no_entry || ctx.state.query.is_empty()) {
                                match other {
                                    KeyBindingAction::MoveCharBack => KeyBindingAction::RowUp,
                                    KeyBindingAction::MoveCharForward => KeyBindingAction::RowDown,
                                    act => act,
                                }
                            } else {
                                other
                            };
                            if let Some(out) = ctx.state.handle_action(mapped, cols, lines) {
                                ctx.outcome = Some(out);
                                should_quit = ctx.quit_on_outcome;
                            }
                        }
                    }
                    if ctx.outcome.is_none() {
                        ctx.relayout(hwnd);
                        let _ = ctx.paint(hwnd);
                    }
                }
            }
            if should_quit {
                PostQuitMessage(0);
            }
            if msg == WM_SYSKEYDOWN && !handled {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            LRESULT(0)
        }
        WM_CHAR => {
            let code = wparam.0 as u32;
            let (ctrl, alt, _, win) = current_modifiers();
            if !ctrl && !alt && !win && code >= 0x20 && code != 0x7F {
                if let Some(ch) = char::from_u32(code) {
                    if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                        // When a theme omits `entry` (e.g. `music-player.rasi` icon buttons),
                        // ignore stray text typing so icon buttons can never be filtered away.
                        if ctx.layout.entry_rect.w > 1.0 && ctx.layout.entry_rect.h > 1.0 {
                            ctx.state.insert_char(ch);
                            ctx.relayout(hwnd);
                            let _ = ctx.paint(hwnd);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let px = (lparam.0 & 0xFFFF) as i16 as f32;
            let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                let scale = (ctx.metrics.dpi / 96.0).max(1.0) as f32;
                let x = px / scale;
                let y = py / scale;
                if ctx.dragging_sb {
                    if let Some(track) = ctx.layout.scrollbar_track {
                        let frac = ((y - track.y) / track.h.max(1.0)).clamp(0.0, 1.0);
                        let max_off = ctx
                            .state
                            .filtered
                            .len()
                            .saturating_sub(ctx.layout.listview_page_size);
                        ctx.state.scroll_offset = (frac * max_off as f32).round() as usize;
                        ctx.relayout(hwnd);
                        let _ = ctx.paint(hwnd);
                    }
                } else {
                    let cur = if ctx.layout.entry_rect.contains(x, y) {
                        IDC_IBEAM
                    } else if ctx.layout.cells.iter().any(|c| c.rect.contains(x, y))
                        || ctx
                            .layout
                            .mode_buttons
                            .iter()
                            .any(|b| b.rect.contains(x, y))
                        || ctx
                            .layout
                            .widgets
                            .iter()
                            .any(|w| w.action.is_some() && w.paint_rect.contains(x, y))
                    {
                        IDC_HAND
                    } else {
                        IDC_ARROW
                    };
                    if let Ok(hc) = LoadCursorW(None, cur) {
                        SetCursor(hc);
                    }
                    if ctx.cfg.hover_select {
                        if let Some(cell) = ctx.layout.cells.iter().find(|c| c.rect.contains(x, y))
                        {
                            let idx = cell.entry_index;
                            if !ctx.state.entries[idx].nonselectable {
                                let fp = cell.filtered_pos;
                                if ctx.state.selected != fp {
                                    ctx.state.selected = fp;
                                    ctx.relayout(hwnd);
                                    let _ = ctx.paint(hwnd);
                                }
                            }
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let px = (lparam.0 & 0xFFFF) as i16 as f32;
            let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
            let mut should_quit = false;
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                let scale = (ctx.metrics.dpi / 96.0).max(1.0) as f32;
                let x = px / scale;
                let y = py / scale;
                if ctx.layout.scrollbar_track.is_some_and(|t| t.contains(x, y)) {
                    ctx.dragging_sb = true;
                    SetCapture(hwnd);
                } else if let Some(btn) = ctx
                    .layout
                    .mode_buttons
                    .iter()
                    .find(|b| b.rect.contains(x, y))
                {
                    ctx.outcome = Some(UiOutcome::ModeSwitch(btn.mode_index));
                    should_quit = ctx.quit_on_outcome;
                } else if let Some(cell) = ctx.layout.cells.iter().find(|c| c.rect.contains(x, y)) {
                    let idx = cell.entry_index;
                    if !ctx.state.entries[idx].nonselectable {
                        let fp = cell.filtered_pos;
                        let single_click_accept = ctx
                            .cfg
                            .me_accept_entry
                            .split(',')
                            .any(|t| t.trim().eq_ignore_ascii_case("MousePrimary"))
                            || !ctx
                                .cfg
                                .me_select_entry
                                .split(',')
                                .any(|t| t.trim().eq_ignore_ascii_case("MousePrimary"));
                        if single_click_accept || ctx.state.selected == fp {
                            ctx.state.selected = fp;
                            ctx.outcome = Some(UiOutcome::Accepted {
                                index: idx,
                                text: ctx.state.entries[idx].text.clone(),
                                exit_code: 0,
                            });
                            should_quit = ctx.quit_on_outcome;
                        } else {
                            ctx.state.selected = fp;
                            ctx.relayout(hwnd);
                            let _ = ctx.paint(hwnd);
                        }
                    }
                } else if let Some(act_name) = ctx
                    .layout
                    .widgets
                    .iter()
                    .rev()
                    .find(|w| w.action.is_some() && w.paint_rect.contains(x, y))
                    .and_then(|w| w.action.clone())
                {
                    let action_opt = ctx
                        .cfg
                        .keybindings
                        .entries()
                        .iter()
                        .find(|e| e.name.eq_ignore_ascii_case(&act_name))
                        .map(|e| e.action);
                    if let Some(action) = action_opt {
                        match action {
                            KeyBindingAction::PastePrimary | KeyBindingAction::PasteSecondary => {
                                if let Some(clip) = read_clipboard_text(hwnd) {
                                    ctx.state.insert_str(&clip);
                                }
                            }
                            other => {
                                let cols = ctx.layout.listview_columns;
                                let lines = ctx.layout.listview_lines;
                                if let Some(out) = ctx.state.handle_action(other, cols, lines) {
                                    ctx.outcome = Some(out);
                                    should_quit = ctx.quit_on_outcome;
                                }
                            }
                        }
                        if ctx.outcome.is_none() {
                            ctx.relayout(hwnd);
                            let _ = ctx.paint(hwnd);
                        }
                    }
                }
            }
            if should_quit {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                if ctx.dragging_sb {
                    ctx.dragging_sb = false;
                    let _ = ReleaseCapture();
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            let mut should_quit = false;
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                ctx.outcome = Some(UiOutcome::Cancelled);
                should_quit = ctx.quit_on_outcome;
            }
            if should_quit {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xFFFF) as i16;
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                let cols = ctx.layout.listview_columns;
                let lines = ctx.layout.listview_lines;
                let act = if delta < 0 {
                    KeyBindingAction::RowDown
                } else {
                    KeyBindingAction::RowUp
                };
                let _ = ctx.state.handle_action(act, cols, lines);
                ctx.relayout(hwnd);
                let _ = ctx.paint(hwnd);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            let active = (wparam.0 & 0xFFFF) as u32;
            if active == 0 && IsWindowVisible(hwnd).as_bool() {
                let mut should_quit = false;
                if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                    let in_show_grace = ctx
                        .shown_at
                        .is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(220));
                    if !ctx.suppress_deactivate
                        && !in_show_grace
                        && ctx.cfg.click_to_exit
                        && ctx.outcome.is_none()
                    {
                        ctx.outcome = Some(UiOutcome::Cancelled);
                        should_quit = ctx.quit_on_outcome;
                    }
                }
                if should_quit {
                    PostQuitMessage(0);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let mut should_quit = false;
            if let Ok(mut ctx) = ctx_rc.try_borrow_mut() {
                ctx.outcome = Some(UiOutcome::Cancelled);
                should_quit = ctx.quit_on_outcome;
            }
            if should_quit {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CREATE | WM_DESTROY => DefWindowProcW(hwnd, msg, wparam, lparam),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Initializes Direct2D + DirectWrite + a hidden Win32 window and paints one frame per
/// provided theme sheet, returning measured metrics (`--render-self-test`).
pub fn run_render_self_test(
    cfg: &Config,
    sheets: &[(&str, &ThemeSheet)],
) -> Result<RenderSelfTestReport, String> {
    let _com = ComGuard::init();
    // SAFETY: Setting Per-Monitor V2 DPI awareness on process startup.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let res = D2dResources::new()?;

    let mon_m = query_active_monitor();
    let sample_specs = [
        ("Explorer", "C:\\Windows\\explorer.exe"),
        ("Notepad", "C:\\Windows\\System32\\notepad.exe"),
        ("Command Prompt", "C:\\Windows\\System32\\cmd.exe"),
        ("Calculator", "C:\\Windows\\System32\\calc.exe"),
        ("PowerShell", "powershell.exe"),
    ];
    let sample_entries: Vec<ListEntry> = sample_specs
        .into_iter()
        .map(|(name, ico)| ListEntry {
            text: name.to_string(),
            icon: Some(ico.to_string()),
            ..Default::default()
        })
        .collect();
    let state = LauncherState::new(cfg, sample_entries);

    // SAFETY: Standard module handle query.
    let hinst: HINSTANCE = unsafe { GetModuleHandleW(None) }
        .map_err(|e| format!("GetModuleHandleW failed: {e}"))?
        .into();
    let class_name = register_window_class(hinst)?;

    let first_sheet = sheets
        .first()
        .map(|(_, s)| *s)
        .ok_or_else(|| "at least one theme sheet required".to_string())?;
    let mut sheet_copy = (*first_sheet).clone();
    sheet_copy.process_conditionals(&mon_m.mon_dip);

    let metrics = res.measure_sheet_font(&sheet_copy, &cfg.font, mon_m.dpi, mon_m.mon_dip);
    let initial_layout = LaidOutTree::build_with_measurers(
        &sheet_copy,
        &state,
        mon_m.workarea_dip,
        &metrics,
        &cfg.font,
        cfg.location,
        cfg.x_offset,
        cfg.y_offset,
        cfg.sidebar_mode,
        true,
        cfg.hide_icons,
        Some(&|f, t| res.measure_text_span(f, t)),
        Some(&|f, t, mw| res.measure_wrapped_height(f, t, mw)),
    );
    let (_, _, init_pw, init_ph) =
        dip_rect_to_phys(initial_layout.window_rect, mon_m.phys_origin, mon_m.dpi);

    let ctx_rc = Rc::new(RefCell::new(WindowCtx {
        sheet: sheet_copy,
        cfg: cfg.clone(),
        state,
        metrics,
        monitor: mon_m,
        sync_window_size: false,
        async_icons: false,
        quit_on_outcome: false,
        suppress_deactivate: false,
        layout: initial_layout.clone(),
        res,
        outcome: None,
        dragging_sb: false,
        backdrop_blur: None,
        shown_at: None,
    }));

    // SAFETY: Creating a temporary hidden layered popup HWND bound to `ctx_rc` on the same stack/thread.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("LiteRofi SelfTest"),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            init_pw.max(64),
            init_ph.max(14),
            None,
            None,
            hinst,
            Some(Rc::as_ptr(&ctx_rc) as *const std::ffi::c_void),
        )
    }
    .map_err(|e| format!("CreateWindowExW failed: {e}"))?;

    // SAFETY: Query DPI of created window handle.
    let win_dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as f64;
    let mut frames_painted = 0usize;
    let mut icons_loaded = 0usize;
    let mut last_w = initial_layout.window_rect.w.round() as i32;
    let mut last_h = initial_layout.window_rect.h.round() as i32;

    for &(_, sheet) in sheets {
        let mut sc = sheet.clone();
        sc.process_conditionals(&mon_m.mon_dip);
        let m = {
            let mut ctx = ctx_rc.borrow_mut();
            let m = ctx
                .res
                .measure_sheet_font(&sc, &ctx.cfg.font, win_dpi, mon_m.mon_dip);
            ctx.metrics = m;
            ctx.sheet = sc;
            let layout = LaidOutTree::build_with_measurers(
                &ctx.sheet,
                &ctx.state,
                mon_m.workarea_dip,
                &m,
                &ctx.cfg.font,
                ctx.cfg.location,
                ctx.cfg.x_offset,
                ctx.cfg.y_offset,
                ctx.cfg.sidebar_mode,
                true,
                ctx.cfg.hide_icons,
                Some(&|f, t| ctx.res.measure_text_span(f, t)),
                Some(&|f, t, mw| ctx.res.measure_wrapped_height(f, t, mw)),
            );
            ctx.layout = layout;
            last_w = ctx.layout.window_rect.w.round() as i32;
            last_h = ctx.layout.window_rect.h.round() as i32;
            icons_loaded = ctx.paint(hwnd)?;
            m
        };
        frames_painted += 1;
        let _ = m;
    }

    // SAFETY: Destroy temporary test window while `ctx_rc` is still alive, then release COM.
    let _ = unsafe { DestroyWindow(hwnd) };
    let metrics_final = ctx_rc.borrow().metrics;
    Ok(RenderSelfTestReport {
        dpi: metrics_final.dpi,
        char_height: metrics_final.char_height,
        char_width: metrics_final.char_width,
        window_width: last_w,
        window_height: last_h,
        frames_painted,
        icons_loaded,
    })
}

fn crc32_png(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in chunk_type.iter().chain(data.iter()) {
        crc ^= u32::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32_png(chunk_type, data).to_be_bytes());
}

fn encode_rgb24_png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let row_bytes = (width as usize) * 3;
    let mut raw = Vec::with_capacity((row_bytes + 1) * (height as usize));
    for y in 0..(height as usize) {
        raw.push(0u8); // filter type None
        let start = y * row_bytes;
        raw.extend_from_slice(&rgb[start..start + row_bytes]);
    }

    // Zlib stream with stored deflate blocks (RFC 1950 + RFC 1951)
    let mut zlib = Vec::with_capacity(raw.len() + (raw.len() / 65535 + 2) * 5 + 6);
    zlib.extend_from_slice(&[0x78, 0x01]);
    let chunks: Vec<&[u8]> = if raw.is_empty() {
        vec![&[]]
    } else {
        raw.chunks(65535).collect()
    };
    for (i, blk) in chunks.iter().enumerate() {
        let is_last = i + 1 == chunks.len();
        zlib.push(u8::from(is_last));
        let len = blk.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(blk);
    }
    let mut s1 = 1u32;
    let mut s2 = 0u32;
    for &b in &raw {
        s1 = (s1 + u32::from(b)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    let adler = (s2 << 16) | s1;
    zlib.extend_from_slice(&adler.to_be_bytes());

    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 2; // color type RGB
    write_png_chunk(&mut png, b"IHDR", &ihdr);
    write_png_chunk(&mut png, b"IDAT", &zlib);
    write_png_chunk(&mut png, b"IEND", &[]);
    png
}

/// Renders one complete frame synchronously (with icons) onto a simulated desktop canvas
/// matching the active monitor and saves it to `out_path` as a 24-bit PNG (`--dump-png`).
pub fn run_render_dump_png(
    sheet: &ThemeSheet,
    cfg: &Config,
    state: &mut LauncherState,
    out_path: &std::path::Path,
) -> Result<(u32, u32), String> {
    let _com = ComGuard::init();
    // SAFETY: Setting Per-Monitor V2 DPI awareness on process startup.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let res = D2dResources::new()?;

    let mon_m = query_active_monitor();
    let mut processed_sheet = sheet.clone();
    processed_sheet.process_conditionals(&mon_m.mon_dip);

    let metrics = res.measure_sheet_font(&processed_sheet, &cfg.font, mon_m.dpi, mon_m.mon_dip);
    let fs = sheet_is_fullscreen(&processed_sheet);
    let bounds = mon_m.bounds_dip(fs);
    let initial_layout = LaidOutTree::build_with_measurers(
        &processed_sheet,
        state,
        bounds,
        &metrics,
        &cfg.font,
        cfg.location,
        cfg.x_offset,
        cfg.y_offset,
        cfg.sidebar_mode,
        cfg.show_icons,
        cfg.hide_icons,
        Some(&|f, t| res.measure_text_span(f, t)),
        Some(&|f, t, mw| res.measure_wrapped_height(f, t, mw)),
    );

    // SAFETY: Standard module handle query.
    let hinst: HINSTANCE = unsafe { GetModuleHandleW(None) }
        .map_err(|e| format!("GetModuleHandleW failed: {e}"))?
        .into();
    let class_name = register_window_class(hinst)?;
    let (_, _, init_pw, init_ph) = dip_rect_to_phys(
        initial_layout.window_rect,
        mon_m.phys_origin_for(fs),
        mon_m.dpi,
    );

    let ctx_rc = Rc::new(RefCell::new(WindowCtx {
        sheet: processed_sheet,
        cfg: cfg.clone(),
        state: state.clone(),
        metrics,
        monitor: mon_m,
        sync_window_size: false,
        async_icons: false,
        quit_on_outcome: false,
        suppress_deactivate: true,
        layout: initial_layout,
        res,
        outcome: None,
        dragging_sb: false,
        backdrop_blur: None,
        shown_at: None,
    }));

    // SAFETY: Hidden offscreen layered HWND used solely to execute the exact `WindowCtx::paint` path.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("LiteRofi DumpPng"),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            init_pw.max(64),
            init_ph.max(14),
            None,
            None,
            hinst,
            Some(Rc::as_ptr(&ctx_rc) as *const std::ffi::c_void),
        )
    }
    .map_err(|e| format!("CreateWindowExW failed: {e}"))?;

    let win_dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as f64;
    {
        let mut ctx = ctx_rc.borrow_mut();
        ctx.metrics = ctx
            .res
            .measure_sheet_font(&ctx.sheet, &ctx.cfg.font, win_dpi, mon_m.mon_dip);
        ctx.relayout(hwnd);
        ctx.paint(hwnd)?;
    }
    // SAFETY: Flush GDI/D2D operations to DIBSection memory before reading `bits`.
    unsafe {
        let _ = GdiFlush();
    }

    let result = {
        let ctx = ctx_rc.borrow();
        let surf = ctx
            .res
            .surface
            .as_ref()
            .ok_or_else(|| "missing rendered surface".to_string())?;
        if surf.bits.is_null() {
            return Err("null DIBSection bits".to_string());
        }
        // Tightly frame the rendered Rofi window with a sleek 32px slate-teal backdrop margin
        // so theme previews fill cards crisply instead of appearing tiny on a full-monitor canvas.
        let pad: u32 = if fs { 0 } else { 32 };
        let desk_w = surf.width.saturating_add(pad * 2).max(1);
        let desk_h = surf.height.saturating_add(pad * 2).max(1);
        let win_x = pad as i32;
        let win_y = pad as i32;

        // Scenic slate-teal backdrop so both light and dark translucent themes show crisp contrast
        let mut desk_rgb = vec![0u8; (desk_w as usize) * (desk_h as usize) * 3];
        for y in 0..desk_h {
            let ty = y as f32 / (desk_h.max(1) as f32);
            for x in 0..desk_w {
                let tx = x as f32 / (desk_w.max(1) as f32);
                let r = (36.0 + 28.0 * tx + 18.0 * (1.0 - ty)) as u8;
                let g = (52.0 + 36.0 * (1.0 - ty) + 14.0 * tx) as u8;
                let b = (78.0 + 44.0 * ty + 20.0 * (1.0 - tx)) as u8;
                let idx = ((y * desk_w + x) as usize) * 3;
                desk_rgb[idx] = r;
                desk_rgb[idx + 1] = g;
                desk_rgb[idx + 2] = b;
            }
        }

        let src_len = (surf.width as usize) * (surf.height as usize) * 4;
        // SAFETY: `surf.bits` points to `surf.width * surf.height * 4` valid DIBSection bytes.
        let src = unsafe { std::slice::from_raw_parts(surf.bits, src_len) };
        for sy in 0..(surf.height as i32) {
            let dy = win_y + sy;
            if dy < 0 || dy >= desk_h as i32 {
                continue;
            }
            for sx in 0..(surf.width as i32) {
                let dx = win_x + sx;
                if dx < 0 || dx >= desk_w as i32 {
                    continue;
                }
                let s_off = ((sy as usize) * (surf.width as usize) + (sx as usize)) * 4;
                let b_pre = u32::from(src[s_off]);
                let g_pre = u32::from(src[s_off + 1]);
                let r_pre = u32::from(src[s_off + 2]);
                let a = u32::from(src[s_off + 3]);
                let d_off = ((dy as usize) * (desk_w as usize) + (dx as usize)) * 3;
                let inv = 255u32.saturating_sub(a);
                desk_rgb[d_off] =
                    (r_pre + (u32::from(desk_rgb[d_off]) * inv + 127) / 255).min(255) as u8;
                desk_rgb[d_off + 1] =
                    (g_pre + (u32::from(desk_rgb[d_off + 1]) * inv + 127) / 255).min(255) as u8;
                desk_rgb[d_off + 2] =
                    (b_pre + (u32::from(desk_rgb[d_off + 2]) * inv + 127) / 255).min(255) as u8;
            }
        }

        // Downscale smoothly to max 1600px wide for compact, crisp inspection PNGs
        let (out_w, out_h, out_rgb) = if desk_w > 1600 {
            let nh = ((u64::from(desk_h) * 1600) / u64::from(desk_w)).max(1) as u32;
            let nw = 1600u32;
            let mut scaled = vec![0u8; (nw as usize) * (nh as usize) * 3];
            for oy in 0..nh {
                let sy0 = (u64::from(oy) * u64::from(desk_h) / u64::from(nh)) as u32;
                let sy1 = (((u64::from(oy) + 1) * u64::from(desk_h) / u64::from(nh)) as u32)
                    .max(sy0 + 1)
                    .min(desk_h);
                for ox in 0..nw {
                    let sx0 = (u64::from(ox) * u64::from(desk_w) / u64::from(nw)) as u32;
                    let sx1 = (((u64::from(ox) + 1) * u64::from(desk_w) / u64::from(nw)) as u32)
                        .max(sx0 + 1)
                        .min(desk_w);
                    let mut sr = 0u32;
                    let mut sg = 0u32;
                    let mut sb = 0u32;
                    let mut cnt = 0u32;
                    for yy in sy0..sy1 {
                        for xx in sx0..sx1 {
                            let idx = ((yy * desk_w + xx) as usize) * 3;
                            sr += u32::from(desk_rgb[idx]);
                            sg += u32::from(desk_rgb[idx + 1]);
                            sb += u32::from(desk_rgb[idx + 2]);
                            cnt += 1;
                        }
                    }
                    let dst = ((oy * nw + ox) as usize) * 3;
                    scaled[dst] = (sr / cnt.max(1)) as u8;
                    scaled[dst + 1] = (sg / cnt.max(1)) as u8;
                    scaled[dst + 2] = (sb / cnt.max(1)) as u8;
                }
            }
            (nw, nh, scaled)
        } else {
            (desk_w, desk_h, desk_rgb)
        };

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }
        let png_bytes = encode_rgb24_png(out_w, out_h, &out_rgb);
        std::fs::write(out_path, png_bytes)
            .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
        Ok((surf.width, surf.height))
    };

    // SAFETY: Destroy temporary window before dropping `ctx_rc`.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let _ = DestroyWindow(hwnd);
    }
    result
}

/// Decodes a single Windows icon spec (`path`, `shell:AppsFolder\...`, image file, or `hwnd:<id>`)
/// onto a real Direct2D render target and returns its decoded `(width, height)` in pixels.
pub fn probe_icon_bitmap_size(spec: &str, size_px: u32) -> Result<Option<(f32, f32)>, String> {
    let _com = ComGuard::init();
    let mut res = D2dResources::new()?;
    let (dc_rt, _) = res.ensure_dc_rt(64, 64, 96.0)?;
    let rt: ID2D1RenderTarget = dc_rt
        .cast()
        .map_err(|e| format!("ID2D1RenderTarget cast failed: {e}"))?;
    let size_opt = res
        .icons
        .as_mut()
        .and_then(|cache| cache.get_or_load(&rt, spec, size_px))
        .map(|bmp| {
            // SAFETY: Valid ID2D1Bitmap belonging to `rt`.
            let sz = unsafe { bmp.GetSize() };
            (sz.width, sz.height)
        });
    res.icons = None;
    res.dc_rt = None;
    res.surface = None;
    Ok(size_opt)
}

/// Runs the interactive frameless Win32 + Direct2D launcher window until selection or cancellation.
pub fn run_interactive(
    sheet: &ThemeSheet,
    cfg: &Config,
    state: &mut LauncherState,
) -> Result<UiOutcome, String> {
    let _com = ComGuard::init();
    // SAFETY: Setting Per-Monitor V2 DPI awareness on process startup.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let res = D2dResources::new()?;

    let mon_m = query_active_monitor();
    let mut processed_sheet = sheet.clone();
    processed_sheet.process_conditionals(&mon_m.mon_dip);

    let metrics = res.measure_sheet_font(&processed_sheet, &cfg.font, mon_m.dpi, mon_m.mon_dip);
    let fs = sheet_is_fullscreen(&processed_sheet);
    let initial_layout = LaidOutTree::build_with_measurers(
        &processed_sheet,
        state,
        mon_m.bounds_dip(fs),
        &metrics,
        &cfg.font,
        cfg.location,
        cfg.x_offset,
        cfg.y_offset,
        cfg.sidebar_mode,
        cfg.show_icons,
        cfg.hide_icons,
        Some(&|f, t| res.measure_text_span(f, t)),
        Some(&|f, t, mw| res.measure_wrapped_height(f, t, mw)),
    );

    // SAFETY: Standard module handle query.
    let hinst: HINSTANCE = unsafe { GetModuleHandleW(None) }
        .map_err(|e| format!("GetModuleHandleW failed: {e}"))?
        .into();
    let class_name = register_window_class(hinst)?;

    let (phys_x, phys_y, phys_w, phys_h) = dip_rect_to_phys(
        initial_layout.window_rect,
        mon_m.phys_origin_for(fs),
        mon_m.dpi,
    );

    let ctx_rc = Rc::new(RefCell::new(WindowCtx {
        sheet: processed_sheet,
        cfg: cfg.clone(),
        state: state.clone(),
        metrics,
        monitor: mon_m,
        sync_window_size: true,
        async_icons: true,
        quit_on_outcome: true,
        suppress_deactivate: true,
        layout: initial_layout,
        res,
        outcome: None,
        dragging_sb: false,
        backdrop_blur: None,
        shown_at: None,
    }));

    let ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED;

    // SAFETY: Creating foreground layered popup HWND bound to `ctx_rc` for duration of message loop.
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name,
            w!("LiteRofi"),
            WS_POPUP,
            phys_x,
            phys_y,
            phys_w,
            phys_h,
            None,
            None,
            hinst,
            Some(Rc::as_ptr(&ctx_rc) as *const std::ffi::c_void),
        )
    }
    .map_err(|e| format!("CreateWindowExW failed: {e}"))?;

    // SAFETY: Check actual DPI after creation and re-layout if Windows assigned a different monitor DPI.
    let actual_dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as f64;
    if (actual_dpi - mon_m.dpi).abs() > 0.5 {
        let mut ctx = ctx_rc.borrow_mut();
        ctx.metrics = ctx
            .res
            .measure_sheet_font(&ctx.sheet, &cfg.font, actual_dpi, mon_m.mon_dip);
        ctx.relayout(hwnd);
    }

    // SAFETY: Capture live desktop behind hidden popup for translucent blur, then paint BEFORE reveal.
    unsafe {
        set_window_cloaked(hwnd, true);
        {
            let mut ctx = ctx_rc.borrow_mut();
            ctx.capture_backdrop_if_needed();
            let _ = ctx.paint(hwnd);
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        set_window_cloaked(hwnd, false);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
        ctx_rc.borrow_mut().suppress_deactivate = false;

        let mut msg = MSG::default();
        while windows::Win32::UI::WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if ctx_rc.borrow().outcome.is_some() {
                break;
            }
        }
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = DestroyWindow(hwnd);
    }

    let _ = (WM_APP_WM_QUIT, COLORREF(0));
    *state = ctx_rc.borrow().state.clone();
    let out = ctx_rc
        .borrow_mut()
        .outcome
        .take()
        .unwrap_or(UiOutcome::Cancelled);
    Ok(out)
}

/// Persistent pre-warmed Win32 + Direct2D popup window hosted by `--daemon`
/// so global hotkey triggers reveal instantaneously (~2-5 ms) with zero process spawn or flash.
pub struct ResidentPopup {
    hwnd: HWND,
    ctx_rc: Rc<RefCell<WindowCtx>>,
    _com: ComGuard,
}

impl ResidentPopup {
    pub fn new(sheet: ThemeSheet, cfg: Config, state: LauncherState) -> Result<Self, String> {
        let com = ComGuard::init();
        // SAFETY: Setting Per-Monitor V2 DPI awareness on process startup.
        let _ =
            unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let res = D2dResources::new()?;

        let mon_m = query_active_monitor();
        let mut processed_sheet = sheet;
        processed_sheet.process_conditionals(&mon_m.mon_dip);

        let metrics = res.measure_sheet_font(&processed_sheet, &cfg.font, mon_m.dpi, mon_m.mon_dip);
        let fs = sheet_is_fullscreen(&processed_sheet);
        let initial_layout = LaidOutTree::build_with_measurers(
            &processed_sheet,
            &state,
            mon_m.bounds_dip(fs),
            &metrics,
            &cfg.font,
            cfg.location,
            cfg.x_offset,
            cfg.y_offset,
            cfg.sidebar_mode,
            cfg.show_icons,
            cfg.hide_icons,
            Some(&|f, t| res.measure_text_span(f, t)),
            Some(&|f, t, mw| res.measure_wrapped_height(f, t, mw)),
        );

        // SAFETY: Standard module handle query.
        let hinst: HINSTANCE = unsafe { GetModuleHandleW(None) }
            .map_err(|e| format!("GetModuleHandleW failed: {e}"))?
            .into();
        let class_name = register_window_class(hinst)?;

        let (phys_x, phys_y, phys_w, phys_h) = dip_rect_to_phys(
            initial_layout.window_rect,
            mon_m.phys_origin_for(fs),
            mon_m.dpi,
        );

        let ctx_rc = Rc::new(RefCell::new(WindowCtx {
            sheet: processed_sheet,
            cfg,
            state,
            metrics,
            monitor: mon_m,
            sync_window_size: true,
            async_icons: true,
            quit_on_outcome: false,
            suppress_deactivate: true,
            layout: initial_layout,
            res,
            outcome: None,
            dragging_sb: false,
            backdrop_blur: None,
            shown_at: None,
        }));

        let ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED;

        // SAFETY: Creating hidden layered popup HWND bound to `ctx_rc` owned by `ResidentPopup`.
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                class_name,
                w!("LiteRofi"),
                WS_POPUP,
                phys_x,
                phys_y,
                phys_w,
                phys_h,
                None,
                None,
                hinst,
                Some(Rc::as_ptr(&ctx_rc) as *const std::ffi::c_void),
            )
        }
        .map_err(|e| format!("CreateWindowExW failed: {e}"))?;

        // SAFETY: Query actual monitor DPI after creation and re-layout if needed.
        unsafe {
            let actual_dpi = GetDpiForWindow(hwnd).max(96) as f64;
            if (actual_dpi - mon_m.dpi).abs() > 0.5 {
                let mut ctx = ctx_rc.borrow_mut();
                ctx.metrics = ctx.res.measure_sheet_font(
                    &ctx.sheet,
                    &ctx.cfg.font,
                    actual_dpi,
                    mon_m.mon_dip,
                );
                ctx.relayout(hwnd);
            }
        }

        let mut popup = Self {
            hwnd,
            ctx_rc,
            _com: com,
        };
        let _ = popup.prewarm_frame();
        Ok(popup)
    }

    pub fn is_alive(&self) -> bool {
        // SAFETY: Queries Win32 window validity for our owned HWND.
        !self.hwnd.0.is_null()
            && unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindow(self.hwnd).as_bool() }
    }

    pub fn is_visible(&self) -> bool {
        // SAFETY: Valid owned HWND.
        self.is_alive() && unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn is_foreground(&self) -> bool {
        // SAFETY: Compares current foreground window against our owned popup HWND.
        self.is_alive() && unsafe { GetForegroundWindow() == self.hwnd }
    }

    pub fn prewarm_frame(&mut self) -> Result<usize, String> {
        self.ctx_rc.borrow_mut().paint(self.hwnd)
    }

    pub fn update_session(&mut self, sheet: ThemeSheet, cfg: Config, state: LauncherState) {
        let mut ctx = self.ctx_rc.borrow_mut();
        let mut processed = sheet;
        processed.process_conditionals(&ctx.monitor.mon_dip);
        let mon_dip = ctx.monitor.mon_dip;
        let dpi = ctx.metrics.dpi;
        ctx.metrics = ctx
            .res
            .measure_sheet_font(&processed, &cfg.font, dpi, mon_dip);
        ctx.sheet = processed;
        ctx.cfg = cfg;
        ctx.state = state;
        ctx.outcome = None;
        ctx.relayout(self.hwnd);
        if self.is_visible() {
            let _ = ctx.paint(self.hwnd);
        }
    }

    pub fn set_state_and_sheet(&mut self, sheet: ThemeSheet, state: LauncherState) {
        let mut ctx = self.ctx_rc.borrow_mut();
        let mut processed = sheet;
        processed.process_conditionals(&ctx.monitor.mon_dip);
        let mon_dip = ctx.monitor.mon_dip;
        let dpi = ctx.metrics.dpi;
        ctx.metrics = ctx
            .res
            .measure_sheet_font(&processed, &ctx.cfg.font, dpi, mon_dip);
        ctx.sheet = processed;
        ctx.state = state;
        ctx.outcome = None;
        ctx.relayout(self.hwnd);
        if self.is_visible() {
            let _ = ctx.paint(self.hwnd);
        }
    }

    /// Hot-swaps the active theme sheet while keeping the popup open and preserving
    /// the current search query, selection, and scroll offset (used by live theme preview).
    pub fn preview_theme_sheet(&mut self, sheet: ThemeSheet, message: Option<String>) {
        let was_visible = self.is_visible();
        {
            let mut ctx = self.ctx_rc.borrow_mut();
            ctx.suppress_deactivate = true;
            let mut processed = sheet;
            processed.process_conditionals(&ctx.monitor.mon_dip);
            let mon_dip = ctx.monitor.mon_dip;
            let dpi = ctx.metrics.dpi;
            ctx.metrics = ctx
                .res
                .measure_sheet_font(&processed, &ctx.cfg.font, dpi, mon_dip);
            ctx.sheet = processed;
            if message.is_some() {
                ctx.state.message = message;
            }
            ctx.outcome = None;
            ctx.relayout(self.hwnd);
            if was_visible && should_blur_backdrop(&ctx.layout, &ctx.cfg).is_some() {
                // SAFETY: Briefly cloaks popup so desktop backdrop capture excludes our own window.
                unsafe {
                    set_window_cloaked(self.hwnd, true);
                }
                ctx.capture_backdrop_if_needed();
                let _ = ctx.paint(self.hwnd);
                // SAFETY: Uncloaks popup immediately after painting blurred surface.
                unsafe {
                    set_window_cloaked(self.hwnd, false);
                }
            } else {
                ctx.backdrop_blur = None;
                if was_visible {
                    let _ = ctx.paint(self.hwnd);
                }
            }
            ctx.suppress_deactivate = false;
        }
    }

    /// Pumps the Win32 message loop for this persistent popup until a [`UiOutcome`] is produced,
    /// without hiding or destroying the window (so live theme preview / mode switch never drops focus).
    pub fn wait_for_outcome(&mut self) -> UiOutcome {
        if let Some(out) = self.take_outcome() {
            return out;
        }
        // SAFETY: Standard Win32 message loop for the owned resident popup HWND.
        unsafe {
            let mut msg = MSG::default();
            while windows::Win32::UI::WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0)
                .as_bool()
            {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
                if let Some(out) = self.take_outcome() {
                    return out;
                }
            }
        }
        UiOutcome::Cancelled
    }

    pub fn current_state_clone(&self) -> LauncherState {
        self.ctx_rc.borrow().state.clone()
    }

    pub fn reset_query(&mut self) {
        let mut ctx = self.ctx_rc.borrow_mut();
        ctx.state.clear_line();
        ctx.outcome = None;
        ctx.relayout(self.hwnd);
    }

    pub fn prepare_and_show(&mut self) -> Result<(), String> {
        if !self.is_alive() {
            return Err("resident popup HWND is no longer valid".to_string());
        }
        let mon_m = query_active_monitor();
        {
            let mut ctx = self.ctx_rc.borrow_mut();
            let dpi_changed = (ctx.metrics.dpi - mon_m.dpi).abs() > 0.5
                || ctx.metrics.monitor_width != f64::from(mon_m.mon_dip.width)
                || ctx.metrics.monitor_height != f64::from(mon_m.mon_dip.height);
            ctx.monitor = mon_m;
            if dpi_changed {
                ctx.metrics =
                    ctx.res
                        .measure_sheet_font(&ctx.sheet, &ctx.cfg.font, mon_m.dpi, mon_m.mon_dip);
            }
            ctx.outcome = None;
            ctx.suppress_deactivate = true;
            ctx.relayout(self.hwnd);
        }

        // SAFETY: Captures live desktop behind hidden popup for frosted-glass blur, paints
        // premultiplied ARGB surface via UpdateLayeredWindow BEFORE reveal, then activates
        // cleanly on top using DosyaBul's foreground-lock-safe activation sequence.
        let paint_res = unsafe {
            set_window_cloaked(self.hwnd, true);
            let res = {
                let mut ctx = self.ctx_rc.borrow_mut();
                ctx.capture_backdrop_if_needed();
                let r = ctx.paint(self.hwnd);
                ctx.shown_at = Some(std::time::Instant::now());
                r
            };
            if res.is_ok() {
                activate_popup_window(self.hwnd);
            } else {
                set_window_cloaked(self.hwnd, false);
            }
            res
        };
        self.ctx_rc.borrow_mut().suppress_deactivate = false;
        paint_res.map(|_| ())
    }

    pub fn hide(&mut self) {
        {
            let mut ctx = self.ctx_rc.borrow_mut();
            ctx.suppress_deactivate = true;
            ctx.outcome = None;
            ctx.backdrop_blur = None;
        }
        // SAFETY: Hides owned popup HWND immediately without destroying D2D resources.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.ctx_rc.borrow_mut().suppress_deactivate = false;
    }

    pub fn take_outcome(&mut self) -> Option<UiOutcome> {
        self.ctx_rc.borrow_mut().outcome.take()
    }

    pub fn current_query(&self) -> String {
        self.ctx_rc.borrow().state.query.clone()
    }
}

impl Drop for ResidentPopup {
    fn drop(&mut self) {
        if !self.hwnd.0.is_null() {
            // SAFETY: Unbinds `GWLP_USERDATA` and destroys owned HWND before `ctx_rc` drops.
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}
