//! `filebrowser` mode implementation mirroring `source/modes/filebrowser.c`.

use crate::{
    expand_tilde_path, open_path_with_command, resolve_cache_dir, user_home_dir, ModeStep,
    ReloadHints, RichMode,
};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::cmp::Ordering;
use std::path::PathBuf;
use std::time::SystemTime;

const FILEBROWSER_CACHE_FILE: &str = "rofi3.filebrowsercache";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FbEntryKind {
    Up,
    Directory,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FbSortMethod {
    Name,
    Mtime,
    Atime,
    Ctime,
}

impl FbSortMethod {
    fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "mtime" => Self::Mtime,
            "atime" => Self::Atime,
            "ctime" => Self::Ctime,
            _ => Self::Name,
        }
    }
}

#[derive(Debug, Clone)]
struct FbEntry {
    name: String,
    path: PathBuf,
    kind: FbEntryKind,
    sort_time: u64,
}

/// Stateful interactive directory browser mode (`filebrowser`).
pub struct FileBrowserMode {
    cfg: Config,
    display_name: String,
    cache_dir: PathBuf,
    current_dir: PathBuf,
    show_hidden: bool,
    entries: Vec<FbEntry>,
    reload_hints: ReloadHints,
    dry_run: bool,
    opened: Vec<String>,
}

impl FileBrowserMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            cfg: cfg.clone(),
            display_name: cfg
                .display_filebrowser
                .clone()
                .unwrap_or_else(|| "filebrowser".to_string()),
            cache_dir: resolve_cache_dir(cfg),
            current_dir: PathBuf::new(),
            show_hidden: cfg.filebrowser_show_hidden,
            entries: Vec::new(),
            reload_hints: ReloadHints::default(),
            dry_run: false,
            opened: Vec::new(),
        }
    }

    pub fn with_dir_and_cache(mut self, dir: PathBuf, cache_dir: PathBuf, dry_run: bool) -> Self {
        self.cfg.filebrowser_directory = Some(dir.to_string_lossy().into_owned());
        self.cache_dir = cache_dir;
        self.dry_run = dry_run;
        self
    }

    pub fn current_dir(&self) -> &PathBuf {
        &self.current_dir
    }

    pub fn opened_paths(&self) -> &[String] {
        &self.opened
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    fn resolve_initial_dir(&self) -> PathBuf {
        if let Some(dir_str) = self.cfg.filebrowser_directory.as_deref() {
            let p = expand_tilde_path(dir_str);
            if p.is_dir() {
                return p;
            }
        }
        let cache_file = self.cache_dir.join(FILEBROWSER_CACHE_FILE);
        if let Ok(cached) = std::fs::read_to_string(&cache_file) {
            let p = PathBuf::from(cached.trim());
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

    fn persist_current_dir(&self) {
        let _ = std::fs::create_dir_all(&self.cache_dir);
        let cache_file = self.cache_dir.join(FILEBROWSER_CACHE_FILE);
        let _ = std::fs::write(cache_file, self.current_dir.to_string_lossy().as_bytes());
    }

    fn load_dir(&mut self) -> Result<(), String> {
        let sort_method = FbSortMethod::parse(&self.cfg.filebrowser_sorting_method);
        let dirs_first = self.cfg.filebrowser_directories_first;
        let show_hidden = self.show_hidden;

        let rd = std::fs::read_dir(&self.current_dir).map_err(|e| {
            format!(
                "failed to read directory `{}`: {e}",
                self.current_dir.display()
            )
        })?;

        let mut items: Vec<FbEntry> = Vec::new();
        for item in rd.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if name == "." || name == ".." {
                continue;
            }
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let path = item.path();
            let meta = item.metadata().ok();
            let is_dir = meta.as_ref().map_or_else(|| path.is_dir(), |m| m.is_dir());
            let sort_time = meta
                .as_ref()
                .and_then(|m| extract_time(m, sort_method))
                .unwrap_or(0);
            items.push(FbEntry {
                name,
                path,
                kind: if is_dir {
                    FbEntryKind::Directory
                } else {
                    FbEntryKind::File
                },
                sort_time,
            });
        }

        items.sort_by(|a, b| {
            if dirs_first {
                let da = a.kind == FbEntryKind::Directory;
                let db = b.kind == FbEntryKind::Directory;
                if da != db {
                    return db.cmp(&da);
                }
            }
            match sort_method {
                FbSortMethod::Name => a
                    .name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
                    .then_with(|| a.name.cmp(&b.name)),
                FbSortMethod::Mtime | FbSortMethod::Atime | FbSortMethod::Ctime => {
                    match b.sort_time.cmp(&a.sort_time) {
                        Ordering::Equal => a
                            .name
                            .to_ascii_lowercase()
                            .cmp(&b.name.to_ascii_lowercase()),
                        other => other,
                    }
                }
            }
        });

        let mut full = Vec::with_capacity(items.len() + 1);
        if let Some(parent) = self.current_dir.parent() {
            full.push(FbEntry {
                name: "..".to_string(),
                path: parent.to_path_buf(),
                kind: FbEntryKind::Up,
                sort_time: 0,
            });
        }
        full.extend(items);
        self.entries = full;
        Ok(())
    }

    fn navigate_to(&mut self, dir: PathBuf) -> Result<(), String> {
        self.current_dir = dir;
        self.persist_current_dir();
        self.load_dir()?;
        self.reload_hints = ReloadHints {
            keep_selection: false,
            new_selection: None,
            keep_filter: false,
        };
        Ok(())
    }
}

fn extract_time(meta: &std::fs::Metadata, method: FbSortMethod) -> Option<u64> {
    let st = match method {
        FbSortMethod::Name => return None,
        FbSortMethod::Mtime => meta.modified().ok()?,
        FbSortMethod::Atime => meta.accessed().ok()?,
        FbSortMethod::Ctime => meta.created().ok().or_else(|| meta.modified().ok())?,
    };
    st.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

impl RichMode for FileBrowserMode {
    fn name(&self) -> &str {
        "filebrowser"
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn init(&mut self) -> Result<(), String> {
        self.current_dir = self.resolve_initial_dir();
        self.load_dir()?;
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.entries
            .iter()
            .map(|e| ListEntry {
                text: e.name.clone(),
                display: Some(if e.kind == FbEntryKind::Directory {
                    format!("{}/", e.name)
                } else {
                    e.name.clone()
                }),
                info: Some(e.path.to_string_lossy().into_owned()),
                icon: (e.kind != FbEntryKind::Up).then(|| e.path.to_string_lossy().into_owned()),
                active: e.kind == FbEntryKind::Directory,
                ..ListEntry::default()
            })
            .collect()
    }

    fn prompt_override(&self) -> Option<String> {
        Some(self.current_dir.to_string_lossy().into_owned())
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
                let Some(entry) = self.entries.get(*index).cloned() else {
                    return Ok(ModeStep::Reload);
                };
                match entry.kind {
                    FbEntryKind::Up | FbEntryKind::Directory => {
                        self.navigate_to(entry.path)?;
                        Ok(ModeStep::Reload)
                    }
                    FbEntryKind::File => {
                        let log = self.dry_run.then_some(&mut self.opened);
                        open_path_with_command(&entry.path, &self.cfg.filebrowser_command, log)?;
                        Ok(ModeStep::Exit(*exit_code))
                    }
                }
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                let Some(entry) = self.entries.get(*index).cloned() else {
                    return Ok(ModeStep::Reload);
                };
                if entry.kind == FbEntryKind::Up {
                    self.navigate_to(entry.path)?;
                    return Ok(ModeStep::Reload);
                }
                let log = self.dry_run.then_some(&mut self.opened);
                open_path_with_command(&entry.path, &self.cfg.filebrowser_command, log)?;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::CustomInput { text, .. } => {
                let candidate = expand_tilde_path(text.trim());
                let resolved = if candidate.is_relative() {
                    self.current_dir.join(candidate)
                } else {
                    candidate
                };
                if resolved.is_dir() {
                    self.navigate_to(resolved)?;
                }
                Ok(ModeStep::Reload)
            }
            UiOutcome::CustomInputAlt { .. } => {
                if let Some(parent) = self.current_dir.parent().map(PathBuf::from) {
                    self.navigate_to(parent)?;
                }
                Ok(ModeStep::Reload)
            }
            UiOutcome::DeleteEntry { .. } => {
                self.show_hidden = !self.show_hidden;
                self.load_dir()?;
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
