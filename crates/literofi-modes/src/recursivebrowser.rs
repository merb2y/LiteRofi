//! `recursivebrowser` mode implementation mirroring `source/modes/recursivebrowser.c`.

use crate::{
    expand_tilde_path, open_path_with_command, user_home_dir, ModeStep, ReloadHints, RichMode,
};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use regex::Regex;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

const DEFAULT_MAX_ENTRIES: usize = 50_000;

/// Stateful breadth-first recursive file browser mode (`recursivebrowser`).
pub struct RecursiveBrowserMode {
    cfg: Config,
    display_name: String,
    root_dir: PathBuf,
    max_entries: usize,
    entries: Vec<PathBuf>,
    reload_hints: ReloadHints,
    dry_run: bool,
    opened: Vec<String>,
}

impl RecursiveBrowserMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            cfg: cfg.clone(),
            display_name: cfg
                .display_recursivebrowser
                .clone()
                .unwrap_or_else(|| "recursivebrowser".to_string()),
            root_dir: PathBuf::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
            entries: Vec::new(),
            reload_hints: ReloadHints::default(),
            dry_run: false,
            opened: Vec::new(),
        }
    }

    pub fn with_dir(mut self, dir: PathBuf, max_entries: usize, dry_run: bool) -> Self {
        self.cfg.recursivebrowser_directory = Some(dir.to_string_lossy().into_owned());
        self.max_entries = max_entries.max(1);
        self.dry_run = dry_run;
        self
    }

    pub fn opened_paths(&self) -> &[String] {
        &self.opened
    }

    fn resolve_root(&self) -> PathBuf {
        if let Some(dir_str) = self.cfg.recursivebrowser_directory.as_deref() {
            let p = expand_tilde_path(dir_str);
            if p.is_dir() {
                return p;
            }
        }
        let home = user_home_dir();
        if home.is_dir() {
            return home;
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

impl RichMode for RecursiveBrowserMode {
    fn name(&self) -> &str {
        "recursivebrowser"
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn init(&mut self) -> Result<(), String> {
        let filter = Regex::new(&self.cfg.recursivebrowser_filter_regex).map_err(|e| {
            format!(
                "invalid `-recursivebrowser-filter-regex` `{}`: {e}",
                self.cfg.recursivebrowser_filter_regex
            )
        })?;
        self.root_dir = self.resolve_root();

        let mut queue = VecDeque::new();
        let mut visited_dirs: HashSet<String> = HashSet::new();
        let mut files: Vec<PathBuf> = Vec::new();
        queue.push_back(self.root_dir.clone());

        while let Some(dir) = queue.pop_front() {
            if files.len() >= self.max_entries {
                break;
            }
            let key = dir.to_string_lossy().to_ascii_lowercase();
            if !visited_dirs.insert(key) {
                continue;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for item in rd.flatten() {
                let name = item.file_name().to_string_lossy().into_owned();
                if name == "." || name == ".." || filter.is_match(&name) {
                    continue;
                }
                let path = item.path();
                let Ok(ft) = item.file_type() else {
                    continue;
                };
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_dir() {
                    queue.push_back(path);
                } else if ft.is_file() {
                    files.push(path);
                    if files.len() >= self.max_entries {
                        break;
                    }
                }
            }
        }

        files.sort();
        self.entries = files;
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.entries
            .iter()
            .map(|p| {
                let rel = p
                    .strip_prefix(&self.root_dir)
                    .map(|r| r.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| p.to_string_lossy().into_owned());
                ListEntry {
                    text: rel,
                    info: Some(p.to_string_lossy().into_owned()),
                    icon: Some(p.to_string_lossy().into_owned()),
                    ..ListEntry::default()
                }
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
                let path = self
                    .entries
                    .get(*index)
                    .cloned()
                    .ok_or_else(|| format!("recursivebrowser index {index} out of bounds"))?;
                let log = self.dry_run.then_some(&mut self.opened);
                open_path_with_command(&path, &self.cfg.recursivebrowser_command, log)?;
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                let path = self
                    .entries
                    .get(*index)
                    .cloned()
                    .ok_or_else(|| format!("recursivebrowser index {index} out of bounds"))?;
                let log = self.dry_run.then_some(&mut self.opened);
                open_path_with_command(&path, &self.cfg.recursivebrowser_command, log)?;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::CustomInput { .. }
            | UiOutcome::CustomInputAlt { .. }
            | UiOutcome::DeleteEntry { .. } => Ok(ModeStep::Reload),
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
        }
    }
}
