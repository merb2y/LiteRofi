//! `dmenu` mode implementation mirroring `source/modes/dmenu.c`.

use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_core::protocol::{format_output_line, parse_dmenu_script_stream, parse_ranges};
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::BTreeSet;
use std::io::Read;

/// Stateful `dmenu` session supporting headers, `-a`/`-u`, `-multi-select`, `-only-match`,
/// `-no-custom`, `-format`, and custom exit codes `10..=28`.
pub struct DmenuMode {
    cfg: Config,
    raw_override: Option<Vec<u8>>,
    entries: Vec<ListEntry>,
    selected_set: BTreeSet<usize>,
    base_active: Vec<bool>,
    prompt_override: Option<String>,
    message: Option<String>,
    no_custom_override: Option<bool>,
    theme_snippets: Vec<String>,
    overlay: Option<String>,
    reload_hints: ReloadHints,
    outputs: Vec<String>,
    capture_only: bool,
}

impl DmenuMode {
    /// Creates a `dmenu` mode reading from `cfg.input` or stdin during `init()`.
    pub fn from_config(cfg: &Config) -> Self {
        Self {
            cfg: cfg.clone(),
            raw_override: None,
            entries: Vec::new(),
            selected_set: BTreeSet::new(),
            base_active: Vec::new(),
            prompt_override: None,
            message: cfg.mesg.clone(),
            no_custom_override: None,
            theme_snippets: Vec::new(),
            overlay: None,
            reload_hints: ReloadHints::default(),
            outputs: Vec::new(),
            capture_only: false,
        }
    }

    /// Creates an isolated in-memory `dmenu` session for tests (captures formatted output in `outputs()`).
    pub fn from_bytes(cfg: &Config, raw_bytes: Vec<u8>) -> Self {
        let mut s = Self::from_config(cfg);
        s.raw_override = Some(raw_bytes);
        s.capture_only = true;
        s
    }

    pub fn outputs(&self) -> &[String] {
        &self.outputs
    }

    pub fn selected_indices(&self) -> &BTreeSet<usize> {
        &self.selected_set
    }

    fn emit_line(&mut self, line: String) {
        if !self.capture_only {
            println!("{line}");
        }
        self.outputs.push(line);
    }

    fn refresh_multi_select_visuals(&mut self) {
        for (i, entry) in self.entries.iter_mut().enumerate() {
            let base = self.base_active.get(i).copied().unwrap_or(false);
            entry.active = base || self.selected_set.contains(&i);
        }
        self.overlay = if self.selected_set.is_empty() {
            None
        } else {
            Some(format!(
                "{}/{}",
                self.selected_set.len(),
                self.entries.len()
            ))
        };
    }

    fn print_accepted(&mut self, fallback_idx: usize, fallback_text: &str, query: &str) {
        if !self.selected_set.is_empty() {
            let chosen: Vec<usize> = self.selected_set.iter().copied().collect();
            for idx in chosen {
                if let Some(entry) = self.entries.get(idx) {
                    let line =
                        format_output_line(&self.cfg.format, &entry.text, idx as i32, Some(query));
                    self.emit_line(line);
                }
            }
        } else {
            let line = format_output_line(
                &self.cfg.format,
                fallback_text,
                fallback_idx as i32,
                Some(query),
            );
            self.emit_line(line);
        }
    }
}

impl RichMode for DmenuMode {
    fn name(&self) -> &str {
        "dmenu"
    }

    fn init(&mut self) -> Result<(), String> {
        let raw_bytes = if let Some(buf) = &self.raw_override {
            buf.clone()
        } else if let Some(input_path) = &self.cfg.input {
            std::fs::read(input_path).map_err(|e| format!("failed to read `{input_path}`: {e}"))?
        } else {
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| format!("failed reading stdin: {e}"))?;
            buf
        };

        let (headers, parsed) = parse_dmenu_script_stream(&raw_bytes, self.cfg.sep);
        if let Some(p) = headers.prompt {
            self.prompt_override = Some(p);
        }
        if let Some(m) = headers.message {
            self.message = Some(m);
        }
        if let Some(nc) = headers.no_custom {
            self.no_custom_override = Some(nc);
        }
        self.theme_snippets = headers.theme_snippets;

        let mut entries: Vec<ListEntry> = parsed.into_iter().map(ListEntry::from).collect();
        for (lo, hi) in headers.urgent_ranges {
            for idx in lo..=hi {
                if let Some(e) = entries.get_mut(idx) {
                    e.urgent = true;
                }
            }
        }
        for (lo, hi) in headers.active_ranges {
            for idx in lo..=hi {
                if let Some(e) = entries.get_mut(idx) {
                    e.active = true;
                }
            }
        }
        if let Some(spec) = &self.cfg.urgent_ranges {
            for (lo, hi) in parse_ranges(spec) {
                for idx in lo..=hi {
                    if let Some(e) = entries.get_mut(idx) {
                        e.urgent = true;
                    }
                }
            }
        }
        if let Some(spec) = &self.cfg.active_ranges {
            for (lo, hi) in parse_ranges(spec) {
                for idx in lo..=hi {
                    if let Some(e) = entries.get_mut(idx) {
                        e.active = true;
                    }
                }
            }
        }

        self.base_active = entries.iter().map(|e| e.active).collect();
        self.entries = entries;
        self.selected_set.clear();
        self.overlay = None;
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.entries.clone()
    }

    fn message(&self) -> Option<String> {
        self.message.clone()
    }

    fn prompt_override(&self) -> Option<String> {
        self.prompt_override.clone()
    }

    fn theme_snippets(&self) -> Vec<String> {
        self.theme_snippets.clone()
    }

    fn no_custom_override(&self) -> Option<bool> {
        self.no_custom_override
    }

    fn overlay(&self) -> Option<String> {
        self.overlay.clone()
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, query: &str) -> Result<ModeStep, String> {
        let custom_blocked =
            self.cfg.only_match || self.no_custom_override.unwrap_or(self.cfg.no_custom);

        match outcome {
            UiOutcome::AcceptedAlt { index, text } => {
                if self.cfg.multi_select {
                    if !self.selected_set.remove(index) {
                        self.selected_set.insert(*index);
                    }
                    self.refresh_multi_select_visuals();
                    let next = (*index + 1).min(self.entries.len().saturating_sub(1));
                    self.reload_hints = ReloadHints {
                        keep_selection: true,
                        new_selection: Some(next),
                        keep_filter: true,
                    };
                    Ok(ModeStep::Reload)
                } else {
                    self.print_accepted(*index, text, query);
                    Ok(ModeStep::Exit(0))
                }
            }
            UiOutcome::Accepted {
                index,
                text,
                exit_code,
            } => {
                self.print_accepted(*index, text, query);
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::CustomInput { text, exit_code } => {
                if !self.selected_set.is_empty() {
                    self.print_accepted(0, text, query);
                    return Ok(ModeStep::Exit(*exit_code));
                }
                if custom_blocked {
                    self.reload_hints = ReloadHints {
                        keep_selection: true,
                        new_selection: None,
                        keep_filter: true,
                    };
                    return Ok(ModeStep::Reload);
                }
                let line = format_output_line(&self.cfg.format, text, -1, Some(query));
                self.emit_line(line);
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::CustomInputAlt { text } => {
                if custom_blocked {
                    self.reload_hints = ReloadHints {
                        keep_selection: true,
                        new_selection: None,
                        keep_filter: true,
                    };
                    return Ok(ModeStep::Reload);
                }
                let line = format_output_line(&self.cfg.format, text, -1, Some(query));
                self.emit_line(line);
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { .. } => {
                self.reload_hints = ReloadHints {
                    keep_selection: true,
                    new_selection: None,
                    keep_filter: true,
                };
                Ok(ModeStep::Reload)
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
        }
    }
}
