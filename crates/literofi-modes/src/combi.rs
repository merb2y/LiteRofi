//! `combi` mode implementation mirroring `source/modes/combi.c`.
//! Aggregates sub-modes, formats `{mode} {text}` prefixes, supports `!mode` bang dispatch,
//! and routes accepted/custom/delete outcomes back to the owning sub-mode.

use crate::registry::{build_mode, parse_mode_specs, ModeSpec};
use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::cli::string_replace_if_exists;
use literofi_core::config::Config;
use literofi_core::protocol::strip_pango_markup;
use literofi_render::state::{ListEntry, UiOutcome};

struct SubModeSlot {
    mode: Box<dyn RichMode>,
    start: usize,
    len: usize,
}

/// Stateful `combi` switcher combining multiple sub-modes.
pub struct CombiMode {
    cfg: Config,
    display_name: String,
    subs: Vec<SubModeSlot>,
    entries: Vec<ListEntry>,
    reload_hints: ReloadHints,
}

impl CombiMode {
    pub fn new(cfg: &Config) -> Result<Self, String> {
        let mut specs = Vec::new();
        for spec in parse_mode_specs(&cfg.combi_modes) {
            if matches!(spec, ModeSpec::Combi | ModeSpec::Dmenu) {
                continue;
            }
            specs.push(spec);
        }
        let mut modes = Vec::with_capacity(specs.len());
        for spec in &specs {
            modes.push(build_mode(spec, cfg)?);
        }
        Ok(Self::from_sub_modes(cfg, modes))
    }

    pub fn from_sub_modes(cfg: &Config, modes: Vec<Box<dyn RichMode>>) -> Self {
        let subs = modes
            .into_iter()
            .map(|mode| SubModeSlot {
                mode,
                start: 0,
                len: 0,
            })
            .collect();
        Self {
            cfg: cfg.clone(),
            display_name: cfg
                .display_combi
                .clone()
                .unwrap_or_else(|| "combi".to_string()),
            subs,
            entries: Vec::new(),
            reload_hints: ReloadHints::default(),
        }
    }

    fn rebuild_entries(&mut self) {
        let mut merged = Vec::new();
        for slot in &mut self.subs {
            slot.start = merged.len();
            let sub_entries = slot.mode.entries();
            slot.len = sub_entries.len();
            let dname = slot.mode.display_name().to_string();
            let tag = slot.mode.name().to_string();

            for mut entry in sub_entries {
                if !self.cfg.combi_hide_mode_prefix {
                    let base = entry.visible_label().to_string();
                    let raw = string_replace_if_exists(
                        &self.cfg.combi_display_format,
                        &[
                            ("{mode}", Some(dname.as_str())),
                            ("{text}", Some(base.as_str())),
                        ],
                    );
                    let formatted = if self.cfg.markup_rows {
                        raw
                    } else {
                        strip_pango_markup(&raw)
                    };
                    entry.display = Some(formatted);
                }
                entry.mode_tag = Some(tag.clone());
                merged.push(entry);
            }
        }
        self.entries = merged;
    }

    fn locate_global(&self, global_idx: usize) -> Option<(usize, usize)> {
        self.subs
            .iter()
            .enumerate()
            .find(|(_, s)| global_idx >= s.start && global_idx < s.start + s.len)
            .map(|(sub_idx, s)| (sub_idx, global_idx - s.start))
    }

    fn map_outcome(&self, outcome: &UiOutcome, query: &str) -> Option<(usize, UiOutcome, String)> {
        match outcome {
            UiOutcome::Accepted {
                index,
                text,
                exit_code,
            } => {
                let (sub, local) = self.locate_global(*index)?;
                Some((
                    sub,
                    UiOutcome::Accepted {
                        index: local,
                        text: text.clone(),
                        exit_code: *exit_code,
                    },
                    strip_bang_prefix(query, self.subs[sub].mode.as_ref()).to_string(),
                ))
            }
            UiOutcome::AcceptedAlt { index, text } => {
                let (sub, local) = self.locate_global(*index)?;
                Some((
                    sub,
                    UiOutcome::AcceptedAlt {
                        index: local,
                        text: text.clone(),
                    },
                    strip_bang_prefix(query, self.subs[sub].mode.as_ref()).to_string(),
                ))
            }
            UiOutcome::DeleteEntry { index } => {
                let (sub, local) = self.locate_global(*index)?;
                Some((
                    sub,
                    UiOutcome::DeleteEntry { index: local },
                    strip_bang_prefix(query, self.subs[sub].mode.as_ref()).to_string(),
                ))
            }
            UiOutcome::CustomInput { text, exit_code } => {
                let (sub, rest) = self.resolve_bang_target(text);
                Some((
                    sub,
                    UiOutcome::CustomInput {
                        text: rest.to_string(),
                        exit_code: *exit_code,
                    },
                    rest.to_string(),
                ))
            }
            UiOutcome::CustomInputAlt { text } => {
                let (sub, rest) = self.resolve_bang_target(text);
                Some((
                    sub,
                    UiOutcome::CustomInputAlt {
                        text: rest.to_string(),
                    },
                    rest.to_string(),
                ))
            }
            UiOutcome::Cancelled | UiOutcome::ModeSwitch(_) => None,
        }
    }

    fn resolve_bang_target<'a>(&self, input: &'a str) -> (usize, &'a str) {
        if let Some(after) = input.strip_prefix('!') {
            let (prefix, rest) = match after.split_once(' ') {
                Some((p, r)) => (p, r),
                None => (after, ""),
            };
            if !prefix.is_empty() {
                if let Some(pos) = self.subs.iter().position(|s| {
                    s.mode
                        .name()
                        .to_ascii_lowercase()
                        .starts_with(&prefix.to_ascii_lowercase())
                }) {
                    return (pos, rest);
                }
            }
        }
        (0, input)
    }
}

fn strip_bang_prefix<'a>(query: &'a str, sub: &dyn RichMode) -> &'a str {
    if let Some(after) = query.strip_prefix('!') {
        let (prefix, rest) = match after.split_once(' ') {
            Some((p, r)) => (p, r),
            None => (after, ""),
        };
        if !prefix.is_empty()
            && sub
                .name()
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
        {
            return rest;
        }
    }
    query
}

impl RichMode for CombiMode {
    fn name(&self) -> &str {
        "combi"
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn init(&mut self) -> Result<(), String> {
        for slot in &mut self.subs {
            slot.mode.init()?;
        }
        self.rebuild_entries();
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.entries.clone()
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Cancelled => return Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => return Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => {}
        }
        if self.subs.is_empty() {
            return Ok(ModeStep::Exit(1));
        }
        let Some((sub_idx, mapped, eff_query)) = self.map_outcome(outcome, query) else {
            return Ok(ModeStep::Exit(1));
        };
        let step = self.subs[sub_idx]
            .mode
            .handle_outcome(&mapped, &eff_query)?;
        if matches!(step, ModeStep::Reload) {
            self.rebuild_entries();
            self.reload_hints = ReloadHints {
                keep_selection: true,
                new_selection: None,
                keep_filter: true,
            };
        }
        Ok(step)
    }
}
