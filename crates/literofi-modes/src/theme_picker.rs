//! Interactive theme selector mode (`-show theme` / `-show themes`):
//! scans available `.rasi` themes and updates `@theme "..."` in `config.rasi` cleanly.

use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct ThemeCandidate {
    name: String,
    path: PathBuf,
}

/// Interactive `.rasi` theme switcher that supports live preview (`Enter`) and
/// persistent saving (`Alt+A` / `Shift+Enter`) into `config.rasi`.
pub struct ThemeMode {
    display: String,
    search_dirs: Vec<PathBuf>,
    config_file: PathBuf,
    items: Vec<ThemeCandidate>,
    live_preview: bool,
    previewed: Option<String>,
}

impl ThemeMode {
    pub fn new(cfg: &Config) -> Self {
        let mut search_dirs = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                search_dirs.push(PathBuf::from(&appdata).join("LiteRofi").join("themes"));
                search_dirs.push(PathBuf::from(&appdata).join("rofi").join("themes"));
            }
        }
        let home = crate::user_home_dir();
        search_dirs.push(home.join(".config").join("rofi").join("themes"));
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                search_dirs.push(parent.join("themes"));
            }
        }

        let config_file = cfg
            .config_file
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(default_config_rasi_path);

        Self {
            display: "themes".to_string(),
            search_dirs,
            config_file,
            items: Vec::new(),
            live_preview: true,
            previewed: None,
        }
    }

    /// Creates a `ThemeMode` scoped to custom theme directories and config path (direct-apply mode for tests).
    pub fn with_paths(search_dirs: Vec<PathBuf>, config_file: PathBuf) -> Self {
        Self {
            display: "themes".to_string(),
            search_dirs,
            config_file,
            items: Vec::new(),
            live_preview: false,
            previewed: None,
        }
    }

    /// Creates a `ThemeMode` with custom paths and live preview enabled (`Enter` previews, `Alt+A`/`Shift+Enter` saves).
    pub fn with_paths_preview(search_dirs: Vec<PathBuf>, config_file: PathBuf) -> Self {
        Self {
            live_preview: true,
            ..Self::with_paths(search_dirs, config_file)
        }
    }
}

fn default_config_rasi_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return PathBuf::from(appdata).join("LiteRofi").join("config.rasi");
        }
    }
    PathBuf::from("config.rasi")
}

/// Updates or appends `@theme "<theme_name>"` inside `config_path` (BOM-free UTF-8).
pub fn apply_theme_to_config(config_path: &Path, theme_name: &str) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create config directory: {e}"))?;
    }
    let existing = std::fs::read_to_string(config_path).unwrap_or_else(|_| {
        "configuration {\n    modes: \"drun,run,window,combi,filebrowser,keys\";\n    show-icons: true;\n    font: \"Segoe UI 11\";\n    drun-display-format: \"{name}\";\n    daemon-hotkey: \"Alt+Space\";\n    daemon-mode: \"drun\";\n}\n".to_string()
    });
    let clean = existing.strip_prefix('\u{FEFF}').unwrap_or(&existing);
    let directive = format!("@theme \"{theme_name}\"");

    let mut replaced = false;
    let mut out_lines: Vec<String> = Vec::new();
    for line in clean.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("@theme") && !trimmed.starts_with("/*") && !trimmed.starts_with("//")
        {
            if !replaced {
                out_lines.push(directive.clone());
                replaced = true;
            }
        } else {
            out_lines.push(line.to_string());
        }
    }
    if !replaced {
        if !out_lines.is_empty() && !out_lines.last().is_some_and(|l| l.is_empty()) {
            out_lines.push(String::new());
        }
        out_lines.push(directive);
    }
    let mut final_text = out_lines.join("\n");
    final_text.push('\n');
    std::fs::write(config_path, final_text)
        .map_err(|e| format!("failed to write {}: {e}", config_path.display()))
}

impl RichMode for ThemeMode {
    fn name(&self) -> &str {
        "theme"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        let mut by_key: BTreeMap<String, ThemeCandidate> = BTreeMap::new();
        for dir in &self.search_dirs {
            let Ok(rd) = std::fs::read_dir(dir) else {
                continue;
            };
            for ent in rd.flatten() {
                let path = ent.path();
                if !path.is_file() {
                    continue;
                }
                let is_rasi = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("rasi"));
                if !is_rasi {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let name = stem.to_string();
                let key = name.to_ascii_lowercase();
                by_key
                    .entry(key)
                    .or_insert_with(|| ThemeCandidate { name, path });
            }
        }
        self.items = by_key.into_values().collect();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.items
            .iter()
            .map(|item| ListEntry {
                text: item.name.clone(),
                meta: Some(item.path.to_string_lossy().into_owned()),
                ..Default::default()
            })
            .collect()
    }

    fn message(&self) -> Option<String> {
        if !self.live_preview {
            return Some(
                "Select a theme and press Enter (applies automatically on next Alt+Space)"
                    .to_string(),
            );
        }
        if let Some(name) = &self.previewed {
            Some(format!(
                "Previewing: {name}  |  Enter: Preview  |  Alt+A / Shift+Enter: Save  |  Esc: Revert"
            ))
        } else {
            Some(
                "Enter: Preview theme live  |  Alt+A / Shift+Enter: Save & apply  |  Esc: Close"
                    .to_string(),
            )
        }
    }

    fn reload_hints(&self) -> ReloadHints {
        ReloadHints {
            keep_selection: true,
            new_selection: None,
            keep_filter: true,
        }
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Accepted {
                index, exit_code, ..
            } => {
                if self.live_preview && *exit_code == 0 {
                    if let Some(item) = self.items.get(*index) {
                        self.previewed = Some(item.name.clone());
                        return Ok(ModeStep::PreviewTheme(item.path.clone()));
                    }
                    return Ok(ModeStep::Reload);
                }
                if let Some(item) = self.items.get(*index) {
                    apply_theme_to_config(&self.config_file, &item.name)?;
                }
                self.previewed = None;
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                if let Some(item) = self.items.get(*index) {
                    apply_theme_to_config(&self.config_file, &item.name)?;
                } else if let Some(name) = &self.previewed {
                    apply_theme_to_config(&self.config_file, name)?;
                }
                self.previewed = None;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::Cancelled => {
                self.previewed = None;
                Ok(ModeStep::Exit(1))
            }
            UiOutcome::ModeSwitch(idx) => {
                self.previewed = None;
                Ok(ModeStep::SwitchModeIndex(*idx))
            }
            _ => Ok(ModeStep::Exit(0)),
        }
    }
}
