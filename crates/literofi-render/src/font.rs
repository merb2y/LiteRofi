//! Pango-style Rofi font string parser (`"mono 12"`, `"Segoe UI Bold Italic 11"`)
//! mapped onto DirectWrite font attributes.

/// Parsed font specification ready for `IDWriteFactory::CreateTextFormat`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFont {
    pub family: String,
    pub size_pt: f32,
    pub weight: u32,
    pub bold: bool,
    pub italic: bool,
    pub light: bool,
}

impl Default for ParsedFont {
    fn default() -> Self {
        Self {
            family: "Consolas".to_string(),
            size_pt: 12.0,
            weight: 400,
            bold: false,
            italic: false,
            light: false,
        }
    }
}

/// Parses Rofi / Pango font descriptors (`"mono 12"`, `"Sans Bold 10.5"`).
pub fn parse_rofi_font(spec: &str) -> ParsedFont {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return ParsedFont::default();
    }

    let mut tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut size_pt = 12.0f32;
    if let Some(last) = tokens.last() {
        let num_str = last
            .strip_suffix("px")
            .or_else(|| last.strip_suffix("pt"))
            .unwrap_or(last);
        if let Ok(sz) = num_str.parse::<f32>() {
            if sz > 0.0 {
                size_pt = sz;
                tokens.pop();
            }
        }
    }

    let mut weight = 400u32;
    let mut bold = false;
    let mut italic = false;
    let mut light = false;
    let mut family_parts = Vec::new();

    for tok in tokens {
        match tok.trim_end_matches(',').to_ascii_lowercase().as_str() {
            "thin" => {
                weight = 100;
                light = true;
            }
            "extralight" | "ultralight" => {
                weight = 200;
                light = true;
            }
            "light" | "semilight" | "book" => {
                weight = 300;
                light = true;
            }
            "regular" | "normal" => {
                weight = 400;
            }
            "medium" => {
                weight = 500;
            }
            "semibold" | "demibold" => {
                weight = 600;
                bold = true;
            }
            "bold" => {
                weight = 700;
                bold = true;
            }
            "extrabold" | "ultrabold" => {
                weight = 800;
                bold = true;
            }
            "heavy" | "black" | "extrablack" | "ultrablack" => {
                weight = 900;
                bold = true;
            }
            "italic" | "oblique" => italic = true,
            "condensed" | "expanded" | "semicondensed" | "semiexpanded" => {}
            _ => family_parts.push(tok.trim_end_matches(',')),
        }
    }

    let raw_family = family_parts.join(" ");
    let family = match raw_family.to_ascii_lowercase().as_str() {
        "" | "mono" | "monospace" => "Consolas".to_string(),
        "sans" | "sans-serif" | "system-ui" => "Segoe UI".to_string(),
        "serif" => "Georgia".to_string(),
        _ => raw_family,
    };

    ParsedFont {
        family,
        size_pt,
        weight,
        bold,
        italic,
        light,
    }
}
