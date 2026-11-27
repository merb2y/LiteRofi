//! Unified Omarchy-style System Menu (`-show menu`, `Ctrl+Alt+Space`):
//! Hierarchical, keyboard-first command hub that inherits the user's active Rofi theme.
//! Pressing `Esc` inside any submenu returns cleanly to the root menu; pressing `Esc`
//! at the root closes the menu.

use crate::control::{open_ms_settings, query_system_summary, send_volume_key};
use crate::{resolve_windows_terminal, ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuPage {
    Root,
    Style,
    Tools,
    System,
    Packages,
    Learn,
    Power,
}

impl MenuPage {
    fn prompt(self) -> &'static str {
        match self {
            Self::Root => "󰀻  LiteRofi Menu",
            Self::Style => "󰉼  Style & Appearance",
            Self::Tools => "󰀻  Apps & Tools",
            Self::System => "󰒓  Setup & System",
            Self::Packages => "󰚰  Install & Update",
            Self::Learn => "󰌌  Learn & Config",
            Self::Power => "⏻  Power",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum MenuAction {
    Navigate(MenuPage),
    SwitchMode(&'static str),
    SpawnLiteRofi(&'static [&'static str]),
    OpenStudio,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MsSettings(&'static str),
    TaskManager,
    TerminalCmd(&'static str),
    OpenConfigFile(&'static str),
    OpenAppDataSubdir(&'static str),
    PowerLock,
    PowerSleep,
    PowerSignOut,
    PowerReboot,
    PowerShutdown,
}

struct MenuItem {
    label: &'static str,
    meta: &'static str,
    action: MenuAction,
}

const ROOT_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "󰀻  Apps & Tools        ›",
        meta: "apps tools games music clipboard calc emoji search translate snippets control",
        action: MenuAction::Navigate(MenuPage::Tools),
    },
    MenuItem {
        label: "󰉼  Style               ›",
        meta: "style theme colors palette wallpaper effects studio appearance rasi",
        action: MenuAction::Navigate(MenuPage::Style),
    },
    MenuItem {
        label: "󰒓  Setup & System      ›",
        meta:
            "setup system audio volume sound night light bluetooth wifi battery taskmgr screenshot",
        action: MenuAction::Navigate(MenuPage::System),
    },
    MenuItem {
        label: "󰚰  Install & Update    ›",
        meta: "install remove update upgrade winget packages apps windows update",
        action: MenuAction::Navigate(MenuPage::Packages),
    },
    MenuItem {
        label: "󰌌  Learn & Config      ›",
        meta: "learn help shortcuts keybindings config rasi snippets about info",
        action: MenuAction::Navigate(MenuPage::Learn),
    },
    MenuItem {
        label: "⏻  Power               ›",
        meta: "power lock sleep suspend signout logout restart reboot shutdown",
        action: MenuAction::Navigate(MenuPage::Power),
    },
];

const STYLE_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "←  Back to Menu",
        meta: "back return root menu",
        action: MenuAction::Navigate(MenuPage::Root),
    },
    MenuItem {
        label: "󰏘  Theme Picker",
        meta: "theme switcher rasi appearance",
        action: MenuAction::SwitchMode("theme"),
    },
    MenuItem {
        label: "󰌁  Color Palettes",
        meta: "colors palette scheme base16 catppuccin gruvbox",
        action: MenuAction::SwitchMode("colors"),
    },
    MenuItem {
        label: "󰸉  Wallpaper Carousel       (Alt+W)",
        meta: "wallpaper background gallery wallhaven 4k",
        action: MenuAction::SpawnLiteRofi(&["-show", "wallpaper", "-theme", "wallpaper-picker"]),
    },
    MenuItem {
        label: "󰃟  Wallpaper Effects        (Alt+Shift+W)",
        meta: "wallpaper effects blur frosted sepia vignette dim",
        action: MenuAction::SwitchMode("wallpaper-effects"),
    },
    MenuItem {
        label: "󰙵  LiteRofi Theme Studio",
        meta: "studio settings gui editor preview",
        action: MenuAction::OpenStudio,
    },
    MenuItem {
        label: "󰉋  Open Themes Folder (.rasi)",
        meta: "themes folder directory explorer rasi",
        action: MenuAction::OpenAppDataSubdir("themes"),
    },
];

const TOOLS_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "←  Back to Menu",
        meta: "back return root menu",
        action: MenuAction::Navigate(MenuPage::Root),
    },
    MenuItem {
        label: "󰍉  App Launcher             (Alt+Space)",
        meta: "drun apps launcher programs",
        action: MenuAction::SwitchMode("drun"),
    },
    MenuItem {
        label: "󰮯  Game Launcher            (Alt+G)",
        meta: "games steam epic riot xbox",
        action: MenuAction::SpawnLiteRofi(&["-show", "games", "-theme", "game-launcher"]),
    },
    MenuItem {
        label: "󰎆  Music Player             (Alt+M)",
        meta: "music media spotify smtc audio player",
        action: MenuAction::SpawnLiteRofi(&["-show", "music", "-theme", "music-player"]),
    },
    MenuItem {
        label: "󰅌  Clipboard Manager        (Alt+V)",
        meta: "clipboard history copy paste images",
        action: MenuAction::SpawnLiteRofi(&["-show", "clipboard", "-theme", "clipboard"]),
    },
    MenuItem {
        label: "󰃬  Calculator & Units       (Alt+C)",
        meta: "calc math converter hex binary units",
        action: MenuAction::SwitchMode("calc"),
    },
    MenuItem {
        label: "󰞅  Emoji & Nerd Icons       (Alt+E)",
        meta: "emoji kaomoji symbols icons nerd font",
        action: MenuAction::SwitchMode("emoji"),
    },
    MenuItem {
        label: "󰖟  Web Search & Bangs       (Alt+S)",
        meta: "search web google youtube github bangs",
        action: MenuAction::SwitchMode("search"),
    },
    MenuItem {
        label: "󰗊  Quick Translator         (Alt+T)",
        meta: "translate english turkish language",
        action: MenuAction::SwitchMode("translate"),
    },
    MenuItem {
        label: "󰅴  Snippets Manager         (Alt+N)",
        meta: "snippets templates text code",
        action: MenuAction::SwitchMode("snippets"),
    },
    MenuItem {
        label: "󰒓  Control Center           (Alt+D)",
        meta: "control center quick toggles",
        action: MenuAction::SpawnLiteRofi(&["-show", "control", "-theme", "control-center"]),
    },
];

const SYSTEM_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "←  Back to Menu",
        meta: "back return root menu",
        action: MenuAction::Navigate(MenuPage::Root),
    },
    MenuItem {
        label: "󰕾  Volume Up  (+10%)",
        meta: "volume up louder audio sound speaker",
        action: MenuAction::VolumeUp,
    },
    MenuItem {
        label: "󰖀  Volume Down  (-10%)",
        meta: "volume down quieter audio sound speaker",
        action: MenuAction::VolumeDown,
    },
    MenuItem {
        label: "󰖁  Mute / Unmute Audio",
        meta: "mute silence audio sound toggle",
        action: MenuAction::VolumeMute,
    },
    MenuItem {
        label: "󰓃  Audio Output & Sound Mixer",
        meta: "audio output headphones speakers sound mixer",
        action: MenuAction::MsSettings("ms-settings:sound"),
    },
    MenuItem {
        label: "󰖔  Night Light & Display",
        meta: "night light display brightness monitor screen",
        action: MenuAction::MsSettings("ms-settings:nightlight"),
    },
    MenuItem {
        label: "󰂯  Bluetooth Devices",
        meta: "bluetooth pair headphones gamepad wireless",
        action: MenuAction::MsSettings("ms-settings:bluetooth"),
    },
    MenuItem {
        label: "󰖩  Wi-Fi & Network",
        meta: "wifi network ethernet vpn internet",
        action: MenuAction::MsSettings("ms-settings:network-wifi"),
    },
    MenuItem {
        label: "󰁹  Power & Battery Settings",
        meta: "battery power sleep performance",
        action: MenuAction::MsSettings("ms-settings:powersleep"),
    },
    MenuItem {
        label: "󰄀  Screenshot (Snipping Tool)",
        meta: "screenshot snip capture screen",
        action: MenuAction::MsSettings("ms-screenclip:"),
    },
    MenuItem {
        label: "󰍛  Task Manager",
        meta: "task manager cpu ram gpu processes",
        action: MenuAction::TaskManager,
    },
];

const PACKAGES_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "←  Back to Menu",
        meta: "back return root menu",
        action: MenuAction::Navigate(MenuPage::Root),
    },
    MenuItem {
        label: "󰚰  Upgrade All Apps (winget upgrade --all)",
        meta: "update upgrade all winget packages",
        action: MenuAction::TerminalCmd(
            "winget upgrade --all --include-unknown; Write-Host ''; Read-Host 'Press Enter to close'",
        ),
    },
    MenuItem {
        label: "󰏗  Check Available Updates (winget upgrade)",
        meta: "check list updates winget",
        action: MenuAction::TerminalCmd(
            "winget upgrade; Write-Host ''; Read-Host 'Press Enter to close'",
        ),
    },
    MenuItem {
        label: "󰍉  Search & Install Package (winget)",
        meta: "install search package app winget",
        action: MenuAction::TerminalCmd(
            "$q = Read-Host 'Package to search'; if ($q) { winget search $q; $id = Read-Host 'Exact ID to install (empty to cancel)'; if ($id) { winget install --id $id -e } }; Read-Host 'Press Enter to close'",
        ),
    },
    MenuItem {
        label: "󰆴  Installed Apps & Uninstall",
        meta: "remove uninstall apps features",
        action: MenuAction::MsSettings("ms-settings:appsfeatures"),
    },
    MenuItem {
        label: "󰏓  Windows Update",
        meta: "windows update system patch",
        action: MenuAction::MsSettings("ms-settings:windowsupdate"),
    },
];

const LEARN_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "←  Back to Menu",
        meta: "back return root menu",
        action: MenuAction::Navigate(MenuPage::Root),
    },
    MenuItem {
        label: "󰌌  Shortcuts Guide",
        meta: "shortcuts hotkeys keybindings cheatsheet",
        action: MenuAction::SwitchMode("shortcuts"),
    },
    MenuItem {
        label: "󰒓  Edit config.rasi",
        meta: "config rasi edit settings",
        action: MenuAction::OpenConfigFile("config.rasi"),
    },
    MenuItem {
        label: "󰅴  Edit snippets.toml",
        meta: "snippets toml edit templates",
        action: MenuAction::OpenConfigFile("snippets.toml"),
    },
    MenuItem {
        label: "󰉋  Open LiteRofi Config Folder",
        meta: "config folder appdata explorer",
        action: MenuAction::OpenAppDataSubdir(""),
    },
    MenuItem {
        label: "󰋼  System & Hardware Info",
        meta: "about system specs hardware version",
        action: MenuAction::MsSettings("ms-settings:about"),
    },
];

const POWER_ITEMS: &[MenuItem] = &[
    MenuItem {
        label: "←  Back to Menu",
        meta: "back return root menu",
        action: MenuAction::Navigate(MenuPage::Root),
    },
    MenuItem {
        label: "⏻  Visual Power Menu        (Alt+P)",
        meta: "power menu visual cards",
        action: MenuAction::SpawnLiteRofi(&["-show", "power", "-theme", "powermenu"]),
    },
    MenuItem {
        label: "󰌾  Lock Screen",
        meta: "lock screen session",
        action: MenuAction::PowerLock,
    },
    MenuItem {
        label: "󰤄  Sleep",
        meta: "sleep suspend standby",
        action: MenuAction::PowerSleep,
    },
    MenuItem {
        label: "󰍃  Sign Out",
        meta: "sign out logout session",
        action: MenuAction::PowerSignOut,
    },
    MenuItem {
        label: "󰜉  Restart",
        meta: "restart reboot",
        action: MenuAction::PowerReboot,
    },
    MenuItem {
        label: "󰐥  Shutdown",
        meta: "shutdown power off",
        action: MenuAction::PowerShutdown,
    },
];

/// Interactive hierarchical Omarchy-style System Menu (`Ctrl+Alt+Space`).
pub struct MenuMode {
    display: String,
    terminal: String,
    dry_run: bool,
    page: MenuPage,
    keep_selection_once: bool,
    last_feedback: Option<String>,
}

impl MenuMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "menu".to_string(),
            terminal: cfg.terminal.clone(),
            dry_run: cfg.modes_self_test,
            page: MenuPage::Root,
            keep_selection_once: false,
            last_feedback: None,
        }
    }

    fn current_items(&self) -> &'static [MenuItem] {
        match self.page {
            MenuPage::Root => ROOT_ITEMS,
            MenuPage::Style => STYLE_ITEMS,
            MenuPage::Tools => TOOLS_ITEMS,
            MenuPage::System => SYSTEM_ITEMS,
            MenuPage::Packages => PACKAGES_ITEMS,
            MenuPage::Learn => LEARN_ITEMS,
            MenuPage::Power => POWER_ITEMS,
        }
    }

    fn spawn_literofi(&self, args: &[&str]) {
        if self.dry_run {
            return;
        }
        if let Ok(exe) = std::env::current_exe() {
            let mut cmd = Command::new(exe);
            cmd.args(args);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let _ = cmd.spawn();
        }
    }

    fn spawn_terminal_ps(&self, ps_script: &str) {
        if self.dry_run {
            return;
        }
        let (term_bin, mut term_args) = resolve_windows_terminal(&self.terminal);
        if term_bin.eq_ignore_ascii_case("wt.exe") {
            term_args.push("powershell.exe".to_string());
            term_args.push("-NoLogo".to_string());
            term_args.push("-Command".to_string());
            term_args.push(ps_script.to_string());
            let _ = Command::new(&term_bin).args(&term_args).spawn();
        } else {
            let _ = Command::new("powershell.exe")
                .args(["-NoLogo", "-NoExit", "-Command", ps_script])
                .spawn();
        }
    }
}

impl RichMode for MenuMode {
    fn name(&self) -> &str {
        "menu"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.page = MenuPage::Root;
        self.keep_selection_once = false;
        self.last_feedback = None;
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some(self.page.prompt().to_string())
    }

    fn message(&self) -> Option<String> {
        let stats = query_system_summary();
        match (self.page, &self.last_feedback) {
            (_, Some(fb)) => Some(format!("{fb}   ·   {stats}")),
            (MenuPage::Root, None) => Some(format!("{stats}   ·   Enter: Open   ·   Esc: Close")),
            (_, None) => Some(format!("{stats}   ·   Esc / Backspace: Back")),
        }
    }

    fn back_on_empty_backspace(&self) -> bool {
        self.page != MenuPage::Root
    }

    fn reload_hints(&self) -> ReloadHints {
        if self.keep_selection_once {
            ReloadHints {
                keep_selection: true,
                new_selection: None,
                keep_filter: false,
            }
        } else {
            ReloadHints {
                keep_selection: false,
                new_selection: Some(0),
                keep_filter: false,
            }
        }
    }

    fn theme_snippets(&self) -> Vec<String> {
        vec![
            "element-icon { enabled: false; }\nmessage { enabled: true; padding: 6px 10px; background-color: transparent; }\ntextbox { background-color: transparent; }".to_string(),
        ]
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.current_items()
            .iter()
            .map(|it| ListEntry {
                text: it.label.to_string(),
                meta: Some(it.meta.to_string()),
                ..Default::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        self.keep_selection_once = false;
        match outcome {
            UiOutcome::Cancelled => {
                if self.page != MenuPage::Root {
                    self.page = MenuPage::Root;
                    self.last_feedback = None;
                    Ok(ModeStep::Reload)
                } else {
                    Ok(ModeStep::Exit(1))
                }
            }
            UiOutcome::Back => {
                if self.page != MenuPage::Root {
                    self.page = MenuPage::Root;
                    self.last_feedback = None;
                }
                Ok(ModeStep::Reload)
            }
            UiOutcome::Accepted { index, .. } => {
                let Some(item) = self.current_items().get(*index) else {
                    return Ok(ModeStep::Exit(0));
                };
                match item.action {
                    MenuAction::Navigate(target) => {
                        self.page = target;
                        self.last_feedback = None;
                        Ok(ModeStep::Reload)
                    }
                    MenuAction::SwitchMode(mode_name) => {
                        Ok(ModeStep::SwitchMode(mode_name.to_string()))
                    }
                    MenuAction::SpawnLiteRofi(args) => {
                        self.spawn_literofi(args);
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::OpenStudio => {
                        self.spawn_literofi(&["--open-settings"]);
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::VolumeUp => {
                        if !self.dry_run {
                            send_volume_key(0xAF, 5);
                        }
                        self.last_feedback = Some("Volume +10%".to_string());
                        self.keep_selection_once = true;
                        Ok(ModeStep::Reload)
                    }
                    MenuAction::VolumeDown => {
                        if !self.dry_run {
                            send_volume_key(0xAE, 5);
                        }
                        self.last_feedback = Some("Volume -10%".to_string());
                        self.keep_selection_once = true;
                        Ok(ModeStep::Reload)
                    }
                    MenuAction::VolumeMute => {
                        if !self.dry_run {
                            send_volume_key(0xAD, 1);
                        }
                        self.last_feedback = Some("Toggled Mute".to_string());
                        self.keep_selection_once = true;
                        Ok(ModeStep::Reload)
                    }
                    MenuAction::MsSettings(uri) => {
                        if !self.dry_run {
                            open_ms_settings(uri);
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::TaskManager => {
                        if !self.dry_run {
                            let _ = Command::new("taskmgr.exe").spawn();
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::TerminalCmd(ps) => {
                        self.spawn_terminal_ps(ps);
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::OpenConfigFile(rel) => {
                        if !self.dry_run {
                            if let Some(p) = appdata_literofi_path(rel) {
                                open_shell_path(&p);
                            }
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::OpenAppDataSubdir(sub) => {
                        if !self.dry_run {
                            if let Some(p) = appdata_literofi_path(sub) {
                                let _ = Command::new("explorer.exe").arg(&p).spawn();
                            }
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::PowerLock => {
                        if !self.dry_run {
                            let _ = Command::new("rundll32.exe")
                                .args(["user32.dll,LockWorkStation"])
                                .spawn();
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::PowerSleep => {
                        if !self.dry_run {
                            let _ = Command::new("rundll32.exe")
                                .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                                .spawn();
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::PowerSignOut => {
                        if !self.dry_run {
                            let _ = Command::new("shutdown.exe").arg("/l").spawn();
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::PowerReboot => {
                        if !self.dry_run {
                            let _ = Command::new("shutdown.exe").args(["/r", "/t", "0"]).spawn();
                        }
                        Ok(ModeStep::Exit(0))
                    }
                    MenuAction::PowerShutdown => {
                        if !self.dry_run {
                            let _ = Command::new("shutdown.exe").args(["/s", "/t", "0"]).spawn();
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

fn appdata_literofi_path(sub: &str) -> Option<PathBuf> {
    let base = PathBuf::from(std::env::var("APPDATA").ok()?).join("LiteRofi");
    if sub.is_empty() {
        Some(base)
    } else {
        Some(base.join(sub))
    }
}

fn open_shell_path(path: &PathBuf) {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/C", "start", "", &path.to_string_lossy()]);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_navigates_and_returns_to_root_on_escape() {
        let mut cfg = Config::default();
        cfg.modes_self_test = true;
        let mut mode = MenuMode::new(&cfg);
        mode.init().unwrap();
        assert_eq!(mode.page, MenuPage::Root);
        assert_eq!(mode.entries().len(), ROOT_ITEMS.len());

        // Enter Style (index 1)
        let step = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 1,
                    text: String::new(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        assert_eq!(step, ModeStep::Reload);
        assert_eq!(mode.page, MenuPage::Style);

        // Backspace on empty query in submenu returns to Root
        assert!(mode.back_on_empty_backspace());
        let back = mode.handle_outcome(&UiOutcome::Back, "").unwrap();
        assert_eq!(back, ModeStep::Reload);
        assert_eq!(mode.page, MenuPage::Root);
        assert!(!mode.back_on_empty_backspace());

        // Enter Style again and test Esc
        let _ = mode
            .handle_outcome(
                &UiOutcome::Accepted {
                    index: 1,
                    text: String::new(),
                    exit_code: 0,
                },
                "",
            )
            .unwrap();
        let esc_back = mode.handle_outcome(&UiOutcome::Cancelled, "").unwrap();
        assert_eq!(esc_back, ModeStep::Reload);
        assert_eq!(mode.page, MenuPage::Root);

        // Esc at Root exits
        let exit = mode.handle_outcome(&UiOutcome::Cancelled, "").unwrap();
        assert_eq!(exit, ModeStep::Exit(1));
    }
}
