//! Keybinding definitions and parser mirroring `include/keyb.h` and `source/keyb.c`.

/// Actions that can be triggered by keyboard bindings (`-kb-*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyBindingAction {
    PastePrimary,
    PasteSecondary,
    CopySecondary,
    ClearLine,
    MoveFront,
    MoveEnd,
    MoveWordBack,
    MoveWordForward,
    MoveCharBack,
    MoveCharForward,
    RemoveWordBack,
    RemoveWordForward,
    RemoveCharForward,
    RemoveCharBack,
    RemoveToEol,
    RemoveToSol,
    TransposeChars,
    AcceptEntry,
    AcceptAlt,
    AcceptCustom,
    AcceptCustomAlt,
    DeleteEntry,
    ModeNext,
    ModePrevious,
    ModeComplete,
    RowLeft,
    RowRight,
    RowUp,
    RowDown,
    RowTab,
    ElementNext,
    ElementPrev,
    PagePrev,
    PageNext,
    RowFirst,
    RowLast,
    RowSelect,
    Screenshot,
    ChangeEllipsize,
    ToggleCaseSensitivity,
    ToggleSort,
    Cancel,
    Custom(u8),
    SelectElement(u8),
    EntryHistoryUp,
    EntryHistoryDown,
    MatcherUp,
    MatcherDown,
}

/// Modifier key bitmask for parsed key chords.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

/// Single parsed key chord (e.g. `Control+Shift+Return`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub modifiers: Modifiers,
    pub key: String,
}

/// Parses a single Rofi chord like `Control+Alt+h`.
pub fn parse_chord(spec: &str) -> Option<KeyChord> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut modifiers = Modifiers::default();
    let mut key = String::new();
    let parts: Vec<&str> = trimmed.split('+').map(str::trim).collect();
    for (idx, part) in parts.iter().enumerate() {
        if idx + 1 < parts.len() {
            match part.to_ascii_lowercase().as_str() {
                "control" | "ctrl" | "primary" => modifiers.control = true,
                "alt" | "mod1" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "super" | "mod4" | "win" => modifiers.super_key = true,
                _ => {
                    if !key.is_empty() {
                        key.push('+');
                    }
                    key.push_str(part);
                }
            }
        } else {
            if !key.is_empty() {
                key.push('+');
            }
            key.push_str(part);
        }
    }
    if key.is_empty() {
        None
    } else {
        Some(KeyChord { modifiers, key })
    }
}

/// Parses a comma-separated list of key chords (`Control+j,Return,KP_Enter`).
pub fn parse_chord_list(spec: &str) -> Vec<KeyChord> {
    spec.split(',').filter_map(parse_chord).collect()
}

/// Named keybinding entry paired with its option name and default binding string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingEntry {
    pub action: KeyBindingAction,
    pub name: &'static str,
    pub default_binding: &'static str,
    pub binding: String,
    pub comment: &'static str,
}

/// Holds all configurable Rofi keyboard bindings (`kb-*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings {
    entries: Vec<BindingEntry>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let specs: &[(KeyBindingAction, &'static str, &'static str, &'static str)] = &[
            (
                KeyBindingAction::PastePrimary,
                "kb-primary-paste",
                "Control+V,Shift+Insert",
                "Paste primary selection",
            ),
            (
                KeyBindingAction::PasteSecondary,
                "kb-secondary-paste",
                "Control+v,Insert",
                "Paste clipboard",
            ),
            (
                KeyBindingAction::CopySecondary,
                "kb-secondary-copy",
                "Control+c",
                "Copy to clipboard",
            ),
            (
                KeyBindingAction::ClearLine,
                "kb-clear-line",
                "Control+w",
                "Clear input line",
            ),
            (
                KeyBindingAction::MoveFront,
                "kb-move-front",
                "Control+a",
                "Beginning of line",
            ),
            (
                KeyBindingAction::MoveEnd,
                "kb-move-end",
                "Control+e",
                "End of line",
            ),
            (
                KeyBindingAction::MoveWordBack,
                "kb-move-word-back",
                "Alt+b,Control+Left",
                "Move back one word",
            ),
            (
                KeyBindingAction::MoveWordForward,
                "kb-move-word-forward",
                "Alt+f,Control+Right",
                "Move forward one word",
            ),
            (
                KeyBindingAction::MoveCharBack,
                "kb-move-char-back",
                "Left,Control+b",
                "Move back one char",
            ),
            (
                KeyBindingAction::MoveCharForward,
                "kb-move-char-forward",
                "Right,Control+f",
                "Move forward one char",
            ),
            (
                KeyBindingAction::RemoveWordBack,
                "kb-remove-word-back",
                "Control+Alt+h,Control+BackSpace",
                "Delete previous word",
            ),
            (
                KeyBindingAction::RemoveWordForward,
                "kb-remove-word-forward",
                "Control+Alt+d",
                "Delete next word",
            ),
            (
                KeyBindingAction::RemoveCharForward,
                "kb-remove-char-forward",
                "Delete,Control+d",
                "Delete next char",
            ),
            (
                KeyBindingAction::RemoveCharBack,
                "kb-remove-char-back",
                "BackSpace,Shift+BackSpace,Control+h",
                "Delete previous char",
            ),
            (
                KeyBindingAction::RemoveToEol,
                "kb-remove-to-eol",
                "Control+k",
                "Delete till the end of line",
            ),
            (
                KeyBindingAction::RemoveToSol,
                "kb-remove-to-sol",
                "Control+u",
                "Delete till the start of line",
            ),
            (
                KeyBindingAction::TransposeChars,
                "kb-transpose-chars",
                "Control+t",
                "Transpose (swap) two chars before cursor",
            ),
            (
                KeyBindingAction::AcceptEntry,
                "kb-accept-entry",
                "Control+j,Control+m,Return,KP_Enter",
                "Accept entry",
            ),
            (
                KeyBindingAction::AcceptCustom,
                "kb-accept-custom",
                "Control+Return",
                "Use entered text as command (in ssh/run modes)",
            ),
            (
                KeyBindingAction::AcceptCustomAlt,
                "kb-accept-custom-alt",
                "Control+Shift+Return",
                "Use entered text as command (in ssh/run modes)",
            ),
            (
                KeyBindingAction::AcceptAlt,
                "kb-accept-alt",
                "Shift+Return",
                "Use alternate accept command.",
            ),
            (
                KeyBindingAction::DeleteEntry,
                "kb-delete-entry",
                "Shift+Delete",
                "Delete entry from history",
            ),
            (
                KeyBindingAction::ModeNext,
                "kb-mode-next",
                "Shift+Right,Control+Tab",
                "Switch to the next mode.",
            ),
            (
                KeyBindingAction::ModePrevious,
                "kb-mode-previous",
                "Shift+Left,Control+ISO_Left_Tab",
                "Switch to the previous mode.",
            ),
            (
                KeyBindingAction::ModeComplete,
                "kb-mode-complete",
                "Control+l",
                "Start completion for mode.",
            ),
            (
                KeyBindingAction::RowLeft,
                "kb-row-left",
                "Control+Page_Up",
                "Go to the previous column",
            ),
            (
                KeyBindingAction::RowRight,
                "kb-row-right",
                "Control+Page_Down",
                "Go to the next column",
            ),
            (
                KeyBindingAction::RowUp,
                "kb-row-up",
                "Up,Control+p",
                "Select previous entry",
            ),
            (
                KeyBindingAction::RowDown,
                "kb-row-down",
                "Down,Control+n",
                "Select next entry",
            ),
            (
                KeyBindingAction::RowTab,
                "kb-row-tab",
                "",
                "Go to next row, if one left, accept it, if no left next mode.",
            ),
            (
                KeyBindingAction::ElementNext,
                "kb-element-next",
                "Tab",
                "Go to next element (in logical order).",
            ),
            (
                KeyBindingAction::ElementPrev,
                "kb-element-prev",
                "ISO_Left_Tab",
                "Go to next previous element (in logical order).",
            ),
            (
                KeyBindingAction::PagePrev,
                "kb-page-prev",
                "Page_Up",
                "Go to the previous page",
            ),
            (
                KeyBindingAction::PageNext,
                "kb-page-next",
                "Page_Down",
                "Go to the next page",
            ),
            (
                KeyBindingAction::RowFirst,
                "kb-row-first",
                "Home,KP_Home",
                "Go to the first entry",
            ),
            (
                KeyBindingAction::RowLast,
                "kb-row-last",
                "End,KP_End",
                "Go to the last entry",
            ),
            (
                KeyBindingAction::RowSelect,
                "kb-row-select",
                "Control+space",
                "Set selected item as input text",
            ),
            (
                KeyBindingAction::Screenshot,
                "kb-screenshot",
                "Alt+S",
                "Take a screenshot of the rofi window",
            ),
            (
                KeyBindingAction::ChangeEllipsize,
                "kb-ellipsize",
                "Alt+period",
                "Toggle between ellipsize modes for displayed data",
            ),
            (
                KeyBindingAction::ToggleCaseSensitivity,
                "kb-toggle-case-sensitivity",
                "grave,dead_grave",
                "Toggle case sensitivity",
            ),
            (
                KeyBindingAction::ToggleSort,
                "kb-toggle-sort",
                "Alt+grave",
                "Toggle sort",
            ),
            (
                KeyBindingAction::Cancel,
                "kb-cancel",
                "Escape,Control+g,Control+bracketleft,MouseSecondary",
                "Quit rofi",
            ),
            (
                KeyBindingAction::Custom(1),
                "kb-custom-1",
                "Alt+1",
                "Custom keybinding 1",
            ),
            (
                KeyBindingAction::Custom(2),
                "kb-custom-2",
                "Alt+2",
                "Custom keybinding 2",
            ),
            (
                KeyBindingAction::Custom(3),
                "kb-custom-3",
                "Alt+3",
                "Custom keybinding 3",
            ),
            (
                KeyBindingAction::Custom(4),
                "kb-custom-4",
                "Alt+4",
                "Custom keybinding 4",
            ),
            (
                KeyBindingAction::Custom(5),
                "kb-custom-5",
                "Alt+5",
                "Custom keybinding 5",
            ),
            (
                KeyBindingAction::Custom(6),
                "kb-custom-6",
                "Alt+6",
                "Custom keybinding 6",
            ),
            (
                KeyBindingAction::Custom(7),
                "kb-custom-7",
                "Alt+7",
                "Custom keybinding 7",
            ),
            (
                KeyBindingAction::Custom(8),
                "kb-custom-8",
                "Alt+8",
                "Custom keybinding 8",
            ),
            (
                KeyBindingAction::Custom(9),
                "kb-custom-9",
                "Alt+9",
                "Custom keybinding 9",
            ),
            (
                KeyBindingAction::Custom(10),
                "kb-custom-10",
                "Alt+0",
                "Custom keybinding 10",
            ),
            (
                KeyBindingAction::Custom(11),
                "kb-custom-11",
                "Alt+exclam",
                "Custom keybinding 11",
            ),
            (
                KeyBindingAction::Custom(12),
                "kb-custom-12",
                "Alt+at",
                "Custom keybinding 12",
            ),
            (
                KeyBindingAction::Custom(13),
                "kb-custom-13",
                "Alt+numbersign",
                "Custom keybinding 13",
            ),
            (
                KeyBindingAction::Custom(14),
                "kb-custom-14",
                "Alt+dollar",
                "Custom keybinding 14",
            ),
            (
                KeyBindingAction::Custom(15),
                "kb-custom-15",
                "Alt+percent",
                "Custom keybinding 15",
            ),
            (
                KeyBindingAction::Custom(16),
                "kb-custom-16",
                "Alt+dead_circumflex",
                "Custom keybinding 16",
            ),
            (
                KeyBindingAction::Custom(17),
                "kb-custom-17",
                "Alt+ampersand",
                "Custom keybinding 17",
            ),
            (
                KeyBindingAction::Custom(18),
                "kb-custom-18",
                "Alt+asterisk",
                "Custom keybinding 18",
            ),
            (
                KeyBindingAction::Custom(19),
                "kb-custom-19",
                "Alt+parenleft",
                "Custom keybinding 19",
            ),
            (
                KeyBindingAction::SelectElement(1),
                "kb-select-1",
                "Super+1",
                "Select row 1",
            ),
            (
                KeyBindingAction::SelectElement(2),
                "kb-select-2",
                "Super+2",
                "Select row 2",
            ),
            (
                KeyBindingAction::SelectElement(3),
                "kb-select-3",
                "Super+3",
                "Select row 3",
            ),
            (
                KeyBindingAction::SelectElement(4),
                "kb-select-4",
                "Super+4",
                "Select row 4",
            ),
            (
                KeyBindingAction::SelectElement(5),
                "kb-select-5",
                "Super+5",
                "Select row 5",
            ),
            (
                KeyBindingAction::SelectElement(6),
                "kb-select-6",
                "Super+6",
                "Select row 6",
            ),
            (
                KeyBindingAction::SelectElement(7),
                "kb-select-7",
                "Super+7",
                "Select row 7",
            ),
            (
                KeyBindingAction::SelectElement(8),
                "kb-select-8",
                "Super+8",
                "Select row 8",
            ),
            (
                KeyBindingAction::SelectElement(9),
                "kb-select-9",
                "Super+9",
                "Select row 9",
            ),
            (
                KeyBindingAction::SelectElement(10),
                "kb-select-10",
                "Super+0",
                "Select row 10",
            ),
            (
                KeyBindingAction::EntryHistoryUp,
                "kb-entry-history-up",
                "Control+Up",
                "Go up in the history of the entry box",
            ),
            (
                KeyBindingAction::EntryHistoryDown,
                "kb-entry-history-down",
                "Control+Down",
                "Go down in the history of the entry box",
            ),
            (
                KeyBindingAction::MatcherUp,
                "kb-matcher-up",
                "Super+equal",
                "Switch to the previous matcher",
            ),
            (
                KeyBindingAction::MatcherDown,
                "kb-matcher-down",
                "Super+minus",
                "Switch to the next matcher",
            ),
        ];

        Self {
            entries: specs
                .iter()
                .map(|&(action, name, default_binding, comment)| BindingEntry {
                    action,
                    name,
                    default_binding,
                    binding: default_binding.to_string(),
                    comment,
                })
                .collect(),
        }
    }
}

impl KeyBindings {
    /// Updates a binding by option name (e.g. `"kb-row-up"`). Returns `false` if unknown.
    pub fn set_by_name(&mut self, name: &str, binding: &str) -> bool {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|e| e.name.eq_ignore_ascii_case(name))
        {
            entry.binding = binding.to_string();
            true
        } else {
            false
        }
    }

    /// Looks up a binding string by option name.
    pub fn get_by_name(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .map(|e| e.binding.as_str())
    }

    /// Returns all binding entries in registration order.
    pub fn entries(&self) -> &[BindingEntry] {
        &self.entries
    }
}
