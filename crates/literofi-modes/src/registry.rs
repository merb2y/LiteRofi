//! Mode parsing, instantiation registry, and headless `--modes-self-test` verification.

use crate::calc::CalcMode;
use crate::clipboard::ClipboardMode;
use crate::color_picker::ColorMode;
use crate::combi::CombiMode;
use crate::control::ControlMode;
use crate::dmenu::DmenuMode;
use crate::drun::DrunMode;
use crate::emoji::EmojiMode;
use crate::filebrowser::FileBrowserMode;
use crate::games::GamesMode;
use crate::keys::KeysMode;
use crate::menu::MenuMode;
use crate::music::MusicMode;
use crate::power::PowerMode;
use crate::recursivebrowser::RecursiveBrowserMode;
use crate::run::RunMode;
use crate::script::ScriptMode;
use crate::search::SearchMode;
use crate::shortcuts::ShortcutsMode;
use crate::snippets::SnippetsMode;
use crate::ssh::SshMode;
use crate::theme_picker::ThemeMode;
use crate::translate::TranslateMode;
use crate::wallpaper::WallpaperMode;
use crate::wallpaper_effects::WallpaperEffectsMode;
use crate::window::WindowMode;
use crate::RichMode;
use literofi_core::config::Config;
use std::time::Instant;

/// Parsed switcher specification from `-modi` / `-modes` or `-show`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeSpec {
    Drun,
    Run,
    Window,
    WindowCd,
    Combi,
    Dmenu,
    FileBrowser,
    RecursiveBrowser,
    Ssh,
    Keys,
    Theme,
    Colors,
    Music,
    Power,
    Games,
    Wallpaper,
    WallpaperEffects,
    Clipboard,
    Control,
    Shortcuts,
    Calc,
    Emoji,
    Search,
    Snippets,
    Translate,
    Menu,
    Script { name: String, command: String },
}

impl ModeSpec {
    pub fn parse_one(token: &str) -> Option<Self> {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some((name, cmd)) = trimmed.split_once(':') {
            let n = name.trim();
            let c = cmd.trim();
            if !n.is_empty() && !c.is_empty() {
                return Some(Self::Script {
                    name: n.to_string(),
                    command: c.to_string(),
                });
            }
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "drun" => Some(Self::Drun),
            "run" => Some(Self::Run),
            "window" => Some(Self::Window),
            "windowcd" => Some(Self::WindowCd),
            "combi" => Some(Self::Combi),
            "dmenu" => Some(Self::Dmenu),
            "filebrowser" | "file-browser" | "file-browser-extended" => Some(Self::FileBrowser),
            "recursivebrowser" | "recursive-browser" => Some(Self::RecursiveBrowser),
            "ssh" => Some(Self::Ssh),
            "keys" | "help-keys" => Some(Self::Keys),
            "theme" | "themes" => Some(Self::Theme),
            "color" | "colors" | "palette" | "palettes" => Some(Self::Colors),
            "music" | "media" | "player" => Some(Self::Music),
            "power" | "powermenu" => Some(Self::Power),
            "games" | "game" | "steam" => Some(Self::Games),
            "wallpaper" | "wallpapers" | "wall" => Some(Self::Wallpaper),
            "wallpaper-effects" | "wall-effects" | "walleffects" => Some(Self::WallpaperEffects),
            "clipboard" | "clip" | "cliphist" => Some(Self::Clipboard),
            "control" | "control-center" | "cc" => Some(Self::Control),
            "shortcuts" | "hotkeys" | "cheatsheet" => Some(Self::Shortcuts),
            "calc" | "calculator" | "math" => Some(Self::Calc),
            "emoji" | "emojis" | "kaomoji" => Some(Self::Emoji),
            "search" | "web" | "google" => Some(Self::Search),
            "snippets" | "snippet" => Some(Self::Snippets),
            "translate" | "trans" | "tr" => Some(Self::Translate),
            "menu" | "omarchy" | "system-menu" | "hub" => Some(Self::Menu),
            other => Some(Self::Script {
                name: other.to_string(),
                command: trimmed.to_string(),
            }),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Drun => "drun",
            Self::Run => "run",
            Self::Window => "window",
            Self::WindowCd => "windowcd",
            Self::Combi => "combi",
            Self::Dmenu => "dmenu",
            Self::FileBrowser => "filebrowser",
            Self::RecursiveBrowser => "recursivebrowser",
            Self::Ssh => "ssh",
            Self::Keys => "keys",
            Self::Theme => "theme",
            Self::Colors => "colors",
            Self::Music => "music",
            Self::Power => "power",
            Self::Games => "games",
            Self::Wallpaper => "wallpaper",
            Self::WallpaperEffects => "wallpaper-effects",
            Self::Clipboard => "clipboard",
            Self::Control => "control",
            Self::Shortcuts => "shortcuts",
            Self::Calc => "calc",
            Self::Emoji => "emoji",
            Self::Search => "search",
            Self::Snippets => "snippets",
            Self::Translate => "translate",
            Self::Menu => "menu",
            Self::Script { name, .. } => name.as_str(),
        }
    }
}

/// Parses comma-separated `-modes` string into ordered [`ModeSpec`] values.
pub fn parse_mode_specs(modes_csv: &str) -> Vec<ModeSpec> {
    modes_csv
        .split(',')
        .filter_map(ModeSpec::parse_one)
        .collect()
}

/// Builds a [`RichMode`] instance from a parsed [`ModeSpec`].
pub fn build_mode(spec: &ModeSpec, cfg: &Config) -> Result<Box<dyn RichMode>, String> {
    match spec {
        ModeSpec::Drun => Ok(Box::new(DrunMode::new(cfg))),
        ModeSpec::Run => Ok(Box::new(RunMode::new(cfg))),
        ModeSpec::Window => Ok(Box::new(WindowMode::new(cfg, false))),
        ModeSpec::WindowCd => Ok(Box::new(WindowMode::new(cfg, true))),
        ModeSpec::Combi => Ok(Box::new(CombiMode::new(cfg)?)),
        ModeSpec::Dmenu => Ok(Box::new(DmenuMode::from_config(cfg))),
        ModeSpec::FileBrowser => Ok(Box::new(FileBrowserMode::new(cfg))),
        ModeSpec::RecursiveBrowser => Ok(Box::new(RecursiveBrowserMode::new(cfg))),
        ModeSpec::Ssh => Ok(Box::new(SshMode::new(cfg))),
        ModeSpec::Keys => Ok(Box::new(KeysMode::new(cfg))),
        ModeSpec::Theme => Ok(Box::new(ThemeMode::new(cfg))),
        ModeSpec::Colors => Ok(Box::new(ColorMode::new(cfg))),
        ModeSpec::Music => Ok(Box::new(MusicMode::new(cfg))),
        ModeSpec::Power => Ok(Box::new(PowerMode::new(cfg))),
        ModeSpec::Games => Ok(Box::new(GamesMode::new(cfg))),
        ModeSpec::Wallpaper => Ok(Box::new(WallpaperMode::new(cfg))),
        ModeSpec::WallpaperEffects => Ok(Box::new(WallpaperEffectsMode::new(cfg))),
        ModeSpec::Clipboard => Ok(Box::new(ClipboardMode::new(cfg))),
        ModeSpec::Control => Ok(Box::new(ControlMode::new(cfg))),
        ModeSpec::Shortcuts => Ok(Box::new(ShortcutsMode::new(cfg))),
        ModeSpec::Calc => Ok(Box::new(CalcMode::new(cfg))),
        ModeSpec::Emoji => Ok(Box::new(EmojiMode::new(cfg))),
        ModeSpec::Search => Ok(Box::new(SearchMode::new(cfg))),
        ModeSpec::Snippets => Ok(Box::new(SnippetsMode::new(cfg))),
        ModeSpec::Translate => Ok(Box::new(TranslateMode::new(cfg))),
        ModeSpec::Menu => Ok(Box::new(MenuMode::new(cfg))),
        ModeSpec::Script { name, command } => Ok(Box::new(ScriptMode::new(
            name.clone(),
            command.clone(),
            cfg,
        ))),
    }
}

/// Collects enabled modes from `cfg.modes` and guarantees `show_name` is present if requested.
pub fn collect_session_modes(
    cfg: &Config,
    show_name: Option<&str>,
) -> Result<(Vec<Box<dyn RichMode>>, usize), String> {
    let mut specs = parse_mode_specs(&cfg.modes);
    let mut active_idx = 0usize;

    if let Some(target) = show_name {
        if let Some(pos) = specs
            .iter()
            .position(|s| s.name().eq_ignore_ascii_case(target))
        {
            active_idx = pos;
        } else if let Some(extra) = ModeSpec::parse_one(target) {
            specs.push(extra);
            active_idx = specs.len() - 1;
        }
    }

    if specs.is_empty() {
        return Err("no enabled modes configured".to_string());
    }

    let mut out = Vec::with_capacity(specs.len());
    for spec in &specs {
        out.push(build_mode(spec, cfg)?);
    }
    Ok((out, active_idx))
}

/// Summary report emitted by `--modes-self-test`.
#[derive(Debug, Clone)]
pub struct ModesSelfTestReport {
    pub run_count: usize,
    pub drun_count: usize,
    pub window_count: usize,
    pub windowcd_count: usize,
    pub combi_count: usize,
    pub filebrowser_count: usize,
    pub recursivebrowser_count: usize,
    pub ssh_count: usize,
    pub keys_count: usize,
    pub total_ms: u128,
}

/// Initializes all built-in Wave 4 + Wave 5 modes cleanly
/// without spawning processes or opening any GUI window.
pub fn run_modes_self_test(cfg: &Config) -> Result<ModesSelfTestReport, String> {
    let t0 = Instant::now();

    let mut run = RunMode::new(cfg);
    run.init()?;
    let run_count = run.entries().len();

    let mut drun = DrunMode::new(cfg);
    drun.init()?;
    let drun_count = drun.entries().len();

    let mut window = WindowMode::new(cfg, false);
    window.init()?;
    let window_count = window.entries().len();

    let mut windowcd = WindowMode::new(cfg, true);
    windowcd.init()?;
    let windowcd_count = windowcd.entries().len();

    let mut combi = CombiMode::new(cfg)?;
    combi.init()?;
    let combi_count = combi.entries().len();

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut fb_cfg = cfg.clone();
    fb_cfg.filebrowser_directory = Some(cwd.to_string_lossy().into_owned());
    let mut filebrowser = FileBrowserMode::new(&fb_cfg);
    filebrowser.init()?;
    let filebrowser_count = filebrowser.entries().len();

    let mut recbrowser = RecursiveBrowserMode::new(cfg).with_dir(cwd, 100, true);
    recbrowser.init()?;
    let recursivebrowser_count = recbrowser.entries().len();

    let mut ssh = SshMode::new(cfg);
    ssh.init()?;
    let ssh_count = ssh.entries().len();

    let mut keys = KeysMode::new(cfg);
    keys.init()?;
    let keys_count = keys.entries().len();

    Ok(ModesSelfTestReport {
        run_count,
        drun_count,
        window_count,
        windowcd_count,
        combi_count,
        filebrowser_count,
        recursivebrowser_count,
        ssh_count,
        keys_count,
        total_ms: t0.elapsed().as_millis(),
    })
}
