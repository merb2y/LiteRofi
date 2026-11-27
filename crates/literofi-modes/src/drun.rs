//! `drun` mode implementation mirroring `source/modes/drun.c`.
//! Discovers Windows Start Menu + `AppsFolder` apps (plus `.desktop` dirs in tests/custom setups),
//! formats with `-drun-display-format`, indexes `-drun-match-fields`, and persists `rofi-3.druncache`.

use crate::win32_apps::{
    discover_windows_apps, launch_windows_app, scan_desktop_dir, DiscoveredApp,
};
use crate::{resolve_cache_dir, ModeStep, ReloadHints, RichMode};
use literofi_core::cli::string_replace_if_exists;
use literofi_core::config::Config;
use literofi_core::history::{history_get_list, history_remove, history_set, HistoryConfig};
use literofi_core::protocol::strip_pango_markup;
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

const DRUN_CACHE_FILE: &str = "rofi-3.druncache";

#[derive(Debug, Clone)]
struct DrunItem {
    app: DiscoveredApp,
    display_label: String,
    meta: String,
}

/// Stateful `drun` application launcher mode.
pub struct DrunMode {
    cfg: Config,
    display_name: String,
    cache_dir: PathBuf,
    extra_desktop_dirs: Vec<PathBuf>,
    override_apps: Option<Vec<DiscoveredApp>>,
    use_windows_discovery: bool,
    items: Vec<DrunItem>,
    reload_hints: ReloadHints,
    dry_run: bool,
    launched: Vec<String>,
}

impl DrunMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            cfg: cfg.clone(),
            display_name: cfg
                .display_drun
                .clone()
                .unwrap_or_else(|| "drun".to_string()),
            cache_dir: resolve_cache_dir(cfg),
            extra_desktop_dirs: Vec::new(),
            override_apps: None,
            use_windows_discovery: true,
            items: Vec::new(),
            reload_hints: ReloadHints::default(),
            dry_run: false,
            launched: Vec::new(),
        }
    }

    pub fn with_desktop_dirs(
        mut self,
        desktop_dirs: Vec<PathBuf>,
        use_windows_discovery: bool,
        cache_dir: PathBuf,
        dry_run: bool,
    ) -> Self {
        self.extra_desktop_dirs = desktop_dirs;
        self.use_windows_discovery = use_windows_discovery;
        self.cache_dir = cache_dir;
        self.dry_run = dry_run;
        self
    }

    pub fn with_apps(
        mut self,
        apps: Vec<DiscoveredApp>,
        cache_dir: PathBuf,
        dry_run: bool,
    ) -> Self {
        self.override_apps = Some(apps);
        self.use_windows_discovery = false;
        self.cache_dir = cache_dir;
        self.dry_run = dry_run;
        self
    }

    pub fn launched_ids(&self) -> &[String] {
        &self.launched
    }

    fn history_path(&self) -> PathBuf {
        self.cache_dir.join(DRUN_CACHE_FILE)
    }

    fn history_cfg(&self) -> HistoryConfig {
        HistoryConfig {
            disabled: self.cfg.disable_history,
            max_size: self.cfg.max_history_size,
            ignored_prefixes: self.cfg.ignored_prefixes.clone(),
        }
    }

    fn passes_category_filters(&self, app: &DiscoveredApp) -> bool {
        if let Some(inc) = self.cfg.drun_categories.as_deref() {
            let wanted: Vec<&str> = inc
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if !wanted.is_empty()
                && !app
                    .categories
                    .iter()
                    .any(|c| wanted.iter().any(|w| c.eq_ignore_ascii_case(w)))
            {
                return false;
            }
        }
        if let Some(exc) = self.cfg.drun_exclude_categories.as_deref() {
            let blocked: Vec<&str> = exc
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if app
                .categories
                .iter()
                .any(|c| blocked.iter().any(|b| c.eq_ignore_ascii_case(b)))
            {
                return false;
            }
        }
        true
    }

    fn format_display(&self, app: &DiscoveredApp) -> String {
        let generic = app.generic.as_deref().unwrap_or("");
        let comment = app.comment.as_deref().unwrap_or("");
        let cats = app.categories.join(" ");
        let kws = app.keywords.join(" ");
        let raw = string_replace_if_exists(
            &self.cfg.drun_display_format,
            &[
                ("{icon}", None),
                ("{name}", Some(app.name.as_str())),
                ("{generic}", (!generic.is_empty()).then_some(generic)),
                (
                    "{exec}",
                    (!app.exec.is_empty()).then_some(app.exec.as_str()),
                ),
                ("{comment}", (!comment.is_empty()).then_some(comment)),
                ("{categories}", (!cats.is_empty()).then_some(cats.as_str())),
                ("{keywords}", (!kws.is_empty()).then_some(kws.as_str())),
            ],
        );
        if self.cfg.markup_rows {
            raw.trim().to_string()
        } else {
            strip_pango_markup(&raw).trim().to_string()
        }
    }

    fn build_meta(&self, app: &DiscoveredApp) -> String {
        let fields: HashSet<&str> = self
            .cfg
            .drun_match_fields
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let all = fields.is_empty() || fields.contains("all");
        let mut parts = Vec::new();
        if all || fields.contains("name") {
            parts.push(app.name.as_str());
        }
        if (all || fields.contains("generic")) && app.generic.is_some() {
            if let Some(g) = app.generic.as_deref() {
                parts.push(g);
            }
        }
        if (all || fields.contains("comment")) && app.comment.is_some() {
            if let Some(c) = app.comment.as_deref() {
                parts.push(c);
            }
        }
        if (all || fields.contains("exec")) && !app.exec.is_empty() {
            parts.push(app.exec.as_str());
        }
        let cats = app.categories.join(" ");
        if (all || fields.contains("categories")) && !cats.is_empty() {
            parts.push(cats.as_str());
        }
        let kws = app.keywords.join(" ");
        if (all || fields.contains("keywords")) && !kws.is_empty() {
            parts.push(kws.as_str());
        }
        parts.join(" ")
    }

    fn launch_index(&mut self, idx: usize) -> Result<(), String> {
        let Some(item) = self.items.get(idx).cloned() else {
            return Err(format!("drun index {idx} out of bounds"));
        };
        let launch_res = if self.dry_run {
            self.launched.push(item.app.id.clone());
            Ok(())
        } else {
            launch_windows_app(&item.app.launch)
        };
        if !self.cfg.disable_history {
            let path = self.history_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = history_set(&path, &item.app.id, &self.history_cfg());
        }
        launch_res
    }
}

impl RichMode for DrunMode {
    fn name(&self) -> &str {
        "drun"
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn init(&mut self) -> Result<(), String> {
        let mut raw_apps = Vec::new();
        if let Some(custom) = &self.override_apps {
            raw_apps.extend(custom.clone());
        }
        for dir in &self.extra_desktop_dirs {
            raw_apps.extend(scan_desktop_dir(dir));
        }
        if self.use_windows_discovery {
            raw_apps.extend(discover_windows_apps());
        }

        let mut by_id: HashMap<String, DiscoveredApp> = HashMap::new();
        for app in raw_apps {
            if !self.passes_category_filters(&app) {
                continue;
            }
            by_id.entry(app.id.to_ascii_lowercase()).or_insert(app);
        }

        let hist_order =
            history_get_list(&self.history_path(), self.cfg.disable_history).unwrap_or_default();

        let mut ordered: Vec<DrunItem> = Vec::new();
        let mut used: HashSet<String> = HashSet::new();

        for h in hist_order {
            let key = h.to_ascii_lowercase();
            if let Some(app) = by_id.remove(&key) {
                used.insert(key);
                let display_label = self.format_display(&app);
                let meta = self.build_meta(&app);
                ordered.push(DrunItem {
                    app,
                    display_label,
                    meta,
                });
            }
        }

        let mut rest: Vec<DiscoveredApp> = by_id
            .into_iter()
            .filter(|(k, _)| !used.contains(k))
            .map(|(_, v)| v)
            .collect();
        rest.sort_by_key(|a| a.name.to_ascii_lowercase());

        for app in rest {
            let display_label = self.format_display(&app);
            let meta = self.build_meta(&app);
            ordered.push(DrunItem {
                app,
                display_label,
                meta,
            });
        }

        self.items = ordered;
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.items
            .iter()
            .map(|item| ListEntry {
                text: item.display_label.clone(),
                display: Some(item.display_label.clone()),
                meta: (!item.meta.is_empty()).then(|| item.meta.clone()),
                info: Some(item.app.id.clone()),
                icon: item.app.icon.clone(),
                active: false,
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
                self.launch_index(*index)?;
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                self.launch_index(*index)?;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { index } => {
                if let Some(item) = self.items.get(*index) {
                    let _ = history_remove(
                        &self.history_path(),
                        &item.app.id,
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
            UiOutcome::CustomInput { .. } | UiOutcome::CustomInputAlt { .. } => {
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
