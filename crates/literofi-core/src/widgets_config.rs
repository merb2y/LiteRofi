//! Persistent user configuration for LiteRofi extra widgets & plugins
//! (`%APPDATA%\LiteRofi\widgets.toml`), editable both by hand and visually
//! from LiteRofi Theme Studio -> Widgets & Plugins.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamesWidgetConfig {
    /// `"active"` (follow active Rofi theme) or `"custom"` (use colors below).
    pub theme_mode: String,
    pub font: String,
    pub ground: String,
    pub surface: String,
    pub ink: String,
    pub accent: String,
    pub accent_ink: String,
    pub radius: u32,
}

impl Default for GamesWidgetConfig {
    fn default() -> Self {
        Self {
            theme_mode: "active".to_string(),
            font: "Segoe UI".to_string(),
            ground: "#181a24".to_string(),
            surface: "#242837".to_string(),
            ink: "#eef1f7".to_string(),
            accent: "#f2f4f8".to_string(),
            accent_ink: "#13161f".to_string(),
            radius: 18,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicWidgetConfig {
    /// `"auto"` (extract from album cover), `"active"` (follow active theme), or `"custom"`.
    pub theme_mode: String,
    pub font: String,
    pub bg: String,
    pub bg_alt: String,
    pub accent: String,
}

impl Default for MusicWidgetConfig {
    fn default() -> Self {
        Self {
            theme_mode: "auto".to_string(),
            font: "Segoe UI".to_string(),
            bg: "#131720".to_string(),
            bg_alt: "#1c2230".to_string(),
            accent: "#7aa2f7".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperWidgetConfig {
    /// `"active"` or `"custom"`.
    pub theme_mode: String,
    pub accent: String,
    /// Optional Wallhaven search query / tags (e.g. `"cyberpunk"`, `"nature"`).
    pub wallhaven_query: String,
    /// 3-bit Wallhaven categories string: `[General][Anime][People]`, e.g. `"111"`, `"100"`, `"010"`.
    pub wallhaven_categories: String,
    /// Wallhaven sorting: `"random"`, `"toplist"`, `"favorites"`, `"views"`, `"date_added"`, `"hot"`.
    pub wallhaven_sorting: String,
    /// Wallhaven topRange (used when sorting is `"toplist"`): `"1d"`, `"3d"`, `"1w"`, `"1M"`, `"3M"`, `"6M"`, `"1y"`.
    pub wallhaven_top_range: String,
    /// Minimum resolution filter: `""` (any), `"1920x1080"`, `"2560x1440"`, `"3840x2160"`, `"3440x1440"`.
    pub wallhaven_atleast: String,
    /// Aspect ratio filter: `""` (any), `"16x9"`, `"16x10"`, `"21x9"`, `"32x9"`, `"landscape"`, `"portrait"`.
    pub wallhaven_ratios: String,
    /// Optional Wallhaven 6-hex color filter without `#` (e.g. `""` or `"000000"`).
    pub wallhaven_color: String,
    /// Optional Steam Workshop search query for Wallpaper Engine discovery (`Alt+W → E`).
    pub we_query: String,
    /// Wallpaper Engine genre/tag filter (e.g. `""`, `"Landscape"`, `"Cyberpunk"`, `"Sci-Fi"`).
    pub we_genre: String,
    /// Wallpaper Engine type filter (`""`, `"Scene"`, `"Video"`, `"Web"`).
    pub we_type: String,
    /// Wallpaper Engine Steam Workshop sort (`"toprated"`, `"trend"`, `"mostrecent"`).
    pub we_sort: String,
    /// Exclude `"Anime"` tag from Wallpaper Engine discovery by default (unless `we_genre` is `"Anime"`).
    pub we_exclude_anime: bool,
    /// Automatically mute Wallpaper Engine audio when applying wallpapers via LiteRofi.
    pub we_mute_audio: bool,
}

impl Default for WallpaperWidgetConfig {
    fn default() -> Self {
        Self {
            theme_mode: "active".to_string(),
            accent: "#7aa2f7".to_string(),
            wallhaven_query: String::new(),
            wallhaven_categories: "111".to_string(),
            wallhaven_sorting: "random".to_string(),
            wallhaven_top_range: "1M".to_string(),
            wallhaven_atleast: "1920x1080".to_string(),
            wallhaven_ratios: String::new(),
            wallhaven_color: String::new(),
            we_query: String::new(),
            we_genre: String::new(),
            we_type: String::new(),
            we_sort: "toprated".to_string(),
            we_exclude_anime: true,
            we_mute_audio: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasiPluginConfig {
    /// `"active"` (follow active theme) or `"custom"`.
    pub theme_mode: String,
    pub font: String,
    pub bg: String,
    pub bg_alt: String,
    pub fg: String,
    pub accent: String,
}

impl Default for RasiPluginConfig {
    fn default() -> Self {
        Self {
            theme_mode: "active".to_string(),
            font: "Segoe UI 11".to_string(),
            bg: "#181a24".to_string(),
            bg_alt: "#242837".to_string(),
            fg: "#eef1f7".to_string(),
            accent: "#7aa2f7".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WidgetsConfig {
    pub games: GamesWidgetConfig,
    pub music: MusicWidgetConfig,
    pub wallpaper: WallpaperWidgetConfig,
    pub clipboard: RasiPluginConfig,
    pub control: RasiPluginConfig,
    pub power: RasiPluginConfig,
}

impl WidgetsConfig {
    /// Default location: `%APPDATA%\LiteRofi\widgets.toml`.
    pub fn default_path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                return PathBuf::from(appdata).join("LiteRofi").join("widgets.toml");
            }
        }
        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            if !home.trim().is_empty() {
                return PathBuf::from(home)
                    .join("AppData")
                    .join("Roaming")
                    .join("LiteRofi")
                    .join("widgets.toml");
            }
        }
        PathBuf::from("widgets.toml")
    }

    pub fn load_default() -> Self {
        Self::load_from(&Self::default_path())
    }

    pub fn save_default(&self) -> Result<PathBuf, String> {
        let p = Self::default_path();
        self.save_to(&p)?;
        Ok(p)
    }

    pub fn load_from(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse_toml(&text)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create config dir: {e}"))?;
        }
        std::fs::write(path, self.to_toml())
            .map_err(|e| format!("failed to write {}: {e}", path.display()))
    }

    pub fn parse_toml(text: &str) -> Self {
        let mut cfg = Self::default();
        let mut section = String::new();
        for raw in text.lines() {
            let line = strip_toml_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().trim_matches('"').trim_matches('\'').trim();
            match section.as_str() {
                "games" => match key.as_str() {
                    "theme_mode" if matches!(val, "active" | "custom") => {
                        cfg.games.theme_mode = val.to_string()
                    }
                    "font" if !val.is_empty() => cfg.games.font = val.to_string(),
                    "ground" if is_hex_color(val) => cfg.games.ground = val.to_string(),
                    "surface" if is_hex_color(val) => cfg.games.surface = val.to_string(),
                    "ink" if is_hex_color(val) => cfg.games.ink = val.to_string(),
                    "accent" if is_hex_color(val) => cfg.games.accent = val.to_string(),
                    "accent_ink" if is_hex_color(val) => cfg.games.accent_ink = val.to_string(),
                    "radius" => {
                        if let Ok(r) = val.parse::<u32>() {
                            cfg.games.radius = r.clamp(0, 32);
                        }
                    }
                    _ => {}
                },
                "music" => match key.as_str() {
                    "theme_mode" if matches!(val, "auto" | "active" | "custom") => {
                        cfg.music.theme_mode = val.to_string()
                    }
                    "font" if !val.is_empty() => cfg.music.font = val.to_string(),
                    "bg" if is_hex_color(val) => cfg.music.bg = val.to_string(),
                    "bg_alt" if is_hex_color(val) => cfg.music.bg_alt = val.to_string(),
                    "accent" if is_hex_color(val) => cfg.music.accent = val.to_string(),
                    _ => {}
                },
                "wallpaper" => match key.as_str() {
                    "theme_mode" if matches!(val, "active" | "custom") => {
                        cfg.wallpaper.theme_mode = val.to_string()
                    }
                    "accent" if is_hex_color(val) => cfg.wallpaper.accent = val.to_string(),
                    "wallhaven_query" => cfg.wallpaper.wallhaven_query = val.to_string(),
                    "wallhaven_categories" if is_valid_wh_categories(val) => {
                        cfg.wallpaper.wallhaven_categories = val.to_string()
                    }
                    "wallhaven_sorting"
                        if matches!(
                            val,
                            "random" | "toplist" | "favorites" | "views" | "date_added" | "hot"
                        ) =>
                    {
                        cfg.wallpaper.wallhaven_sorting = val.to_string()
                    }
                    "wallhaven_top_range"
                        if matches!(val, "1d" | "3d" | "1w" | "1M" | "3M" | "6M" | "1y") =>
                    {
                        cfg.wallpaper.wallhaven_top_range = val.to_string()
                    }
                    "wallhaven_atleast" => cfg.wallpaper.wallhaven_atleast = val.to_string(),
                    "wallhaven_ratios" => cfg.wallpaper.wallhaven_ratios = val.to_string(),
                    "wallhaven_color" => {
                        let clean = val.trim_start_matches('#');
                        if clean.is_empty()
                            || (clean.len() == 6 && clean.chars().all(|c| c.is_ascii_hexdigit()))
                        {
                            cfg.wallpaper.wallhaven_color = clean.to_ascii_lowercase();
                        }
                    }
                    "we_query" => cfg.wallpaper.we_query = val.to_string(),
                    "we_genre" => cfg.wallpaper.we_genre = val.to_string(),
                    "we_type" if matches!(val, "" | "Scene" | "Video" | "Web") => {
                        cfg.wallpaper.we_type = val.to_string()
                    }
                    "we_sort" if matches!(val, "toprated" | "trend" | "mostrecent") => {
                        cfg.wallpaper.we_sort = val.to_string()
                    }
                    "we_exclude_anime" => {
                        cfg.wallpaper.we_exclude_anime = val == "true" || val == "1";
                    }
                    "we_mute_audio" => {
                        cfg.wallpaper.we_mute_audio = val == "true" || val == "1";
                    }
                    _ => {}
                },
                "clipboard" => apply_rasi_kv(&mut cfg.clipboard, &key, val),
                "control" => apply_rasi_kv(&mut cfg.control, &key, val),
                "power" => apply_rasi_kv(&mut cfg.power, &key, val),
                _ => {}
            }
        }
        cfg
    }

    pub fn to_toml(&self) -> String {
        format!(
            "# LiteRofi Extra Widgets & Plugins Configuration\n\
             # Edit manually here OR customize visually from LiteRofi Theme Studio -> Widgets\n\n\
             [games]\n\
             theme_mode = \"{}\"\n\
             font = \"{}\"\n\
             ground = \"{}\"\n\
             surface = \"{}\"\n\
             ink = \"{}\"\n\
             accent = \"{}\"\n\
             accent_ink = \"{}\"\n\
             radius = {}\n\n\
             [music]\n\
             theme_mode = \"{}\"\n\
             font = \"{}\"\n\
             bg = \"{}\"\n\
             bg_alt = \"{}\"\n\
             accent = \"{}\"\n\n\
             [wallpaper]\n\
             theme_mode = \"{}\"\n\
             accent = \"{}\"\n\
             wallhaven_query = \"{}\"\n\
             wallhaven_categories = \"{}\"\n\
             wallhaven_sorting = \"{}\"\n\
             wallhaven_top_range = \"{}\"\n\
             wallhaven_atleast = \"{}\"\n\
             wallhaven_ratios = \"{}\"\n\
             wallhaven_color = \"{}\"\n\
             we_query = \"{}\"\n\
             we_genre = \"{}\"\n\
             we_type = \"{}\"\n\
             we_sort = \"{}\"\n\
             we_exclude_anime = {}\n\
             we_mute_audio = {}\n\n\
             [clipboard]\n\
             theme_mode = \"{}\"\n\
             font = \"{}\"\n\
             bg = \"{}\"\n\
             bg_alt = \"{}\"\n\
             fg = \"{}\"\n\
             accent = \"{}\"\n\n\
             [control]\n\
             theme_mode = \"{}\"\n\
             font = \"{}\"\n\
             bg = \"{}\"\n\
             bg_alt = \"{}\"\n\
             fg = \"{}\"\n\
             accent = \"{}\"\n\n\
             [power]\n\
             theme_mode = \"{}\"\n\
             font = \"{}\"\n\
             bg = \"{}\"\n\
             bg_alt = \"{}\"\n\
             fg = \"{}\"\n\
             accent = \"{}\"\n",
            esc(&self.games.theme_mode),
            esc(&self.games.font),
            esc(&self.games.ground),
            esc(&self.games.surface),
            esc(&self.games.ink),
            esc(&self.games.accent),
            esc(&self.games.accent_ink),
            self.games.radius,
            esc(&self.music.theme_mode),
            esc(&self.music.font),
            esc(&self.music.bg),
            esc(&self.music.bg_alt),
            esc(&self.music.accent),
            esc(&self.wallpaper.theme_mode),
            esc(&self.wallpaper.accent),
            esc(&self.wallpaper.wallhaven_query),
            esc(&self.wallpaper.wallhaven_categories),
            esc(&self.wallpaper.wallhaven_sorting),
            esc(&self.wallpaper.wallhaven_top_range),
            esc(&self.wallpaper.wallhaven_atleast),
            esc(&self.wallpaper.wallhaven_ratios),
            esc(&self.wallpaper.wallhaven_color),
            esc(&self.wallpaper.we_query),
            esc(&self.wallpaper.we_genre),
            esc(&self.wallpaper.we_type),
            esc(&self.wallpaper.we_sort),
            self.wallpaper.we_exclude_anime,
            self.wallpaper.we_mute_audio,
            esc(&self.clipboard.theme_mode),
            esc(&self.clipboard.font),
            esc(&self.clipboard.bg),
            esc(&self.clipboard.bg_alt),
            esc(&self.clipboard.fg),
            esc(&self.clipboard.accent),
            esc(&self.control.theme_mode),
            esc(&self.control.font),
            esc(&self.control.bg),
            esc(&self.control.bg_alt),
            esc(&self.control.fg),
            esc(&self.control.accent),
            esc(&self.power.theme_mode),
            esc(&self.power.font),
            esc(&self.power.bg),
            esc(&self.power.bg_alt),
            esc(&self.power.fg),
            esc(&self.power.accent),
        )
    }

    pub fn to_json(&self, config_path: &Path) -> String {
        format!(
            concat!(
                r#"{{"configPath":"{}","games":{{"theme_mode":"{}","font":"{}","ground":"{}","surface":"{}","ink":"{}","accent":"{}","accent_ink":"{}","radius":{}}},"#,
                r#""music":{{"theme_mode":"{}","font":"{}","bg":"{}","bg_alt":"{}","accent":"{}"}},"#,
                r#""wallpaper":{{"theme_mode":"{}","accent":"{}","wallhaven_query":"{}","wallhaven_categories":"{}","wallhaven_sorting":"{}","wallhaven_top_range":"{}","wallhaven_atleast":"{}","wallhaven_ratios":"{}","wallhaven_color":"{}","we_query":"{}","we_genre":"{}","we_type":"{}","we_sort":"{}","we_exclude_anime":{},"we_mute_audio":{}}},"#,
                r#""clipboard":{{"theme_mode":"{}","font":"{}","bg":"{}","bg_alt":"{}","fg":"{}","accent":"{}"}},"#,
                r#""control":{{"theme_mode":"{}","font":"{}","bg":"{}","bg_alt":"{}","fg":"{}","accent":"{}"}},"#,
                r#""power":{{"theme_mode":"{}","font":"{}","bg":"{}","bg_alt":"{}","fg":"{}","accent":"{}"}}}}"#
            ),
            json_esc(&config_path.to_string_lossy()),
            json_esc(&self.games.theme_mode),
            json_esc(&self.games.font),
            json_esc(&self.games.ground),
            json_esc(&self.games.surface),
            json_esc(&self.games.ink),
            json_esc(&self.games.accent),
            json_esc(&self.games.accent_ink),
            self.games.radius,
            json_esc(&self.music.theme_mode),
            json_esc(&self.music.font),
            json_esc(&self.music.bg),
            json_esc(&self.music.bg_alt),
            json_esc(&self.music.accent),
            json_esc(&self.wallpaper.theme_mode),
            json_esc(&self.wallpaper.accent),
            json_esc(&self.wallpaper.wallhaven_query),
            json_esc(&self.wallpaper.wallhaven_categories),
            json_esc(&self.wallpaper.wallhaven_sorting),
            json_esc(&self.wallpaper.wallhaven_top_range),
            json_esc(&self.wallpaper.wallhaven_atleast),
            json_esc(&self.wallpaper.wallhaven_ratios),
            json_esc(&self.wallpaper.wallhaven_color),
            json_esc(&self.wallpaper.we_query),
            json_esc(&self.wallpaper.we_genre),
            json_esc(&self.wallpaper.we_type),
            json_esc(&self.wallpaper.we_sort),
            self.wallpaper.we_exclude_anime,
            self.wallpaper.we_mute_audio,
            json_esc(&self.clipboard.theme_mode),
            json_esc(&self.clipboard.font),
            json_esc(&self.clipboard.bg),
            json_esc(&self.clipboard.bg_alt),
            json_esc(&self.clipboard.fg),
            json_esc(&self.clipboard.accent),
            json_esc(&self.control.theme_mode),
            json_esc(&self.control.font),
            json_esc(&self.control.bg),
            json_esc(&self.control.bg_alt),
            json_esc(&self.control.fg),
            json_esc(&self.control.accent),
            json_esc(&self.power.theme_mode),
            json_esc(&self.power.font),
            json_esc(&self.power.bg),
            json_esc(&self.power.bg_alt),
            json_esc(&self.power.fg),
            json_esc(&self.power.accent),
        )
    }

    /// Generates an optional `.rasi` override snippet for a list-based Rasi plugin (`clipboard`, `control`).
    pub fn rasi_override_snippet(cfg: &RasiPluginConfig) -> Option<String> {
        Self::build_rasi_override_snippet(cfg, true)
    }

    /// Generates an optional `.rasi` override snippet for `powermenu` (preserves large Nerd Font icon glyphs on `element-text`).
    pub fn rasi_power_override_snippet(cfg: &RasiPluginConfig) -> Option<String> {
        Self::build_rasi_override_snippet(cfg, false)
    }

    fn build_rasi_override_snippet(
        cfg: &RasiPluginConfig,
        override_element_text: bool,
    ) -> Option<String> {
        let mut lines = Vec::new();
        let font = cfg.font.trim();
        if !font.is_empty() && font != "Segoe UI 11" {
            let f = esc(font);
            let small_f = esc(&smaller_font_spec(font, 1));
            if override_element_text {
                lines.push(format!(
                    "* {{ font: \"{f}\"; }}\n\
                     prompt {{ font: \"{f}\"; }}\n\
                     entry {{ font: \"{f}\"; }}\n\
                     textbox {{ font: \"{small_f}\"; }}\n\
                     element-text {{ font: \"{f}\"; }}"
                ));
            } else {
                lines.push(format!(
                    "* {{ font: \"{f}\"; }}\n\
                     prompt {{ font: \"{f}\"; }}\n\
                     textbox {{ font: \"{f}\"; }}"
                ));
            }
        }
        if cfg.theme_mode == "custom" {
            let fg = &cfg.fg;
            let bg = &cfg.bg;
            let bg_alt = &cfg.bg_alt;
            let accent = &cfg.accent;
            let accent_fg = contrast_hex_for(accent);
            lines.push(format!(
                "* {{ bg: {bg}; bg-alt: {bg_alt}; fg: {fg}; accent: {accent}; accent-fg: {accent_fg}; background: {bg}; background-alt: {bg_alt}; foreground: {fg}; selected: {accent}; }}\n\
                 window {{ background-color: {bg}; border-color: {accent}; }}\n\
                 inputbar {{ background-color: {bg_alt}; }}\n\
                 prompt {{ background-color: {accent}; text-color: {accent_fg}; }}\n\
                 entry {{ text-color: {fg}; }}\n\
                 textbox {{ text-color: {fg}; }}\n\
                 element {{ background-color: {bg_alt}; text-color: {fg}; }}\n\
                 element selected.normal {{ background-color: {accent}; border-color: {accent}; text-color: {accent_fg}; }}"
            ));
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }
}

fn smaller_font_spec(font: &str, delta: u32) -> String {
    if let Some((head, tail)) = font.rsplit_once(' ') {
        if let Ok(sz) = tail.parse::<u32>() {
            return format!("{} {}", head.trim(), sz.saturating_sub(delta).max(8));
        }
    }
    font.to_string()
}

/// Parses `#RRGGBB` into normalized `(r, g, b)` floats in `0.0..=1.0`.
pub fn parse_hex_rgb(hex: &str) -> Option<(f32, f32, f32)> {
    let s = hex.trim().strip_prefix('#')?;
    if s.len() != 6 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.0;
    Some((r, g, b))
}

fn contrast_hex_for(hex: &str) -> &'static str {
    if let Some((r, g, b)) = parse_hex_rgb(hex) {
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        if luma > 0.45 {
            "#13161f"
        } else {
            "#eef1f7"
        }
    } else {
        "#13161f"
    }
}

fn is_hex_color(s: &str) -> bool {
    parse_hex_rgb(s).is_some()
}

fn is_valid_wh_categories(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b == b'0' || b == b'1') && s != "000"
}

fn apply_rasi_kv(dst: &mut RasiPluginConfig, key: &str, val: &str) {
    match key {
        "theme_mode" if matches!(val, "active" | "custom") => dst.theme_mode = val.to_string(),
        "font" if !val.is_empty() => dst.font = val.to_string(),
        "bg" if is_hex_color(val) => dst.bg = val.to_string(),
        "bg_alt" if is_hex_color(val) => dst.bg_alt = val.to_string(),
        "fg" if is_hex_color(val) => dst.fg = val.to_string(),
        "accent" if is_hex_color(val) => dst.accent = val.to_string(),
        _ => {}
    }
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (i, ch) in line.char_indices() {
        if ch == '"' {
            in_quotes = !in_quotes;
        } else if ch == '#' && !in_quotes {
            return &line[..i];
        }
    }
    line
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn json_esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_widgets_toml_cleanly() {
        let mut cfg = WidgetsConfig::default();
        cfg.games.theme_mode = "custom".to_string();
        cfg.games.font = "JetBrains Mono".to_string();
        cfg.games.accent = "#ff5577".to_string();
        cfg.games.radius = 12;
        cfg.music.theme_mode = "active".to_string();
        cfg.music.font = "Inter".to_string();
        cfg.clipboard.theme_mode = "custom".to_string();
        cfg.clipboard.accent = "#9ece6a".to_string();
        cfg.wallpaper.wallhaven_query = "cyberpunk city".to_string();
        cfg.wallpaper.wallhaven_categories = "110".to_string();
        cfg.wallpaper.wallhaven_sorting = "toplist".to_string();
        cfg.wallpaper.wallhaven_top_range = "1w".to_string();
        cfg.wallpaper.wallhaven_atleast = "3840x2160".to_string();
        cfg.wallpaper.wallhaven_ratios = "16x9".to_string();
        cfg.wallpaper.wallhaven_color = "424153".to_string();

        let toml = cfg.to_toml();
        let parsed = WidgetsConfig::parse_toml(&toml);
        assert_eq!(parsed, cfg);
        assert!(WidgetsConfig::rasi_override_snippet(&parsed.clipboard)
            .unwrap()
            .contains("#9ece6a"));
    }
}
