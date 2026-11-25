//! `ssh` mode implementation mirroring `source/modes/ssh.c`.
//! Discovers SSH targets from `rofi-3.sshcache`, `~/.ssh/config` (including `Include` and
//! `UserKnownHostsFile`), `~/.ssh/known_hosts`, and system `hosts` file.

use crate::{
    expand_terminal_argv, expand_tilde_path, resolve_cache_dir, spawn_command_argv, user_home_dir,
    ModeStep, ReloadHints, RichMode,
};
use literofi_core::cli::parse_command_template;
use literofi_core::config::Config;
use literofi_core::history::{history_get_list, history_remove, history_set, HistoryConfig};
use literofi_render::state::{ListEntry, UiOutcome};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const SSH_CACHE_FILE: &str = "rofi-3.sshcache";
const DEFAULT_SSH_TEMPLATE: &str = "{terminal} -e {ssh-client} {host} [-p {port}]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct SshEntry {
    hostname: String,
    port: u16,
    aliases: Vec<String>,
    from_history: bool,
}

impl SshEntry {
    fn history_record(&self) -> String {
        let mut s = format!("{}\x1f{}", self.hostname, self.port);
        for alias in &self.aliases {
            s.push('\x1f');
            s.push_str(alias);
        }
        s
    }

    fn delete_key(&self) -> String {
        self.history_record()
    }
}

/// Stateful `ssh` host switcher mode.
pub struct SshMode {
    cfg: Config,
    display_name: String,
    cache_dir: PathBuf,
    ssh_config_override: Option<PathBuf>,
    known_hosts_override: Option<PathBuf>,
    etc_hosts_override: Option<PathBuf>,
    entries: Vec<SshEntry>,
    reload_hints: ReloadHints,
    dry_run: bool,
    launched: Vec<Vec<String>>,
}

impl SshMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            cfg: cfg.clone(),
            display_name: cfg.display_ssh.clone().unwrap_or_else(|| "ssh".to_string()),
            cache_dir: resolve_cache_dir(cfg),
            ssh_config_override: None,
            known_hosts_override: None,
            etc_hosts_override: None,
            entries: Vec::new(),
            reload_hints: ReloadHints::default(),
            dry_run: false,
            launched: Vec::new(),
        }
    }

    pub fn with_test_paths(
        mut self,
        ssh_config: Option<PathBuf>,
        known_hosts: Option<PathBuf>,
        etc_hosts: Option<PathBuf>,
        cache_dir: PathBuf,
        dry_run: bool,
    ) -> Self {
        self.ssh_config_override = ssh_config;
        self.known_hosts_override = known_hosts;
        self.etc_hosts_override = etc_hosts;
        self.cache_dir = cache_dir;
        self.dry_run = dry_run;
        self
    }

    pub fn launched_commands(&self) -> &[Vec<String>] {
        &self.launched
    }

    fn history_path(&self) -> PathBuf {
        self.cache_dir.join(SSH_CACHE_FILE)
    }

    fn history_cfg(&self) -> HistoryConfig {
        HistoryConfig {
            disabled: self.cfg.disable_history,
            max_size: self.cfg.max_history_size,
            ignored_prefixes: self.cfg.ignored_prefixes.clone(),
        }
    }

    fn exec_ssh(&mut self, entry: &SshEntry) -> Result<(), String> {
        let port_str = (entry.port > 0).then(|| entry.port.to_string());
        let argv = if self.cfg.terminal.trim() == "rofi-sensible-terminal"
            && self.cfg.ssh_command.trim() == DEFAULT_SSH_TEMPLATE
        {
            let mut ssh_cmd = format!("{} {}", self.cfg.ssh_client, entry.hostname);
            if let Some(p) = &port_str {
                ssh_cmd.push_str(&format!(" -p {p}"));
            }
            expand_terminal_argv(
                &self.cfg.terminal,
                &self.cfg.ssh_client,
                &self.cfg.run_shell_command,
                &ssh_cmd,
            )?
        } else {
            parse_command_template(
                &self.cfg.ssh_command,
                &self.cfg.terminal,
                &self.cfg.ssh_client,
                &[
                    ("{host}", Some(entry.hostname.as_str())),
                    ("{port}", port_str.as_deref()),
                ],
            )?
        };

        let hist_path = self.history_path();
        if let Some(parent) = hist_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = history_set(&hist_path, &entry.history_record(), &self.history_cfg());

        let log = self.dry_run.then_some(&mut self.launched);
        spawn_command_argv(&argv, None, log)
    }
}

impl RichMode for SshMode {
    fn name(&self) -> &str {
        "ssh"
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn init(&mut self) -> Result<(), String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut list: Vec<SshEntry> = Vec::new();

        // 1. History entries first
        let hist =
            history_get_list(&self.history_path(), self.cfg.disable_history).unwrap_or_default();
        for raw in hist {
            let mut parts = raw.split('\x1f');
            let Some(host) = parts.next().filter(|h| !h.is_empty()) else {
                continue;
            };
            let port = parts
                .next()
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(0);
            let aliases: Vec<String> = parts
                .filter(|a| !a.is_empty())
                .map(str::to_string)
                .collect();
            register_host_keys(&mut seen, host, &aliases);
            list.push(SshEntry {
                hostname: host.to_string(),
                port,
                aliases,
                from_history: true,
            });
        }

        // 2. ~/.ssh/config + Include + UserKnownHostsFile
        let home = user_home_dir();
        let config_path = self
            .ssh_config_override
            .clone()
            .unwrap_or_else(|| home.join(".ssh").join("config"));
        let mut extra_known_hosts: Vec<PathBuf> = Vec::new();
        if config_path.is_file() {
            parse_ssh_config_file(
                &config_path,
                &mut seen,
                &mut list,
                &mut extra_known_hosts,
                0,
            );
        }

        // 3. known_hosts files
        if self.cfg.parse_known_hosts {
            let primary_kh = self
                .known_hosts_override
                .clone()
                .unwrap_or_else(|| home.join(".ssh").join("known_hosts"));
            if primary_kh.is_file() {
                parse_known_hosts_file(&primary_kh, &mut seen, &mut list);
            }
            for kh in extra_known_hosts {
                if kh.is_file() {
                    parse_known_hosts_file(&kh, &mut seen, &mut list);
                }
            }
        }

        // 4. system hosts file
        if self.cfg.parse_hosts {
            let hosts_path = self
                .etc_hosts_override
                .clone()
                .unwrap_or_else(default_system_hosts_path);
            if hosts_path.is_file() {
                parse_etc_hosts_file(&hosts_path, &mut seen, &mut list);
            }
        }

        self.entries = list;
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.entries
            .iter()
            .map(|e| ListEntry {
                text: e.hostname.clone(),
                meta: (!e.aliases.is_empty()).then(|| e.aliases.join(" ")),
                active: e.from_history,
                ..ListEntry::default()
            })
            .collect()
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, _query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Accepted {
                index, exit_code, ..
            } => {
                if *exit_code >= 10 {
                    return Ok(ModeStep::Exit(*exit_code));
                }
                let entry = self
                    .entries
                    .get(*index)
                    .cloned()
                    .ok_or_else(|| format!("ssh index {index} out of bounds"))?;
                self.exec_ssh(&entry)?;
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::AcceptedAlt { index, .. } => {
                let entry = self
                    .entries
                    .get(*index)
                    .cloned()
                    .ok_or_else(|| format!("ssh index {index} out of bounds"))?;
                self.exec_ssh(&entry)?;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::CustomInput { text, exit_code } => {
                let host = text.trim();
                if host.is_empty() {
                    return Ok(ModeStep::Exit(1));
                }
                self.exec_ssh(&SshEntry {
                    hostname: host.to_string(),
                    port: 0,
                    aliases: Vec::new(),
                    from_history: false,
                })?;
                Ok(ModeStep::Exit(*exit_code))
            }
            UiOutcome::CustomInputAlt { text } => {
                let host = text.trim();
                if host.is_empty() {
                    return Ok(ModeStep::Exit(1));
                }
                self.exec_ssh(&SshEntry {
                    hostname: host.to_string(),
                    port: 0,
                    aliases: Vec::new(),
                    from_history: false,
                })?;
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { index } => {
                if let Some(entry) = self.entries.get(*index).cloned() {
                    let _ = history_remove(
                        &self.history_path(),
                        &entry.delete_key(),
                        self.cfg.disable_history,
                        self.cfg.max_history_size,
                    );
                    self.init()?;
                }
                self.reload_hints = ReloadHints {
                    keep_selection: true,
                    new_selection: None,
                    keep_filter: true,
                };
                Ok(ModeStep::Reload)
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
        }
    }
}

fn default_system_hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        PathBuf::from(root)
            .join("System32")
            .join("drivers")
            .join("etc")
            .join("hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

fn register_host_keys(seen: &mut HashSet<String>, host: &str, aliases: &[String]) {
    seen.insert(host.to_ascii_lowercase());
    for a in aliases {
        seen.insert(a.to_ascii_lowercase());
    }
}

fn parse_ssh_config_file(
    filename: &Path,
    seen: &mut HashSet<String>,
    out: &mut Vec<SshEntry>,
    extra_known_hosts: &mut Vec<PathBuf>,
    depth: u8,
) {
    if depth > 8 {
        return;
    }
    let Ok(content) = std::fs::read_to_string(filename) else {
        return;
    };
    let parent_dir = filename.parent().unwrap_or_else(|| Path::new("."));

    for raw_line in content.lines() {
        let stripped = strip_inline_comment(raw_line).trim();
        if stripped.is_empty() {
            continue;
        }
        let mut tokens = split_ssh_kv_tokens(stripped);
        if tokens.len() < 2 {
            continue;
        }
        let key = tokens.remove(0).to_ascii_lowercase();
        match key.as_str() {
            "include" => {
                for pat in tokens {
                    for inc_path in expand_ssh_include(parent_dir, &pat) {
                        parse_ssh_config_file(&inc_path, seen, out, extra_known_hosts, depth + 1);
                    }
                }
            }
            "userknownhostsfile" => {
                for p in tokens {
                    extra_known_hosts.push(expand_tilde_path(&p));
                }
            }
            "host" => {
                let mut host_names: Vec<String> = Vec::new();
                for tok in tokens {
                    if tok.starts_with('!') || tok.contains('*') || tok.contains('?') {
                        continue;
                    }
                    if seen.contains(&tok.to_ascii_lowercase()) {
                        continue;
                    }
                    host_names.push(tok);
                }
                if let Some((primary, rest)) = host_names.split_first() {
                    let aliases = rest.to_vec();
                    register_host_keys(seen, primary, &aliases);
                    out.push(SshEntry {
                        hostname: primary.clone(),
                        port: 0,
                        aliases,
                        from_history: false,
                    });
                }
            }
            _ => {}
        }
    }
}

fn strip_inline_comment(line: &str) -> &str {
    match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn split_ssh_kv_tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in line.chars() {
        if ch == '"' {
            in_quote = !in_quote;
            continue;
        }
        if !in_quote && (ch.is_whitespace() || ch == '=') {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn expand_ssh_include(base_dir: &Path, pattern: &str) -> Vec<PathBuf> {
    let expanded = expand_tilde_path(pattern);
    let full = if expanded.is_relative() {
        base_dir.join(expanded)
    } else {
        expanded
    };
    let s = full.to_string_lossy();
    if !s.contains('*') && !s.contains('?') {
        return full.is_file().then_some(full).into_iter().collect();
    }
    let Some(dir) = full.parent() else {
        return Vec::new();
    };
    let Some(pat_name) = full.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for item in rd.flatten() {
            let p = item.path();
            if p.is_file() {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if glob_match_simple(pat_name, name) {
                        matches.push(p);
                    }
                }
            }
        }
    }
    matches.sort();
    matches
}

fn glob_match_simple(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut dp = vec![false; t.len() + 1];
    dp[0] = true;
    for &pc in &p {
        let mut next = vec![false; t.len() + 1];
        if pc == '*' {
            let mut any = false;
            for j in 0..=t.len() {
                if dp[j] {
                    any = true;
                }
                next[j] = any;
            }
        } else {
            for j in 0..t.len() {
                if dp[j] && (pc == '?' || pc.eq_ignore_ascii_case(&t[j])) {
                    next[j + 1] = true;
                }
            }
        }
        dp = next;
    }
    dp[t.len()]
}

fn parse_known_hosts_file(path: &Path, seen: &mut HashSet<String>, out: &mut Vec<SshEntry>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('|') {
            continue;
        }
        let Some(first_col) = line.split_whitespace().next() else {
            continue;
        };
        if first_col.starts_with('@') {
            continue;
        }
        for raw_host in first_col.split(',') {
            let (host, port) = parse_known_host_token(raw_host);
            if host.is_empty() || host.starts_with('!') || host.contains('*') || host.contains('?')
            {
                continue;
            }
            if seen.insert(host.to_ascii_lowercase()) {
                out.push(SshEntry {
                    hostname: host,
                    port,
                    aliases: Vec::new(),
                    from_history: false,
                });
            }
        }
    }
}

fn parse_known_host_token(tok: &str) -> (String, u16) {
    let t = tok.trim();
    if let Some(rest) = t.strip_prefix('[') {
        if let Some((host, after)) = rest.split_once("]:") {
            let port = after.parse::<u16>().unwrap_or(0);
            return (host.to_string(), port);
        }
        if let Some(host) = rest.strip_suffix(']') {
            return (host.to_string(), 0);
        }
    }
    (t.to_string(), 0)
}

fn parse_etc_hosts_file(path: &Path, seen: &mut HashSet<String>, out: &mut Vec<SshEntry>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    for raw_line in content.lines() {
        let line = strip_inline_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let mut cols = line.split_whitespace();
        let _ip = cols.next();
        for host in cols {
            if seen.insert(host.to_ascii_lowercase()) {
                out.push(SshEntry {
                    hostname: host.to_string(),
                    port: 0,
                    aliases: Vec::new(),
                    from_history: false,
                });
            }
        }
    }
}
