//! Quick Web Search & Bang Shortcuts mode (`-show search`, inspired by `JaKooLit/RofiSearch`).
//! Supports `!g`, `!yt`, `!gh`, `!w`, `!r`, `!tr`, `!en`, `!maps`, `!npm`, `!crate`, `!ddg`, `!ai`
//! bangs plus direct URL opening and persistent recent-query history.

use crate::active_palette::ActivePalette;
use crate::win32_apps::{launch_windows_app, AppLaunchTarget};
use crate::{resolve_cache_dir, ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::PathBuf;

const SEARCH_HISTORY_FILE: &str = "rofi-search.history";
const MAX_SEARCH_HISTORY: usize = 25;

struct BangEngine {
    bang: &'static str,
    label: &'static str,
    template: &'static str,
}

const BANGS: &[BangEngine] = &[
    BangEngine {
        bang: "!g",
        label: "Google",
        template: "https://www.google.com/search?q={q}",
    },
    BangEngine {
        bang: "!yt",
        label: "YouTube",
        template: "https://www.youtube.com/results?search_query={q}",
    },
    BangEngine {
        bang: "!gh",
        label: "GitHub",
        template: "https://github.com/search?q={q}",
    },
    BangEngine {
        bang: "!w",
        label: "Wikipedia",
        template: "https://en.wikipedia.org/w/index.php?search={q}",
    },
    BangEngine {
        bang: "!r",
        label: "Reddit",
        template: "https://www.reddit.com/search/?q={q}",
    },
    BangEngine {
        bang: "!tr",
        label: "Google Translate (→ TR)",
        template: "https://translate.google.com/?sl=auto&tl=tr&text={q}",
    },
    BangEngine {
        bang: "!en",
        label: "Google Translate (→ EN)",
        template: "https://translate.google.com/?sl=auto&tl=en&text={q}",
    },
    BangEngine {
        bang: "!maps",
        label: "Google Maps",
        template: "https://www.google.com/maps/search/{q}",
    },
    BangEngine {
        bang: "!crate",
        label: "crates.io",
        template: "https://crates.io/search?q={q}",
    },
    BangEngine {
        bang: "!npm",
        label: "npm",
        template: "https://www.npmjs.com/search?q={q}",
    },
    BangEngine {
        bang: "!ddg",
        label: "DuckDuckGo",
        template: "https://duckduckgo.com/?q={q}",
    },
    BangEngine {
        bang: "!ai",
        label: "Perplexity AI",
        template: "https://www.perplexity.ai/search?q={q}",
    },
];

/// Stateful web search launcher mode.
pub struct SearchMode {
    display: String,
    cache_dir: PathBuf,
    dry_run: bool,
    palette: Option<ActivePalette>,
    history: Vec<String>,
    reload_hints: ReloadHints,
}

impl SearchMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "search".to_string(),
            cache_dir: resolve_cache_dir(cfg),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
            history: Vec::new(),
            reload_hints: ReloadHints::default(),
        }
    }

    fn history_path(&self) -> PathBuf {
        self.cache_dir.join(SEARCH_HISTORY_FILE)
    }

    fn load_history(&mut self) {
        self.history = std::fs::read_to_string(self.history_path())
            .ok()
            .map(|s| {
                s.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .take(MAX_SEARCH_HISTORY)
                    .collect()
            })
            .unwrap_or_default();
    }

    fn record_query(&mut self, q: &str) {
        let trimmed = q.trim();
        if trimmed.is_empty() {
            return;
        }
        self.history.retain(|h| !h.eq_ignore_ascii_case(trimmed));
        self.history.insert(0, trimmed.to_string());
        self.history.truncate(MAX_SEARCH_HISTORY);
        if !self.dry_run {
            let path = self.history_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, self.history.join("\n"));
        }
    }

    fn execute_search(&mut self, raw: &str) -> Result<(), String> {
        let url = build_search_url(raw);
        if url.is_empty() {
            return Ok(());
        }
        self.record_query(raw);
        if self.dry_run {
            return Ok(());
        }
        launch_windows_app(&AppLaunchTarget::ShellPath(PathBuf::from(url)))
    }
}

/// Resolves a user query (with optional `!bang` prefix or direct URL) into a target URL.
pub fn build_search_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    if !trimmed.contains(' ')
        && trimmed.contains('.')
        && !trimmed.starts_with('!')
        && trimmed.rsplit('.').next().is_some_and(|tld| tld.len() >= 2)
    {
        return format!("https://{trimmed}");
    }
    if trimmed.starts_with('!') {
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let bang = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        if let Some(eng) = BANGS.iter().find(|b| b.bang.eq_ignore_ascii_case(bang)) {
            if rest.is_empty() {
                return eng
                    .template
                    .split('?')
                    .next()
                    .unwrap_or(eng.template)
                    .to_string();
            }
            return eng.template.replace("{q}", &url_encode(rest));
        }
    }
    format!("https://www.google.com/search?q={}", url_encode(trimmed))
}

pub(crate) fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b' ' => out.push('+'),
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

impl RichMode for SearchMode {
    fn name(&self) -> &str {
        "search"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        self.load_history();
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some(" Web Search".to_string())
    }

    fn message(&self) -> Option<String> {
        Some(
            "Bangs: !g Google · !yt YouTube · !gh GitHub · !w Wiki · !r Reddit · !tr/!en Translate · !ai"
                .to_string(),
        )
    }

    fn no_custom_override(&self) -> Option<bool> {
        Some(false)
    }

    fn theme_snippets(&self) -> Vec<String> {
        vec!["element-icon { enabled: false; }".to_string()]
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn entries(&self) -> Vec<ListEntry> {
        let mut out = Vec::new();
        for h in &self.history {
            out.push(ListEntry {
                text: format!("󰋚  {h}"),
                meta: Some(format!("history {h}")),
                info: Some(format!("hist:{h}")),
                ..ListEntry::default()
            });
        }
        for b in BANGS {
            out.push(ListEntry {
                text: format!("{}   —   {}", b.bang, b.label),
                meta: Some(format!("{} {}", b.bang, b.label)),
                info: Some(format!("bang:{}", b.bang)),
                ..ListEntry::default()
            });
        }
        out
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::CustomInput { text, .. } | UiOutcome::CustomInputAlt { text } => {
                let q = if text.trim().is_empty() {
                    query.trim()
                } else {
                    text.trim()
                };
                self.execute_search(q)?;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::Accepted { index, .. } | UiOutcome::AcceptedAlt { index, .. } => {
                let trimmed = query.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('!') {
                    // If user typed a fresh query and hit Enter while a bang hint matched, search it directly.
                    if *index >= self.history.len() {
                        self.execute_search(trimmed)?;
                        return Ok(ModeStep::Exit(0));
                    }
                }
                if *index < self.history.len() {
                    let q = self.history[*index].clone();
                    self.execute_search(&q)?;
                    return Ok(ModeStep::Exit(0));
                }
                let bang_idx = *index - self.history.len();
                if let Some(b) = BANGS.get(bang_idx) {
                    let q = if !trimmed.is_empty() && !trimmed.starts_with('!') {
                        format!("{} {trimmed}", b.bang)
                    } else if !trimmed.is_empty() {
                        trimmed.to_string()
                    } else {
                        b.bang.to_string()
                    };
                    self.execute_search(&q)?;
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { index } => {
                if *index < self.history.len() {
                    self.history.remove(*index);
                    if !self.dry_run {
                        let _ = std::fs::write(self.history_path(), self.history.join("\n"));
                    }
                }
                self.reload_hints = ReloadHints {
                    keep_selection: true,
                    new_selection: None,
                    keep_filter: false,
                };
                Ok(ModeStep::Reload)
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            UiOutcome::Back => Ok(ModeStep::Reload),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_builds_bang_and_default_urls() {
        assert_eq!(
            build_search_url("rust direct2d"),
            "https://www.google.com/search?q=rust+direct2d"
        );
        assert_eq!(
            build_search_url("!yt lofi beats"),
            "https://www.youtube.com/results?search_query=lofi+beats"
        );
        assert_eq!(
            build_search_url("!gh literofi"),
            "https://github.com/search?q=literofi"
        );
        assert_eq!(build_search_url("github.com"), "https://github.com");
    }
}
