use crate::backend::{SortOrder, StudioPrefs, ThemeEntry};
use crate::i18n::{category_label, pick};
use crate::preview_loader::PreviewLoader;
use crate::theme::*;
use crate::ui_widgets::*;
use egui::{Margin, Pos2, Rect, RichText, Rounding, Stroke, Vec2};
use std::collections::HashMap;

pub struct ThemesViewState {
    pub search: String,
    pub category: String,
    pub show_import_modal: bool,
    pub paste_name: String,
    pub paste_code: String,
    pub focus_search_requested: bool,
}

impl Default for ThemesViewState {
    fn default() -> Self {
        Self {
            search: String::new(),
            category: "All".to_string(),
            show_import_modal: false,
            paste_name: String::new(),
            paste_code: String::new(),
            focus_search_requested: false,
        }
    }
}

#[derive(Default)]
pub struct ThemesAction {
    pub apply_theme: Option<String>,
    pub live_preview: Option<String>,
    pub toggle_favorite: Option<String>,
    pub edit_theme: Option<String>,
    pub reveal_theme: Option<String>,
    pub open_themes_dir: bool,
    pub clear_cache: bool,
    pub prerender_missing: bool,
    pub pick_rasi_files: bool,
    pub pick_theme_folder: bool,
    pub save_paste_theme: Option<(String, String)>,
    pub prefs_changed: bool,
}

pub struct ThemesContext<'a> {
    pub themes: &'a [ThemeEntry],
    pub active_theme: &'a str,
    pub active_palette: &'a str,
    pub loading_themes: bool,
}

pub fn render_themes_view(
    ui: &mut egui::Ui,
    state: &mut ThemesViewState,
    prefs: &mut StudioPrefs,
    ctx_data: ThemesContext<'_>,
    loader: &mut PreviewLoader,
) -> ThemesAction {
    let themes = ctx_data.themes;
    let active_theme = ctx_data.active_theme;
    let active_palette = ctx_data.active_palette;
    let loading_themes = ctx_data.loading_themes;
    let mut act = ThemesAction::default();
    let lang = prefs.lang;

    // Header
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() * 0.48).max(170.0));
            ui.label(
                RichText::new(pick(lang, "Tema Galerisi", "Theme Gallery"))
                    .size(18.0)
                    .color(INK),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(pick(
                        lang,
                        "Direct2D önizlemeli yerel ve Rofi uyumlu .rasi tema yöneticisi",
                        "Native Rofi-compatible .rasi theme browser with Direct2D previews",
                    ))
                    .size(11.5)
                    .color(MUTED),
                )
                .wrap(),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.horizontal_wrapped(|ui| {
                let (pr_run, pr_done, pr_total, _) = loader.prerender.snapshot();
                let pr_label = if pr_run {
                    format!(
                        "{} {pr_done}/{pr_total}",
                        pick(lang, "Üretiliyor", "Rendering")
                    )
                } else {
                    pick(lang, "Önizleme Üret", "Generate Previews").to_string()
                };
                if accent_outline_button(ui, &pr_label).clicked() && !pr_run {
                    act.prerender_missing = true;
                }
                if secondary_button(ui, pick(lang, ".rasi Klasörü", ".rasi Folder")).clicked() {
                    act.open_themes_dir = true;
                }
                if secondary_button(ui, pick(lang, "Önbelleği Temizle", "Clear Cache")).clicked()
                {
                    act.clear_cache = true;
                }
                if primary_button(ui, pick(lang, "+ Tema Ekle", "+ Add Theme")).clicked() {
                    state.show_import_modal = true;
                }
            });
        });
    });

    ui.add_space(10.0);

    // Compute counts + filtered sorted list
    let cat_order = [
        "All",
        "Favorites",
        "Lab",
        "KooL",
        "adi1090x",
        "gh0stzk",
        "ML4W",
        "Catppuccin",
        "Modern",
        "Retro",
        "Classic",
    ];
    let mut counts: HashMap<&str, usize> = HashMap::new();
    let mut fav_count = 0usize;
    let mut ready_count = 0usize;
    for t in themes {
        *counts.entry(t.category.as_str()).or_insert(0) += 1;
        if t.is_favorite {
            fav_count += 1;
        }
        if t.has_preview || loader.get_texture(&t.name).is_some() {
            ready_count += 1;
        }
    }

    let q = state.search.trim().to_ascii_lowercase();
    let mut filtered: Vec<&ThemeEntry> = themes
        .iter()
        .filter(|t| {
            if state.category == "Favorites" && !t.is_favorite {
                return false;
            }
            if state.category != "All"
                && state.category != "Favorites"
                && t.category != state.category
            {
                return false;
            }
            if !q.is_empty()
                && !t.name.to_ascii_lowercase().contains(&q)
                && !t.category.to_ascii_lowercase().contains(&q)
            {
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

    // 4-cell Mango-Minimal telemetry ribbon
    let (pr_run, pr_done, pr_total, _) = loader.prerender.snapshot();
    let cache_ratio = if pr_run && pr_total > 0 {
        pr_done as f32 / pr_total as f32
    } else if themes.is_empty() {
        0.0
    } else {
        ready_count as f32 / themes.len() as f32
    };
    telemetry_ribbon(
        ui,
        &[
            TelemetryCell {
                label: pick(lang, "AKTİF TEMA", "ACTIVE THEME"),
                value: if active_theme.is_empty() {
                    "-".to_string()
                } else {
                    active_theme.to_string()
                },
                unit: "",
                sub: pick(lang, "config.rasi @theme", "config.rasi @theme").to_string(),
                ratio: if active_theme.is_empty() { 0.0 } else { 1.0 },
                bar_color: OK,
            },
            TelemetryCell {
                label: pick(lang, "AKTİF PALET", "ACTIVE PALETTE"),
                value: if active_palette.is_empty() {
                    pick(lang, "Varsayılan", "Default").to_string()
                } else {
                    active_palette.to_string()
                },
                unit: "",
                sub: pick(lang, "wallust / shared colors", "wallust / shared colors").to_string(),
                ratio: if active_palette.is_empty() { 0.35 } else { 1.0 },
                bar_color: ACCENT,
            },
            TelemetryCell {
                label: pick(lang, "GÖRÜNEN / TOPLAM", "SHOWN / TOTAL"),
                value: format!("{}/{}", filtered.len(), themes.len()),
                unit: "",
                sub: format!("{} ★ {}", fav_count, pick(lang, "favori", "favorites")),
                ratio: if themes.is_empty() {
                    0.0
                } else {
                    filtered.len() as f32 / themes.len() as f32
                },
                bar_color: ACCENT,
            },
            TelemetryCell {
                label: pick(lang, "ÖNİZLEME ÖNBELLEĞİ", "PREVIEW CACHE"),
                value: if pr_run {
                    format!("{pr_done}/{pr_total}")
                } else {
                    format!("{ready_count}/{}", themes.len())
                },
                unit: "",
                sub: if pr_run {
                    pick(lang, "Direct2D render sürüyor", "Direct2D rendering...").to_string()
                } else {
                    pick(lang, "Hazır PNG önizleme", "Cached PNG previews").to_string()
                },
                ratio: cache_ratio,
                bar_color: if pr_run { ACCENT } else { SIGNAL_BLUE },
            },
        ],
    );

    ui.add_space(10.0);

    // Search + Sort + Density + Categories filter bar
    well_frame().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(pick(lang, "Ara:", "Search:"))
                    .size(12.0)
                    .color(MUTED),
            );
            let search_w = ui.available_width().clamp(160.0, 280.0);
            let resp = ui.add_sized(
                [search_w, 24.0],
                egui::TextEdit::singleline(&mut state.search).hint_text(pick(
                    lang,
                    "tema veya kategori... (Ctrl+K)",
                    "theme or category... (Ctrl+K)",
                )),
            );
            if state.focus_search_requested {
                resp.request_focus();
                state.focus_search_requested = false;
            }
            if !state.search.is_empty() && secondary_button(ui, "×").clicked() {
                state.search.clear();
            }

            ui.add_space(6.0);
            for (ord, lbl) in [
                (SortOrder::Az, "A→Z"),
                (SortOrder::Za, "Z→A"),
                (SortOrder::Newest, pick(lang, "Yeni", "Newest")),
                (SortOrder::Oldest, pick(lang, "Eski", "Oldest")),
            ] {
                if chip_button(ui, lbl, prefs.sort == ord).clicked() && prefs.sort != ord {
                    prefs.sort = ord;
                    act.prefs_changed = true;
                }
            }

            ui.add_space(6.0);
            let density_lbl = if prefs.compact {
                pick(lang, "Görünüm: Kompakt", "View: Compact")
            } else {
                pick(lang, "Görünüm: Geniş", "View: Cozy")
            };
            if chip_button(ui, density_lbl, prefs.compact).clicked() {
                prefs.compact = !prefs.compact;
                act.prefs_changed = true;
            }
        });

        ui.add_space(6.0);
        hairline_rule(ui, RULE_2);
        ui.add_space(6.0);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            for cat in cat_order {
                let cnt = match cat {
                    "All" => themes.len(),
                    "Favorites" => fav_count,
                    other => counts.get(other).copied().unwrap_or(0),
                };
                if cat != "All" && cat != "Favorites" && cnt == 0 {
                    continue;
                }
                let active = state.category == cat;
                let lbl = format!("{} ({cnt})", category_label(lang, cat));
                if chip_button(ui, &lbl, active).clicked() && !active {
                    state.category = cat.to_string();
                }
            }
        });
    });

    ui.add_space(10.0);

    if loading_themes && themes.is_empty() {
        well_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(pick(
                        lang,
                        "Temalar taranıyor...",
                        "Scanning .rasi themes...",
                    ))
                    .size(13.0)
                    .color(MUTED),
                );
            });
        });
        return act;
    }

    if filtered.is_empty() {
        well_frame().show(ui, |ui| {
            ui.label(
                RichText::new(pick(
                    lang,
                    "Filtreyle eşleşen tema bulunamadı.",
                    "No themes match the current filter.",
                ))
                .size(13.0)
                .color(MUTED),
            );
        });
        return act;
    }

    // Virtualized responsive grid with Mango-Minimal 0px cards
    let avail_w = ui.available_width();
    let gap = 12.0;
    let min_card_w = if prefs.compact { 228.0 } else { 296.0 };
    let cols = (((avail_w + gap) / (min_card_w + gap)).floor() as usize).max(1);
    let card_w = ((avail_w - gap * (cols - 1) as f32) / cols as f32).max(180.0);
    let thumb_h = if prefs.compact { 136.0 } else { 184.0 };
    let visible_band = ui.clip_rect().expand2(Vec2::new(0.0, 360.0));

    for row in filtered.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for item in row {
                let is_active = item.name.eq_ignore_ascii_case(active_theme);
                let frame_stroke = if is_active {
                    Stroke::new(1.0_f32, OK)
                } else {
                    Stroke::new(1.0_f32, RULE)
                };
                let resp = egui::Frame::none()
                    .fill(PAPER_2)
                    .stroke(frame_stroke)
                    .rounding(Rounding::ZERO)
                    .inner_margin(Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.set_width(card_w - 22.0);
                        ui.vertical(|ui| {
                            // Thumbnail box
                            let inner_w = ui.available_width();
                            let (thumb_rect, thumb_resp) = ui.allocate_exact_size(
                                Vec2::new(inner_w, thumb_h),
                                egui::Sense::click(),
                            );
                            let p = ui.painter();
                            p.rect_filled(thumb_rect, Rounding::ZERO, PAPER);
                            p.rect_stroke(thumb_rect, Rounding::ZERO, Stroke::new(1.0_f32, RULE_2));

                            if thumb_rect.intersects(visible_band) {
                                loader.request_preview(&item.name, ui.ctx());
                            }

                            if let Some(tex) = loader.get_texture(&item.name) {
                                let tex_sz = tex.size_vec2();
                                let scale = ((thumb_rect.width() - 8.0) / tex_sz.x.max(1.0))
                                    .min((thumb_rect.height() - 8.0) / tex_sz.y.max(1.0))
                                    .min(1.5);
                                let draw_sz = tex_sz * scale;
                                let img_rect = Rect::from_center_size(thumb_rect.center(), draw_sz);
                                p.image(
                                    tex.id(),
                                    img_rect,
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                    egui::Color32::WHITE,
                                );
                            } else if loader.is_failed(&item.name) {
                                p.text(
                                    thumb_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    pick(lang, "Önizleme yok", "No preview"),
                                    egui::FontId::monospace(11.0),
                                    MUTED_2,
                                );
                            } else {
                                p.text(
                                    thumb_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    pick(lang, "Yükleniyor...", "Loading..."),
                                    egui::FontId::monospace(11.0),
                                    MUTED,
                                );
                            }

                            if is_active {
                                let badge_rect = Rect::from_min_size(
                                    Pos2::new(thumb_rect.left() + 6.0, thumb_rect.top() + 6.0),
                                    Vec2::new(48.0, 18.0),
                                );
                                p.rect_filled(badge_rect, Rounding::ZERO, OK);
                                p.text(
                                    badge_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    pick(lang, "AKTİF", "ACTIVE"),
                                    egui::FontId::monospace(10.0),
                                    ACCENT_INK,
                                );
                            }

                            if thumb_resp.hovered() {
                                p.rect_stroke(
                                    thumb_rect,
                                    Rounding::ZERO,
                                    Stroke::new(1.0_f32, ACCENT),
                                );
                            }
                            if thumb_resp.clicked() {
                                act.live_preview = Some(item.name.clone());
                            }
                            thumb_resp.on_hover_text(pick(
                                lang,
                                "Canlı önizleme için tıklayın",
                                "Click for live preview",
                            ));

                            ui.add_space(7.0);

                            // Title + Category row
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&item.name).size(12.5).color(if is_active {
                                            INK
                                        } else {
                                            INK_2
                                        }),
                                    )
                                    .truncate(),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let fav_btn = egui::Button::new(
                                            RichText::new(if item.is_favorite {
                                                "★"
                                            } else {
                                                "☆"
                                            })
                                            .size(12.0)
                                            .color(if item.is_favorite { ACCENT } else { MUTED }),
                                        )
                                        .fill(PAPER_3)
                                        .stroke(Stroke::new(1.0_f32, RULE))
                                        .rounding(Rounding::ZERO);
                                        if ui.add(fav_btn).clicked() {
                                            act.toggle_favorite = Some(item.name.clone());
                                        }
                                        ui.label(
                                            RichText::new(category_label(lang, &item.category))
                                                .size(10.5)
                                                .monospace()
                                                .color(MUTED),
                                        );
                                    },
                                );
                            });

                            ui.add_space(5.0);

                            // Actions row
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                if is_active {
                                    let sel_btn = egui::Button::new(
                                        RichText::new(pick(lang, "Seçili", "Active"))
                                            .size(11.0)
                                            .color(ACCENT_INK),
                                    )
                                    .fill(OK)
                                    .stroke(Stroke::new(1.0_f32, OK))
                                    .rounding(Rounding::ZERO);
                                    let _ = ui.add(sel_btn);
                                } else if accent_outline_button(ui, pick(lang, "Uygula", "Apply"))
                                    .clicked()
                                {
                                    act.apply_theme = Some(item.name.clone());
                                }
                                if secondary_button(ui, pick(lang, "Canlı", "Live")).clicked() {
                                    act.live_preview = Some(item.name.clone());
                                }
                                if secondary_button(ui, pick(lang, "Düzenle", "Edit")).clicked() {
                                    act.edit_theme = Some(item.name.clone());
                                }
                                if secondary_button(ui, ".rasi").clicked() {
                                    act.reveal_theme = Some(item.name.clone());
                                }
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

    // Native Mango-Minimal Modal for importing .rasi file, folder, or pasting code
    if state.show_import_modal {
        egui::Window::new(pick(lang, "LiteRofi'ye Tema Ekle", "Add Theme to LiteRofi"))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(540.0)
            .frame(
                egui::Frame::window(&ui.ctx().style())
                    .fill(PAPER_2)
                    .stroke(Stroke::new(1.0_f32, OUTER_BORDER))
                    .rounding(Rounding::ZERO)
                    .inner_margin(Margin::same(16.0)),
            )
            .show(ui.ctx(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if primary_button(
                        ui,
                        pick(lang, ".rasi Dosyası Seç...", "Select .rasi File(s)..."),
                    )
                    .clicked()
                    {
                        act.pick_rasi_files = true;
                    }
                    if accent_outline_button(
                        ui,
                        pick(lang, "Tema Klasörü İçe Aktar...", "Import Theme Folder..."),
                    )
                    .clicked()
                    {
                        act.pick_theme_folder = true;
                    }
                });
                ui.add_space(6.0);
                ui.label(
                    RichText::new(pick(
                        lang,
                        "İpucu: .rasi dosyalarını doğrudan pencereye sürükleyip bırakabilirsiniz.",
                        "Tip: You can also drag & drop .rasi files directly onto this window.",
                    ))
                    .size(11.0)
                    .color(MUTED),
                );
                ui.add_space(8.0);
                hairline_rule(ui, RULE);
                ui.add_space(8.0);

                ui.label(
                    RichText::new(pick(lang, "Tema Adı", "Theme Name"))
                        .size(12.0)
                        .color(INK_2),
                );
                ui.add_sized(
                    [ui.available_width(), 24.0],
                    egui::TextEdit::singleline(&mut state.paste_name).hint_text("ornek-tema"),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(pick(lang, ".rasi İçeriği", ".rasi Source"))
                        .size(12.0)
                        .color(INK_2),
                );
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        ui.add_sized(
                            [ui.available_width(), 170.0],
                            egui::TextEdit::multiline(&mut state.paste_code)
                                .code_editor()
                                .hint_text("window { background-color: #080706; }"),
                        );
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary_button(ui, pick(lang, "Kaydet ve Uygula", "Save & Apply"))
                            .clicked()
                        {
                            act.save_paste_theme = Some((
                                state.paste_name.trim().to_string(),
                                state.paste_code.clone(),
                            ));
                        }
                        if secondary_button(ui, pick(lang, "İptal", "Cancel")).clicked() {
                            state.show_import_modal = false;
                        }
                    });
                });
            });
    }

    act
}
