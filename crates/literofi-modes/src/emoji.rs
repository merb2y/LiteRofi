//! Emoji, Kaomoji & Nerd Font Icon Picker mode (`-show emoji`, inspired by `rofi-emoji` / `JaKooLit/RofiEmoji`).
//! Fuzzy-searchable with English + Turkish keywords; persists most-recently-used picks to top.

use crate::active_palette::ActivePalette;
use crate::clipboard::write_text_to_clipboard;
use crate::{resolve_cache_dir, ModeStep, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::HashSet;
use std::path::PathBuf;

const EMOJI_HISTORY_FILE: &str = "rofi-emoji.history";
const MAX_EMOJI_HISTORY: usize = 40;

struct GlyphRow {
    glyph: &'static str,
    name: &'static str,
    category: &'static str,
    keywords: &'static str,
}

const GLYPHS: &[GlyphRow] = &[
    // Faces & Reactions
    GlyphRow {
        glyph: "😀",
        name: "Grinning Face",
        category: "Emoji",
        keywords: "smile happy gulumse mutlu",
    },
    GlyphRow {
        glyph: "😂",
        name: "Face with Tears of Joy",
        category: "Emoji",
        keywords: "laugh lol haha gulme komik",
    },
    GlyphRow {
        glyph: "🤣",
        name: "Rolling on the Floor Laughing",
        category: "Emoji",
        keywords: "rofl lmao gulme kahkaha",
    },
    GlyphRow {
        glyph: "😊",
        name: "Smiling Face with Smiling Eyes",
        category: "Emoji",
        keywords: "blush warm mutlu gulumse",
    },
    GlyphRow {
        glyph: "😍",
        name: "Heart Eyes",
        category: "Emoji",
        keywords: "love crush ask kalp",
    },
    GlyphRow {
        glyph: "🥰",
        name: "Smiling Face with Hearts",
        category: "Emoji",
        keywords: "love adore sevgi",
    },
    GlyphRow {
        glyph: "😎",
        name: "Smiling Face with Sunglasses",
        category: "Emoji",
        keywords: "cool shades havali gozluk",
    },
    GlyphRow {
        glyph: "🤔",
        name: "Thinking Face",
        category: "Emoji",
        keywords: "hmm wonder dusun",
    },
    GlyphRow {
        glyph: "🤯",
        name: "Exploding Head",
        category: "Emoji",
        keywords: "mind blown shock sasir",
    },
    GlyphRow {
        glyph: "😭",
        name: "Loudly Crying Face",
        category: "Emoji",
        keywords: "cry sad sob agla uzgun",
    },
    GlyphRow {
        glyph: "😅",
        name: "Grinning Face with Sweat",
        category: "Emoji",
        keywords: "phew awkward ter",
    },
    GlyphRow {
        glyph: "😉",
        name: "Winking Face",
        category: "Emoji",
        keywords: "wink flirt goz kirp",
    },
    GlyphRow {
        glyph: "🙄",
        name: "Face with Rolling Eyes",
        category: "Emoji",
        keywords: "eyeroll annoyed goz devir",
    },
    GlyphRow {
        glyph: "😤",
        name: "Face with Steam From Nose",
        category: "Emoji",
        keywords: "angry huff sinir",
    },
    GlyphRow {
        glyph: "😡",
        name: "Pouting Face",
        category: "Emoji",
        keywords: "mad angry rage kizgin",
    },
    GlyphRow {
        glyph: "🥳",
        name: "Partying Face",
        category: "Emoji",
        keywords: "party celebrate kutlama",
    },
    GlyphRow {
        glyph: "😴",
        name: "Sleeping Face",
        category: "Emoji",
        keywords: "sleep tired zzz uyku",
    },
    GlyphRow {
        glyph: "🫡",
        name: "Saluting Face",
        category: "Emoji",
        keywords: "salute respect selam",
    },
    GlyphRow {
        glyph: "🫠",
        name: "Melting Face",
        category: "Emoji",
        keywords: "melt awkward eri",
    },
    GlyphRow {
        glyph: "💀",
        name: "Skull",
        category: "Emoji",
        keywords: "dead rip lmao kurukafa",
    },
    GlyphRow {
        glyph: "🤡",
        name: "Clown Face",
        category: "Emoji",
        keywords: "clown silly palyaco",
    },
    GlyphRow {
        glyph: "👻",
        name: "Ghost",
        category: "Emoji",
        keywords: "boo spooky hayalet",
    },
    GlyphRow {
        glyph: "🤖",
        name: "Robot",
        category: "Emoji",
        keywords: "bot ai machine yapay zeka",
    },
    // Hands & Gestures
    GlyphRow {
        glyph: "👍",
        name: "Thumbs Up",
        category: "Emoji",
        keywords: "ok yes good approve tamam evet",
    },
    GlyphRow {
        glyph: "👎",
        name: "Thumbs Down",
        category: "Emoji",
        keywords: "no bad dislike hayir",
    },
    GlyphRow {
        glyph: "👏",
        name: "Clapping Hands",
        category: "Emoji",
        keywords: "clap bravo alkis",
    },
    GlyphRow {
        glyph: "🙌",
        name: "Raising Hands",
        category: "Emoji",
        keywords: "hooray yay yasasin",
    },
    GlyphRow {
        glyph: "🙏",
        name: "Folded Hands",
        category: "Emoji",
        keywords: "please thanks pray tesekkur lutfen",
    },
    GlyphRow {
        glyph: "🤝",
        name: "Handshake",
        category: "Emoji",
        keywords: "deal agree anlasma",
    },
    GlyphRow {
        glyph: "✌️",
        name: "Victory Hand",
        category: "Emoji",
        keywords: "peace barıs zafer",
    },
    GlyphRow {
        glyph: "👌",
        name: "OK Hand",
        category: "Emoji",
        keywords: "perfect nice harika",
    },
    GlyphRow {
        glyph: "🤌",
        name: "Pinched Fingers",
        category: "Emoji",
        keywords: "chef kiss italian mukemmel",
    },
    GlyphRow {
        glyph: "💪",
        name: "Flexed Biceps",
        category: "Emoji",
        keywords: "strong power gym guc",
    },
    GlyphRow {
        glyph: "👋",
        name: "Waving Hand",
        category: "Emoji",
        keywords: "hello hi bye merhaba selam",
    },
    GlyphRow {
        glyph: "🫶",
        name: "Heart Hands",
        category: "Emoji",
        keywords: "love appreciate kalp",
    },
    // Hearts & Symbols
    GlyphRow {
        glyph: "❤️",
        name: "Red Heart",
        category: "Emoji",
        keywords: "love heart kalp ask",
    },
    GlyphRow {
        glyph: "🔥",
        name: "Fire",
        category: "Emoji",
        keywords: "lit hot flame ates efsane",
    },
    GlyphRow {
        glyph: "✨",
        name: "Sparkles",
        category: "Emoji",
        keywords: "magic shiny new parilti",
    },
    GlyphRow {
        glyph: "⚡",
        name: "High Voltage",
        category: "Emoji",
        keywords: "fast lightning bolt hizli simsek",
    },
    GlyphRow {
        glyph: "🚀",
        name: "Rocket",
        category: "Emoji",
        keywords: "launch ship fast roket baslat",
    },
    GlyphRow {
        glyph: "💯",
        name: "Hundred Points",
        category: "Emoji",
        keywords: "100 perfect yuz",
    },
    GlyphRow {
        glyph: "🎉",
        name: "Party Popper",
        category: "Emoji",
        keywords: "tada celebrate kutla",
    },
    GlyphRow {
        glyph: "⭐",
        name: "Star",
        category: "Emoji",
        keywords: "favorite yildiz",
    },
    GlyphRow {
        glyph: "🌟",
        name: "Glowing Star",
        category: "Emoji",
        keywords: "shine yildiz",
    },
    GlyphRow {
        glyph: "✅",
        name: "Check Mark Button",
        category: "Emoji",
        keywords: "done yes pass tamam onay",
    },
    GlyphRow {
        glyph: "❌",
        name: "Cross Mark",
        category: "Emoji",
        keywords: "no fail error iptal hata",
    },
    GlyphRow {
        glyph: "⚠️",
        name: "Warning",
        category: "Emoji",
        keywords: "alert caution uyari",
    },
    GlyphRow {
        glyph: "💡",
        name: "Light Bulb",
        category: "Emoji",
        keywords: "idea tip fikir ampul",
    },
    GlyphRow {
        glyph: "📌",
        name: "Pushpin",
        category: "Emoji",
        keywords: "pin sabitle",
    },
    GlyphRow {
        glyph: "🔗",
        name: "Link",
        category: "Emoji",
        keywords: "url baglanti",
    },
    // Tech, Dev & Work
    GlyphRow {
        glyph: "💻",
        name: "Laptop",
        category: "Dev",
        keywords: "code computer pc bilgisayar",
    },
    GlyphRow {
        glyph: "🖥️",
        name: "Desktop Computer",
        category: "Dev",
        keywords: "monitor screen masaustu",
    },
    GlyphRow {
        glyph: "⌨️",
        name: "Keyboard",
        category: "Dev",
        keywords: "type klavye",
    },
    GlyphRow {
        glyph: "🐛",
        name: "Bug",
        category: "Dev",
        keywords: "issue fix hata bocek",
    },
    GlyphRow {
        glyph: "🛠️",
        name: "Hammer and Wrench",
        category: "Dev",
        keywords: "tools build fix arac",
    },
    GlyphRow {
        glyph: "⚙️",
        name: "Gear",
        category: "Dev",
        keywords: "settings config ayar",
    },
    GlyphRow {
        glyph: "📦",
        name: "Package",
        category: "Dev",
        keywords: "crate release paket",
    },
    GlyphRow {
        glyph: "🔒",
        name: "Locked",
        category: "Dev",
        keywords: "security private kilit",
    },
    GlyphRow {
        glyph: "🔓",
        name: "Unlocked",
        category: "Dev",
        keywords: "open acik",
    },
    GlyphRow {
        glyph: "🔑",
        name: "Key",
        category: "Dev",
        keywords: "password token anahtar",
    },
    GlyphRow {
        glyph: "📁",
        name: "File Folder",
        category: "Dev",
        keywords: "dir klasor",
    },
    GlyphRow {
        glyph: "📝",
        name: "Memo",
        category: "Dev",
        keywords: "note docs edit not",
    },
    GlyphRow {
        glyph: "📊",
        name: "Bar Chart",
        category: "Dev",
        keywords: "stats metrics grafik",
    },
    GlyphRow {
        glyph: "🔍",
        name: "Magnifying Glass",
        category: "Dev",
        keywords: "search find ara",
    },
    GlyphRow {
        glyph: "🦀",
        name: "Crab (Rust)",
        category: "Dev",
        keywords: "rust ferris yengec",
    },
    GlyphRow {
        glyph: "🐧",
        name: "Penguin (Linux)",
        category: "Dev",
        keywords: "linux tux penguen",
    },
    GlyphRow {
        glyph: "🎮",
        name: "Video Game",
        category: "Fun",
        keywords: "game controller oyun",
    },
    GlyphRow {
        glyph: "🎧",
        name: "Headphone",
        category: "Fun",
        keywords: "music audio kulaklik muzik",
    },
    GlyphRow {
        glyph: "🎵",
        name: "Musical Note",
        category: "Fun",
        keywords: "song music nota muzik",
    },
    GlyphRow {
        glyph: "☕",
        name: "Hot Beverage",
        category: "Food",
        keywords: "coffee tea kahve cay",
    },
    GlyphRow {
        glyph: "🍕",
        name: "Pizza",
        category: "Food",
        keywords: "food yemek",
    },
    GlyphRow {
        glyph: "🌙",
        name: "Crescent Moon",
        category: "Nature",
        keywords: "night dark mode gece ay",
    },
    GlyphRow {
        glyph: "☀️",
        name: "Sun",
        category: "Nature",
        keywords: "day light gunes",
    },
    GlyphRow {
        glyph: "🌊",
        name: "Water Wave",
        category: "Nature",
        keywords: "ocean sea dalga",
    },
    GlyphRow {
        glyph: "🇹🇷",
        name: "Turkey Flag",
        category: "Flags",
        keywords: "tr turkiye bayrak",
    },
    // Kaomoji
    GlyphRow {
        glyph: "¯\\_(ツ)_/¯",
        name: "Shrug",
        category: "Kaomoji",
        keywords: "dunno whatever bilmem omuz silk",
    },
    GlyphRow {
        glyph: "(╯°□°)╯︵ ┻━┻",
        name: "Table Flip",
        category: "Kaomoji",
        keywords: "rage angry masa devir",
    },
    GlyphRow {
        glyph: "┬─┬ノ( º _ ºノ)",
        name: "Put Table Back",
        category: "Kaomoji",
        keywords: "calm fix masa duzelt",
    },
    GlyphRow {
        glyph: "(⌐■_■)",
        name: "Deal With It",
        category: "Kaomoji",
        keywords: "cool shades havali",
    },
    GlyphRow {
        glyph: "ಠ_ಠ",
        name: "Look of Disapproval",
        category: "Kaomoji",
        keywords: "stare serious bakis",
    },
    GlyphRow {
        glyph: "(づ｡◕‿‿◕｡)づ",
        name: "Cute Hug",
        category: "Kaomoji",
        keywords: "hug love saril",
    },
    GlyphRow {
        glyph: "(☞ﾟヮﾟ)☞",
        name: "Finger Guns",
        category: "Kaomoji",
        keywords: "ayy point isaret",
    },
    GlyphRow {
        glyph: "ʕ•ᴥ•ʔ",
        name: "Bear",
        category: "Kaomoji",
        keywords: "cute ayi",
    },
    GlyphRow {
        glyph: "(ﾉ◕ヮ◕)ﾉ*:･ﾟ✧",
        name: "Sparkle Throw",
        category: "Kaomoji",
        keywords: "magic happy parilti",
    },
    GlyphRow {
        glyph: "(•_•) ( •_•)>⌐■-■ (⌐■_■)",
        name: "CSI Sunglasses",
        category: "Kaomoji",
        keywords: "yeah cool",
    },
    GlyphRow {
        glyph: "(╥﹏╥)",
        name: "Crying Kaomoji",
        category: "Kaomoji",
        keywords: "sad cry agla",
    },
    GlyphRow {
        glyph: "(ง'̀-'́)ง",
        name: "Fight Me",
        category: "Kaomoji",
        keywords: "fight boxing dovus",
    },
    // Popular Nerd Font Icons
    GlyphRow {
        glyph: "󰣇",
        name: "Arch Linux",
        category: "NerdFont",
        keywords: "arch linux distro",
    },
    GlyphRow {
        glyph: "",
        name: "Windows Logo",
        category: "NerdFont",
        keywords: "windows os",
    },
    GlyphRow {
        glyph: "",
        name: "Rust Language",
        category: "NerdFont",
        keywords: "rust lang cargo",
    },
    GlyphRow {
        glyph: "",
        name: "Git",
        category: "NerdFont",
        keywords: "git vcs branch",
    },
    GlyphRow {
        glyph: "",
        name: "GitHub",
        category: "NerdFont",
        keywords: "github repo",
    },
    GlyphRow {
        glyph: "",
        name: "Terminal",
        category: "NerdFont",
        keywords: "shell console bash",
    },
    GlyphRow {
        glyph: "󰀻",
        name: "Apps Grid",
        category: "NerdFont",
        keywords: "launcher menu",
    },
    GlyphRow {
        glyph: "",
        name: "Search",
        category: "NerdFont",
        keywords: "find query ara",
    },
    GlyphRow {
        glyph: "󰋜",
        name: "Home",
        category: "NerdFont",
        keywords: "house ev",
    },
    GlyphRow {
        glyph: "",
        name: "Folder",
        category: "NerdFont",
        keywords: "dir klasor",
    },
    GlyphRow {
        glyph: "󰝚",
        name: "Music Note",
        category: "NerdFont",
        keywords: "audio song muzik",
    },
    GlyphRow {
        glyph: "󰕾",
        name: "Volume High",
        category: "NerdFont",
        keywords: "sound audio ses",
    },
    GlyphRow {
        glyph: "󰖩",
        name: "Wi-Fi",
        category: "NerdFont",
        keywords: "network internet ag",
    },
    GlyphRow {
        glyph: "󰂯",
        name: "Bluetooth",
        category: "NerdFont",
        keywords: "bt wireless",
    },
    GlyphRow {
        glyph: "󰌾",
        name: "Lock",
        category: "NerdFont",
        keywords: "security kilit",
    },
    GlyphRow {
        glyph: "⏻",
        name: "Power",
        category: "NerdFont",
        keywords: "shutdown guc",
    },
    GlyphRow {
        glyph: "󰒓",
        name: "Settings Cog",
        category: "NerdFont",
        keywords: "config ayar",
    },
    GlyphRow {
        glyph: "󰅌",
        name: "Clipboard",
        category: "NerdFont",
        keywords: "copy paste pano",
    },
    GlyphRow {
        glyph: "󰸉",
        name: "Wallpaper",
        category: "NerdFont",
        keywords: "image background duvar",
    },
    GlyphRow {
        glyph: "󰪚",
        name: "Calculator",
        category: "NerdFont",
        keywords: "math calc hesap",
    },
    GlyphRow {
        glyph: "󰊗",
        name: "Gamepad",
        category: "NerdFont",
        keywords: "games controller oyun",
    },
    GlyphRow {
        glyph: "󰗊",
        name: "Translate",
        category: "NerdFont",
        keywords: "language ceviri",
    },
];

/// Stateful emoji/kaomoji/Nerd Font picker mode.
pub struct EmojiMode {
    display: String,
    cache_dir: PathBuf,
    dry_run: bool,
    palette: Option<ActivePalette>,
    ordered: Vec<(String, String, String, String)>,
}

impl EmojiMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "emoji".to_string(),
            cache_dir: resolve_cache_dir(cfg),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
            ordered: Vec::new(),
        }
    }

    fn history_path(&self) -> PathBuf {
        self.cache_dir.join(EMOJI_HISTORY_FILE)
    }

    fn record_pick(&self, glyph: &str) {
        if self.dry_run {
            return;
        }
        let path = self.history_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut hist: Vec<String> = std::fs::read_to_string(&path)
            .ok()
            .map(|s| {
                s.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        hist.retain(|g| g != glyph);
        hist.insert(0, glyph.to_string());
        hist.truncate(MAX_EMOJI_HISTORY);
        let _ = std::fs::write(path, hist.join("\n"));
    }
}

impl RichMode for EmojiMode {
    fn name(&self) -> &str {
        "emoji"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        let hist: Vec<String> = std::fs::read_to_string(self.history_path())
            .ok()
            .map(|s| {
                s.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let mut seen = HashSet::new();
        let mut out = Vec::with_capacity(GLYPHS.len());
        for h in &hist {
            if let Some(row) = GLYPHS.iter().find(|r| r.glyph == h.as_str()) {
                if seen.insert(row.glyph) {
                    out.push((
                        row.glyph.to_string(),
                        row.name.to_string(),
                        row.category.to_string(),
                        row.keywords.to_string(),
                    ));
                }
            }
        }
        for row in GLYPHS {
            if seen.insert(row.glyph) {
                out.push((
                    row.glyph.to_string(),
                    row.name.to_string(),
                    row.category.to_string(),
                    row.keywords.to_string(),
                ));
            }
        }
        self.ordered = out;
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some("󰞅 Emoji".to_string())
    }

    fn message(&self) -> Option<String> {
        Some(format!(
            "{} emojis, kaomoji & Nerd Font icons · Enter: Copy to clipboard",
            self.ordered.len()
        ))
    }

    fn theme_snippets(&self) -> Vec<String> {
        vec!["element-icon { enabled: false; }".to_string()]
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.ordered
            .iter()
            .map(|(glyph, name, cat, kw)| ListEntry {
                text: format!("{glyph}   {name}   [{cat}]"),
                meta: Some(format!("{name} {cat} {kw}")),
                info: Some(glyph.clone()),
                ..ListEntry::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Accepted { index, .. } | UiOutcome::AcceptedAlt { index, .. } => {
                if let Some((glyph, _, _, _)) = self.ordered.get(*index).cloned() {
                    if !self.dry_run {
                        let _ = write_text_to_clipboard(&glyph);
                    }
                    self.record_pick(&glyph);
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            _ => Ok(ModeStep::Reload),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_mode_loads_all_categories() {
        let mut cfg = Config::default();
        cfg.modes_self_test = true;
        let mut m = EmojiMode::new(&cfg);
        m.init().expect("init emoji");
        let entries = m.entries();
        assert!(entries.len() >= 80);
        assert!(entries.iter().any(|e| e.text.contains("[Kaomoji]")));
        assert!(entries.iter().any(|e| e.text.contains("[NerdFont]")));
    }
}
