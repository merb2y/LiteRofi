use crate::i18n::Lang;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug)]
pub struct StudioPaths {
    pub config_root: PathBuf,
    pub themes_dir: PathBuf,
    pub colors_dir: PathBuf,
    pub config_rasi: PathBuf,
    pub favorites_file: PathBuf,
    pub widgets_toml: PathBuf,
    pub cache_dir: PathBuf,
    pub prefs_file: PathBuf,
}

impl StudioPaths {
    pub fn detect() -> Self {
        let appdata =
            env_path("APPDATA").unwrap_or_else(|| home_dir().join("AppData").join("Roaming"));
        let localappdata =
            env_path("LOCALAPPDATA").unwrap_or_else(|| home_dir().join("AppData").join("Local"));
        let config_root = appdata.join("LiteRofi");
        Self {
            themes_dir: config_root.join("themes"),
            colors_dir: config_root.join("colors"),
            config_rasi: config_root.join("config.rasi"),
            favorites_file: config_root.join("studio-favorites.json"),
            widgets_toml: config_root.join("widgets.toml"),
            prefs_file: config_root.join("studio-prefs.conf"),
            cache_dir: localappdata.join("LiteRofi").join("studio-cache"),
            config_root,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    Az,
    Za,
    Newest,
    Oldest,
}

impl SortOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            SortOrder::Az => "az",
            SortOrder::Za => "za",
            SortOrder::Newest => "newest",
            SortOrder::Oldest => "oldest",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "za" => SortOrder::Za,
            "newest" => SortOrder::Newest,
            "oldest" => SortOrder::Oldest,
            _ => SortOrder::Az,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavLayout {
    Top,
    Left,
}

impl NavLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            NavLayout::Top => "top",
            NavLayout::Left => "left",
        }
    }

    pub fn from_str(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("left") {
            NavLayout::Left
        } else {
            NavLayout::Top
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            NavLayout::Top => NavLayout::Left,
            NavLayout::Left => NavLayout::Top,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StudioPrefs {
    pub lang: Lang,
    pub sort: SortOrder,
    pub compact: bool,
    pub nav_layout: NavLayout,
}

impl Default for StudioPrefs {
    fn default() -> Self {
        Self {
            lang: Lang::Tr,
            sort: SortOrder::Az,
            compact: false,
            nav_layout: NavLayout::Top,
        }
    }
}

impl StudioPrefs {
    pub fn load(path: &Path) -> Self {
        let mut prefs = Self::default();
        let Ok(text) = std::fs::read_to_string(path) else {
            return prefs;
        };
        for line in text.lines() {
            let Some((k, v)) = line.trim().split_once('=') else {
                continue;
            };
            let key = k.trim();
            let val = v.trim();
            match key {
                "lang" => prefs.lang = Lang::from_code(val),
                "sort" => prefs.sort = SortOrder::from_str(val),
                "compact" => prefs.compact = val == "true" || val == "1",
                "nav_layout" => prefs.nav_layout = NavLayout::from_str(val),
                _ => {}
            }
        }
        prefs
    }

    pub fn save(&self, path: &Path) {
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let body = format!(
            "lang={}\nsort={}\ncompact={}\nnav_layout={}\n",
            self.lang.code(),
            self.sort.as_str(),
            self.compact,
            self.nav_layout.as_str()
        );
        let _ = std::fs::write(path, body);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub name: String,
    pub bg: String,
    pub bg_alt: String,
    pub fg: String,
    pub selected: String,
    pub active: String,
    pub urgent: String,
    pub border: String,
    pub mtime: f64,
    pub is_custom: bool,
}

#[derive(Clone, Debug)]
pub struct ThemeEntry {
    pub name: String,
    pub category: String,
    pub has_preview: bool,
    pub is_favorite: bool,
    pub mtime: f64,
}

pub struct LivePreviewProc {
    child: Mutex<Option<Child>>,
}

impl LivePreviewProc {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }

    pub fn close(&self) {
        let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut ch) = guard.take() {
            if ch.try_wait().ok().flatten().is_none() {
                let _ = ch.kill();
                let _ = ch.wait();
            }
        }
    }

    pub fn launch_theme(&self, paths: &StudioPaths, theme_name: &str) -> Result<(), String> {
        let exe = find_literofi_exe().ok_or_else(|| "literofi.exe bulunamadi".to_string())?;
        let rasi = paths.themes_dir.join(format!("{theme_name}.rasi"));
        if !rasi.is_file() {
            return Err(format!("Tema bulunamadi: {theme_name}"));
        }
        self.close();
        let mut cmd = Command::new(exe);
        cmd.args(["-show", "drun", "-show-icons", "-theme"])
            .arg(&rasi)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let child = cmd
            .spawn()
            .map_err(|e| format!("literofi baslatilamadi: {e}"))?;
        let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(child);
        Ok(())
    }

    pub fn launch_widget_mode(&self, mode: &str) -> Result<(), String> {
        let exe = find_literofi_exe().ok_or_else(|| "literofi.exe bulunamadi".to_string())?;
        self.close();
        let theme_arg: Option<&str> = match mode {
            "clipboard" => Some("clipboard"),
            "control" => Some("control-center"),
            "power" => Some("powermenu"),
            _ => None,
        };
        let mut cmd = Command::new(exe);
        cmd.arg("-show").arg(mode);
        if let Some(th) = theme_arg {
            cmd.arg("-theme").arg(th);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let child = cmd
            .spawn()
            .map_err(|e| format!("literofi baslatilamadi: {e}"))?;
        let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(child);
        Ok(())
    }
}

impl Default for LivePreviewProc {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LivePreviewProc {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

pub fn home_dir() -> PathBuf {
    env_path("USERPROFILE")
        .or_else(|| env_path("HOME"))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn find_literofi_exe() -> Option<PathBuf> {
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            let cand = dir.join("literofi.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let local =
        env_path("LOCALAPPDATA").unwrap_or_else(|| home_dir().join("AppData").join("Local"));
    let installed = local.join("LiteRofi").join("literofi.exe");
    if installed.is_file() {
        return Some(installed);
    }
    None
}

fn file_mtime_secs(path: &Path) -> f64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn builtin_palettes() -> &'static [Palette] {
    static BUILTINS: OnceLock<Vec<Palette>> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        let raw = [
            (
                "Catppuccin Mocha",
                "#1e1e2e",
                "#313244",
                "#cdd6f4",
                "#89b4fa",
                "#a6e3a1",
                "#f38ba8",
                "#89b4fa",
            ),
            (
                "Catppuccin Macchiato",
                "#24273a",
                "#363a4f",
                "#cad3f5",
                "#c6a0f6",
                "#a6da95",
                "#ed8796",
                "#c6a0f6",
            ),
            (
                "Catppuccin Latte",
                "#eff1f5",
                "#ccd0da",
                "#4c4f69",
                "#1e66f5",
                "#40a02b",
                "#d20f39",
                "#1e66f5",
            ),
            (
                "Tokyo Night",
                "#1a1b26",
                "#24283b",
                "#c0caf5",
                "#7aa2f7",
                "#9ece6a",
                "#f7768e",
                "#7aa2f7",
            ),
            (
                "Dracula", "#282a36", "#44475a", "#f8f8f2", "#bd93f9", "#50fa7b", "#ff5555",
                "#bd93f9",
            ),
            (
                "Nord", "#2e3440", "#3b4252", "#eceff4", "#88c0d0", "#a3be8c", "#bf616a", "#88c0d0",
            ),
            (
                "Gruvbox Dark",
                "#282828",
                "#3c3836",
                "#ebdbb2",
                "#fabd2f",
                "#b8bb26",
                "#fb4934",
                "#fabd2f",
            ),
            (
                "Rose Pine",
                "#191724",
                "#26233a",
                "#e0def4",
                "#c4a7e7",
                "#9ccfd8",
                "#eb6f92",
                "#c4a7e7",
            ),
            (
                "Everforest Dark",
                "#2d353b",
                "#3d484d",
                "#d3c6aa",
                "#a7c080",
                "#83c092",
                "#e67e80",
                "#a7c080",
            ),
            (
                "One Dark", "#282c34", "#353b45", "#abb2bf", "#61afef", "#98c379", "#e06c75",
                "#61afef",
            ),
            (
                "Kanagawa", "#1f1f28", "#2a2a37", "#dcd7ba", "#7e9cd8", "#98bb6c", "#c34043",
                "#7e9cd8",
            ),
            (
                "Solarized Dark",
                "#002b36",
                "#073642",
                "#eee8d5",
                "#268bd2",
                "#859900",
                "#dc322f",
                "#268bd2",
            ),
            (
                "Cyberpunk Neon",
                "#0d0f1a",
                "#1a1f36",
                "#f0f4ff",
                "#ff2a85",
                "#00f0ff",
                "#ffe600",
                "#00f0ff",
            ),
            (
                "Monochrome OLED",
                "#09090b",
                "#18181b",
                "#fafafa",
                "#e4e4e7",
                "#a1a1aa",
                "#ef4444",
                "#52525b",
            ),
        ];
        raw.into_iter()
            .map(
                |(name, bg, bg_alt, fg, selected, active, urgent, border)| Palette {
                    name: name.to_string(),
                    bg: bg.to_string(),
                    bg_alt: bg_alt.to_string(),
                    fg: fg.to_string(),
                    selected: selected.to_string(),
                    active: active.to_string(),
                    urgent: urgent.to_string(),
                    border: border.to_string(),
                    mtime: 0.0,
                    is_custom: false,
                },
            )
            .collect()
    })
}

pub fn classify_theme(name: &str) -> &'static str {
    let low = name.to_ascii_lowercase();
    if low.starts_with("lab-") {
        "Lab"
    } else if low.starts_with("kool_") {
        "KooL"
    } else if low.starts_with("adi_") {
        "adi1090x"
    } else if low.starts_with("gh0stzk_") {
        "gh0stzk"
    } else if low.starts_with("ml4w_") {
        "ML4W"
    } else if low.contains("catppuccin") {
        "Catppuccin"
    } else if matches!(low.as_str(), "windows95" | "cyberpunk-hud" | "c64")
        || low.starts_with("breaking-")
    {
        "Retro"
    } else if [
        "rounded-",
        "squared-",
        "spotlight",
        "launchpad",
        "windows11",
        "nord",
        "dracula",
        "rose-pine",
        "tokyonight",
        "saint-",
        "mango-",
    ]
    .iter()
    .any(|p| low.starts_with(p))
    {
        "Modern"
    } else {
        "Classic"
    }
}

pub fn read_active_theme(config_rasi: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(config_rasi) else {
        return String::new();
    };
    let clean = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    for line in clean.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("@theme") {
            let rest = rest.trim();
            if let Some(q1) = rest.find('"') {
                if let Some(q2) = rest[q1 + 1..].find('"') {
                    let raw = &rest[q1 + 1..q1 + 1 + q2];
                    return Path::new(raw)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(raw)
                        .to_string();
                }
            }
        }
    }
    String::new()
}

pub fn read_active_palette(config_root: &Path) -> String {
    let wallust = config_root.join("wallust").join("colors-rofi.rasi");
    let Ok(text) = std::fs::read_to_string(wallust) else {
        return String::new();
    };
    for line in text.lines().take(3) {
        if let Some(idx) = line.find("LiteRofi Color Palette:") {
            let after = &line[idx + "LiteRofi Color Palette:".len()..];
            let trimmed = after.split("*/").next().unwrap_or(after).trim();
            return trimmed.to_string();
        }
    }
    String::new()
}

fn parse_json_string_array(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            let mut s = String::new();
            while let Some(nc) = chars.next() {
                if nc == '\\' {
                    if let Some(esc) = chars.next() {
                        s.push(esc);
                    }
                } else if nc == '"' {
                    break;
                } else {
                    s.push(nc);
                }
            }
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
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

pub fn load_favorites(path: &Path) -> HashSet<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashSet::new();
    };
    parse_json_string_array(&text).into_iter().collect()
}

pub fn save_favorites(path: &Path, favs: &HashSet<String>) {
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let mut sorted: Vec<&String> = favs.iter().collect();
    sorted.sort();
    let mut out = String::from("[\n");
    for (i, item) in sorted.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        out.push_str("  \"");
        out.push_str(&json_escape(item));
        out.push('"');
    }
    out.push_str("\n]\n");
    let _ = std::fs::write(path, out);
}

pub fn studio_parse_options(paths: &StudioPaths) -> literofi_theme::ParseOptions {
    let mut search_dirs = vec![paths.themes_dir.clone(), paths.config_root.clone()];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            search_dirs.push(dir.join("themes"));
        }
    }
    literofi_theme::ParseOptions {
        search_dirs,
        dmenu_mode: false,
    }
}

pub fn is_selectable_theme(path: &Path, opts: &literofi_theme::ParseOptions) -> bool {
    let mut sheet = literofi_theme::ThemeSheet::new();
    match sheet.parse_file(path, opts) {
        Ok(()) => !sheet.root.children.is_empty(),
        Err(_) => false,
    }
}

pub fn list_themes(paths: &StudioPaths) -> Vec<ThemeEntry> {
    let favs = load_favorites(&paths.favorites_file);
    let _ = std::fs::create_dir_all(&paths.cache_dir);
    let parse_opts = studio_parse_options(paths);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&paths.themes_dir) {
        for ent in rd.flatten() {
            let path = ent.path();
            if !path.is_file() {
                continue;
            }
            let is_rasi = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("rasi"));
            if !is_rasi || !is_selectable_theme(&path, &parse_opts) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let name = stem.to_string();
            let png = paths.cache_dir.join(format!("{name}.png"));
            let has_preview = std::fs::metadata(&png)
                .map(|m| m.is_file() && m.len() > 256)
                .unwrap_or(false);
            out.push(ThemeEntry {
                category: classify_theme(&name).to_string(),
                is_favorite: favs.contains(&name),
                mtime: file_mtime_secs(&path),
                has_preview,
                name,
            });
        }
    }
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    out
}

pub fn list_installed_fonts() -> Vec<String> {
    let mut set = BTreeSet::new();
    #[cfg(windows)]
    {
        for key in [
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts",
            r"HKCU\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts",
        ] {
            let mut cmd = Command::new("reg.exe");
            cmd.args(["query", key]).creation_flags(CREATE_NO_WINDOW);
            if let Ok(out) = cmd.output() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let trimmed = line.trim();
                    let Some((raw_name, _)) = trimmed.split_once("REG_SZ") else {
                        continue;
                    };
                    let mut name = raw_name.trim().to_string();
                    for paren in ["(TrueType)", "(OpenType)", "(All res)"] {
                        if let Some(stripped) = name.strip_suffix(paren) {
                            name = stripped.trim().to_string();
                        }
                    }
                    for suffix in [
                        " ExtraBold Italic",
                        " ExtraLight Italic",
                        " SemiBold Italic",
                        " Bold Italic",
                        " Medium Italic",
                        " Light Italic",
                        " Thin Italic",
                        " Black Italic",
                        " ExtraBold",
                        " ExtraLight",
                        " SemiBold",
                        " Regular",
                        " Bold",
                        " Italic",
                        " Medium",
                        " Light",
                        " Thin",
                        " Black",
                    ] {
                        if let Some(stripped) = name.strip_suffix(suffix) {
                            name = stripped.trim().to_string();
                        }
                    }
                    if !name.is_empty() && !name.starts_with('@') && !name.contains('&') {
                        set.insert(name);
                    }
                }
            }
        }
    }
    set.into_iter().collect()
}

pub fn parse_hex6(val: &str) -> Option<String> {
    let s = val.trim().trim_end_matches(';').trim();
    if let Some(hex) = s.strip_prefix('#') {
        if (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(format!("#{}", hex[..6].to_ascii_lowercase()));
        }
    }
    let low = s.to_ascii_lowercase();
    if low.starts_with("rgb") {
        let open = s.find('(')?;
        let close = s[open + 1..].find(')')?;
        let inner = &s[open + 1..open + 1 + close];
        let mut nums = inner
            .split(',')
            .filter_map(|part| part.trim().parse::<i32>().ok());
        let r = nums.next()?.clamp(0, 255);
        let g = nums.next()?.clamp(0, 255);
        let b = nums.next()?.clamp(0, 255);
        return Some(format!("#{r:02x}{g:02x}{b:02x}"));
    }
    None
}

pub fn parse_palette_file(path: &Path) -> Option<Palette> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut kv: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("//")
        {
            continue;
        }
        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = v.split(';').next().unwrap_or(v);
        if let Some(hx) = parse_hex6(val) {
            kv.insert(key, hx);
        }
    }
    let bg = kv
        .get("background")
        .or_else(|| kv.get("base00"))
        .or_else(|| kv.get("normal-background"))?
        .clone();
    let fg = kv
        .get("foreground")
        .or_else(|| kv.get("base05"))
        .or_else(|| kv.get("normal-foreground"))?
        .clone();
    let bg_alt = kv
        .get("background-alt")
        .or_else(|| kv.get("lightbg"))
        .or_else(|| kv.get("base01"))
        .or_else(|| kv.get("alternate-normal-background"))
        .cloned()
        .unwrap_or_else(|| bg.clone());
    let selected = kv
        .get("selected")
        .or_else(|| kv.get("selected-normal-background"))
        .or_else(|| kv.get("blue"))
        .or_else(|| kv.get("base0d"))
        .or_else(|| kv.get("lightfg"))
        .cloned()
        .unwrap_or_else(|| fg.clone());
    let active = kv
        .get("active")
        .or_else(|| kv.get("active-background"))
        .or_else(|| kv.get("green"))
        .or_else(|| kv.get("base0b"))
        .or_else(|| kv.get("blue"))
        .cloned()
        .unwrap_or_else(|| selected.clone());
    let urgent = kv
        .get("urgent")
        .or_else(|| kv.get("urgent-background"))
        .or_else(|| kv.get("red"))
        .or_else(|| kv.get("base08"))
        .cloned()
        .unwrap_or_else(|| selected.clone());
    let border = kv
        .get("border-color")
        .cloned()
        .unwrap_or_else(|| selected.clone());

    let stem = path.file_stem()?.to_str()?;
    let stripped = stem.strip_prefix("base16-").unwrap_or(stem);
    let words: Vec<String> = stripped
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), ch.as_str()),
                None => String::new(),
            }
        })
        .collect();
    let mut pretty = words.join(" ");
    if stem.to_ascii_lowercase().starts_with("base16-") {
        pretty = format!("Base16 {pretty}");
    }

    Some(Palette {
        name: pretty,
        bg,
        bg_alt,
        fg,
        selected,
        active,
        urgent,
        border,
        mtime: file_mtime_secs(path),
        is_custom: true,
    })
}

pub fn list_palettes(colors_dir: &Path) -> Vec<Palette> {
    let mut out = builtin_palettes().to_vec();
    let mut seen: HashSet<String> = out.iter().map(|p| p.name.to_ascii_lowercase()).collect();
    let mut extras: BTreeMap<String, Palette> = BTreeMap::new();
    if let Ok(rd) = std::fs::read_dir(colors_dir) {
        for ent in rd.flatten() {
            let path = ent.path();
            let is_rasi = path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("rasi"));
            if !is_rasi {
                continue;
            }
            if let Some(pal) = parse_palette_file(&path) {
                let key = pal.name.to_ascii_lowercase();
                if seen.insert(key.clone()) {
                    extras.insert(key, pal);
                }
            }
        }
    }
    out.extend(extras.into_values());
    out
}

pub fn save_custom_palette(colors_dir: &Path, pal: &Palette) -> Result<Palette, String> {
    let stem = sanitize_theme_stem(&pal.name)?;
    let slug = stem
        .to_ascii_lowercase()
        .replace(' ', "-")
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        return Err("Invalid palette name".to_string());
    }
    std::fs::create_dir_all(colors_dir).map_err(|e| e.to_string())?;
    let file_path = colors_dir.join(format!("{slug}.rasi"));
    let body = format!(
        "/* LiteRofi Custom Palette: {} */\n\
         * {{\n\
             background: {};\n\
             background-alt: {};\n\
             foreground: {};\n\
             selected: {};\n\
             active: {};\n\
             urgent: {};\n\
             border-color: {};\n\
         }}\n",
        pal.name.trim(),
        pal.bg,
        pal.bg_alt,
        pal.fg,
        pal.selected,
        pal.active,
        pal.urgent,
        pal.border
    );
    std::fs::write(&file_path, body).map_err(|e| e.to_string())?;
    parse_palette_file(&file_path).ok_or_else(|| "Kaydedilen palet okunamadi".to_string())
}

fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return (30, 30, 46);
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(30);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(30);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(46);
    (r, g, b)
}

pub fn apply_palette(paths: &StudioPaths, p: &Palette) -> Result<(), String> {
    let (r, g, b) = hex_to_rgb(&p.bg);
    let bg_rgba = format!("rgba({r},{g},{b},0.88)");
    let wallust_text = format!(
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
    );
    let wallust_path = paths.config_root.join("wallust").join("colors-rofi.rasi");
    if let Some(parent) = wallust_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&wallust_path, wallust_text).map_err(|e| e.to_string())?;

    let adi_text = format!(
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
    );
    let launchers = paths.config_root.join("launchers");
    if let Ok(rd) = std::fs::read_dir(launchers) {
        for ent in rd.flatten() {
            let shared = ent.path().join("shared").join("colors.rasi");
            if shared.is_file() {
                let _ = std::fs::write(&shared, &adi_text);
            }
        }
    }
    Ok(())
}

pub fn render_preview_png(paths: &StudioPaths, name: &str, force: bool) -> Option<PathBuf> {
    let rasi = paths.themes_dir.join(format!("{name}.rasi"));
    if !rasi.is_file() {
        return None;
    }
    let _ = std::fs::create_dir_all(&paths.cache_dir);
    let out_png = paths.cache_dir.join(format!("{name}.png"));
    if !force
        && std::fs::metadata(&out_png)
            .map(|m| m.is_file() && m.len() > 256)
            .unwrap_or(false)
    {
        return Some(out_png);
    }
    let exe = find_literofi_exe()?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_png = paths.cache_dir.join(format!(".{name}.{stamp}.png"));
    let mut cmd = Command::new(exe);
    cmd.args(["-show", "drun", "-show-icons", "-theme"])
        .arg(&rasi)
        .arg("--dump-png")
        .arg(&tmp_png)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let ok = cmd.status().map(|s| s.success()).unwrap_or(false);
    if ok
        && std::fs::metadata(&tmp_png)
            .map(|m| m.is_file() && m.len() > 256)
            .unwrap_or(false)
    {
        let _ = std::fs::rename(&tmp_png, &out_png).or_else(|_| {
            std::fs::copy(&tmp_png, &out_png)?;
            std::fs::remove_file(&tmp_png)
        });
        return Some(out_png);
    }
    let _ = std::fs::remove_file(&tmp_png);
    std::fs::metadata(&out_png)
        .map(|m| m.is_file() && m.len() > 256)
        .unwrap_or(false)
        .then_some(out_png)
}

pub fn sanitize_theme_stem(raw_name: &str) -> Result<String, String> {
    let trimmed = raw_name.trim();
    let without_ext = trimmed
        .strip_suffix(".rasi")
        .or_else(|| trimmed.strip_suffix(".RASI"))
        .unwrap_or(trimmed);
    let stem: String = without_ext
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if stem.is_empty() || stem.contains("..") {
        return Err("Invalid theme name".to_string());
    }
    Ok(stem)
}

pub fn import_theme_file(
    themes_dir: &Path,
    cache_dir: &Path,
    raw_name: &str,
    content: &str,
) -> Result<String, String> {
    let stem = sanitize_theme_stem(raw_name)?;
    let clean = content.strip_prefix('\u{FEFF}').unwrap_or(content);
    if clean.trim().is_empty() {
        return Err("Theme content cannot be empty".to_string());
    }
    std::fs::create_dir_all(themes_dir)
        .map_err(|e| format!("Failed to create themes directory: {e}"))?;
    let dest = themes_dir.join(format!("{stem}.rasi"));
    std::fs::write(&dest, clean).map_err(|e| format!("Failed to write theme file: {e}"))?;
    let cached_png = cache_dir.join(format!("{stem}.png"));
    if cached_png.exists() {
        let _ = std::fs::remove_file(cached_png);
    }
    Ok(stem)
}

fn copy_theme_assets(src: &Path, dest: &Path) -> std::io::Result<()> {
    for item in std::fs::read_dir(src)? {
        let item = item?;
        let kind = item.file_type()?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if std::fs::symlink_metadata(item.path())?.file_attributes() & 0x400 != 0 {
                continue;
            }
        }
        let target = dest.join(item.file_name());
        if kind.is_dir() && item.file_name() != ".git" {
            std::fs::create_dir(&target)?;
            copy_theme_assets(&item.path(), &target)?;
        } else if kind.is_file() {
            let path = item.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(
                ext.as_str(),
                "rasi"
                    | "rasinc"
                    | "css"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "bmp"
                    | "svg"
                    | "ico"
                    | "ttf"
                    | "otf"
            ) {
                std::fs::copy(path, target)?;
            }
        }
    }
    Ok(())
}

pub fn install_theme_folder(themes: &Path, root: &Path, entry: &Path) -> Result<String, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let entry = entry.canonicalize().map_err(|e| e.to_string())?;
    let relative = entry
        .strip_prefix(&root)
        .map_err(|_| "Select a .rasi file inside the selected folder")?;
    if !entry
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("rasi"))
    {
        return Err("Select a .rasi file".into());
    }
    let stem = sanitize_theme_stem(
        entry
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid theme name")?,
    )?;
    std::fs::create_dir_all(themes).map_err(|e| e.to_string())?;
    if themes
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&root)
    {
        return Err("Select the downloaded theme folder outside the installation directory".into());
    }
    let packages = themes.join("packages");
    std::fs::create_dir_all(&packages).map_err(|e| e.to_string())?;
    for n in 1.. {
        let name = if n == 1 {
            stem.clone()
        } else {
            format!("{stem}-{n}")
        };
        let wrapper = themes.join(format!("{name}.rasi"));
        if wrapper.exists() {
            continue;
        }
        let dest = packages.join(&name);
        match std::fs::create_dir(&dest) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
            Ok(()) => {}
        }
        let result = (|| {
            copy_theme_assets(&root, &dest).map_err(|e| e.to_string())?;
            literofi_theme::ThemeSheet::new()
                .parse_file(
                    &dest.join(relative),
                    &literofi_theme::ParseOptions {
                        search_dirs: vec![themes.to_owned()],
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())?;
            let rel = relative.to_string_lossy().replace('\\', "/");
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&wrapper)
                .map_err(|e| e.to_string())?;
            if let Err(e) = writeln!(file, "@import \"packages/{name}/{rel}\"") {
                drop(file);
                let _ = std::fs::remove_file(wrapper);
                return Err(e.to_string());
            }
            Ok(name)
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(dest);
        }
        return result;
    }
    unreachable!()
}

pub fn choose_theme_folder(themes: &Path) -> Result<Option<String>, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Windows.Forms
$folder = New-Object Windows.Forms.FolderBrowserDialog
$folder.Description = 'Select the theme package folder'
if ($folder.ShowDialog() -ne 'OK') { exit 0 }
$file = New-Object Windows.Forms.OpenFileDialog
$file.InitialDirectory = $folder.SelectedPath
$file.Title = 'Select the main .rasi file inside the theme folder'
$file.Filter = 'Rofi theme (*.rasi)|*.rasi'
if ($file.ShowDialog() -ne 'OK') { exit 0 }
[Console]::WriteLine($folder.SelectedPath)
[Console]::WriteLine($file.FileName)
"#;
    let mut cmd = Command::new("powershell.exe");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd
        .args(["-NoProfile", "-STA", "-Command", script])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let paths = String::from_utf8(out.stdout).map_err(|e| e.to_string())?;
    let mut lines = paths.lines();
    let Some(root) = lines.next() else {
        return Ok(None);
    };
    let entry = lines.next().ok_or("Missing theme entry")?;
    install_theme_folder(themes, Path::new(root), Path::new(entry)).map(Some)
}

pub fn choose_rasi_files() -> Result<Vec<PathBuf>, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Windows.Forms
$file = New-Object Windows.Forms.OpenFileDialog
$file.Title = 'Select .rasi theme file(s)'
$file.Filter = 'Rofi theme (*.rasi;*.rasinc;*.css)|*.rasi;*.rasinc;*.css|All files (*.*)|*.*'
$file.Multiselect = $true
if ($file.ShowDialog() -ne 'OK') { exit 0 }
foreach ($f in $file.FileNames) {
    [Console]::WriteLine($f)
}
"#;
    let mut cmd = Command::new("powershell.exe");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd
        .args(["-NoProfile", "-STA", "-Command", script])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let text = String::from_utf8(out.stdout).map_err(|e| e.to_string())?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect())
}

pub fn reveal_theme_in_explorer(paths: &StudioPaths, name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("Invalid theme name".to_string());
    }
    let file_path = paths.themes_dir.join(format!("{name}.rasi"));
    if !file_path.is_file() {
        return Err(format!("Tema dosyasi bulunamadi: {}", file_path.display()));
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("explorer.exe");
        cmd.arg(format!("/select,{}", file_path.display()));
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn open_themes_folder(paths: &StudioPaths) -> Result<(), String> {
    let _ = std::fs::create_dir_all(&paths.themes_dir);
    #[cfg(windows)]
    {
        let mut cmd = Command::new("explorer.exe");
        cmd.arg(&paths.themes_dir);
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn open_widgets_config(paths: &StudioPaths) -> Result<(), String> {
    if !paths.widgets_toml.is_file() {
        literofi_core::widgets_config::WidgetsConfig::default().save_to(&paths.widgets_toml)?;
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/C", "start", "", &paths.widgets_toml.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW);
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn clear_preview_cache(cache_dir: &Path) -> usize {
    let mut removed = 0usize;
    if let Ok(rd) = std::fs::read_dir(cache_dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().and_then(|e| e.to_str()) == Some("png")
                && std::fs::remove_file(p).is_ok()
            {
                removed += 1;
            }
        }
    }
    removed
}

pub fn validate_rasi_source(paths: &StudioPaths, source: &str) -> Result<usize, String> {
    let mut sheet = literofi_theme::ThemeSheet::new();
    let opts = studio_parse_options(paths);
    sheet.parse_str(source, &opts).map_err(|e| e.to_string())?;
    Ok(sheet.root.children.len())
}

pub fn extract_rasi_hex_colors(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("/*")
            || trimmed.starts_with('*')
            || trimmed.starts_with("//")
        {
            continue;
        }
        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let val = v.split(';').next().unwrap_or(v);
        if let Some(hx) = parse_hex6(val) {
            let sig = format!("{key}:{hx}");
            if seen.insert(sig) {
                out.push((key.to_string(), hx));
                if out.len() >= 18 {
                    break;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("literofi_studio_{name}_{stamp}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn folder_import_preserves_nested_files_and_rolls_back_broken_imports() {
        let sandbox = temp_dir("folder_import");
        let themes = sandbox.join("themes");
        let pkg = sandbox.join("pkg");
        std::fs::create_dir_all(pkg.join("colors")).unwrap();
        std::fs::create_dir_all(pkg.join("assets")).unwrap();
        std::fs::write(pkg.join("colors/pal.rasi"), "* { bg: #111111; }").unwrap();
        std::fs::write(pkg.join("assets/bg.png"), b"png").unwrap();
        std::fs::write(pkg.join("ignore.exe"), b"bin").unwrap();
        std::fs::write(
            pkg.join("theme.rasi"),
            "@import \"colors/pal.rasi\"\nwindow { background-color: #111111; }",
        )
        .unwrap();
        std::fs::write(pkg.join("broken.rasi"), "window { unclosed").unwrap();

        let err = install_theme_folder(&themes, &pkg, &pkg.join("broken.rasi")).unwrap_err();
        assert!(!err.is_empty());
        assert!(!themes.join("broken.rasi").exists());
        assert!(!themes.join("packages/broken").exists());

        let name = install_theme_folder(&themes, &pkg, &pkg.join("theme.rasi")).unwrap();
        assert_eq!(name, "theme");
        assert!(themes.join("packages/theme/colors/pal.rasi").exists());
        assert!(themes.join("packages/theme/assets/bg.png").exists());
        assert!(!themes.join("packages/theme/ignore.exe").exists());
        let wrapper = std::fs::read_to_string(themes.join("theme.rasi")).unwrap();
        assert_eq!(wrapper.trim(), "@import \"packages/theme/theme.rasi\"");

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn import_theme_sanitizes_name_strips_bom_and_invalidates_cache() {
        let sandbox = temp_dir("single_import");
        let themes = sandbox.join("themes");
        let cache = sandbox.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("my_theme.png"), b"stale_png").unwrap();

        let saved = import_theme_file(
            &themes,
            &cache,
            "my:theme.rasi",
            "\u{FEFF}window { background-color: #111111; }",
        )
        .expect("import should succeed");
        assert_eq!(saved, "my_theme");
        let written = std::fs::read_to_string(themes.join("my_theme.rasi")).unwrap();
        assert!(!written.starts_with('\u{FEFF}'));
        assert!(written.contains("window"));
        assert!(!cache.join("my_theme.png").exists());

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn list_themes_hides_palette_only_and_broken_rasi_files() {
        let sandbox = temp_dir("list_filter");
        let paths = StudioPaths {
            config_root: sandbox.clone(),
            themes_dir: sandbox.join("themes"),
            colors_dir: sandbox.join("colors"),
            config_rasi: sandbox.join("config.rasi"),
            favorites_file: sandbox.join("fav.json"),
            widgets_toml: sandbox.join("widgets.toml"),
            cache_dir: sandbox.join("cache"),
            prefs_file: sandbox.join("prefs.conf"),
        };
        std::fs::create_dir_all(&paths.themes_dir).unwrap();
        std::fs::write(
            paths.themes_dir.join("valid-theme.rasi"),
            "window { background-color: #111111; } listview { lines: 8; }",
        )
        .unwrap();
        std::fs::write(
            paths.themes_dir.join("tokyo-palette-only.rasi"),
            "* { background: #1a1b26; foreground: #c0caf5; }",
        )
        .unwrap();
        std::fs::write(paths.themes_dir.join("broken.rasi"), "window { unclosed").unwrap();

        let listed = list_themes(&paths);
        let names: Vec<&str> = listed.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["valid-theme"]);

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn custom_palette_save_and_parse_roundtrip() {
        let sandbox = temp_dir("palette_roundtrip");
        let colors = sandbox.join("colors");
        let pal = Palette {
            name: "Warm Slate".to_string(),
            bg: "#12100e".to_string(),
            bg_alt: "#1e1a16".to_string(),
            fg: "#e6dbc8".to_string(),
            selected: "#c8a97e".to_string(),
            active: "#b5bd68".to_string(),
            urgent: "#cc6666".to_string(),
            border: "#a89984".to_string(),
            mtime: 0.0,
            is_custom: true,
        };
        let saved = save_custom_palette(&colors, &pal).expect("palette save");
        assert_eq!(saved.bg, "#12100e");
        assert_eq!(saved.selected, "#c8a97e");
        let listed = list_palettes(&colors);
        assert!(listed
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("Warm Slate")));

        let _ = std::fs::remove_dir_all(&sandbox);
    }

    #[test]
    fn studio_prefs_roundtrip_preserves_nav_layout() {
        let sandbox = temp_dir("prefs_roundtrip");
        let prefs_path = sandbox.join("studio-prefs.conf");
        let prefs = StudioPrefs {
            lang: Lang::En,
            sort: SortOrder::Newest,
            compact: true,
            nav_layout: NavLayout::Left,
        };
        prefs.save(&prefs_path);
        let loaded = StudioPrefs::load(&prefs_path);
        assert_eq!(loaded.lang, Lang::En);
        assert_eq!(loaded.sort, SortOrder::Newest);
        assert!(loaded.compact);
        assert_eq!(loaded.nav_layout, NavLayout::Left);

        let _ = std::fs::remove_dir_all(&sandbox);
    }
}
