//! Searchable Shortcuts Guide mode (`-show shortcuts`, inspired by `pocket-dots`):
//! Lists all LiteRofi global hotkeys, in-mode shortcuts, and essential Windows shortcuts
//! with instant fuzzy search; pressing Enter on a LiteRofi mode shortcut launches it directly.

use crate::active_palette::ActivePalette;
use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::process::Command;

struct ShortcutRow {
    chord: &'static str,
    desc: &'static str,
    category: &'static str,
    launch_args: Option<&'static [&'static str]>,
}

const SHORTCUTS: &[ShortcutRow] = &[
    ShortcutRow {
        chord: "Alt + Space",
        desc: "Open / Close LiteRofi App Launcher",
        category: "LiteRofi",
        launch_args: Some(&["-show", "drun"]),
    },
    ShortcutRow {
        chord: "Ctrl + Alt + Space",
        desc: "Unified System Menu (Style, Tools, System, Winget, Power)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "menu"]),
    },
    ShortcutRow {
        chord: "Alt + G",
        desc: "Game Launcher (Steam, Riot, Xbox, Epic/Heroic)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "games", "-theme", "game-launcher"]),
    },
    ShortcutRow {
        chord: "Alt + M",
        desc: "Music Player (SMTC + Cover-Art Auto-Accent)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "music", "-theme", "music-player"]),
    },
    ShortcutRow {
        chord: "Alt + W",
        desc: "Wallpaper Carousel (60fps Slanted 5-Card + Web 4K)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "wallpaper"]),
    },
    ShortcutRow {
        chord: "Alt + V",
        desc: "Clipboard Manager (Text + Live Image Previews)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "clipboard", "-theme", "clipboard"]),
    },
    ShortcutRow {
        chord: "Alt + D",
        desc: "Control Center (Volume, Audio, Night Light, RAM)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "control", "-theme", "control-center"]),
    },
    ShortcutRow {
        chord: "Alt + P",
        desc: "Power Menu (Lock, Sleep, Reboot, Shutdown)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "power", "-theme", "powermenu"]),
    },
    ShortcutRow {
        chord: "Alt + C",
        desc: "Live Calculator & Unit/Base Converter (Copy on Enter)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "calc"]),
    },
    ShortcutRow {
        chord: "Alt + E",
        desc: "Emoji, Kaomoji & Nerd Font Icon Picker",
        category: "LiteRofi",
        launch_args: Some(&["-show", "emoji"]),
    },
    ShortcutRow {
        chord: "Alt + S",
        desc: "Quick Web Search & Bangs (!g, !yt, !gh, !w, !r, !tr, !ai)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "search"]),
    },
    ShortcutRow {
        chord: "Alt + Shift + W",
        desc: "Wallpaper Effects (Blur, Frosted, B&W, Sepia, Vignette, Dim)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "wallpaper-effects"]),
    },
    ShortcutRow {
        chord: "Alt + N",
        desc: "Text & Code Snippets Manager (snippets.toml)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "snippets"]),
    },
    ShortcutRow {
        chord: "Alt + T",
        desc: "Quick Translator (Auto TR↔EN + Language Prefixes)",
        category: "LiteRofi",
        launch_args: Some(&["-show", "translate"]),
    },
    ShortcutRow {
        chord: "Alt + W  →  W / E",
        desc: "W: Local+WE ↔ Wallhaven 4K · E: Wallpaper Engine Discover",
        category: "Wallpaper",
        launch_args: None,
    },
    ShortcutRow {
        chord: "Alt + W  →  T",
        desc: "Apply Wallpaper + Extract Auto-Theme Colors",
        category: "Wallpaper",
        launch_args: None,
    },
    ShortcutRow {
        chord: "Alt + V  →  Shift+Enter",
        desc: "Pin / Unpin Clipboard Entry to Top",
        category: "Clipboard",
        launch_args: None,
    },
    ShortcutRow {
        chord: "Ctrl + Tab",
        desc: "Switch Active Mode Tab inside LiteRofi",
        category: "Navigation",
        launch_args: None,
    },
    ShortcutRow {
        chord: "Win + Shift + S",
        desc: "Windows Snipping Tool Screenshot (auto-saved to Alt+V)",
        category: "Windows",
        launch_args: None,
    },
    ShortcutRow {
        chord: "Win + L",
        desc: "Lock Windows Session",
        category: "Windows",
        launch_args: None,
    },
    ShortcutRow {
        chord: "Ctrl + Shift + Esc",
        desc: "Open Windows Task Manager",
        category: "Windows",
        launch_args: None,
    },
];

/// Interactive searchable keybindings & shortcuts guide.
pub struct ShortcutsMode {
    display: String,
    dry_run: bool,
    palette: Option<ActivePalette>,
}

impl ShortcutsMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "shortcuts".to_string(),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
        }
    }
}

impl RichMode for ShortcutsMode {
    fn name(&self) -> &str {
        "shortcuts"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some("󰌌  Shortcuts".to_string())
    }

    fn message(&self) -> Option<String> {
        Some("Type to filter shortcuts  ·  Enter: Launch LiteRofi mode  ·  Esc: Close".to_string())
    }

    fn reload_hints(&self) -> ReloadHints {
        ReloadHints::default()
    }

    fn theme_snippets(&self) -> Vec<String> {
        vec!["element-icon { enabled: false; }".to_string()]
    }

    fn entries(&self) -> Vec<ListEntry> {
        SHORTCUTS
            .iter()
            .map(|s| ListEntry {
                text: format!("{:<22}  {}", s.chord, s.desc),
                meta: Some(format!("{} {} {}", s.category, s.chord, s.desc)),
                ..Default::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::Accepted { index, .. } => {
                if let Some(row) = SHORTCUTS.get(*index) {
                    if let Some(args) = row.launch_args {
                        if !self.dry_run {
                            if let Ok(exe) = std::env::current_exe() {
                                let _ = Command::new(exe).args(args).spawn();
                            }
                        }
                    }
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => Ok(ModeStep::Exit(0)),
        }
    }
}
