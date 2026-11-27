use crate::i18n::{pick, Lang};
use crate::theme::*;
use crate::ui_widgets::*;
use egui::{Margin, RichText, Rounding, Stroke};
use literofi_core::widgets_config::{RasiPluginConfig, WidgetsConfig};

const RICING_MONO_FONTS: &[&str] = &[
    "Fira Code",
    "FiraCode Nerd Font",
    "FiraCode Nerd Font Mono",
    "JetBrainsMono NF",
    "JetBrains Mono",
    "JetBrainsMono Nerd Font",
    "Iosevka",
    "Iosevka Slab",
    "Iosevka Nerd Font",
    "Cascadia Code",
    "Cascadia Mono",
    "CaskaydiaCove Nerd Font",
    "Hack",
    "Hack Nerd Font",
    "MesloLGS NF",
    "Maple Mono",
    "Victor Mono",
    "Geist Mono",
    "IBM Plex Mono",
    "Consolas",
];

const MODERN_UI_FONTS: &[&str] = &[
    "Inter",
    "Segoe UI",
    "Segoe UI Variable",
    "Fira Sans",
    "Geist",
    "IBM Plex Sans",
    "Plus Jakarta Sans",
    "SF Pro Display",
    "Poppins",
    "Roboto",
    "Noto Sans",
    "Outfit",
];

#[derive(Default)]
pub struct WidgetsViewState {
    pub show_raw_toml: bool,
    pub raw_toml_buf: String,
    pub dirty: bool,
    pub custom_font_sec: Option<String>,
    pub custom_font_buf: String,
}

#[derive(Default)]
pub struct WidgetsAction {
    pub save_config: bool,
    pub save_raw_toml: Option<String>,
    pub reset_defaults: bool,
    pub open_toml_file: bool,
    pub preview_mode: Option<String>,
}

fn split_rasi_font(val: &str) -> (String, String) {
    let s = val.trim();
    if let Some((fam, sz)) = s.rsplit_once(' ') {
        if let Ok(n) = sz.trim().parse::<u32>() {
            if (8..=28).contains(&n) && !fam.trim().is_empty() {
                return (fam.trim().to_string(), sz.trim().to_string());
            }
        }
    }
    (
        if s.is_empty() {
            "Segoe UI".to_string()
        } else {
            s.to_string()
        },
        "11".to_string(),
    )
}

fn render_font_selector(
    ui: &mut egui::Ui,
    state: &mut WidgetsViewState,
    sec_id: &str,
    font_val: &mut String,
    is_rasi: bool,
    installed_fonts: &[String],
    lang: Lang,
) -> bool {
    let mut changed = false;
    let (cur_fam, cur_sz) = if is_rasi {
        split_rasi_font(font_val)
    } else {
        (font_val.trim().to_string(), String::new())
    };

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Font").size(12.5).color(INK_2));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if is_rasi {
                    let mut sz = cur_sz.clone();
                    egui::ComboBox::from_id_salt(format!("font_sz_{sec_id}"))
                        .width(64.0)
                        .selected_text(format!("{sz}pt"))
                        .show_ui(ui, |ui| {
                            for opt in ["9", "10", "11", "12", "13", "14", "15", "16"] {
                                if ui.selectable_label(sz == opt, format!("{opt}pt")).clicked() {
                                    sz = opt.to_string();
                                    *font_val = format!("{cur_fam} {sz}");
                                    changed = true;
                                }
                            }
                        });
                }

                let fam_w = (ui.available_width() - 8.0).clamp(88.0, 190.0);
                let mut selected_fam = cur_fam.clone();
                egui::ComboBox::from_id_salt(format!("font_fam_{sec_id}"))
                    .width(fam_w)
                    .selected_text(RichText::new(&selected_fam).size(11.5).color(INK))
                    .show_ui(ui, |ui| {
                        ui.label(
                            RichText::new(pick(
                                lang,
                                "★ Ricing & Nerd Fonts",
                                "★ Ricing & Nerd Fonts",
                            ))
                            .size(10.5)
                            .color(ACCENT),
                        );
                        for &f in RICING_MONO_FONTS {
                            let mark = installed_fonts
                                .iter()
                                .any(|inst| inst.eq_ignore_ascii_case(f));
                            let lbl = if mark {
                                format!("{f} ✓")
                            } else {
                                f.to_string()
                            };
                            if ui
                                .selectable_label(selected_fam.eq_ignore_ascii_case(f), lbl)
                                .clicked()
                            {
                                selected_fam = f.to_string();
                                *font_val = if is_rasi {
                                    format!("{selected_fam} {cur_sz}")
                                } else {
                                    selected_fam.clone()
                                };
                                changed = true;
                            }
                        }

                        ui.separator();
                        ui.label(
                            RichText::new(pick(lang, "Modern Arayüz", "Modern UI"))
                                .size(10.5)
                                .color(ACCENT),
                        );
                        for &f in MODERN_UI_FONTS {
                            let mark = installed_fonts
                                .iter()
                                .any(|inst| inst.eq_ignore_ascii_case(f));
                            let lbl = if mark {
                                format!("{f} ✓")
                            } else {
                                f.to_string()
                            };
                            if ui
                                .selectable_label(selected_fam.eq_ignore_ascii_case(f), lbl)
                                .clicked()
                            {
                                selected_fam = f.to_string();
                                *font_val = if is_rasi {
                                    format!("{selected_fam} {cur_sz}")
                                } else {
                                    selected_fam.clone()
                                };
                                changed = true;
                            }
                        }

                        if !installed_fonts.is_empty() {
                            ui.separator();
                            ui.label(
                                RichText::new(pick(lang, "Sistemde Kurulu", "Installed on PC"))
                                    .size(10.5)
                                    .color(OK),
                            );
                            for f in installed_fonts {
                                if RICING_MONO_FONTS.iter().any(|x| x.eq_ignore_ascii_case(f))
                                    || MODERN_UI_FONTS.iter().any(|x| x.eq_ignore_ascii_case(f))
                                {
                                    continue;
                                }
                                if ui
                                    .selectable_label(
                                        selected_fam.eq_ignore_ascii_case(f),
                                        format!("{f} ✓"),
                                    )
                                    .clicked()
                                {
                                    selected_fam = f.clone();
                                    *font_val = if is_rasi {
                                        format!("{selected_fam} {cur_sz}")
                                    } else {
                                        selected_fam.clone()
                                    };
                                    changed = true;
                                }
                            }
                        }

                        ui.separator();
                        if ui
                            .selectable_label(
                                false,
                                pick(lang, "✎ Özel font adı gir...", "✎ Enter custom font..."),
                            )
                            .clicked()
                        {
                            state.custom_font_sec = Some(sec_id.to_string());
                            state.custom_font_buf = cur_fam.clone();
                        }
                    });
            });
        });

        if state.custom_font_sec.as_deref() == Some(sec_id) {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_sized(
                    [160.0, 22.0],
                    egui::TextEdit::singleline(&mut state.custom_font_buf),
                );
                if primary_button(ui, "OK").clicked() {
                    let clean = state.custom_font_buf.trim();
                    if !clean.is_empty() {
                        *font_val = if is_rasi {
                            format!("{clean} {cur_sz}")
                        } else {
                            clean.to_string()
                        };
                        changed = true;
                    }
                    state.custom_font_sec = None;
                }
                if secondary_button(ui, "×").clicked() {
                    state.custom_font_sec = None;
                }
            });
        }
    });
    ui.add_space(3.0);
    hairline_rule(ui, RULE_2);
    ui.add_space(3.0);
    changed
}

struct RasiPluginMeta<'a> {
    id: &'a str,
    title: &'a str,
    hotkey: &'a str,
}

fn render_rasi_plugin_panel(
    ui: &mut egui::Ui,
    state: &mut WidgetsViewState,
    meta: RasiPluginMeta<'_>,
    cfg: &mut RasiPluginConfig,
    installed_fonts: &[String],
    lang: Lang,
    act: &mut WidgetsAction,
) {
    let sec_id = meta.id;
    let title = meta.title;
    let hotkey = meta.hotkey;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(14.5).color(INK));
            ui.label(RichText::new(hotkey).size(11.0).monospace().color(ACCENT));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if secondary_button(ui, pick(lang, "Canlı Aç", "Preview")).clicked() {
                act.preview_mode = Some(sec_id.to_string());
            }
        });
    });
    ui.add_space(4.0);
    hairline_rule(ui, RULE);
    ui.add_space(6.0);

    if segmented_str_row(
        ui,
        pick(lang, "Tema Modu", "Theme Mode"),
        "",
        &mut cfg.theme_mode,
        &[
            ("active", pick(lang, "Aktif Temayı İzle", "Follow Active")),
            ("custom", pick(lang, "Özel Renkler", "Custom Colors")),
        ],
    ) {
        state.dirty = true;
    }

    if render_font_selector(
        ui,
        state,
        sec_id,
        &mut cfg.font,
        true,
        installed_fonts,
        lang,
    ) {
        state.dirty = true;
    }

    let is_custom = cfg.theme_mode == "custom";
    let k_bg = format!("{sec_id}_bg");
    let k_alt = format!("{sec_id}_alt");
    let k_fg = format!("{sec_id}_fg");
    let k_acc = format!("{sec_id}_acc");
    if color_picker_grid(
        ui,
        &mut [
            (&k_bg, "BG", &mut cfg.bg),
            (&k_alt, "Alt", &mut cfg.bg_alt),
            (&k_fg, "FG", &mut cfg.fg),
            (&k_acc, "Accent", &mut cfg.accent),
        ],
        is_custom,
    ) {
        state.dirty = true;
    }
}

pub fn render_widgets_view(
    ui: &mut egui::Ui,
    state: &mut WidgetsViewState,
    cfg: &mut WidgetsConfig,
    config_path: &str,
    installed_fonts: &[String],
    lang: Lang,
) -> WidgetsAction {
    let mut act = WidgetsAction::default();

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() * 0.56).max(200.0));
            ui.label(
                RichText::new(pick(
                    lang,
                    "Widget ve Eklenti Ayarları",
                    "Widgets & Plugins",
                ))
                .size(18.0)
                .color(INK),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(config_path)
                        .size(11.0)
                        .monospace()
                        .color(MUTED),
                )
                .truncate(),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.horizontal_wrapped(|ui| {
                if secondary_button(ui, pick(lang, "Dosyayı Aç", "Open File")).clicked() {
                    act.open_toml_file = true;
                }
                if secondary_button(ui, pick(lang, "Ham TOML", "Raw TOML")).clicked() {
                    state.show_raw_toml = !state.show_raw_toml;
                    if state.show_raw_toml {
                        state.raw_toml_buf = cfg.to_toml();
                    }
                }
                if secondary_button(ui, pick(lang, "Sıfırla", "Reset")).clicked() {
                    act.reset_defaults = true;
                }
                let save_clicked = if state.dirty {
                    primary_button(ui, pick(lang, "Tümünü Kaydet *", "Save All *")).clicked()
                } else {
                    secondary_button(ui, pick(lang, "Tümünü Kaydet", "Save All")).clicked()
                };
                if save_clicked {
                    act.save_config = true;
                }
            });
        });
    });

    ui.add_space(10.0);

    if state.show_raw_toml {
        well_frame().show(ui, |ui| {
            section_head(
                ui,
                "widgets.toml",
                pick(
                    lang,
                    "Doğrudan TOML düzenleme",
                    "Direct TOML source editing",
                ),
            );
            egui::ScrollArea::vertical()
                .max_height(210.0)
                .show(ui, |ui| {
                    ui.add_sized(
                        [ui.available_width(), 200.0],
                        egui::TextEdit::multiline(&mut state.raw_toml_buf).code_editor(),
                    );
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary_button(ui, pick(lang, "TOML Kaydet", "Save TOML")).clicked() {
                        act.save_raw_toml = Some(state.raw_toml_buf.clone());
                    }
                });
            });
        });
        ui.add_space(10.0);
    }

    // Row 1: Game Launcher | Music Player
    equal_two_columns(ui, "widgets_row_1", |col, ui| match col {
        0 => {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(pick(lang, "Oyun Başlatıcı", "Game Launcher"))
                            .size(14.5)
                            .color(INK),
                    );
                    ui.label(RichText::new("Alt+G").size(11.0).monospace().color(ACCENT));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if secondary_button(ui, pick(lang, "Canlı Aç", "Preview")).clicked() {
                        act.preview_mode = Some("games".to_string());
                    }
                });
            });
            ui.add_space(4.0);
            hairline_rule(ui, RULE);
            ui.add_space(6.0);

            if segmented_str_row(
                ui,
                pick(lang, "Tema Modu", "Theme Mode"),
                "",
                &mut cfg.games.theme_mode,
                &[
                    ("active", pick(lang, "Aktif Temayı İzle", "Follow Active")),
                    ("custom", pick(lang, "Özel Renkler", "Custom Colors")),
                ],
            ) {
                state.dirty = true;
            }
            if render_font_selector(
                ui,
                state,
                "games",
                &mut cfg.games.font,
                false,
                installed_fonts,
                lang,
            ) {
                state.dirty = true;
            }
            let mut rad = cfg.games.radius as i32;
            if slider_row(
                ui,
                pick(lang, "Köşe Yarıçapı", "Corner Radius"),
                &mut rad,
                0,
                32,
                "px",
                Some(18),
            ) {
                cfg.games.radius = rad.clamp(0, 32) as u32;
                state.dirty = true;
            }
            let is_custom = cfg.games.theme_mode == "custom";
            if color_picker_grid(
                ui,
                &mut [
                    ("g_ground", "Ground", &mut cfg.games.ground),
                    ("g_surf", "Surface", &mut cfg.games.surface),
                    ("g_ink", "Ink", &mut cfg.games.ink),
                    ("g_acc", "Accent", &mut cfg.games.accent),
                    ("g_aink", "Btn Text", &mut cfg.games.accent_ink),
                ],
                is_custom,
            ) {
                state.dirty = true;
            }
        }
        _ => {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(pick(lang, "Müzik Çalar (Limusic)", "Music Player"))
                            .size(14.5)
                            .color(INK),
                    );
                    ui.label(RichText::new("Alt+M").size(11.0).monospace().color(ACCENT));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if secondary_button(ui, pick(lang, "Canlı Aç", "Preview")).clicked() {
                        act.preview_mode = Some("music".to_string());
                    }
                });
            });
            ui.add_space(4.0);
            hairline_rule(ui, RULE);
            ui.add_space(6.0);

            if segmented_str_row(
                ui,
                pick(lang, "Tema Modu", "Theme Mode"),
                pick(
                    lang,
                    "Albüm kapağından otomatik renk veya sabit palet",
                    "Auto album art colors or fixed palette",
                ),
                &mut cfg.music.theme_mode,
                &[
                    ("auto", pick(lang, "Kapak Resmi", "Auto Cover")),
                    ("active", pick(lang, "Aktif Tema", "Active Theme")),
                    ("custom", pick(lang, "Özel", "Custom")),
                ],
            ) {
                state.dirty = true;
            }
            if render_font_selector(
                ui,
                state,
                "music",
                &mut cfg.music.font,
                false,
                installed_fonts,
                lang,
            ) {
                state.dirty = true;
            }
            let is_custom = cfg.music.theme_mode == "custom";
            if color_picker_grid(
                ui,
                &mut [
                    ("m_bg", "Background", &mut cfg.music.bg),
                    ("m_alt", "Surface", &mut cfg.music.bg_alt),
                    ("m_acc", "Accent", &mut cfg.music.accent),
                ],
                is_custom,
            ) {
                state.dirty = true;
            }
        }
    });

    ui.add_space(10.0);

    // Row 2: Wallpaper Carousel + Wallhaven | Wallpaper Engine Filters
    equal_two_columns(ui, "widgets_row_2", |col, ui| match col {
        0 => {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(pick(
                            lang,
                            "Duvar Kağıdı Vitrini & Wallhaven",
                            "Wallpaper Carousel & Wallhaven",
                        ))
                        .size(14.5)
                        .color(INK),
                    );
                    ui.label(
                        RichText::new("Alt+W → W")
                            .size(11.0)
                            .monospace()
                            .color(ACCENT),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if secondary_button(ui, pick(lang, "Canlı Aç", "Preview")).clicked() {
                        act.preview_mode = Some("wallpaper".to_string());
                    }
                });
            });
            ui.add_space(4.0);
            hairline_rule(ui, RULE);
            ui.add_space(6.0);

            if segmented_str_row(
                ui,
                pick(lang, "Vurgu Çerçevesi", "Highlight Accent"),
                "",
                &mut cfg.wallpaper.theme_mode,
                &[
                    ("active", pick(lang, "Aktif Tema", "Follow Active")),
                    ("custom", pick(lang, "Özel Vurgu", "Custom Accent")),
                ],
            ) {
                state.dirty = true;
            }
            if cfg.wallpaper.theme_mode == "custom"
                && color_picker_cell(
                    ui,
                    "wp_acc",
                    pick(lang, "Seçili Çerçeve", "Selected Border"),
                    &mut cfg.wallpaper.accent,
                    true,
                )
            {
                state.dirty = true;
            }
            ui.add_space(4.0);

            // Wallhaven query
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(pick(lang, "Arama / Etiket", "Search / Tag"))
                        .size(12.5)
                        .color(INK_2),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let w = (ui.available_width() - 8.0).clamp(90.0, 200.0);
                    if ui
                        .add_sized(
                            [w, 22.0],
                            egui::TextEdit::singleline(&mut cfg.wallpaper.wallhaven_query)
                                .hint_text("cyberpunk, minimal..."),
                        )
                        .changed()
                    {
                        state.dirty = true;
                    }
                });
            });
            ui.add_space(4.0);
            hairline_rule(ui, RULE_2);
            ui.add_space(4.0);

            // Wallhaven categories
            let mut chars: Vec<char> = format!("{:0<3}", cfg.wallpaper.wallhaven_categories)
                .chars()
                .take(3)
                .collect();
            while chars.len() < 3 {
                chars.push('1');
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(pick(lang, "Kategoriler", "Categories"))
                        .size(12.5)
                        .color(INK_2),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (idx, lbl) in [
                        (2usize, pick(lang, "İnsanlar", "People")),
                        (1usize, "Anime"),
                        (0usize, pick(lang, "Genel", "General")),
                    ] {
                        let on = chars[idx] == '1';
                        if chip_button(ui, lbl, on).clicked() {
                            chars[idx] = if on { '0' } else { '1' };
                            if chars.iter().all(|&c| c == '0') {
                                chars[idx] = '1';
                            }
                            cfg.wallpaper.wallhaven_categories = chars.iter().collect();
                            state.dirty = true;
                        }
                    }
                });
            });
            ui.add_space(4.0);
            hairline_rule(ui, RULE_2);
            ui.add_space(4.0);

            let sort_opts: &[(&str, &str)] = &[
                (
                    "random",
                    pick(lang, "Rastgele (Dinamik)", "Random (Dynamic)"),
                ),
                ("toplist", pick(lang, "En İyi (Toplist)", "Toplist")),
                (
                    "favorites",
                    pick(lang, "En Çok Favorilenen", "Most Favorited"),
                ),
                ("views", pick(lang, "En Çok Görüntülenen", "Most Viewed")),
                ("date_added", pick(lang, "En Yeni", "Latest")),
                ("hot", pick(lang, "Popüler (Hot)", "Hot")),
            ];
            let sort_sel = sort_opts
                .iter()
                .find(|(k, _)| *k == cfg.wallpaper.wallhaven_sorting)
                .map(|(_, l)| *l)
                .unwrap_or("Random");
            if combo_row(
                ui,
                "wh_sort",
                pick(lang, "Sıralama", "Sort By"),
                sort_sel,
                &mut cfg.wallpaper.wallhaven_sorting,
                sort_opts,
            ) {
                state.dirty = true;
            }

            if cfg.wallpaper.wallhaven_sorting == "toplist" {
                let range_opts: &[(&str, &str)] = &[
                    ("1d", pick(lang, "Son 1 Gün", "Last Day")),
                    ("3d", pick(lang, "Son 3 Gün", "Last 3 Days")),
                    ("1w", pick(lang, "Son 1 Hafta", "Last Week")),
                    ("1M", pick(lang, "Son 1 Ay", "Last Month")),
                    ("3M", pick(lang, "Son 3 Ay", "Last 3 Months")),
                    ("6M", pick(lang, "Son 6 Ay", "Last 6 Months")),
                    ("1y", pick(lang, "Son 1 Yıl", "Last Year")),
                ];
                let range_sel = range_opts
                    .iter()
                    .find(|(k, _)| *k == cfg.wallpaper.wallhaven_top_range)
                    .map(|(_, l)| *l)
                    .unwrap_or("1M");
                if combo_row(
                    ui,
                    "wh_range",
                    pick(lang, "Zaman Aralığı", "Top Range"),
                    range_sel,
                    &mut cfg.wallpaper.wallhaven_top_range,
                    range_opts,
                ) {
                    state.dirty = true;
                }
            }

            let res_opts: &[(&str, &str)] = &[
                ("", pick(lang, "Tümü", "Any")),
                ("1920x1080", "1080p Full HD"),
                ("2560x1440", "2K QHD"),
                ("3840x2160", "4K UHD"),
                ("3440x1440", "Ultrawide 3440x1440"),
            ];
            let res_sel = res_opts
                .iter()
                .find(|(k, _)| *k == cfg.wallpaper.wallhaven_atleast)
                .map(|(_, l)| *l)
                .unwrap_or("Any");
            if combo_row(
                ui,
                "wh_res",
                pick(lang, "Min. Çözünürlük", "Min Resolution"),
                res_sel,
                &mut cfg.wallpaper.wallhaven_atleast,
                res_opts,
            ) {
                state.dirty = true;
            }

            let ratio_opts: &[(&str, &str)] = &[
                ("", pick(lang, "Tümü", "Any")),
                ("16x9", "16:9"),
                ("16x10", "16:10"),
                ("21x9", "21:9 (Ultrawide)"),
                ("32x9", "32:9 (Superwide)"),
                ("landscape", pick(lang, "Yatay", "Landscape")),
                ("portrait", pick(lang, "Dikey", "Portrait")),
            ];
            let ratio_sel = ratio_opts
                .iter()
                .find(|(k, _)| *k == cfg.wallpaper.wallhaven_ratios)
                .map(|(_, l)| *l)
                .unwrap_or("Any");
            if combo_row(
                ui,
                "wh_ratio",
                pick(lang, "En-Boy Oranı", "Aspect Ratio"),
                ratio_sel,
                &mut cfg.wallpaper.wallhaven_ratios,
                ratio_opts,
            ) {
                state.dirty = true;
            }

            let col_opts: &[(&str, &str)] = &[
                ("", pick(lang, "Tümü", "Any")),
                ("000000", pick(lang, "Siyah / Koyu", "Black / Dark")),
                ("424153", pick(lang, "Kül / Koyu Gri", "Slate Gray")),
                ("660000", pick(lang, "Kızıl / Bordo", "Crimson")),
                ("cc6633", pick(lang, "Kehribar / Turuncu", "Amber / Orange")),
                ("336600", pick(lang, "Orman Yeşili", "Forest Green")),
                ("0066cc", pick(lang, "Gece Mavisi", "Deep Blue")),
                ("663399", pick(lang, "Mor / Violet", "Purple / Violet")),
                ("009999", pick(lang, "Turkuaz / Teal", "Teal / Cyan")),
            ];
            let col_sel = col_opts
                .iter()
                .find(|(k, _)| *k == cfg.wallpaper.wallhaven_color)
                .map(|(_, l)| *l)
                .unwrap_or("Any");
            if combo_row(
                ui,
                "wh_color",
                pick(lang, "Renk Tonu", "Color Tone"),
                col_sel,
                &mut cfg.wallpaper.wallhaven_color,
                col_opts,
            ) {
                state.dirty = true;
            }
        }
        _ => {
            section_head(
                ui,
                pick(
                    lang,
                    "Wallpaper Engine Atölye Filtreleri",
                    "Wallpaper Engine Workshop Filters",
                ),
                "Alt+W → E",
            );

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(pick(lang, "Arama / Etiket", "Search / Tag"))
                        .size(12.5)
                        .color(INK_2),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let w = (ui.available_width() - 8.0).clamp(90.0, 200.0);
                    if ui
                        .add_sized(
                            [w, 22.0],
                            egui::TextEdit::singleline(&mut cfg.wallpaper.we_query)
                                .hint_text("dark, rain, minimal, space..."),
                        )
                        .changed()
                    {
                        state.dirty = true;
                    }
                });
            });
            ui.add_space(4.0);
            hairline_rule(ui, RULE_2);
            ui.add_space(4.0);

            let genre_opts: &[(&str, &str)] = &[
                ("", pick(lang, "Tümü", "All")),
                ("Landscape", pick(lang, "Manzara (Landscape)", "Landscape")),
                ("Cyberpunk", "Cyberpunk"),
                ("Sci-Fi", "Sci-Fi"),
                ("Nature", pick(lang, "Doğa (Nature)", "Nature")),
                ("Abstract", pick(lang, "Soyut (Abstract)", "Abstract")),
                ("Minimalist", "Minimalist"),
                ("Fantasy", pick(lang, "Fantastik (Fantasy)", "Fantasy")),
                ("Space", pick(lang, "Uzay (Space)", "Space")),
                ("Game", pick(lang, "Oyun (Game)", "Game")),
                ("Retro", "Retro"),
                ("Technology", pick(lang, "Teknoloji (Tech)", "Technology")),
                ("Anime", "Anime"),
            ];
            let g_sel = genre_opts
                .iter()
                .find(|(k, _)| *k == cfg.wallpaper.we_genre)
                .map(|(_, l)| *l)
                .unwrap_or("All");
            if combo_row(
                ui,
                "we_genre",
                pick(lang, "Tür / Kategori", "Genre / Tag"),
                g_sel,
                &mut cfg.wallpaper.we_genre,
                genre_opts,
            ) {
                state.dirty = true;
            }

            let type_opts: &[(&str, &str)] = &[
                ("", pick(lang, "Tümü (Hepsi)", "All Types")),
                ("Scene", "Scene"),
                ("Video", "Video"),
                ("Web", "Web"),
            ];
            let t_sel = type_opts
                .iter()
                .find(|(k, _)| *k == cfg.wallpaper.we_type)
                .map(|(_, l)| *l)
                .unwrap_or("All");
            if combo_row(
                ui,
                "we_type",
                pick(lang, "İçerik Tipi", "Content Type"),
                t_sel,
                &mut cfg.wallpaper.we_type,
                type_opts,
            ) {
                state.dirty = true;
            }

            let we_sort_opts: &[(&str, &str)] = &[
                ("toprated", pick(lang, "En Yüksek Puanlı", "Top Rated")),
                ("trend", pick(lang, "Trend (Popüler)", "Trending")),
                ("mostrecent", pick(lang, "En Yeni", "Most Recent")),
            ];
            let ws_sel = we_sort_opts
                .iter()
                .find(|(k, _)| *k == cfg.wallpaper.we_sort)
                .map(|(_, l)| *l)
                .unwrap_or("Top Rated");
            if combo_row(
                ui,
                "we_sort",
                pick(lang, "Sıralama", "Sort By"),
                ws_sel,
                &mut cfg.wallpaper.we_sort,
                we_sort_opts,
            ) {
                state.dirty = true;
            }

            if toggle_row(
                ui,
                pick(lang, "Anime Etiketini Gizle", "Exclude Anime Tag"),
                pick(
                    lang,
                    "Keşif sorgularından Anime içeriklerini hariç tutar",
                    "Filters out Anime tag unless explicitly selected",
                ),
                &mut cfg.wallpaper.we_exclude_anime,
            ) {
                state.dirty = true;
            }

            if toggle_row(
                ui,
                pick(
                    lang,
                    "Ses Varsayılan Kapalı Gelsin",
                    "Mute Audio by Default",
                ),
                pick(
                    lang,
                    "Uygulanan duvar kağıtlarında Wallpaper Engine sesini kapatır",
                    "Applies silent mode when activating Wallpaper Engine wallpapers",
                ),
                &mut cfg.wallpaper.we_mute_audio,
            ) {
                state.dirty = true;
            }
        }
    });

    ui.add_space(10.0);

    // Row 3: Clipboard | Control Center
    equal_two_columns(ui, "widgets_row_3", |col, ui| match col {
        0 => render_rasi_plugin_panel(
            ui,
            state,
            RasiPluginMeta {
                id: "clipboard",
                title: pick(lang, "Pano Yöneticisi", "Clipboard Manager"),
                hotkey: "Alt+V",
            },
            &mut cfg.clipboard,
            installed_fonts,
            lang,
            &mut act,
        ),
        _ => render_rasi_plugin_panel(
            ui,
            state,
            RasiPluginMeta {
                id: "control",
                title: pick(lang, "Kontrol Merkezi", "Control Center"),
                hotkey: "Alt+D",
            },
            &mut cfg.control,
            installed_fonts,
            lang,
            &mut act,
        ),
    });

    ui.add_space(10.0);

    // Row 4: Power Menu | Quick Shortcuts Reference
    equal_two_columns(ui, "widgets_row_4", |col, ui| match col {
        0 => render_rasi_plugin_panel(
            ui,
            state,
            RasiPluginMeta {
                id: "power",
                title: pick(lang, "Güç Menüsü", "Power Menu"),
                hotkey: "Alt+P",
            },
            &mut cfg.power,
            installed_fonts,
            lang,
            &mut act,
        ),
        _ => {
            section_head(
                ui,
                pick(
                    lang,
                    "LiteRofi Kısayol Referansı",
                    "LiteRofi Hotkey Reference",
                ),
                pick(
                    lang,
                    "Arka plan daemon kısayolları",
                    "Active background daemon shortcuts",
                ),
            );
            for (k, desc) in [
                (
                    "Alt+Space",
                    pick(lang, "Ana Uygulama Başlatıcı (drun)", "App Launcher (drun)"),
                ),
                (
                    "Ctrl+Alt+Space",
                    pick(
                        lang,
                        "Omarchy Birleşik Sistem Menüsü",
                        "Unified System Menu",
                    ),
                ),
                (
                    "Alt+W / W / E",
                    pick(
                        lang,
                        "Duvar Kağıdı / Wallhaven / WE Keşfet",
                        "Wallpaper / Wallhaven / WE Discover",
                    ),
                ),
                (
                    "Alt+G / Alt+M",
                    pick(
                        lang,
                        "Oyun Başlatıcı / Müzik Çalar",
                        "Game Launcher / Music Player",
                    ),
                ),
                (
                    "Alt+V / Alt+D / Alt+P",
                    pick(
                        lang,
                        "Pano / Kontrol Merkezi / Güç",
                        "Clipboard / Control / Power",
                    ),
                ),
                (
                    "Alt+C / Alt+E / Alt+T",
                    pick(
                        lang,
                        "Hesap / Emoji / Hızlı Çeviri",
                        "Calc / Emoji / Translate",
                    ),
                ),
            ] {
                egui::Frame::none()
                    .fill(PAPER_3)
                    .stroke(Stroke::new(1.0_f32, RULE_2))
                    .rounding(Rounding::ZERO)
                    .inner_margin(Margin::symmetric(8.0, 5.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(k).size(11.5).monospace().color(ACCENT));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(desc).size(11.0).color(INK_2),
                                        )
                                        .truncate(),
                                    );
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }
        }
    });

    act
}
