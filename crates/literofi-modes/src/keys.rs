//! `keys` mode implementation mirroring `source/modes/help-keys.c`.

use crate::{ModeStep, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};

/// Displays all active Rofi/LiteRofi keybindings and supports filtering by action or chord.
pub struct KeysMode {
    display_name: String,
    rows: Vec<String>,
}

impl KeysMode {
    pub fn new(cfg: &Config) -> Self {
        let rows = cfg
            .keybindings
            .entries()
            .iter()
            .map(|e| format!("{} — {}", e.name, e.binding))
            .collect();
        Self {
            display_name: cfg
                .display_keys
                .clone()
                .unwrap_or_else(|| "keys".to_string()),
            rows,
        }
    }
}

impl RichMode for KeysMode {
    fn name(&self) -> &str {
        "keys"
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.rows
            .iter()
            .map(|r| ListEntry {
                text: r.clone(),
                ..ListEntry::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Accepted { exit_code, .. } | UiOutcome::CustomInput { exit_code, .. } => {
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::AcceptedAlt { .. } | UiOutcome::CustomInputAlt { .. } => {
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { .. } => Ok(ModeStep::Reload),
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
        }
    }
}
