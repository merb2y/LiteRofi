use crate::backend::{extract_rasi_hex_colors, validate_rasi_source, StudioPaths, ThemeEntry};
use crate::i18n::{pick, Lang};
use crate::preview_loader::PreviewLoader;
use crate::theme::*;
use crate::ui_widgets::*;
use egui::{Margin, Pos2, Rect, RichText, Rounding, Stroke, Vec2};

pub struct EditorViewState {
    pub selected_theme: String,
    pub draft_name: String,
    pub source_buf: String,
    pub dirty: bool,
    pub extracted_colors: Vec<(String, String)>,
    pub validation_result: Result<usize, String>,
}

impl Default for EditorViewState {
    fn default() -> Self {
        Self {
            selected_theme: String::new(),
            draft_name: "custom-theme".to_string(),
            source_buf: "* {\n    background: #080706;\n    foreground: #d8ccb8;\n    accent: #c8a97e;\n}\n\nwindow {\n    width: 640px;\n    border: 1px;\n    border-color: #a89984;\n    background-color: #080706;\n}\n".to_string(),
            dirty: false,
            extracted_colors: Vec::new(),
            validation_result: Ok(1),
        }
    }
}

impl EditorViewState {
    pub fn load_theme_from_disk(&mut self, paths: &StudioPaths, name: &str) {
        let rasi_path = paths.themes_dir.join(format!("{name}.rasi"));
        if let Ok(text) = std::fs::read_to_string(&rasi_path) {
            self.selected_theme = name.to_string();
            self.draft_name = name.to_string();
            self.source_buf = text.strip_prefix('\u{FEFF}').unwrap_or(&text).to_string();
            self.dirty = false;
            self.revalidate(paths);
        }
    }

    pub fn revalidate(&mut self, paths: &StudioPaths) {
        self.extracted_colors = extract_rasi_hex_colors(&self.source_buf);
        self.validation_result = validate_rasi_source(paths, &self.source_buf);
    }
}

#[derive(Default)]
pub struct EditorAction {
    pub save_and_preview: Option<(String, String)>,
    pub apply_theme: Option<String>,
    pub live_preview: Option<String>,
    pub reveal_theme: Option<String>,
    pub rerender_theme: Option<String>,
}

pub fn render_editor_view(
    ui: &mut egui::Ui,
    state: &mut EditorViewState,
    paths: &StudioPaths,
    themes: &[ThemeEntry],
    loader: &mut PreviewLoader,
    lang: Lang,
) -> EditorAction {
    let mut act = EditorAction::default();

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() * 0.52).max(180.0));
            ui.label(
                RichText::new(pick(lang, "Rasi Tema Editörü", ".rasi Theme Editor"))
                    .size(18.0)
                    .color(INK),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(pick(
                        lang,
                        "Canlı sözdizimi doğrulaması, renk paleti çıkarımı ve anlık önizleme",
                        "Live syntax validation, color swatch inspection, and instant preview rendering",
                    ))
                    .size(11.5)
                    .color(MUTED),
                )
                .wrap(),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.horizontal_wrapped(|ui| {
                if secondary_button(ui, pick(lang, "Yeni Şablon", "New Template")).clicked() {
                    let mut fresh = EditorViewState::default();
                    fresh.revalidate(paths);
                    *state = fresh;
                    state.dirty = true;
                }
                if !state.selected_theme.is_empty() {
                    if secondary_button(ui, pick(lang, "Canlı Aç", "Live")).clicked() {
                        act.live_preview = Some(state.selected_theme.clone());
                    }
                    if accent_outline_button(ui, pick(lang, "Aktif Yap", "Apply")).clicked() {
                        act.apply_theme = Some(state.selected_theme.clone());
                    }
                }
                let save_lbl = if state.dirty {
                    pick(lang, "Kaydet & Önizle *", "Save & Preview *")
                } else {
                    pick(lang, "Kaydet & Önizle", "Save & Preview")
                };
                if primary_button(ui, save_lbl).clicked() {
                    act.save_and_preview =
                        Some((state.draft_name.trim().to_string(), state.source_buf.clone()));
                }
            });
        });
    });

    ui.add_space(10.0);

    // Theme selector + name + live syntax status bar
    well_frame().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(pick(lang, "Yükle:", "Load:"))
                    .size(12.0)
                    .color(MUTED),
            );
            let current_lbl = if state.selected_theme.is_empty() {
                pick(lang, "(Yeni Tema)", "(New Theme)").to_string()
            } else {
                state.selected_theme.clone()
            };
            egui::ComboBox::from_id_salt("editor_theme_select")
                .width(190.0)
                .selected_text(RichText::new(current_lbl).size(12.0).color(INK))
                .show_ui(ui, |ui| {
                    for t in themes {
                        if ui
                            .selectable_label(state.selected_theme == t.name, &t.name)
                            .clicked()
                        {
                            state.load_theme_from_disk(paths, &t.name);
                        }
                    }
                });

            ui.add_space(8.0);
            ui.label(
                RichText::new(pick(lang, "Dosya Adı:", "File Stem:"))
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_sized(
                [160.0, 22.0],
                egui::TextEdit::singleline(&mut state.draft_name),
            );

            ui.add_space(8.0);
            match &state.validation_result {
                Ok(rules) => {
                    ui.label(
                        RichText::new(format!(
                            "OK · {rules} {}",
                            pick(lang, "widget kuralı", "widget rules")
                        ))
                        .size(11.5)
                        .monospace()
                        .color(OK),
                    );
                }
                Err(err) => {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("ERR · {err}"))
                                .size(11.5)
                                .monospace()
                                .color(WARN),
                        )
                        .truncate(),
                    );
                }
            }
        });
    });

    ui.add_space(10.0);

    equal_two_columns(ui, "editor_split", |col, ui| match col {
        0 => {
            section_head(
                ui,
                pick(lang, ".rasi Kaynak Kodu", ".rasi Source"),
                pick(
                    lang,
                    "Düzenlemeler kaydedildiğinde önizleme otomatik yenilenir",
                    "Preview refreshes automatically when saved",
                ),
            );
            let edit_w = (ui.available_width() - 8.0).max(200.0);
            egui::ScrollArea::vertical()
                .id_salt("rasi_editor_scroll")
                .max_height(430.0)
                .show(ui, |ui| {
                    let resp = ui.add_sized(
                        [edit_w, 410.0],
                        egui::TextEdit::multiline(&mut state.source_buf)
                            .desired_width(edit_w)
                            .code_editor()
                            .lock_focus(true),
                    );
                    if resp.changed() {
                        state.dirty = true;
                        state.revalidate(paths);
                    }
                });
        }
        _ => {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(pick(lang, "Önizleme & Renkler", "Preview & Swatches"))
                            .size(14.5)
                            .color(INK),
                    );
                    ui.label(
                        RichText::new(pick(
                            lang,
                            "Direct2D render çıktısı",
                            "Direct2D rendered output",
                        ))
                        .size(11.5)
                        .color(MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !state.selected_theme.is_empty() {
                        if secondary_button(ui, pick(lang, "Yeniden Render Et", "Re-render"))
                            .clicked()
                        {
                            act.rerender_theme = Some(state.selected_theme.clone());
                        }
                        if secondary_button(ui, pick(lang, "Klasörde Aç", "Reveal")).clicked() {
                            act.reveal_theme = Some(state.selected_theme.clone());
                        }
                    }
                });
            });
            ui.add_space(4.0);
            hairline_rule(ui, RULE);
            ui.add_space(8.0);

            let prev_w = ui.available_width();
            let (prev_rect, _) =
                ui.allocate_exact_size(Vec2::new(prev_w, 250.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(prev_rect, Rounding::ZERO, PAPER);
            p.rect_stroke(prev_rect, Rounding::ZERO, Stroke::new(1.0_f32, RULE));

            if !state.selected_theme.is_empty() {
                loader.request_preview(&state.selected_theme, ui.ctx());
                if let Some(tex) = loader.get_texture(&state.selected_theme) {
                    let tsz = tex.size_vec2();
                    let scale = ((prev_rect.width() - 12.0) / tsz.x.max(1.0))
                        .min((prev_rect.height() - 12.0) / tsz.y.max(1.0))
                        .min(1.4);
                    let dsz = tsz * scale;
                    p.image(
                        tex.id(),
                        Rect::from_center_size(prev_rect.center(), dsz),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    p.text(
                        prev_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        pick(lang, "Önizleme bekleniyor...", "Waiting for preview..."),
                        egui::FontId::monospace(11.5),
                        MUTED,
                    );
                }
            } else {
                p.text(
                    prev_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    pick(
                        lang,
                        "Önizleme için temayı kaydedin",
                        "Save theme to render preview",
                    ),
                    egui::FontId::monospace(11.5),
                    MUTED,
                );
            }
            draw_corner_ticks(p, prev_rect);

            ui.add_space(10.0);
            ui.label(
                RichText::new(pick(
                    lang,
                    "Çıkarılan Renk Token'ları",
                    "Extracted Color Tokens",
                ))
                .size(12.5)
                .color(INK_2),
            );
            ui.add_space(6.0);
            if state.extracted_colors.is_empty() {
                ui.label(
                    RichText::new(pick(
                        lang,
                        "Kaynakta #hex renk bulunamadı.",
                        "No #hex colors detected in source.",
                    ))
                    .size(11.5)
                    .color(MUTED),
                );
            } else {
                let sw_avail = ui.available_width().max(180.0);
                let gap = 6.0_f32;
                let cols = (((sw_avail + gap) / (150.0 + gap)).floor() as usize).clamp(1, 3);
                let cell_w = ((sw_avail - gap * (cols - 1) as f32) / cols as f32).max(110.0);
                for chunk in state.extracted_colors.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for (k, hx) in chunk {
                            let col = parse_hex_color32(hx);
                            ui.allocate_ui(Vec2::new(cell_w, 24.0), |ui| {
                                ui.set_max_width(cell_w);
                                egui::Frame::none()
                                    .fill(PAPER_3)
                                    .stroke(Stroke::new(1.0_f32, RULE))
                                    .rounding(Rounding::ZERO)
                                    .inner_margin(Margin::symmetric(7.0, 4.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let (sq, _) = ui.allocate_exact_size(
                                                Vec2::new(12.0, 12.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(sq, Rounding::ZERO, col);
                                            ui.painter().rect_stroke(
                                                sq,
                                                Rounding::ZERO,
                                                Stroke::new(1.0_f32, OUTER_BORDER),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(format!("{k}: {hx}"))
                                                        .size(10.5)
                                                        .monospace()
                                                        .color(INK_2),
                                                )
                                                .truncate(),
                                            );
                                        });
                                    });
                            });
                        }
                    });
                    ui.add_space(gap);
                }
            }
        }
    });

    act
}
