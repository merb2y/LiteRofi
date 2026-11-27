mod daemon;

use literofi_core::cli::{parse_argv, usage_text};
use literofi_core::config::Config;
use literofi_modes::dmenu::DmenuMode;
use literofi_modes::registry::{build_mode, collect_session_modes, run_modes_self_test, ModeSpec};
use literofi_modes::{ModeStep, RichMode};
use literofi_render::state::{LauncherState, UiOutcome};
use literofi_theme::{DistanceMetrics, ParseOptions, ThemeSheet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = "2.1.1-dev (literofi 0.1.0)";

const STARTER_CONFIG_RASI: &str = r#"configuration {
    modes: "drun,run,window,combi,filebrowser,keys";
    show-icons: true;
    font: "Segoe UI 11";
    drun-display-format: "{name}";

    /* Global background daemon hotkey & default mode (`literofi --daemon`) */
    daemon-hotkey: "Alt+Space";
    daemon-mode: "drun";
    /* Extra global shortcuts: "Chord=mode[:theme]" (comma-separated, or "" to disable) */
    daemon-hotkeys: "Alt+G=games:game-launcher, Alt+M=music:music-player, Alt+P=power:powermenu";

    /* Keybindings inside LiteRofi (run `literofi -show keys` to list all kb-* names) */
    /* kb-row-down: "Down,Control+n,Control+j"; */
    /* kb-row-up: "Up,Control+p,Control+k"; */
    /* kb-accept-entry: "Return,KP_Enter"; */
    /* kb-cancel: "Escape"; */
    /* kb-mode-next: "Shift+Right,Control+Tab"; */
}

/* Drop any GitHub .rasi theme into %APPDATA%\LiteRofi\themes\ and select it below */
@theme "tokyonight"
"#;

fn default_user_config_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return PathBuf::from(appdata).join("LiteRofi").join("config.rasi");
        }
    }
    PathBuf::from("config.rasi")
}

fn default_user_themes_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return PathBuf::from(appdata).join("LiteRofi").join("themes");
        }
    }
    PathBuf::from("themes")
}

fn ensure_starter_config(path: &Path) -> Result<(), String> {
    if path.is_file() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create config directory: {e}"))?;
    }
    std::fs::write(path, STARTER_CONFIG_RASI)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}

pub(crate) fn discover_user_config_file(cfg: &Config) -> Option<PathBuf> {
    if cfg.no_config {
        return None;
    }
    if let Some(explicit) = &cfg.config_file {
        return Some(PathBuf::from(explicit));
    }
    let mut candidates = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            candidates.push(PathBuf::from(&appdata).join("LiteRofi").join("config.rasi"));
            candidates.push(PathBuf::from(&appdata).join("rofi").join("config.rasi"));
        }
    }
    for home_key in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(home_key) {
            if !home.trim().is_empty() {
                candidates.push(
                    PathBuf::from(&home)
                        .join(".config")
                        .join("rofi")
                        .join("config.rasi"),
                );
                break;
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn theme_search_dirs(config_path: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(parent) = config_path.and_then(|p| p.parent()) {
        dirs.push(parent.to_path_buf());
        dirs.push(parent.join("themes"));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            dirs.push(PathBuf::from(&appdata).join("LiteRofi").join("themes"));
            dirs.push(PathBuf::from(&appdata).join("LiteRofi"));
            dirs.push(PathBuf::from(&appdata).join("rofi").join("themes"));
            dirs.push(PathBuf::from(&appdata).join("rofi"));
        }
    }
    for home_key in ["USERPROFILE", "HOME"] {
        if let Ok(home) = std::env::var(home_key) {
            if !home.trim().is_empty() {
                dirs.push(
                    PathBuf::from(&home)
                        .join(".config")
                        .join("rofi")
                        .join("themes"),
                );
                dirs.push(PathBuf::from(&home).join(".config").join("rofi"));
                break;
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            dirs.push(exe_dir.join("themes"));
        }
    }
    dirs
}

fn resolve_theme_cli_path(spec: &str, search_dirs: &[PathBuf]) -> PathBuf {
    let raw = PathBuf::from(spec);
    if raw.is_file() {
        return raw;
    }
    if raw.extension().is_none() {
        let with_ext = raw.with_extension("rasi");
        if with_ext.is_file() {
            return with_ext;
        }
    }
    for dir in search_dirs {
        let cand = dir.join(spec);
        if cand.is_file() {
            return cand;
        }
        if Path::new(spec).extension().is_none() {
            let cand_ext = cand.with_extension("rasi");
            if cand_ext.is_file() {
                return cand_ext;
            }
        }
    }
    raw
}

pub(crate) fn load_theme_and_config(cfg: &mut Config) -> Result<ThemeSheet, String> {
    let mut sheet = ThemeSheet::new();
    let mut explicit_null_theme = false;
    let config_path = discover_user_config_file(cfg);
    let search_dirs = theme_search_dirs(config_path.as_deref());
    let opts = ParseOptions {
        search_dirs: search_dirs.clone(),
        dmenu_mode: cfg.dmenu,
    };

    if let Some(cfg_file) = &config_path {
        if cfg.theme.is_some() {
            // When `-theme` is passed on CLI, skip `@theme` lines in `config.rasi` so
            // the previously active theme's `configuration { display-drun: ... }` cannot
            // leak into the overridden theme.
            let raw = std::fs::read_to_string(cfg_file)
                .map_err(|e| format!("failed to read {}: {e}", cfg_file.display()))?;
            let stripped: String = raw
                .strip_prefix('\u{FEFF}')
                .unwrap_or(&raw)
                .lines()
                .filter(|line| {
                    let t = line.trim_start();
                    !(t.starts_with("@theme") && !t.starts_with("/*") && !t.starts_with("//"))
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mut cfg_opts = opts.clone();
            if let Some(parent) = cfg_file.parent() {
                cfg_opts.search_dirs.insert(0, parent.to_path_buf());
            }
            sheet
                .parse_str(&stripped, &cfg_opts)
                .map_err(|e| e.to_string())?;
        } else {
            sheet
                .parse_file(cfg_file, &opts)
                .map_err(|e| e.to_string())?;
        }
    }

    if let Some(path) = cfg.theme.clone() {
        if path.eq_ignore_ascii_case("default") {
            sheet.reset_theme();
            sheet
                .parse_str(literofi_theme::DEFAULT_THEME_RASI, &opts)
                .map_err(|e| e.to_string())?;
        } else if path.trim() == "/dev/null" || path.trim().eq_ignore_ascii_case("nul") {
            explicit_null_theme = true;
            sheet.reset_theme();
        } else {
            sheet.reset_theme();
            let resolved = resolve_theme_cli_path(&path, &search_dirs);
            sheet
                .parse_file(&resolved, &opts)
                .map_err(|e| e.to_string())?;
        }
    }
    if sheet.is_theme_empty() && !explicit_null_theme {
        sheet
            .parse_str(literofi_theme::DEFAULT_THEME_RASI, &opts)
            .map_err(|e| e.to_string())?;
    }
    for snippet in &cfg.theme_str {
        sheet.parse_str(snippet, &opts).map_err(|e| e.to_string())?;
    }
    let hide_icons_cli = cfg.hide_icons;
    sheet.apply_configuration(cfg);
    if hide_icons_cli {
        cfg.show_icons = false;
        cfg.hide_icons = true;
    }
    Ok(sheet)
}

#[cfg(windows)]
pub(crate) fn load_preview_theme_sheet(
    theme_path: &std::path::Path,
    cfg: &Config,
) -> Result<ThemeSheet, String> {
    let opts = ParseOptions {
        search_dirs: theme_search_dirs(cfg.config_file.as_deref().map(Path::new)),
        dmenu_mode: cfg.dmenu,
    };
    let mut sheet = ThemeSheet::new();
    sheet
        .parse_file(theme_path, &opts)
        .map_err(|e| e.to_string())?;
    if sheet.is_theme_empty() {
        sheet
            .parse_str(literofi_theme::DEFAULT_THEME_RASI, &opts)
            .map_err(|e| e.to_string())?;
    }
    Ok(sheet)
}

#[cfg(windows)]
pub(crate) fn apply_mode_to_state(
    mode: &dyn RichMode,
    cfg: &Config,
    base_sheet: &ThemeSheet,
    state: &mut LauncherState,
    first_init: bool,
) -> Result<ThemeSheet, String> {
    let mut working = base_sheet.clone();
    let opts = ParseOptions {
        search_dirs: Vec::new(),
        dmenu_mode: cfg.dmenu,
    };
    for snippet in mode.theme_snippets() {
        working
            .parse_str(&snippet, &opts)
            .map_err(|e| e.to_string())?;
    }
    if first_init {
        *state = LauncherState::new(cfg, mode.entries());
    } else {
        let hints = mode.reload_hints();
        state.replace_entries(
            mode.entries(),
            hints.keep_selection,
            hints.new_selection,
            hints.keep_filter,
        );
    }
    if let Some(p) = mode.prompt_override() {
        state.prompt = p;
    } else if cfg.prompt.is_none() {
        state.prompt = mode.display_name().to_string();
    }
    state.markup_rows_override = mode.markup_rows_override();
    state.message = mode
        .message()
        .or_else(|| cfg.mesg.clone())
        .filter(|s| !s.is_empty());
    state.base_message = state.message.clone();
    state.live_message_fn = mode.live_message_evaluator();
    state.back_on_empty_backspace = mode.back_on_empty_backspace();
    if let Some(nc) = mode.no_custom_override() {
        state.no_custom = nc;
    }
    state.overlay = mode.overlay();
    Ok(working)
}

#[cfg(windows)]
fn run_session_loop(
    cfg: &Config,
    base_sheet: &ThemeSheet,
    modes: &mut Vec<Box<dyn RichMode>>,
    mut active_idx: usize,
) -> Result<ExitCode, String> {
    if modes.is_empty() {
        return Err("no active modes".to_string());
    }
    active_idx %= modes.len();
    modes[active_idx].init()?;

    let mut active_base_sheet = base_sheet.clone();
    let mut state = LauncherState::new(cfg, Vec::new());
    let working_sheet = apply_mode_to_state(
        modes[active_idx].as_ref(),
        cfg,
        &active_base_sheet,
        &mut state,
        true,
    )?;
    state.modes = modes.iter().map(|m| m.display_name().to_string()).collect();
    state.active_mode = active_idx;

    // Keep a single persistent HWND across preview / reload / mode-switch so Windows never
    // drops foreground focus and triggers an immediate `click_to_exit` deactivation.
    let mut popup =
        literofi_render::win32_d2d::ResidentPopup::new(working_sheet, cfg.clone(), state)?;
    popup.prepare_and_show()?;

    loop {
        let outcome = popup.wait_for_outcome();
        if let UiOutcome::ModeSwitch(next_idx) = outcome {
            active_idx = next_idx % modes.len();
            modes[active_idx].init()?;
            let mut next_state = LauncherState::new(cfg, Vec::new());
            let ws = apply_mode_to_state(
                modes[active_idx].as_ref(),
                cfg,
                &active_base_sheet,
                &mut next_state,
                true,
            )?;
            next_state.modes = modes.iter().map(|m| m.display_name().to_string()).collect();
            next_state.active_mode = active_idx;
            popup.set_state_and_sheet(ws, next_state);
            continue;
        }

        let query = popup.current_query();
        let pre_hide = matches!(
            &outcome,
            UiOutcome::Accepted { exit_code, .. } if *exit_code < 10
        ) || matches!(&outcome, UiOutcome::AcceptedAlt { .. });
        let is_instant_exit_mode = matches!(
            modes[active_idx].name().to_ascii_lowercase().as_str(),
            "drun" | "run" | "ssh" | "window" | "combi"
        );
        if pre_hide && is_instant_exit_mode {
            popup.hide();
        }
        match modes[active_idx].handle_outcome(&outcome, &query)? {
            ModeStep::Exit(code) => {
                popup.hide();
                return Ok(ExitCode::from(code.clamp(0, 255) as u8));
            }
            ModeStep::Reload => {
                let mut next_state = popup.current_state_clone();
                let ws = apply_mode_to_state(
                    modes[active_idx].as_ref(),
                    cfg,
                    &active_base_sheet,
                    &mut next_state,
                    false,
                )?;
                popup.set_state_and_sheet(ws, next_state);
            }
            ModeStep::PreviewTheme(theme_path) => {
                match load_preview_theme_sheet(&theme_path, cfg) {
                    Ok(preview_sheet) => {
                        active_base_sheet = preview_sheet.clone();
                        popup.preview_theme_sheet(preview_sheet, modes[active_idx].message());
                    }
                    Err(err) => {
                        popup.preview_theme_sheet(
                            active_base_sheet.clone(),
                            Some(format!("Theme preview error: {err}")),
                        );
                    }
                }
            }
            ModeStep::ReloadTheme => {
                let mut tmp_cfg = cfg.clone();
                if let Ok(reloaded) = load_theme_and_config(&mut tmp_cfg) {
                    active_base_sheet = reloaded.clone();
                    popup.preview_theme_sheet(reloaded, modes[active_idx].message());
                }
            }
            ModeStep::SwitchModeIndex(idx) => {
                active_idx = idx % modes.len();
                modes[active_idx].init()?;
                let mut next_state = LauncherState::new(cfg, Vec::new());
                let ws = apply_mode_to_state(
                    modes[active_idx].as_ref(),
                    cfg,
                    &active_base_sheet,
                    &mut next_state,
                    true,
                )?;
                next_state.modes = modes.iter().map(|m| m.display_name().to_string()).collect();
                next_state.active_mode = active_idx;
                popup.set_state_and_sheet(ws, next_state);
            }
            ModeStep::SwitchMode(target) => {
                if let Some(pos) = modes
                    .iter()
                    .position(|m| m.name().eq_ignore_ascii_case(&target))
                {
                    active_idx = pos;
                } else if let Some(spec) = ModeSpec::parse_one(&target) {
                    modes.push(build_mode(&spec, cfg)?);
                    active_idx = modes.len() - 1;
                } else {
                    popup.hide();
                    return Err(format!("unknown target mode `{target}`"));
                }
                modes[active_idx].init()?;
                let mut next_state = LauncherState::new(cfg, Vec::new());
                let ws = apply_mode_to_state(
                    modes[active_idx].as_ref(),
                    cfg,
                    &active_base_sheet,
                    &mut next_state,
                    true,
                )?;
                next_state.modes = modes.iter().map(|m| m.display_name().to_string()).collect();
                next_state.active_mode = active_idx;
                popup.set_state_and_sheet(ws, next_state);
            }
        }
    }
}

fn run_dmenu(cfg: &Config, base_sheet: &ThemeSheet) -> Result<ExitCode, String> {
    let mut modes: Vec<Box<dyn RichMode>> = vec![Box::new(DmenuMode::from_config(cfg))];
    #[cfg(windows)]
    {
        run_session_loop(cfg, base_sheet, &mut modes, 0)
    }
    #[cfg(not(windows))]
    {
        let _ = (base_sheet, &mut modes, DistanceMetrics::default());
        Err("Win32 + Direct2D backend requires Windows".to_string())
    }
}

fn run_show(cfg: &Config, base_sheet: &ThemeSheet, show_mode: &str) -> Result<ExitCode, String> {
    let (mut modes, active_idx) = collect_session_modes(cfg, Some(show_mode))?;
    #[cfg(windows)]
    {
        run_session_loop(cfg, base_sheet, &mut modes, active_idx)
    }
    #[cfg(not(windows))]
    {
        let _ = (
            base_sheet,
            &mut modes,
            active_idx,
            DistanceMetrics::default(),
        );
        Err("Win32 + Direct2D backend requires Windows".to_string())
    }
}

fn run() -> Result<ExitCode, String> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--we-subscribe") {
        let id_u64 = args
            .get(2)
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| "missing workshop item id for --we-subscribe".to_string())?;
        let monitor_idx = args.get(3).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        let mute_audio = literofi_core::widgets_config::WidgetsConfig::load_default()
            .wallpaper
            .we_mute_audio;
        literofi_modes::wallpaper::subscribe_and_apply_workshop_item(
            id_u64,
            monitor_idx,
            mute_audio,
        )?;
        return Ok(ExitCode::SUCCESS);
    }
    let mut cfg = parse_argv(&args)?;

    if cfg.show_version {
        println!("Version: {VERSION}");
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.show_help {
        print!("{}", usage_text());
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.open_settings {
        let path = discover_user_config_file(&cfg).unwrap_or_else(default_user_config_path);
        ensure_starter_config(&path)?;
        println!("{}", path.display());
        #[cfg(windows)]
        {
            std::process::Command::new("notepad.exe")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("failed to launch Notepad for {}: {e}", path.display()))?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.open_themes_dir {
        let dir = default_user_themes_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("failed to create themes directory {}: {e}", dir.display()))?;
        println!("{}", dir.display());
        #[cfg(windows)]
        {
            std::process::Command::new("explorer.exe")
                .arg(&dir)
                .spawn()
                .map_err(|e| format!("failed to open {}: {e}", dir.display()))?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.daemon_self_test {
        let report = daemon::run_daemon_self_test(&cfg)?;
        println!(
            "OK: daemon_hotkey={} requested_registered={} probe_registered={} idle_kb={}",
            report.hotkey, report.requested_registered, report.probe_registered, report.idle_kb
        );
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.stop_daemon {
        let stopped = daemon::stop_daemon();
        println!("OK: stopped_daemons={stopped}");
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.start_daemon {
        {
            let _ = load_theme_and_config(&mut cfg)?;
        }
        daemon::start_daemon_detached(&cfg)?;
        println!(
            "OK: LiteRofi background daemon started (hotkey={}, mode={})",
            cfg.daemon_hotkey, cfg.daemon_mode
        );
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.daemon {
        let cli_cfg = cfg.clone();
        let sheet = load_theme_and_config(&mut cfg)?;
        daemon::run_daemon(&cli_cfg, &cfg, &sheet)?;
        return Ok(ExitCode::SUCCESS);
    }

    let sheet = load_theme_and_config(&mut cfg)?;

    if cfg.dump_config {
        print!("{}", cfg.dump_config_rasi());
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.dump_theme {
        print!("{}", sheet.dump_theme());
        return Ok(ExitCode::SUCCESS);
    }
    if cfg.render_self_test {
        #[cfg(windows)]
        {
            let report =
                literofi_render::win32_d2d::run_render_self_test(&cfg, &[("active", &sheet)])?;
            println!(
                "OK: d2d=true, dwrite=true, dpi={:.0}, char_h={:.2}, char_w={:.2}, window={}x{}, frames={}, icons_loaded={}",
                report.dpi,
                report.char_height,
                report.char_width,
                report.window_width,
                report.window_height,
                report.frames_painted,
                report.icons_loaded
            );
            return Ok(ExitCode::SUCCESS);
        }
        #[cfg(not(windows))]
        {
            let _ = DistanceMetrics::default();
            return Err("render self-test requires Windows".to_string());
        }
    }
    if cfg.modes_self_test {
        let report = run_modes_self_test(&cfg)?;
        println!(
            "OK: run={} drun={} window={} windowcd={} combi={} filebrowser={} recursivebrowser={} ssh={} keys={} total_ms={}",
            report.run_count,
            report.drun_count,
            report.window_count,
            report.windowcd_count,
            report.combi_count,
            report.filebrowser_count,
            report.recursivebrowser_count,
            report.ssh_count,
            report.keys_count,
            report.total_ms
        );
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(png_path) = cfg.dump_png.clone() {
        #[cfg(windows)]
        {
            let mode_name = cfg.show.as_deref().unwrap_or("drun");
            if matches!(
                mode_name.to_ascii_lowercase().as_str(),
                "wallpaper" | "wallpapers" | "wall"
            ) {
                let (pw, ph) = dump_wallpaper_carousel_from_config(&cfg, Path::new(&png_path))?;
                println!("OK: dump_png={} window_phys={}x{}", png_path, pw, ph);
                return Ok(ExitCode::SUCCESS);
            }
            if matches!(
                mode_name.to_ascii_lowercase().as_str(),
                "music" | "media" | "player"
            ) {
                let (pw, ph) = dump_music_widget_from_config(&cfg, Path::new(&png_path))?;
                println!("OK: dump_png={} window_phys={}x{}", png_path, pw, ph);
                return Ok(ExitCode::SUCCESS);
            }
            if matches!(mode_name.to_ascii_lowercase().as_str(), "games" | "game") {
                let (pw, ph) = dump_games_widget_from_config(&cfg, Path::new(&png_path))?;
                println!("OK: dump_png={} window_phys={}x{}", png_path, pw, ph);
                return Ok(ExitCode::SUCCESS);
            }
            let (mut modes, active_idx) = collect_session_modes(&cfg, Some(mode_name))?;
            modes[active_idx].init()?;
            let mut state = LauncherState::new(&cfg, Vec::new());
            let working_sheet =
                apply_mode_to_state(modes[active_idx].as_ref(), &cfg, &sheet, &mut state, true)?;
            state.modes = modes.iter().map(|m| m.display_name().to_string()).collect();
            state.active_mode = active_idx;
            let (pw, ph) = literofi_render::win32_d2d::run_render_dump_png(
                &working_sheet,
                &cfg,
                &mut state,
                Path::new(&png_path),
            )?;
            println!("OK: dump_png={} window_phys={}x{}", png_path, pw, ph);
            return Ok(ExitCode::SUCCESS);
        }
        #[cfg(not(windows))]
        {
            let _ = png_path;
            return Err("dump-png requires Windows".to_string());
        }
    }
    if cfg.dmenu {
        return run_dmenu(&cfg, &sheet);
    }
    if let Some(show_mode) = cfg.show.clone() {
        hide_standalone_console();
        #[cfg(windows)]
        if matches!(
            show_mode.to_ascii_lowercase().as_str(),
            "wallpaper" | "wallpapers" | "wall"
        ) {
            return run_wallpaper_carousel_mode(&cfg);
        }
        #[cfg(windows)]
        if matches!(
            show_mode.to_ascii_lowercase().as_str(),
            "music" | "media" | "player"
        ) {
            return run_music_widget_mode(&cfg);
        }
        #[cfg(windows)]
        if matches!(show_mode.to_ascii_lowercase().as_str(), "games" | "game") {
            return run_games_widget_mode(&cfg);
        }
        return run_show(&cfg, &sheet, &show_mode);
    }

    let _ = DistanceMetrics::default();
    Err("specify `-show <mode>` or `-dmenu` (use `-h` for usage)".to_string())
}

fn hide_standalone_console() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
        // Only detach if this process is the sole owner of its console window
        // (e.g. launched via a `.lnk` shortcut), preserving parent terminal output in CLI usage.
        let mut pids = [0u32; 4];
        // SAFETY: Valid buffer passed to GetConsoleProcessList; FreeConsole is safe when count <= 1.
        let count = unsafe { GetConsoleProcessList(&mut pids) };
        if count <= 1 {
            let _ = unsafe { FreeConsole() };
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(msg) => {
            eprintln!("literofi: {msg}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn wallpaper_carousel_inputs(
    cfg: &Config,
) -> (
    PathBuf,
    Vec<literofi_modes::wallpaper::WallpaperEntry>,
    Vec<literofi_render::wallpaper_carousel::CarouselItem>,
    (f32, f32, f32),
) {
    let config_root = cfg
        .config_file
        .as_ref()
        .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
        .unwrap_or_else(|| {
            std::env::var("APPDATA")
                .map(|a| PathBuf::from(a).join("LiteRofi"))
                .unwrap_or_else(|_| PathBuf::from("."))
        });
    let entries = literofi_modes::wallpaper::discover_wallpapers(&config_root);
    let items: Vec<literofi_render::wallpaper_carousel::CarouselItem> = entries
        .iter()
        .map(|w| literofi_render::wallpaper_carousel::CarouselItem {
            title: w.name.clone(),
            path: w.path.clone(),
            is_current: w.is_current,
        })
        .collect();
    let wcfg = literofi_core::widgets_config::WidgetsConfig::load_default();
    let accent = if wcfg.wallpaper.theme_mode == "custom" {
        literofi_core::widgets_config::parse_hex_rgb(&wcfg.wallpaper.accent)
            .unwrap_or((0.54, 0.71, 0.98))
    } else {
        literofi_modes::active_palette::ActivePalette::load_from_user_config()
            .map(|p| (p.accent.r as f32, p.accent.g as f32, p.accent.b as f32))
            .unwrap_or((0.54, 0.71, 0.98))
    };
    (config_root, entries, items, accent)
}

#[cfg(windows)]
fn run_wallpaper_carousel_mode(cfg: &Config) -> Result<ExitCode, String> {
    use literofi_render::wallpaper_carousel::{
        run_wallpaper_carousel, run_wallpaper_carousel_paginated, run_wallpaper_carousel_streaming,
        CarouselItem, CarouselOutcome, CarouselSource,
    };
    use std::sync::{Arc, Mutex};

    enum LocalCarouselTarget {
        Static(literofi_modes::wallpaper::WallpaperEntry),
        WallpaperEngine(literofi_modes::wallpaper::WallpaperEngineItem),
    }

    let (config_root, local_entries, mut combined_local_items, accent) =
        wallpaper_carousel_inputs(cfg);
    let we_entries = literofi_modes::wallpaper::discover_wallpaper_engine_items();

    let mut combined_local_targets: Vec<LocalCarouselTarget> = local_entries
        .into_iter()
        .map(LocalCarouselTarget::Static)
        .collect();
    let mut subscribed_we_ids = std::collections::HashSet::new();
    for we in we_entries {
        if let Some(id) = we
            .project_json
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        {
            subscribed_we_ids.insert(id.to_string());
        }
        combined_local_items.push(CarouselItem {
            title: format!("⚡ {}", we.title),
            path: we.preview_path.clone(),
            is_current: false,
        });
        combined_local_targets.push(LocalCarouselTarget::WallpaperEngine(we));
    }

    let target_monitor = literofi_modes::wallpaper::detect_cursor_monitor_target();
    let mut source = CarouselSource::Local;
    let mut we_refresh_count = 0u32;
    let mut wh_force_refresh = false;
    let mut we_force_refresh = false;

    loop {
        let we_mute_audio = literofi_core::widgets_config::WidgetsConfig::load_default()
            .wallpaper
            .we_mute_audio;
        match source {
            CarouselSource::Local => {
                match run_wallpaper_carousel(
                    combined_local_items.clone(),
                    accent,
                    CarouselSource::Local,
                    true,
                )? {
                    CarouselOutcome::Cancelled => return Ok(ExitCode::from(1)),
                    CarouselOutcome::ToggleSource | CarouselOutcome::RefreshWeb => {
                        source = CarouselSource::Web;
                        continue;
                    }
                    CarouselOutcome::SwitchToWallpaperEngine => {
                        source = CarouselSource::WallpaperEngine;
                        continue;
                    }
                    CarouselOutcome::OpenInWallpaperEngine(_) => {
                        let _ = literofi_modes::wallpaper::open_wallpaper_engine_ui();
                        return Ok(ExitCode::SUCCESS);
                    }
                    CarouselOutcome::Apply(idx) => {
                        match combined_local_targets.get(idx) {
                            Some(LocalCarouselTarget::Static(wp)) => {
                                literofi_modes::wallpaper::stop_wallpaper_engine_on_monitor_if_running(
                                    target_monitor.index,
                                );
                                literofi_modes::wallpaper::set_windows_wallpaper_on_monitor(
                                    &wp.path,
                                    &target_monitor,
                                )?;
                            }
                            Some(LocalCarouselTarget::WallpaperEngine(we)) => {
                                literofi_modes::wallpaper::apply_wallpaper_engine_item_on_monitor(
                                    &we.project_json,
                                    target_monitor.index,
                                    we_mute_audio,
                                )?;
                            }
                            None => {}
                        }
                        return Ok(ExitCode::SUCCESS);
                    }
                    CarouselOutcome::ApplyWithTheme(idx) => {
                        match combined_local_targets.get(idx) {
                            Some(LocalCarouselTarget::Static(wp)) => {
                                literofi_modes::wallpaper::stop_wallpaper_engine_on_monitor_if_running(
                                    target_monitor.index,
                                );
                                literofi_modes::wallpaper::set_windows_wallpaper_on_monitor(
                                    &wp.path,
                                    &target_monitor,
                                )?;
                                if let Some(pal) =
                                    literofi_modes::wallpaper::extract_wallpaper_palette(
                                        &wp.path, &wp.name,
                                    )
                                {
                                    literofi_modes::color_picker::apply_palette_to_root(
                                        &config_root,
                                        &pal,
                                    )?;
                                }
                            }
                            Some(LocalCarouselTarget::WallpaperEngine(we)) => {
                                literofi_modes::wallpaper::apply_wallpaper_engine_item_on_monitor(
                                    &we.project_json,
                                    target_monitor.index,
                                    we_mute_audio,
                                )?;
                                if let Some(pal) =
                                    literofi_modes::wallpaper::extract_wallpaper_palette(
                                        &we.preview_path,
                                        &we.title,
                                    )
                                {
                                    literofi_modes::color_picker::apply_palette_to_root(
                                        &config_root,
                                        &pal,
                                    )?;
                                }
                            }
                            None => {}
                        }
                        return Ok(ExitCode::SUCCESS);
                    }
                }
            }
            CarouselSource::Web => {
                // Re-read widgets.toml each time Web mode opens or refreshes with 'R'
                // so Wallhaven filter changes in Theme Studio apply immediately.
                let wh_filter = literofi_modes::wallpaper::WallhavenFilter::from(
                    &literofi_core::widgets_config::WidgetsConfig::load_default().wallpaper,
                );
                let wh_key = literofi_modes::wallpaper::wallhaven_filter_key(&wh_filter);
                let seed = literofi_modes::wallpaper::new_wallhaven_seed();

                let cached = if !wh_force_refresh {
                    literofi_modes::wallpaper::load_wallhaven_cache(&wh_key)
                } else {
                    Vec::new()
                };
                let (first_batch, mut next_page) = if cached.len() >= 5 {
                    (cached, 1u32)
                } else {
                    let fetched = literofi_modes::wallpaper::fetch_wallhaven_page_limit(
                        &wh_filter, &seed, 1, 6,
                    );
                    if !fetched.is_empty() {
                        literofi_modes::wallpaper::save_wallhaven_cache(&wh_key, &fetched);
                    }
                    (fetched, 2u32)
                };
                wh_force_refresh = false;

                if first_batch.is_empty() {
                    source = CarouselSource::Local;
                    continue;
                }
                let web_items: Vec<CarouselItem> = first_batch
                    .iter()
                    .map(|w| CarouselItem {
                        title: w.title.clone(),
                        path: w.thumb_path.clone(),
                        is_current: false,
                    })
                    .collect();

                let shared_entries = Arc::new(Mutex::new(first_batch));
                let entries_for_loader = Arc::clone(&shared_entries);
                let loader_filter = wh_filter.clone();
                let loader_key = wh_key.clone();
                let loader_seed = seed.clone();

                let outcome = run_wallpaper_carousel_paginated(
                    web_items,
                    accent,
                    CarouselSource::Web,
                    true,
                    Some(move || {
                        let batch = literofi_modes::wallpaper::fetch_wallhaven_page_limit(
                            &loader_filter,
                            &loader_seed,
                            next_page,
                            12,
                        );
                        next_page = next_page.saturating_add(1);
                        let mut fresh_items = Vec::new();
                        if let Ok(mut guard) = entries_for_loader.lock() {
                            for w in batch {
                                if !guard.iter().any(|x| x.id == w.id) {
                                    fresh_items.push(CarouselItem {
                                        title: w.title.clone(),
                                        path: w.thumb_path.clone(),
                                        is_current: false,
                                    });
                                    guard.push(w);
                                }
                            }
                            if !fresh_items.is_empty() {
                                literofi_modes::wallpaper::save_wallhaven_cache(
                                    &loader_key,
                                    &guard,
                                );
                            }
                        }
                        fresh_items
                    }),
                )?;

                match outcome {
                    CarouselOutcome::Cancelled => return Ok(ExitCode::from(1)),
                    CarouselOutcome::ToggleSource => {
                        source = CarouselSource::Local;
                        continue;
                    }
                    CarouselOutcome::SwitchToWallpaperEngine => {
                        source = CarouselSource::WallpaperEngine;
                        continue;
                    }
                    CarouselOutcome::RefreshWeb => {
                        wh_force_refresh = true;
                        continue;
                    }
                    CarouselOutcome::OpenInWallpaperEngine(_) => {
                        let _ = literofi_modes::wallpaper::open_wallpaper_engine_ui();
                        return Ok(ExitCode::SUCCESS);
                    }
                    CarouselOutcome::Apply(idx) => {
                        let chosen = shared_entries.lock().ok().and_then(|g| g.get(idx).cloned());
                        if let Some(w) = chosen {
                            let full = literofi_modes::wallpaper::download_wallhaven_full(&w)?;
                            literofi_modes::wallpaper::stop_wallpaper_engine_on_monitor_if_running(
                                target_monitor.index,
                            );
                            literofi_modes::wallpaper::set_windows_wallpaper_on_monitor(
                                &full,
                                &target_monitor,
                            )?;
                        }
                        return Ok(ExitCode::SUCCESS);
                    }
                    CarouselOutcome::ApplyWithTheme(idx) => {
                        let chosen = shared_entries.lock().ok().and_then(|g| g.get(idx).cloned());
                        if let Some(w) = chosen {
                            let full = literofi_modes::wallpaper::download_wallhaven_full(&w)?;
                            literofi_modes::wallpaper::stop_wallpaper_engine_on_monitor_if_running(
                                target_monitor.index,
                            );
                            literofi_modes::wallpaper::set_windows_wallpaper_on_monitor(
                                &full,
                                &target_monitor,
                            )?;
                            if let Some(pal) =
                                literofi_modes::wallpaper::extract_wallpaper_palette(&full, &w.id)
                            {
                                literofi_modes::color_picker::apply_palette_to_root(
                                    &config_root,
                                    &pal,
                                )?;
                            }
                        }
                        return Ok(ExitCode::SUCCESS);
                    }
                }
            }
            CarouselSource::WallpaperEngine => {
                let we_filter = literofi_modes::wallpaper::WorkshopFilter::from(
                    &literofi_core::widgets_config::WidgetsConfig::load_default().wallpaper,
                );
                let we_key = literofi_modes::wallpaper::workshop_filter_key(&we_filter);
                let has_specific_target =
                    !we_filter.query.trim().is_empty() || !we_filter.genre.trim().is_empty();

                let cached = if !we_force_refresh {
                    literofi_modes::wallpaper::load_workshop_cache(&we_key, &subscribed_we_ids)
                } else {
                    Vec::new()
                };
                let (first_batch, mut next_page, need_prefetch) = if cached.len() >= 5 {
                    (
                        cached,
                        literofi_modes::wallpaper::new_workshop_start_page(),
                        false,
                    )
                } else {
                    let start_page = if we_refresh_count == 0 && has_specific_target {
                        1
                    } else {
                        literofi_modes::wallpaper::new_workshop_start_page()
                    };
                    we_refresh_count = we_refresh_count.saturating_add(1);
                    let fetched = literofi_modes::wallpaper::query_workshop_page_metadata(
                        &we_filter,
                        start_page,
                        &subscribed_we_ids,
                        36,
                    );
                    (fetched, start_page.saturating_add(1), true)
                };
                we_force_refresh = false;

                if first_batch.is_empty() {
                    source = CarouselSource::Local;
                    continue;
                }
                let ws_items: Vec<CarouselItem> = first_batch
                    .iter()
                    .map(|w| CarouselItem {
                        title: w.title.clone(),
                        path: w.thumb_path.clone(),
                        is_current: false,
                    })
                    .collect();

                let prefetch_entries = if need_prefetch {
                    first_batch.clone()
                } else {
                    Vec::new()
                };
                let shared_entries = Arc::new(Mutex::new(first_batch));
                let entries_for_loader = Arc::clone(&shared_entries);
                let entries_for_cache = Arc::clone(&shared_entries);
                let loader_exclude = subscribed_we_ids.clone();
                let loader_filter = we_filter.clone();
                let loader_key = we_key.clone();
                let cache_key = we_key.clone();

                let outcome = run_wallpaper_carousel_streaming(
                    ws_items,
                    accent,
                    CarouselSource::WallpaperEngine,
                    true,
                    Some(move || {
                        let batch = literofi_modes::wallpaper::fetch_workshop_page_limit(
                            &loader_filter,
                            next_page,
                            &loader_exclude,
                            18,
                        );
                        next_page = next_page.saturating_add(1);
                        let mut fresh_items = Vec::new();
                        if let Ok(mut guard) = entries_for_loader.lock() {
                            for w in batch {
                                if !guard.iter().any(|x| x.id == w.id) {
                                    fresh_items.push(CarouselItem {
                                        title: w.title.clone(),
                                        path: w.thumb_path.clone(),
                                        is_current: false,
                                    });
                                    guard.push(w);
                                }
                            }
                            if !fresh_items.is_empty() {
                                let ready_entries: Vec<_> = guard
                                    .iter()
                                    .filter(|w| w.thumb_path.is_file())
                                    .cloned()
                                    .collect();
                                if !ready_entries.is_empty() {
                                    literofi_modes::wallpaper::save_workshop_cache(
                                        &loader_key,
                                        &ready_entries,
                                    );
                                }
                            }
                        }
                        fresh_items
                    }),
                    Some(
                        move |notify: Box<
                            dyn Fn(Vec<std::path::PathBuf>) -> bool + Send + Sync,
                        >| {
                            if prefetch_entries.is_empty() {
                                return;
                            }
                            let len = prefetch_entries.len();
                            let mut wave1_indices = Vec::with_capacity(6);
                            for raw in [0isize, 1, 2, 3, -1, -2] {
                                let idx = raw.rem_euclid(len as isize) as usize;
                                if !wave1_indices.contains(&idx) {
                                    wave1_indices.push(idx);
                                }
                            }
                            let wave1: Vec<_> = wave1_indices
                                .iter()
                                .filter_map(|&i| prefetch_entries.get(i).cloned())
                                .collect();
                            let ready1 =
                                literofi_modes::wallpaper::download_workshop_thumbs_fast(&wave1);
                            if !ready1.is_empty() && !notify(ready1) {
                                return;
                            }
                            if let Ok(guard) = entries_for_cache.lock() {
                                let ready_entries: Vec<_> = guard
                                    .iter()
                                    .filter(|w| w.thumb_path.is_file())
                                    .cloned()
                                    .collect();
                                if !ready_entries.is_empty() {
                                    literofi_modes::wallpaper::save_workshop_cache(
                                        &cache_key,
                                        &ready_entries,
                                    );
                                }
                            }

                            let hd1 =
                                literofi_modes::wallpaper::download_workshop_hd_thumbs(&wave1);
                            if !hd1.is_empty() && !notify(hd1) {
                                return;
                            }

                            let rest: Vec<_> = prefetch_entries
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| !wave1_indices.contains(i))
                                .map(|(_, e)| e.clone())
                                .collect();
                            for chunk in rest.chunks(6) {
                                let ready =
                                    literofi_modes::wallpaper::download_workshop_thumbs_fast(chunk);
                                if !ready.is_empty() && !notify(ready) {
                                    return;
                                }
                                let hd =
                                    literofi_modes::wallpaper::download_workshop_hd_thumbs(chunk);
                                if !hd.is_empty() && !notify(hd) {
                                    return;
                                }
                            }
                            if let Ok(guard) = entries_for_cache.lock() {
                                let ready_entries: Vec<_> = guard
                                    .iter()
                                    .filter(|w| w.thumb_path.is_file())
                                    .cloned()
                                    .collect();
                                if !ready_entries.is_empty() {
                                    literofi_modes::wallpaper::save_workshop_cache(
                                        &cache_key,
                                        &ready_entries,
                                    );
                                }
                            }
                        },
                    ),
                )?;

                match outcome {
                    CarouselOutcome::Cancelled => return Ok(ExitCode::from(1)),
                    CarouselOutcome::ToggleSource => {
                        source = CarouselSource::Web;
                        continue;
                    }
                    CarouselOutcome::SwitchToWallpaperEngine => {
                        source = CarouselSource::Local;
                        continue;
                    }
                    CarouselOutcome::RefreshWeb => {
                        we_force_refresh = true;
                        continue;
                    }
                    CarouselOutcome::OpenInWallpaperEngine(idx) => {
                        let chosen = shared_entries.lock().ok().and_then(|g| g.get(idx).cloned());
                        if let Some(w) = chosen {
                            literofi_modes::wallpaper::open_steam_workshop_item(&w.id);
                        }
                        let _ = literofi_modes::wallpaper::open_wallpaper_engine_ui();
                        return Ok(ExitCode::SUCCESS);
                    }
                    CarouselOutcome::Apply(idx) => {
                        let chosen = shared_entries.lock().ok().and_then(|g| g.get(idx).cloned());
                        if let Some(w) = chosen {
                            literofi_modes::wallpaper::spawn_we_subscribe_helper(
                                &w.id,
                                target_monitor.index,
                            );
                        }
                        return Ok(ExitCode::SUCCESS);
                    }
                    CarouselOutcome::ApplyWithTheme(idx) => {
                        let chosen = shared_entries.lock().ok().and_then(|g| g.get(idx).cloned());
                        if let Some(w) = chosen {
                            literofi_modes::wallpaper::spawn_we_subscribe_helper(
                                &w.id,
                                target_monitor.index,
                            );
                            if let Some(pal) = literofi_modes::wallpaper::extract_wallpaper_palette(
                                &w.thumb_path,
                                &w.title,
                            ) {
                                literofi_modes::color_picker::apply_palette_to_root(
                                    &config_root,
                                    &pal,
                                )?;
                            }
                        }
                        return Ok(ExitCode::SUCCESS);
                    }
                }
            }
        }
    }
}

#[cfg(windows)]
fn dump_wallpaper_carousel_from_config(cfg: &Config, out: &Path) -> Result<(i32, i32), String> {
    let (_config_root, _entries, items, accent) = wallpaper_carousel_inputs(cfg);
    literofi_render::wallpaper_carousel::dump_wallpaper_carousel_png(items, accent, out)
}

#[cfg(windows)]
fn parse_hex_rgb_tuple(hex: &str) -> Option<(f32, f32, f32)> {
    let h = hex.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&h[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&h[4..6], 16).ok()? as f32 / 255.0;
    Some((r, g, b))
}

#[cfg(windows)]
fn build_music_card_data(
    snap: literofi_modes::music::MediaSnapshot,
) -> literofi_render::music_widget::MusicCardData {
    let wcfg = literofi_core::widgets_config::WidgetsConfig::load_default();
    let fallback_pal = literofi_modes::active_palette::ActivePalette::load_from_user_config();
    let mut bg = fallback_pal
        .as_ref()
        .map(|p| (p.bg.r as f32, p.bg.g as f32, p.bg.b as f32))
        .unwrap_or((0.10, 0.12, 0.15));
    let mut bg_alt = fallback_pal
        .as_ref()
        .map(|p| (p.bg_alt.r as f32, p.bg_alt.g as f32, p.bg_alt.b as f32))
        .unwrap_or((0.15, 0.18, 0.22));
    let mut accent = fallback_pal
        .as_ref()
        .map(|p| (p.accent.r as f32, p.accent.g as f32, p.accent.b as f32))
        .unwrap_or((0.28, 0.78, 0.72));

    if wcfg.music.theme_mode == "custom" {
        if let Some(c) = parse_hex_rgb_tuple(&wcfg.music.bg) {
            bg = c;
        }
        if let Some(c) = parse_hex_rgb_tuple(&wcfg.music.bg_alt) {
            bg_alt = c;
        }
        if let Some(c) = parse_hex_rgb_tuple(&wcfg.music.accent) {
            accent = c;
        }
    } else if wcfg.music.theme_mode == "auto" {
        // Cover-Art Auto-Accent: derive card tint & accent directly from the playing song's cover
        if let Some(cover) = snap.cover_path.as_ref().filter(|p| p.is_file()) {
            if let Some(cp) = literofi_modes::wallpaper::extract_wallpaper_palette(cover, "cover") {
                if let Some(c) = parse_hex_rgb_tuple(&cp.bg) {
                    bg = c;
                }
                if let Some(c) = parse_hex_rgb_tuple(&cp.bg_alt) {
                    bg_alt = c;
                }
                if let Some(c) = parse_hex_rgb_tuple(&cp.selected) {
                    accent = c;
                }
            }
        }
    }

    let (volume, muted) = literofi_modes::music::get_master_volume();
    let corner_radius = if wcfg.music.theme_mode == "custom" {
        14.0
    } else {
        fallback_pal
            .as_ref()
            .and_then(|p| p.border_radius)
            .unwrap_or(14.0)
    };
    let font_family =
        if wcfg.music.theme_mode != "custom" && wcfg.music.font.eq_ignore_ascii_case("Segoe UI") {
            fallback_pal
                .as_ref()
                .and_then(|p| p.font_family.clone())
                .unwrap_or(wcfg.music.font)
        } else {
            wcfg.music.font
        };
    literofi_render::music_widget::MusicCardData {
        title: snap.title,
        artist: snap.artist,
        album: snap.album,
        is_playing: snap.state == literofi_modes::music::PlaybackState::Playing,
        cover_path: snap.cover_path,
        position_secs: snap.position_secs,
        duration_secs: snap.duration_secs,
        volume,
        muted,
        bg,
        bg_alt,
        accent,
        font_family,
        corner_radius,
    }
}

#[cfg(windows)]
fn run_music_widget_mode(cfg: &Config) -> Result<ExitCode, String> {
    use literofi_modes::music::{
        dispatch_media_action, load_last_media_snapshot, query_system_media_snapshot,
        seek_system_media, set_master_volume, toggle_master_mute, MediaAction,
    };
    use literofi_render::music_widget::{run_music_widget, MusicWidgetCommand};

    let cache_dir = literofi_modes::resolve_cache_dir(cfg);
    let mut last_valid = load_last_media_snapshot(&cache_dir);
    let init_data = build_music_card_data(last_valid.clone());

    run_music_widget(init_data, move |cmd| {
        match cmd {
            MusicWidgetCommand::SetVolume(v) => {
                let (lvl, muted) = set_master_volume(v);
                let mut data = build_music_card_data(last_valid.clone());
                data.volume = lvl;
                data.muted = muted;
                return data;
            }
            MusicWidgetCommand::ToggleMute => {
                let (lvl, muted) = toggle_master_mute();
                let mut data = build_music_card_data(last_valid.clone());
                data.volume = lvl;
                data.muted = muted;
                return data;
            }
            MusicWidgetCommand::Previous | MusicWidgetCommand::Next => {
                let prev_title = last_valid.title.clone();
                let prev_artist = last_valid.artist.clone();
                let action = if cmd == MusicWidgetCommand::Previous {
                    MediaAction::Previous
                } else {
                    MediaAction::Next
                };
                dispatch_media_action(action);
                // Poll SMTC quickly (every 70ms up to ~630ms) so the new track appears the instant
                // Spotify/YouTube Music updates metadata instead of waiting for the 3s timer.
                for _ in 0..9 {
                    std::thread::sleep(std::time::Duration::from_millis(70));
                    let probe = query_system_media_snapshot(&cache_dir);
                    let changed = (!probe.title.is_empty() || !probe.artist.is_empty())
                        && (probe.title != prev_title || probe.artist != prev_artist);
                    if changed {
                        last_valid = probe.clone();
                        return build_music_card_data(probe);
                    }
                }
            }
            MusicWidgetCommand::TogglePlayPause => {
                dispatch_media_action(MediaAction::TogglePlayPause);
                std::thread::sleep(std::time::Duration::from_millis(75));
            }
            MusicWidgetCommand::Stop => {
                dispatch_media_action(MediaAction::Stop);
                std::thread::sleep(std::time::Duration::from_millis(85));
            }
            MusicWidgetCommand::Seek(secs) => {
                let _ = seek_system_media(secs);
                std::thread::sleep(std::time::Duration::from_millis(70));
            }
            MusicWidgetCommand::Refresh => {}
        }
        let mut snap = query_system_media_snapshot(&cache_dir);
        let got_track = !snap.title.is_empty() || !snap.artist.is_empty();
        let had_track = !last_valid.title.is_empty() || !last_valid.artist.is_empty();
        if !got_track && had_track && cmd != MusicWidgetCommand::Stop {
            if snap.state != literofi_modes::music::PlaybackState::NoSession {
                last_valid.state = snap.state;
            }
            snap = last_valid.clone();
        } else if got_track {
            if snap.cover_path.is_none()
                && snap.title == last_valid.title
                && snap.artist == last_valid.artist
            {
                snap.cover_path = last_valid.cover_path.clone();
            }
            last_valid = snap.clone();
        }
        build_music_card_data(snap)
    })?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(windows)]
fn dump_music_widget_from_config(cfg: &Config, out: &Path) -> Result<(i32, i32), String> {
    let cache_dir = literofi_modes::resolve_cache_dir(cfg);
    let mut snap = literofi_modes::music::query_system_media_snapshot(&cache_dir);
    if snap.title.is_empty() {
        snap.title = "Driver's Seat".to_string();
        snap.artist = "Sniff 'n' The Tears".to_string();
        snap.album = "Fickle Heart".to_string();
        snap.state = literofi_modes::music::PlaybackState::Playing;
        snap.position_secs = 138;
        snap.duration_secs = 239;
    }
    let data = build_music_card_data(snap);
    literofi_render::music_widget::dump_music_widget_png(data, out)
}

#[cfg(windows)]
fn games_palette_from_user_config() -> literofi_render::games_widget::GamesPalette {
    use literofi_core::widgets_config::{parse_hex_rgb, WidgetsConfig};
    use literofi_modes::active_palette::ActivePalette;
    use literofi_render::games_widget::GamesPalette;

    let wcfg = WidgetsConfig::load_default().games;
    if wcfg.theme_mode == "custom" {
        let ground = parse_hex_rgb(&wcfg.ground).unwrap_or((0.094, 0.102, 0.141));
        let surface = parse_hex_rgb(&wcfg.surface).unwrap_or((0.141, 0.157, 0.216));
        let ink = parse_hex_rgb(&wcfg.ink).unwrap_or((0.933, 0.945, 0.969));
        let ink_muted = (
            ink.0 * 0.68 + ground.0 * 0.32,
            ink.1 * 0.68 + ground.1 * 0.32,
            ink.2 * 0.68 + ground.2 * 0.32,
        );
        let accent = parse_hex_rgb(&wcfg.accent).unwrap_or((0.949, 0.957, 0.973));
        let accent_ink = parse_hex_rgb(&wcfg.accent_ink).unwrap_or((0.075, 0.086, 0.122));
        return GamesPalette {
            ground,
            surface,
            ink,
            ink_muted,
            accent,
            accent_ink,
            font_family: wcfg.font,
            corner_radius: wcfg.radius as f32,
        };
    }

    ActivePalette::load_from_user_config()
        .map(|p| {
            let luma = 0.2126 * p.bg.r + 0.7152 * p.bg.g + 0.7220 * 0.1 * p.bg.b;
            let (ground, surface, ink, ink_muted) = if luma > 0.55 {
                (
                    (0.102, 0.110, 0.145),
                    (0.153, 0.165, 0.216),
                    (0.933, 0.945, 0.969),
                    (0.651, 0.686, 0.761),
                )
            } else {
                (
                    (p.bg.r as f32, p.bg.g as f32, p.bg.b as f32),
                    (p.bg_alt.r as f32, p.bg_alt.g as f32, p.bg_alt.b as f32),
                    (p.fg.r as f32, p.fg.g as f32, p.fg.b as f32),
                    (
                        (p.fg.r * 0.68 + p.bg.r * 0.32) as f32,
                        (p.fg.g * 0.68 + p.bg.g * 0.32) as f32,
                        (p.fg.b * 0.68 + p.bg.b * 0.32) as f32,
                    ),
                )
            };
            let raw_acc = (p.accent.r as f32, p.accent.g as f32, p.accent.b as f32);
            let acc_luma = 0.2126 * raw_acc.0 + 0.7152 * raw_acc.1 + 0.0722 * raw_acc.2;
            let (accent, accent_ink) = if acc_luma < 0.36 {
                ((0.949, 0.957, 0.973), (0.075, 0.086, 0.122))
            } else {
                (
                    raw_acc,
                    (
                        p.accent_fg.r as f32,
                        p.accent_fg.g as f32,
                        p.accent_fg.b as f32,
                    ),
                )
            };
            let inherited_font = if wcfg.font.eq_ignore_ascii_case("Segoe UI") {
                p.font_family.clone().unwrap_or_else(|| wcfg.font.clone())
            } else {
                wcfg.font.clone()
            };
            let inherited_radius = p.border_radius.unwrap_or(wcfg.radius as f32);
            GamesPalette {
                ground,
                surface,
                ink,
                ink_muted,
                accent,
                accent_ink,
                font_family: inherited_font,
                corner_radius: inherited_radius,
            }
        })
        .unwrap_or_else(|| GamesPalette {
            font_family: wcfg.font,
            corner_radius: wcfg.radius as f32,
            ..Default::default()
        })
}

#[cfg(windows)]
fn catalog_records_to_cards(
    records: Vec<(
        literofi_modes::games::CachedGameRecord,
        Option<literofi_modes::games::GameAccess>,
    )>,
) -> Vec<literofi_render::games_widget::GameCardItem> {
    use literofi_modes::games::{format_last_played, game_access_key};
    use literofi_render::games_widget::GameCardItem;

    records
        .into_iter()
        .map(|(rec, acc)| {
            let g = rec.game;
            let key = game_access_key(&g);
            let library_uri = g
                .appid
                .as_ref()
                .map(|id| format!("steam://nav/games/details/{id}"));
            let last_played = acc.and_then(|a| format_last_played(a.last_played_epoch));
            GameCardItem {
                key,
                name: g.name,
                source: g.source.to_string(),
                launch_uri: g.launch_uri,
                library_uri,
                folder_path: rec.folder_path,
                cover_path: g.cover_path,
                hero_path: g.hero_path,
                logo_path: g.logo_path,
                last_played,
            }
        })
        .collect()
}

#[cfg(windows)]
fn run_games_widget_mode(_cfg: &Config) -> Result<ExitCode, String> {
    use literofi_modes::games::{
        clear_game_access, discover_games_catalog_fresh, launch_game_target,
        load_games_catalog_cached, record_game_launch,
    };
    use literofi_render::games_widget::{run_games_widget, GamesWidgetOutcome};

    let palette = games_palette_from_user_config();
    let (items, needs_bg_refresh) = match load_games_catalog_cached() {
        Some(cached) => (catalog_records_to_cards(cached), true),
        None => (
            catalog_records_to_cards(discover_games_catalog_fresh()),
            false,
        ),
    };
    let refresh_fn =
        needs_bg_refresh.then_some(|| catalog_records_to_cards(discover_games_catalog_fresh()));

    match run_games_widget(
        items,
        palette,
        |key| {
            clear_game_access(key);
        },
        refresh_fn,
    )? {
        GamesWidgetOutcome::Cancelled => Ok(ExitCode::from(1)),
        GamesWidgetOutcome::Launch { key, uri } => {
            record_game_launch(&key);
            launch_game_target(&uri)?;
            Ok(ExitCode::SUCCESS)
        }
        GamesWidgetOutcome::OpenLibrary { key, uri } => {
            record_game_launch(&key);
            launch_game_target(&uri)?;
            Ok(ExitCode::SUCCESS)
        }
        GamesWidgetOutcome::OpenFolder { key, path } => {
            record_game_launch(&key);
            let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
            Ok(ExitCode::SUCCESS)
        }
    }
}

#[cfg(windows)]
fn dump_games_widget_from_config(_cfg: &Config, out: &Path) -> Result<(i32, i32), String> {
    use literofi_modes::games::{discover_games_catalog_fresh, load_games_catalog_cached};

    let palette = games_palette_from_user_config();
    let records = load_games_catalog_cached().unwrap_or_else(discover_games_catalog_fresh);
    let items = catalog_records_to_cards(records);
    literofi_render::games_widget::dump_games_widget_png(items, palette, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_theme_override_does_not_leak_config_theme_display_drun() {
        let base = std::env::temp_dir().join(format!(
            "literofi_theme_override_leak_{}",
            std::process::id()
        ));
        let themes = base.join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::write(
            themes.join("old_theme.rasi"),
            "configuration { display-drun: \"\"; }\n* { background-color: #111111; }\n",
        )
        .unwrap();
        std::fs::write(
            themes.join("new_theme.rasi"),
            "configuration { sidebar-mode: true; }\n* { background-color: #222d32; }\n",
        )
        .unwrap();
        let cfg_path = base.join("config.rasi");
        std::fs::write(
            &cfg_path,
            "configuration { modes: \"drun,run\"; }\n@theme \"old_theme\"\n",
        )
        .unwrap();

        let mut cfg = Config {
            config_file: Some(cfg_path.to_string_lossy().into_owned()),
            theme: Some("new_theme".to_string()),
            ..Config::default()
        };
        let _sheet = load_theme_and_config(&mut cfg).unwrap();
        assert_eq!(
            cfg.display_drun, None,
            "old @theme's display-drun must not leak when CLI -theme overrides it"
        );
        assert!(cfg.sidebar_mode);

        let _ = std::fs::remove_dir_all(&base);
    }
}
