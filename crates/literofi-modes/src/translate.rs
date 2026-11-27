//! Quick Translator mode (`-show translate`, inspired by `rofi-translate`).
//! Auto-detects TR ↔ EN (or uses explicit target prefix like `de:hello`, `fr:merhaba`, `en:selam`),
//! translates via Google Translate's lightweight public endpoint using built-in `curl.exe`,
//! displays the result inline, copies it to clipboard, and persists history.

use crate::active_palette::ActivePalette;
use crate::clipboard::write_text_to_clipboard;
use crate::search::url_encode;
use crate::{resolve_cache_dir, ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::PathBuf;
use std::process::Command;

const TRANSLATE_HISTORY_FILE: &str = "rofi-translate.history";
const MAX_TRANSLATE_HISTORY: usize = 30;

/// Stateful quick translator mode.
pub struct TranslateMode {
    display: String,
    cache_dir: PathBuf,
    dry_run: bool,
    palette: Option<ActivePalette>,
    history: Vec<(String, String, String)>, // (pair, source, translated)
    last_status: Option<String>,
    reload_hints: ReloadHints,
}

impl TranslateMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "translate".to_string(),
            cache_dir: resolve_cache_dir(cfg),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
            history: Vec::new(),
            last_status: None,
            reload_hints: ReloadHints::default(),
        }
    }

    fn history_path(&self) -> PathBuf {
        self.cache_dir.join(TRANSLATE_HISTORY_FILE)
    }

    fn load_history(&mut self) {
        self.history.clear();
        let Ok(raw) = std::fs::read_to_string(self.history_path()) else {
            return;
        };
        for line in raw.lines() {
            let mut parts = line.splitn(3, '\t');
            if let (Some(pair), Some(src), Some(dst)) = (parts.next(), parts.next(), parts.next()) {
                if !src.is_empty() && !dst.is_empty() {
                    self.history
                        .push((pair.to_string(), src.to_string(), dst.to_string()));
                }
            }
        }
        self.history.truncate(MAX_TRANSLATE_HISTORY);
    }

    fn save_history(&self) {
        let path = self.history_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut out = String::new();
        for (pair, src, dst) in self.history.iter().take(MAX_TRANSLATE_HISTORY) {
            out.push_str(&pair.replace(['\t', '\n', '\r'], " "));
            out.push('\t');
            out.push_str(&src.replace(['\t', '\n', '\r'], " "));
            out.push('\t');
            out.push_str(&dst.replace(['\t', '\n', '\r'], " "));
            out.push('\n');
        }
        let _ = std::fs::write(path, out);
    }

    fn run_translate(&mut self, raw: &str) -> Result<ModeStep, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(ModeStep::Reload);
        }
        let (sl, tl, body) = parse_translate_request(trimmed);
        if body.is_empty() {
            return Ok(ModeStep::Reload);
        }
        let translated = if self.dry_run {
            format!("[{tl}] {body}")
        } else {
            match fetch_translation(sl, tl, body) {
                Ok(t) => t,
                Err(err) => {
                    self.last_status = Some(format!("Translate error: {err}"));
                    self.reload_hints = ReloadHints {
                        keep_selection: true,
                        new_selection: None,
                        keep_filter: true,
                    };
                    return Ok(ModeStep::Reload);
                }
            }
        };
        if !self.dry_run {
            let _ = write_text_to_clipboard(&translated);
        }
        let pair = format!("{sl}→{tl}").to_ascii_uppercase();
        self.history
            .retain(|(_, s, _)| !s.eq_ignore_ascii_case(body));
        self.history
            .insert(0, (pair.clone(), body.to_string(), translated.clone()));
        self.history.truncate(MAX_TRANSLATE_HISTORY);
        if !self.dry_run {
            self.save_history();
        }
        self.last_status = Some(format!("[{pair}] {translated}   (copied)"));
        self.reload_hints = ReloadHints {
            keep_selection: false,
            new_selection: Some(0),
            keep_filter: false,
        };
        Ok(ModeStep::Reload)
    }
}

/// Parses optional target-language prefix (`de:hello`, `en:merhaba`, `tr:world`) or auto-picks TR↔EN.
pub fn parse_translate_request(raw: &str) -> (&'static str, &'static str, &str) {
    let trimmed = raw.trim();
    if let Some((prefix, rest)) = trimmed.split_once(':') {
        let p = prefix.trim().to_ascii_lowercase();
        let body = rest.trim();
        let tl = match p.as_str() {
            "tr" => Some("tr"),
            "en" => Some("en"),
            "de" => Some("de"),
            "fr" => Some("fr"),
            "es" => Some("es"),
            "it" => Some("it"),
            "ru" => Some("ru"),
            "ja" => Some("ja"),
            "ko" => Some("ko"),
            "ar" => Some("ar"),
            "zh" => Some("zh-CN"),
            _ => None,
        };
        if let Some(target) = tl {
            return ("auto", target, body);
        }
    }
    if looks_turkish(trimmed) {
        ("tr", "en", trimmed)
    } else {
        ("auto", "tr", trimmed)
    }
}

fn looks_turkish(s: &str) -> bool {
    if s.chars().any(|c| {
        matches!(
            c,
            'ç' | 'ğ' | 'ı' | 'ş' | 'ö' | 'ü' | 'Ç' | 'Ğ' | 'İ' | 'Ş' | 'Ö' | 'Ü'
        )
    }) {
        return true;
    }
    let lower = s.to_ascii_lowercase();
    let tr_words = [
        "merhaba", "selam", "nasilsin", "tesekkur", "lutfen", "evet", "hayir", "bugun", "yarin",
        "nedir", "neden", "nasil", "bir", "icin", "ile", "olarak", "cok",
    ];
    lower
        .split_whitespace()
        .any(|w| tr_words.contains(&w.trim_matches(|c: char| !c.is_alphanumeric())))
}

fn fetch_translation(sl: &str, tl: &str, text: &str) -> Result<String, String> {
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&sl={sl}&tl={tl}&dt=t&q={}",
        url_encode(text)
    );
    let mut cmd = Command::new("curl.exe");
    cmd.args(["-sS", "-m", "5", "--fail", &url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd
        .output()
        .map_err(|e| format!("failed to run curl.exe: {e}"))?;
    if !output.status.success() {
        return Err(format!("HTTP error (status {})", output.status));
    }
    let body = String::from_utf8_lossy(&output.stdout);
    parse_google_translate_json(&body)
        .ok_or_else(|| "could not parse translation response".to_string())
}

/// Extracts translated segments from Google Translate `[[["translated","source",...],...],...]` JSON.
pub fn parse_google_translate_json(json: &str) -> Option<String> {
    let bytes = json.as_bytes();
    let mut i = 0;
    // Expect outer `[` then sentences array `[`
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if bytes.get(i) != Some(&b'[') {
        return None;
    }
    i += 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if bytes.get(i) != Some(&b'[') {
        return None;
    }
    i += 1;

    let mut result = String::new();
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b',') {
            i += 1;
        }
        if bytes.get(i) == Some(&b']') {
            break;
        }
        if bytes.get(i) != Some(&b'[') {
            break;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'"') {
            let (seg, next_i) = read_json_string(json, i)?;
            result.push_str(&seg);
            i = next_i;
        }
        // Skip remaining elements of this sentence array
        let mut depth = 1usize;
        let mut in_str = false;
        let mut esc = false;
        while i < bytes.len() && depth > 0 {
            let b = bytes[i];
            if in_str {
                if esc {
                    esc = false;
                } else if b == b'\\' {
                    esc = true;
                } else if b == b'"' {
                    in_str = false;
                }
            } else {
                match b {
                    b'"' => in_str = true,
                    b'[' => depth += 1,
                    b']' => depth -= 1,
                    _ => {}
                }
            }
            i += 1;
        }
    }
    (!result.trim().is_empty()).then_some(result)
}

fn read_json_string(s: &str, start_quote: usize) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.get(start_quote) != Some(&b'"') {
        return None;
    }
    let mut i = start_quote + 1;
    let mut out = String::new();
    while i < bytes.len() {
        let ch = s[i..].chars().next()?;
        i += ch.len_utf8();
        match ch {
            '"' => return Some((out, i)),
            '\\' => {
                let esc = s[i..].chars().next()?;
                i += esc.len_utf8();
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' if i + 4 <= s.len() => {
                        if let Ok(code) = u32::from_str_radix(&s[i..i + 4], 16) {
                            if let Some(uc) = char::from_u32(code) {
                                out.push(uc);
                            }
                        }
                        i += 4;
                    }
                    other => out.push(other),
                }
            }
            other => out.push(other),
        }
    }
    None
}

impl RichMode for TranslateMode {
    fn name(&self) -> &str {
        "translate"
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
        Some("󰗊 Translate".to_string())
    }

    fn message(&self) -> Option<String> {
        if let Some(status) = &self.last_status {
            return Some(status.clone());
        }
        Some(
            "Type text (auto TR↔EN) or prefix en: / tr: / de: / fr: / ja: · Enter: Translate & Copy"
                .to_string(),
        )
    }

    fn no_custom_override(&self) -> Option<bool> {
        Some(false)
    }

    fn theme_snippets(&self) -> Vec<String> {
        vec!["element-icon { enabled: false; }\n\
             mainbox { children: [ inputbar, message, listview ]; }\n\
             message { enabled: true; padding: 6px 8px; background-color: transparent; }\n\
             textbox { background-color: transparent; }"
            .to_string()]
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn entries(&self) -> Vec<ListEntry> {
        if self.history.is_empty() {
            return vec![
                ListEntry {
                    text: "hello world  →  merhaba dünya   [auto→TR]".to_string(),
                    meta: Some("example translate".to_string()),
                    ..ListEntry::default()
                },
                ListEntry {
                    text: "en: günaydın  →  good morning   [TR→EN]".to_string(),
                    meta: Some("example english".to_string()),
                    ..ListEntry::default()
                },
                ListEntry {
                    text: "de: thank you  →  danke schön   [→DE]".to_string(),
                    meta: Some("example german".to_string()),
                    ..ListEntry::default()
                },
            ];
        }
        self.history
            .iter()
            .map(|(pair, src, dst)| ListEntry {
                text: format!("[{pair}]  {src}  →  {dst}"),
                meta: Some(format!("{pair} {src} {dst}")),
                info: Some(dst.clone()),
                ..ListEntry::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::CustomInput { text, .. } | UiOutcome::CustomInputAlt { text } => {
                let q = if text.trim().is_empty() {
                    query.trim()
                } else {
                    text.trim()
                };
                self.run_translate(q)
            }
            UiOutcome::Accepted { index, .. } | UiOutcome::AcceptedAlt { index, .. } => {
                let trimmed = query.trim();
                if !trimmed.is_empty() {
                    return self.run_translate(trimmed);
                }
                if let Some((_, _, dst)) = self.history.get(*index).cloned() {
                    if !self.dry_run {
                        let _ = write_text_to_clipboard(&dst);
                    }
                    return Ok(ModeStep::Exit(0));
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { index } => {
                if *index < self.history.len() {
                    self.history.remove(*index);
                    if !self.dry_run {
                        self.save_history();
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
    fn translate_detects_language_and_parses_google_json() {
        assert_eq!(
            parse_translate_request("merhaba dünya"),
            ("tr", "en", "merhaba dünya")
        );
        assert_eq!(
            parse_translate_request("good morning"),
            ("auto", "tr", "good morning")
        );
        assert_eq!(
            parse_translate_request("de: good morning"),
            ("auto", "de", "good morning")
        );
        let sample = r#"[[["Merhaba dünya","Hello world",null,null,10],[" nasılsın?"," how are you?",null,null,10]],null,"en"]"#;
        assert_eq!(
            parse_google_translate_json(sample).as_deref(),
            Some("Merhaba dünya nasılsın?")
        );
    }
}
