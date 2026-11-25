//! Lightweight Pango markup parser (`<b>`, `<i>`, `<u>`, `<s>`, `<small>`, `<big>`,
//! `<span foreground="..." weight="..." style="..." underline="..." strikethrough="..." size="...">`)
//! used by `-markup-rows`, `drun-display-format`, and `message` text rendering.

use literofi_theme::{lookup_css_color, parse_hex_color, ThemeColor};

/// Styled byte span inside the plain text produced by [`parse_pango_markup`].
#[derive(Debug, Clone, PartialEq)]
pub struct PangoSpan {
    pub byte_start: usize,
    pub byte_end: usize,
    pub bold: bool,
    pub light: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub color: Option<ThemeColor>,
    pub size_scale: Option<f32>,
}

#[derive(Debug, Clone, Default)]
struct ActiveStyle {
    bold: bool,
    light: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    color: Option<ThemeColor>,
    size_scale: Option<f32>,
}

impl ActiveStyle {
    fn is_styled(&self) -> bool {
        self.bold
            || self.light
            || self.italic
            || self.underline
            || self.strikethrough
            || self.color.is_some()
            || self.size_scale.is_some()
    }
}

/// Strips Pango tags/entities into plain UTF-8 text while recording exact styled byte spans.
pub fn parse_pango_markup(input: &str) -> (String, Vec<PangoSpan>) {
    let mut plain = String::with_capacity(input.len());
    let mut spans: Vec<PangoSpan> = Vec::new();
    let mut stack: Vec<ActiveStyle> = vec![ActiveStyle::default()];

    let bytes = input.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(rel_end) = input[i + 1..].find('>') {
                let tag_raw = input[i + 1..i + 1 + rel_end].trim();
                i += rel_end + 2;
                if tag_raw.starts_with('/') {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                    continue;
                }
                let self_closing = tag_raw.ends_with('/');
                let tag_body = tag_raw.trim_end_matches('/').trim();
                let mut next = stack.last().cloned().unwrap_or_default();
                apply_open_tag(tag_body, &mut next);
                if !self_closing {
                    stack.push(next);
                }
                continue;
            }
        }

        let seg_start = plain.len();
        if bytes[i] == b'&' {
            if let Some((decoded, consumed)) = decode_entity(&input[i..]) {
                plain.push_str(decoded);
                i += consumed;
            } else {
                plain.push('&');
                i += 1;
            }
        } else {
            let ch = input[i..].chars().next().unwrap_or('\0');
            plain.push(ch);
            i += ch.len_utf8();
        }
        let seg_end = plain.len();

        if seg_end > seg_start {
            if let Some(cur) = stack.last() {
                if cur.is_styled() {
                    if let Some(last) = spans.last_mut() {
                        if last.byte_end == seg_start
                            && last.bold == cur.bold
                            && last.light == cur.light
                            && last.italic == cur.italic
                            && last.underline == cur.underline
                            && last.strikethrough == cur.strikethrough
                            && last.color == cur.color
                            && last.size_scale == cur.size_scale
                        {
                            last.byte_end = seg_end;
                            continue;
                        }
                    }
                    spans.push(PangoSpan {
                        byte_start: seg_start,
                        byte_end: seg_end,
                        bold: cur.bold,
                        light: cur.light,
                        italic: cur.italic,
                        underline: cur.underline,
                        strikethrough: cur.strikethrough,
                        color: cur.color,
                        size_scale: cur.size_scale,
                    });
                }
            }
        }
    }

    (plain, spans)
}

fn apply_open_tag(tag_body: &str, style: &mut ActiveStyle) {
    let mut parts = tag_body.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let attrs = parts.next().unwrap_or("");
    match name.as_str() {
        "b" => style.bold = true,
        "i" => style.italic = true,
        "u" => style.underline = true,
        "s" => style.strikethrough = true,
        "small" => style.size_scale = Some(0.83),
        "big" => style.size_scale = Some(1.2),
        "span" => parse_span_attrs(attrs, style),
        _ => {}
    }
}

fn parse_span_attrs(attrs: &str, style: &mut ActiveStyle) {
    let bytes = attrs.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = attrs[key_start..i].trim().to_ascii_lowercase();
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'=') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let val = if bytes[i] == b'\'' || bytes[i] == b'"' {
            let quote = bytes[i];
            i += 1;
            let val_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let slice = &attrs[val_start..i];
            if i < bytes.len() {
                i += 1;
            }
            slice
        } else {
            let val_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            &attrs[val_start..i]
        };

        match key.as_str() {
            "foreground" | "fgcolor" | "color" => {
                if let Some(c) = parse_pango_color(val) {
                    style.color = Some(c);
                }
            }
            "weight" => match val.to_ascii_lowercase().as_str() {
                "bold" | "ultrabold" | "heavy" | "semibold" | "700" | "800" | "900" => {
                    style.bold = true;
                    style.light = false;
                }
                "light" | "ultralight" | "thin" | "200" | "300" => {
                    style.light = true;
                    style.bold = false;
                }
                "normal" | "400" => {
                    style.bold = false;
                    style.light = false;
                }
                _ => {}
            },
            "style" => match val.to_ascii_lowercase().as_str() {
                "italic" | "oblique" => style.italic = true,
                "normal" => style.italic = false,
                _ => {}
            },
            "underline" => {
                style.underline = !val.eq_ignore_ascii_case("none");
            }
            "strikethrough" => {
                style.strikethrough = val.eq_ignore_ascii_case("true");
            }
            "size" => {
                let scale = match val.to_ascii_lowercase().as_str() {
                    "xx-small" => Some(0.65),
                    "x-small" => Some(0.75),
                    "small" | "smaller" => Some(0.83),
                    "medium" => Some(1.0),
                    "large" | "larger" => Some(1.2),
                    "x-large" => Some(1.44),
                    "xx-large" => Some(1.73),
                    _ => None,
                };
                if let Some(s) = scale {
                    style.size_scale = Some(s);
                }
            }
            _ => {}
        }
    }
}

fn parse_pango_color(spec: &str) -> Option<ThemeColor> {
    let trimmed = spec.trim();
    if trimmed.starts_with('#') {
        return parse_hex_color(trimmed).ok();
    }
    lookup_css_color(trimmed)
}

fn decode_entity(s: &str) -> Option<(&'static str, usize)> {
    let semi = s.find(';')?;
    if semi > 8 {
        return None;
    }
    let decoded = match &s[1..semi] {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => return None,
    };
    Some((decoded, semi + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pango_tags_entities_and_colors() {
        let input = "<b>Code</b> &amp; <span foreground='#ff00aa' weight='light' size='small'><i>(Editor)</i></span>";
        let (plain, spans) = parse_pango_markup(input);
        assert_eq!(plain, "Code & (Editor)");
        assert_eq!(spans.len(), 2);
        assert_eq!(&plain[spans[0].byte_start..spans[0].byte_end], "Code");
        assert!(spans[0].bold);
        assert_eq!(&plain[spans[1].byte_start..spans[1].byte_end], "(Editor)");
        assert!(spans[1].light && spans[1].italic);
        assert!(spans[1].color.is_some());
        assert_eq!(spans[1].size_scale, Some(0.83));
    }
}
