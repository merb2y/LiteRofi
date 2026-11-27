//! `dmenu` and `script` row metadata & header protocol (`\0key\x1fvalue`),
//! plus formatted output (`rofi_output_formatted_line`).

use crate::cli::parse_char;
use crate::matcher::rofi_force_utf8;

/// Parsed entry from `dmenu` or `script` input (`DmenuScriptEntry`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DmenuScriptEntry {
    pub entry: String,
    pub display: Option<String>,
    pub icons: Vec<String>,
    pub meta: Option<String>,
    pub info: Option<String>,
    pub nonselectable: bool,
    pub permanent: bool,
    pub urgent: bool,
    pub active: bool,
}

/// Stream-level header options emitted via lines starting with `\0` (`\0key\x1fvalue`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptHeaderOptions {
    pub message: Option<String>,
    pub prompt: Option<String>,
    pub markup_rows: Option<bool>,
    pub urgent_ranges: Vec<(usize, usize)>,
    pub active_ranges: Vec<(usize, usize)>,
    pub delim: u8,
    pub no_custom: Option<bool>,
    pub use_hot_keys: Option<bool>,
    pub keep_selection: bool,
    pub keep_filter: bool,
    pub new_selection: Option<i64>,
    pub data: Option<String>,
    pub theme_snippets: Vec<String>,
    pub switch_mode: Option<String>,
}

impl Default for ScriptHeaderOptions {
    fn default() -> Self {
        Self {
            message: None,
            prompt: None,
            markup_rows: None,
            urgent_ranges: Vec::new(),
            active_ranges: Vec::new(),
            delim: b'\n',
            no_custom: None,
            use_hot_keys: None,
            keep_selection: false,
            keep_filter: false,
            new_selection: None,
            data: None,
            theme_snippets: Vec::new(),
            switch_mode: None,
        }
    }
}

/// Parses comma-separated 0-based inclusive index or `start-end` pairs (`parse_ranges`).
pub fn parse_ranges(spec: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for token in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some((a, b)) = token.split_once('-') {
            if let (Ok(start), Ok(stop)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) {
                let (lo, hi) = if start <= stop {
                    (start, stop)
                } else {
                    (stop, start)
                };
                out.push((lo, hi));
            }
        } else if let Ok(single) = token.parse::<usize>() {
            out.push((single, single));
        }
    }
    out
}

/// Parses `\x1f`-delimited `key\x1fvalue\x1f...` row extras (`dmenuscript_parse_entry_extras`).
pub fn parse_entry_extras(entry: &mut DmenuScriptEntry, extras: &[u8]) {
    let text = rofi_force_utf8(extras);
    let mut parts = text.split('\x1f');
    while let (Some(key), Some(value)) = (parts.next(), parts.next()) {
        match key.to_ascii_lowercase().as_str() {
            "icon" => {
                entry.icons = value.split(',').map(str::to_string).collect();
            }
            "display" => {
                entry.display = Some(value.to_string());
            }
            "meta" => {
                entry.meta = Some(value.to_string());
            }
            "info" => {
                entry.info = Some(value.to_string());
            }
            "nonselectable" => {
                entry.nonselectable = value.eq_ignore_ascii_case("true");
            }
            "permanent" => {
                entry.permanent = value.eq_ignore_ascii_case("true");
            }
            "urgent" => {
                entry.urgent = value.eq_ignore_ascii_case("true");
            }
            "active" => {
                entry.active = value.eq_ignore_ascii_case("true");
            }
            _ => {}
        }
    }
}

/// Parses one header payload (everything after leading `\0`) (`parse_header_entry`).
pub fn parse_header_entry(headers: &mut ScriptHeaderOptions, payload: &[u8]) {
    let text = rofi_force_utf8(payload);
    let Some((key, value)) = text.split_once('\x1f') else {
        return;
    };
    match key.to_ascii_lowercase().as_str() {
        "message" => {
            headers.message = Some(value.to_string());
        }
        "prompt" => {
            headers.prompt = Some(value.to_string());
        }
        "markup-rows" => {
            headers.markup_rows = Some(value.eq_ignore_ascii_case("true"));
        }
        "urgent" => {
            headers.urgent_ranges = parse_ranges(value);
        }
        "active" => {
            headers.active_ranges = parse_ranges(value);
        }
        "delim" => {
            headers.delim = parse_char(value);
        }
        "no-custom" => {
            headers.no_custom = Some(value.eq_ignore_ascii_case("true"));
        }
        "use-hot-keys" => {
            headers.use_hot_keys = Some(value.eq_ignore_ascii_case("true"));
        }
        "keep-selection" => {
            headers.keep_selection = value.eq_ignore_ascii_case("true");
        }
        "keep-filter" => {
            headers.keep_filter = value.eq_ignore_ascii_case("true");
        }
        "new-selection" => {
            headers.new_selection = value.trim().parse::<i64>().ok();
        }
        "data" => {
            headers.data = Some(value.to_string());
        }
        "theme" => {
            if !value.is_empty() {
                headers.theme_snippets.push(value.to_string());
            }
        }
        "switch-mode" => {
            headers.switch_mode = Some(value.to_string());
        }
        _ => {}
    }
}

/// Parses a raw `dmenu` or `script` input stream, honoring dynamic `\0delim\x1f...`.
pub fn parse_dmenu_script_stream(
    bytes: &[u8],
    initial_delim: u8,
) -> (ScriptHeaderOptions, Vec<DmenuScriptEntry>) {
    let mut headers = ScriptHeaderOptions {
        delim: initial_delim,
        ..ScriptHeaderOptions::default()
    };
    let mut entries = Vec::new();
    let mut pos = 0usize;

    while pos < bytes.len() {
        let delim = headers.delim;
        let end = bytes[pos..]
            .iter()
            .position(|&b| b == delim)
            .map(|rel| pos + rel)
            .unwrap_or(bytes.len());
        let mut record = &bytes[pos..end];
        if delim == b'\n' && record.ends_with(b"\r") {
            record = &record[..record.len() - 1];
        }
        pos = if end < bytes.len() { end + 1 } else { end };

        if record.is_empty() {
            continue;
        }
        if record[0] == 0 {
            parse_header_entry(&mut headers, &record[1..]);
        } else {
            let mut item = DmenuScriptEntry::default();
            if let Some(nul) = record.iter().position(|&b| b == 0) {
                item.entry = rofi_force_utf8(&record[..nul]);
                parse_entry_extras(&mut item, &record[nul + 1..]);
            } else {
                item.entry = rofi_force_utf8(record);
            }
            entries.push(item);
        }
    }

    (headers, entries)
}

/// Formats accepted `dmenu` output (`rofi_output_formatted_line` in `source/helper.c:1708`).
pub fn format_output_line(
    format: &str,
    entry: &str,
    selected_line: i32,
    filter: Option<&str>,
) -> String {
    let mut out = String::new();
    for ch in format.chars() {
        match ch {
            'i' => out.push_str(&selected_line.to_string()),
            'd' => out.push_str(&(selected_line + 1).to_string()),
            's' => out.push_str(entry),
            'p' => out.push_str(&strip_pango_markup(entry)),
            'q' => out.push_str(&shell_quote(entry)),
            'f' => {
                if let Some(f) = filter {
                    out.push_str(f);
                }
            }
            'F' => {
                if let Some(f) = filter {
                    out.push_str(&shell_quote(f));
                }
            }
            other => out.push(other),
        }
    }
    out
}

pub fn strip_pango_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut in_tag = false;
    while let Some(c) = chars.next() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' && in_tag {
            in_tag = false;
            continue;
        }
        if in_tag {
            continue;
        }
        if c == '&' {
            let mut entity = String::new();
            let mut found_semi = false;
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == ';' {
                    found_semi = true;
                    break;
                }
                entity.push(next);
                if entity.len() > 8 {
                    break;
                }
            }
            if found_semi {
                match entity.as_str() {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    "apos" => out.push('\''),
                    _ => {
                        out.push('&');
                        out.push_str(&entity);
                        out.push(';');
                    }
                }
            } else {
                out.push('&');
                out.push_str(&entity);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}
