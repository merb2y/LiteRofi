//! Interactive launcher state machine: filtering, sorting, UTF-8 editing,
//! list/grid navigation, highlighting, and keybinding dispatch.

use literofi_core::config::{Config, MatchingMethod, SortingMethod};
use literofi_core::keyb::KeyBindingAction;
use literofi_core::matcher::{
    fuzzy_score_fzf_v2, fuzzy_score_rofi, fzf_v2_sort_key, levenshtein, parse_case_sensitivity,
    token_match, token_match_ranges, tokenize, MatchOptions, TokenMatcher,
};
use literofi_core::mode::custom_exit_code;
use literofi_core::protocol::DmenuScriptEntry;

/// Display entry stored in [`LauncherState`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListEntry {
    pub text: String,
    pub display: Option<String>,
    pub meta: Option<String>,
    pub info: Option<String>,
    pub icon: Option<String>,
    pub mode_tag: Option<String>,
    pub urgent: bool,
    pub active: bool,
    pub nonselectable: bool,
    pub permanent: bool,
}

impl From<DmenuScriptEntry> for ListEntry {
    fn from(d: DmenuScriptEntry) -> Self {
        Self {
            text: d.entry,
            display: d.display,
            meta: d.meta,
            info: d.info,
            icon: d.icons.into_iter().next(),
            mode_tag: None,
            urgent: d.urgent,
            active: d.active,
            nonselectable: d.nonselectable,
            permanent: d.permanent,
        }
    }
}

impl ListEntry {
    pub fn visible_label(&self) -> &str {
        self.display.as_deref().unwrap_or(&self.text)
    }
}

/// Terminal action returned by [`LauncherState::handle_action`] or mouse activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiOutcome {
    Accepted {
        index: usize,
        text: String,
        exit_code: i32,
    },
    AcceptedAlt {
        index: usize,
        text: String,
    },
    CustomInput {
        text: String,
        exit_code: i32,
    },
    CustomInputAlt {
        text: String,
    },
    DeleteEntry {
        index: usize,
    },
    Cancelled,
    ModeSwitch(usize),
}

/// Complete mutable interactive state for one Rofi window session.
#[derive(Debug, Clone)]
pub struct LauncherState {
    pub entries: Vec<ListEntry>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub scroll_offset: usize,
    pub hovered: Option<usize>,
    pub query: String,
    pub cursor_byte: usize,
    pub prompt: String,
    pub message: Option<String>,
    pub overlay: Option<String>,
    pub modes: Vec<String>,
    pub active_mode: usize,
    pub case_sensitive: bool,
    pub case_smart: bool,
    pub tokenize_enabled: bool,
    pub normalize_match: bool,
    pub negate_char: char,
    pub sort_enabled: bool,
    pub cycle: bool,
    pub matching: MatchingMethod,
    pub sorting: SortingMethod,
    pub password: bool,
    pub no_custom: bool,
    pub only_match: bool,
    pub click_to_exit: bool,
    pub auto_select: bool,
    compiled_tokens: Vec<TokenMatcher>,
}

impl LauncherState {
    pub fn new(cfg: &Config, entries: Vec<ListEntry>) -> Self {
        let modes: Vec<String> = cfg
            .modes
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let active_mode = cfg
            .show
            .as_deref()
            .and_then(|s| modes.iter().position(|m| m.eq_ignore_ascii_case(s)))
            .unwrap_or(0);

        let initial_prompt = cfg
            .prompt
            .clone()
            .or_else(|| {
                if cfg.dmenu {
                    Some("dmenu".to_string())
                } else {
                    modes.get(active_mode).cloned()
                }
            })
            .unwrap_or_else(|| "run".to_string());

        let initial_query = cfg.filter.clone().unwrap_or_default();
        let cursor_byte = initial_query.len();

        let mut st = Self {
            entries,
            filtered: Vec::new(),
            selected: cfg.selected_row.unwrap_or(0),
            scroll_offset: 0,
            hovered: None,
            query: initial_query,
            cursor_byte,
            prompt: initial_prompt,
            message: cfg.mesg.clone(),
            overlay: None,
            modes,
            active_mode,
            case_sensitive: cfg.case_sensitive,
            case_smart: cfg.case_smart,
            tokenize_enabled: cfg.tokenize,
            normalize_match: cfg.normalize_match,
            negate_char: cfg.matching_negate_char,
            sort_enabled: cfg.sort,
            cycle: cfg.cycle,
            matching: cfg.matching,
            sorting: cfg.sorting_method,
            password: cfg.password,
            no_custom: cfg.no_custom,
            only_match: cfg.only_match,
            click_to_exit: cfg.click_to_exit,
            auto_select: cfg.auto_select,
            compiled_tokens: Vec::new(),
        };
        st.refilter();
        if let Some(target) = cfg.select.as_deref() {
            let tok = tokenize(
                target,
                &MatchOptions {
                    method: MatchingMethod::Normal,
                    case_sensitive: parse_case_sensitivity(
                        target,
                        st.case_sensitive,
                        st.case_smart,
                    ),
                    tokenize: st.tokenize_enabled,
                    negate_char: st.negate_char,
                    normalize_match: st.normalize_match,
                },
            );
            if let Some(pos) = st.filtered.iter().position(|&idx| {
                token_match(&tok, st.entries[idx].visible_label(), st.normalize_match)
            }) {
                st.selected = pos;
            }
        }
        st
    }

    pub fn effective_case_sensitive(&self) -> bool {
        parse_case_sensitivity(&self.query, self.case_sensitive, self.case_smart)
    }

    /// Recomputes `filtered`, sorting, and clamps `selected` onto a selectable entry.
    pub fn refilter(&mut self) {
        let has_tags = self.entries.iter().any(|e| e.mode_tag.is_some());
        let (mode_filter, effective_query) = split_bang_query(&self.query, has_tags);
        let cs = parse_case_sensitivity(effective_query, self.case_sensitive, self.case_smart);
        let opts = MatchOptions {
            method: self.matching,
            case_sensitive: cs,
            tokenize: self.tokenize_enabled,
            negate_char: self.negate_char,
            normalize_match: self.normalize_match,
        };
        let compiled_tokens = tokenize(effective_query, &opts);

        let mut hits: Vec<(u8, i32, usize)> = Vec::new();
        for (idx, item) in self.entries.iter().enumerate() {
            if let Some(prefix) = mode_filter {
                let tag_matches = item.mode_tag.as_deref().is_some_and(|tag| {
                    tag.to_ascii_lowercase()
                        .starts_with(&prefix.to_ascii_lowercase())
                });
                if !tag_matches {
                    continue;
                }
            }
            let label = item.visible_label();
            let label_matched = compiled_tokens.is_empty()
                || token_match(&compiled_tokens, label, self.normalize_match);
            let meta_matched = !label_matched
                && !compiled_tokens.is_empty()
                && item
                    .meta
                    .as_deref()
                    .is_some_and(|m| token_match(&compiled_tokens, m, self.normalize_match));
            if !label_matched && !meta_matched && !item.permanent {
                continue;
            }
            let tier = if effective_query.is_empty() {
                0
            } else {
                label_match_tier(effective_query, label, cs, label_matched, meta_matched)
            };
            let key = if !self.sort_enabled || effective_query.is_empty() {
                0
            } else {
                match self.sorting {
                    SortingMethod::Normal => levenshtein(effective_query, label, cs) as i32,
                    SortingMethod::Fzf => fuzzy_score_rofi(effective_query, label, cs),
                    SortingMethod::FzfV2 => {
                        let sc =
                            fuzzy_score_fzf_v2(effective_query, label, cs, self.normalize_match);
                        fzf_v2_sort_key(sc, label)
                    }
                }
            };
            hits.push((tier, key, idx));
        }

        if !effective_query.is_empty() {
            hits.sort_by_key(|&(tier, key, idx)| (tier, key, idx));
        }

        self.compiled_tokens = compiled_tokens;
        self.filtered = hits.into_iter().map(|(_, _, idx)| idx).collect();
        self.selected = self.first_selectable().unwrap_or(0);
        self.scroll_offset = 0;
    }

    /// Replaces entry list during an active session (for `script`, `dmenu -multi-select`, or delete).
    pub fn replace_entries(
        &mut self,
        entries: Vec<ListEntry>,
        keep_selection: bool,
        new_selection: Option<usize>,
        keep_filter: bool,
    ) {
        let prev_sel = self.selected;
        self.entries = entries;
        if !keep_filter {
            self.query.clear();
            self.cursor_byte = 0;
        }
        self.refilter();
        if let Some(target) = new_selection {
            if target < self.filtered.len() {
                self.selected = target;
            }
        } else if keep_selection && prev_sel < self.filtered.len() {
            self.selected = prev_sel;
        }
    }

    /// Returns byte highlight spans for `text` matching current search tokens.
    pub fn match_highlight_ranges(&self, text: &str) -> Vec<(usize, usize)> {
        token_match_ranges(&self.compiled_tokens, text, self.normalize_match)
    }

    /// Ensures `scroll_offset` keeps `selected` inside `[scroll_offset, scroll_offset + page_size)`.
    pub fn ensure_visible(&mut self, page_size: usize) {
        if page_size == 0 || self.filtered.is_empty() {
            self.scroll_offset = 0;
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + page_size {
            self.scroll_offset = self.selected + 1 - page_size;
        }
    }

    pub fn display_query(&self) -> String {
        if self.password {
            "*".repeat(self.query.chars().count())
        } else {
            self.query.clone()
        }
    }
}

fn label_match_tier(
    query: &str,
    label: &str,
    case_sensitive: bool,
    label_matched: bool,
    meta_matched: bool,
) -> u8 {
    if !label_matched && !meta_matched {
        return 4;
    }
    if !label_matched {
        return 3;
    }
    let q = query.trim();
    let l = label.trim();
    if case_sensitive {
        if l == q {
            0
        } else if l.starts_with(q) {
            1
        } else {
            2
        }
    } else {
        let ql = q.to_lowercase();
        let ll = l.to_lowercase();
        if ll == ql {
            0
        } else if ll.starts_with(&ql) {
            1
        } else {
            2
        }
    }
}

impl LauncherState {
    pub fn case_indicator_str(&self) -> &'static str {
        let matching_ch = match self.matching {
            MatchingMethod::Normal => "-",
            MatchingMethod::Regex => "R",
            MatchingMethod::Glob => "G",
            MatchingMethod::Fuzzy => "F",
            MatchingMethod::Prefix => "P",
        };
        if self.effective_case_sensitive() {
            matching_ch
        } else if self.sort_enabled {
            match self.sorting {
                SortingMethod::FzfV2 | SortingMethod::Fzf => "~",
                SortingMethod::Normal => "+",
            }
        } else {
            " "
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let pos = self.cursor_byte.min(self.query.len());
        self.query.insert(pos, ch);
        self.cursor_byte = pos + ch.len_utf8();
        self.refilter();
    }

    pub fn insert_str(&mut self, s: &str) {
        let filtered: String = s.chars().filter(|c| !c.is_control()).collect();
        if filtered.is_empty() {
            return;
        }
        let pos = self.cursor_byte.min(self.query.len());
        self.query.insert_str(pos, &filtered);
        self.cursor_byte = pos + filtered.len();
        self.refilter();
    }

    pub fn backspace(&mut self) {
        let pos = self.cursor_byte.min(self.query.len());
        if pos == 0 {
            return;
        }
        let prev = prev_char_boundary(&self.query, pos);
        self.query.replace_range(prev..pos, "");
        self.cursor_byte = prev;
        self.refilter();
    }

    pub fn delete_forward(&mut self) {
        let pos = self.cursor_byte.min(self.query.len());
        if pos >= self.query.len() {
            return;
        }
        let next = next_char_boundary(&self.query, pos);
        self.query.replace_range(pos..next, "");
        self.refilter();
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor_byte = prev_char_boundary(&self.query, self.cursor_byte.min(self.query.len()));
    }

    pub fn move_cursor_right(&mut self) {
        self.cursor_byte = next_char_boundary(&self.query, self.cursor_byte.min(self.query.len()));
    }

    pub fn move_word_left(&mut self) {
        self.cursor_byte = prev_word_boundary(&self.query, self.cursor_byte.min(self.query.len()));
    }

    pub fn move_word_right(&mut self) {
        self.cursor_byte = next_word_boundary(&self.query, self.cursor_byte.min(self.query.len()));
    }

    pub fn move_home(&mut self) {
        self.cursor_byte = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_byte = self.query.len();
    }

    pub fn remove_to_sol(&mut self) {
        let pos = self.cursor_byte.min(self.query.len());
        if pos > 0 {
            self.query.replace_range(0..pos, "");
            self.cursor_byte = 0;
            self.refilter();
        }
    }

    pub fn remove_to_eol(&mut self) {
        let pos = self.cursor_byte.min(self.query.len());
        if pos < self.query.len() {
            self.query.truncate(pos);
            self.refilter();
        }
    }

    pub fn remove_word_back(&mut self) {
        let pos = self.cursor_byte.min(self.query.len());
        let prev = prev_word_boundary(&self.query, pos);
        if prev < pos {
            self.query.replace_range(prev..pos, "");
            self.cursor_byte = prev;
            self.refilter();
        }
    }

    pub fn clear_line(&mut self) {
        if !self.query.is_empty() {
            self.query.clear();
            self.cursor_byte = 0;
            self.refilter();
        }
    }

    fn first_selectable(&self) -> Option<usize> {
        (0..self.filtered.len()).find(|&p| !self.entries[self.filtered[p]].nonselectable)
    }

    fn last_selectable(&self) -> Option<usize> {
        (0..self.filtered.len())
            .rev()
            .find(|&p| !self.entries[self.filtered[p]].nonselectable)
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.filtered.len();
        if len == 0 {
            return;
        }
        let step = if delta >= 0 { 1isize } else { -1isize };
        let steps = delta.unsigned_abs().max(1);
        let mut pos = self.selected.min(len - 1);

        // First move by `steps`, clamping or wrapping on cycle
        let target = pos as isize + delta;
        pos = if self.cycle {
            target.rem_euclid(len as isize) as usize
        } else {
            target.clamp(0, (len - 1) as isize) as usize
        };

        // Advance past nonselectable rows up to `len` probes
        for _ in 0..len {
            if !self.entries[self.filtered[pos]].nonselectable {
                self.selected = pos;
                return;
            }
            let next = pos as isize + step;
            pos = if self.cycle {
                next.rem_euclid(len as isize) as usize
            } else if next < 0 || next >= len as isize {
                return;
            } else {
                next as usize
            };
        }
        let _ = steps;
    }

    /// Dispatches a resolved [`KeyBindingAction`], updating state or yielding [`UiOutcome`].
    pub fn handle_action(
        &mut self,
        action: KeyBindingAction,
        columns: usize,
        lines: usize,
    ) -> Option<UiOutcome> {
        let cols = columns.max(1) as isize;
        let page = (columns.max(1) * lines.max(1)) as isize;
        match action {
            KeyBindingAction::ClearLine => self.clear_line(),
            KeyBindingAction::MoveFront => self.move_home(),
            KeyBindingAction::MoveEnd => self.move_end(),
            KeyBindingAction::MoveWordBack => self.move_word_left(),
            KeyBindingAction::MoveWordForward => self.move_word_right(),
            KeyBindingAction::MoveCharBack => self.move_cursor_left(),
            KeyBindingAction::MoveCharForward => self.move_cursor_right(),
            KeyBindingAction::RemoveWordBack => self.remove_word_back(),
            KeyBindingAction::RemoveCharForward => self.delete_forward(),
            KeyBindingAction::RemoveCharBack => self.backspace(),
            KeyBindingAction::RemoveToEol => self.remove_to_eol(),
            KeyBindingAction::RemoveToSol => self.remove_to_sol(),

            KeyBindingAction::RowUp | KeyBindingAction::ElementPrev => {
                self.move_selection(-1);
                self.ensure_visible(page as usize);
            }
            KeyBindingAction::RowDown | KeyBindingAction::ElementNext => {
                self.move_selection(1);
                self.ensure_visible(page as usize);
            }
            KeyBindingAction::RowLeft => {
                self.move_selection(-cols);
                self.ensure_visible(page as usize);
            }
            KeyBindingAction::RowRight => {
                self.move_selection(cols);
                self.ensure_visible(page as usize);
            }
            KeyBindingAction::PagePrev => {
                self.move_selection(-page);
                self.ensure_visible(page as usize);
            }
            KeyBindingAction::PageNext => {
                self.move_selection(page);
                self.ensure_visible(page as usize);
            }
            KeyBindingAction::RowFirst => {
                if let Some(p) = self.first_selectable() {
                    self.selected = p;
                    self.ensure_visible(page as usize);
                }
            }
            KeyBindingAction::RowLast => {
                if let Some(p) = self.last_selectable() {
                    self.selected = p;
                    self.ensure_visible(page as usize);
                }
            }
            KeyBindingAction::RowSelect => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    if !self.entries[idx].nonselectable {
                        self.query = self.entries[idx].text.clone();
                        self.cursor_byte = self.query.len();
                        self.refilter();
                    }
                }
            }
            KeyBindingAction::ToggleCaseSensitivity => {
                self.case_sensitive = !self.case_sensitive;
                self.refilter();
            }
            KeyBindingAction::ToggleSort => {
                self.sort_enabled = !self.sort_enabled;
                self.refilter();
            }
            KeyBindingAction::SelectElement(n) => {
                let target = self.scroll_offset + usize::from(n.saturating_sub(1));
                if let Some(&idx) = self.filtered.get(target) {
                    if !self.entries[idx].nonselectable {
                        return Some(UiOutcome::Accepted {
                            index: idx,
                            text: self.entries[idx].text.clone(),
                            exit_code: 0,
                        });
                    }
                }
            }
            KeyBindingAction::AcceptEntry => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    if !self.entries[idx].nonselectable {
                        return Some(UiOutcome::Accepted {
                            index: idx,
                            text: self.entries[idx].text.clone(),
                            exit_code: 0,
                        });
                    }
                    return None;
                }
                if !self.no_custom && !self.only_match && !self.query.is_empty() {
                    return Some(UiOutcome::CustomInput {
                        text: self.query.clone(),
                        exit_code: 0,
                    });
                }
            }
            KeyBindingAction::AcceptAlt => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    if !self.entries[idx].nonselectable {
                        return Some(UiOutcome::AcceptedAlt {
                            index: idx,
                            text: self.entries[idx].text.clone(),
                        });
                    }
                    return None;
                }
                if !self.no_custom && !self.only_match && !self.query.is_empty() {
                    return Some(UiOutcome::CustomInputAlt {
                        text: self.query.clone(),
                    });
                }
            }
            KeyBindingAction::AcceptCustom => {
                if !self.no_custom && !self.only_match {
                    return Some(UiOutcome::CustomInput {
                        text: self.query.clone(),
                        exit_code: 0,
                    });
                }
            }
            KeyBindingAction::AcceptCustomAlt => {
                if !self.no_custom && !self.only_match {
                    return Some(UiOutcome::CustomInputAlt {
                        text: self.query.clone(),
                    });
                }
            }
            KeyBindingAction::DeleteEntry => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    if !self.entries[idx].nonselectable {
                        return Some(UiOutcome::DeleteEntry { index: idx });
                    }
                }
            }
            KeyBindingAction::Custom(n) => {
                let code = custom_exit_code(n);
                if let Some(&idx) = self.filtered.get(self.selected) {
                    if !self.entries[idx].nonselectable {
                        return Some(UiOutcome::Accepted {
                            index: idx,
                            text: self.entries[idx].text.clone(),
                            exit_code: code,
                        });
                    }
                    return None;
                }
                if !self.no_custom && !self.only_match {
                    return Some(UiOutcome::CustomInput {
                        text: self.query.clone(),
                        exit_code: code,
                    });
                }
            }
            KeyBindingAction::Cancel => return Some(UiOutcome::Cancelled),
            KeyBindingAction::ModeNext if !self.modes.is_empty() => {
                let next = (self.active_mode + 1) % self.modes.len();
                return Some(UiOutcome::ModeSwitch(next));
            }
            KeyBindingAction::ModePrevious if !self.modes.is_empty() => {
                let prev = (self.active_mode + self.modes.len() - 1) % self.modes.len();
                return Some(UiOutcome::ModeSwitch(prev));
            }
            _ => {}
        }
        None
    }
}

fn split_bang_query(query: &str, has_tags: bool) -> (Option<&str>, &str) {
    if has_tags {
        if let Some(after) = query.strip_prefix('!') {
            let (prefix, rest) = match after.split_once(' ') {
                Some((p, r)) => (p, r),
                None => (after, ""),
            };
            if !prefix.is_empty() {
                return (Some(prefix), rest);
            }
        }
    }
    (None, query)
}

fn prev_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    idx -= 1;
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn next_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    idx += 1;
    while !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn prev_word_boundary(s: &str, mut idx: usize) -> usize {
    while idx > 0 {
        let prev = prev_char_boundary(s, idx);
        let ch = s[prev..idx].chars().next().unwrap_or(' ');
        if ch.is_alphanumeric() {
            break;
        }
        idx = prev;
    }
    while idx > 0 {
        let prev = prev_char_boundary(s, idx);
        let ch = s[prev..idx].chars().next().unwrap_or(' ');
        if !ch.is_alphanumeric() {
            break;
        }
        idx = prev;
    }
    idx
}

fn next_word_boundary(s: &str, mut idx: usize) -> usize {
    let len = s.len();
    while idx < len {
        let next = next_char_boundary(s, idx);
        let ch = s[idx..next].chars().next().unwrap_or(' ');
        if ch.is_alphanumeric() {
            break;
        }
        idx = next;
    }
    while idx < len {
        let next = next_char_boundary(s, idx);
        let ch = s[idx..next].chars().next().unwrap_or(' ');
        if !ch.is_alphanumeric() {
            break;
        }
        idx = next;
    }
    idx
}
