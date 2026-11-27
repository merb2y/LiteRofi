//! `window` and `windowcd` modes mirroring `source/modes/window.c`.

use crate::win32_windows::{activate_window, close_window, enumerate_windows, DiscoveredWindow};
use crate::{spawn_command_argv, ModeStep, ReloadHints, RichMode};
use literofi_core::cli::{parse_command_template, string_replace_if_exists};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::HashSet;

#[derive(Debug, Clone)]
struct WindowItem {
    win: DiscoveredWindow,
    formatted: String,
    meta: String,
}

/// Stateful `window` / `windowcd` switcher mode.
pub struct WindowMode {
    cfg: Config,
    current_desktop_only: bool,
    display_name: String,
    override_windows: Option<Vec<DiscoveredWindow>>,
    items: Vec<WindowItem>,
    reload_hints: ReloadHints,
    dry_run: bool,
    action_log: Vec<String>,
}

impl WindowMode {
    pub fn new(cfg: &Config, current_desktop_only: bool) -> Self {
        let display_name = if current_desktop_only {
            cfg.display_windowcd
                .clone()
                .unwrap_or_else(|| "windowcd".to_string())
        } else {
            cfg.display_window
                .clone()
                .unwrap_or_else(|| "window".to_string())
        };
        Self {
            cfg: cfg.clone(),
            current_desktop_only,
            display_name,
            override_windows: None,
            items: Vec::new(),
            reload_hints: ReloadHints::default(),
            dry_run: false,
            action_log: Vec::new(),
        }
    }

    pub fn with_mock_windows(mut self, windows: Vec<DiscoveredWindow>, dry_run: bool) -> Self {
        self.override_windows = Some(windows);
        self.dry_run = dry_run;
        self
    }

    pub fn action_log(&self) -> &[String] {
        &self.action_log
    }

    fn format_window(&self, w: &DiscoveredWindow) -> String {
        let mon = w.monitor_index.to_string();
        string_replace_if_exists(
            &self.cfg.window_format,
            &[
                ("{w}", Some(mon.as_str())),
                ("{c}", Some(w.process_name.as_str())),
                ("{t}", Some(w.title.as_str())),
                ("{n}", Some(w.class_name.as_str())),
                ("{r}", Some("")),
            ],
        )
    }

    fn build_meta(&self, w: &DiscoveredWindow) -> String {
        let fields: HashSet<&str> = self
            .cfg
            .window_match_fields
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let all = fields.is_empty() || fields.contains("all");
        let mon = w.monitor_index.to_string();
        let mut parts = Vec::new();
        if all || fields.contains("title") {
            parts.push(w.title.as_str());
        }
        if all || fields.contains("class") {
            parts.push(w.process_name.as_str());
        }
        if all || fields.contains("name") {
            parts.push(w.class_name.as_str());
        }
        if all || fields.contains("desktop") {
            parts.push(mon.as_str());
        }
        parts.join(" ")
    }
}

impl RichMode for WindowMode {
    fn name(&self) -> &str {
        if self.current_desktop_only {
            "windowcd"
        } else {
            "window"
        }
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn init(&mut self) -> Result<(), String> {
        let raw = match &self.override_windows {
            Some(list) => list.clone(),
            None => enumerate_windows(None),
        };
        let mut items = Vec::new();
        for w in raw {
            if self.current_desktop_only && !w.is_on_current_monitor {
                continue;
            }
            let formatted = self.format_window(&w);
            let meta = self.build_meta(&w);
            items.push(WindowItem {
                win: w,
                formatted,
                meta,
            });
        }
        self.items = items;
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.items
            .iter()
            .map(|item| ListEntry {
                text: item.formatted.clone(),
                display: Some(item.formatted.clone()),
                meta: (!item.meta.is_empty()).then(|| item.meta.clone()),
                info: Some(item.win.hwnd.to_string()),
                icon: Some(format!("hwnd:{}", item.win.hwnd)),
                active: item.win.is_active,
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
                let hwnd = self
                    .items
                    .get(*index)
                    .map(|w| w.win.hwnd)
                    .ok_or_else(|| format!("window index {index} out of bounds"))?;
                if self.dry_run {
                    self.action_log.push(format!("activate:{hwnd}"));
                } else {
                    activate_window(hwnd)?;
                }
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                let hwnd = self
                    .items
                    .get(*index)
                    .map(|w| w.win.hwnd)
                    .ok_or_else(|| format!("window index {index} out of bounds"))?;
                let id_str = hwnd.to_string();
                let argv = parse_command_template(
                    &self.cfg.window_command,
                    &self.cfg.terminal,
                    &self.cfg.ssh_client,
                    &[("{window}", Some(&id_str))],
                )?;
                if self.dry_run {
                    self.action_log.push(format!("cmd:{}", argv.join(" ")));
                } else {
                    spawn_command_argv(&argv, None, None)?;
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { index } => {
                if let Some(item) = self.items.get(*index) {
                    let hwnd = item.win.hwnd;
                    if self.dry_run {
                        self.action_log.push(format!("close:{hwnd}"));
                        self.items.remove(*index);
                    } else {
                        let _ = close_window(hwnd);
                        self.init()?;
                    }
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
