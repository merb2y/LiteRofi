#![cfg_attr(windows, windows_subsystem = "windows")]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const DEFAULT_PORT: u16 = 47821;
const HOST: &str = "127.0.0.1";
const INDEX_HTML: &str = include_str!("../../../studio/index.html");

#[derive(Clone)]
struct Palette {
    name: String,
    bg: String,
    bg_alt: String,
    fg: String,
    selected: String,
    active: String,
    urgent: String,
    border: String,
    mtime: f64,
}

#[derive(Clone)]
struct ThemeEntry {
    name: String,
    category: String,
    has_preview: bool,
    is_favorite: bool,
    mtime: f64,
}

struct PrerenderState {
    running: AtomicBool,
    done: AtomicUsize,
    total: AtomicUsize,
    failed: AtomicUsize,
}

impl PrerenderState {
    fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            done: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
        }
    }

    fn to_json(&self) -> String {
        format!(
            r#"{{"running":{},"done":{},"total":{},"failed":{}}}"#,
            self.running.load(Ordering::Relaxed),
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
            self.failed.load(Ordering::Relaxed),
        )
    }
}

struct AppState {
    config_root: PathBuf,
    themes_dir: PathBuf,
    colors_dir: PathBuf,
    config_rasi: PathBuf,
    favorites_file: PathBuf,
    cache_dir: PathBuf,
    render_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    live_preview: Mutex<Option<Child>>,
    prerender: Arc<PrerenderState>,
    last_ping_ms: AtomicU64,
    start_instant: Instant,
}

impl AppState {
    fn new() -> Self {
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
            cache_dir: localappdata.join("LiteRofi").join("studio-cache"),
            config_root,
            render_locks: Mutex::new(HashMap::new()),
            live_preview: Mutex::new(None),
            prerender: Arc::new(PrerenderState::new()),
            last_ping_ms: AtomicU64::new(0),
            start_instant: Instant::now(),
        }
    }

    fn touch_ping(&self) {
        let elapsed = self.start_instant.elapsed().as_millis() as u64;
        self.last_ping_ms.store(elapsed, Ordering::Relaxed);
    }

    fn idle_secs(&self) -> u64 {
        let now = self.start_instant.elapsed().as_millis() as u64;
        let last = self.last_ping_ms.load(Ordering::Relaxed);
        now.saturating_sub(last) / 1000
    }

    fn get_render_lock(&self, name: &str) -> Arc<Mutex<()>> {
        let mut map = self.render_locks.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(name.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn close_live_preview(&self) {
        let mut guard = self.live_preview.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut child) = guard.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn launch_live_preview(&self, theme_name: &str) -> Result<(), String> {
        let exe = find_literofi_exe().ok_or_else(|| "literofi.exe not found".to_string())?;
        let rasi = self.themes_dir.join(format!("{theme_name}.rasi"));
        if !rasi.is_file() {
            return Err(format!("Theme not found: {theme_name}"));
        }
        self.close_live_preview();
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
        let mut guard = self.live_preview.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(child);
        Ok(())
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn home_dir() -> PathBuf {
    env_path("USERPROFILE")
        .or_else(|| env_path("HOME"))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn find_literofi_exe() -> Option<PathBuf> {
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            let sibling = dir.join("literofi.exe");
            if sibling.is_file() {
                return Some(sibling);
            }
        }
    }
    if let Some(local) = env_path("LOCALAPPDATA") {
        let p = local.join("LiteRofi").join("literofi.exe");
        if p.is_file() {
            return Some(p);
        }
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

fn builtin_palettes() -> &'static [Palette] {
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
                },
            )
            .collect()
    })
}

fn classify_theme(name: &str) -> &'static str {
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
    ]
    .iter()
    .any(|p| low.starts_with(p))
    {
        "Modern"
    } else {
        "Classic"
    }
}

fn read_active_theme(config_rasi: &Path) -> String {
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

fn read_active_palette(config_root: &Path) -> String {
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

fn load_favorites(path: &Path) -> HashSet<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashSet::new();
    };
    parse_json_string_array(&text).into_iter().collect()
}

fn save_favorites(path: &Path, favs: &HashSet<String>) {
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

fn list_themes(state: &AppState) -> Vec<ThemeEntry> {
    let favs = load_favorites(&state.favorites_file);
    let _ = std::fs::create_dir_all(&state.cache_dir);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&state.themes_dir) {
        for ent in rd.flatten() {
            let path = ent.path();
            if !path.is_file() {
                continue;
            }
            let is_rasi = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("rasi"));
            if !is_rasi {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let name = stem.to_string();
            let png = state.cache_dir.join(format!("{name}.png"));
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

fn parse_hex6(val: &str) -> Option<String> {
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

fn parse_palette_file(path: &Path) -> Option<Palette> {
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
    })
}

fn list_palettes(colors_dir: &Path) -> Vec<Palette> {
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

fn apply_palette(state: &AppState, palette_name: &str) -> Result<(), String> {
    let palettes = list_palettes(&state.colors_dir);
    let p = palettes
        .iter()
        .find(|x| x.name.eq_ignore_ascii_case(palette_name))
        .ok_or_else(|| format!("Palet bulunamadi: {palette_name}"))?;
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
    let wallust_path = state.config_root.join("wallust").join("colors-rofi.rasi");
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
    let launchers = state.config_root.join("launchers");
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

fn render_preview_png(state: &AppState, name: &str, force: bool) -> Option<PathBuf> {
    let rasi = state.themes_dir.join(format!("{name}.rasi"));
    if !rasi.is_file() {
        return None;
    }
    let _ = std::fs::create_dir_all(&state.cache_dir);
    let out_png = state.cache_dir.join(format!("{name}.png"));
    let lock = state.get_render_lock(name);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
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
    let tmp_png = state.cache_dir.join(format!(".{name}.{stamp}.png"));
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

fn start_prerender(state: Arc<AppState>, force: bool) -> bool {
    if state
        .prerender
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return false;
    }
    let targets: Vec<String> = list_themes(&state)
        .into_iter()
        .filter(|t| force || !t.has_preview)
        .map(|t| t.name)
        .collect();
    state.prerender.done.store(0, Ordering::Relaxed);
    state.prerender.failed.store(0, Ordering::Relaxed);
    state
        .prerender
        .total
        .store(targets.len(), Ordering::Relaxed);

    let queue = Arc::new(Mutex::new(targets));
    let worker_count = 4usize;
    thread::spawn(move || {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let st = Arc::clone(&state);
            let q = Arc::clone(&queue);
            handles.push(thread::spawn(move || loop {
                let next = {
                    let mut guard = q.lock().unwrap_or_else(|e| e.into_inner());
                    guard.pop()
                };
                let Some(name) = next else { break };
                if render_preview_png(&st, &name, force).is_none() {
                    st.prerender.failed.fetch_add(1, Ordering::Relaxed);
                }
                st.prerender.done.fetch_add(1, Ordering::Relaxed);
            }));
        }
        for h in handles {
            let _ = h.join();
        }
        state.prerender.running.store(false, Ordering::SeqCst);
    });
    true
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn extract_json_string_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let pos = body.find(&needle)?;
    let rest = &body[pos + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    if !after.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = after[1..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => {
                if let Some(esc) = chars.next() {
                    match esc {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => {
                            let hex: String = chars.by_ref().take(4).collect();
                            if hex.len() == 4 {
                                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                    if let Some(c) = char::from_u32(code) {
                                        out.push(c);
                                    }
                                }
                            }
                        }
                        other => out.push(other),
                    }
                }
            }
            c => out.push(c),
        }
    }
    None
}

fn extract_json_bool_field(body: &str, key: &str) -> bool {
    let needle = format!("\"{key}\"");
    let Some(pos) = body.find(&needle) else {
        return false;
    };
    let rest = &body[pos + needle.len()..];
    let Some(colon) = rest.find(':') else {
        return false;
    };
    rest[colon + 1..].trim_start().starts_with("true")
}

fn parse_json_string_array(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            let mut s = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => {
                        if let Some(esc) = chars.next() {
                            s.push(esc);
                        }
                    }
                    other => s.push(other),
                }
            }
            out.push(s);
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(val) = u8::from_str_radix(hex, 16) {
                    out.push(val);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn write_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn write_json(stream: &mut TcpStream, status: &str, json: &str) {
    write_response(
        stream,
        status,
        "application/json; charset=utf-8",
        json.as_bytes(),
    );
}

fn handle_client(mut stream: TcpStream, state: Arc<AppState>) {
    state.touch_ping();
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut req_line = String::new();
    if reader.read_line(&mut req_line).unwrap_or(0) == 0 {
        return;
    }
    let parts: Vec<&str> = req_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let full_target = parts[1];
    let (path, query) = full_target.split_once('?').unwrap_or((full_target, ""));

    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let t = line.trim();
        if t.is_empty() {
            break;
        }
        if let Some((k, v)) = t.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_len = v.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body = vec![0u8; content_len.min(2 * 1024 * 1024)];
    if content_len > 0 {
        let _ = reader.read_exact(&mut body);
    }
    let body_str = String::from_utf8_lossy(&body);

    if method == "GET" {
        if path == "/" || path == "/index.html" {
            write_response(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                INDEX_HTML.as_bytes(),
            );
            return;
        }
        if path == "/api/ping" {
            write_json(&mut stream, "200 OK", r#"{"ok":true}"#);
            return;
        }
        if path == "/api/prerender-status" {
            write_json(&mut stream, "200 OK", &state.prerender.to_json());
            return;
        }
        if path == "/api/state" {
            let themes = list_themes(&state);
            let palettes = list_palettes(&state.colors_dir);
            let preview_count = themes.iter().filter(|t| t.has_preview).count();
            let active_theme = read_active_theme(&state.config_rasi);
            let active_palette = read_active_palette(&state.config_root);

            let mut json = String::with_capacity(themes.len() * 90 + palettes.len() * 140 + 256);
            json.push_str("{\"activeTheme\":\"");
            json.push_str(&json_escape(&active_theme));
            json.push_str("\",\"activePalette\":\"");
            json.push_str(&json_escape(&active_palette));
            json.push_str("\",\"themes\":[");
            for (i, t) in themes.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                json.push_str(&format!(
                    r#"{{"name":"{}","category":"{}","hasPreview":{},"isFavorite":{},"mtime":{:.3}}}"#,
                    json_escape(&t.name),
                    json_escape(&t.category),
                    t.has_preview,
                    t.is_favorite,
                    t.mtime
                ));
            }
            json.push_str("],\"palettes\":[");
            for (i, p) in palettes.iter().enumerate() {
                if i > 0 {
                    json.push(',');
                }
                json.push_str(&format!(
                    r#"{{"name":"{}","bg":"{}","bg_alt":"{}","fg":"{}","selected":"{}","active":"{}","urgent":"{}","border":"{}","mtime":{:.3}}}"#,
                    json_escape(&p.name),
                    json_escape(&p.bg),
                    json_escape(&p.bg_alt),
                    json_escape(&p.fg),
                    json_escape(&p.selected),
                    json_escape(&p.active),
                    json_escape(&p.urgent),
                    json_escape(&p.border),
                    p.mtime
                ));
            }
            json.push_str(&format!(
                "],\"previewCount\":{},\"totalThemes\":{},\"prerender\":{}}}",
                preview_count,
                themes.len(),
                state.prerender.to_json()
            ));
            write_json(&mut stream, "200 OK", &json);
            return;
        }
        if let Some(rest) = path
            .strip_prefix("/api/preview/")
            .and_then(|s| s.strip_suffix(".png"))
        {
            let name = url_decode(rest);
            if name.contains('/') || name.contains('\\') || name.contains("..") {
                write_json(
                    &mut stream,
                    "400 Bad Request",
                    r#"{"error":"invalid name"}"#,
                );
                return;
            }
            let render_flag = query.split('&').any(|kv| kv == "render=1");
            let force_flag = query.split('&').any(|kv| kv == "force=1");
            let mut png = state.cache_dir.join(format!("{name}.png"));
            let exists = std::fs::metadata(&png)
                .map(|m| m.is_file() && m.len() > 256)
                .unwrap_or(false);
            if force_flag || (render_flag && !exists) {
                match render_preview_png(&state, &name, force_flag) {
                    Some(p) => png = p,
                    None => {
                        write_response(&mut stream, "404 Not Found", "text/plain", b"");
                        return;
                    }
                }
            }
            match std::fs::read(&png) {
                Ok(bytes) if bytes.len() > 256 => {
                    write_response(&mut stream, "200 OK", "image/png", &bytes);
                }
                _ => write_response(&mut stream, "404 Not Found", "text/plain", b""),
            }
            return;
        }
        write_json(&mut stream, "404 Not Found", r#"{"error":"not found"}"#);
        return;
    }

    if method == "POST" {
        if path == "/api/apply-theme" {
            let name = extract_json_string_field(&body_str, "name").unwrap_or_default();
            let rasi = state.themes_dir.join(format!("{name}.rasi"));
            if !rasi.is_file() {
                write_json(
                    &mut stream,
                    "404 Not Found",
                    &format!(r#"{{"error":"Tema bulunamadi: {}"}}"#, json_escape(&name)),
                );
                return;
            }
            match literofi_modes::theme_picker::apply_theme_to_config(&state.config_rasi, &name) {
                Ok(()) => write_json(
                    &mut stream,
                    "200 OK",
                    &format!(r#"{{"ok":true,"activeTheme":"{}"}}"#, json_escape(&name)),
                ),
                Err(e) => write_json(
                    &mut stream,
                    "500 Internal Server Error",
                    &format!(r#"{{"error":"{}"}}"#, json_escape(&e)),
                ),
            }
            return;
        }
        if path == "/api/apply-palette" {
            let name = extract_json_string_field(&body_str, "name").unwrap_or_default();
            match apply_palette(&state, &name) {
                Ok(()) => write_json(
                    &mut stream,
                    "200 OK",
                    &format!(r#"{{"ok":true,"activePalette":"{}"}}"#, json_escape(&name)),
                ),
                Err(e) => write_json(
                    &mut stream,
                    "500 Internal Server Error",
                    &format!(r#"{{"error":"{}"}}"#, json_escape(&e)),
                ),
            }
            return;
        }
        if path == "/api/live-preview" {
            let name = extract_json_string_field(&body_str, "name").unwrap_or_default();
            match state.launch_live_preview(&name) {
                Ok(()) => write_json(&mut stream, "200 OK", r#"{"ok":true}"#),
                Err(e) => write_json(
                    &mut stream,
                    "500 Internal Server Error",
                    &format!(r#"{{"error":"{}"}}"#, json_escape(&e)),
                ),
            }
            return;
        }
        if path == "/api/favorite" {
            let name = extract_json_string_field(&body_str, "name").unwrap_or_default();
            let mut favs = load_favorites(&state.favorites_file);
            let is_fav = if favs.contains(&name) {
                favs.remove(&name);
                false
            } else {
                favs.insert(name.clone());
                true
            };
            save_favorites(&state.favorites_file, &favs);
            write_json(
                &mut stream,
                "200 OK",
                &format!(
                    r#"{{"ok":true,"name":"{}","isFavorite":{}}}"#,
                    json_escape(&name),
                    is_fav
                ),
            );
            return;
        }
        if path == "/api/prerender" {
            let force = extract_json_bool_field(&body_str, "force");
            let started = start_prerender(Arc::clone(&state), force);
            write_json(
                &mut stream,
                "200 OK",
                &format!(
                    r#"{{"ok":true,"started":{},"prerender":{}}}"#,
                    started,
                    state.prerender.to_json()
                ),
            );
            return;
        }
        if path == "/api/clear-cache" {
            if let Ok(rd) = std::fs::read_dir(&state.cache_dir) {
                for ent in rd.flatten() {
                    let p = ent.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("png") {
                        let _ = std::fs::remove_file(p);
                    }
                }
            }
            write_json(&mut stream, "200 OK", r#"{"ok":true}"#);
            return;
        }
        if path == "/api/import-theme" {
            let raw_name = extract_json_string_field(&body_str, "name").unwrap_or_default();
            let content = extract_json_string_field(&body_str, "content").unwrap_or_default();
            match import_theme_file(&state.themes_dir, &state.cache_dir, &raw_name, &content) {
                Ok(saved_name) => write_json(
                    &mut stream,
                    "200 OK",
                    &format!(r#"{{"ok":true,"name":"{}"}}"#, json_escape(&saved_name)),
                ),
                Err(e) => write_json(
                    &mut stream,
                    "400 Bad Request",
                    &format!(r#"{{"error":"{}"}}"#, json_escape(&e)),
                ),
            }
            return;
        }
    }

    write_json(&mut stream, "404 Not Found", r#"{"error":"not found"}"#);
}

fn sanitize_theme_stem(raw_name: &str) -> Result<String, String> {
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

fn import_theme_file(
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

fn find_chromium_browser() -> Option<PathBuf> {
    let pf = env_path("ProgramFiles").unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    let pf86 =
        env_path("ProgramFiles(x86)").unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
    let local =
        env_path("LOCALAPPDATA").unwrap_or_else(|| home_dir().join("AppData").join("Local"));
    let candidates = [
        pf86.join("Microsoft")
            .join("Edge")
            .join("Application")
            .join("msedge.exe"),
        pf.join("Microsoft")
            .join("Edge")
            .join("Application")
            .join("msedge.exe"),
        pf.join("Google")
            .join("Chrome")
            .join("Application")
            .join("chrome.exe"),
        pf86.join("Google")
            .join("Chrome")
            .join("Application")
            .join("chrome.exe"),
        local
            .join("Google")
            .join("Chrome")
            .join("Application")
            .join("chrome.exe"),
        pf.join("BraveSoftware")
            .join("Brave-Browser")
            .join("Application")
            .join("brave.exe"),
        local
            .join("BraveSoftware")
            .join("Brave-Browser")
            .join("Application")
            .join("brave.exe"),
        local
            .join("Vivaldi")
            .join("Application")
            .join("vivaldi.exe"),
        local.join("Programs").join("Opera").join("opera.exe"),
        local.join("Programs").join("Opera GX").join("opera.exe"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn open_app_window(url: &str) {
    if let Some(browser) = find_chromium_browser() {
        let local =
            env_path("LOCALAPPDATA").unwrap_or_else(|| home_dir().join("AppData").join("Local"));
        let profile = local.join("LiteRofi").join("studio-profile");
        let _ = std::fs::create_dir_all(&profile);
        let _ = Command::new(browser)
            .arg(format!("--app={url}"))
            .arg("--window-size=1280,820")
            .arg(format!("--user-data-dir={}", profile.display()))
            .spawn();
    } else {
        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd.exe");
            cmd.args(["/C", "start", "", url])
                .creation_flags(CREATE_NO_WINDOW);
            let _ = cmd.spawn();
        }
    }
}

fn main() {
    let mut port = DEFAULT_PORT;
    let mut no_browser = false;
    let mut no_autoshutdown = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                if let Some(v) = args.next() {
                    if let Ok(p) = v.parse::<u16>() {
                        port = p;
                    }
                }
            }
            "--no-browser" => no_browser = true,
            "--no-autoshutdown" => no_autoshutdown = true,
            _ => {}
        }
    }

    let addr = format!("{HOST}:{port}");
    let url = format!("http://{addr}/");

    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(_) => {
            // Already running: just open window and exit cleanly.
            if !no_browser {
                open_app_window(&url);
            }
            return;
        }
    };

    let state = Arc::new(AppState::new());
    state.touch_ping();

    if !no_autoshutdown {
        let st = Arc::clone(&state);
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(25));
            loop {
                thread::sleep(Duration::from_secs(5));
                if st.idle_secs() > 35 {
                    st.close_live_preview();
                    std::process::exit(0);
                }
            }
        });
    }

    if !no_browser {
        let u = url.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            open_app_window(&u);
        });
    }

    for stream in listener.incoming().flatten() {
        let st = Arc::clone(&state);
        thread::spawn(move || handle_client(stream, st));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_theme_sanitizes_name_strips_bom_and_invalidates_cache() {
        let base =
            std::env::temp_dir().join(format!("literofi_studio_import_{}", std::process::id()));
        let themes = base.join("themes");
        let cache = base.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("my-custom.png"), b"oldpng").unwrap();

        let saved = import_theme_file(
            &themes,
            &cache,
            "my-custom.rasi",
            "\u{FEFF}* { background-color: #111111; }\n",
        )
        .unwrap();
        assert_eq!(saved, "my-custom");
        let text = std::fs::read_to_string(themes.join("my-custom.rasi")).unwrap();
        assert!(!text.starts_with('\u{FEFF}'));
        assert!(text.contains("#111111"));
        assert!(!cache.join("my-custom.png").exists());

        assert!(import_theme_file(&themes, &cache, "empty.rasi", "   ").is_err());
        assert!(sanitize_theme_stem("...").is_err());

        let _ = std::fs::remove_dir_all(&base);
    }
}
