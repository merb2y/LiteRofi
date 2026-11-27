use crate::theme::*;
use egui::{Color32, Margin, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2};

pub fn hairline_rule(ui: &mut egui::Ui, color: Color32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0_f32, color),
    );
}

pub fn section_head(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(title).size(14.5).color(INK));
        if !subtitle.is_empty() {
            ui.label(RichText::new(subtitle).size(11.5).color(MUTED));
        }
    });
    ui.add_space(4.0);
    hairline_rule(ui, RULE);
    ui.add_space(6.0);
}

pub struct TelemetryCell<'a> {
    pub label: &'a str,
    pub value: String,
    pub unit: &'a str,
    pub sub: String,
    pub ratio: f32,
    pub bar_color: Color32,
}

pub fn telemetry_ribbon(ui: &mut egui::Ui, cells: &[TelemetryCell<'_>]) {
    if cells.is_empty() {
        return;
    }
    let resp = egui::Frame::none()
        .fill(PAPER_2)
        .stroke(Stroke::new(1.0_f32, RULE))
        .rounding(Rounding::ZERO)
        .inner_margin(Margin::symmetric(0.0, 10.0))
        .show(ui, |ui| {
            let n = cells.len();
            ui.columns(n, |cols| {
                for (i, cell) in cells.iter().enumerate() {
                    cols[i].vertical(|ui| {
                        ui.add_space(2.0);
                        egui::Frame::none()
                            .inner_margin(Margin::symmetric(12.0, 0.0))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(RichText::new(cell.label).size(11.5).color(MUTED));
                                ui.add_space(3.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&cell.value)
                                                .size(18.0)
                                                .monospace()
                                                .color(INK),
                                        )
                                        .truncate(),
                                    );
                                    if !cell.unit.is_empty() {
                                        ui.label(
                                            RichText::new(cell.unit)
                                                .size(11.5)
                                                .monospace()
                                                .color(MUTED),
                                        );
                                    }
                                });
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&cell.sub).size(11.0).color(MUTED_2),
                                    )
                                    .truncate(),
                                );
                                ui.add_space(6.0);
                                let (bar_rect, _) = ui.allocate_exact_size(
                                    Vec2::new(ui.available_width(), 2.0),
                                    Sense::hover(),
                                );
                                ui.painter().rect_filled(bar_rect, Rounding::ZERO, PAPER_3);
                                let fw = bar_rect.width() * cell.ratio.clamp(0.0, 1.0);
                                if fw > 0.5 {
                                    let fill = Rect::from_min_size(
                                        bar_rect.min,
                                        Vec2::new(fw, bar_rect.height()),
                                    );
                                    ui.painter()
                                        .rect_filled(fill, Rounding::ZERO, cell.bar_color);
                                }
                            });
                    });
                }
            });
        });

    let outer = resp.response.rect;
    let p = ui.painter();
    let n = cells.len();
    for i in 1..n {
        let x = outer.left() + (outer.width() * i as f32) / n as f32;
        p.line_segment(
            [
                Pos2::new(x, outer.top() + 6.0),
                Pos2::new(x, outer.bottom() - 6.0),
            ],
            Stroke::new(1.0_f32, RULE),
        );
    }
    draw_corner_ticks(p, outer);
}

pub fn square_toggle(ui: &mut egui::Ui, checked: &mut bool) -> bool {
    let size = Vec2::new(38.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let changed = resp.clicked();
    if changed {
        *checked = !*checked;
    }
    let p = ui.painter();
    let fill = if *checked { ACCENT } else { PAPER_3 };
    let border = if *checked {
        ACCENT
    } else if resp.hovered() {
        MUTED_2
    } else {
        RULE
    };
    p.rect_filled(rect, Rounding::ZERO, fill);
    p.rect_stroke(rect, Rounding::ZERO, Stroke::new(1.0_f32, border));

    let knob_w = 12.0;
    let knob_h = 12.0;
    let kx = if *checked {
        rect.right() - knob_w - 4.0
    } else {
        rect.left() + 4.0
    };
    let knob = Rect::from_min_size(
        Pos2::new(kx, rect.center().y - knob_h * 0.5),
        Vec2::new(knob_w, knob_h),
    );
    p.rect_filled(
        knob,
        Rounding::ZERO,
        if *checked { ACCENT_INK } else { MUTED },
    );
    changed
}

pub fn toggle_row(ui: &mut egui::Ui, label: &str, desc: &str, checked: &mut bool) -> bool {
    let mut changed = false;
    let avail = ui.available_width();
    let text_w = (avail - 52.0).max(120.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(text_w);
            ui.add(egui::Label::new(RichText::new(label).size(13.0).color(INK)).wrap());
            if !desc.is_empty() {
                ui.add(egui::Label::new(RichText::new(desc).size(11.0).color(MUTED)).wrap());
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if square_toggle(ui, checked) {
                changed = true;
            }
        });
    });
    ui.add_space(4.0);
    hairline_rule(ui, RULE_2);
    ui.add_space(4.0);
    changed
}

pub fn segmented_str_row(
    ui: &mut egui::Ui,
    label: &str,
    desc: &str,
    value: &mut String,
    options: &[(&str, &str)],
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.add(egui::Label::new(RichText::new(label).size(13.0).color(INK)).wrap());
        if !desc.is_empty() {
            ui.add(egui::Label::new(RichText::new(desc).size(11.0).color(MUTED)).wrap());
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            for &(opt_val, opt_lbl) in options {
                let active = value == opt_val;
                let btn = egui::Button::new(RichText::new(opt_lbl).size(11.5).color(if active {
                    ACCENT_INK
                } else {
                    INK_2
                }))
                .fill(if active { ACCENT } else { PAPER_3 })
                .stroke(Stroke::new(1.0_f32, if active { ACCENT } else { RULE }))
                .rounding(Rounding::ZERO);
                if ui.add(btn).clicked() && !active {
                    *value = opt_val.to_string();
                    changed = true;
                }
            }
        });
    });
    ui.add_space(4.0);
    hairline_rule(ui, RULE_2);
    ui.add_space(4.0);
    changed
}

pub fn slider_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut i32,
    min: i32,
    max: i32,
    unit: &str,
    default_val: Option<i32>,
) -> bool {
    let mut changed = false;
    let lo = min.min(max);
    let hi = min.max(max);
    *value = (*value).clamp(lo, hi);

    let val_str = format!("{}", *value);

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.5).color(INK_2));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(def) = default_val {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("↺").size(10.5).color(MUTED))
                                .fill(PAPER_3)
                                .stroke(Stroke::new(1.0_f32, RULE))
                                .rounding(Rounding::ZERO),
                        )
                        .clicked()
                        && *value != def
                    {
                        *value = def.clamp(lo, hi);
                        changed = true;
                    }
                }
                if !unit.is_empty() {
                    ui.label(RichText::new(unit).size(11.0).monospace().color(MUTED));
                }
                ui.label(RichText::new(&val_str).size(13.0).monospace().color(INK));
            });
        });
        let avail_w = (ui.available_width() - 8.0).max(80.0);
        ui.spacing_mut().slider_width = avail_w;
        if ui
            .add(
                egui::Slider::new(value, lo..=hi)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .changed()
        {
            changed = true;
        }
    });
    ui.add_space(3.0);
    hairline_rule(ui, RULE_2);
    ui.add_space(3.0);
    changed
}

pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(12.0).color(ACCENT_INK))
            .fill(ACCENT)
            .stroke(Stroke::new(1.0_f32, ACCENT))
            .rounding(Rounding::ZERO),
    )
}

pub fn secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(12.0).color(INK))
            .fill(PAPER_3)
            .stroke(Stroke::new(1.0_f32, RULE))
            .rounding(Rounding::ZERO),
    )
}

pub fn accent_outline_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(12.0).color(ACCENT))
            .fill(PAPER_3)
            .stroke(Stroke::new(1.0_f32, ACCENT))
            .rounding(Rounding::ZERO),
    )
}

pub fn chip_button(ui: &mut egui::Ui, text: &str, active: bool) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(11.5).color(if active {
            ACCENT_INK
        } else {
            INK_2
        }))
        .fill(if active { ACCENT } else { PAPER_3 })
        .stroke(Stroke::new(1.0_f32, if active { ACCENT } else { RULE }))
        .rounding(Rounding::ZERO),
    )
}

pub fn color_picker_cell(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    hex_val: &mut String,
    enabled: bool,
) -> bool {
    let mut changed = false;
    let mut c = parse_hex_color32(hex_val);
    ui.add_enabled_ui(enabled, |ui| {
        egui::Frame::none()
            .fill(PAPER_3)
            .stroke(Stroke::new(1.0_f32, RULE))
            .rounding(Rounding::ZERO)
            .inner_margin(Margin::symmetric(8.0, 6.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(label).size(10.5).color(MUTED));
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.push_id(id, |ui| {
                            if ui.color_edit_button_srgba(&mut c).changed() {
                                *hex_val = color32_to_hex(c);
                                changed = true;
                            }
                        });
                        ui.label(
                            RichText::new(hex_val.as_str())
                                .size(11.0)
                                .monospace()
                                .color(INK_2),
                        );
                    });
                });
            });
    });
    changed
}

pub fn color_picker_grid(
    ui: &mut egui::Ui,
    cells: &mut [(&str, &str, &mut String)],
    enabled: bool,
) -> bool {
    let mut changed = false;
    let avail = ui.available_width().max(180.0);
    let gap = 6.0_f32;
    let cols = (((avail + gap) / (118.0 + gap)).floor() as usize).clamp(1, 5);
    let cell_w = ((avail - gap * (cols - 1) as f32) / cols as f32).max(96.0);
    for row in cells.chunks_mut(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (id, label, hex_val) in row.iter_mut() {
                ui.allocate_ui(Vec2::new(cell_w, 46.0), |ui| {
                    ui.set_min_width(cell_w);
                    if color_picker_cell(ui, id, label, hex_val, enabled) {
                        changed = true;
                    }
                });
            }
        });
        ui.add_space(gap);
    }
    changed
}

pub fn combo_row(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    selected_text: &str,
    value: &mut String,
    options: &[(&str, &str)],
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(label).size(12.5).color(INK_2)).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let w = (ui.available_width() - 8.0).clamp(96.0, 220.0);
                egui::ComboBox::from_id_salt(id)
                    .width(w)
                    .selected_text(RichText::new(selected_text).size(12.0).color(INK))
                    .show_ui(ui, |ui| {
                        for &(opt_val, opt_lbl) in options {
                            if ui.selectable_label(*value == opt_val, opt_lbl).clicked()
                                && *value != opt_val
                            {
                                *value = opt_val.to_string();
                                changed = true;
                            }
                        }
                    });
            });
        });
    });
    ui.add_space(3.0);
    hairline_rule(ui, RULE_2);
    ui.add_space(3.0);
    changed
}

pub fn equal_two_columns(
    ui: &mut egui::Ui,
    id: &str,
    mut render_col: impl FnMut(usize, &mut egui::Ui),
) {
    let state_id = ui.make_persistent_id(id);
    let prev_max_h: f32 = ui.ctx().data(|d| d.get_temp(state_id)).unwrap_or(0.0);
    let mut left_h = 0.0_f32;
    let mut right_h = 0.0_f32;

    let resp = egui::Frame::none()
        .fill(PAPER_2)
        .stroke(Stroke::new(1.0_f32, RULE_2))
        .rounding(Rounding::ZERO)
        .inner_margin(Margin::symmetric(0.0, 14.0))
        .show(ui, |ui| {
            ui.columns(2, |cols| {
                cols[0].vertical(|ui| {
                    let y0 = ui.cursor().top();
                    egui::Frame::none()
                        .inner_margin(Margin::symmetric(14.0, 0.0))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            render_col(0, ui);
                        });
                    left_h = (ui.min_rect().bottom() - y0).max(0.0);
                    if prev_max_h > left_h + 1.0 {
                        ui.add_space(prev_max_h - left_h);
                    }
                });

                cols[1].vertical(|ui| {
                    let y0 = ui.cursor().top();
                    egui::Frame::none()
                        .inner_margin(Margin::symmetric(14.0, 0.0))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            render_col(1, ui);
                        });
                    right_h = (ui.min_rect().bottom() - y0).max(0.0);
                    if prev_max_h > right_h + 1.0 {
                        ui.add_space(prev_max_h - right_h);
                    }
                });
            });
        });

    let next_max_h = left_h.max(right_h);
    ui.ctx().data_mut(|d| d.insert_temp(state_id, next_max_h));

    let outer = resp.response.rect;
    let cx = outer.center().x;
    ui.painter().line_segment(
        [
            Pos2::new(cx, outer.top() + 8.0),
            Pos2::new(cx, outer.bottom() - 8.0),
        ],
        Stroke::new(1.0_f32, RULE_2),
    );
}
