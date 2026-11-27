use crate::backend::{Palette, SortOrder, StudioPrefs};
use crate::i18n::pick;
use crate::theme::*;
use crate::ui_widgets::*;
use egui::{Margin, Pos2, Rect, RichText, Rounding, Stroke, Vec2};

pub struct PalettesViewState {
    pub search: String,
    pub source_filter: String,
    pub show_creator: bool,
    pub draft: Palette,
}

impl Default for PalettesViewState {
    fn default() -> Self {
        Self {
            search: String::new(),
            source_filter: "all".to_string(),
            show_creator: false,
            draft: Palette {
                name: "Mango Warm".to_string(),
                bg: "#080706".to_string(),
                bg_alt: "#141210".to_string(),
                fg: "#d8ccb8".to_string(),
                selected: "#c8a97e".to_string(),
                active: "#b5bd68".to_string(),
                urgent: "#cc6666".to_string(),
                border: "#a89984".to_string(),
                mtime: 0.0,
                is_custom: true,
            },
        }
    }
}

#[derive(Default)]
pub struct PalettesAction {
    pub apply_palette: Option<Palette>,
    pub save_custom_palette: Option<Palette>,
    pub prefs_changed: bool,
}

pub fn render_palettes_view(
    ui: &mut egui::Ui,
    state: &mut PalettesViewState,
    prefs: &mut StudioPrefs,
    palettes: &[Palette],
    active_palette: &str,
) -> PalettesAction {
    let mut act = PalettesAction::default();
    let lang = prefs.lang;

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() * 0.60).max(180.0));
            ui.label(
                RichText::new(pick(lang, "Renk Paletleri", "Color Palettes"))
                    .size(18.0)
                    .color(INK),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(pick(
                        lang,
                        "wallust/colors-rofi.rasi ve tüm launcher paylaşımlı paletlerini tek tıkla uygula",
                        "Apply color schemes to wallust/colors-rofi.rasi and shared launcher themes",
                    ))
                    .size(11.5)
                    .color(MUTED),
                )
                .wrap(),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let lbl = if state.show_creator {
                pick(lang, "× Oluşturucuyu Gizle", "× Hide Creator")
            } else {
                pick(lang, "+ Özel Palet Oluştur", "+ New Custom Palette")
            };
            if primary_button(ui, lbl).clicked() {
                state.show_creator = !state.show_creator;
            }
        });
    });

    ui.add_space(10.0);

    // Optional custom palette creator panel
    if state.show_creator {
        well_frame().show(ui, |ui| {
            section_head(
                ui,
                pick(lang, "Özel Palet Oluşturucu", "Custom Palette Builder"),
                pick(
                    lang,
                    "colors/ klasörüne .rasi paleti olarak kaydedilir ve anında uygulanır",
                    "Saved as a .rasi palette in colors/ and applied immediately",
                ),
            );
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(pick(lang, "Palet Adı:", "Palette Name:"))
                        .size(12.0)
                        .color(INK_2),
                );
                ui.add_sized(
                    [220.0, 24.0],
                    egui::TextEdit::singleline(&mut state.draft.name),
                );
            });
            ui.add_space(8.0);

            color_picker_grid(
                ui,
                &mut [
                    ("cp_bg", "Background", &mut state.draft.bg),
                    ("cp_alt", "Surface / Alt", &mut state.draft.bg_alt),
                    ("cp_fg", "Foreground", &mut state.draft.fg),
                    ("cp_sel", "Selected", &mut state.draft.selected),
                    ("cp_act", "Active", &mut state.draft.active),
                    ("cp_urg", "Urgent", &mut state.draft.urgent),
                    ("cp_brd", "Border", &mut state.draft.border),
                ],
                true,
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary_button(
                        ui,
                        pick(lang, "Paleti Kaydet ve Uygula", "Save & Apply Palette"),
                    )
                    .clicked()
                    {
                        act.save_custom_palette = Some(state.draft.clone());
                    }
                });
            });
        });
        ui.add_space(10.0);
    }

    // Search + sort + source filter
    well_frame().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(pick(lang, "Ara:", "Search:"))
                    .size(12.0)
                    .color(MUTED),
            );
            let w = ui.available_width().clamp(150.0, 260.0);
            ui.add_sized(
                [w, 24.0],
                egui::TextEdit::singleline(&mut state.search).hint_text(pick(
                    lang,
                    "palet adı...",
                    "palette name...",
                )),
            );
            if !state.search.is_empty() && secondary_button(ui, "×").clicked() {
                state.search.clear();
            }

            ui.add_space(6.0);
            for (src_id, src_lbl) in [
                ("all", pick(lang, "Tümü", "All")),
                ("builtin", pick(lang, "Yerleşik", "Built-in")),
                ("custom", pick(lang, "Özel / Wallust", "Custom / Disk")),
            ] {
                if chip_button(ui, src_lbl, state.source_filter == src_id).clicked() {
                    state.source_filter = src_id.to_string();
                }
            }

            ui.add_space(6.0);
            for (ord, lbl) in [
                (SortOrder::Az, "A→Z"),
                (SortOrder::Za, "Z→A"),
                (SortOrder::Newest, pick(lang, "Yeni", "Newest")),
            ] {
                if chip_button(ui, lbl, prefs.sort == ord).clicked() && prefs.sort != ord {
                    prefs.sort = ord;
                    act.prefs_changed = true;
                }
            }
        });
    });

    ui.add_space(10.0);

    let q = state.search.trim().to_ascii_lowercase();
    let mut filtered: Vec<&Palette> = palettes
        .iter()
        .filter(|p| {
            if state.source_filter == "builtin" && p.is_custom {
                return false;
            }
            if state.source_filter == "custom" && !p.is_custom {
                return false;
            }
            if !q.is_empty() && !p.name.to_ascii_lowercase().contains(&q) {
                return false;
            }
            true
        })
        .collect();

    match prefs.sort {
        SortOrder::Az => filtered.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        }),
        SortOrder::Za => filtered.sort_by(|a, b| {
            b.name
                .to_ascii_lowercase()
                .cmp(&a.name.to_ascii_lowercase())
        }),
        SortOrder::Newest => filtered.sort_by(|a, b| {
            b.mtime.total_cmp(&a.mtime).then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
        }),
        SortOrder::Oldest => filtered.sort_by(|a, b| {
            a.mtime.total_cmp(&b.mtime).then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
        }),
    }

    if filtered.is_empty() {
        well_frame().show(ui, |ui| {
            ui.label(
                RichText::new(pick(
                    lang,
                    "Eşleşen palet bulunamadı.",
                    "No palettes match the filter.",
                ))
                .size(13.0)
                .color(MUTED),
            );
        });
        return act;
    }

    let avail_w = ui.available_width();
    let gap = 12.0;
    let cols = (((avail_w + gap) / (260.0 + gap)).floor() as usize).max(1);
    let card_w = ((avail_w - gap * (cols - 1) as f32) / cols as f32).max(180.0);

    for row in filtered.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for pal in row {
                let is_active = pal.name.eq_ignore_ascii_case(active_palette);
                let frame_stroke = if is_active {
                    Stroke::new(1.0_f32, OK)
                } else {
                    Stroke::new(1.0_f32, RULE)
                };
                let resp =
                    egui::Frame::none()
                        .fill(PAPER_2)
                        .stroke(frame_stroke)
                        .rounding(Rounding::ZERO)
                        .inner_margin(Margin::same(11.0))
                        .show(ui, |ui| {
                            ui.set_width(card_w - 24.0);
                            ui.vertical(|ui| {
                                let inner_w = ui.available_width();
                                // 6 sharp color swatches
                                let (sw_rect, _) = ui.allocate_exact_size(
                                    Vec2::new(inner_w, 30.0),
                                    egui::Sense::hover(),
                                );
                                let swatches = [
                                    parse_hex_color32(&pal.bg),
                                    parse_hex_color32(&pal.bg_alt),
                                    parse_hex_color32(&pal.selected),
                                    parse_hex_color32(&pal.active),
                                    parse_hex_color32(&pal.urgent),
                                    parse_hex_color32(&pal.fg),
                                ];
                                {
                                    let p = ui.painter();
                                    let sw_w = sw_rect.width() / swatches.len() as f32;
                                    for (idx, col) in swatches.iter().enumerate() {
                                        let r = Rect::from_min_size(
                                            Pos2::new(
                                                sw_rect.left() + idx as f32 * sw_w,
                                                sw_rect.top(),
                                            ),
                                            Vec2::new(sw_w, sw_rect.height()),
                                        );
                                        p.rect_filled(r, Rounding::ZERO, *col);
                                    }
                                    p.rect_stroke(
                                        sw_rect,
                                        Rounding::ZERO,
                                        Stroke::new(1.0_f32, parse_hex_color32(&pal.border)),
                                    );
                                }

                                ui.add_space(6.0);

                                // Mini Rofi sample preview bar
                                let (bar_rect, _) = ui.allocate_exact_size(
                                    Vec2::new(inner_w, 22.0),
                                    egui::Sense::hover(),
                                );
                                let bg_c = parse_hex_color32(&pal.bg);
                                let sel_c = parse_hex_color32(&pal.selected);
                                let fg_c = parse_hex_color32(&pal.fg);
                                {
                                    let p = ui.painter();
                                    p.rect_filled(bar_rect, Rounding::ZERO, bg_c);
                                    let sel_pill = Rect::from_min_size(
                                        Pos2::new(bar_rect.left() + 4.0, bar_rect.top() + 3.0),
                                        Vec2::new((bar_rect.width() * 0.42).max(52.0), 16.0),
                                    );
                                    p.rect_filled(sel_pill, Rounding::ZERO, sel_c);
                                    p.text(
                                        sel_pill.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "drun",
                                        egui::FontId::monospace(10.0),
                                        bg_c,
                                    );
                                    p.text(
                                        Pos2::new(sel_pill.right() + 8.0, bar_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        &pal.selected,
                                        egui::FontId::monospace(10.0),
                                        fg_c,
                                    );
                                    p.rect_stroke(
                                        bar_rect,
                                        Rounding::ZERO,
                                        Stroke::new(1.0_f32, RULE_2),
                                    );
                                }

                                ui.add_space(7.0);

                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&pal.name)
                                                .size(12.5)
                                                .color(if is_active { INK } else { INK_2 }),
                                        )
                                        .truncate(),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if is_active {
                                                let b = egui::Button::new(
                                                    RichText::new(pick(lang, "AKTİF", "ACTIVE"))
                                                        .size(10.0)
                                                        .color(ACCENT_INK),
                                                )
                                                .fill(OK)
                                                .stroke(Stroke::new(1.0_f32, OK))
                                                .rounding(Rounding::ZERO);
                                                let _ = ui.add(b);
                                            } else if accent_outline_button(
                                                ui,
                                                pick(lang, "Uygula", "Apply"),
                                            )
                                            .clicked()
                                            {
                                                act.apply_palette = Some((*pal).clone());
                                            }
                                        },
                                    );
                                });
                            });
                        });
                if is_active {
                    draw_corner_ticks(ui.painter(), resp.response.rect);
                }
            }
        });
        ui.add_space(gap);
    }

    act
}
