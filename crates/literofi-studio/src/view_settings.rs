use crate::backend::{NavLayout, SortOrder, StudioPaths, StudioPrefs};
use crate::i18n::{pick, Lang};
use crate::theme::*;
use crate::ui_widgets::*;
use egui::{Margin, RichText, Rounding, Stroke};

#[derive(Default)]
pub struct SettingsAction {
    pub prefs_changed: bool,
    pub open_themes_dir: bool,
    pub open_widgets_toml: bool,
    pub clear_cache: bool,
}

fn path_info_row(ui: &mut egui::Ui, label: &str, path: &str) {
    egui::Frame::none()
        .fill(PAPER_3)
        .stroke(Stroke::new(1.0_f32, RULE_2))
        .rounding(Rounding::ZERO)
        .inner_margin(Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(label).size(11.0).color(MUTED));
                ui.add_space(2.0);
                ui.add(
                    egui::Label::new(RichText::new(path).size(11.5).monospace().color(INK_2))
                        .truncate(),
                );
            });
        });
    ui.add_space(5.0);
}

pub fn render_settings_view(
    ui: &mut egui::Ui,
    prefs: &mut StudioPrefs,
    paths: &StudioPaths,
) -> SettingsAction {
    let mut act = SettingsAction::default();
    let lang = prefs.lang;

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() * 0.65).max(180.0));
            ui.label(
                RichText::new(pick(lang, "Studio Ayarları", "Studio Settings"))
                    .size(18.0)
                    .color(INK),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(pick(
                        lang,
                        "Navigasyon düzeni, kart yoğunluğu, dil ve yerel önbellek yönetimi",
                        "Navigation layout, card density, language, and local cache controls",
                    ))
                    .size(11.5)
                    .color(MUTED),
                )
                .wrap(),
            );
        });
    });

    ui.add_space(10.0);

    equal_two_columns(ui, "settings_split", |col, ui| match col {
        0 => {
            section_head(
                ui,
                pick(lang, "Arayüz ve Düzen", "Interface & Layout"),
                pick(
                    lang,
                    "Değişiklikler anında studio-prefs.conf dosyasına kaydedilir",
                    "Changes are saved immediately to studio-prefs.conf",
                ),
            );

            let mut layout_val = prefs.nav_layout.as_str().to_string();
            if segmented_str_row(
                ui,
                pick(lang, "Sekme Yerleşimi (Layout)", "Navigation Layout"),
                pick(
                    lang,
                    "Ana sekmeleri üst barda veya sol kenar çubuğunda göster",
                    "Place primary navigation tabs in top bar or left sidebar",
                ),
                &mut layout_val,
                &[
                    ("top", pick(lang, "Üst Çubuk (Top)", "Top Bar")),
                    (
                        "left",
                        pick(lang, "Sol Kenar Çubuğu (Left)", "Left Sidebar"),
                    ),
                ],
            ) {
                prefs.nav_layout = NavLayout::from_str(&layout_val);
                act.prefs_changed = true;
            }

            let mut lang_val = prefs.lang.code().to_string();
            if segmented_str_row(
                ui,
                pick(lang, "Arayüz Dili", "Interface Language"),
                pick(
                    lang,
                    "Tüm başlıklar ve ipuçları seçili dile geçer",
                    "Switches all labels and hints between Turkish and English",
                ),
                &mut lang_val,
                &[("tr", "Türkçe"), ("en", "English")],
            ) {
                prefs.lang = Lang::from_code(&lang_val);
                act.prefs_changed = true;
            }

            let mut density_val = if prefs.compact { "compact" } else { "cozy" }.to_string();
            if segmented_str_row(
                ui,
                pick(lang, "Tema Kart Yoğunluğu", "Theme Card Density"),
                pick(
                    lang,
                    "Tema galerisindeki kart boyutunu belirler",
                    "Controls card size inside the theme gallery",
                ),
                &mut density_val,
                &[
                    ("cozy", pick(lang, "Geniş (Cozy)", "Cozy")),
                    ("compact", pick(lang, "Kompakt", "Compact")),
                ],
            ) {
                prefs.compact = density_val == "compact";
                act.prefs_changed = true;
            }

            let mut sort_val = prefs.sort.as_str().to_string();
            if segmented_str_row(
                ui,
                pick(lang, "Varsayılan Sıralama", "Default Sort Order"),
                pick(
                    lang,
                    "Temalar ve renk paletleri listeleme sırası",
                    "Sorting used for themes and color palettes",
                ),
                &mut sort_val,
                &[
                    ("az", "A→Z"),
                    ("za", "Z→A"),
                    ("newest", pick(lang, "En Yeni", "Newest")),
                    ("oldest", pick(lang, "En Eski", "Oldest")),
                ],
            ) {
                prefs.sort = SortOrder::from_str(&sort_val);
                act.prefs_changed = true;
            }
        }
        _ => {
            section_head(
                ui,
                pick(lang, "Yapılandırma Yolları ve Bakım", "Paths & Maintenance"),
                pick(
                    lang,
                    "Aktif LiteRofi dosyaları ve önbellek araçları",
                    "Active LiteRofi file paths and maintenance actions",
                ),
            );

            path_info_row(
                ui,
                pick(
                    lang,
                    "Ana Yapılandırma (config.rasi)",
                    "Main Config (config.rasi)",
                ),
                &paths.config_rasi.to_string_lossy(),
            );
            path_info_row(
                ui,
                pick(lang, "Temalar Klasörü", "Themes Directory"),
                &paths.themes_dir.to_string_lossy(),
            );
            path_info_row(
                ui,
                pick(
                    lang,
                    "Widget Ayarları (widgets.toml)",
                    "Widgets Config (widgets.toml)",
                ),
                &paths.widgets_toml.to_string_lossy(),
            );
            path_info_row(
                ui,
                pick(lang, "Önizleme Önbelleği", "Preview Cache"),
                &paths.cache_dir.to_string_lossy(),
            );
            path_info_row(
                ui,
                pick(lang, "Studio Tercihleri", "Studio Preferences"),
                &paths.prefs_file.to_string_lossy(),
            );

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                if secondary_button(ui, pick(lang, ".rasi Klasörünü Aç", "Open Themes Folder"))
                    .clicked()
                {
                    act.open_themes_dir = true;
                }
                if secondary_button(ui, pick(lang, "widgets.toml Aç", "Open widgets.toml"))
                    .clicked()
                {
                    act.open_widgets_toml = true;
                }
                if accent_outline_button(
                    ui,
                    pick(lang, "Önizleme Önbelleğini Temizle", "Clear Preview Cache"),
                )
                .clicked()
                {
                    act.clear_cache = true;
                }
            });
        }
    });

    act
}
