//! `run` mode implementation mirroring `source/modes/run.c`.
//! Scans Windows `PATH` + `PATHEXT` executables, merges `rofi-3.runcache` history,
//! and launches commands directly or in terminal (`-run-shell-command`).

use crate::{
    expand_terminal_argv, resolve_cache_dir, spawn_command_argv, ModeStep, ReloadHints, RichMode,
};
use literofi_core::cli::parse_command_template;
use literofi_core::config::Config;
use literofi_core::history::{history_get_list, history_remove, history_set, HistoryConfig};
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUN_CACHE_FILE: &str = "rofi-3.runcache";

/// Entry backing `run` switcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEntry {
    pub command: String,
    pub full_path: Option<PathBuf>,
    pub from_history: bool,
}

/// Stateful `run` switcher mode.
pub struct RunMode {
    cfg: Config,
    display_name: String,
    cache_dir: PathBuf,
    extra_path_dirs: Vec<PathBuf>,
    use_system_path: bool,
    entries: Vec<RunEntry>,
    reload_hints: ReloadHints,
    dry_run: bool,
    launched: Vec<Vec<String>>,
}

impl RunMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            cfg: cfg.clone(),
            display_name: cfg.display_run.clone().unwrap_or_else(|| "run".to_string()),
            cache_dir: resolve_cache_dir(cfg),
            extra_path_dirs: Vec::new(),
            use_system_path: true,
            entries: Vec::new(),
            reload_hints: ReloadHints::default(),
            dry_run: false,
            launched: Vec::new(),
        }
    }

    /// Configures an isolated `run` instance for deterministic tests (no OS process spawn).
    pub fn with_test_env(
        mut self,
        extra_dirs: Vec<PathBuf>,
        use_system_path: bool,
        cache_dir: PathBuf,
        dry_run: bool,
    ) -> Self {
        self.extra_path_dirs = extra_dirs;
        self.use_system_path = use_system_path;
        self.cache_dir = cache_dir;
        self.dry_run = dry_run;
        self
    }

    pub fn launched_commands(&self) -> &[Vec<String>] {
        &self.launched
    }

    fn history_path(&self) -> PathBuf {
        self.cache_dir.join(RUN_CACHE_FILE)
    }

    fn history_cfg(&self) -> HistoryConfig {
        HistoryConfig {
            disabled: self.cfg.disable_history,
            max_size: self.cfg.max_history_size,
            ignored_prefixes: self.cfg.ignored_prefixes.clone(),
        }
    }

    fn record_history(&self, cmd: &str) {
        if self.cfg.disable_history || cmd.trim().is_empty() {
            return;
        }
        let path = self.history_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = history_set(&path, cmd, &self.history_cfg());
    }

    fn execute_entry(&mut self, cmd: &str, in_terminal: bool) -> Result<(), String> {
        self.record_history(cmd);
        let argv = if in_terminal {
            expand_terminal_argv(
                &self.cfg.terminal,
                &self.cfg.ssh_client,
                &self.cfg.run_shell_command,
                cmd,
            )?
        } else {
            parse_command_template(
                &self.cfg.run_command,
                &self.cfg.terminal,
                &self.cfg.ssh_client,
                &[("{cmd}", Some(cmd))],
            )?
        };
        let log = self.dry_run.then_some(&mut self.launched);
        spawn_command_argv(&argv, None, log)
    }
}

impl RichMode for RunMode {
    fn name(&self) -> &str {
        "run"
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn init(&mut self) -> Result<(), String> {
        let ignored: Vec<&str> = self
            .cfg
            .ignored_prefixes
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        let mut seen: HashSet<String> = HashSet::new();
        let mut out: Vec<RunEntry> = Vec::new();

        // 1. History items first (unless disabled)
        if !self.cfg.disable_history {
            let hist = history_get_list(&self.history_path(), false).unwrap_or_default();
            for item in hist {
                if ignored.iter().any(|p| item.starts_with(p)) {
                    continue;
                }
                let key = item.to_ascii_lowercase();
                if seen.insert(key) {
                    let full_path = crate::find_on_path(&item);
                    out.push(RunEntry {
                        command: item,
                        full_path,
                        from_history: true,
                    });
                }
            }
        }

        // 2. Scan directories for PATHEXT executables
        let pathext = parse_pathext();
        let mut dirs: Vec<PathBuf> = self.extra_path_dirs.clone();
        if self.use_system_path {
            if let Some(path_os) = std::env::var_os("PATH") {
                dirs.extend(std::env::split_paths(&path_os));
            }
        }

        let mut discovered: Vec<(String, Option<PathBuf>)> = Vec::new();
        for dir in dirs {
            collect_executables_in_dir(&dir, &pathext, &ignored, &mut seen, &mut discovered);
        }

        // 3. Optional `-run-list-command`
        if !self.cfg.run_list_command.trim().is_empty() {
            if let Ok(extra) = run_list_command(&self.cfg.run_list_command) {
                for line in extra {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || ignored.iter().any(|p| trimmed.starts_with(p)) {
                        continue;
                    }
                    let key = trimmed.to_ascii_lowercase();
                    if seen.insert(key) {
                        let full_path = crate::find_on_path(trimmed);
                        discovered.push((trimmed.to_string(), full_path));
                    }
                }
            }
        }

        discovered.sort_by_key(|a| a.0.to_ascii_lowercase());
        for (command, full_path) in discovered {
            out.push(RunEntry {
                command,
                full_path,
                from_history: false,
            });
        }

        self.entries = out;
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.entries
            .iter()
            .map(|e| ListEntry {
                text: e.command.clone(),
                icon: e
                    .full_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                active: e.from_history,
                ..ListEntry::default()
            })
            .collect()
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Accepted {
                index, exit_code, ..
            } => {
                if *exit_code >= 10 {
                    return Ok(ModeStep::Exit(*exit_code));
                }
                let cmd = self
                    .entries
                    .get(*index)
                    .map(|e| e.command.clone())
                    .ok_or_else(|| format!("run entry index {index} out of bounds"))?;
                if self.execute_entry(&cmd, false).is_err() {
                    self.reload_hints = ReloadHints {
                        keep_selection: true,
                        new_selection: None,
                        keep_filter: true,
                    };
                    return Ok(ModeStep::Reload);
                }
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                let cmd = self
                    .entries
                    .get(*index)
                    .map(|e| e.command.clone())
                    .ok_or_else(|| format!("run entry index {index} out of bounds"))?;
                if self.execute_entry(&cmd, true).is_err() {
                    return Ok(ModeStep::Reload);
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::CustomInput { text, exit_code } => {
                if text.trim().is_empty() {
                    return Ok(ModeStep::Exit(1));
                }
                if self.execute_entry(text.trim(), false).is_err() {
                    return Ok(ModeStep::Reload);
                }
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::CustomInputAlt { text } => {
                if text.trim().is_empty() {
                    return Ok(ModeStep::Exit(1));
                }
                if self.execute_entry(text.trim(), true).is_err() {
                    return Ok(ModeStep::Reload);
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { index } => {
                if let Some(entry) = self.entries.get(*index).cloned() {
                    let _ = history_remove(
                        &self.history_path(),
                        &entry.command,
                        self.cfg.disable_history,
                        self.cfg.max_history_size,
                    );
                    self.init()?;
                }
                self.reload_hints = ReloadHints {
                    keep_selection: true,
                    new_selection: None,
                    keep_filter: true,
                };
                Ok(ModeStep::Reload)
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            UiOutcome::Back => Ok(ModeStep::Reload),
        }
    }
}

fn parse_pathext() -> Vec<String> {
    let raw = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD;.PS1".to_string());
    let mut exts: Vec<String> = raw
        .split(';')
        .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if exts.is_empty() {
        exts.extend(["exe", "cmd", "bat", "com", "ps1"].map(str::to_string));
    }
    exts
}

fn collect_executables_in_dir(
    dir: &Path,
    pathext: &[String],
    ignored: &[&str],
    seen: &mut HashSet<String>,
    out: &mut Vec<(String, Option<PathBuf>)>,
) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for item in rd.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext_low = ext.to_ascii_lowercase();
        if !pathext.iter().any(|p| p == &ext_low) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if ignored.iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if seen.insert(key) {
            out.push((name.to_string(), Some(path)));
        }
    }
}

fn run_list_command(cmd_str: &str) -> Result<Vec<String>, String> {
    #[cfg(windows)]
    let out = Command::new("cmd.exe")
        .args(["/C", cmd_str])
        .output()
        .map_err(|e| format!("`-run-list-command` failed: {e}"))?;
    #[cfg(not(windows))]
    let out = Command::new("sh")
        .args(["-c", cmd_str])
        .output()
        .map_err(|e| format!("`-run-list-command` failed: {e}"))?;

    if !out.status.success() {
        return Err(format!("`-run-list-command` exited with {}", out.status));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.lines().map(str::to_string).collect())
}
