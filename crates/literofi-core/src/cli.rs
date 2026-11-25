//! Rofi-compatible CLI option & command-template parser mirroring
//! `find_arg*`, `helper_parse_char`, `helper_string_replace_if_exists`,
//! `helper_parse_setup` (`source/helper.c`), and `source/xrmoptions.c`.

use crate::config::{Config, MatchingMethod, SortingMethod, WindowLocation};

/// Wrapper around raw `argv` supporting Rofi's case-insensitive `find_arg*` helpers.
#[derive(Debug, Clone)]
pub struct Argv<'a> {
    args: &'a [String],
}

impl<'a> Argv<'a> {
    pub fn new(args: &'a [String]) -> Self {
        Self { args }
    }

    pub fn find_arg(&self, key: &str) -> Option<usize> {
        self.args.iter().position(|a| a.eq_ignore_ascii_case(key))
    }

    pub fn find_arg_str(&self, key: &str) -> Option<&'a str> {
        let i = self.find_arg(key)?;
        if i > 0 && i + 1 < self.args.len() {
            Some(self.args[i + 1].as_str())
        } else {
            None
        }
    }

    pub fn find_arg_strv(&self, key: &str) -> Vec<&'a str> {
        let mut out = Vec::new();
        for i in 0..self.args.len() {
            if i + 1 < self.args.len() && self.args[i].eq_ignore_ascii_case(key) {
                out.push(self.args[i + 1].as_str());
            }
        }
        out
    }

    pub fn find_arg_int(&self, key: &str) -> Option<i32> {
        let val = self.find_arg_str(key)?;
        Some(parse_c_int_prefix(val) as i32)
    }

    pub fn find_arg_uint(&self, key: &str) -> Option<u32> {
        let val = self.find_arg_str(key)?;
        // C `strtoul` accepts optional leading `-` and wraps to `unsigned long` (`u32`).
        Some(parse_c_int_prefix(val) as u32)
    }

    pub fn find_arg_char(&self, key: &str) -> Option<u8> {
        self.find_arg_str(key).map(parse_char)
    }
}

fn parse_c_int_prefix(s: &str) -> i64 {
    let trimmed = s.trim_start();
    if trimmed.is_empty() {
        return 0;
    }
    let mut end = 0usize;
    for (idx, ch) in trimmed.char_indices() {
        if (idx == 0 && (ch == '-' || ch == '+')) || ch.is_ascii_digit() {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    trimmed[..end].parse::<i64>().unwrap_or(0)
}

/// Parses a character literal or Rofi escape (`\n`, `\a`, `\b`, `\t`, `\v`, `\f`,
/// `\r`, `\\`, `\0`, `\xHH`), mirroring `helper_parse_char`.
pub fn parse_char(arg: &str) -> u8 {
    let bytes = arg.as_bytes();
    if bytes.len() == 1 {
        return bytes[0];
    }
    if bytes.len() == 2 && bytes[0] == b'\\' {
        return match bytes[1] {
            b'n' => b'\n',
            b'a' => 0x07,
            b'b' => 0x08,
            b't' => b'\t',
            b'v' => 0x0b,
            b'f' => 0x0c,
            b'r' => b'\r',
            b'\\' => b'\\',
            b'0' => 0,
            _ => b'\n',
        };
    }
    if bytes.len() > 2 && bytes[0] == b'\\' && bytes[1] == b'x' {
        if let Ok(v) = u8::from_str_radix(&arg[2..], 16) {
            return v;
        }
    }
    b'\n'
}

/// Expands `{key}` and optional guarded `[prefix{key}postfix]` segments matching
/// `helper_string_replace_if_exists` (`source/helper.c:1748-1854`).
pub fn string_replace_if_exists(template: &str, vars: &[(&str, Option<&str>)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut i = 0usize;
    let bytes = template.as_bytes();

    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(rel_close) = template[i + 1..].find(']') {
                let inner = &template[i + 1..i + 1 + rel_close];
                if let Some((prefix, key, postfix)) = split_guarded_placeholder(inner) {
                    if let Some(val) = lookup_var(vars, key) {
                        out.push_str(prefix);
                        out.push_str(val);
                        out.push_str(postfix);
                    }
                    i += 1 + rel_close + 1;
                    continue;
                }
            }
        }

        if bytes[i] == b'{' {
            if let Some(rel_close) = template[i + 1..].find('}') {
                let candidate = &template[i..=i + 1 + rel_close];
                if is_placeholder_token(candidate) {
                    if let Some(val) = lookup_var(vars, candidate) {
                        out.push_str(val);
                    }
                    i += 1 + rel_close + 1;
                    continue;
                }
            }
        }

        let ch = template[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn is_placeholder_token(token: &str) -> bool {
    token.starts_with('{')
        && token.ends_with('}')
        && token.len() > 2
        && token[1..token.len() - 1]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn split_guarded_placeholder(inner: &str) -> Option<(&str, &str, &str)> {
    let open = inner.find('{')?;
    let rel_close = inner[open + 1..].find('}')?;
    let close = open + 1 + rel_close;
    let key = &inner[open..=close];
    if is_placeholder_token(key) {
        Some((&inner[..open], key, &inner[close + 1..]))
    } else {
        None
    }
}

fn lookup_var<'a>(vars: &[(&str, Option<&'a str>)], key: &str) -> Option<&'a str> {
    // Later / explicit entries take precedence over defaults (matching GHashTable overwrite).
    for &(k, v) in vars.iter().rev() {
        if k == key {
            return v;
        }
    }
    None
}

/// Splits a shell-like command line supporting `'...'`, `"..."`, and `\` escapes (`g_shell_parse_argv`).
pub fn split_shell_argv(cmd: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut chars = cmd.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            '\'' => {
                in_arg = true;
                let mut closed = false;
                for sc in chars.by_ref() {
                    if sc == '\'' {
                        closed = true;
                        break;
                    }
                    current.push(sc);
                }
                if !closed {
                    return Err("unclosed single quote in command template".to_string());
                }
            }
            '"' => {
                in_arg = true;
                let mut closed = false;
                while let Some(dc) = chars.next() {
                    if dc == '"' {
                        closed = true;
                        break;
                    }
                    if dc == '\\' {
                        match chars.peek().copied() {
                            Some(next @ ('"' | '\\' | '$' | '`')) => {
                                chars.next();
                                current.push(next);
                            }
                            Some('\n') => {
                                chars.next();
                            }
                            _ => current.push('\\'),
                        }
                    } else {
                        current.push(dc);
                    }
                }
                if !closed {
                    return Err("unclosed double quote in command template".to_string());
                }
            }
            '\\' => {
                in_arg = true;
                if let Some(next) = chars.next() {
                    if next != '\n' {
                        current.push(next);
                    }
                }
            }
            other => {
                in_arg = true;
                current.push(other);
            }
        }
    }
    if in_arg {
        args.push(current);
    }
    Ok(args)
}

/// Combines default `{terminal}`/`{ssh-client}` substitutions + `vars` and parses into argv (`helper_parse_setup`).
pub fn parse_command_template(
    template: &str,
    terminal: &str,
    ssh_client: &str,
    vars: &[(&str, Option<&str>)],
) -> Result<Vec<String>, String> {
    let mut merged = Vec::with_capacity(vars.len() + 2);
    merged.push(("{terminal}", Some(terminal)));
    merged.push(("{ssh-client}", Some(ssh_client)));
    merged.extend_from_slice(vars);
    let expanded = string_replace_if_exists(template, &merged);
    split_shell_argv(&expanded)
}

/// Parses Rofi-style command line flags (`args[0]` is program name).
pub fn parse_argv(args: &[String]) -> Result<Config, String> {
    let mut cfg = Config::default();
    let mut i = 1usize;

    while i < args.len() {
        let flag = args[i].as_str();
        let next_val = |idx: usize| -> Result<&str, String> {
            args.get(idx + 1)
                .map(String::as_str)
                .ok_or_else(|| format!("missing argument for `{flag}`"))
        };

        match flag {
            "-h" | "-help" | "--help" => cfg.show_help = true,
            "-version" | "--version" => cfg.show_version = true,
            "-dump-config" => cfg.dump_config = true,
            "-dump-theme" => cfg.dump_theme = true,
            "-config" => {
                cfg.config_file = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-no-config" => cfg.no_config = true,
            "--daemon" | "-daemon" => cfg.daemon = true,
            "--start-daemon" | "-start-daemon" | "--daemon-bg" => cfg.start_daemon = true,
            "--stop-daemon" | "-stop-daemon" => cfg.stop_daemon = true,
            "--daemon-hotkey" | "-daemon-hotkey" => {
                cfg.daemon_hotkey = next_val(i)?.to_string();
                cfg.daemon_hotkey_set = true;
                i += 1;
            }
            "--daemon-mode" | "-daemon-mode" => {
                cfg.daemon_mode = next_val(i)?.to_string();
                cfg.daemon_mode_set = true;
                i += 1;
            }
            "--settings" | "-settings" => cfg.open_settings = true,
            "--themes-dir" | "-themes-dir" => cfg.open_themes_dir = true,
            "--daemon-self-test" => cfg.daemon_self_test = true,
            "--render-self-test" => cfg.render_self_test = true,
            "--modes-self-test" => cfg.modes_self_test = true,
            "--dump-png" | "-dump-png" => {
                cfg.dump_png = Some(next_val(i)?.to_string());
                i += 1;
            }

            "-show" => {
                cfg.show = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-modi" | "-modes" | "-switchers" => {
                cfg.modes = next_val(i)?.to_string();
                i += 1;
            }
            "-combi-modi" | "-combi-modes" => {
                cfg.combi_modes = next_val(i)?.to_string();
                i += 1;
            }
            "-dmenu" => cfg.dmenu = true,
            "-p" => {
                cfg.prompt = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-mesg" => {
                cfg.mesg = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-sep" => {
                cfg.sep = parse_char(next_val(i)?);
                i += 1;
            }
            "-0" => cfg.sep = 0,
            "-format" => {
                cfg.format = next_val(i)?.to_string();
                i += 1;
            }
            "-filter" => {
                cfg.filter = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-select" => {
                cfg.select = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-selected-row" => {
                let raw = next_val(i)?;
                cfg.selected_row = Some(
                    raw.parse::<usize>()
                        .map_err(|_| format!("invalid integer for `-selected-row`: `{raw}`"))?,
                );
                i += 1;
            }
            "-a" => {
                cfg.active_ranges = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-u" => {
                cfg.urgent_ranges = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-multi-select" => cfg.multi_select = true,
            "-only-match" => cfg.only_match = true,
            "-no-custom" => cfg.no_custom = true,
            "-markup-rows" => cfg.markup_rows = true,
            "-password" => cfg.password = true,
            "-input" => {
                cfg.input = Some(next_val(i)?.to_string());
                i += 1;
            }

            "-i" | "-no-case-sensitive" => cfg.case_sensitive = false,
            "-case-sensitive" => cfg.case_sensitive = true,
            "-case-smart" => cfg.case_smart = true,
            "-no-case-smart" => cfg.case_smart = false,

            "-sort" => cfg.sort = true,
            "-no-sort" => cfg.sort = false,
            "-sorting-method" => {
                let raw = next_val(i)?;
                cfg.sorting_method = SortingMethod::parse(raw)
                    .ok_or_else(|| format!("invalid sorting method: `{raw}`"))?;
                i += 1;
            }
            "-matching" => {
                let raw = next_val(i)?;
                cfg.matching = MatchingMethod::parse(raw)
                    .ok_or_else(|| format!("invalid matching method: `{raw}`"))?;
                i += 1;
            }
            "-matching-negate-char" => {
                let b = parse_char(next_val(i)?);
                cfg.matching_negate_char = char::from(b);
                i += 1;
            }
            "-tokenize" => cfg.tokenize = true,
            "-no-tokenize" => cfg.tokenize = false,
            "-normalize-match" => cfg.normalize_match = true,
            "-no-normalize-match" => cfg.normalize_match = false,

            "-show-icons" => {
                cfg.show_icons = true;
                cfg.hide_icons = false;
            }
            "-no-show-icons" => {
                cfg.show_icons = false;
                cfg.hide_icons = true;
            }
            "-blur-radius" => {
                cfg.blur_radius = next_val(i)?.parse::<u32>().unwrap_or(12).min(64);
                i += 1;
            }
            "-no-blur" => cfg.blur_radius = 0,
            "-disable-history" => cfg.disable_history = true,
            "-no-disable-history" => cfg.disable_history = false,
            "-sidebar-mode" => cfg.sidebar_mode = true,
            "-no-sidebar-mode" => cfg.sidebar_mode = false,
            "-cycle" => cfg.cycle = true,
            "-no-cycle" => cfg.cycle = false,
            "-fixed-num-lines" => cfg.fixed_num_lines = true,
            "-no-fixed-num-lines" => cfg.fixed_num_lines = false,
            "-click-to-exit" => cfg.click_to_exit = true,
            "-no-click-to-exit" => cfg.click_to_exit = false,

            "-font" => {
                cfg.font = next_val(i)?.to_string();
                i += 1;
            }
            "-m" | "-monitor" => {
                cfg.monitor = next_val(i)?.to_string();
                i += 1;
            }
            "-location" => {
                let raw = next_val(i)?;
                let v = raw
                    .parse::<u8>()
                    .map_err(|_| format!("invalid integer for `-location`: `{raw}`"))?;
                cfg.location = WindowLocation::from_u8(v);
                i += 1;
            }
            "-xoffset" => {
                let raw = next_val(i)?;
                cfg.x_offset = raw
                    .parse::<i32>()
                    .map_err(|_| format!("invalid integer for `-xoffset`: `{raw}`"))?;
                i += 1;
            }
            "-yoffset" => {
                let raw = next_val(i)?;
                cfg.y_offset = raw
                    .parse::<i32>()
                    .map_err(|_| format!("invalid integer for `-yoffset`: `{raw}`"))?;
                i += 1;
            }
            "-dpi" => {
                let raw = next_val(i)?;
                cfg.dpi = raw
                    .parse::<i32>()
                    .map_err(|_| format!("invalid integer for `-dpi`: `{raw}`"))?;
                i += 1;
            }
            "-max-history-size" => {
                let raw = next_val(i)?;
                cfg.max_history_size = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid integer for `-max-history-size`: `{raw}`"))?;
                i += 1;
            }
            "-theme" => {
                cfg.theme = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-theme-str" => {
                cfg.theme_str.push(next_val(i)?.to_string());
                i += 1;
            }
            "-terminal" => {
                cfg.terminal = next_val(i)?.to_string();
                i += 1;
            }
            "-run-command" => {
                cfg.run_command = next_val(i)?.to_string();
                i += 1;
            }
            "-run-shell-command" => {
                cfg.run_shell_command = next_val(i)?.to_string();
                i += 1;
            }
            "-run-list-command" => {
                cfg.run_list_command = next_val(i)?.to_string();
                i += 1;
            }
            "-ignored-prefixes" => {
                cfg.ignored_prefixes = next_val(i)?.to_string();
                i += 1;
            }
            "-cache-dir" => {
                cfg.cache_dir = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-display-drun" => {
                cfg.display_drun = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-display-run" => {
                cfg.display_run = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-display-window" => {
                cfg.display_window = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-display-windowcd" => {
                cfg.display_windowcd = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-display-combi" => {
                cfg.display_combi = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-drun-display-format" => {
                cfg.drun_display_format = next_val(i)?.to_string();
                i += 1;
            }
            "-drun-match-fields" => {
                cfg.drun_match_fields = next_val(i)?.to_string();
                i += 1;
            }
            "-drun-categories" => {
                cfg.drun_categories = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-drun-exclude-categories" => {
                cfg.drun_exclude_categories = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-window-format" => {
                cfg.window_format = next_val(i)?.to_string();
                i += 1;
            }
            "-window-match-fields" => {
                cfg.window_match_fields = next_val(i)?.to_string();
                i += 1;
            }
            "-window-command" => {
                cfg.window_command = next_val(i)?.to_string();
                i += 1;
            }
            "-combi-display-format" => {
                cfg.combi_display_format = next_val(i)?.to_string();
                i += 1;
            }
            "-combi-hide-mode-prefix" => cfg.combi_hide_mode_prefix = true,
            "-no-combi-hide-mode-prefix" => cfg.combi_hide_mode_prefix = false,
            "-display-filebrowser" => {
                cfg.display_filebrowser = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-display-recursivebrowser" => {
                cfg.display_recursivebrowser = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-display-ssh" => {
                cfg.display_ssh = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-display-keys" => {
                cfg.display_keys = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-filebrowser-directory" => {
                cfg.filebrowser_directory = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-filebrowser-command" => {
                cfg.filebrowser_command = next_val(i)?.to_string();
                i += 1;
            }
            "-filebrowser-sorting-method" => {
                cfg.filebrowser_sorting_method = next_val(i)?.to_string();
                i += 1;
            }
            "-filebrowser-directories-first" => cfg.filebrowser_directories_first = true,
            "-no-filebrowser-directories-first" => cfg.filebrowser_directories_first = false,
            "-filebrowser-show-hidden" => cfg.filebrowser_show_hidden = true,
            "-no-filebrowser-show-hidden" => cfg.filebrowser_show_hidden = false,
            "-recursivebrowser-directory" => {
                cfg.recursivebrowser_directory = Some(next_val(i)?.to_string());
                i += 1;
            }
            "-recursivebrowser-command" => {
                cfg.recursivebrowser_command = next_val(i)?.to_string();
                i += 1;
            }
            "-recursivebrowser-filter-regex" => {
                cfg.recursivebrowser_filter_regex = next_val(i)?.to_string();
                i += 1;
            }
            "-ssh-client" => {
                cfg.ssh_client = next_val(i)?.to_string();
                i += 1;
            }
            "-ssh-command" => {
                cfg.ssh_command = next_val(i)?.to_string();
                i += 1;
            }
            "-parse-hosts" => cfg.parse_hosts = true,
            "-no-parse-hosts" => cfg.parse_hosts = false,
            "-parse-known-hosts" => cfg.parse_known_hosts = true,
            "-no-parse-known-hosts" => cfg.parse_known_hosts = false,
            other if other.starts_with("-kb-") => {
                let key_name = &other[1..];
                let val = next_val(i)?;
                if !cfg.keybindings.set_by_name(key_name, val) {
                    return Err(format!("unknown keybinding option `{other}`"));
                }
                i += 1;
            }
            other => {
                return Err(format!("unknown option `{other}`"));
            }
        }
        i += 1;
    }

    Ok(cfg)
}

/// Rofi-style CLI usage summary.
pub fn usage_text() -> &'static str {
    "LiteRofi usage:\n\
     \tliterofi [-options]\n\n\
     Main options:\n\
     \t-show <mode>            Start in mode (drun, run, window, windowcd, combi, script,\n\
     \t                        filebrowser, recursivebrowser, ssh, keys)\n\
     \t-dmenu                  Run in dmenu filter mode reading stdin\n\
     \t-modes / -modi <list>   Comma-separated enabled modes\n\
     \t--daemon                Listen in foreground for global hotkey\n\
     \t--start-daemon          Start background hotkey listener with no console window\n\
     \t--stop-daemon           Stop running background hotkey listener\n\
     \t--daemon-hotkey <chord> Global hotkey for daemon (default: Alt+Space)\n\
     \t--settings              Open %APPDATA%\\LiteRofi\\config.rasi (creating starter if missing)\n\
     \t--themes-dir            Open %APPDATA%\\LiteRofi\\themes folder\n\
     \t-matching <method>      normal | regex | glob | fuzzy | prefix\n\\
     \t-sorting-method <name>  normal | fzf | fzf_v2\n\
     \t-theme <file>           Load .rasi theme file\n\
     \t-theme-str <snippet>    Override theme snippet (repeatable)\n\
     \t-dump-config            Dump current configuration in .rasi format\n\
     \t-dump-theme             Dump active theme in .rasi format\n\
     \t-version                Print version and exit\n\
     \t-h / -help              Show this help\n"
}
