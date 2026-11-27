//! Live Calculator & Unit/Base Converter mode (`-show calc`, inspired by `rofi-calc` / `JaKooLit/RofiCalc`).
//! Evaluates math expressions, percentages, functions, base conversions (`255 to hex`, `0xff to dec`),
//! and unit conversions (`10 km to mile`, `100 c to f`, `2048 mb to gb`) in pure Rust with zero dependencies.

use crate::active_palette::ActivePalette;
use crate::clipboard::write_text_to_clipboard;
use crate::{resolve_cache_dir, ModeStep, ReloadHints, RichMode};
use literofi_core::config::Config;
use literofi_render::state::{ListEntry, UiOutcome};
use std::path::PathBuf;

const CALC_HISTORY_FILE: &str = "rofi-calc.history";
const MAX_CALC_HISTORY: usize = 30;

/// Stateful live calculator mode.
pub struct CalcMode {
    display: String,
    cache_dir: PathBuf,
    dry_run: bool,
    palette: Option<ActivePalette>,
    history: Vec<(String, String)>,
    last_status: Option<String>,
    reload_hints: ReloadHints,
}

impl CalcMode {
    pub fn new(cfg: &Config) -> Self {
        Self {
            display: "calc".to_string(),
            cache_dir: resolve_cache_dir(cfg),
            dry_run: cfg.modes_self_test,
            palette: ActivePalette::load_from_user_config(),
            history: Vec::new(),
            last_status: None,
            reload_hints: ReloadHints::default(),
        }
    }

    fn history_path(&self) -> PathBuf {
        self.cache_dir.join(CALC_HISTORY_FILE)
    }

    fn load_history(&mut self) {
        self.history.clear();
        let Ok(raw) = std::fs::read_to_string(self.history_path()) else {
            return;
        };
        for line in raw.lines() {
            if let Some((expr, res)) = line.split_once('\t') {
                if !expr.trim().is_empty() && !res.trim().is_empty() {
                    self.history.push((expr.to_string(), res.to_string()));
                }
            }
        }
        self.history.truncate(MAX_CALC_HISTORY);
    }

    fn save_history(&self) {
        let path = self.history_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut out = String::new();
        for (expr, res) in self.history.iter().take(MAX_CALC_HISTORY) {
            out.push_str(&expr.replace(['\t', '\n', '\r'], " "));
            out.push('\t');
            out.push_str(&res.replace(['\t', '\n', '\r'], " "));
            out.push('\n');
        }
        let _ = std::fs::write(path, out);
    }

    fn record_result(&mut self, expr: String, res: String) {
        self.history.retain(|(e, _)| !e.eq_ignore_ascii_case(&expr));
        self.history.insert(0, (expr, res));
        self.history.truncate(MAX_CALC_HISTORY);
        if !self.dry_run {
            self.save_history();
        }
    }
}

impl RichMode for CalcMode {
    fn name(&self) -> &str {
        "calc"
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn init(&mut self) -> Result<(), String> {
        self.palette = ActivePalette::load_from_user_config();
        self.load_history();
        self.reload_hints = ReloadHints::default();
        Ok(())
    }

    fn prompt_override(&self) -> Option<String> {
        Some("󰪚 Calc".to_string())
    }

    fn message(&self) -> Option<String> {
        if let Some(status) = &self.last_status {
            return Some(status.clone());
        }
        Some(
            "Type math, % or units (e.g. 2500+18%, 255 to hex, 10 km to mile) · Enter: Copy"
                .to_string(),
        )
    }

    fn no_custom_override(&self) -> Option<bool> {
        Some(false)
    }

    fn theme_snippets(&self) -> Vec<String> {
        vec!["element-icon { enabled: false; }\n\
             mainbox { children: [ inputbar, message, listview ]; }\n\
             message { enabled: true; padding: 6px 8px; background-color: transparent; }\n\
             textbox { background-color: transparent; }"
            .to_string()]
    }

    fn reload_hints(&self) -> ReloadHints {
        self.reload_hints
    }

    fn live_message_evaluator(&self) -> Option<fn(&str) -> Option<String>> {
        Some(evaluate_live_calc)
    }

    fn entries(&self) -> Vec<ListEntry> {
        if self.history.is_empty() {
            return vec![
                ListEntry {
                    text: "2500 + 18%  =  2950".to_string(),
                    meta: Some("example percent math".to_string()),
                    info: Some("2950".to_string()),
                    ..ListEntry::default()
                },
                ListEntry {
                    text: "(1920 * 1080) / 16  =  129600".to_string(),
                    meta: Some("example arithmetic".to_string()),
                    info: Some("129600".to_string()),
                    ..ListEntry::default()
                },
                ListEntry {
                    text: "255 to hex  =  0xFF".to_string(),
                    meta: Some("example base conversion hex bin oct dec".to_string()),
                    info: Some("0xFF".to_string()),
                    ..ListEntry::default()
                },
                ListEntry {
                    text: "100 c to f  =  212 °F".to_string(),
                    meta: Some("example temperature unit conversion".to_string()),
                    info: Some("212".to_string()),
                    ..ListEntry::default()
                },
                ListEntry {
                    text: "2048 mb to gb  =  2 GB".to_string(),
                    meta: Some("example data size unit conversion".to_string()),
                    info: Some("2".to_string()),
                    ..ListEntry::default()
                },
            ];
        }
        self.history
            .iter()
            .map(|(expr, res)| ListEntry {
                text: format!("{expr}  =  {res}"),
                meta: Some(format!("{expr} {res}")),
                info: Some(res.clone()),
                ..ListEntry::default()
            })
            .collect()
    }

    fn handle_outcome(&mut self, outcome: &UiOutcome, query: &str) -> Result<ModeStep, String> {
        match outcome {
            UiOutcome::CustomInput { text, .. } | UiOutcome::CustomInputAlt { text } => {
                let expr = if text.trim().is_empty() {
                    query.trim()
                } else {
                    text.trim()
                };
                let last_ans = self
                    .history
                    .first()
                    .and_then(|(_, r)| r.split_whitespace().next())
                    .and_then(|s| s.parse::<f64>().ok());
                if let Some(res) = evaluate_expression(expr, last_ans) {
                    let copy_val = res.split_whitespace().next().unwrap_or(&res).to_string();
                    if !self.dry_run {
                        let _ = write_text_to_clipboard(&copy_val);
                    }
                    self.record_result(expr.to_string(), res.clone());
                    self.last_status = Some(format!("= {res}   (copied to clipboard)"));
                    self.reload_hints = ReloadHints {
                        keep_selection: false,
                        new_selection: Some(0),
                        keep_filter: false,
                    };
                    if matches!(outcome, UiOutcome::CustomInputAlt { .. }) {
                        return Ok(ModeStep::Exit(0));
                    }
                    Ok(ModeStep::Reload)
                } else {
                    self.last_status = Some(format!("Cannot evaluate: {expr}"));
                    self.reload_hints = ReloadHints {
                        keep_selection: true,
                        new_selection: None,
                        keep_filter: true,
                    };
                    Ok(ModeStep::Reload)
                }
            }
            UiOutcome::Accepted { index, .. } | UiOutcome::AcceptedAlt { index, .. } => {
                // If the user typed a fresh valid expression that wasn't a history item, evaluate it first!
                let trimmed = query.trim();
                let last_ans = self
                    .history
                    .first()
                    .and_then(|(_, r)| r.split_whitespace().next())
                    .and_then(|s| s.parse::<f64>().ok());
                if !trimmed.is_empty() {
                    if let Some(res) = evaluate_expression(trimmed, last_ans) {
                        let copy_val = res.split_whitespace().next().unwrap_or(&res).to_string();
                        if !self.dry_run {
                            let _ = write_text_to_clipboard(&copy_val);
                        }
                        self.record_result(trimmed.to_string(), res.clone());
                        self.last_status = Some(format!("= {res}   (copied to clipboard)"));
                        self.reload_hints = ReloadHints {
                            keep_selection: false,
                            new_selection: Some(0),
                            keep_filter: false,
                        };
                        return Ok(ModeStep::Reload);
                    }
                }
                if let Some((_, res)) = self.history.get(*index).cloned() {
                    let copy_val = res.split_whitespace().next().unwrap_or(&res).to_string();
                    if !self.dry_run {
                        let _ = write_text_to_clipboard(&copy_val);
                    }
                    return Ok(ModeStep::Exit(0));
                }
                Ok(ModeStep::Exit(0))
            }
            UiOutcome::DeleteEntry { index } => {
                if *index < self.history.len() {
                    self.history.remove(*index);
                    if !self.dry_run {
                        self.save_history();
                    }
                }
                self.reload_hints = ReloadHints {
                    keep_selection: true,
                    new_selection: None,
                    keep_filter: false,
                };
                Ok(ModeStep::Reload)
            }
            UiOutcome::Cancelled => Ok(ModeStep::Exit(1)),
            UiOutcome::ModeSwitch(idx) => Ok(ModeStep::SwitchModeIndex(*idx)),
            UiOutcome::Back => Ok(ModeStep::Reload),
        }
    }
}

/// Live preview helper evaluated on every keystroke in `calc` mode.
pub fn evaluate_live_calc(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let res = evaluate_expression(trimmed, None)?;
    Some(format!("= {res}   · Enter: Copy & Save"))
}

/// Evaluates math expressions, percentages, base conversions, and unit conversions.
pub fn evaluate_expression(raw: &str, ans: Option<f64>) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }

    // 1. Check `<expr> to <target>` / `<expr> in <target>` conversions
    if let Some(conv) = try_conversion(s, ans) {
        return Some(conv);
    }

    // 2. Check `<pct>% of <expr>`
    let lower = s.to_ascii_lowercase();
    if let Some(pos) = lower.find("% of ") {
        let lhs = parse_and_eval(&s[..pos], ans)?;
        let rhs = parse_and_eval(&s[pos + 5..], ans)?;
        return Some(format_number((lhs / 100.0) * rhs));
    }

    // 3. Standard math expression (with trailing `+ X%` / `- X%` support)
    let val = eval_with_percent_sugar(s, ans)?;
    if !val.is_finite() {
        return None;
    }
    Some(format_number(val))
}

fn eval_with_percent_sugar(s: &str, ans: Option<f64>) -> Option<f64> {
    let trimmed = s.trim();
    if let Some(without_pct) = trimmed.strip_suffix('%') {
        // Match `<base> + <pct>%` or `<base> - <pct>%`
        if let Some(idx) = without_pct.rfind(['+', '-']) {
            if idx > 0 {
                let op = without_pct.as_bytes()[idx] as char;
                let base_str = without_pct[..idx].trim();
                let pct_str = without_pct[idx + 1..].trim();
                if let (Some(base), Some(pct)) =
                    (parse_and_eval(base_str, ans), parse_and_eval(pct_str, ans))
                {
                    let delta = base * (pct / 100.0);
                    return Some(if op == '+' {
                        base + delta
                    } else {
                        base - delta
                    });
                }
            }
        }
    }
    parse_and_eval(trimmed, ans)
}

fn try_conversion(s: &str, ans: Option<f64>) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let (lhs_raw, rhs_raw) = if let Some(pos) = lower.rfind(" to ") {
        (&s[..pos], s[pos + 4..].trim())
    } else if let Some(pos) = lower.rfind(" in ") {
        (&s[..pos], s[pos + 4..].trim())
    } else {
        return None;
    };
    let target = rhs_raw.to_ascii_lowercase();

    // Base conversions: `255 to hex`, `0xff to dec`, `42 to bin`, `64 to oct`
    if matches!(
        target.as_str(),
        "hex" | "hexadecimal" | "bin" | "binary" | "oct" | "octal" | "dec" | "decimal"
    ) {
        let val = parse_int_or_math(lhs_raw.trim(), ans)?;
        return Some(match target.as_str() {
            "hex" | "hexadecimal" => format!("0x{:X}", val),
            "bin" | "binary" => format!("0b{:b}", val),
            "oct" | "octal" => format!("0o{:o}", val),
            _ => format!("{val}"),
        });
    }

    // Unit conversions: `<value> <from_unit> to <to_unit>`
    let (num_str, from_unit) = split_value_and_unit(lhs_raw.trim())?;
    let val = parse_and_eval(num_str, ans)?;
    let from = from_unit.to_ascii_lowercase();
    convert_units(val, &from, &target)
}

fn parse_int_or_math(s: &str, ans: Option<f64>) -> Option<i128> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i128::from_str_radix(&hex.replace('_', ""), 16).ok();
    }
    if let Some(bin) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return i128::from_str_radix(&bin.replace('_', ""), 2).ok();
    }
    if let Some(oct) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return i128::from_str_radix(&oct.replace('_', ""), 8).ok();
    }
    let f = parse_and_eval(t, ans)?;
    if f.is_finite() {
        Some(f.round() as i128)
    } else {
        None
    }
}

fn split_value_and_unit(s: &str) -> Option<(&str, &str)> {
    let idx = s.rfind(|c: char| c.is_ascii_digit() || c == ')' || c == '.')?;
    let num = s[..=idx].trim();
    let unit = s[idx + 1..].trim();
    if num.is_empty() || unit.is_empty() {
        return None;
    }
    Some((num, unit))
}

fn convert_units(val: f64, from: &str, to: &str) -> Option<String> {
    // Temperature
    let temp_unit = |u: &str| match u {
        "c" | "°c" | "celsius" => Some('c'),
        "f" | "°f" | "fahrenheit" => Some('f'),
        "k" | "kelvin" => Some('k'),
        _ => None,
    };
    if let (Some(fc), Some(tc)) = (temp_unit(from), temp_unit(to)) {
        let c = match fc {
            'c' => val,
            'f' => (val - 32.0) * 5.0 / 9.0,
            'k' => val - 273.15,
            _ => unreachable!(),
        };
        let (out, label) = match tc {
            'c' => (c, "°C"),
            'f' => (c * 9.0 / 5.0 + 32.0, "°F"),
            'k' => (c + 273.15, "K"),
            _ => unreachable!(),
        };
        return Some(format!("{} {label}", format_number(out)));
    }

    // Length (meters)
    let len_factor = |u: &str| -> Option<(f64, &'static str)> {
        match u {
            "mm" => Some((0.001, "mm")),
            "cm" => Some((0.01, "cm")),
            "m" | "meter" | "meters" => Some((1.0, "m")),
            "km" | "kilometer" | "kilometers" => Some((1000.0, "km")),
            "in" | "inch" | "inches" => Some((0.0254, "in")),
            "ft" | "foot" | "feet" => Some((0.3048, "ft")),
            "yd" | "yard" | "yards" => Some((0.9144, "yd")),
            "mi" | "mile" | "miles" => Some((1609.344, "mi")),
            _ => None,
        }
    };
    if let (Some((f, _)), Some((t, label))) = (len_factor(from), len_factor(to)) {
        return Some(format!("{} {label}", format_number(val * f / t)));
    }

    // Mass (grams)
    let mass_factor = |u: &str| -> Option<(f64, &'static str)> {
        match u {
            "mg" => Some((0.001, "mg")),
            "g" | "gram" | "grams" => Some((1.0, "g")),
            "kg" | "kilogram" | "kilograms" => Some((1000.0, "kg")),
            "oz" | "ounce" | "ounces" => Some((28.349_523_125, "oz")),
            "lb" | "lbs" | "pound" | "pounds" => Some((453.592_37, "lb")),
            _ => None,
        }
    };
    if let (Some((f, _)), Some((t, label))) = (mass_factor(from), mass_factor(to)) {
        return Some(format!("{} {label}", format_number(val * f / t)));
    }

    // Data size (bytes, binary 1024)
    let data_factor = |u: &str| -> Option<(f64, &'static str)> {
        match u {
            "b" | "byte" | "bytes" => Some((1.0, "B")),
            "kb" | "kib" => Some((1024.0, "KB")),
            "mb" | "mib" => Some((1024.0 * 1024.0, "MB")),
            "gb" | "gib" => Some((1024.0 * 1024.0 * 1024.0, "GB")),
            "tb" | "tib" => Some((1024.0 * 1024.0 * 1024.0 * 1024.0, "TB")),
            _ => None,
        }
    };
    if let (Some((f, _)), Some((t, label))) = (data_factor(from), data_factor(to)) {
        return Some(format!("{} {label}", format_number(val * f / t)));
    }

    // Time (seconds)
    let time_factor = |u: &str| -> Option<(f64, &'static str)> {
        match u {
            "ms" => Some((0.001, "ms")),
            "s" | "sec" | "secs" | "second" | "seconds" => Some((1.0, "s")),
            "min" | "mins" | "minute" | "minutes" => Some((60.0, "min")),
            "h" | "hr" | "hrs" | "hour" | "hours" => Some((3600.0, "h")),
            "d" | "day" | "days" => Some((86_400.0, "d")),
            _ => None,
        }
    };
    if let (Some((f, _)), Some((t, label))) = (time_factor(from), time_factor(to)) {
        return Some(format!("{} {label}", format_number(val * f / t)));
    }

    None
}

fn format_number(v: f64) -> String {
    if (v - v.round()).abs() < 1e-10 && v.abs() < 1e15 {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{v:.8}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    LParen,
    RParen,
    Comma,
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == '0' && i + 1 < bytes.len() {
            let next = (bytes[i + 1] as char).to_ascii_lowercase();
            if next == 'x' || next == 'b' || next == 'o' {
                let radix = match next {
                    'x' => 16,
                    'b' => 2,
                    _ => 8,
                };
                i += 2;
                let start = i;
                while i < bytes.len() && ((bytes[i] as char).is_digit(radix) || bytes[i] == b'_') {
                    i += 1;
                }
                let digits = s[start..i].replace('_', "");
                let v = i128::from_str_radix(&digits, radix).ok()?;
                out.push(Tok::Num(v as f64));
                continue;
            }
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < bytes.len() && ((bytes[i] as char).is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let save = i;
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                let exp_start = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
                if i == exp_start {
                    i = save;
                }
            }
            let num: f64 = s[start..i].parse().ok()?;
            out.push(Tok::Num(num));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len()
                && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            out.push(Tok::Ident(s[start..i].to_ascii_lowercase()));
            continue;
        }
        match c {
            '+' => out.push(Tok::Plus),
            '-' => out.push(Tok::Minus),
            '*' | '×' => out.push(Tok::Star),
            '/' | '÷' => out.push(Tok::Slash),
            '%' => out.push(Tok::Percent),
            '^' => out.push(Tok::Caret),
            '(' => out.push(Tok::LParen),
            ')' => out.push(Tok::RParen),
            ',' => out.push(Tok::Comma),
            _ => return None,
        }
        i += 1;
    }
    Some(out)
}

struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
    ans: Option<f64>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos)?.clone();
        self.pos += 1;
        Some(t)
    }

    fn parse_expr(&mut self) -> Option<f64> {
        let mut lhs = self.parse_term()?;
        while let Some(t) = self.peek() {
            match t {
                Tok::Plus => {
                    self.next();
                    lhs += self.parse_term()?;
                }
                Tok::Minus => {
                    self.next();
                    lhs -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Some(lhs)
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut lhs = self.parse_power()?;
        while let Some(t) = self.peek() {
            match t {
                Tok::Star => {
                    self.next();
                    lhs *= self.parse_power()?;
                }
                Tok::Slash => {
                    self.next();
                    let rhs = self.parse_power()?;
                    if rhs == 0.0 {
                        return None;
                    }
                    lhs /= rhs;
                }
                Tok::Percent => {
                    self.next();
                    let rhs = self.parse_power()?;
                    if rhs == 0.0 {
                        return None;
                    }
                    lhs %= rhs;
                }
                _ => break,
            }
        }
        Some(lhs)
    }

    fn parse_power(&mut self) -> Option<f64> {
        let base = self.parse_unary()?;
        if matches!(self.peek(), Some(Tok::Caret)) {
            self.next();
            let exp = self.parse_power()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn parse_unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(Tok::Plus) => {
                self.next();
                self.parse_unary()
            }
            Some(Tok::Minus) => {
                self.next();
                Some(-self.parse_unary()?)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        match self.next()? {
            Tok::Num(n) => Some(n),
            Tok::LParen => {
                let v = self.parse_expr()?;
                if !matches!(self.next(), Some(Tok::RParen)) {
                    return None;
                }
                Some(v)
            }
            Tok::Ident(name) => {
                if matches!(self.peek(), Some(Tok::LParen)) {
                    self.next();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        args.push(self.parse_expr()?);
                        while matches!(self.peek(), Some(Tok::Comma)) {
                            self.next();
                            args.push(self.parse_expr()?);
                        }
                    }
                    if !matches!(self.next(), Some(Tok::RParen)) {
                        return None;
                    }
                    return eval_func(&name, &args);
                }
                match name.as_str() {
                    "pi" => Some(std::f64::consts::PI),
                    "e" => Some(std::f64::consts::E),
                    "tau" => Some(std::f64::consts::TAU),
                    "ans" => self.ans,
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

fn eval_func(name: &str, args: &[f64]) -> Option<f64> {
    match (name, args) {
        ("sqrt", [x]) => Some(x.sqrt()),
        ("abs", [x]) => Some(x.abs()),
        ("sin", [x]) => Some(x.sin()),
        ("cos", [x]) => Some(x.cos()),
        ("tan", [x]) => Some(x.tan()),
        ("ln", [x]) => Some(x.ln()),
        ("log" | "log10", [x]) => Some(x.log10()),
        ("log2", [x]) => Some(x.log2()),
        ("round", [x]) => Some(x.round()),
        ("floor", [x]) => Some(x.floor()),
        ("ceil", [x]) => Some(x.ceil()),
        ("pow", [a, b]) => Some(a.powf(*b)),
        ("min", [a, b]) => Some(a.min(*b)),
        ("max", [a, b]) => Some(a.max(*b)),
        _ => None,
    }
}

fn parse_and_eval(s: &str, ans: Option<f64>) -> Option<f64> {
    let toks = tokenize(s)?;
    if toks.is_empty() {
        return None;
    }
    let mut p = Parser {
        toks: &toks,
        pos: 0,
        ans,
    };
    let val = p.parse_expr()?;
    if p.pos != toks.len() {
        return None;
    }
    Some(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_evaluates_math_percent_base_and_units() {
        assert_eq!(
            evaluate_expression("(1920*1080)/16", None).as_deref(),
            Some("129600")
        );
        assert_eq!(
            evaluate_expression("2500 + 18%", None).as_deref(),
            Some("2950")
        );
        assert_eq!(
            evaluate_expression("20% of 500", None).as_deref(),
            Some("100")
        );
        assert_eq!(
            evaluate_expression("sqrt(144) + 2^3", None).as_deref(),
            Some("20")
        );
        assert_eq!(
            evaluate_expression("255 to hex", None).as_deref(),
            Some("0xFF")
        );
        assert_eq!(
            evaluate_expression("0xff to dec", None).as_deref(),
            Some("255")
        );
        assert_eq!(
            evaluate_expression("100 c to f", None).as_deref(),
            Some("212 °F")
        );
        assert_eq!(
            evaluate_expression("2048 mb to gb", None).as_deref(),
            Some("2 GB")
        );
    }
}
