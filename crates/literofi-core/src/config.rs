//! Rofi-compatible configuration settings mirroring `include/settings.h`,
//! `config/config.c`, and `source/xrmoptions.c`.

use crate::keyb::KeyBindings;
use std::fmt::Write as _;

/// Supported Rofi matching algorithms (`MM_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchingMethod {
    #[default]
    Normal = 0,
    Regex = 1,
    Glob = 2,
    Fuzzy = 3,
    Prefix = 4,
}

impl MatchingMethod {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Self::Normal),
            "regex" => Some(Self::Regex),
            "glob" => Some(Self::Glob),
            "fuzzy" => Some(Self::Fuzzy),
            "prefix" => Some(Self::Prefix),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Regex => "regex",
            Self::Glob => "glob",
            Self::Fuzzy => "fuzzy",
            Self::Prefix => "prefix",
        }
    }
}

/// Supported sorting strategies (`SORT_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortingMethod {
    #[default]
    Normal = 0,
    Fzf = 1,
    FzfV2 = 2,
}

impl SortingMethod {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" | "levenshtein" => Some(Self::Normal),
            "fzf" => Some(Self::Fzf),
            "fzf_v2" | "fzf-v2" => Some(Self::FzfV2),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Fzf => "fzf",
            Self::FzfV2 => "fzf_v2",
        }
    }
}

/// Window anchor/gravity flags (`WindowLocation` in `include/rofi-types.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum WindowLocation {
    #[default]
    Center = 0,
    North = 1,
    East = 2,
    NorthEast = 3,
    South = 4,
    SouthEast = 6,
    West = 8,
    NorthWest = 9,
    SouthWest = 12,
}

impl WindowLocation {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::North,
            2 => Self::East,
            3 => Self::NorthEast,
            4 => Self::South,
            6 => Self::SouthEast,
            8 => Self::West,
            9 => Self::NorthWest,
            12 => Self::SouthWest,
            _ => Self::Center,
        }
    }
}

/// Complete Rofi runtime & configuration state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub modes: String,
    pub font: String,
    pub show_icons: bool,
    pub hide_icons: bool,
    pub blur_radius: u32,
    pub icon_theme: Option<String>,
    pub terminal: String,
    pub ssh_client: String,
    pub ssh_command: String,
    pub run_command: String,
    pub run_list_command: String,
    pub run_shell_command: String,
    pub window_command: String,
    pub window_match_fields: String,
    pub window_format: String,
    pub location: WindowLocation,
    pub x_offset: i32,
    pub y_offset: i32,
    pub fixed_num_lines: bool,
    pub disable_history: bool,
    pub ignored_prefixes: String,
    pub sort: bool,
    pub sorting_method: SortingMethod,
    pub case_sensitive: bool,
    pub case_smart: bool,
    pub cycle: bool,
    pub element_height: i32,
    pub sidebar_mode: bool,
    pub hover_select: bool,
    pub auto_select: bool,
    pub parse_hosts: bool,
    pub parse_known_hosts: bool,
    pub combi_modes: String,
    pub combi_hide_mode_prefix: bool,
    pub combi_display_format: String,
    pub tokenize: bool,
    pub matching: MatchingMethod,
    pub matching_negate_char: char,
    pub normalize_match: bool,
    pub drun_match_fields: String,
    pub drun_categories: Option<String>,
    pub drun_exclude_categories: Option<String>,
    pub drun_show_actions: bool,
    pub drun_display_format: String,
    pub drun_url_launcher: String,
    pub monitor: String,
    pub filter: Option<String>,
    pub dpi: i32,
    pub threads: u32,
    pub scroll_method: u32,
    pub click_to_exit: bool,
    pub steal_focus: bool,
    pub max_history_size: usize,
    pub refilter_timeout_limit: u32,
    pub cache_dir: Option<String>,
    pub application_fallback_icon: Option<String>,
    pub keybindings: KeyBindings,

    // Invocation / CLI runtime flags
    pub show: Option<String>,
    pub dmenu: bool,
    pub prompt: Option<String>,
    pub mesg: Option<String>,
    pub sep: u8,
    pub format: String,
    pub multi_select: bool,
    pub only_match: bool,
    pub no_custom: bool,
    pub select: Option<String>,
    pub selected_row: Option<usize>,
    pub active_ranges: Option<String>,
    pub urgent_ranges: Option<String>,
    pub markup_rows: bool,
    pub password: bool,
    pub input: Option<String>,
    pub display_drun: Option<String>,
    pub display_run: Option<String>,
    pub display_window: Option<String>,
    pub display_windowcd: Option<String>,
    pub display_combi: Option<String>,
    pub display_filebrowser: Option<String>,
    pub display_recursivebrowser: Option<String>,
    pub display_ssh: Option<String>,
    pub display_keys: Option<String>,
    pub filebrowser_directory: Option<String>,
    pub filebrowser_command: String,
    pub filebrowser_sorting_method: String,
    pub filebrowser_directories_first: bool,
    pub filebrowser_show_hidden: bool,
    pub recursivebrowser_directory: Option<String>,
    pub recursivebrowser_command: String,
    pub recursivebrowser_filter_regex: String,
    pub theme: Option<String>,
    pub theme_str: Vec<String>,
    pub config_file: Option<String>,
    pub no_config: bool,
    pub dump_config: bool,
    pub dump_theme: bool,
    pub show_version: bool,
    pub show_help: bool,
    pub open_settings: bool,
    pub open_themes_dir: bool,
    pub daemon: bool,
    pub start_daemon: bool,
    pub stop_daemon: bool,
    pub daemon_hotkey: String,
    pub daemon_hotkey_set: bool,
    pub daemon_mode: String,
    pub daemon_mode_set: bool,
    pub daemon_self_test: bool,
    pub render_self_test: bool,
    pub modes_self_test: bool,
    pub dump_png: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            modes: "window,drun,run,ssh".to_string(),
            font: "mono 12".to_string(),
            show_icons: false,
            hide_icons: false,
            blur_radius: 12,
            icon_theme: None,
            terminal: "rofi-sensible-terminal".to_string(),
            ssh_client: "ssh".to_string(),
            ssh_command: "{terminal} -e {ssh-client} {host} [-p {port}]".to_string(),
            run_command: "{cmd}".to_string(),
            run_list_command: String::new(),
            run_shell_command: "{terminal} -e {cmd}".to_string(),
            window_command: "wmctrl -i -R {window}".to_string(),
            window_match_fields: "all".to_string(),
            window_format: "{w}    {c}   {t}".to_string(),
            location: WindowLocation::Center,
            x_offset: 0,
            y_offset: 0,
            fixed_num_lines: true,
            disable_history: false,
            ignored_prefixes: String::new(),
            sort: false,
            sorting_method: SortingMethod::Normal,
            case_sensitive: false,
            case_smart: false,
            cycle: true,
            element_height: 1,
            sidebar_mode: false,
            hover_select: false,
            auto_select: false,
            parse_hosts: false,
            parse_known_hosts: true,
            combi_modes: "window,run".to_string(),
            combi_hide_mode_prefix: false,
            combi_display_format: "{mode} {text}".to_string(),
            tokenize: true,
            matching: MatchingMethod::Normal,
            matching_negate_char: '-',
            normalize_match: false,
            drun_match_fields: "name,generic,exec,categories,keywords".to_string(),
            drun_categories: None,
            drun_exclude_categories: None,
            drun_show_actions: false,
            drun_display_format:
                "{name} [<span weight='light' size='small'><i>({generic})</i></span>]".to_string(),
            drun_url_launcher: "xdg-open".to_string(),
            monitor: "-5".to_string(),
            filter: None,
            dpi: -1,
            threads: 0,
            scroll_method: 0,
            click_to_exit: true,
            steal_focus: false,
            max_history_size: 25,
            refilter_timeout_limit: 300,
            cache_dir: None,
            application_fallback_icon: None,
            keybindings: KeyBindings::default(),

            show: None,
            dmenu: false,
            prompt: None,
            mesg: None,
            sep: b'\n',
            format: "s".to_string(),
            multi_select: false,
            only_match: false,
            no_custom: false,
            select: None,
            selected_row: None,
            active_ranges: None,
            urgent_ranges: None,
            markup_rows: false,
            password: false,
            input: None,
            display_drun: None,
            display_run: None,
            display_window: None,
            display_windowcd: None,
            display_combi: None,
            display_filebrowser: None,
            display_recursivebrowser: None,
            display_ssh: None,
            display_keys: None,
            filebrowser_directory: None,
            filebrowser_command: "xdg-open".to_string(),
            filebrowser_sorting_method: "name".to_string(),
            filebrowser_directories_first: true,
            filebrowser_show_hidden: false,
            recursivebrowser_directory: None,
            recursivebrowser_command: "xdg-open".to_string(),
            recursivebrowser_filter_regex: "^(\\..*)".to_string(),
            theme: None,
            theme_str: Vec::new(),
            config_file: None,
            no_config: false,
            dump_config: false,
            dump_theme: false,
            show_version: false,
            show_help: false,
            open_settings: false,
            open_themes_dir: false,
            daemon: false,
            start_daemon: false,
            stop_daemon: false,
            daemon_hotkey: "Alt+Space".to_string(),
            daemon_hotkey_set: false,
            daemon_mode: "drun".to_string(),
            daemon_mode_set: false,
            daemon_self_test: false,
            render_self_test: false,
            modes_self_test: false,
            dump_png: None,
        }
    }
}

impl Config {
    /// Emits Rofi's `configuration { ... }` dump format matching `-dump-config`.
    pub fn dump_config_rasi(&self) -> String {
        let mut out = String::from("configuration {\n");
        let _ = writeln!(out, "\tmodes: \"{}\";", escape_rasi_str(&self.modes));
        let _ = writeln!(out, "\tfont: \"{}\";", escape_rasi_str(&self.font));
        let _ = writeln!(out, "\tlocation: {};", self.location as u8);
        let _ = writeln!(out, "\tyoffset: {};", self.y_offset);
        let _ = writeln!(out, "\txoffset: {};", self.x_offset);
        let _ = writeln!(out, "\tfixed-num-lines: {};", self.fixed_num_lines);
        let _ = writeln!(out, "\tshow-icons: {};", self.show_icons);
        let _ = writeln!(out, "\tblur-radius: {};", self.blur_radius);
        let _ = writeln!(out, "\tterminal: \"{}\";", escape_rasi_str(&self.terminal));
        let _ = writeln!(
            out,
            "\tssh-client: \"{}\";",
            escape_rasi_str(&self.ssh_client)
        );
        let _ = writeln!(
            out,
            "\tssh-command: \"{}\";",
            escape_rasi_str(&self.ssh_command)
        );
        let _ = writeln!(
            out,
            "\trun-command: \"{}\";",
            escape_rasi_str(&self.run_command)
        );
        let _ = writeln!(
            out,
            "\trun-list-command: \"{}\";",
            escape_rasi_str(&self.run_list_command)
        );
        let _ = writeln!(
            out,
            "\trun-shell-command: \"{}\";",
            escape_rasi_str(&self.run_shell_command)
        );
        let _ = writeln!(
            out,
            "\twindow-command: \"{}\";",
            escape_rasi_str(&self.window_command)
        );
        let _ = writeln!(
            out,
            "\twindow-match-fields: \"{}\";",
            escape_rasi_str(&self.window_match_fields)
        );
        let _ = writeln!(
            out,
            "\twindow-format: \"{}\";",
            escape_rasi_str(&self.window_format)
        );
        let _ = writeln!(
            out,
            "\tdrun-match-fields: \"{}\";",
            escape_rasi_str(&self.drun_match_fields)
        );
        let _ = writeln!(out, "\tdrun-show-actions: {};", self.drun_show_actions);
        let _ = writeln!(
            out,
            "\tdrun-display-format: \"{}\";",
            escape_rasi_str(&self.drun_display_format)
        );
        let _ = writeln!(
            out,
            "\tdrun-url-launcher: \"{}\";",
            escape_rasi_str(&self.drun_url_launcher)
        );
        let _ = writeln!(out, "\tdisable-history: {};", self.disable_history);
        let _ = writeln!(
            out,
            "\tignored-prefixes: \"{}\";",
            escape_rasi_str(&self.ignored_prefixes)
        );
        let _ = writeln!(out, "\tsort: {};", self.sort);
        let _ = writeln!(
            out,
            "\tsorting-method: \"{}\";",
            self.sorting_method.as_str()
        );
        let _ = writeln!(out, "\tcase-sensitive: {};", self.case_sensitive);
        let _ = writeln!(out, "\tcase-smart: {};", self.case_smart);
        let _ = writeln!(out, "\tcycle: {};", self.cycle);
        let _ = writeln!(out, "\tsidebar-mode: {};", self.sidebar_mode);
        let _ = writeln!(out, "\thover-select: {};", self.hover_select);
        let _ = writeln!(out, "\teh: {};", self.element_height);
        let _ = writeln!(out, "\tauto-select: {};", self.auto_select);
        let _ = writeln!(out, "\tparse-hosts: {};", self.parse_hosts);
        let _ = writeln!(out, "\tparse-known-hosts: {};", self.parse_known_hosts);
        let _ = writeln!(
            out,
            "\tcombi-modes: \"{}\";",
            escape_rasi_str(&self.combi_modes)
        );
        let _ = writeln!(out, "\tmatching: \"{}\";", self.matching.as_str());
        let _ = writeln!(out, "\ttokenize: {};", self.tokenize);
        let _ = writeln!(out, "\tmonitor: \"{}\";", escape_rasi_str(&self.monitor));
        if let Some(f) = &self.filter {
            let _ = writeln!(out, "\tfilter: \"{}\";", escape_rasi_str(f));
        }
        let _ = writeln!(out, "\tdpi: {};", self.dpi);
        let _ = writeln!(out, "\tthreads: {};", self.threads);
        let _ = writeln!(out, "\tscroll-method: {};", self.scroll_method);
        let _ = writeln!(out, "\tclick-to-exit: {};", self.click_to_exit);
        let _ = writeln!(out, "\tmax-history-size: {};", self.max_history_size);
        let _ = writeln!(
            out,
            "\tcombi-hide-mode-prefix: {};",
            self.combi_hide_mode_prefix
        );
        let _ = writeln!(
            out,
            "\tcombi-display-format: \"{}\";",
            escape_rasi_str(&self.combi_display_format)
        );
        let _ = writeln!(out, "\tnormalize-match: {};", self.normalize_match);
        let _ = writeln!(out, "\tsteal-focus: {};", self.steal_focus);
        let _ = writeln!(
            out,
            "\trefilter-timeout-limit: {};",
            self.refilter_timeout_limit
        );
        for entry in self.keybindings.entries() {
            let _ = writeln!(
                out,
                "\t{}: \"{}\";",
                entry.name,
                escape_rasi_str(&entry.binding)
            );
        }
        out.push_str("}\n");
        if let Some(theme) = &self.theme {
            let _ = writeln!(out, "@theme \"{}\"", escape_rasi_str(theme));
        }
        out
    }
}

fn escape_rasi_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
