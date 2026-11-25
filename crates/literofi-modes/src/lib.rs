//! Mode implementations (`dmenu`, `run`, `drun`, `window`, `windowcd`, `combi`, `script`)
//! mirroring upstream `source/modes/*.c` with Windows-native app & window discovery.

pub mod color_picker;
pub mod combi;
pub mod dmenu;
pub mod drun;
pub mod filebrowser;
pub mod keys;
pub mod music;
pub mod recursivebrowser;
pub mod registry;
pub mod run;
pub mod script;
pub mod ssh;
pub mod theme_picker;
pub mod win32_apps;
pub mod win32_windows;
pub mod window;

use literofi_core::cli::split_shell_argv;
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Step returned by a mode after evaluating a user UI outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeStep {
    Exit(i32),
    Reload,
    PreviewTheme(PathBuf),
    ReloadTheme,
    SwitchMode(String),
    SwitchModeIndex(usize),
}

/// Hints applied by the CLI loop when reloading entries into `LauncherState`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReloadHints {
    pub keep_selection: bool,
    pub new_selection: Option<usize>,
    pub keep_filter: bool,
}

/// Unified session interface implemented by all Wave 4/5 modes.
pub trait RichMode {
    fn name(&self) -> &str;
    fn display_name(&self) -> &str {
        self.name()
    }
    fn init(&mut self) -> Result<(), String>;
    fn entries(&self) -> Vec<ListEntry>;
    fn message(&self) -> Option<String> {
        None
    }
    fn prompt_override(&self) -> Option<String> {
        None
    }
    fn theme_snippets(&self) -> Vec<String> {
        Vec::new()
    }
    fn no_custom_override(&self) -> Option<bool> {
        None
    }
    fn overlay(&self) -> Option<String> {
        None
    }
    fn reload_hints(&self) -> ReloadHints {
        ReloadHints::default()
    }
    fn handle_outcome(&mut self, outcome: &UiOutcome, query: &str) -> Result<ModeStep, String>;
}

/// Resolves Rofi-compatible cache directory (`-cache-dir` -> `%LOCALAPPDATA%\LiteRofi\cache`).
pub fn resolve_cache_dir(cfg: &Config) -> PathBuf {
    if let Some(custom) = cfg.cache_dir.as_deref() {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.is_empty() {
            return PathBuf::from(local).join("LiteRofi").join("cache");
        }
    }
    std::env::temp_dir().join("literofi-cache")
}

/// Resolves `-terminal` on Windows: translates `rofi-sensible-terminal` into Windows Terminal (`wt.exe`)
/// if present on `PATH`, falling back to `cmd.exe /K`.
pub fn resolve_windows_terminal(configured: &str) -> (String, Vec<String>) {
    let trimmed = configured.trim();
    if trimmed.is_empty() || trimmed == "rofi-sensible-terminal" {
        if find_on_path("wt.exe").is_some() {
            return (
                "wt.exe".to_string(),
                vec!["new-tab".to_string(), "--".to_string()],
            );
        }
        return ("cmd.exe".to_string(), vec!["/K".to_string()]);
    }
    let parts = split_shell_argv(trimmed).unwrap_or_default();
    if let Some((prog, rest)) = parts.split_first() {
        (prog.clone(), rest.to_vec())
    } else {
        ("cmd.exe".to_string(), vec!["/K".to_string()])
    }
}

/// Looks up an executable name on `PATH`.
pub fn find_on_path(exe_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Expands a terminal-wrapped command template (`{terminal}` + `{cmd}`), substituting
/// `rofi-sensible-terminal` intelligently on Windows.
pub fn expand_terminal_argv(
    terminal_cfg: &str,
    ssh_client: &str,
    template: &str,
    cmd: &str,
) -> Result<Vec<String>, String> {
    if terminal_cfg.trim().is_empty() || terminal_cfg.trim() == "rofi-sensible-terminal" {
        let (prog, mut base_args) = resolve_windows_terminal(terminal_cfg);
        let cmd_parts = split_shell_argv(cmd)?;
        let mut out = vec![prog];
        out.append(&mut base_args);
        if cmd_parts.is_empty() {
            out.push(cmd.to_string());
        } else {
            out.extend(cmd_parts);
        }
        return Ok(out);
    }
    literofi_core::cli::parse_command_template(
        template,
        terminal_cfg,
        ssh_client,
        &[("{cmd}", Some(cmd))],
    )
}

/// Spawns an argv list detached without blocking caller. If `dry_run_log` is `Some`, records
/// the argv without spawning any OS process (for isolated unit/E2E verification).
pub fn spawn_command_argv(
    argv: &[String],
    working_dir: Option<&Path>,
    dry_run_log: Option<&mut Vec<Vec<String>>>,
) -> Result<(), String> {
    if argv.is_empty() {
        return Err("empty command argv".to_string());
    }
    if let Some(log) = dry_run_log {
        log.push(argv.to_vec());
        return Ok(());
    }
    let mut cmd = Command::new(&argv[0]);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    if let Some(dir) = working_dir {
        if dir.is_dir() {
            cmd.current_dir(dir);
        }
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        Ok(_) => Ok(()),
        Err(first_err) => {
            // Fallback through `cmd /C` on Windows for shell built-ins or `.cmd`/`.bat` scripts.
            #[cfg(windows)]
            {
                let mut shell = Command::new("cmd.exe");
                shell.arg("/C").arg(argv.join(" "));
                if let Some(dir) = working_dir {
                    if dir.is_dir() {
                        shell.current_dir(dir);
                    }
                }
                shell
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if shell.spawn().is_ok() {
                    return Ok(());
                }
            }
            Err(format!("failed to execute `{}`: {first_err}", argv[0]))
        }
    }
}

/// Returns current user's home directory (`USERPROFILE` on Windows, `HOME` otherwise).
pub fn user_home_dir() -> PathBuf {
    for key in ["USERPROFILE", "HOME"] {
        if let Ok(val) = std::env::var(key) {
            if !val.trim().is_empty() {
                return PathBuf::from(val);
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Expands leading `~` or `~/`/`~\` against [`user_home_dir`].
pub fn expand_tilde_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return user_home_dir();
    }
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        return user_home_dir().join(rest);
    }
    PathBuf::from(trimmed)
}

/// Opens a file or directory using a configured launcher command or Windows Shell default (`xdg-open`).
pub fn open_path_with_command(
    path: &Path,
    command: &str,
    dry_run_log: Option<&mut Vec<String>>,
) -> Result<(), String> {
    let path_str = path.to_string_lossy().into_owned();
    if let Some(log) = dry_run_log {
        log.push(path_str);
        return Ok(());
    }
    let trimmed = command.trim();
    if trimmed.is_empty() || trimmed == "xdg-open" {
        return win32_apps::launch_windows_app(&win32_apps::AppLaunchTarget::ShellPath(
            path.to_path_buf(),
        ));
    }
    let mut argv = split_shell_argv(trimmed)?;
    argv.push(path_str);
    spawn_command_argv(&argv, path.parent(), None)
}
