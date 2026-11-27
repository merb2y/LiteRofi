//! `script` mode implementation mirroring `source/modes/script.c`.
//! Executes custom switcher scripts (`name:command`), sets `ROFI_RETV`/`ROFI_OUTSIDE`/
//! `ROFI_INFO`/`ROFI_DATA`/`ROFI_INPUT`, and parses `\0key\x1fvalue` header/row updates.

use crate::{ModeStep, ReloadHints, RichMode};
use literofi_core::cli::split_shell_argv;
use literofi_core::config::Config;
use literofi_core::protocol::parse_dmenu_script_stream;
use literofi_render::state::{ListEntry, UiOutcome};
#[cfg(windows)]
use std::io::Read;
use std::path::Path;
use std::process::Command;

fn script_command(program: &str) -> Result<Command, String> {
    #[cfg(windows)]
    {
        let mut path = Path::new(program).to_owned();
        if path.components().count() == 1 && !path.is_file() {
            if let Some(root) = std::env::var_os("APPDATA") {
                let scripts = Path::new(&root).join("LiteRofi/scripts");
                for suffix in ["", ".sh", ".ps1", ".exe", ".cmd", ".bat"] {
                    let candidate = scripts.join(format!("{program}{suffix}"));
                    if candidate.is_file() {
                        path = candidate;
                        break;
                    }
                }
            }
        }
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("ps1") {
            let mut cmd = Command::new("powershell.exe");
            cmd.args(["-NoProfile", "-File"]).arg(&path);
            return Ok(cmd);
        }
        let mut prefix = [0; 256];
        let bash_shebang = ext.is_empty()
            && std::fs::File::open(&path)
                .and_then(|mut file| file.read(&mut prefix))
                .ok()
                .and_then(|n| std::str::from_utf8(prefix[..n].split(|b| *b == b'\n').next()?).ok())
                .and_then(|line| line.strip_prefix("#!"))
                .is_some_and(|line| {
                    line.split_whitespace()
                        .any(|word| matches!(word.rsplit('/').next(), Some("bash" | "sh")))
                });
        if bash_shebang || ext.eq_ignore_ascii_case("sh") || ext.eq_ignore_ascii_case("bash") {
            let bash = std::env::var_os("LITEROFI_BASH").map(std::path::PathBuf::from)
                .or_else(|| ["ProgramW6432", "ProgramFiles", "LOCALAPPDATA"].iter()
                    .filter_map(std::env::var_os)
                    .flat_map(|p| [Path::new(&p).join("Git/bin/bash.exe"), Path::new(&p).join("Programs/Git/bin/bash.exe")])
                    .find(|p| p.is_file()))
                .ok_or("Git Bash was not found. Install Git for Windows or set LITEROFI_BASH to bash.exe.")?;
            let mut cmd = Command::new(bash);
            cmd.arg("--").arg(path.to_string_lossy().replace('\\', "/"));
            return Ok(cmd);
        }
        return Ok(Command::new(path));
    }
    #[cfg(not(windows))]
    Ok(Command::new(program))
}

/// Stateful custom `script` switcher mode (`name:command`).
pub struct ScriptMode {
    mode_name: String,
    command: String,
    markup_rows: Option<bool>,
    delim: u8,
    entries: Vec<ListEntry>,
    message: Option<String>,
    prompt_override: Option<String>,
    no_custom: bool,
    use_hot_keys: bool,
    data: Option<String>,
    theme_snippets: Vec<String>,
    reload_hints: ReloadHints,
}

impl ScriptMode {
    pub fn new(name: String, command: String, cfg: &Config) -> Self {
        Self {
            mode_name: name,
            command,
            markup_rows: None,
            delim: cfg.sep,
            entries: Vec::new(),
            message: cfg.mesg.clone(),
            prompt_override: None,
            no_custom: cfg.no_custom,
            use_hot_keys: false,
            data: None,
            theme_snippets: Vec::new(),
            reload_hints: ReloadHints::default(),
        }
    }

    fn execute(
        &mut self,
        arg: Option<&str>,
        retv: i32,
        entry_info: Option<&str>,
        input: Option<&str>,
    ) -> Result<Option<ModeStep>, String> {
        let argv = if std::path::Path::new(&self.command).is_file() {
            vec![self.command.clone()]
        } else {
            split_shell_argv(&self.command)?
        };
        if argv.is_empty() {
            return Err(format!(
                "empty script command for mode `{}`",
                self.mode_name
            ));
        }

        let mut cmd = script_command(&argv[0])?;
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_owned))
        {
            let mut paths = vec![dir];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
            if let Ok(path) = std::env::join_paths(paths) {
                cmd.env("PATH", path);
            }
        }
        if argv.len() > 1 {
            cmd.args(&argv[1..]);
        }
        if let Some(a) = arg {
            cmd.arg(a);
        }

        cmd.env("ROFI_RETV", retv.to_string());
        cmd.env("ROFI_OUTSIDE", std::process::id().to_string());
        if let Some(info) = entry_info {
            cmd.env("ROFI_INFO", info);
        }
        if let Some(data) = &self.data {
            cmd.env("ROFI_DATA", data);
        }
        if let Some(inp) = input {
            cmd.env("ROFI_INPUT", inp);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to start {}: {e}", self.command))?;
        if !output.status.success() {
            return Err(format!(
                "{} ({}): {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let (headers, parsed) = parse_dmenu_script_stream(&output.stdout, self.delim);
        self.delim = headers.delim;
        if let Some(markup) = headers.markup_rows {
            self.markup_rows = Some(markup);
        }
        if headers.message.is_some() {
            self.message = headers.message;
        }
        if headers.prompt.is_some() {
            self.prompt_override = headers.prompt;
        }
        if let Some(nc) = headers.no_custom {
            self.no_custom = nc;
        }
        if let Some(hk) = headers.use_hot_keys {
            self.use_hot_keys = hk;
        }
        if headers.data.is_some() {
            self.data = headers.data;
        }
        if !headers.theme_snippets.is_empty() {
            self.theme_snippets = headers.theme_snippets;
        }

        self.reload_hints = ReloadHints {
            keep_selection: headers.keep_selection,
            new_selection: headers.new_selection.and_then(|v| usize::try_from(v).ok()),
            keep_filter: headers.keep_filter,
        };

        if let Some(target) = headers.switch_mode {
            return Ok(Some(ModeStep::SwitchMode(target)));
        }

        if parsed.is_empty() {
            return Ok(None);
        }

        let mut next_entries: Vec<ListEntry> = parsed.into_iter().map(ListEntry::from).collect();
        for (lo, hi) in headers.urgent_ranges {
            for idx in lo..=hi {
                if let Some(e) = next_entries.get_mut(idx) {
                    e.urgent = true;
                }
            }
        }
        for (lo, hi) in headers.active_ranges {
            for idx in lo..=hi {
                if let Some(e) = next_entries.get_mut(idx) {
                    e.active = true;
                }
            }
        }
        self.entries = next_entries;
        Ok(Some(ModeStep::Reload))
    }
}

impl RichMode for ScriptMode {
    fn name(&self) -> &str {
        &self.mode_name
    }

    fn init(&mut self) -> Result<(), String> {
        let _ = self.execute(None, 0, None, None)?;
        Ok(())
    }

    fn entries(&self) -> Vec<ListEntry> {
        self.entries.clone()
    }

    fn message(&self) -> Option<String> {
        self.message.clone()
    }

    fn prompt_override(&self) -> Option<String> {
        self.prompt_override.clone()
    }

    fn markup_rows_override(&self) -> Option<bool> {
        self.markup_rows
    }

    fn theme_snippets(&self) -> Vec<String> {
        self.theme_snippets.clone()
    }

    fn no_custom_override(&self) -> Option<bool> {
        Some(self.no_custom)
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::Accepted {
                index,
                text,
                exit_code,
            } => {
                if *exit_code >= 10 && !self.use_hot_keys {
                    return Ok(ModeStep::Exit(*exit_code));
                }
                let (arg, info) = match self.entries.get(*index) {
                    Some(e) => (e.text.clone(), e.info.clone()),
                    None => (text.clone(), None),
                };
                let retv = if *exit_code >= 10 { *exit_code } else { 1 };
                let step = self.execute(Some(&arg), retv, info.as_deref(), Some(query))?;
                Ok(step.unwrap_or(ModeStep::Exit(0)))
            }
            UiOutcome::AcceptedAlt { index, text } => {
                let (arg, info) = match self.entries.get(*index) {
                    Some(e) => (e.text.clone(), e.info.clone()),
                    None => (text.clone(), None),
                };
                let step = self.execute(Some(&arg), 1, info.as_deref(), Some(query))?;
                Ok(step.unwrap_or(ModeStep::Exit(0)))
            }
            UiOutcome::CustomInput { text, exit_code } => {
                if self.no_custom {
                    return Ok(ModeStep::Reload);
                }
                if *exit_code >= 10 && !self.use_hot_keys {
                    return Ok(ModeStep::Exit(*exit_code));
                }
                let retv = if *exit_code >= 10 { *exit_code } else { 2 };
                let step = self.execute(Some(text), retv, None, Some(query))?;
                Ok(step.unwrap_or(ModeStep::Exit(0)))
            }
            UiOutcome::CustomInputAlt { text } => {
                if self.no_custom {
                    return Ok(ModeStep::Reload);
                }
                let step = self.execute(Some(text), 2, None, Some(query))?;
                Ok(step.unwrap_or(ModeStep::Exit(0)))
            }
            UiOutcome::DeleteEntry { index } => {
                if let Some(e) = self.entries.get(*index).cloned() {
                    let step = self.execute(Some(&e.text), 3, e.info.as_deref(), Some(query))?;
                    return Ok(step.unwrap_or(ModeStep::Exit(0)));
                }
                Ok(ModeStep::Reload)
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            UiOutcome::Back => Ok(ModeStep::Reload),
        }
    }
}
