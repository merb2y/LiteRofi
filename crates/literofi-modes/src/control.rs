//! Native Windows Control Center mode (`-show control`, `Alt+D`, inspired by `pocket-dots`):
//! - Live system telemetry bar (RAM usage, uptime, battery / AC status via Win32 APIs).
//! - Instant media/volume controls (Volume Up/Down/Mute via `SendInput`, staying open for rapid taps).
//! - One-click Windows quick panels (Audio Output, Night Light, Bluetooth, Wi-Fi, Power & Battery,
//!   Display, and LiteRofi's searchable Shortcuts Guide).

use crate::active_palette::ActivePalette;
use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::process::Command;

#[derive(Debug, Clone, Copy)]
enum ControlAction {
    VolumeUp,
    VolumeDown,
    VolumeMute,
    AudioSettings,
    NightLight,
    Bluetooth,
    Wifi,
    BatteryPower,
    Shortcuts,
    TaskManager,
}

struct ControlItem {
    label: &'static str,
    meta: &'static str,
    action: ControlAction,
}

const CONTROL_ITEMS: &[ControlItem] = &[
    ControlItem {
        label: "󰕾  Volume Up  (+10%)",
        meta: "audio sound speaker louder volume up",
        action: ControlAction::VolumeUp,
    },
    ControlItem {
        label: "󰖀  Volume Down  (-10%)",
        meta: "audio sound speaker quieter volume down",
        action: ControlAction::VolumeDown,
    },
    ControlItem {
        label: "󰖁  Mute / Unmute Audio",
        meta: "audio sound silence mute toggle",
        action: ControlAction::VolumeMute,
    },
    ControlItem {
        label: "󰓃  Audio Output & Sound Mixer",
        meta: "headphones speakers output device audio mixer",
        action: ControlAction::AudioSettings,
    },
    ControlItem {
        label: "󰖔  Night Light & Brightness",
        meta: "display night light blue light brightness screen",
        action: ControlAction::NightLight,
    },
    ControlItem {
        label: "󰂯  Bluetooth Devices",
        meta: "bluetooth pair headphones gamepad wireless",
        action: ControlAction::Bluetooth,
    },
    ControlItem {
        label: "󰖩  Wi-Fi & Network",
        meta: "wifi network internet ethernet vpn",
        action: ControlAction::Wifi,
    },
    ControlItem {
        label: "󰁹  Power & Battery Settings",
        meta: "battery power sleep performance mode",
        action: ControlAction::BatteryPower,
    },
    ControlItem {
        label: "󰌌  Shortcuts Guide",
        meta: "hotkeys keybindings help shortcuts",
        action: ControlAction::Shortcuts,
    },
    ControlItem {
        label: "󰍛  Task Manager",
        meta: "cpu ram gpu processes monitor taskmgr",
        action: ControlAction::TaskManager,
    },
];

/// Interactive Windows Control Center panel (`Alt+D`).
pub struct ControlMode {
    display: String,
    dry_run: bool,
    palette: Option<ActivePalette>,
    last_feedback: Option<String>,
}

impl ControlMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "control".to_string(),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
            last_feedback: None,
        }
    }
}

impl RichMode for ControlMode {
    fn name(&self) -> &str {
        "control"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        self.last_feedback = None;
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some("󰒓  Control Center".to_string())
    }

    fn message(&self) -> Option<String> {
        let stats = query_system_summary();
        if let Some(fb) = &self.last_feedback {
            Some(format!("{fb}   ·   {stats}"))
        } else {
            Some(stats)
        }
    }

    fn reload_hints(&self) -> ReloadHints {
        ReloadHints {
            keep_selection: true,
            new_selection: None,
            keep_filter: false,
        }
    }

    fn theme_snippets(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .palette
            .as_ref()
            .map(|p| vec![p.to_control_rasi()])
            .unwrap_or_default();
        let wcfg = literofi_core::widgets_config::WidgetsConfig::load_default();
        if let Some(ov) =
            literofi_core::widgets_config::WidgetsConfig::rasi_override_snippet(&wcfg.control)
        {
            out.push(ov);
        }
        out
    }

    fn entries(&self) -> Vec<ListEntry> {
        CONTROL_ITEMS
            .iter()
            .map(|it| ListEntry {
                text: it.label.to_string(),
                meta: Some(it.meta.to_string()),
                ..Default::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::Accepted { index, .. } => {
                let Some(item) = CONTROL_ITEMS.get(*index) else {
                    return Ok(ModeStep::Exit(0));
                };
                match item.action {
                    ControlAction::VolumeUp => {
                        if !self.dry_run {
                            send_volume_key(0xAF, 5); // VK_VOLUME_UP x5 (~10%)
                        }
                        self.last_feedback = Some("Volume +10%".to_string());
                        Ok(ModeStep::Reload)
                    }
                    ControlAction::VolumeDown => {
                        if !self.dry_run {
                            send_volume_key(0xAE, 5); // VK_VOLUME_DOWN x5 (~10%)
                        }
                        self.last_feedback = Some("Volume -10%".to_string());
                        Ok(ModeStep::Reload)
                    }
                    ControlAction::VolumeMute => {
                        if !self.dry_run {
                            send_volume_key(0xAD, 1); // VK_VOLUME_MUTE
                        }
                        self.last_feedback = Some("Toggled Mute".to_string());
                        Ok(ModeStep::Reload)
                    }
                    ControlAction::AudioSettings => {
                        if !self.dry_run {
                            open_ms_settings("ms-settings:sound");
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    ControlAction::NightLight => {
                        if !self.dry_run {
                            open_ms_settings("ms-settings:nightlight");
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    ControlAction::Bluetooth => {
                        if !self.dry_run {
                            open_ms_settings("ms-settings:bluetooth");
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    ControlAction::Wifi => {
                        if !self.dry_run {
                            open_ms_settings("ms-settings:network-wifi");
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    ControlAction::BatteryPower => {
                        if !self.dry_run {
                            open_ms_settings("ms-settings:powersleep");
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    ControlAction::Shortcuts => Ok(ModeStep::SwitchMode("shortcuts".to_string())),
                    ControlAction::TaskManager => {
                        if !self.dry_run {
                            let _ = Command::new("taskmgr.exe").spawn();
                        }
                        Ok(ModeStep::Exit(0))
                    }
                }
            }
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => Ok(ModeStep::Reload),
        }
    }
}

#[cfg(windows)]
pub(crate) fn send_volume_key(vk: u16, count: usize) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    for _ in 0..count {
        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            },
        ];
        // SAFETY: Valid array of 2 keyboard INPUT structs passed to SendInput.
        unsafe {
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn send_volume_key(_vk: u16, _count: usize) {}

pub(crate) fn open_ms_settings(uri: &str) {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/C", "start", "", uri]);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let _ = cmd.spawn();
}

#[cfg(windows)]
pub fn query_system_summary() -> String {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows::Win32::System::SystemInformation::{
        GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
    };

    let mut mem = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let ram_str = if unsafe { GlobalMemoryStatusEx(&mut mem) }.is_ok() {
        let total_gb = mem.ullTotalPhys as f64 / (1024.0 * 1024.0 * 1024.0);
        let used_gb =
            (mem.ullTotalPhys.saturating_sub(mem.ullAvailPhys)) as f64 / (1024.0 * 1024.0 * 1024.0);
        format!("RAM: {used_gb:.1}/{total_gb:.0} GB ({}%)", mem.dwMemoryLoad)
    } else {
        "RAM: N/A".to_string()
    };

    let up_secs = unsafe { GetTickCount64() } / 1000;
    let up_h = up_secs / 3600;
    let up_m = (up_secs % 3600) / 60;
    let up_str = format!("Uptime: {up_h}h {up_m:02}m");

    let mut pwr = SYSTEM_POWER_STATUS::default();
    let bat_str = if unsafe { GetSystemPowerStatus(&mut pwr) }.is_ok() {
        if pwr.BatteryFlag == 128 || pwr.BatteryLifePercent > 100 {
            "AC Power".to_string()
        } else if pwr.ACLineStatus == 1 {
            format!("Charging {}%", pwr.BatteryLifePercent)
        } else {
            format!("Battery {}%", pwr.BatteryLifePercent)
        }
    } else {
        "AC Power".to_string()
    };

    format!("{ram_str}   ·   {up_str}   ·   {bat_str}")
}

#[cfg(not(windows))]
pub fn query_system_summary() -> String {
    "System Control Center".to_string()
}
