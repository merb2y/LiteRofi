#![cfg_attr(windows, windows_subsystem = "windows")]

mod backend;
mod i18n;
mod preview_loader;
mod theme;
mod ui_widgets;
mod view_editor;
mod view_palettes;
mod view_settings;
mod view_themes;
mod view_widgets;
mod win32_chrome;

use backend::{
    apply_palette, choose_rasi_files, choose_theme_folder, clear_preview_cache, import_theme_file,
    list_installed_fonts, list_palettes, list_themes, load_favorites, open_themes_folder,
    open_widgets_config, read_active_palette, read_active_theme, reveal_theme_in_explorer,
    save_custom_palette, save_favorites, LivePreviewProc, NavLayout, Palette, StudioPaths,
    StudioPrefs, ThemeEntry,
};
use egui::{
    Key, LayerId, Order, Pos2, Rect, ResizeDirection, RichText, Rounding, Sense, Stroke, Vec2,
};
use i18n::{pick, Lang};
use literofi_core::widgets_config::WidgetsConfig;
use preview_loader::PreviewLoader;
use std::collections::HashSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use theme::*;
use view_editor::EditorViewState;
use view_palettes::PalettesViewState;
use view_themes::ThemesViewState;
use view_widgets::WidgetsViewState;
use win32_chrome::{
    apply_dwm_window_chrome, bring_to_foreground, ensure_window_min_size_centered,
    try_acquire_single_instance,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioTab {
    Themes,
    Palettes,
    Widgets,
    Editor,
    Settings,
}

impl StudioTab {
    const ALL: &'static [(StudioTab, &'static str, &'static str)] = &[
        (StudioTab::Themes, "Temalar", "Themes"),
        (StudioTab::Palettes, "Renk Paletleri", "Palettes"),
        (StudioTab::Widgets, "Widget & Eklenti", "Widgets"),
        (StudioTab::Editor, "Rasi Editör", "Rasi Editor"),
        (StudioTab::Settings, "Ayarlar", "Settings"),
    ];
}

enum BgEvent {
    ThemesLoaded(Vec<ThemeEntry>),
    FontsLoaded(Vec<String>),
    FilesPicked(Vec<std::path::PathBuf>),
    FolderPicked(Result<Option<String>, String>),
}

pub struct StudioApp {
    tab: StudioTab,
    paths: StudioPaths,
    prefs: StudioPrefs,
    themes: Vec<ThemeEntry>,
    palettes: Vec<Palette>,
    installed_fonts: Vec<String>,
    active_theme: String,
    active_palette: String,
    widgets_cfg: WidgetsConfig,
    live_proc: LivePreviewProc,
    preview_loader: PreviewLoader,

    themes_state: ThemesViewState,
    palettes_state: PalettesViewState,
    widgets_state: WidgetsViewState,
    editor_state: EditorViewState,

    loading_themes: bool,
    bg_rx: Option<mpsc::Receiver<BgEvent>>,
    bg_tx: Option<mpsc::Sender<BgEvent>>,

    banner: Option<(String, bool, Instant)>,
    chrome_applied: bool,
    main_hwnd: Option<isize>,
    initial_size_applied: bool,
    size_fix_attempts: u8,
}

impl StudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_windows_fonts(&cc.egui_ctx);
        apply_rim_theme(&cc.egui_ctx);
        let app = Self::build(false);
        app.start_background_loads(Some(cc.egui_ctx.clone()));
        app
    }

    #[cfg(test)]
    pub fn new_headless() -> Self {
        let mut app = Self::build(true);
        app.themes = vec![
            ThemeEntry {
                name: "mango-minimal".to_string(),
                category: "Modern".to_string(),
                has_preview: false,
                is_favorite: true,
                mtime: 100.0,
            },
            ThemeEntry {
                name: "catppuccin-mocha".to_string(),
                category: "Catppuccin".to_string(),
                has_preview: false,
                is_favorite: false,
                mtime: 90.0,
            },
        ];
        app.active_theme = "mango-minimal".to_string();
        app.palettes = backend::builtin_palettes().to_vec();
        app.loading_themes = false;
        app
    }

    fn build(headless: bool) -> Self {
        let paths = StudioPaths::detect();
        let prefs = if headless {
            StudioPrefs::default()
        } else {
            StudioPrefs::load(&paths.prefs_file)
        };
        let (active_theme, active_palette, palettes, widgets_cfg) = if headless {
            (
                String::new(),
                String::new(),
                Vec::new(),
                WidgetsConfig::default(),
            )
        } else {
            (
                read_active_theme(&paths.config_rasi),
                read_active_palette(&paths.config_root),
                list_palettes(&paths.colors_dir),
                WidgetsConfig::load_from(&paths.widgets_toml),
            )
        };
        let (tx, rx) = mpsc::channel();
        let mut editor_state = EditorViewState::default();
        if !headless && !active_theme.is_empty() {
            editor_state.load_theme_from_disk(&paths, &active_theme);
        } else {
            editor_state.revalidate(&paths);
        }

        Self {
            tab: StudioTab::Themes,
            preview_loader: PreviewLoader::new(paths.clone()),
            paths,
            prefs,
            themes: Vec::new(),
            palettes,
            installed_fonts: Vec::new(),
            active_theme,
            active_palette,
            widgets_cfg,
            live_proc: LivePreviewProc::new(),
            themes_state: ThemesViewState::default(),
            palettes_state: PalettesViewState::default(),
            widgets_state: WidgetsViewState::default(),
            editor_state,
            loading_themes: !headless,
            bg_rx: Some(rx),
            bg_tx: Some(tx),
            banner: None,
            chrome_applied: headless,
            main_hwnd: None,
            initial_size_applied: headless,
            size_fix_attempts: 0,
        }
    }

    fn start_background_loads(&self, ctx: Option<egui::Context>) {
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        let paths = self.paths.clone();
        let c1 = ctx.clone();
        std::thread::spawn(move || {
            let themes = list_themes(&paths);
            let _ = tx.send(BgEvent::ThemesLoaded(themes));
            if let Some(c) = c1 {
                c.request_repaint();
            }
        });

        let Some(tx2) = self.bg_tx.clone() else {
            return;
        };
        let c2 = ctx;
        std::thread::spawn(move || {
            let fonts = list_installed_fonts();
            let _ = tx2.send(BgEvent::FontsLoaded(fonts));
            if let Some(c) = c2 {
                c.request_repaint();
            }
        });
    }

    fn reload_themes_async(&mut self, ctx: &egui::Context) {
        self.loading_themes = true;
        let Some(tx) = self.bg_tx.clone() else {
            return;
        };
        let paths = self.paths.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let themes = list_themes(&paths);
            let _ = tx.send(BgEvent::ThemesLoaded(themes));
            c.request_repaint();
        });
    }

    fn set_banner(&mut self, msg: impl Into<String>, is_err: bool) {
        self.banner = Some((msg.into(), is_err, Instant::now()));
    }

    fn poll_background(&mut self, ctx: &egui::Context) {
        self.preview_loader.pump_uploads(ctx);

        let mut events = Vec::new();
        if let Some(rx) = &self.bg_rx {
            while let Ok(ev) = rx.try_recv() {
                events.push(ev);
            }
        }
        for ev in events {
            match ev {
                BgEvent::ThemesLoaded(list) => {
                    self.loading_themes = false;
                    self.themes = list;
                    if self.editor_state.selected_theme.is_empty() && !self.active_theme.is_empty()
                    {
                        self.editor_state
                            .load_theme_from_disk(&self.paths, &self.active_theme);
                    }
                }
                BgEvent::FontsLoaded(fonts) => {
                    self.installed_fonts = fonts;
                }
                BgEvent::FilesPicked(files) => {
                    self.import_dropped_or_picked_paths(&files, ctx);
                }
                BgEvent::FolderPicked(res) => match res {
                    Ok(Some(name)) => {
                        self.themes_state.show_import_modal = false;
                        self.prefs.sort = backend::SortOrder::Newest;
                        self.prefs.save(&self.paths.prefs_file);
                        self.reload_themes_async(ctx);
                        self.set_banner(
                            format!(
                                "{}: {name}",
                                pick(
                                    self.prefs.lang,
                                    "Tema paketi eklendi",
                                    "Theme folder imported"
                                )
                            ),
                            false,
                        );
                    }
                    Ok(None) => {}
                    Err(e) => self.set_banner(e, true),
                },
            }
        }
    }

    fn import_dropped_or_picked_paths(
        &mut self,
        paths: &[std::path::PathBuf],
        ctx: &egui::Context,
    ) {
        if paths.is_empty() {
            return;
        }
        let mut added = Vec::new();
        for p in paths {
            if !p.is_file() {
                continue;
            }
            let Some(fname) = p.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(content) = std::fs::read_to_string(p) else {
                self.set_banner(format!("Dosya okunamadi: {}", p.display()), true);
                continue;
            };
            match import_theme_file(
                &self.paths.themes_dir,
                &self.paths.cache_dir,
                fname,
                &content,
            ) {
                Ok(saved) => {
                    self.preview_loader.invalidate_one(&saved);
                    added.push(saved);
                }
                Err(e) => self.set_banner(e, true),
            }
        }
        if !added.is_empty() {
            if added.len() == 1 {
                let _ = literofi_modes::theme_picker::apply_theme_to_config(
                    &self.paths.config_rasi,
                    &added[0],
                );
                self.active_theme = added[0].clone();
            }
            self.themes_state.show_import_modal = false;
            self.tab = StudioTab::Themes;
            self.themes_state.category = "All".to_string();
            self.prefs.sort = backend::SortOrder::Newest;
            self.prefs.save(&self.paths.prefs_file);
            self.reload_themes_async(ctx);
            let msg = if added.len() == 1 {
                format!(
                    "{}: {}",
                    pick(
                        self.prefs.lang,
                        "Tema eklendi ve uygulandı",
                        "Theme added & applied"
                    ),
                    added[0]
                )
            } else {
                format!(
                    "{} {}",
                    added.len(),
                    pick(self.prefs.lang, "tema içe aktarıldı", "themes imported")
                )
            };
            self.set_banner(msg, false);
        }
    }

    fn draw_literofi_emblem(painter: &egui::Painter, rect: Rect) {
        let x0 = rect.left();
        let y0 = rect.top();
        let w = rect.width();
        let h = rect.height();
        painter.rect_filled(rect, Rounding::ZERO, PAPER);
        painter.rect_stroke(rect, Rounding::ZERO, Stroke::new(1.0_f32, OUTER_BORDER));
        // Sharp top Rofi prompt bar
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x0 + w * 0.17, y0 + h * 0.18),
                Pos2::new(x0 + w * 0.83, y0 + h * 0.36),
            ),
            Rounding::ZERO,
            ACCENT,
        );
        // Bottom theme list rows
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x0 + w * 0.17, y0 + h * 0.46),
                Pos2::new(x0 + w * 0.62, y0 + h * 0.59),
            ),
            Rounding::ZERO,
            INK,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x0 + w * 0.17, y0 + h * 0.67),
                Pos2::new(x0 + w * 0.48, y0 + h * 0.80),
            ),
            Rounding::ZERO,
            MUTED,
        );
        // Corner color swatch
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(x0 + w * 0.68, y0 + h * 0.55),
                Pos2::new(x0 + w * 0.83, y0 + h * 0.80),
            ),
            Rounding::ZERO,
            OK,
        );
    }

    fn handle_window_edge_resize(ctx: &egui::Context) {
        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if is_max {
            return;
        }
        let screen = ctx.screen_rect();
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let border = 5.0;
        let left = pos.x <= screen.left() + border;
        let right = pos.x >= screen.right() - border;
        let top = pos.y <= screen.top() + border;
        let bottom = pos.y >= screen.bottom() - border;

        let dir = match (left, right, top, bottom) {
            (true, false, true, false) => Some(ResizeDirection::NorthWest),
            (false, true, true, false) => Some(ResizeDirection::NorthEast),
            (true, false, false, true) => Some(ResizeDirection::SouthWest),
            (false, true, false, true) => Some(ResizeDirection::SouthEast),
            (true, false, false, false) => Some(ResizeDirection::West),
            (false, true, false, false) => Some(ResizeDirection::East),
            (false, false, true, false) => Some(ResizeDirection::North),
            (false, false, false, true) => Some(ResizeDirection::South),
            _ => None,
        };

        if let Some(d) = dir {
            let cursor = match d {
                ResizeDirection::North | ResizeDirection::South => egui::CursorIcon::ResizeVertical,
                ResizeDirection::East | ResizeDirection::West => egui::CursorIcon::ResizeHorizontal,
                ResizeDirection::NorthWest | ResizeDirection::SouthEast => {
                    egui::CursorIcon::ResizeNwSe
                }
                ResizeDirection::NorthEast | ResizeDirection::SouthWest => {
                    egui::CursorIcon::ResizeNeSw
                }
            };
            ctx.set_cursor_icon(cursor);
            if ctx.input(|i| i.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(d));
            }
        }
    }

    pub fn draw_ui(&mut self, ctx: &egui::Context) {
        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let lang = self.prefs.lang;

        // Global shortcuts: Ctrl+K -> focus search, Esc -> close modal
        let ctrl_k = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::K));
        if ctrl_k {
            self.tab = StudioTab::Themes;
            self.themes_state.focus_search_requested = true;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) && self.themes_state.show_import_modal {
            self.themes_state.show_import_modal = false;
        }

        // Native drag & drop handling
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            let paths: Vec<std::path::PathBuf> =
                dropped.into_iter().filter_map(|df| df.path).collect();
            self.import_dropped_or_picked_paths(&paths, ctx);
        }

        // Frameless eldertuner-rs top navigation bar
        egui::TopBottomPanel::top("top_nav")
            .frame(
                egui::Frame::none()
                    .fill(PAPER)
                    .stroke(Stroke::new(1.0_f32, RULE_2))
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0)),
            )
            .show(ctx, |ui| {
                let bar_resp = ui.interact(
                    ui.max_rect(),
                    egui::Id::new("frameless_titlebar_drag"),
                    Sense::click_and_drag(),
                );
                if bar_resp.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if bar_resp.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                }

                ui.horizontal(|ui| {
                    let (mark_rect, _) =
                        ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::hover());
                    Self::draw_literofi_emblem(ui.painter(), mark_rect);
                    ui.label(RichText::new("> LiteRofi Studio").size(13.5).color(INK));
                    ui.add_space(10.0);

                    if self.prefs.nav_layout == NavLayout::Top {
                        for &(tab, tr_lbl, en_lbl) in StudioTab::ALL {
                            let selected = self.tab == tab;
                            let lbl = if lang == Lang::Tr { tr_lbl } else { en_lbl };
                            let btn = egui::Button::new(
                                RichText::new(lbl).size(12.0).color(if selected {
                                    INK
                                } else {
                                    MUTED
                                }),
                            )
                            .fill(if selected {
                                BG_SEL
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .stroke(if selected {
                                Stroke::new(1.0_f32, RULE)
                            } else {
                                Stroke::NONE
                            })
                            .rounding(Rounding::ZERO);
                            let resp = ui.add(btn);
                            if selected {
                                let r = resp.rect;
                                ui.painter().line_segment(
                                    [
                                        Pos2::new(r.left(), r.bottom()),
                                        Pos2::new(r.right(), r.bottom()),
                                    ],
                                    Stroke::new(2.0_f32, ACCENT),
                                );
                            }
                            if resp.clicked() {
                                self.tab = tab;
                            }
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;

                        // Close button
                        let close_btn =
                            egui::Button::new(RichText::new("×").size(14.0).color(INK_2))
                                .min_size(Vec2::new(26.0, 22.0))
                                .fill(PAPER_2)
                                .stroke(Stroke::new(1.0_f32, RULE))
                                .rounding(Rounding::ZERO);
                        let close_resp = ui.add(close_btn);
                        if close_resp.hovered() {
                            ui.painter()
                                .rect_filled(close_resp.rect, Rounding::ZERO, WARN);
                            ui.painter().text(
                                close_resp.rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "×",
                                egui::FontId::monospace(14.0),
                                ACCENT_INK,
                            );
                        }
                        if close_resp.clicked() {
                            self.live_proc.close();
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }

                        // Maximize / Restore
                        let max_lbl = if is_max { "❐" } else { "□" };
                        let max_btn =
                            egui::Button::new(RichText::new(max_lbl).size(11.5).color(INK_2))
                                .min_size(Vec2::new(26.0, 22.0))
                                .fill(PAPER_2)
                                .stroke(Stroke::new(1.0_f32, RULE))
                                .rounding(Rounding::ZERO);
                        if ui.add(max_btn).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                        }

                        // Minimize
                        let min_btn = egui::Button::new(RichText::new("—").size(11.0).color(INK_2))
                            .min_size(Vec2::new(26.0, 22.0))
                            .fill(PAPER_2)
                            .stroke(Stroke::new(1.0_f32, RULE))
                            .rounding(Rounding::ZERO);
                        if ui.add(min_btn).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }

                        // Layout toggle button (Top / Left)
                        let layout_lbl = match self.prefs.nav_layout {
                            NavLayout::Top => pick(lang, "☰ Sol", "☰ Side"),
                            NavLayout::Left => pick(lang, "▤ Üst", "▤ Top"),
                        };
                        let layout_btn = egui::Button::new(
                            RichText::new(layout_lbl)
                                .size(11.0)
                                .monospace()
                                .color(INK_2),
                        )
                        .min_size(Vec2::new(48.0, 22.0))
                        .fill(PAPER_2)
                        .stroke(Stroke::new(1.0_f32, RULE))
                        .rounding(Rounding::ZERO);
                        let layout_resp = ui.add(layout_btn);
                        if layout_resp.clicked() {
                            self.prefs.nav_layout = self.prefs.nav_layout.toggled();
                            self.prefs.save(&self.paths.prefs_file);
                        }
                        layout_resp.on_hover_text(pick(
                            lang,
                            "Sekmeleri sol kenar çubuğu / üst bar arasında değiştir",
                            "Toggle tabs between top bar and left sidebar",
                        ));

                        // Language toggle button
                        let lang_btn = egui::Button::new(
                            RichText::new(lang.label())
                                .size(11.0)
                                .monospace()
                                .color(ACCENT),
                        )
                        .min_size(Vec2::new(30.0, 22.0))
                        .fill(PAPER_2)
                        .stroke(Stroke::new(1.0_f32, RULE))
                        .rounding(Rounding::ZERO);
                        if ui.add(lang_btn).clicked() {
                            self.prefs.lang = self.prefs.lang.toggled();
                            self.prefs.save(&self.paths.prefs_file);
                        }

                        ui.add_space(4.0);

                        // Prerender or active theme status chip
                        let (pr_run, pr_done, pr_total, _) =
                            self.preview_loader.prerender.snapshot();
                        let status_text = if pr_run {
                            format!("Render {pr_done}/{pr_total}")
                        } else if !self.active_theme.is_empty() {
                            self.active_theme.clone()
                        } else {
                            format!("{} {}", self.themes.len(), pick(lang, "tema", "themes"))
                        };
                        let box_btn = egui::Button::new(
                            RichText::new(status_text).size(11.0).monospace().color(
                                if pr_run || !self.active_theme.is_empty() {
                                    INK
                                } else {
                                    MUTED
                                },
                            ),
                        )
                        .fill(if pr_run { SIGNAL_BLUE_SOFT } else { PAPER_2 })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if pr_run { SIGNAL_BLUE } else { RULE },
                        ))
                        .rounding(Rounding::ZERO);
                        if ui.add(box_btn).clicked() {
                            self.tab = StudioTab::Themes;
                        }
                    });
                });
            });

        // Left navigation sidebar when NavLayout::Left is active
        if self.prefs.nav_layout == NavLayout::Left {
            egui::SidePanel::left("left_nav")
                .resizable(false)
                .exact_width(196.0)
                .frame(
                    egui::Frame::none()
                        .fill(PAPER_2)
                        .stroke(Stroke::new(1.0_f32, RULE))
                        .inner_margin(egui::Margin::symmetric(10.0, 12.0)),
                )
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(pick(lang, "GEZİNTİ", "NAVIGATION"))
                            .size(10.5)
                            .monospace()
                            .color(MUTED),
                    );
                    ui.add_space(4.0);
                    ui_widgets::hairline_rule(ui, RULE);
                    ui.add_space(6.0);

                    for &(tab, tr_lbl, en_lbl) in StudioTab::ALL {
                        let selected = self.tab == tab;
                        let lbl = if lang == Lang::Tr { tr_lbl } else { en_lbl };
                        let count_str = match tab {
                            StudioTab::Themes => Some(format!("{}", self.themes.len())),
                            StudioTab::Palettes => Some(format!("{}", self.palettes.len())),
                            _ => None,
                        };
                        let w = ui.available_width();
                        let (row_rect, resp) =
                            ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
                        let fill = if selected {
                            BG_SEL
                        } else if resp.hovered() {
                            PAPER_3
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        let p = ui.painter();
                        p.rect_filled(row_rect, Rounding::ZERO, fill);
                        if selected {
                            p.rect_stroke(row_rect, Rounding::ZERO, Stroke::new(1.0_f32, RULE));
                            p.line_segment(
                                [
                                    Pos2::new(row_rect.left(), row_rect.top()),
                                    Pos2::new(row_rect.left(), row_rect.bottom()),
                                ],
                                Stroke::new(2.0_f32, ACCENT),
                            );
                        } else if resp.hovered() {
                            p.rect_stroke(row_rect, Rounding::ZERO, Stroke::new(1.0_f32, RULE_2));
                        }
                        p.text(
                            Pos2::new(row_rect.left() + 10.0, row_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            lbl,
                            egui::FontId::proportional(12.5),
                            if selected {
                                INK
                            } else if resp.hovered() {
                                INK_2
                            } else {
                                MUTED
                            },
                        );
                        if let Some(cnt) = count_str {
                            p.text(
                                Pos2::new(row_rect.right() - 8.0, row_rect.center().y),
                                egui::Align2::RIGHT_CENTER,
                                cnt,
                                egui::FontId::monospace(10.5),
                                if selected { ACCENT } else { MUTED_2 },
                            );
                        }
                        if resp.clicked() {
                            self.tab = tab;
                        }
                        ui.add_space(3.0);
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        let flip_lbl = pick(lang, "▤ Üst Çubuğa Al", "▤ Move to Top Bar");
                        if ui_widgets::secondary_button(ui, flip_lbl).clicked() {
                            self.prefs.nav_layout = NavLayout::Top;
                            self.prefs.save(&self.paths.prefs_file);
                        }
                        ui.add_space(6.0);
                        egui::Frame::none()
                            .fill(PAPER_3)
                            .stroke(Stroke::new(1.0_f32, RULE))
                            .rounding(Rounding::ZERO)
                            .inner_margin(egui::Margin::same(9.0))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(
                                    RichText::new(pick(lang, "AKTİF TEMA", "ACTIVE THEME"))
                                        .size(10.0)
                                        .monospace()
                                        .color(MUTED),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(if self.active_theme.is_empty() {
                                            "-"
                                        } else {
                                            self.active_theme.as_str()
                                        })
                                        .size(11.5)
                                        .color(INK),
                                    )
                                    .truncate(),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(pick(lang, "AKTİF PALET", "ACTIVE PALETTE"))
                                        .size(10.0)
                                        .monospace()
                                        .color(MUTED),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(if self.active_palette.is_empty() {
                                            "-"
                                        } else {
                                            self.active_palette.as_str()
                                        })
                                        .size(11.5)
                                        .color(INK_2),
                                    )
                                    .truncate(),
                                );
                            });
                    });
                });
        }

        // Bottom hairline notification strip
        if let Some((msg, is_err, ts)) = &self.banner {
            if ts.elapsed() < Duration::from_secs(5) {
                let is_err = *is_err;
                let msg = msg.clone();
                egui::TopBottomPanel::bottom("status_banner")
                    .frame(
                        egui::Frame::none()
                            .fill(PAPER_2)
                            .stroke(Stroke::new(1.0_f32, if is_err { WARN } else { RULE }))
                            .inner_margin(egui::Margin::symmetric(16.0, 7.0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(msg).size(12.0).color(if is_err {
                                WARN
                            } else {
                                OK
                            }));
                        });
                    });
            } else {
                self.banner = None;
            }
        }

        // Main workspace
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(PAPER)
                    .inner_margin(egui::Margin::symmetric(18.0, 14.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    StudioTab::Themes => {
                        let act = view_themes::render_themes_view(
                            ui,
                            &mut self.themes_state,
                            &mut self.prefs,
                            view_themes::ThemesContext {
                                themes: &self.themes,
                                active_theme: &self.active_theme,
                                active_palette: &self.active_palette,
                                loading_themes: self.loading_themes,
                            },
                            &mut self.preview_loader,
                        );
                        if act.prefs_changed {
                            self.prefs.save(&self.paths.prefs_file);
                        }
                        if let Some(name) = act.apply_theme {
                            match literofi_modes::theme_picker::apply_theme_to_config(
                                &self.paths.config_rasi,
                                &name,
                            ) {
                                Ok(()) => {
                                    self.active_theme = name.clone();
                                    self.set_banner(
                                        format!(
                                            "{}: {name}",
                                            pick(lang, "Tema uygulandı", "Theme applied")
                                        ),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                        if let Some(name) = act.live_preview {
                            match self.live_proc.launch_theme(&self.paths, &name) {
                                Ok(()) => self.set_banner(
                                    format!(
                                        "{}: {name}",
                                        pick(lang, "Canlı önizleme açıldı", "Live preview opened")
                                    ),
                                    false,
                                ),
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                        if let Some(name) = act.toggle_favorite {
                            let mut favs: HashSet<String> =
                                load_favorites(&self.paths.favorites_file);
                            let now_fav = if favs.contains(&name) {
                                favs.remove(&name);
                                false
                            } else {
                                favs.insert(name.clone());
                                true
                            };
                            save_favorites(&self.paths.favorites_file, &favs);
                            for t in &mut self.themes {
                                if t.name == name {
                                    t.is_favorite = now_fav;
                                }
                            }
                        }
                        if let Some(name) = act.edit_theme {
                            self.editor_state.load_theme_from_disk(&self.paths, &name);
                            self.tab = StudioTab::Editor;
                        }
                        if let Some(name) = act.reveal_theme {
                            if let Err(e) = reveal_theme_in_explorer(&self.paths, &name) {
                                self.set_banner(e, true);
                            }
                        }
                        if act.open_themes_dir {
                            if let Err(e) = open_themes_folder(&self.paths) {
                                self.set_banner(e, true);
                            }
                        }
                        if act.clear_cache {
                            let removed = clear_preview_cache(&self.paths.cache_dir);
                            self.preview_loader.clear_all();
                            for t in &mut self.themes {
                                t.has_preview = false;
                            }
                            self.set_banner(
                                format!(
                                    "{removed} {}",
                                    pick(
                                        lang,
                                        "önbellek dosyası temizlendi",
                                        "cached previews cleared"
                                    )
                                ),
                                false,
                            );
                        }
                        if act.prerender_missing {
                            let targets: Vec<String> = self
                                .themes
                                .iter()
                                .filter(|t| !t.has_preview)
                                .map(|t| t.name.clone())
                                .collect();
                            let count = targets.len();
                            if count == 0 {
                                self.set_banner(
                                    pick(
                                        lang,
                                        "Tüm önizlemeler zaten hazır",
                                        "All previews already cached",
                                    ),
                                    false,
                                );
                            } else if self
                                .preview_loader
                                .start_batch_prerender(targets, false, ctx)
                            {
                                self.set_banner(
                                    format!(
                                        "{count} {}",
                                        pick(
                                            lang,
                                            "tema önizlemesi hazırlanıyor...",
                                            "theme previews queued..."
                                        )
                                    ),
                                    false,
                                );
                            }
                        }
                        if act.pick_rasi_files {
                            if let Some(tx) = self.bg_tx.clone() {
                                let c = ctx.clone();
                                std::thread::spawn(move || {
                                    if let Ok(files) = choose_rasi_files() {
                                        let _ = tx.send(BgEvent::FilesPicked(files));
                                        c.request_repaint();
                                    }
                                });
                            }
                        }
                        if act.pick_theme_folder {
                            if let Some(tx) = self.bg_tx.clone() {
                                let themes_dir = self.paths.themes_dir.clone();
                                let c = ctx.clone();
                                std::thread::spawn(move || {
                                    let res = choose_theme_folder(&themes_dir);
                                    let _ = tx.send(BgEvent::FolderPicked(res));
                                    c.request_repaint();
                                });
                            }
                        }
                        if let Some((raw_name, code)) = act.save_paste_theme {
                            match import_theme_file(
                                &self.paths.themes_dir,
                                &self.paths.cache_dir,
                                &raw_name,
                                &code,
                            ) {
                                Ok(saved) => {
                                    self.preview_loader.invalidate_one(&saved);
                                    let _ = literofi_modes::theme_picker::apply_theme_to_config(
                                        &self.paths.config_rasi,
                                        &saved,
                                    );
                                    self.active_theme = saved.clone();
                                    self.themes_state.paste_name.clear();
                                    self.themes_state.paste_code.clear();
                                    self.themes_state.show_import_modal = false;
                                    self.reload_themes_async(ctx);
                                    self.set_banner(
                                        format!(
                                            "{}: {saved}",
                                            pick(
                                                lang,
                                                "Tema kaydedildi ve uygulandı",
                                                "Theme saved & applied"
                                            )
                                        ),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                    }
                    StudioTab::Palettes => {
                        let act = view_palettes::render_palettes_view(
                            ui,
                            &mut self.palettes_state,
                            &mut self.prefs,
                            &self.palettes,
                            &self.active_palette,
                        );
                        if act.prefs_changed {
                            self.prefs.save(&self.paths.prefs_file);
                        }
                        if let Some(pal) = act.apply_palette {
                            match apply_palette(&self.paths, &pal) {
                                Ok(()) => {
                                    self.active_palette = pal.name.clone();
                                    self.set_banner(
                                        format!(
                                            "{}: {}",
                                            pick(lang, "Palet uygulandı", "Palette applied"),
                                            pal.name
                                        ),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                        if let Some(draft) = act.save_custom_palette {
                            match save_custom_palette(&self.paths.colors_dir, &draft) {
                                Ok(saved) => {
                                    let _ = apply_palette(&self.paths, &saved);
                                    self.active_palette = saved.name.clone();
                                    self.palettes = list_palettes(&self.paths.colors_dir);
                                    self.palettes_state.show_creator = false;
                                    self.set_banner(
                                        format!(
                                            "{}: {}",
                                            pick(
                                                lang,
                                                "Özel palet kaydedildi",
                                                "Custom palette saved"
                                            ),
                                            saved.name
                                        ),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                    }
                    StudioTab::Widgets => {
                        let cfg_path_str = self.paths.widgets_toml.to_string_lossy().into_owned();
                        let act = view_widgets::render_widgets_view(
                            ui,
                            &mut self.widgets_state,
                            &mut self.widgets_cfg,
                            &cfg_path_str,
                            &self.installed_fonts,
                            lang,
                        );
                        if act.save_config {
                            match self.widgets_cfg.save_to(&self.paths.widgets_toml) {
                                Ok(()) => {
                                    self.widgets_state.dirty = false;
                                    self.widgets_state.raw_toml_buf = self.widgets_cfg.to_toml();
                                    self.set_banner(
                                        pick(
                                            lang,
                                            "Widget ayarları kaydedildi",
                                            "Widgets config saved",
                                        ),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                        if let Some(toml_txt) = act.save_raw_toml {
                            let parsed = WidgetsConfig::parse_toml(&toml_txt);
                            match parsed.save_to(&self.paths.widgets_toml) {
                                Ok(()) => {
                                    self.widgets_cfg = parsed;
                                    self.widgets_state.dirty = false;
                                    self.widgets_state.raw_toml_buf = self.widgets_cfg.to_toml();
                                    self.set_banner(
                                        pick(lang, "Ham TOML kaydedildi", "Raw TOML saved"),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                        if act.reset_defaults {
                            let def = WidgetsConfig::default();
                            match def.save_to(&self.paths.widgets_toml) {
                                Ok(()) => {
                                    self.widgets_cfg = def;
                                    self.widgets_state.dirty = false;
                                    self.widgets_state.raw_toml_buf = self.widgets_cfg.to_toml();
                                    self.set_banner(
                                        pick(
                                            lang,
                                            "Varsayılanlara sıfırlandı",
                                            "Reset to defaults",
                                        ),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                        if act.open_toml_file {
                            if let Err(e) = open_widgets_config(&self.paths) {
                                self.set_banner(e, true);
                            }
                        }
                        if let Some(mode) = act.preview_mode {
                            let _ = self.widgets_cfg.save_to(&self.paths.widgets_toml);
                            self.widgets_state.dirty = false;
                            match self.live_proc.launch_widget_mode(&mode) {
                                Ok(()) => self.set_banner(
                                    format!(
                                        "{}: {mode}",
                                        pick(
                                            lang,
                                            "Widget önizlemesi açıldı",
                                            "Widget preview opened"
                                        )
                                    ),
                                    false,
                                ),
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                    }
                    StudioTab::Editor => {
                        let act = view_editor::render_editor_view(
                            ui,
                            &mut self.editor_state,
                            &self.paths,
                            &self.themes,
                            &mut self.preview_loader,
                            lang,
                        );
                        if let Some((raw_name, code)) = act.save_and_preview {
                            match import_theme_file(
                                &self.paths.themes_dir,
                                &self.paths.cache_dir,
                                &raw_name,
                                &code,
                            ) {
                                Ok(saved) => {
                                    self.editor_state.selected_theme = saved.clone();
                                    self.editor_state.draft_name = saved.clone();
                                    self.editor_state.dirty = false;
                                    self.editor_state.revalidate(&self.paths);
                                    self.preview_loader.force_rerender_async(saved.clone(), ctx);
                                    self.reload_themes_async(ctx);
                                    self.set_banner(
                                        format!(
                                            "{}: {saved}",
                                            pick(lang, "Tema kaydedildi", "Theme saved")
                                        ),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                        if let Some(name) = act.apply_theme {
                            match literofi_modes::theme_picker::apply_theme_to_config(
                                &self.paths.config_rasi,
                                &name,
                            ) {
                                Ok(()) => {
                                    self.active_theme = name.clone();
                                    self.set_banner(
                                        format!(
                                            "{}: {name}",
                                            pick(lang, "Tema uygulandı", "Theme applied")
                                        ),
                                        false,
                                    );
                                }
                                Err(e) => self.set_banner(e, true),
                            }
                        }
                        if let Some(name) = act.live_preview {
                            if let Err(e) = self.live_proc.launch_theme(&self.paths, &name) {
                                self.set_banner(e, true);
                            }
                        }
                        if let Some(name) = act.reveal_theme {
                            if let Err(e) = reveal_theme_in_explorer(&self.paths, &name) {
                                self.set_banner(e, true);
                            }
                        }
                        if let Some(name) = act.rerender_theme {
                            self.preview_loader.force_rerender_async(name, ctx);
                        }
                    }
                    StudioTab::Settings => {
                        let act =
                            view_settings::render_settings_view(ui, &mut self.prefs, &self.paths);
                        if act.prefs_changed {
                            self.prefs.save(&self.paths.prefs_file);
                        }
                        if act.open_themes_dir {
                            if let Err(e) = open_themes_folder(&self.paths) {
                                self.set_banner(e, true);
                            }
                        }
                        if act.open_widgets_toml {
                            if let Err(e) = open_widgets_config(&self.paths) {
                                self.set_banner(e, true);
                            }
                        }
                        if act.clear_cache {
                            let removed = clear_preview_cache(&self.paths.cache_dir);
                            self.preview_loader.clear_all();
                            for t in &mut self.themes {
                                t.has_preview = false;
                            }
                            self.set_banner(
                                format!(
                                    "{removed} {}",
                                    pick(
                                        self.prefs.lang,
                                        "önbellek dosyası temizlendi",
                                        "cached previews cleared"
                                    )
                                ),
                                false,
                            );
                        }
                    }
                });
            });

        // Native drag & drop hover indicator
        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let fg = ctx.layer_painter(LayerId::new(
                Order::Foreground,
                egui::Id::new("studio_drop_hint"),
            ));
            let sr = ctx.screen_rect();
            fg.rect_filled(
                sr,
                Rounding::ZERO,
                egui::Color32::from_rgba_unmultiplied(8, 7, 6, 210),
            );
            fg.rect_stroke(
                sr.shrink(14.0),
                Rounding::ZERO,
                Stroke::new(2.0_f32, ACCENT),
            );
            fg.text(
                sr.center(),
                egui::Align2::CENTER_CENTER,
                pick(
                    lang,
                    ".rasi tema dosyasını içe aktarmak için bırakın",
                    "Drop .rasi theme file to import into LiteRofi",
                ),
                egui::FontId::monospace(16.0),
                INK,
            );
        }

        // 1px crisp outer border (#a89984) matching eldertuner-rs / mango-minimal.rasi
        let fg = ctx.layer_painter(LayerId::new(
            Order::Foreground,
            egui::Id::new("mango_outer_border"),
        ));
        fg.rect_stroke(
            ctx.screen_rect().shrink(0.5),
            Rounding::ZERO,
            Stroke::new(1.0_f32, OUTER_BORDER),
        );
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.chrome_applied {
            if let Some(hwnd) = apply_dwm_window_chrome("LiteRofi Theme Studio") {
                self.chrome_applied = true;
                self.main_hwnd = Some(hwnd);
                bring_to_foreground(hwnd);
            }
        }

        let is_minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if !is_minimized && !self.initial_size_applied {
            let sr = ctx.screen_rect();
            if sr.width() >= 900.0 && sr.height() >= 560.0 {
                self.initial_size_applied = true;
            } else if sr.width() > 0.0 && sr.height() > 0.0 && self.size_fix_attempts < 6 {
                self.size_fix_attempts += 1;
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(1220.0, 780.0)));
                if let Some(hwnd) = self.main_hwnd {
                    ensure_window_min_size_centered(hwnd, 920, 580, 1220, 780);
                }
            }
        }

        self.poll_background(ctx);
        if !is_minimized {
            Self::handle_window_edge_resize(ctx);
            self.draw_ui(ctx);
        }

        if self.loading_themes || self.preview_loader.has_pending() {
            ctx.request_repaint_after(Duration::from_millis(80));
        } else if self.banner.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

impl Drop for StudioApp {
    fn drop(&mut self) {
        self.live_proc.close();
    }
}

fn configure_windows_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let jbm_candidates = [
        r"C:\Windows\Fonts\JetBrainsMonoNerdFont-Regular.ttf",
        r"C:\Windows\Fonts\JetBrainsMonoNerdFontMono-Regular.ttf",
        r"C:\Windows\Fonts\JetBrainsMono-Regular.ttf",
        r"C:\Windows\Fonts\CascadiaMono.ttf",
        r"C:\Windows\Fonts\consola.ttf",
    ];
    for path in jbm_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("jbm_nf".to_string(), egui::FontData::from_owned(bytes));
            if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                fam.insert(0, "jbm_nf".to_string());
            }
            if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                fam.insert(0, "jbm_nf".to_string());
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn load_app_icon() -> Option<egui::IconData> {
    let ico_bytes = include_bytes!("../../../studio/studio.ico");
    let img = image::load_from_memory(ico_bytes).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result<()> {
    if !try_acquire_single_instance("LiteRofi Theme Studio") {
        return Ok(());
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_inner_size([1220.0, 780.0])
        .with_min_inner_size([920.0, 580.0])
        .with_title("LiteRofi Theme Studio");
    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        persist_window: false,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "LiteRofi Theme Studio",
        options,
        Box::new(|cc| Ok(Box::new(StudioApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_icon_and_fonts_load_cleanly() {
        let icon = load_app_icon().expect("studio.ico should decode");
        assert!(icon.width >= 16 && icon.height >= 16);
    }

    #[test]
    fn all_four_tabs_render_without_panic_and_fit_width() {
        let mut app = StudioApp::new_headless();
        let ctx = egui::Context::default();
        configure_windows_fonts(&ctx);
        apply_rim_theme(&ctx);

        for layout in [NavLayout::Top, NavLayout::Left] {
            app.prefs.nav_layout = layout;
            for size in [egui::vec2(1220.0, 780.0), egui::vec2(920.0, 580.0)] {
                for &(tab, _, _) in StudioTab::ALL {
                    app.tab = tab;
                    let raw = egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    };
                    let mut used = Rect::NOTHING;
                    let _ = ctx.run(raw, |ctx| {
                        app.draw_ui(ctx);
                        used = ctx.used_rect();
                    });
                    if used.is_finite() {
                        assert!(
                            used.right() <= size.x + 2.0,
                            "tab {:?} ({:?}) overflowed width at {:?}: used_right={} > {}",
                            tab,
                            layout,
                            size,
                            used.right(),
                            size.x
                        );
                    }
                }
            }
        }
    }
}
