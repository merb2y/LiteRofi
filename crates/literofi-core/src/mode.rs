//! Mode abstraction mirroring `include/mode.h`.

/// State returned by UI loop / mode evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ModeState {
    Exit = 1000,
    NextDialog = 1001,
    ReloadDialog = 1002,
    PreviousDialog = 1003,
    ResetDialog = 1004,
}

/// Action flags produced by selecting/activating or triggering a keybinding in a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuReturn {
    Ok,
    Cancel,
    Next,
    Previous,
    CustomInput,
    EntryDelete,
    QuickSwitch(u16),
    CustomCommand,
    Complete,
    CustomAction(u8),
}

/// Visual/formatting state bits for a displayed row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModeEntryState {
    pub urgent: bool,
    pub active: bool,
    pub markup: bool,
    pub selected: bool,
}

/// Result of calling [`Mode::on_select`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeAction {
    Exit(i32),
    Reload,
    Reset,
    SwitchMode(String),
    NextMode,
    PreviousMode,
    Continue,
}

/// Computes the Rofi exit code for `-kb-custom-N` (`N` in `1..=19` -> `10..=28`).
pub fn custom_exit_code(custom_index: u8) -> i32 {
    match custom_index {
        1..=19 => 10 + i32::from(custom_index - 1),
        _ => 0,
    }
}

/// Interface implemented by all switcher modes (`dmenu`, `run`, `drun`, `window`, etc.).
pub trait Mode {
    fn name(&self) -> &str;
    fn display_name(&self) -> &str {
        self.name()
    }
    fn init(&mut self) -> Result<(), String>;
    fn destroy(&mut self) {}
    fn num_entries(&self) -> usize;
    fn display_value(&self, index: usize) -> (String, ModeEntryState);
    fn icon_name(&self, _index: usize) -> Option<&str> {
        None
    }
    fn message(&self) -> Option<&str> {
        None
    }
    fn preprocess_input<'a>(&self, input: &'a str) -> &'a str {
        input
    }
    fn on_select(&mut self, index: Option<usize>, input: &str, ret: MenuReturn) -> ModeAction;
}
