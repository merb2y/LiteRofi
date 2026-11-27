//! Text & Code Snippets Manager mode (`-show snippets`, inspired by `rofi-snippets`).
//! Reads user snippets from `%APPDATA%\LiteRofi\snippets.toml`, fuzzy-searches by name/tags/body,
//! copies selected snippet on Enter, and opens `snippets.toml` on Shift+Enter.

use crate::active_palette::ActivePalette;
use crate::clipboard::write_text_to_clipboard;
use crate::win32_apps::{launch_windows_app, AppLaunchTarget};
use crate::{ModeStep, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetEntry {
    pub name: String,
    pub tags: String,
    pub text: String,
}

const DEFAULT_SNIPPETS_TOML: &str = r#"# LiteRofi Snippets — Edit freely!
# Press Alt+N to open Snippets, Enter to copy, Shift+Enter to open this file.

[[snippet]]
name = "Git Status & Recent Log"
tags = "git dev shell"
text = "git status -sb && git log -n 5 --oneline"

[[snippet]]
name = "Cargo Release Check"
tags = "rust cargo dev"
text = "cargo fmt --all && cargo clippy --workspace -- -D warnings && cargo test --workspace"

[[snippet]]
name = "PowerShell Kill Process"
tags = "windows ps1 admin"
text = "Stop-Process -Name <app> -Force"

[[snippet]]
name = "Markdown Code Block"
tags = "md docs"
text = "```rust\n\n```"

[[snippet]]
name = "ISO Date Format"
tags = "date template"
text = "YYYY-MM-DD"
"#;

/// Stateful text/code snippets mode.
pub struct SnippetsMode {
    display: String,
    dry_run: bool,
    palette: Option<ActivePalette>,
    snippets: Vec<SnippetEntry>,
    file_path: PathBuf,
}

impl SnippetsMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "snippets".to_string(),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
            snippets: Vec::new(),
            file_path: snippets_file_path(),
        }
    }

    fn ensure_and_load(&mut self) {
        if !self.file_path.is_file() && !self.dry_run {
            if let Some(parent) = self.file_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&self.file_path, DEFAULT_SNIPPETS_TOML);
        }
        let raw = std::fs::read_to_string(&self.file_path)
            .unwrap_or_else(|_| DEFAULT_SNIPPETS_TOML.to_string());
        self.snippets = parse_snippets_toml(&raw);
    }

    fn open_snippets_file(&self) -> Result<(), String> {
        if self.dry_run {
            return Ok(());
        }
        if !self.file_path.is_file() {
            if let Some(parent) = self.file_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&self.file_path, DEFAULT_SNIPPETS_TOML);
        }
        launch_windows_app(&AppLaunchTarget::ShellPath(self.file_path.clone()))
    }
}

pub fn snippets_file_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return PathBuf::from(appdata)
                .join("LiteRofi")
                .join("snippets.toml");
        }
    }
    crate::user_home_dir()
        .join(".config")
        .join("literofi")
        .join("snippets.toml")
}

/// Parses `[[snippet]]` tables from `snippets.toml`.
pub fn parse_snippets_toml(raw: &str) -> Vec<SnippetEntry> {
    let mut out = Vec::new();
    let mut cur_name: Option<String> = None;
    let mut cur_tags = String::new();
    let mut cur_text: Option<String> = None;

    let flush = |out: &mut Vec<SnippetEntry>,
                 name: &mut Option<String>,
                 tags: &mut String,
                 text: &mut Option<String>| {
        if let (Some(n), Some(t)) = (name.take(), text.take()) {
            if !n.is_empty() && !t.is_empty() {
                out.push(SnippetEntry {
                    name: n,
                    tags: std::mem::take(tags),
                    text: t,
                });
            }
        }
        tags.clear();
    };

    for raw_line in raw.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[snippet]]" {
            flush(&mut out, &mut cur_name, &mut cur_tags, &mut cur_text);
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = unescape_toml_str(v.trim());
        match key {
            "name" => cur_name = Some(val),
            "tags" => cur_tags = val,
            "text" => cur_text = Some(val),
            _ => {}
        }
    }
    flush(&mut out, &mut cur_name, &mut cur_tags, &mut cur_text);
    out
}

fn unescape_toml_str(raw: &str) -> String {
    let trimmed = raw.trim();
    let inner = if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    if trimmed.starts_with('\'') {
        return inner.to_string();
    }
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn preview_single_line(text: &str) -> String {
    let one = text.replace(['\r', '\n', '\t'], " ");
    let trimmed = one.trim();
    if trimmed.chars().count() > 64 {
        let head: String = trimmed.chars().take(61).collect();
        format!("{head}...")
    } else {
        trimmed.to_string()
    }
}

fn short_path(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("snippets.toml")
        .to_string()
}

impl RichMode for SnippetsMode {
    fn name(&self) -> &str {
        "snippets"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        self.ensure_and_load();
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some("󰅴 Snippets".to_string())
    }

    fn message(&self) -> Option<String> {
        Some(format!(
            "{} snippets ({}) · Enter: Copy · Shift+Enter: Edit file",
            self.snippets.len(),
            short_path(&self.file_path)
        ))
    }

    fn theme_snippets(&self) -> Vec<String> {
        vec!["element-icon { enabled: false; }".to_string()]
    }

    fn entries(&self) -> Vec<ListEntry> {
        let mut out: Vec<ListEntry> = self
            .snippets
            .iter()
            .map(|s| ListEntry {
                text: format!("{}   —   {}", s.name, preview_single_line(&s.text)),
                meta: Some(format!("{} {} {}", s.name, s.tags, s.text)),
                info: Some(s.name.clone()),
                ..ListEntry::default()
            })
            .collect();
        out.push(ListEntry {
            text: "✎  Edit snippets.toml  (add / customize snippets)".to_string(),
            meta: Some("edit config add new snippet".to_string()),
            info: Some("__edit__".to_string()),
            ..ListEntry::default()
        });
        out
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::AcceptedAlt { .. } => {
                self.open_snippets_file()?;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::Accepted { index, .. } => {
                if *index == self.snippets.len() {
                    self.open_snippets_file()?;
                    return Ok(ModeStep::Exit(0));
                }
                if let Some(s) = self.snippets.get(*index) {
                    if !self.dry_run {
                        let _ = write_text_to_clipboard(&s.text);
                    }
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => Ok(ModeStep::Reload),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_toml_parses_defaults_and_escapes() {
        let parsed = parse_snippets_toml(DEFAULT_SNIPPETS_TOML);
        assert!(parsed.len() >= 4);
        assert_eq!(parsed[0].name, "Git Status & Recent Log");
        assert!(parsed.iter().any(|s| s.text.contains('\n')));
    }
}
