//! Built-in `-show power` Windows power menu mode inspired by `adi1090x/rofi` powermenu.
//! Displays system uptime + 5 horizontal icon actions (Lock, Suspend, Logout, Reboot, Shutdown)
//! with a built-in two-step confirmation screen before destructive actions.

use crate::active_palette::ActivePalette;
use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Lock,
    Suspend,
    Logout,
    Reboot,
    Shutdown,
}

impl PowerAction {
    fn label(self) -> &'static str {
        match self {
            Self::Lock => "Lock",
            Self::Suspend => "Suspend",
            Self::Logout => "Logout",
            Self::Reboot => "Reboot",
            Self::Shutdown => "Shutdown",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Lock => "\u{f023}",     // fa-lock
            Self::Suspend => "\u{f186}",  // fa-moon
            Self::Logout => "\u{f08b}",   // fa-sign-out
            Self::Reboot => "\u{f021}",   // fa-refresh
            Self::Shutdown => "\u{f011}", // fa-power-off
        }
    }
}

const ALL_ACTIONS: [PowerAction; 5] = [
    PowerAction::Lock,
    PowerAction::Suspend,
    PowerAction::Logout,
    PowerAction::Reboot,
    PowerAction::Shutdown,
];

/// Interactive Windows Power Menu mode (`-show power` / `-show powermenu`).
pub struct PowerMode {
    display: String,
    confirming: Option<PowerAction>,
    palette: Option<ActivePalette>,
    dry_run: bool,
    executed: Option<PowerAction>,
}

impl PowerMode {
    pub fn new(_cfg: &Config) -> Self {
        Self {
            display: "power".to_string(),
            confirming: None,
            palette: None,
            dry_run: false,
            executed: None,
        }
    }

    #[cfg(test)]
    pub fn with_dry_run() -> Self {
        Self {
            display: "power".to_string(),
            confirming: None,
            palette: None,
            dry_run: true,
            executed: None,
        }
    }
}

impl RichMode for PowerMode {
    fn name(&self) -> &str {
        "power"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.confirming = None;
        if !self.dry_run {
            self.palette = ActivePalette::load_from_user_config();
        }
        Ok(())
    }

    fn theme_snippets(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .palette
            .as_ref()
            .map(|p| vec![p.to_powermenu_rasi()])
            .unwrap_or_default();
        let wcfg = literofi_core::widgets_config::WidgetsConfig::load_default();
        if let Some(ov) =
            literofi_core::widgets_config::WidgetsConfig::rasi_power_override_snippet(&wcfg.power)
        {
            out.push(ov);
        }
        out
    }

    fn prompt_override(&self) -> Option<String> {
        if let Some(act) = self.confirming {
            return Some(format!("Confirm {}?", act.label()));
        }
        Some(format!("Uptime: {}", format_system_uptime()))
    }

    fn message(&self) -> Option<String> {
        self.confirming
            .map(|act| format!("Are you sure you want to {}?", act.label()))
    }

    fn no_custom_override(&self) -> Option<bool> {
        Some(true)
    }

    fn reload_hints(&self) -> ReloadHints {
        ReloadHints {
            keep_selection: false,
            new_selection: Some(if self.confirming.is_some() { 1 } else { 0 }),
            keep_filter: false,
        }
    }

    fn entries(&self) -> Vec<ListEntry> {
        if self.confirming.is_some() {
            return [
                ("\u{f00c}  Yes", "confirm-yes"),
                ("\u{f00d}  No", "confirm-no"),
            ]
            .into_iter()
            .map(|(text, meta)| ListEntry {
                text: text.to_string(),
                meta: Some(meta.to_string()),
                ..Default::default()
            })
            .collect();
        }
        ALL_ACTIONS
            .into_iter()
            .map(|act| ListEntry {
                text: act.glyph().to_string(),
                meta: Some(act.label().to_string()),
                ..Default::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Cancelled => {
                if self.confirming.is_some() {
                    self.confirming = None;
                    Ok(ModeStep::Reload)
                } else {
                    Ok(ModeStep::Exit(1))
                }
            }
            UiOutcome::Accepted { index, .. } => {
                if let Some(pending) = self.confirming {
                    if *index == 0 {
                        self.executed = Some(pending);
                        if !self.dry_run {
                            execute_power_action(pending)?;
                        }
                        self.confirming = None;
                        Ok(ModeStep::Exit(0))
                    } else {
                        self.confirming = None;
                        Ok(ModeStep::Reload)
                    }
                } else if let Some(&act) = ALL_ACTIONS.get(*index) {
                    if act == PowerAction::Lock {
                        self.executed = Some(act);
                        if !self.dry_run {
                            execute_power_action(act)?;
                        }
                        Ok(ModeStep::Exit(0))
                    } else {
                        self.confirming = Some(act);
                        Ok(ModeStep::Reload)
                    }
                } else {
                    Ok(ModeStep::Reload)
                }
            }
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => Ok(ModeStep::Reload),
        }
    }
}

fn format_system_uptime() -> String {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetTickCount64;
        // SAFETY: `GetTickCount64` has no preconditions and returns milliseconds since boot.
        let total_mins = unsafe { GetTickCount64() } / 60_000;
        let days = total_mins / (60 * 24);
        let hours = (total_mins / 60) % 24;
        let mins = total_mins % 60;
        if days > 0 {
            format!("{days}d {hours}h {mins}m")
        } else if hours > 0 {
            format!("{hours}h {mins}m")
        } else {
            format!("{mins}m")
        }
    }
    #[cfg(not(windows))]
    {
        "0m".to_string()
    }
}

#[cfg(windows)]
fn execute_power_action(action: PowerAction) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use windows::Win32::System::Shutdown::LockWorkStation;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    match action {
        PowerAction::Lock => {
            // SAFETY: Standard Win32 LockWorkStation call.
            unsafe { LockWorkStation() }.map_err(|e| format!("LockWorkStation failed: {e}"))?;
            Ok(())
        }
        PowerAction::Suspend => {
            Command::new("rundll32.exe")
                .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("suspend failed: {e}"))?;
            Ok(())
        }
        PowerAction::Logout => {
            Command::new("shutdown.exe")
                .args(["/l"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("logout failed: {e}"))?;
            Ok(())
        }
        PowerAction::Reboot => {
            Command::new("shutdown.exe")
                .args(["/r", "/t", "0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("reboot failed: {e}"))?;
            Ok(())
        }
        PowerAction::Shutdown => {
            Command::new("shutdown.exe")
                .args(["/s", "/t", "0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("shutdown failed: {e}"))?;
            Ok(())
        }
    }
}

#[cfg(not(windows))]
fn execute_power_action(_action: PowerAction) -> Result<(), String> {
    Err("power actions require Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_mode_requires_confirmation_for_shutdown_and_executes_on_yes() {
        let mut mode = PowerMode::with_dry_run();
        mode.init().unwrap();
        assert_eq!(mode.entries().len(), 5);

        // Select Shutdown (index 4) -> enters confirmation screen
        let step1 = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 4,
                    text: "\u{f011}".to_string(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        assert_eq!(step1, ModeStep::Reload);
        assert_eq!(mode.prompt_override().as_deref(), Some("Confirm Shutdown?"));
        assert_eq!(mode.entries().len(), 2);

        // Cancel returns to main menu
        let step_back = mode.handle_outcome(&UiOutcome::Cancelled, "").unwrap();
        assert_eq!(step_back, ModeStep::Reload);
        assert_eq!(mode.entries().len(), 5);

        // Select Reboot (index 3) then Yes (index 0)
        let _ = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 3,
                    text: "\u{f021}".to_string(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        let step_yes = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 0,
                    text: "\u{f00c}  Yes".to_string(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        assert_eq!(step_yes, ModeStep::Exit(0));
        assert_eq!(mode.executed, Some(PowerAction::Reboot));
    }
}
