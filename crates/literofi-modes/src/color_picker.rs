//! Interactive color palette switcher (`-show colors` / `color` / `palette`):
//! combines curated built-in palettes with any `.rasi` palette files in
//! `%APPDATA%\LiteRofi\colors\` (both `adi1090x` `#hex` and `base16-rofi` `rgba(...)`),
//! writing active colors to `wallust/colors-rofi.rasi` and `launchers/type-*/shared/colors.rasi`
//! with live preview (`Enter`), save (`Alt+A` / `Shift+Enter`), and revert (`Esc`).

use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ColorPalette {
    pub name: String,
    pub bg: String,
    pub bg_alt: String,
    pub fg: String,
    pub selected: String,
    pub active: String,
    pub urgent: String,
    pub border: String,
    pub bg_rgba: String,
}

fn builtin_palettes() -> Vec<ColorPalette> {
    #[allow(clippy::type_complexity)]
    let raw: &[(&str, &str, &str, &str, &str, &str, &str, &str, &str)] = &[
        (
            "Catppuccin Mocha",
            "#1e1e2e",
            "#313244",
            "#cdd6f4",
            "#89b4fa",
            "#a6e3a1",
            "#f38ba8",
            "#89b4fa",
            "rgba(30,30,46,0.88)",
        ),
        (
            "Catppuccin Macchiato",
            "#24273a",
            "#363a4f",
            "#cad3f5",
            "#8aadf4",
            "#a6da95",
            "#ed8796",
            "#c6a0f6",
            "rgba(36,39,58,0.88)",
        ),
        (
            "Catppuccin Frappe",
            "#303446",
            "#414559",
            "#c6d0f5",
            "#8caaee",
            "#a6d189",
            "#e78284",
            "#ca9ee6",
            "rgba(48,52,70,0.88)",
        ),
        (
            "Catppuccin Latte",
            "#eff1f5",
            "#dce0e8",
            "#4c4f69",
            "#1e66f5",
            "#40a02b",
            "#d20f39",
            "#8839ef",
            "rgba(239,241,245,0.92)",
        ),
        (
            "TokyoNight",
            "#1a1b26",
            "#24283b",
            "#c0caf5",
            "#7aa2f7",
            "#9ece6a",
            "#f7768e",
            "#7aa2f7",
            "rgba(26,27,38,0.88)",
        ),
        (
            "Nord",
            "#2e3440",
            "#3b4252",
            "#eceff4",
            "#88c0d0",
            "#a3be8c",
            "#bf616a",
            "#88c0d0",
            "rgba(46,52,64,0.88)",
        ),
        (
            "Dracula",
            "#282a36",
            "#44475a",
            "#f8f8f2",
            "#bd93f9",
            "#50fa7b",
            "#ff5555",
            "#bd93f9",
            "rgba(40,42,54,0.88)",
        ),
        (
            "Gruvbox Dark",
            "#282828",
            "#3c3836",
            "#ebdbb2",
            "#d79921",
            "#98971a",
            "#cc241d",
            "#fabd2f",
            "rgba(40,40,40,0.88)",
        ),
        (
            "OneDark",
            "#1e2127",
            "#282b31",
            "#abb2bf",
            "#61afef",
            "#98c379",
            "#e06c75",
            "#61afef",
            "rgba(30,33,39,0.88)",
        ),
        (
            "Rose Pine",
            "#191724",
            "#26233a",
            "#e0def4",
            "#c4a7e7",
            "#9ccfd8",
            "#eb6f92",
            "#ebbcba",
            "rgba(25,23,36,0.88)",
        ),
        (
            "Everforest",
            "#2d353b",
            "#343f44",
            "#d3c6aa",
            "#a7c080",
            "#83c092",
            "#e67e80",
            "#a7c080",
            "rgba(45,53,59,0.88)",
        ),
        (
            "Kanagawa",
            "#1f1f28",
            "#2a2a37",
            "#dcd7ba",
            "#7e9cd8",
            "#98bb6c",
            "#e46876",
            "#957fb8",
            "rgba(31,31,40,0.88)",
        ),
        (
            "JaKooLit Purple",
            "#181519",
            "#23152d",
            "#fae8e1",
            "#cc659a",
            "#784ca0",
            "#914b4b",
            "#784ca0",
            "rgba(24,21,25,0.82)",
        ),
        (
            "Windows 11 Dark",
            "#202020",
            "#2c2c2c",
            "#ffffff",
            "#60cdff",
            "#4cc2ff",
            "#ff99a4",
            "#60cdff",
            "rgba(32,32,32,0.82)",
        ),
    ];
    raw.iter()
        .map(|&(n, bg, ba, fg, sel, act, urg, brd, rgba)| ColorPalette {
            name: n.to_string(),
            bg: bg.to_string(),
            bg_alt: ba.to_string(),
            fg: fg.to_string(),
            selected: sel.to_string(),
            active: act.to_string(),
            urgent: urg.to_string(),
            border: brd.to_string(),
            bg_rgba: rgba.to_string(),
        })
        .collect()
}

/// Parses `#RRGGBB[AA]` or `rgba ( r, g, b, ... )` / `rgb(r,g,b)` into `(r, g, b)`.
fn parse_rgb_literal(val: &str) -> Option<(u8, u8, u8)> {
    let v = val.trim().trim_end_matches(';').trim();
    if let Some(hex) = v.strip_prefix('#') {
        if hex.len() >= 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some((r, g, b));
        }
        return None;
    }
    let lower = v.to_ascii_lowercase();
    if lower.starts_with("rgb") {
        let start = v.find('(')?;
        let end = v.rfind(')')?;
        let parts: Vec<&str> = v[start + 1..end].split(',').collect();
        if parts.len() >= 3 {
            let r = parts[0].trim().parse::<u8>().ok()?;
            let g = parts[1].trim().parse::<u8>().ok()?;
            let b = parts[2].trim().parse::<u8>().ok()?;
            return Some((r, g, b));
        }
    }
    None
}

fn rgb_to_hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Parses a `.rasi` color file (`adi1090x` or `base16-rofi` format) into a [`ColorPalette`].
fn parse_palette_rasi_file(path: &Path) -> Option<ColorPalette> {
    let stem = path.file_stem()?.to_str()?;
    let content = std::fs::read_to_string(path).ok()?;
    let mut vars: HashMap<String, String> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with('{')
            || trimmed.starts_with('}')
        {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v
                .split("//")
                .next()
                .unwrap_or("")
                .trim()
                .trim_end_matches(';')
                .trim()
                .to_string();
            if !key.is_empty() && !val.is_empty() {
                vars.insert(key, val);
            }
        }
    }

    let resolve_rgb = |keys: &[&str]| -> Option<(u8, u8, u8)> {
        for &k in keys {
            let mut cur = vars.get(k).cloned();
            for _ in 0..8 {
                let Some(val) = cur else { break };
                if let Some(rgb) = parse_rgb_literal(&val) {
                    return Some(rgb);
                }
                if let Some(link) = val.strip_prefix('@') {
                    cur = vars.get(&link.trim().to_ascii_lowercase()).cloned();
                } else {
                    break;
                }
            }
        }
        None
    };

    let bg_rgb = resolve_rgb(&["background", "normal-background", "base"])?;
    let fg_rgb = resolve_rgb(&["foreground", "normal-foreground", "text"])?;
    let bg_alt_rgb = resolve_rgb(&["background-alt", "lightbg", "surface0"]).unwrap_or(bg_rgb);
    let sel_rgb = resolve_rgb(&[
        "selected",
        "blue",
        "selected-active-background",
        "selected-normal-background",
    ])
    .unwrap_or(fg_rgb);
    let act_rgb = resolve_rgb(&["active", "green", "blue", "active-foreground"]).unwrap_or(sel_rgb);
    let urg_rgb = resolve_rgb(&["urgent", "red", "urgent-foreground"]).unwrap_or(sel_rgb);

    let pretty_name = stem
        .strip_prefix("base16-")
        .unwrap_or(stem)
        .replace('-', " ");

    Some(ColorPalette {
        name: pretty_name,
        bg: rgb_to_hex(bg_rgb),
        bg_alt: rgb_to_hex(bg_alt_rgb),
        fg: rgb_to_hex(fg_rgb),
        selected: rgb_to_hex(sel_rgb),
        active: rgb_to_hex(act_rgb),
        urgent: rgb_to_hex(urg_rgb),
        border: rgb_to_hex(sel_rgb),
        bg_rgba: format!("rgba({},{},{},0.88)", bg_rgb.0, bg_rgb.1, bg_rgb.2),
    })
}

fn format_wallust_rasi(p: &ColorPalette) -> String {
    format!(
        "/* LiteRofi Color Palette: {name} */\n\
* {{\n\
    active-background: {active};\n\
    active-foreground: {fg};\n\
    normal-background: {bg};\n\
    normal-foreground: {fg};\n\
    urgent-background: {urgent};\n\
    urgent-foreground: {fg};\n\
    alternate-active-background: {bg_alt};\n\
    alternate-active-foreground: {fg};\n\
    alternate-normal-background: {bg};\n\
    alternate-normal-foreground: {fg};\n\
    alternate-urgent-background: {bg};\n\
    alternate-urgent-foreground: {fg};\n\
    selected-active-background: {selected};\n\
    selected-active-foreground: {bg};\n\
    selected-normal-background: {selected};\n\
    selected-normal-foreground: {bg};\n\
    selected-urgent-background: {urgent};\n\
    selected-urgent-foreground: {bg};\n\
    background-color: {bg};\n\
    background: {bg_rgba};\n\
    background-alt: {bg_alt};\n\
    foreground: {fg};\n\
    selected: {selected};\n\
    active: {active};\n\
    urgent: {urgent};\n\
    border-color: {border};\n\
    color0: {bg};\n\
    color1: {bg_alt};\n\
    color2: {bg_alt};\n\
    color3: {urgent};\n\
    color4: {selected};\n\
    color5: {border};\n\
    color6: {active};\n\
    color7: {fg};\n\
    color8: {bg_alt};\n\
    color9: {bg_alt};\n\
    color10: {active};\n\
    color11: {active};\n\
    color12: {selected};\n\
    color13: {urgent};\n\
    color14: {fg};\n\
    color15: {fg};\n\
}}\n",
        name = p.name,
        bg = p.bg,
        bg_alt = p.bg_alt,
        fg = p.fg,
        selected = p.selected,
        active = p.active,
        urgent = p.urgent,
        border = p.border,
        bg_rgba = p.bg_rgba,
    )
}

fn format_adi_colors_rasi(p: &ColorPalette) -> String {
    format!(
        "/* LiteRofi Color Palette: {name} */\n\
* {{\n\
    background:     {bg}FF;\n\
    background-alt: {bg_alt}FF;\n\
    foreground:     {fg}FF;\n\
    selected:       {selected}FF;\n\
    active:         {active}FF;\n\
    urgent:         {urgent}FF;\n\
}}\n",
        name = p.name,
        bg = p.bg,
        bg_alt = p.bg_alt,
        fg = p.fg,
        selected = p.selected,
        active = p.active,
        urgent = p.urgent,
    )
}

/// Interactive color palette switcher supporting live preview (`Enter`), save (`Alt+A`/`Shift+Enter`),
/// and revert (`Esc`).
pub struct ColorMode {
    display: String,
    config_root: PathBuf,
    palettes: Vec<ColorPalette>,
    previewed: Option<String>,
    backups: Vec<(PathBuf, Option<String>)>,
}

impl ColorMode {
    pub fn new(cfg: &Config) -> Self {
        let config_root = cfg
            .config_file
            .as_ref()
            .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
            .unwrap_or_else(default_config_root);
        Self::with_root(config_root)
    }

    pub fn with_root(config_root: PathBuf) -> Self {
        Self {
            display: "colors".to_string(),
            config_root,
            palettes: Vec::new(),
            previewed: None,
            backups: Vec::new(),
        }
    }

    fn target_paths(&self) -> (PathBuf, Vec<PathBuf>) {
        let wallust = self.config_root.join("wallust").join("colors-rofi.rasi");
        let mut adi_paths = Vec::new();
        let launchers = self.config_root.join("launchers");
        if let Ok(rd) = std::fs::read_dir(&launchers) {
            for ent in rd.flatten() {
                let shared = ent.path().join("shared").join("colors.rasi");
                if shared.is_file() {
                    adi_paths.push(shared);
                }
            }
        }
        (wallust, adi_paths)
    }

    fn snapshot_backups_once(&mut self) {
        if !self.backups.is_empty() {
            return;
        }
        let (wallust, adi_paths) = self.target_paths();
        self.backups
            .push((wallust.clone(), std::fs::read_to_string(&wallust).ok()));
        for p in adi_paths {
            self.backups
                .push((p.clone(), std::fs::read_to_string(&p).ok()));
        }
    }

    fn restore_backups(&mut self) {
        for (path, orig) in self.backups.drain(..) {
            if let Some(content) = orig {
                let _ = std::fs::write(&path, content);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
        self.previewed = None;
    }

    fn write_palette(&mut self, palette: &ColorPalette) -> Result<(), String> {
        self.snapshot_backups_once();
        apply_palette_to_root(&self.config_root, palette)
    }
}

/// Applies `palette` directly to `wallust/colors-rofi.rasi` and any `launchers/*/shared/colors.rasi`
/// under `config_root` (used by both `ColorMode` and `WallpaperMode`'s Auto-Theme action).
pub fn apply_palette_to_root(config_root: &Path, palette: &ColorPalette) -> Result<(), String> {
    let wallust = config_root.join("wallust").join("colors-rofi.rasi");
    if let Some(parent) = wallust.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(&wallust, format_wallust_rasi(palette))
        .map_err(|e| format!("failed to write {}: {e}", wallust.display()))?;

    let adi_text = format_adi_colors_rasi(palette);
    let launchers = config_root.join("launchers");
    if let Ok(rd) = std::fs::read_dir(&launchers) {
        for ent in rd.flatten() {
            let shared = ent.path().join("shared").join("colors.rasi");
            if shared.is_file() {
                let _ = std::fs::write(&shared, &adi_text);
            }
        }
    }

    sync_wallust_templates(config_root, palette);
    Ok(())
}

/// Renders Wallust/Pywal-style color artifacts (`colors.json`, `vscode-colors.json`,
/// `windows-terminal-scheme.json`) and expands any user templates under
/// `%APPDATA%\LiteRofi\templates\` (`{{bg}}`, `{{bg_alt}}`, `{{fg}}`, `{{accent}}`,
/// `{{active}}`, `{{urgent}}`, `{{border}}`) into `%APPDATA%\LiteRofi\generated\`.
pub fn sync_wallust_templates(config_root: &Path, p: &ColorPalette) {
    let gen_dir = config_root.join("generated");
    if std::fs::create_dir_all(&gen_dir).is_err() {
        return;
    }

    // 1. Pywal/Wallust-compatible colors.json
    let colors_json = format!(
        "{{\n  \"name\": \"{}\",\n  \"special\": {{\n    \"background\": \"{}\",\n    \"backgroundAlt\": \"{}\",\n    \"foreground\": \"{}\",\n    \"accent\": \"{}\",\n    \"active\": \"{}\",\n    \"urgent\": \"{}\",\n    \"border\": \"{}\"\n  }}\n}}\n",
        p.name.replace('"', "\\\""),
        p.bg,
        p.bg_alt,
        p.fg,
        p.selected,
        p.active,
        p.urgent,
        p.border
    );
    let _ = std::fs::write(gen_dir.join("colors.json"), colors_json);

    // 2. Ready-to-use VS Code workbench.colorCustomizations snippet
    let vscode_json = format!(
        "{{\n  \"workbench.colorCustomizations\": {{\n    \"editor.background\": \"{}\",\n    \"sideBar.background\": \"{}\",\n    \"activityBar.background\": \"{}\",\n    \"statusBar.background\": \"{}\",\n    \"focusBorder\": \"{}\",\n    \"editor.foreground\": \"{}\"\n  }}\n}}\n",
        p.bg, p.bg_alt, p.bg, p.selected, p.selected, p.fg
    );
    let _ = std::fs::write(gen_dir.join("vscode-colors.json"), vscode_json);

    // 3. Ready-to-use Windows Terminal color scheme
    let wt_json = format!(
        "{{\n  \"name\": \"LiteRofi\",\n  \"background\": \"{}\",\n  \"foreground\": \"{}\",\n  \"cursorColor\": \"{}\",\n  \"selectionBackground\": \"{}\",\n  \"black\": \"{}\",\n  \"red\": \"{}\",\n  \"green\": \"{}\",\n  \"blue\": \"{}\",\n  \"purple\": \"{}\",\n  \"white\": \"{}\"\n}}\n",
        p.bg, p.fg, p.selected, p.bg_alt, p.bg, p.urgent, p.active, p.selected, p.border, p.fg
    );
    let _ = std::fs::write(gen_dir.join("windows-terminal-scheme.json"), wt_json);

    // 4. Ready-to-use Limusic matugen.css (also synced live to %APPDATA%\limusic\matugen.css if present)
    let limusic_css = format_limusic_matugen_css(p);
    let _ = std::fs::write(gen_dir.join("limusic-matugen.css"), &limusic_css);
    if let Ok(appdata) = std::env::var("APPDATA") {
        let limusic_dir = PathBuf::from(appdata).join("limusic");
        if limusic_dir.is_dir() {
            let _ = std::fs::write(limusic_dir.join("matugen.css"), &limusic_css);
        }
    }

    // 5. Expand custom user templates in %APPDATA%\LiteRofi\templates\*
    let tpl_dir = config_root.join("templates");
    if let Ok(rd) = std::fs::read_dir(&tpl_dir) {
        for ent in rd.flatten() {
            let path = ent.path();
            if !path.is_file() {
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(&path) {
                let expanded = expand_palette_template(&raw, p);
                if let Some(fname) = path.file_name() {
                    let _ = std::fs::write(gen_dir.join(fname), expanded);
                }
            }
        }
    }
}

fn darken_hex(hex: &str, amount: f32) -> String {
    let s = hex.trim().trim_start_matches('#');
    if s.len() != 6 {
        return hex.to_string();
    }
    let (Ok(r), Ok(g), Ok(b)) = (
        u8::from_str_radix(&s[0..2], 16),
        u8::from_str_radix(&s[2..4], 16),
        u8::from_str_radix(&s[4..6], 16),
    ) else {
        return hex.to_string();
    };
    let factor = (1.0 - amount).clamp(0.0, 1.0);
    format!(
        "#{:02x}{:02x}{:02x}",
        (f32::from(r) * factor).round() as u8,
        (f32::from(g) * factor).round() as u8,
        (f32::from(b) * factor).round() as u8,
    )
}

/// Generates a Limusic-compatible `matugen.css` stylesheet from the active LiteRofi palette.
pub fn format_limusic_matugen_css(p: &ColorPalette) -> String {
    let card = darken_hex(&p.bg, 0.06);
    let surface_low = darken_hex(&p.bg, 0.03);
    let popover = darken_hex(&p.bg, 0.12);
    let muted_text = darken_hex(&p.fg, 0.30);
    let border = darken_hex(&p.fg, 0.40);
    let outline = darken_hex(&p.fg, 0.30);

    let dark_bg = darken_hex(&p.bg, 0.35);
    let dark_border = darken_hex(&dark_bg, 0.15);
    let dark_outline = darken_hex(&dark_bg, 0.10);
    let dark_card = darken_hex(&dark_bg, 0.08);
    let dark_popover = darken_hex(&dark_bg, 0.14);
    let dark_muted_text = darken_hex(&p.fg, 0.50);
    let dark_secondary = darken_hex(&p.border, 0.20);

    format!(
        "/* LiteRofi Wallust -> Limusic theme ({name}) */\n\
html.matugen-theme {{\n\
\t--background: {bg};\n\
\t--foreground: {fg};\n\
\t--card: {card};\n\
\t--card-foreground: {fg};\n\
\t--popover: {popover};\n\
\t--popover-foreground: {fg};\n\
\t--primary: {primary};\n\
\t--primary-foreground: {bg};\n\
\t--accent: {primary};\n\
\t--accent-foreground: {bg};\n\
\t--secondary: {secondary};\n\
\t--secondary-foreground: {fg};\n\
\t--muted: {surface_low};\n\
\t--muted-foreground: {muted_text};\n\
\t--destructive: {urgent};\n\
\t--destructive-foreground: {bg};\n\
\t--border: {border};\n\
\t--input: {border};\n\
\t--outline: {outline};\n\
\t--ring: {primary};\n\
\t--chart-1: {primary};\n\
\t--chart-2: {secondary};\n\
\t--chart-3: {active};\n\
\t--chart-4: {urgent};\n\
\t--chart-5: {outline};\n\
\t--sidebar: {bg};\n\
\t--sidebar-foreground: {fg};\n\
\t--sidebar-primary: {primary};\n\
\t--sidebar-primary-foreground: {bg};\n\
\t--sidebar-accent: {card};\n\
\t--sidebar-accent-foreground: {fg};\n\
\t--sidebar-border: {border};\n\
\t--sidebar-ring: {primary};\n\
}}\n\n\
html.matugen-theme.dark {{\n\
\t--background: {dark_bg};\n\
\t--foreground: {fg};\n\
\t--card: {dark_card};\n\
\t--card-foreground: {fg};\n\
\t--popover: {dark_popover};\n\
\t--popover-foreground: {fg};\n\
\t--primary: {primary};\n\
\t--primary-foreground: {dark_bg};\n\
\t--accent: {primary};\n\
\t--accent-foreground: {dark_bg};\n\
\t--secondary: {dark_secondary};\n\
\t--secondary-foreground: {fg};\n\
\t--muted: {dark_card};\n\
\t--muted-foreground: {dark_muted_text};\n\
\t--destructive: {urgent};\n\
\t--destructive-foreground: {dark_bg};\n\
\t--border: {dark_border};\n\
\t--input: {dark_border};\n\
\t--outline: {dark_outline};\n\
\t--ring: {primary};\n\
\t--chart-1: {primary};\n\
\t--chart-2: {dark_secondary};\n\
\t--chart-3: {active};\n\
\t--chart-4: {urgent};\n\
\t--chart-5: {dark_outline};\n\
\t--sidebar: {dark_bg};\n\
\t--sidebar-foreground: {fg};\n\
\t--sidebar-primary: {primary};\n\
\t--sidebar-primary-foreground: {dark_bg};\n\
\t--sidebar-accent: {dark_card};\n\
\t--sidebar-accent-foreground: {fg};\n\
\t--sidebar-border: {dark_border};\n\
\t--sidebar-ring: {primary};\n\
}}\n",
        name = p.name,
        bg = p.bg,
        fg = p.fg,
        primary = p.selected,
        secondary = p.border,
        active = p.active,
        urgent = p.urgent,
    )
}

pub fn expand_palette_template(template: &str, p: &ColorPalette) -> String {
    template
        .replace("{{bg}}", &p.bg)
        .replace("{{bg_alt}}", &p.bg_alt)
        .replace("{{fg}}", &p.fg)
        .replace("{{accent}}", &p.selected)
        .replace("{{selected}}", &p.selected)
        .replace("{{active}}", &p.active)
        .replace("{{urgent}}", &p.urgent)
        .replace("{{border}}", &p.border)
        .replace("{{bg_rgba}}", &p.bg_rgba)
        .replace("{{name}}", &p.name)
}

fn default_config_root() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return PathBuf::from(appdata).join("LiteRofi");
        }
    }
    PathBuf::from(".")
}

impl RichMode for ColorMode {
    fn name(&self) -> &str {
        "colors"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        let mut ordered = builtin_palettes();
        let mut seen: std::collections::HashSet<String> = ordered
            .iter()
            .map(|p| p.name.to_ascii_lowercase())
            .collect();
        let mut extras: BTreeMap<String, ColorPalette> = BTreeMap::new();
        let colors_dir = self.config_root.join("colors");
        if let Ok(rd) = std::fs::read_dir(&colors_dir) {
            for ent in rd.flatten() {
                let path = ent.path();
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("rasi"))
                {
                    if let Some(p) = parse_palette_rasi_file(&path) {
                        let key = p.name.to_ascii_lowercase();
                        if !seen.contains(&key) {
                            seen.insert(key.clone());
                            extras.insert(key, p);
                        }
                    }
                }
            }
        }
        ordered.extend(extras.into_values());
        self.palettes = ordered;
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.palettes
            .iter()
            .map(|p| ListEntry {
                text: p.name.clone(),
                meta: Some(format!("{} {} {}", p.bg, p.selected, p.active)),
                ..Default::default()
            })
            .collect()
    }

    fn message(&self) -> Option<String> {
        if let Some(name) = &self.previewed {
            Some(format!(
                "Palette: {name}  |  Enter: Preview  |  Alt+A: Save  |  Shift+Left/Ctrl+Tab: Switch tab  |  Esc: Revert"
            ))
        } else {
            Some(
                "Enter: Preview  |  Alt+A / Shift+Enter: Save  |  Shift+Left/Ctrl+Tab: Switch tab  |  Esc: Close"
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
                if let Some(p) = self.palettes.get(*index).cloned() {
                    self.write_palette(&p)?;
                    if *exit_code == 0 {
                        self.previewed = Some(p.name);
                        return Ok(ModeStep::ReloadTheme);
                    }
                }
                self.backups.clear();
                self.previewed = None;
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                if let Some(p) = self.palettes.get(*index).cloned() {
                    self.write_palette(&p)?;
                }
                self.backups.clear();
                self.previewed = None;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::Cancelled => {
                self.restore_backups();
                Ok(ModeStep::Exit(1))
            }
            UiOutcome::ModeSwitch(idx) => {
                self.backups.clear();
                self.previewed = None;
                Ok(ModeStep::SwitchModeIndex(*idx))
            }
            _ => Ok(ModeStep::Exit(0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_wallust_templates_generates_limusic_matugen_css() {
        let dir = std::env::temp_dir().join(format!("literofi-limusic-css-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = ColorPalette {
            name: "Rembrandt Dark".into(),
            bg: "#080706".into(),
            bg_rgba: "rgba(8,7,6,0.96)".into(),
            bg_alt: "#141210".into(),
            fg: "#d8ccb8".into(),
            selected: "#c8a97e".into(),
            active: "#b5bd68".into(),
            urgent: "#cc6666".into(),
            border: "#a89984".into(),
        };
        sync_wallust_templates(&dir, &p);
        let css = std::fs::read_to_string(dir.join("generated").join("limusic-matugen.css"))
            .expect("limusic-matugen.css generated");
        assert!(css.contains("html.matugen-theme {"));
        assert!(css.contains("html.matugen-theme.dark {"));
        assert!(css.contains("--primary: #c8a97e;"));
        assert!(css.contains("--foreground: #d8ccb8;"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
