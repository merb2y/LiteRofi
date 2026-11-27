use egui::{Color32, Margin, Pos2, Rect, Rounding, Shadow, Stroke, Visuals};

// Exact eldertuner-rs / mango-minimal.rasi palette tokens
pub const PAPER: Color32 = Color32::from_rgb(8, 7, 6); // bg: #080706
pub const PAPER_2: Color32 = Color32::from_rgb(20, 18, 16); // bg-alt: #141210
pub const PAPER_3: Color32 = Color32::from_rgb(8, 7, 6); // deep: #080706
pub const BG_SEL: Color32 = Color32::from_rgb(31, 27, 23); // bg-sel: #1f1b17

pub const INK: Color32 = Color32::from_rgb(242, 229, 188); // fg-sel: #f2e5bc
pub const INK_2: Color32 = Color32::from_rgb(216, 204, 184); // fg: #d8ccb8
pub const MUTED: Color32 = Color32::from_rgb(124, 114, 101); // fg-muted: #7c7265
pub const MUTED_2: Color32 = Color32::from_rgb(92, 84, 75);

pub const OUTER_BORDER: Color32 = Color32::from_rgb(168, 153, 132); // border: #a89984
pub const RULE: Color32 = Color32::from_rgb(43, 38, 33);
pub const RULE_2: Color32 = Color32::from_rgb(31, 27, 23); // bg-sel hairline

pub const ACCENT: Color32 = Color32::from_rgb(200, 169, 126); // accent: #c8a97e
pub const ACCENT_INK: Color32 = Color32::from_rgb(8, 7, 6);

pub const SIGNAL_BLUE: Color32 = Color32::from_rgb(181, 189, 104); // active: #b5bd68
pub const SIGNAL_BLUE_SOFT: Color32 = Color32::from_rgb(26, 28, 18);

pub const OK: Color32 = Color32::from_rgb(181, 189, 104); // active: #b5bd68
pub const WARN: Color32 = Color32::from_rgb(204, 102, 102); // urgent: #cc6666

pub fn apply_rim_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.dark_mode = true;
    visuals.override_text_color = Some(INK_2);
    visuals.panel_fill = PAPER;
    visuals.window_fill = PAPER_2;
    visuals.extreme_bg_color = PAPER_3;
    visuals.faint_bg_color = PAPER_2;
    visuals.selection.bg_fill = BG_SEL;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.window_shadow = Shadow::NONE;
    visuals.popup_shadow = Shadow::NONE;
    visuals.window_rounding = Rounding::ZERO;
    visuals.window_stroke = Stroke::new(1.0_f32, OUTER_BORDER);

    visuals.widgets.noninteractive.bg_fill = PAPER_2;
    visuals.widgets.noninteractive.weak_bg_fill = PAPER_2;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, RULE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK_2);
    visuals.widgets.noninteractive.rounding = Rounding::ZERO;

    visuals.widgets.inactive.bg_fill = PAPER_3;
    visuals.widgets.inactive.weak_bg_fill = PAPER_3;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, RULE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, INK_2);
    visuals.widgets.inactive.rounding = Rounding::ZERO;

    visuals.widgets.hovered.bg_fill = BG_SEL;
    visuals.widgets.hovered.weak_bg_fill = BG_SEL;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, OUTER_BORDER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, INK);
    visuals.widgets.hovered.rounding = Rounding::ZERO;

    visuals.widgets.active.bg_fill = BG_SEL;
    visuals.widgets.active.weak_bg_fill = BG_SEL;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, INK);
    visuals.widgets.active.rounding = Rounding::ZERO;

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(11.0, 5.0);
    style.spacing.window_margin = Margin::same(16.0);
    style.spacing.interact_size.y = 26.0;
    ctx.set_style(style);
}

pub fn well_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(PAPER_2)
        .stroke(Stroke::new(1.0_f32, RULE_2))
        .rounding(Rounding::ZERO)
        .inner_margin(Margin::same(14.0))
}

#[allow(dead_code)]
pub fn sunk_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(PAPER_3)
        .stroke(Stroke::new(1.0_f32, RULE_2))
        .rounding(Rounding::ZERO)
        .inner_margin(Margin::same(12.0))
}

pub fn draw_corner_ticks(painter: &egui::Painter, rect: Rect) {
    let l = 10.0_f32;
    let st = Stroke::new(1.0_f32, MUTED);
    painter.line_segment(
        [rect.left_top(), Pos2::new(rect.left() + l, rect.top())],
        st,
    );
    painter.line_segment(
        [rect.left_top(), Pos2::new(rect.left(), rect.top() + l)],
        st,
    );
    painter.line_segment(
        [
            rect.right_bottom(),
            Pos2::new(rect.right() - l, rect.bottom()),
        ],
        st,
    );
    painter.line_segment(
        [
            rect.right_bottom(),
            Pos2::new(rect.right(), rect.bottom() - l),
        ],
        st,
    );
}

pub fn parse_hex_color32(hex: &str) -> Color32 {
    let h = hex.trim().trim_start_matches('#');
    if h.len() < 6 {
        return Color32::from_rgb(24, 24, 30);
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(24);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(24);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(30);
    Color32::from_rgb(r, g, b)
}

pub fn color32_to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}
