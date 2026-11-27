//! Dynamic `.rasi` widget tree builder (`window -> mainbox -> children`), style
//! resolver, listview cell & mode-switcher hit-testing, and window sizing.

use crate::font::parse_rofi_font;
use crate::layout::{
    compute_window_rect, layout_box, layout_listview_fixed_row_height, BoxChildSpec, Rect,
};
use crate::state::{LauncherState, ListEntry};
use literofi_core::config::WindowLocation;
use literofi_theme::{
    highlight_flags, CursorType, Distance, DistanceMetrics, HighlightColorStyle, ImageValue,
    LineStyle, Orientation, Padding, PropertyValue, ThemeColor, ThemeSheet, WidgetQuery,
};

/// Resolved visual properties for any `.rasi` widget node.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetStyle {
    pub background: ThemeColor,
    pub foreground: ThemeColor,
    pub background_image: Option<ImageValue>,
    pub border_color: ThemeColor,
    pub border: [f32; 4], // top, right, bottom, left
    pub border_styles: [LineStyle; 4],
    pub border_radius: [f32; 4], // tl, tr, br, bl
    pub padding: [f32; 4],
    pub margin: [f32; 4],
    pub spacing: f32,
    pub highlight: HighlightColorStyle,
    pub font: String,
    pub placeholder: String,
    pub placeholder_color: ThemeColor,
    pub horizontal_align: f32,
    pub vertical_align: f32,
    pub expand: bool,
    pub cursor: CursorType,
    pub hide_cursor_on_empty: bool,
    pub cursor_width: f32,
    pub cursor_color: ThemeColor,
}

impl Default for WidgetStyle {
    fn default() -> Self {
        Self {
            background: ThemeColor::TRANSPARENT,
            foreground: ThemeColor::BLACK,
            background_image: None,
            border_color: ThemeColor::BLACK,
            border: [0.0; 4],
            border_styles: [LineStyle::Solid; 4],
            border_radius: [0.0; 4],
            padding: [0.0; 4],
            margin: [0.0; 4],
            spacing: 2.0,
            highlight: HighlightColorStyle {
                style: highlight_flags::BOLD | highlight_flags::UNDERLINE,
                color: ThemeColor::BLACK,
            },
            font: "mono 12".to_string(),
            placeholder: String::new(),
            placeholder_color: ThemeColor::new(0.5, 0.5, 0.5, 1.0),
            horizontal_align: 0.0,
            vertical_align: 0.5,
            expand: true,
            cursor: CursorType::Default,
            hide_cursor_on_empty: false,
            cursor_width: 1.5,
            cursor_color: ThemeColor::BLACK,
        }
    }
}

fn fallback_text_width(text: &str, ch_w: f32) -> f32 {
    text.chars()
        .map(|ch| {
            let wide = ('\u{e000}'..='\u{f8ff}').contains(&ch)
                || ('\u{f0000}'..='\u{ffffd}').contains(&ch)
                || ('\u{2300}'..='\u{27bf}').contains(&ch)
                || ('\u{1f000}'..='\u{1faff}').contains(&ch)
                || ('\u{3000}'..='\u{9fff}').contains(&ch);
            if wide {
                2.1 * ch_w
            } else {
                ch_w
            }
        })
        .sum()
}

fn pad_to_px(p: &Padding, m: &DistanceMetrics) -> ([f32; 4], [LineStyle; 4]) {
    (
        [
            p.top.to_pixels(Orientation::Vertical, m).max(0.0) as f32,
            p.right.to_pixels(Orientation::Horizontal, m).max(0.0) as f32,
            p.bottom.to_pixels(Orientation::Vertical, m).max(0.0) as f32,
            p.left.to_pixels(Orientation::Horizontal, m).max(0.0) as f32,
        ],
        [p.top.style, p.right.style, p.bottom.style, p.left.style],
    )
}

/// Returns the effective base font for `sheet` (`window { font }` -> `* { font }` -> `cfg_font`).
pub fn effective_base_font(sheet: &ThemeSheet, cfg_font: &str) -> String {
    let q_win = WidgetQuery::new("window");
    if let Some(PropertyValue::String(s)) =
        sheet.find_property_aliases(&q_win, &["font", "text-font"])
    {
        if !s.trim().is_empty() {
            return s.clone();
        }
    }
    cfg_font.to_string()
}

fn root_initial_foreground(sheet: &ThemeSheet) -> ThemeColor {
    for key in ["text-color", "color", "foreground-color", "foreground"] {
        if let Some(PropertyValue::Color(c)) = sheet.find_root_property(key) {
            return *c;
        }
    }
    ThemeColor::BLACK
}

fn default_widget_state(name: &str) -> Option<&'static str> {
    if matches!(
        name,
        "prompt"
            | "entry"
            | "case-indicator"
            | "num-rows"
            | "num-filtered-rows"
            | "textbox-current-entry"
            | "textbox"
    ) || name.starts_with("textbox-")
        || name.starts_with("button-")
    {
        Some("normal.normal")
    } else if name == "overlay" {
        Some("normal.urgent")
    } else {
        None
    }
}

/// Resolves full visual properties for a widget query against `sheet`.
pub fn resolve_widget_style(
    sheet: &ThemeSheet,
    query: &WidgetQuery<'_>,
    metrics: &DistanceMetrics,
    default_font: &str,
    default_expand: bool,
) -> WidgetStyle {
    let init_fg = root_initial_foreground(sheet);
    resolve_widget_style_with_parent(
        sheet,
        query,
        metrics,
        default_font,
        default_expand,
        init_fg,
        default_font,
    )
}

/// Resolves visual properties for `query` with Rofi Cairo-compatible widget-tree `inherit`
/// resolution (`source/theme.c`, `source/widgets/widget.c`).
pub fn resolve_widget_style_with_parent(
    sheet: &ThemeSheet,
    query: &WidgetQuery<'_>,
    metrics: &DistanceMetrics,
    default_font: &str,
    default_expand: bool,
    parent_fg: ThemeColor,
    parent_font: &str,
) -> WidgetStyle {
    resolve_widget_style_inner(
        sheet,
        query,
        query,
        metrics,
        default_font,
        default_expand,
        ThemeColor::TRANSPARENT,
        parent_fg,
        parent_font,
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_widget_style_inner(
    sheet: &ThemeSheet,
    query: &WidgetQuery<'_>,
    box_metrics_query: &WidgetQuery<'_>,
    metrics: &DistanceMetrics,
    default_font: &str,
    default_expand: bool,
    parent_bg: ThemeColor,
    parent_fg: ThemeColor,
    parent_font: &str,
) -> WidgetStyle {
    // Upstream `widget.c:245` / `default_theme.rasi`: `element-text` and `element-icon` default
    // to `transparent` unless explicitly styled on the widget/state itself — they must NOT jump
    // past `element selected` to `* { background-color }` (which would paint an opaque box over
    // the selected row pill, e.g. `flamingo.rasi`).
    let bg = if query.name == "element-text" || query.name == "element-icon" {
        match sheet.find_widget_property_aliases(query.name, query.state, &["background-color"]) {
            Some(PropertyValue::Color(c)) => *c,
            Some(PropertyValue::Inherit) => parent_bg,
            _ => ThemeColor::TRANSPARENT,
        }
    } else {
        match sheet.find_property_aliases(query, &["background-color"]) {
            Some(PropertyValue::Color(c)) => *c,
            Some(PropertyValue::Inherit) => parent_bg,
            _ if query.name == "window" => match sheet.find_root_property("background") {
                Some(PropertyValue::Color(c)) => *c,
                _ => ThemeColor::TRANSPARENT,
            },
            _ => ThemeColor::TRANSPARENT,
        }
    };

    // Upstream `textbox.c` / `theme.c` / `default_theme.rasi`: `element-text` and `message > textbox`
    // inherit their parent container's `text-color` unless explicitly overridden on the widget/state
    // itself, rather than jumping past `element selected` / `message` to `* { text-color }`.
    let prefer_parent_fg = query.name == "element-text"
        || (query.name == "textbox" && query.parent.is_some_and(|p| p.name == "message"));
    let fg = if prefer_parent_fg {
        match sheet.find_widget_property_aliases(
            query.name,
            query.state,
            &["text-color", "color", "foreground-color"],
        ) {
            Some(PropertyValue::Color(c)) => *c,
            _ => parent_fg,
        }
    } else {
        match sheet.find_property_aliases(query, &["text-color", "color", "foreground-color"]) {
            Some(PropertyValue::Color(c)) => *c,
            Some(PropertyValue::Inherit) => parent_fg,
            _ => parent_fg,
        }
    };

    let border_color = match sheet.find_property_aliases(query, &["border-color"]) {
        Some(PropertyValue::Color(c)) => *c,
        _ => fg,
    };

    // Upstream `widget.c:46-78`: `padding` and `margin` are resolved once at `widget_init`
    // (stateless `box_metrics_query`), whereas `border` and `border-radius` update on state change.
    let zero = Padding::all(Distance::px(0.0));
    let (border, border_styles) =
        pad_to_px(&sheet.get_padding(query, "border", zero.clone()), metrics);
    let (border_radius, _) = pad_to_px(
        &sheet.get_padding(query, "border-radius", zero.clone()),
        metrics,
    );
    let (padding, _) = pad_to_px(
        &sheet.get_padding(box_metrics_query, "padding", zero.clone()),
        metrics,
    );
    let (margin, _) = pad_to_px(
        &sheet.get_padding(box_metrics_query, "margin", zero),
        metrics,
    );
    let default_spacing = if box_metrics_query.name == "element" {
        Distance::px(5.0)
    } else {
        Distance::px(2.0)
    };
    let spacing = sheet
        .get_distance(box_metrics_query, "spacing", default_spacing)
        .to_pixels(Orientation::Vertical, metrics)
        .max(0.0) as f32;

    let font = match sheet.find_property_aliases(query, &["font", "text-font"]) {
        Some(PropertyValue::String(s)) => s.clone(),
        Some(PropertyValue::Inherit) => parent_font.to_string(),
        _ if parent_font != default_font => parent_font.to_string(),
        _ => default_font.to_string(),
    };

    let placeholder_color = match sheet.find_property_aliases(query, &["placeholder-color"]) {
        Some(PropertyValue::Color(c)) => *c,
        Some(PropertyValue::Inherit) => fg.with_alpha((fg.a * 0.65).clamp(0.45, 0.85)),
        _ => fg.with_alpha(0.55),
    };

    let default_halign =
        if matches!(query.name, "element-icon" | "button") || query.name.starts_with("icon-") {
            0.5
        } else {
            0.0
        };
    // Upstream `textbox.c:275`: `vertical-align` defaults to `0.5` for buttons/icons, and for
    // single-line textboxes we center unless asymmetric vertical padding implies top-alignment.
    let default_valign =
        if query.name.starts_with("textbox") && (padding[0] - padding[2]).abs() > 1.0 {
            0.0
        } else {
            0.5
        };

    WidgetStyle {
        background: bg,
        foreground: fg,
        background_image: sheet.get_image(query, "background-image").cloned(),
        border_color,
        border,
        border_styles,
        border_radius,
        padding,
        margin,
        spacing,
        highlight: sheet.get_highlight(
            query,
            "highlight",
            HighlightColorStyle {
                style: highlight_flags::BOLD | highlight_flags::UNDERLINE,
                color: fg,
            },
        ),
        font,
        placeholder: sheet.get_string(query, "placeholder", "").to_string(),
        placeholder_color,
        horizontal_align: sheet
            .get_double(query, "horizontal-align", default_halign)
            .clamp(0.0, 1.0) as f32,
        vertical_align: sheet
            .get_double(query, "vertical-align", default_valign)
            .clamp(0.0, 1.0) as f32,
        expand: sheet.get_boolean(box_metrics_query, "expand", default_expand),
        cursor: sheet.get_cursor(query, "cursor", CursorType::Default),
        hide_cursor_on_empty: sheet.get_boolean(query, "hide-cursor-on-empty", false),
        cursor_width: sheet
            .get_distance(query, "cursor-width", Distance::px(1.5))
            .to_pixels(Orientation::Horizontal, metrics)
            .clamp(1.0, 8.0) as f32,
        cursor_color: match sheet.find_property_aliases(query, &["cursor-color"]) {
            Some(PropertyValue::Color(c)) => *c,
            Some(PropertyValue::Inherit) => parent_fg,
            _ => fg,
        },
    }
}

/// Kind of each widget node emitted into the flattened paint & hit-test list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderWidgetKind {
    Box,
    Prompt,
    Entry,
    CaseIndicator,
    NumFilteredRows,
    NumRows,
    Overlay,
    StaticTextbox(String),
    CurrentEntryTextbox,
    MessageTextbox,
    Message,
    ListView,
    ModeSwitcher,
    Icon(Option<String>),
}

/// Positioned box or leaf widget inside the window.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutWidget {
    pub name: String,
    pub kind: RenderWidgetKind,
    pub rect: Rect,
    pub paint_rect: Rect,
    pub inner_rect: Rect,
    pub children_indices: Vec<usize>,
    pub style: WidgetStyle,
    pub action: Option<String>,
}

/// Positioned row/grid cell inside `listview`.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutCell {
    pub filtered_pos: usize,
    pub entry_index: usize,
    pub rect: Rect,
    pub icon_box_rect: Option<Rect>,
    pub icon_rect: Option<Rect>,
    pub icon_style: WidgetStyle,
    pub text_box_rect: Rect,
    pub text_rect: Rect,
    pub row_style: WidgetStyle,
    pub text_style: WidgetStyle,
    pub state_name: &'static str,
}

/// Positioned mode tab inside `mode-switcher` / `sidebar`.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutModeButton {
    pub mode_index: usize,
    pub label: String,
    pub rect: Rect,
    pub style: WidgetStyle,
}

/// Optional callback measuring `(width, height)` of `(font_spec, text)` in DIPs.
pub type TextMeasureFn<'a> = &'a dyn Fn(&str, &str) -> (f32, f32);

/// Optional callback measuring wrapped text height in DIPs for `(font_spec, text, max_width)`.
pub type WrappedHeightFn<'a> = &'a dyn Fn(&str, &str, f32) -> f32;

/// Complete laid-out UI frame ready for Direct2D painting and mouse/key interaction.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutTree {
    pub window_rect: Rect,
    pub widgets: Vec<LaidOutWidget>,
    pub entry_rect: Rect,
    pub listview_rect: Rect,
    pub listview_columns: usize,
    pub listview_lines: usize,
    pub listview_page_size: usize,
    pub cells: Vec<LaidOutCell>,
    pub scrollbar_track: Option<Rect>,
    pub scrollbar_thumb: Option<Rect>,
    pub scrollbar_style: WidgetStyle,
    pub mode_buttons: Vec<LaidOutModeButton>,
}

impl LaidOutTree {
    /// Builds and computes full geometry for `sheet` + `state` on `workarea`.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        sheet: &ThemeSheet,
        state: &LauncherState,
        workarea: Rect,
        metrics: &DistanceMetrics,
        default_font: &str,
        location: WindowLocation,
        x_offset: i32,
        y_offset: i32,
        sidebar_mode: bool,
        show_icons: bool,
    ) -> Self {
        Self::build_with_options(
            sheet,
            state,
            workarea,
            metrics,
            default_font,
            location,
            x_offset,
            y_offset,
            sidebar_mode,
            show_icons,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_with_options(
        sheet: &ThemeSheet,
        state: &LauncherState,
        workarea: Rect,
        metrics: &DistanceMetrics,
        default_font: &str,
        location: WindowLocation,
        x_offset: i32,
        y_offset: i32,
        sidebar_mode: bool,
        show_icons: bool,
        hide_icons: bool,
    ) -> Self {
        Self::build_with_measurers(
            sheet,
            state,
            workarea,
            metrics,
            default_font,
            location,
            x_offset,
            y_offset,
            sidebar_mode,
            show_icons,
            hide_icons,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_with_measurer(
        sheet: &ThemeSheet,
        state: &LauncherState,
        workarea: Rect,
        metrics: &DistanceMetrics,
        default_font: &str,
        location: WindowLocation,
        x_offset: i32,
        y_offset: i32,
        sidebar_mode: bool,
        show_icons: bool,
        hide_icons: bool,
        measure_text: Option<TextMeasureFn<'_>>,
    ) -> Self {
        Self::build_with_measurers(
            sheet,
            state,
            workarea,
            metrics,
            default_font,
            location,
            x_offset,
            y_offset,
            sidebar_mode,
            show_icons,
            hide_icons,
            measure_text,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_with_measurers(
        sheet: &ThemeSheet,
        state: &LauncherState,
        workarea: Rect,
        metrics: &DistanceMetrics,
        default_font: &str,
        location: WindowLocation,
        x_offset: i32,
        y_offset: i32,
        sidebar_mode: bool,
        show_icons: bool,
        hide_icons: bool,
        measure_text: Option<TextMeasureFn<'_>>,
        measure_wrapped_h: Option<WrappedHeightFn<'_>>,
    ) -> Self {
        let init_fg = root_initial_foreground(sheet);
        let q_win = WidgetQuery::new("window");
        let win_style = resolve_widget_style_with_parent(
            sheet,
            &q_win,
            metrics,
            default_font,
            true,
            init_fg,
            default_font,
        );
        let fullscreen = sheet.get_boolean(&q_win, "fullscreen", false);
        // Upstream `xcb/view.c:280-283`: `anchor` defaults to `win_loc`, not CLI `location`.
        let win_loc = sheet.get_position(&q_win, "location", location);
        let anchor = sheet.get_position(&q_win, "anchor", win_loc);
        let win_ori = sheet.get_orientation(&q_win, "orientation", Orientation::Vertical);

        let lv_q = WidgetQuery::new("listview");
        let lv_pack_ori = sheet.get_orientation(&lv_q, "layout", Orientation::Vertical);
        let raw_lines = sheet.get_integer(&lv_q, "lines", 15);
        let est_lines = if raw_lines <= 0 {
            10
        } else {
            raw_lines as usize
        };
        let columns = sheet.get_integer(&lv_q, "columns", 1).max(1) as usize;
        let flow = sheet.get_orientation(&lv_q, "flow", Orientation::Vertical);
        let show_scrollbar = sheet.get_boolean(&lv_q, "scrollbar", false);
        let reverse = sheet.get_boolean(&lv_q, "reverse", false);
        let fixed_height = sheet.get_boolean(&lv_q, "fixed-height", true);
        let fixed_columns = sheet.get_boolean(&lv_q, "fixed-columns", false);

        // Upstream `textbox.c:1123-1126`: single-line height is `1.0 * char_height`.
        let row_h = metrics.char_height.ceil().max(14.0) as f32;
        let base_font_pt = parse_rofi_font(&win_style.font).size_pt.max(6.0);
        let default_win_w = (metrics.monitor_width * 0.50).clamp(320.0, 960.0) as f32;
        let configured_w = sheet
            .find_property(&q_win, "width")
            .and_then(|_| {
                let px = sheet
                    .get_distance(&q_win, "width", Distance::px(0.0))
                    .to_pixels(Orientation::Horizontal, metrics) as f32;
                (px > 0.0).then_some(px)
            })
            .unwrap_or(default_win_w);

        let visible_lines = if fixed_height {
            est_lines
        } else {
            let needed = state.filtered.len().div_ceil(columns).max(1);
            needed.min(est_lines)
        };

        let elem_q = WidgetQuery::new("element").with_parent(&lv_q);
        let elem_ori = sheet.get_orientation(&elem_q, "orientation", Orientation::Horizontal);
        let elem_children = sheet.get_list_strings(&elem_q, "children");
        let elem_wants_text = elem_children
            .as_ref()
            .is_none_or(|ch| ch.iter().any(|c| c == "element-text"));
        let elem_explicit_icon = elem_children
            .as_ref()
            .is_some_and(|ch| ch.iter().any(|c| c == "element-icon"));
        let elem_allows_icon = elem_children
            .as_ref()
            .is_none_or(|ch| ch.iter().any(|c| c == "element-icon"));
        let icon_first = elem_children.as_ref().is_none_or(|ch| {
            let i_pos = ch.iter().position(|c| c == "element-icon");
            let t_pos = ch.iter().position(|c| c == "element-text");
            match (i_pos, t_pos) {
                (Some(i), Some(t)) => i <= t,
                _ => true,
            }
        });

        // Upstream `icon.c:164, 174`: default icon size is 16px (`1em` in default_theme.rasi)
        // and `%` distances scale along `Orientation::Vertical`.
        let icon_q = WidgetQuery::new("element-icon").with_parent(&elem_q);
        let max_icon_px = workarea.h.max(256.0);
        let icon_size = sheet
            .get_distance(&icon_q, "size", Distance::px(16.0))
            .to_pixels(Orientation::Vertical, metrics)
            .clamp(4.0, f64::from(max_icon_px)) as f32;
        let icon_w = sheet
            .find_widget_property_aliases("element-icon", None, &["width"])
            .map(|_| {
                sheet
                    .get_distance(&icon_q, "width", Distance::px(f64::from(icon_size)))
                    .to_pixels(Orientation::Horizontal, metrics)
                    .clamp(4.0, f64::from(workarea.w.max(256.0))) as f32
            })
            .unwrap_or(icon_size);
        let icon_h = sheet
            .find_widget_property_aliases("element-icon", None, &["height"])
            .map(|_| {
                sheet
                    .get_distance(&icon_q, "height", Distance::px(f64::from(icon_size)))
                    .to_pixels(Orientation::Vertical, metrics)
                    .clamp(4.0, f64::from(max_icon_px)) as f32
            })
            .unwrap_or(icon_size);
        let icon_base_style = resolve_widget_style_with_parent(
            sheet,
            &icon_q,
            metrics,
            default_font,
            false,
            win_style.foreground,
            &win_style.font,
        );
        let icon_pad_v = icon_base_style.padding[0]
            + icon_base_style.padding[2]
            + icon_base_style.border[0]
            + icon_base_style.border[2]
            + icon_base_style.margin[0]
            + icon_base_style.margin[2];
        let any_entry_has_icon =
            state.entries.is_empty() || state.entries.iter().any(|e| e.icon.is_some());
        let icons_active = !hide_icons
            && elem_allows_icon
            && (any_entry_has_icon || !elem_wants_text)
            && (show_icons || elem_explicit_icon || state.entries.iter().any(|e| e.icon.is_some()));

        let elem_style = resolve_widget_style_with_parent(
            sheet,
            &elem_q,
            metrics,
            default_font,
            true,
            win_style.foreground,
            &win_style.font,
        );
        let elem_txt_base_q = WidgetQuery::new("element-text").with_parent(&elem_q);
        let elem_txt_base_style = resolve_widget_style_with_parent(
            sheet,
            &elem_txt_base_q,
            metrics,
            default_font,
            true,
            elem_style.foreground,
            &elem_style.font,
        );
        let elem_txt_pt = parse_rofi_font(&elem_txt_base_style.font).size_pt.max(6.0);
        let elem_line_h = (metrics.char_height as f32 * (elem_txt_pt / base_font_pt))
            .ceil()
            .max(12.0);
        let txt_pad_v = elem_txt_base_style.padding[0]
            + elem_txt_base_style.padding[2]
            + elem_txt_base_style.border[0]
            + elem_txt_base_style.border[2]
            + elem_txt_base_style.margin[0]
            + elem_txt_base_style.margin[2];
        let elem_pad_v = elem_style.padding[0]
            + elem_style.padding[2]
            + elem_style.border[0]
            + elem_style.border[2]
            + elem_style.margin[0]
            + elem_style.margin[2];
        let elem_h = if icons_active && !elem_wants_text {
            (icon_h + icon_pad_v + elem_pad_v).ceil().max(16.0)
        } else if icons_active && elem_ori == Orientation::Vertical {
            (icon_h + icon_pad_v + elem_style.spacing + elem_line_h + txt_pad_v + elem_pad_v)
                .ceil()
                .max(24.0)
        } else if icons_active {
            ((elem_line_h + txt_pad_v).max(icon_h + icon_pad_v) + elem_pad_v)
                .ceil()
                .max(16.0)
        } else {
            (elem_line_h + txt_pad_v + elem_pad_v).ceil().max(16.0)
        };
        let lv_spacing = sheet
            .get_distance(&lv_q, "spacing", Distance::px(2.0))
            .to_pixels(Orientation::Vertical, metrics)
            .max(0.0) as f32;
        let lv_style_est = resolve_widget_style_with_parent(
            sheet,
            &lv_q,
            metrics,
            default_font,
            true,
            win_style.foreground,
            &win_style.font,
        );
        let lv_pad_v = lv_style_est.padding[0]
            + lv_style_est.padding[2]
            + lv_style_est.border[0]
            + lv_style_est.border[2]
            + lv_style_est.margin[0]
            + lv_style_est.margin[2];
        let desired_lv_h = if lv_pack_ori == Orientation::Horizontal {
            (elem_h + lv_pad_v).ceil()
        } else {
            (visible_lines as f32 * elem_h
                + visible_lines.saturating_sub(1) as f32 * lv_spacing
                + lv_pad_v)
                .ceil()
        };

        let win_children = sheet
            .get_list_strings(&q_win, "children")
            .unwrap_or_else(|| vec!["mainbox".to_string()]);
        let win_explicit_children = sheet.get_list_strings(&q_win, "children").is_some();

        let mut builder = TreeBuilder {
            sheet,
            state,
            metrics,
            default_font,
            sidebar_mode,
            show_icons,
            row_h,
            base_font_pt,
            desired_lv_h,
            measure_text,
            measure_wrapped_h,
            widgets: Vec::new(),
            entry_rect: Rect::ZERO,
            listview_rect: Rect::ZERO,
            listview_style: lv_style_est,
            mode_switcher_rect: None,
            mode_switcher_name: "mode-switcher".to_string(),
            mode_switcher_style: WidgetStyle::default(),
        };

        let win_pad_h = win_style.padding[1]
            + win_style.padding[3]
            + win_style.border[1]
            + win_style.border[3]
            + win_style.margin[1]
            + win_style.margin[3];
        let win_pad_v = win_style.padding[0]
            + win_style.padding[2]
            + win_style.border[0]
            + win_style.border[2]
            + win_style.margin[0]
            + win_style.margin[2];
        let est_avail = Rect::new(
            0.0,
            0.0,
            (configured_w - win_pad_h).max(64.0),
            (workarea.h - win_pad_v).max(40.0),
        );
        let (_, est_content_h) = builder.estimate_children_size(
            &q_win,
            &win_children,
            est_avail,
            win_ori,
            win_style.spacing,
            win_explicit_children,
            win_style.background,
            win_style.foreground,
            &win_style.font,
            0,
        );
        let default_win_h = (win_pad_v + est_content_h as f32).ceil().max(14.0);

        let configured_h = sheet
            .find_property(&q_win, "height")
            .and_then(|_| {
                let px = sheet
                    .get_distance(&q_win, "height", Distance::px(0.0))
                    .to_pixels(Orientation::Vertical, metrics) as f32;
                (px > 0.0).then_some(px)
            })
            .unwrap_or(default_win_h);

        let x_off_px = sheet
            .find_property(&q_win, "x-offset")
            .map(|_| {
                sheet
                    .get_distance(&q_win, "x-offset", Distance::px(0.0))
                    .to_pixels(Orientation::Horizontal, metrics) as f32
            })
            .unwrap_or(x_offset as f32);
        let y_off_px = sheet
            .find_property(&q_win, "y-offset")
            .map(|_| {
                sheet
                    .get_distance(&q_win, "y-offset", Distance::px(0.0))
                    .to_pixels(Orientation::Vertical, metrics) as f32
            })
            .unwrap_or(y_offset as f32);

        let window_rect = compute_window_rect(
            workarea,
            configured_w,
            configured_h,
            win_loc,
            anchor,
            x_off_px,
            y_off_px,
            fullscreen,
        );

        let local_win = Rect::new(0.0, 0.0, window_rect.w, window_rect.h);
        let win_paint = margin_inset_with_style(local_win, &win_style);
        let win_inner = inset_with_style(local_win, &win_style);
        builder.widgets.push(LaidOutWidget {
            name: "window".to_string(),
            kind: RenderWidgetKind::Box,
            rect: local_win,
            paint_rect: win_paint,
            inner_rect: win_inner,
            children_indices: Vec::new(),
            style: win_style.clone(),
            action: None,
        });

        builder.layout_container(
            0,
            &q_win,
            win_inner,
            win_ori,
            win_style.spacing,
            &win_children,
            win_style.background,
            win_style.foreground,
            &win_style.font,
            0,
        );

        let lv_style_resolved = builder.listview_style;
        let ms_name = builder.mode_switcher_name;
        let ms_style_resolved = builder.mode_switcher_style;
        let entry_rect = builder.entry_rect;
        let listview_rect = builder.listview_rect;
        let mode_switcher_rect = builder.mode_switcher_rect;
        let widgets = builder.widgets;

        // Compute listview item cells and scrollbar using Rofi's fixed natural `element_height`
        let lv_inner = inset_with_style(listview_rect, &lv_style_resolved);
        let sb_q = WidgetQuery::new("scrollbar").with_parent(&lv_q);
        let sb_style = resolve_widget_style_with_parent(
            sheet,
            &sb_q,
            metrics,
            default_font,
            false,
            lv_style_resolved.foreground,
            &lv_style_resolved.font,
        );
        let sb_handle_w = if sheet.find_property(&sb_q, "handle-width").is_some() {
            sheet
                .get_distance(&sb_q, "handle-width", Distance::px(8.0))
                .to_pixels(Orientation::Horizontal, metrics)
                .max(2.0) as f32
        } else if sheet.find_property(&sb_q, "width").is_some() {
            sheet
                .get_distance(&sb_q, "width", Distance::px(8.0))
                .to_pixels(Orientation::Horizontal, metrics)
                .max(2.0) as f32
        } else {
            8.0
        };
        let sb_outer_w = (sb_handle_w
            + sb_style.padding[1]
            + sb_style.padding[3]
            + sb_style.border[1]
            + sb_style.border[3]
            + sb_style.margin[1]
            + sb_style.margin[3])
            .max(2.0);

        let effective_lines = if lv_pack_ori == Orientation::Horizontal {
            1
        } else if lv_inner.h <= 0.0 {
            est_lines.max(1)
        } else {
            ((lv_inner.h + lv_spacing) / (elem_h + lv_spacing))
                .floor()
                .max(1.0) as usize
        };

        let (raw_cells, scrollbar_track, scrollbar_thumb, effective_columns, page_size) =
            if lv_pack_ori == Orientation::Horizontal {
                let mut h_cells = Vec::new();
                if !state.filtered.is_empty() && lv_inner.w > 0.0 && lv_inner.h > 0.0 {
                    let start = state.scroll_offset.min(state.filtered.len() - 1);
                    let mut x = lv_inner.x;
                    let right = lv_inner.x + lv_inner.w;
                    let ch_w = metrics.char_width as f32;
                    let elem_pad_h = elem_style.padding[1]
                        + elem_style.padding[3]
                        + elem_style.border[1]
                        + elem_style.border[3]
                        + elem_style.margin[1]
                        + elem_style.margin[3]
                        + elem_txt_base_style.padding[1]
                        + elem_txt_base_style.padding[3]
                        + elem_txt_base_style.border[1]
                        + elem_txt_base_style.border[3]
                        + elem_txt_base_style.margin[1]
                        + elem_txt_base_style.margin[3];
                    for pos in start..state.filtered.len().min(start + est_lines.max(1)) {
                        let e_idx = state.filtered[pos];
                        let lbl = state.entries[e_idx].visible_label();
                        let lbl_w = if let Some(m) = measure_text {
                            m(&elem_txt_base_style.font, lbl).0
                        } else {
                            fallback_text_width(lbl, ch_w)
                        };
                        let icon_extra = if icons_active {
                            icon_size + elem_style.spacing
                        } else {
                            0.0
                        };
                        let item_w = (icon_extra + lbl_w + elem_pad_h + 10.0)
                            .ceil()
                            .clamp(24.0, lv_inner.w);
                        if !h_cells.is_empty() && x + item_w > right {
                            break;
                        }
                        let w = item_w.min((right - x).max(1.0));
                        h_cells.push((pos, Rect::new(x, lv_inner.y, w, lv_inner.h)));
                        x += w + lv_spacing;
                        if x >= right {
                            break;
                        }
                    }
                }
                let count = h_cells.len().max(1);
                (h_cells, None, None, count, count)
            } else {
                let lv_layout = layout_listview_fixed_row_height(
                    &literofi_render_spec(
                        lv_inner,
                        columns,
                        effective_lines,
                        lv_spacing,
                        flow,
                        show_scrollbar,
                        sb_outer_w,
                        state.filtered.len(),
                        state.scroll_offset,
                    ),
                    elem_h,
                    fixed_columns,
                );
                let eff_cols = if !fixed_columns
                    && state.filtered.len() < lv_layout.page_size
                    && !state.filtered.is_empty()
                {
                    match flow {
                        Orientation::Vertical => state
                            .filtered
                            .len()
                            .div_ceil(effective_lines)
                            .clamp(1, columns),
                        Orientation::Horizontal => state.filtered.len().min(columns).max(1),
                    }
                } else {
                    columns
                };
                let sb_thumb_padded = lv_layout.scrollbar_track.map(|outer_sb| {
                    let sb_inner = inset_with_style(outer_sb, &sb_style);
                    let total_items = state.filtered.len();
                    let psize = lv_layout.page_size.max(1);
                    let thumb_w = sb_handle_w.min(sb_inner.w.max(1.0));
                    let thumb_x = sb_inner.x + ((sb_inner.w - thumb_w).max(0.0) * 0.5);
                    if sb_inner.h <= 1.0 {
                        Rect::new(thumb_x, sb_inner.y, thumb_w, 0.0)
                    } else if total_items <= psize {
                        Rect::new(thumb_x, sb_inner.y, thumb_w, sb_inner.h)
                    } else {
                        let vis_frac = (psize as f32 / total_items as f32).clamp(0.08, 1.0);
                        let thumb_h = (sb_inner.h * vis_frac).max(20.0).min(sb_inner.h);
                        let max_off = total_items.saturating_sub(psize).max(1) as f32;
                        let start = state.scroll_offset.min(total_items.saturating_sub(1)) as f32;
                        let frac = (start / max_off).clamp(0.0, 1.0);
                        let thumb_y = sb_inner.y + (sb_inner.h - thumb_h).max(0.0) * frac;
                        Rect::new(thumb_x, thumb_y, thumb_w, thumb_h)
                    }
                });
                (
                    lv_layout.cells,
                    lv_layout.scrollbar_track,
                    sb_thumb_padded,
                    eff_cols,
                    lv_layout.page_size,
                )
            };

        let mut cells = Vec::with_capacity(raw_cells.len());
        let q_row_base = WidgetQuery::new("element").with_parent(&lv_q);
        for (filtered_pos, mut cell_rect) in raw_cells {
            if reverse && lv_pack_ori == Orientation::Vertical {
                cell_rect.y = lv_inner.y + lv_inner.h - (cell_rect.y - lv_inner.y) - cell_rect.h;
            }
            let entry_index = state.filtered[filtered_pos];
            let entry = &state.entries[entry_index];
            let state_name = cell_state_name(
                entry,
                filtered_pos == state.selected,
                (filtered_pos % 2) == 1,
            );
            // Upstream `widget.c:46-78`: `padding`/`margin` come from stateless `q_row_base`,
            // while `border`, `border-radius`, `background-color`, `text-color` update per state.
            let q_row = WidgetQuery::new("element")
                .with_parent(&lv_q)
                .with_state(state_name);
            let row_style = resolve_widget_style_inner(
                sheet,
                &q_row,
                &q_row_base,
                metrics,
                default_font,
                true,
                lv_style_resolved.background,
                lv_style_resolved.foreground,
                &lv_style_resolved.font,
            );
            let q_txt_base = WidgetQuery::new("element-text").with_parent(&q_row_base);
            let q_txt = WidgetQuery::new("element-text")
                .with_parent(&q_row)
                .with_state(state_name);
            let text_style = resolve_widget_style_inner(
                sheet,
                &q_txt,
                &q_txt_base,
                metrics,
                default_font,
                true,
                row_style.background,
                row_style.foreground,
                &row_style.font,
            );
            let q_ico_base = WidgetQuery::new("element-icon").with_parent(&q_row_base);
            let q_ico = WidgetQuery::new("element-icon")
                .with_parent(&q_row)
                .with_state(state_name);
            let ico_style = resolve_widget_style_inner(
                sheet,
                &q_ico,
                &q_ico_base,
                metrics,
                default_font,
                false,
                row_style.background,
                row_style.foreground,
                &row_style.font,
            );

            let inner = inset_with_style(cell_rect, &row_style);
            let entry_has_icon = state
                .entries
                .get(entry_index)
                .is_some_and(|e| e.icon.is_some());
            // For rectangular thumbnail cards (e.g. mixed image/text clipboard lists),
            // collapse the thumbnail slot on text-only rows so text aligns cleanly to the left.
            let has_icon = icons_active
                && (entry_has_icon || !elem_wants_text || (icon_w - icon_h).abs() <= 0.5);
            let has_text = elem_wants_text;

            let fit_icon_box = |slot: Rect| -> Rect {
                let padded = inset_with_style(slot, &ico_style);
                let (bw, bh) = if (icon_w - icon_h).abs() > 0.5 {
                    (icon_w.min(padded.w).max(0.0), icon_h.min(padded.h).max(0.0))
                } else {
                    let side = icon_size.min(padded.w).min(padded.h).max(0.0);
                    (side, side)
                };
                let ix = padded.x + (padded.w - bw) * ico_style.horizontal_align;
                let iy = padded.y + (padded.h - bh) * ico_style.vertical_align;
                Rect::new(ix, iy, bw, bh)
            };

            let (icon_box_rect, icon_rect, text_box_rect, text_rect) = match (has_icon, has_text) {
                (true, false) => (
                    Some(inner),
                    Some(fit_icon_box(inner)),
                    Rect::ZERO,
                    Rect::ZERO,
                ),
                (false, true) => (None, None, inner, inset_with_style(inner, &text_style)),
                (false, false) => (None, None, Rect::ZERO, Rect::ZERO),
                (true, true) => {
                    let ico_box_w = (icon_w
                        + ico_style.padding[1]
                        + ico_style.padding[3]
                        + ico_style.border[1]
                        + ico_style.border[3]
                        + ico_style.margin[1]
                        + ico_style.margin[3])
                        .round() as i32;
                    let ico_box_h = (icon_h
                        + ico_style.padding[0]
                        + ico_style.padding[2]
                        + ico_style.border[0]
                        + ico_style.border[2]
                        + ico_style.margin[0]
                        + ico_style.margin[2])
                        .round() as i32;
                    let ico_spec = BoxChildSpec {
                        enabled: true,
                        expand: ico_style.expand,
                        desired_w: ico_box_w,
                        desired_h: ico_box_h,
                    };
                    let txt_spec = BoxChildSpec {
                        enabled: true,
                        expand: text_style.expand,
                        desired_w: (inner.w.round() as i32).max(0),
                        desired_h: (elem_line_h + txt_pad_v).ceil() as i32,
                    };
                    let specs = if icon_first {
                        [ico_spec, txt_spec]
                    } else {
                        [txt_spec, ico_spec]
                    };
                    let parts = layout_box(
                        inner,
                        elem_ori,
                        (0, 0, 0, 0),
                        row_style.spacing.round() as i32,
                        &specs,
                    );
                    let (raw_ico, raw_txt) = if icon_first {
                        (parts[0], parts[1])
                    } else {
                        (parts[1], parts[0])
                    };
                    (
                        Some(raw_ico),
                        Some(fit_icon_box(raw_ico)),
                        raw_txt,
                        inset_with_style(raw_txt, &text_style),
                    )
                }
            };

            cells.push(LaidOutCell {
                filtered_pos,
                entry_index,
                rect: margin_inset_with_style(cell_rect, &row_style),
                icon_box_rect,
                icon_rect,
                icon_style: ico_style,
                text_box_rect,
                text_rect,
                row_style,
                text_style,
                state_name,
            });
        }

        // Mode switcher buttons if active (`mode_switcher_rect` is the outer `rect`, inset once here)
        let mut mode_buttons = Vec::new();
        if let Some(ms_rect) = mode_switcher_rect {
            if !state.modes.is_empty() && ms_rect.w > 0.0 && ms_rect.h > 0.0 {
                let ms_q = WidgetQuery::new(&ms_name);
                let ms_ori = sheet.get_orientation(&ms_q, "orientation", Orientation::Horizontal);
                let ms_inner = inset_with_style(ms_rect, &ms_style_resolved);
                let btn_gap = ms_style_resolved.spacing.round() as i32;
                let b_base_q = WidgetQuery::new("button").with_parent(&ms_q);
                let b_base_style = resolve_widget_style_inner(
                    sheet,
                    &b_base_q,
                    &b_base_q,
                    metrics,
                    default_font,
                    true,
                    ms_style_resolved.background,
                    ms_style_resolved.foreground,
                    &ms_style_resolved.font,
                );
                let b_pad_h = b_base_style.padding[0]
                    + b_base_style.padding[2]
                    + b_base_style.border[0]
                    + b_base_style.border[2]
                    + b_base_style.margin[0]
                    + b_base_style.margin[2];
                let b_explicit_w = sheet.find_property(&b_base_q, "width").and_then(|_| {
                    let px = sheet
                        .get_distance(&b_base_q, "width", Distance::px(0.0))
                        .to_pixels(Orientation::Horizontal, metrics)
                        as f32;
                    (px > 0.0).then_some(px.round() as i32)
                });
                let specs: Vec<BoxChildSpec> = state
                    .modes
                    .iter()
                    .map(|lbl| {
                        let txt_w = if let Some(m) = measure_text {
                            m(&b_base_style.font, lbl).0
                        } else {
                            fallback_text_width(lbl, metrics.char_width as f32)
                        };
                        let lbl_w = (txt_w
                            + b_base_style.padding[1]
                            + b_base_style.padding[3]
                            + b_base_style.border[1]
                            + b_base_style.border[3]
                            + b_base_style.margin[1]
                            + b_base_style.margin[3]
                            + 6.0)
                            .ceil() as i32;
                        BoxChildSpec {
                            enabled: true,
                            expand: b_base_style.expand,
                            desired_w: b_explicit_w.unwrap_or(lbl_w),
                            desired_h: (row_h + b_pad_h).ceil() as i32,
                        }
                    })
                    .collect();
                let btn_rects = layout_box(ms_inner, ms_ori, (0, 0, 0, 0), btn_gap, &specs);
                for (i, (label, brect)) in state.modes.iter().zip(btn_rects).enumerate() {
                    let b_q = if i == state.active_mode {
                        WidgetQuery::new("button")
                            .with_parent(&ms_q)
                            .with_state("selected.normal")
                    } else {
                        WidgetQuery::new("button")
                            .with_parent(&ms_q)
                            .with_state("normal.normal")
                    };
                    let b_style = resolve_widget_style_inner(
                        sheet,
                        &b_q,
                        &b_base_q,
                        metrics,
                        default_font,
                        true,
                        ms_style_resolved.background,
                        ms_style_resolved.foreground,
                        &ms_style_resolved.font,
                    );
                    mode_buttons.push(LaidOutModeButton {
                        mode_index: i,
                        label: label.clone(),
                        rect: brect,
                        style: b_style,
                    });
                }
            }
        }

        Self {
            window_rect,
            widgets,
            entry_rect,
            listview_rect,
            listview_columns: effective_columns,
            listview_lines: effective_lines,
            listview_page_size: page_size,
            cells,
            scrollbar_track,
            scrollbar_thumb,
            scrollbar_style: sb_style,
            mode_buttons,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn literofi_render_spec(
    area: Rect,
    columns: usize,
    lines: usize,
    spacing: f32,
    flow: Orientation,
    show_scrollbar: bool,
    scrollbar_width: f32,
    total_items: usize,
    scroll_offset: usize,
) -> crate::layout::ListViewLayoutSpec {
    crate::layout::ListViewLayoutSpec {
        area,
        columns,
        lines,
        spacing_x: spacing,
        spacing_y: spacing,
        flow,
        show_scrollbar,
        scrollbar_width,
        total_items,
        scroll_offset,
    }
}

fn cell_state_name(entry: &ListEntry, is_selected: bool, is_alt: bool) -> &'static str {
    let row_prefix = if is_selected {
        "selected"
    } else if is_alt {
        "alternate"
    } else {
        "normal"
    };
    let status = if entry.active {
        "active"
    } else if entry.urgent {
        "urgent"
    } else {
        "normal"
    };
    match (row_prefix, status) {
        ("selected", "active") => "selected.active",
        ("selected", "urgent") => "selected.urgent",
        ("selected", _) => "selected.normal",
        ("alternate", "active") => "alternate.active",
        ("alternate", "urgent") => "alternate.urgent",
        ("alternate", _) => "alternate.normal",
        (_, "active") => "normal.active",
        (_, "urgent") => "normal.urgent",
        _ => "normal.normal",
    }
}

fn margin_inset_with_style(r: Rect, st: &WidgetStyle) -> Rect {
    r.inset(st.margin[0], st.margin[1], st.margin[2], st.margin[3])
}

fn inset_with_style(r: Rect, st: &WidgetStyle) -> Rect {
    r.inset(
        st.margin[0] + st.border[0] + st.padding[0],
        st.margin[1] + st.border[1] + st.padding[1],
        st.margin[2] + st.border[2] + st.padding[2],
        st.margin[3] + st.border[3] + st.padding[3],
    )
}

struct TreeBuilder<'a> {
    sheet: &'a ThemeSheet,
    state: &'a LauncherState,
    metrics: &'a DistanceMetrics,
    default_font: &'a str,
    sidebar_mode: bool,
    show_icons: bool,
    row_h: f32,
    base_font_pt: f32,
    desired_lv_h: f32,
    measure_text: Option<TextMeasureFn<'a>>,
    measure_wrapped_h: Option<WrappedHeightFn<'a>>,
    widgets: Vec<LaidOutWidget>,
    entry_rect: Rect,
    listview_rect: Rect,
    listview_style: WidgetStyle,
    mode_switcher_rect: Option<Rect>,
    mode_switcher_name: String,
    mode_switcher_style: WidgetStyle,
}

fn strip_simple_markup_tags(s: &str) -> String {
    if !s.contains('<') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

impl<'a> TreeBuilder<'a> {
    fn text_width(&self, font: &str, text: &str, ch_w: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        if let Some(m) = self.measure_text {
            m(font, text).0.max(0.0)
        } else {
            fallback_text_width(text, ch_w)
        }
    }

    fn wrapped_text_height(
        &self,
        font: &str,
        text: &str,
        max_w: f32,
        ch_w: f32,
        line_h: f32,
    ) -> f32 {
        if text.is_empty() {
            return line_h;
        }
        let eff_w = max_w.max(24.0);
        if let Some(mw) = self.measure_wrapped_h {
            return mw(font, text, eff_w).max(line_h);
        }
        let plain = strip_simple_markup_tags(text);
        let mut total_lines = 0usize;
        for para in plain.split('\n') {
            if para.is_empty() {
                total_lines += 1;
            } else {
                let w = self.text_width(font, para, ch_w);
                total_lines += (w / eff_w).ceil().max(1.0) as usize;
            }
        }
        (total_lines.max(1) as f32) * line_h
    }
    #[allow(clippy::too_many_arguments)]
    fn estimate_children_size(
        &self,
        parent_q: &WidgetQuery<'_>,
        child_names: &[String],
        avail: Rect,
        ori: Orientation,
        spacing: f32,
        explicit_in_theme: bool,
        parent_bg: ThemeColor,
        parent_fg: ThemeColor,
        parent_font: &str,
        depth: usize,
    ) -> (i32, i32) {
        let mut active = 0i32;
        let mut sum_w = 0i32;
        let mut max_w = 0i32;
        let mut max_w_non_expand = 0i32;
        let mut sum_h = 0i32;
        let mut max_h = 0i32;
        for cname in child_names {
            let (spec, _, _) = self.estimate_child(
                parent_q,
                cname,
                avail,
                explicit_in_theme,
                parent_bg,
                parent_fg,
                parent_font,
                depth + 1,
            );
            if !spec.enabled {
                continue;
            }
            active += 1;
            sum_w += spec.desired_w.max(0);
            max_w = max_w.max(spec.desired_w.max(0));
            if !spec.expand && spec.desired_w > 0 {
                max_w_non_expand = max_w_non_expand.max(spec.desired_w);
            }
            sum_h += spec.desired_h.max(0);
            max_h = max_h.max(spec.desired_h.max(0));
        }
        if active == 0 {
            return (0, 0);
        }
        let gap = (active - 1) * (spacing.round() as i32);
        match ori {
            Orientation::Horizontal => ((sum_w + gap).min(avail.w.round() as i32), max_h),
            Orientation::Vertical => {
                let eff_w = if max_w_non_expand > 0 {
                    max_w_non_expand
                } else {
                    max_w
                };
                (eff_w.min(avail.w.round() as i32), sum_h + gap)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn estimate_child(
        &self,
        parent_q: &WidgetQuery<'_>,
        name: &str,
        avail: Rect,
        explicit_in_theme: bool,
        parent_bg: ThemeColor,
        parent_fg: ThemeColor,
        parent_font: &str,
        depth: usize,
    ) -> (BoxChildSpec, Orientation, WidgetStyle) {
        let q = match default_widget_state(name) {
            Some(st) => WidgetQuery::new(name).with_parent(parent_q).with_state(st),
            None => WidgetQuery::new(name).with_parent(parent_q),
        };
        let sub_children = if depth < 8 {
            self.sheet
                .get_list_strings(&q, "children")
                .or_else(|| self.default_children_for(name))
        } else {
            None
        };

        // Upstream `view.c:1663-1839` default `expand` values.
        let default_expand = !matches!(
            name,
            "inputbar"
                | "prompt"
                | "num-rows"
                | "num-filtered-rows"
                | "textbox-current-entry"
                | "icon-current-entry"
                | "case-indicator"
                | "message"
                | "mode-switcher"
                | "sidebar"
                | "overlay"
        );

        let style = resolve_widget_style_inner(
            self.sheet,
            &q,
            &q,
            self.metrics,
            self.default_font,
            default_expand,
            parent_bg,
            parent_fg,
            parent_font,
        );

        let default_ori = if matches!(
            name,
            "inputbar" | "mode-switcher" | "sidebar" | "listview-split" | "horibox"
        ) {
            Orientation::Horizontal
        } else {
            Orientation::Vertical
        };
        let ori = self.sheet.get_orientation(&q, "orientation", default_ori);

        let font_scale =
            (parse_rofi_font(&style.font).size_pt.max(6.0) / self.base_font_pt).clamp(0.5, 4.0);
        let line_h = (self.row_h * font_scale).ceil().max(12.0);
        let ch_w = (self.metrics.char_width as f32 * font_scale).max(4.0);
        let pad_w = style.padding[1]
            + style.padding[3]
            + style.border[1]
            + style.border[3]
            + style.margin[1]
            + style.margin[3];
        let pad_h = style.padding[0]
            + style.padding[2]
            + style.border[0]
            + style.border[2]
            + style.margin[0]
            + style.margin[2];

        let explicit_w = self.sheet.find_property(&q, "width").and_then(|_| {
            let px = self
                .sheet
                .get_distance(&q, "width", Distance::px(0.0))
                .to_pixels(Orientation::Horizontal, self.metrics) as f32;
            (px > 0.0).then_some(px.round() as i32)
        });
        let explicit_h = self.sheet.find_property(&q, "height").and_then(|_| {
            let px = self
                .sheet
                .get_distance(&q, "height", Distance::px(0.0))
                .to_pixels(Orientation::Vertical, self.metrics) as f32;
            (px > 0.0).then_some(px.round() as i32)
        });

        let enabled = match name {
            "message" => {
                self.state.message.is_some() && self.sheet.get_boolean(&q, "enabled", true)
            }
            "overlay" => {
                self.state.overlay.is_some() && self.sheet.get_boolean(&q, "enabled", true)
            }
            "mode-switcher" | "sidebar" => {
                !self.state.modes.is_empty()
                    && self.sheet.get_boolean(&q, "enabled", true)
                    && (explicit_in_theme || self.sidebar_mode)
            }
            _ => self.sheet.get_boolean(&q, "enabled", true),
        };

        let (mut dw, mut dh) = if let Some(children) = &sub_children {
            let child_explicit = self.sheet.get_list_strings(&q, "children").is_some();
            let inner_avail = Rect::new(
                0.0,
                0.0,
                (avail.w - pad_w).max(16.0),
                (avail.h - pad_h).max(16.0),
            );
            let (cw, ch) = self.estimate_children_size(
                &q,
                children,
                inner_avail,
                ori,
                style.spacing,
                child_explicit,
                style.background,
                style.foreground,
                &style.font,
                depth,
            );
            let box_w =
                if matches!(name, "mainbox" | "message") || (name == "inputbar" && style.expand) {
                    avail.w.round() as i32
                } else {
                    (cw as f32 + pad_w).ceil() as i32
                };
            (box_w, (ch as f32 + pad_h).ceil() as i32)
        } else {
            match name {
                "pad" => (0, 0),
                n if n.starts_with("dummy") => (0, 0),
                "prompt" => {
                    let extra = if self.measure_text.is_some() {
                        2.0
                    } else {
                        4.0
                    };
                    let w = if self.state.prompt.is_empty() {
                        pad_w.ceil() as i32
                    } else {
                        (self.text_width(&style.font, &self.state.prompt, ch_w) + pad_w + extra)
                            .ceil() as i32
                    };
                    (w, (line_h + pad_h).ceil() as i32)
                }
                "case-indicator" => {
                    let extra = if self.measure_text.is_some() {
                        2.0
                    } else {
                        4.0
                    };
                    let cs = self.state.case_indicator_str();
                    let w = if cs.trim().is_empty() {
                        0
                    } else {
                        (self.text_width(&style.font, cs, ch_w) + pad_w + extra).ceil() as i32
                    };
                    (w, (line_h + pad_h).ceil() as i32)
                }
                "num-filtered-rows" | "num-rows" => {
                    let extra = if self.measure_text.is_some() {
                        2.0
                    } else {
                        4.0
                    };
                    let count_str = self.state.entries.len().to_string();
                    let s = if count_str.is_empty() {
                        "0"
                    } else {
                        count_str.as_str()
                    };
                    (
                        (self.text_width(&style.font, s, ch_w) + pad_w + extra).ceil() as i32,
                        (line_h + pad_h).ceil() as i32,
                    )
                }
                "textbox-num-sep" | "textbox-prompt-colon" => {
                    let extra = if self.measure_text.is_some() {
                        2.0
                    } else {
                        4.0
                    };
                    let s =
                        self.sheet
                            .get_string(&q, "str", self.sheet.get_string(&q, "content", ":"));
                    let w = if s.is_empty() {
                        pad_w.ceil() as i32
                    } else {
                        (self.text_width(&style.font, s, ch_w) + pad_w + extra).ceil() as i32
                    };
                    (w, (line_h + pad_h).ceil() as i32)
                }
                "textbox-current-entry" => (avail.w.round() as i32, (line_h + pad_h).ceil() as i32),
                "overlay" => {
                    let extra = if self.measure_text.is_some() {
                        2.0
                    } else {
                        4.0
                    };
                    let ov = self.state.overlay.as_deref().unwrap_or("");
                    let w = if ov.is_empty() {
                        (ch_w + pad_w + extra).ceil() as i32
                    } else {
                        (self.text_width(&style.font, ov, ch_w) + pad_w + extra).ceil() as i32
                    };
                    (w, (line_h + pad_h).ceil() as i32)
                }
                "mode-switcher" | "sidebar" => {
                    let b_q = WidgetQuery::new("button").with_parent(&q);
                    let b_style = resolve_widget_style_inner(
                        self.sheet,
                        &b_q,
                        &b_q,
                        self.metrics,
                        self.default_font,
                        true,
                        style.background,
                        style.foreground,
                        &style.font,
                    );
                    let b_pad_w = b_style.padding[1]
                        + b_style.padding[3]
                        + b_style.border[1]
                        + b_style.border[3]
                        + b_style.margin[1]
                        + b_style.margin[3];
                    let b_pad_h = b_style.padding[0]
                        + b_style.padding[2]
                        + b_style.border[0]
                        + b_style.border[2]
                        + b_style.margin[0]
                        + b_style.margin[2];
                    let b_explicit_w = self.sheet.find_property(&b_q, "width").and_then(|_| {
                        let px = self
                            .sheet
                            .get_distance(&b_q, "width", Distance::px(0.0))
                            .to_pixels(Orientation::Horizontal, self.metrics)
                            as f32;
                        (px > 0.0).then_some(px)
                    });
                    let max_lbl_w = self
                        .state
                        .modes
                        .iter()
                        .map(|m| self.text_width(&b_style.font, m, ch_w) + b_pad_w + 6.0)
                        .fold(24.0f32, f32::max);
                    let each_w = b_explicit_w.unwrap_or(max_lbl_w).max(20.0);
                    let each_h = (line_h + b_pad_h).max(16.0);
                    let n = self.state.modes.len().max(1) as f32;
                    let gap = style.spacing;
                    if ori == Orientation::Vertical {
                        (
                            (each_w + pad_w).ceil() as i32,
                            (each_h * n + gap * (n - 1.0) + pad_h).ceil() as i32,
                        )
                    } else {
                        (
                            (each_w * n + gap * (n - 1.0) + pad_w).ceil() as i32,
                            (each_h + pad_h).ceil() as i32,
                        )
                    }
                }
                "entry" => (avail.w.round() as i32, (line_h + pad_h).ceil() as i32),
                "message" | "textbox" => {
                    let text_h = if let Some(msg) = &self.state.message {
                        let inner_w = (avail.w - pad_w).max(32.0);
                        self.wrapped_text_height(&style.font, msg, inner_w, ch_w, line_h)
                    } else {
                        line_h
                    };
                    (avail.w.round() as i32, (text_h + pad_h).ceil() as i32)
                }
                "listview" => (avail.w.round() as i32, self.desired_lv_h.ceil() as i32),
                n if n.starts_with("icon-") => {
                    // Upstream `icon.c:164, 174`: default 16px, `%` scales along Vertical.
                    let raw_sz = self
                        .sheet
                        .get_distance(&q, "size", Distance::px(16.0))
                        .to_pixels(Orientation::Vertical, self.metrics)
                        .ceil() as i32;
                    let sz = if n == "icon-current-entry" && avail.h > 0.0 {
                        raw_sz.min((avail.h * 0.32).round() as i32).max(64)
                    } else {
                        raw_sz.max(4)
                    };
                    (sz + pad_w.ceil() as i32, sz + pad_h.ceil() as i32)
                }
                n if n.starts_with("textbox") || n.starts_with("button") => {
                    let s =
                        self.sheet
                            .get_string(&q, "str", self.sheet.get_string(&q, "content", ""));
                    let w = if s.is_empty() {
                        (ch_w + pad_w + 2.0).ceil() as i32
                    } else {
                        (self.text_width(&style.font, s, ch_w) + pad_w + 4.0).ceil() as i32
                    };
                    let h = if s.contains('\n') {
                        let inner_w = (avail.w - pad_w).max(32.0);
                        (self.wrapped_text_height(&style.font, s, inner_w, ch_w, line_h) + pad_h)
                            .ceil() as i32
                    } else {
                        (line_h + pad_h).ceil() as i32
                    };
                    (w, h)
                }
                _ => {
                    // Upstream `view.c:1836`: unknown custom names are boxes (`box_create`),
                    // not textboxes, unless they explicitly carry `str`/`content`.
                    let s =
                        self.sheet
                            .get_string(&q, "str", self.sheet.get_string(&q, "content", ""));
                    if s.is_empty() {
                        (pad_w.ceil() as i32, pad_h.ceil() as i32)
                    } else {
                        (
                            (self.text_width(&style.font, s, ch_w) + pad_w + 4.0).ceil() as i32,
                            (line_h + pad_h).ceil() as i32,
                        )
                    }
                }
            }
        };

        if let Some(w) = explicit_w {
            dw = w + (style.margin[1] + style.margin[3]).round() as i32;
        }
        if let Some(h) = explicit_h {
            dh = h + (style.margin[0] + style.margin[2]).round() as i32;
        }

        (
            BoxChildSpec {
                enabled,
                expand: style.expand,
                desired_w: dw,
                desired_h: dh,
            },
            ori,
            style,
        )
    }

    fn default_children_for(&self, name: &str) -> Option<Vec<String>> {
        match name {
            "mainbox" => {
                let mut v = vec![
                    "inputbar".to_string(),
                    "message".to_string(),
                    "listview".to_string(),
                ];
                if self.sidebar_mode {
                    v.push("mode-switcher".to_string());
                }
                Some(v)
            }
            "inputbar" => Some(vec![
                "prompt".to_string(),
                "entry".to_string(),
                "overlay".to_string(),
                "case-indicator".to_string(),
            ]),
            "message" => Some(vec!["textbox".to_string()]),
            "listview-split" => Some(vec!["listview".to_string()]),
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_container(
        &mut self,
        parent_idx: usize,
        parent_q: &WidgetQuery<'_>,
        area: Rect,
        ori: Orientation,
        spacing: f32,
        child_names: &[String],
        parent_bg: ThemeColor,
        parent_fg: ThemeColor,
        parent_font: &str,
        depth: usize,
    ) {
        if depth > 8 {
            return;
        }
        let explicit_in_theme = self.sheet.get_list_strings(parent_q, "children").is_some();
        let mut specs = Vec::with_capacity(child_names.len());
        let mut metas = Vec::with_capacity(child_names.len());
        for name in child_names {
            let (spec, c_ori, style) = self.estimate_child(
                parent_q,
                name,
                area,
                explicit_in_theme,
                parent_bg,
                parent_fg,
                parent_font,
                depth,
            );
            specs.push(spec);
            metas.push((name.clone(), c_ori, style));
        }

        let rects = layout_box(area, ori, (0, 0, 0, 0), spacing.round() as i32, &specs);
        for ((name, c_ori, style), rect) in metas.into_iter().zip(rects) {
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }
            let paint_rect = margin_inset_with_style(rect, &style);
            let inner = inset_with_style(rect, &style);
            let q = match default_widget_state(&name) {
                Some(st) => WidgetQuery::new(&name).with_parent(parent_q).with_state(st),
                None => WidgetQuery::new(&name).with_parent(parent_q),
            };
            let sub_children = self
                .sheet
                .get_list_strings(&q, "children")
                .or_else(|| self.default_children_for(&name));

            let action_str = self.sheet.get_string(&q, "action", "");
            let action = (!action_str.is_empty()).then(|| action_str.to_string());

            let kind = match name.as_str() {
                "prompt" => RenderWidgetKind::Prompt,
                "entry" => {
                    self.entry_rect = inner;
                    RenderWidgetKind::Entry
                }
                "case-indicator" => RenderWidgetKind::CaseIndicator,
                "num-filtered-rows" => RenderWidgetKind::NumFilteredRows,
                "num-rows" => RenderWidgetKind::NumRows,
                "overlay" => RenderWidgetKind::Overlay,
                "textbox-current-entry" => RenderWidgetKind::CurrentEntryTextbox,
                "textbox" if parent_q.name == "message" || self.state.message.is_some() => {
                    RenderWidgetKind::MessageTextbox
                }
                "message" => {
                    if sub_children.is_some() {
                        RenderWidgetKind::Box
                    } else {
                        RenderWidgetKind::Message
                    }
                }
                "listview" => {
                    self.listview_rect = rect;
                    self.listview_style = style.clone();
                    RenderWidgetKind::ListView
                }
                "mode-switcher" | "sidebar" => {
                    self.mode_switcher_rect = Some(rect);
                    self.mode_switcher_name = name.clone();
                    self.mode_switcher_style = style.clone();
                    RenderWidgetKind::ModeSwitcher
                }
                n if n.starts_with("icon-") => {
                    let fname = self.sheet.get_string(&q, "filename", "").to_string();
                    let spec_str = if !fname.is_empty() {
                        Some(fname)
                    } else if n == "icon-current-entry" {
                        Some("icon-current-entry".to_string())
                    } else {
                        None
                    };
                    RenderWidgetKind::Icon(spec_str)
                }
                n if sub_children.is_none()
                    && (n.starts_with("textbox") || n.starts_with("button")) =>
                {
                    let s =
                        self.sheet
                            .get_string(&q, "str", self.sheet.get_string(&q, "content", ""));
                    RenderWidgetKind::StaticTextbox(s.to_string())
                }
                _ if sub_children.is_none() => {
                    let s =
                        self.sheet
                            .get_string(&q, "str", self.sheet.get_string(&q, "content", ""));
                    if s.is_empty() {
                        RenderWidgetKind::Box
                    } else {
                        RenderWidgetKind::StaticTextbox(s.to_string())
                    }
                }
                _ => RenderWidgetKind::Box,
            };

            let widget_ori = c_ori;
            let widget_spacing = style.spacing;
            let child_bg = style.background;
            let child_fg = style.foreground;
            let child_font = style.font.clone();
            let child_idx = self.widgets.len();
            self.widgets.push(LaidOutWidget {
                name: name.clone(),
                kind,
                rect,
                paint_rect,
                inner_rect: inner,
                children_indices: Vec::new(),
                style,
                action,
            });
            self.widgets[parent_idx].children_indices.push(child_idx);

            if let Some(children) = sub_children {
                self.layout_container(
                    child_idx,
                    &q,
                    inner,
                    widget_ori,
                    widget_spacing,
                    &children,
                    child_bg,
                    child_fg,
                    &child_font,
                    depth + 1,
                );
            }
        }
        let _ = self.show_icons;
    }
}
