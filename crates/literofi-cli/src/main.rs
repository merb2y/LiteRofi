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
    if let Some(m) = mode.message() {
        state.message = Some(m);
    }
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
