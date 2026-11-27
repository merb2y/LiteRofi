//! Extracts the semantic color palette of the user's currently active theme
//! (`%APPDATA%\LiteRofi\config.rasi` -> `@theme "..."`) so built-in modes like
//! `PowerMode` and `GamesMode` dynamically adapt to any selected theme or palette.

use literofi_theme::{
    DistanceMetrics, Orientation, ParseOptions, PropertyValue, ThemeColor, ThemeSheet, WidgetQuery,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct ActivePalette {
    pub bg: ThemeColor,
    pub bg_alt: ThemeColor,
    pub fg: ThemeColor,
    pub accent: ThemeColor,
    pub accent_fg: ThemeColor,
    pub urgent: ThemeColor,
    /// Border color extracted from `window { border-color: ... }` if defined.
    pub border_color: Option<ThemeColor>,
    /// Corner radius (px) extracted from `window { border-radius: ... }` if defined.
    pub border_radius: Option<f32>,
    /// Border thickness (px) extracted from `window { border: ... }` if defined.
    pub border_width: Option<f32>,
    /// Font family name extracted from the active theme's `font` property if defined.
    pub font_family: Option<String>,
}

impl ActivePalette {
    /// Loads and extracts the semantic palette from the user's active `config.rasi`.
    pub fn load_from_user_config() -> Option<Self> {
        let appdata = std::env::var("APPDATA").ok()?;
        let root = PathBuf::from(appdata).join("LiteRofi");
        let cfg_path = root.join("config.rasi");
        if !cfg_path.is_file() {
            return None;
        }
        let mut search_dirs = vec![root.join("themes"), root.clone()];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                search_dirs.push(dir.join("themes"));
            }
        }
        search_dirs.push(PathBuf::from("themes"));
        let opts = ParseOptions {
            search_dirs,
            dmenu_mode: false,
        };
        let mut sheet = ThemeSheet::new();
        sheet.parse_file(&cfg_path, &opts).ok()?;
        Self::from_sheet(&sheet)
    }

    /// Extracts a coherent 6-color palette from any parsed [`ThemeSheet`].
    pub fn from_sheet(sheet: &ThemeSheet) -> Option<Self> {
        let pick_opaque = |c: Option<ThemeColor>| c.filter(|col| col.a > 0.05);

        let bg = pick_opaque(query_color(sheet, "window", None, &["background-color"]))
            .or_else(|| pick_opaque(root_color(sheet, &["background", "bg", "background-color"])))
            .or_else(|| pick_opaque(query_color(sheet, "mainbox", None, &["background-color"])))?;

        let fg = pick_opaque(query_color(
            sheet,
            "element-text",
            Some("normal.normal"),
            &["text-color", "color"],
        ))
        .or_else(|| {
            pick_opaque(query_color(
                sheet,
                "element",
                Some("normal.normal"),
                &["text-color"],
            ))
        })
        .or_else(|| pick_opaque(root_color(sheet, &["foreground", "fg", "text-color"])))
        .unwrap_or_else(|| contrast_text_for(bg));

        let bg_alt = pick_opaque(query_color(sheet, "inputbar", None, &["background-color"]))
            .filter(|c| !colors_close(*c, bg))
            .or_else(|| {
                pick_opaque(root_color(
                    sheet,
                    &["background-alt", "bg-alt", "button", "secondary", "bg1"],
                ))
                .filter(|c| !colors_close(*c, bg))
            })
            .or_else(|| {
                pick_opaque(query_color(
                    sheet,
                    "element",
                    Some("alternate.normal"),
                    &["background-color"],
                ))
                .filter(|c| !colors_close(*c, bg))
            })
            .unwrap_or_else(|| shift_surface(bg));

        let accent = pick_opaque(query_color(
            sheet,
            "element",
            Some("selected.normal"),
            &["background-color"],
        ))
        .filter(|c| !colors_close(*c, bg))
        .or_else(|| {
            pick_opaque(query_color(sheet, "window", None, &["border-color"]))
                .filter(|c| !colors_close(*c, bg))
        })
        .or_else(|| {
            pick_opaque(root_color(
                sheet,
                &["selected", "primary", "accent", "border-colour", "active"],
            ))
            .filter(|c| !colors_close(*c, bg))
        })
        .unwrap_or(fg);

        let accent_fg = pick_opaque(widget_color(
            sheet,
            "element-text",
            Some("selected.normal"),
            &["text-color", "color"],
        ))
        .or_else(|| {
            pick_opaque(widget_color(
                sheet,
                "element",
                Some("selected.normal"),
                &["text-color"],
            ))
        })
        .filter(|c| !colors_close(*c, accent))
        .unwrap_or_else(|| contrast_text_for(accent));

        let urgent = pick_opaque(query_color(
            sheet,
            "element",
            Some("normal.urgent"),
            &["background-color"],
        ))
        .or_else(|| pick_opaque(root_color(sheet, &["urgent", "red"])))
        .unwrap_or(ThemeColor::new(0.75, 0.38, 0.42, 1.0));

        let border_color = pick_opaque(query_color(sheet, "window", None, &["border-color"]));
        let border_radius = extract_window_px(sheet, "border-radius");
        let border_width = extract_window_px(sheet, "border");
        let font_family = extract_font_family(sheet);

        Some(Self {
            bg: ThemeColor::new(bg.r, bg.g, bg.b, 1.0),
            bg_alt: ThemeColor::new(bg_alt.r, bg_alt.g, bg_alt.b, 1.0),
            fg: ThemeColor::new(fg.r, fg.g, fg.b, 1.0),
            accent: ThemeColor::new(accent.r, accent.g, accent.b, 1.0),
            accent_fg: ThemeColor::new(accent_fg.r, accent_fg.g, accent_fg.b, 1.0),
            urgent: ThemeColor::new(urgent.r, urgent.g, urgent.b, 1.0),
            border_color: border_color.map(|c| ThemeColor::new(c.r, c.g, c.b, 1.0)),
            border_radius,
            border_width,
            font_family,
        })
    }

    pub fn bg_hex(&self) -> String {
        to_hex_rgb(self.bg)
    }

    pub fn bg_translucent_hex(&self) -> String {
        format!("{}E0", to_hex_rgb(self.bg))
    }

    pub fn bg_alt_hex(&self) -> String {
        to_hex_rgb(self.bg_alt)
    }

    pub fn fg_hex(&self) -> String {
        to_hex_rgb(self.fg)
    }

    pub fn accent_hex(&self) -> String {
        to_hex_rgb(self.accent)
    }

    pub fn accent_fg_hex(&self) -> String {
        to_hex_rgb(self.accent_fg)
    }

    pub fn urgent_hex(&self) -> String {
        to_hex_rgb(self.urgent)
    }

    /// Generates a `.rasi` snippet overriding `powermenu.rasi` colors & geometry to match the active theme.
    pub fn to_powermenu_rasi(&self) -> String {
        let bg = self.bg_hex();
        let bg_alt = self.bg_alt_hex();
        let fg = self.fg_hex();
        let accent = self.accent_hex();
        let accent_fg = self.accent_fg_hex();
        let urgent = self.urgent_hex();
        let urgent_fg = to_hex_rgb(contrast_text_for(self.urgent));
        let border_col = self
            .border_color
            .map(to_hex_rgb)
            .unwrap_or_else(|| accent.clone());
        let rad = self.border_radius.unwrap_or(14.0).round() as i32;
        let inner_rad = (rad as f32 * 0.65).round() as i32;
        let bw = self.border_width.unwrap_or(1.0).round() as i32;
        format!(
            "* {{ background: {bg}; background-alt: {bg_alt}; foreground: {fg}; selected: {accent}; active: {accent}; urgent: {urgent}; }}\n\
             window {{ background-color: {bg}; border: {bw}px solid; border-color: {border_col}; border-radius: {rad}px; }}\n\
             textbox-prompt-colon {{ background-color: {urgent}; text-color: {urgent_fg}; border-radius: {inner_rad}px; }}\n\
             prompt {{ background-color: {accent}; text-color: {accent_fg}; border-radius: {inner_rad}px; }}\n\
             message {{ background-color: {bg_alt}; text-color: {fg}; border-radius: {inner_rad}px; }}\n\
             element {{ background-color: {bg_alt}; text-color: {fg}; border-radius: {inner_rad}px; }}\n\
             element selected.normal {{ background-color: {accent}; text-color: {accent_fg}; }}"
        )
    }

    /// Generates a `.rasi` snippet overriding `game-launcher.rasi` colors to match the active theme.
    pub fn to_games_rasi(&self) -> String {
        let bg = self.bg_hex();
        let bg_trans = self.bg_translucent_hex();
        let bg_alt = self.bg_alt_hex();
        let fg = self.fg_hex();
        let accent = self.accent_hex();
        let accent_fg = self.accent_fg_hex();
        let urgent = self.urgent_hex();
        format!(
            "* {{ background: {bg}; background-transparent: {bg_trans}; text-color: {fg}; primary: {accent}; secondary: {bg_alt}; urgent: {urgent}; }}\n\
             window {{ background-color: {bg_trans}; border-color: {accent}; }}\n\
             inputbar-side {{ border-color: {accent}; }}\n\
             inputbar {{ background-color: {accent}; border-color: {accent}; }}\n\
             textbox-prompt {{ text-color: {accent_fg}; }}\n\
             entry {{ background-color: {bg}; border-color: {accent}; text-color: {fg}; }}\n\
             element-icon selected {{ border-color: {accent}; }}\n\
             element-text {{ text-color: {fg}; }}"
        )
    }

    /// Generates a `.rasi` snippet overriding `music-player.rasi` colors to match the active theme
    /// while preserving album artwork on `inputbar`.
    pub fn to_music_rasi(&self) -> String {
        let bg = self.bg_hex();
        let bg_trans = self.bg_translucent_hex();
        let bg_alt = self.bg_alt_hex();
        let fg = self.fg_hex();
        let accent = self.accent_hex();
        let accent_fg = self.accent_fg_hex();
        format!(
            "* {{ bg: {bg}; fg: {fg}; button: {bg_alt}; background-color: {bg}; text-color: {fg}; }}\n\
             window {{ background-color: {bg}; border-color: {accent}; }}\n\
             mainbox {{ background-color: {bg}; }}\n\
             inputbar {{ background-color: {bg_alt}; }}\n\
             prompt {{ background-color: {bg_trans}; text-color: {fg}; }}\n\
             listview {{ background-color: {bg}; }}\n\
             element {{ background-color: {bg_alt}; text-color: {fg}; }}\n\
             element selected.normal {{ background-color: {accent}; text-color: {accent_fg}; }}"
        )
    }

    /// Generates a `.rasi` snippet overriding `wallpaper-picker.rasi` colors to match the active theme.
    pub fn to_wallpaper_rasi(&self) -> String {
        let bg = self.bg_hex();
        let bg_alt = self.bg_alt_hex();
        let fg = to_hex_rgb(contrast_text_for(self.bg_alt));
        let accent = self.accent_hex();
        let accent_fg = to_hex_rgb(contrast_text_for(self.accent));
        format!(
            "* {{ bg: {bg}; bg-alt: {bg_alt}; fg: {fg}; accent: {accent}; accent-fg: {accent_fg}; }}\n\
             window {{ background-color: {bg}; border-color: {accent}; }}\n\
             inputbar {{ background-color: {bg_alt}; }}\n\
             prompt {{ background-color: {accent}; text-color: {accent_fg}; }}\n\
             entry {{ text-color: {fg}; }}\n\
             textbox {{ text-color: {fg}; }}\n\
             element {{ background-color: {bg_alt}; text-color: {fg}; }}\n\
             element selected.normal {{ background-color: {accent}; border-color: {accent}; text-color: {accent_fg}; }}"
        )
    }

    /// Generates a `.rasi` snippet overriding `clipboard.rasi` colors & geometry to match the active theme.
    pub fn to_clipboard_rasi(&self) -> String {
        let bg = self.bg_hex();
        let bg_alt = self.bg_alt_hex();
        let fg = to_hex_rgb(contrast_text_for(self.bg_alt));
        let accent = self.accent_hex();
        let accent_fg = to_hex_rgb(contrast_text_for(self.accent));
        let border_col = self
            .border_color
            .map(to_hex_rgb)
            .unwrap_or_else(|| accent.clone());
        let rad = self.border_radius.unwrap_or(14.0).round() as i32;
        let inner_rad = (rad as f32 * 0.65).round() as i32;
        let bw = self.border_width.unwrap_or(1.0).round() as i32;
        format!(
            "* {{ bg: {bg}; bg-alt: {bg_alt}; fg: {fg}; accent: {accent}; accent-fg: {accent_fg}; }}\n\
             window {{ background-color: {bg}; border: {bw}px solid; border-color: {border_col}; border-radius: {rad}px; }}\n\
             inputbar {{ background-color: {bg_alt}; border-radius: {inner_rad}px; }}\n\
             prompt {{ background-color: {accent}; text-color: {accent_fg}; border-radius: {inner_rad}px; }}\n\
             entry {{ text-color: {fg}; }}\n\
             textbox {{ text-color: {fg}; }}\n\
             scrollbar {{ handle-color: {accent}; }}\n\
             element {{ background-color: {bg_alt}; text-color: {fg}; border-radius: {inner_rad}px; }}\n\
             element selected.normal {{ background-color: {accent}; border-color: {accent}; text-color: {accent_fg}; }}"
        )
    }

    /// Generates a `.rasi` snippet overriding `control-center.rasi` colors & geometry with WCAG contrast.
    pub fn to_control_rasi(&self) -> String {
        let bg = self.bg_hex();
        let bg_alt = self.bg_alt_hex();
        let fg = to_hex_rgb(contrast_text_for(self.bg_alt));
        let accent = self.accent_hex();
        let accent_fg = to_hex_rgb(contrast_text_for(self.accent));
        let border_col = self
            .border_color
            .map(to_hex_rgb)
            .unwrap_or_else(|| accent.clone());
        let rad = self.border_radius.unwrap_or(14.0).round() as i32;
        let inner_rad = (rad as f32 * 0.65).round() as i32;
        let bw = self.border_width.unwrap_or(1.0).round() as i32;
        format!(
            "* {{ bg: {bg}; bg-alt: {bg_alt}; fg: {fg}; accent: {accent}; accent-fg: {accent_fg}; }}\n\
             window {{ background-color: {bg}; border: {bw}px solid; border-color: {border_col}; border-radius: {rad}px; }}\n\
             inputbar {{ background-color: {bg_alt}; border-radius: {inner_rad}px; }}\n\
             prompt {{ background-color: {accent}; text-color: {accent_fg}; border-radius: {inner_rad}px; }}\n\
             entry {{ text-color: {fg}; }}\n\
             textbox {{ text-color: {fg}; }}\n\
             element {{ background-color: {bg_alt}; text-color: {fg}; border-radius: {inner_rad}px; }}\n\
             element selected.normal {{ background-color: {accent}; border-color: {accent}; text-color: {accent_fg}; }}"
        )
    }
}

fn extract_window_px(sheet: &ThemeSheet, prop: &str) -> Option<f32> {
    let q = WidgetQuery::new("window");
    let metrics = DistanceMetrics::default();
    match sheet.find_property(&q, prop) {
        Some(PropertyValue::Padding(p)) => {
            Some(p.left.to_pixels(Orientation::Horizontal, &metrics).max(0.0) as f32)
        }
        Some(PropertyValue::Integer(i)) => Some((*i).max(0) as f32),
        Some(PropertyValue::Double(f)) => Some((*f).max(0.0) as f32),
        _ => None,
    }
}

fn extract_font_family(sheet: &ThemeSheet) -> Option<String> {
    let raw = [
        sheet.find_property(&WidgetQuery::new("element-text"), "font"),
        sheet.find_property(&WidgetQuery::new("window"), "font"),
        sheet.find_root_property("font"),
    ]
    .into_iter()
    .flatten()
    .find_map(|v| match v {
        PropertyValue::String(s) if !s.trim().is_empty() => Some(s.as_str()),
        _ => None,
    })?;
    // Strip trailing size/weight tokens (e.g. "JetBrainsMono NF 11" -> "JetBrainsMono NF")
    let mut parts: Vec<&str> = raw.split_whitespace().collect();
    while let Some(&last) = parts.last() {
        if last.chars().all(|c| c.is_ascii_digit() || c == '.')
            || matches!(
                last.to_ascii_lowercase().as_str(),
                "regular" | "bold" | "medium" | "semibold" | "light" | "italic"
            )
        {
            parts.pop();
        } else {
            break;
        }
    }
    let fam = parts.join(" ");
    (!fam.is_empty()).then_some(fam)
}

fn widget_color(
    sheet: &ThemeSheet,
    widget: &str,
    state: Option<&str>,
    keys: &[&str],
) -> Option<ThemeColor> {
    match sheet.find_widget_property_aliases(widget, state, keys) {
        Some(PropertyValue::Color(c)) => Some(*c),
        _ => None,
    }
}

fn query_color(
    sheet: &ThemeSheet,
    widget: &str,
    state: Option<&str>,
    keys: &[&str],
) -> Option<ThemeColor> {
    let q = match state {
        Some(st) => WidgetQuery::new(widget).with_state(st),
        None => WidgetQuery::new(widget),
    };
    match sheet.find_property_aliases(&q, keys) {
        Some(PropertyValue::Color(c)) => Some(*c),
        _ => None,
    }
}

fn root_color(sheet: &ThemeSheet, keys: &[&str]) -> Option<ThemeColor> {
    for k in keys {
        if let Some(PropertyValue::Color(c)) = sheet.find_root_property(k) {
            return Some(*c);
        }
    }
    None
}

fn luma(c: ThemeColor) -> f64 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

fn contrast_text_for(bg: ThemeColor) -> ThemeColor {
    if luma(bg) > 0.45 {
        ThemeColor::new(0.12, 0.13, 0.16, 1.0)
    } else {
        ThemeColor::new(0.93, 0.94, 0.96, 1.0)
    }
}

fn shift_surface(bg: ThemeColor) -> ThemeColor {
    let delta = if luma(bg) > 0.45 { -0.07 } else { 0.07 };
    ThemeColor::new(
        (bg.r + delta).clamp(0.0, 1.0),
        (bg.g + delta).clamp(0.0, 1.0),
        (bg.b + delta).clamp(0.0, 1.0),
        1.0,
    )
}

fn colors_close(a: ThemeColor, b: ThemeColor) -> bool {
    (a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs() < 0.06
}

fn to_hex_rgb(c: ThemeColor) -> String {
    let r = (c.r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c.g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c.b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[allow(dead_code)]
fn _keep_path_import(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_palette_from_yuky_carbonized_style_sheet() {
        let rasi = r#"
            * { bg: #b5b5aa; fg: #2b2b2b; button: #9e9e95; background-color: @bg; text-color: @fg; }
            window { background-color: @bg; }
            inputbar { background-color: @button; }
            element selected { background-color: @fg; text-color: @bg; }
        "#;
        let mut sheet = ThemeSheet::new();
        sheet.parse_str(rasi, &ParseOptions::default()).unwrap();
        let pal = ActivePalette::from_sheet(&sheet).expect("palette");
        assert_eq!(pal.bg_hex(), "#B5B5AA");
        assert_eq!(pal.bg_alt_hex(), "#9E9E95");
        assert_eq!(pal.fg_hex(), "#2B2B2B");
        assert_eq!(pal.accent_hex(), "#2B2B2B");
        assert_eq!(pal.accent_fg_hex(), "#B5B5AA");
        assert!(pal.to_powermenu_rasi().contains("#B5B5AA"));
        assert!(pal.to_games_rasi().contains("#2B2B2B"));
    }
}
