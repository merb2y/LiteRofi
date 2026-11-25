//! Theme tree (`ThemeNode`), cascade/inheritance resolution, `@media` evaluation,
//! `configuration {}` mapping, and `-dump-theme` formatting (`source/theme.c`).

use crate::colors::ThemeColor;
use crate::parser::{parse_into_sheet, ParseOptions};
use crate::types::{
    highlight_flags, CursorType, Distance, GradientDirection, HighlightColorStyle, ImageScale,
    ImageValue, MediaCondition, MonitorGeometry, Orientation, Padding, PropertyValue, ThemeError,
};
use literofi_core::config::{Config, MatchingMethod, SortingMethod, WindowLocation};
use std::fmt::Write as _;
use std::path::Path;

/// Embedded upstream Rofi default `.rasi` stylesheet.
pub const DEFAULT_THEME_RASI: &str = include_str!("default_theme.rasi");

/// Recursive rule tree node (`ThemeWidget`).
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeNode {
    pub name: String,
    pub properties: Vec<(String, PropertyValue)>,
    pub children: Vec<ThemeNode>,
    pub media: Option<MediaCondition>,
}

impl ThemeNode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            properties: Vec::new(),
            children: Vec::new(),
            media: None,
        }
    }

    pub fn set_property(&mut self, name: impl Into<String>, value: PropertyValue) {
        let key = name.into();
        if let Some((_, existing)) = self.properties.iter_mut().find(|(k, _)| k == &key) {
            *existing = value;
        } else {
            self.properties.push((key, value));
        }
    }

    pub fn get_raw_property(&self, name: &str) -> Option<&PropertyValue> {
        self.properties
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }

    pub fn find_or_create_child(&mut self, name: &str) -> &mut ThemeNode {
        if let Some(pos) = self.children.iter().position(|c| c.name == name) {
            return &mut self.children[pos];
        }
        self.children.push(ThemeNode::new(name));
        let last = self.children.len() - 1;
        &mut self.children[last]
    }

    pub fn find_child(&self, name: &str) -> Option<&ThemeNode> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn merge_from(&mut self, other: ThemeNode, is_root: bool) {
        if is_root && other.name == "*" {
            for (k, v) in other.properties {
                self.set_property(k, v);
            }
            return;
        }
        if other.media.is_some() {
            self.children.push(other);
            return;
        }
        let target = self.find_or_create_child(&other.name);
        if other.media.is_some() {
            target.media = other.media;
        }
        for (k, v) in other.properties {
            target.set_property(k, v);
        }
        for child in other.children {
            target.merge_from(child, false);
        }
    }
}

/// Widget path & state used to resolve cascaded/inherited theme properties.
#[derive(Debug, Clone, Copy)]
pub struct WidgetQuery<'a> {
    pub name: &'a str,
    pub state: Option<&'a str>,
    pub parent: Option<&'a WidgetQuery<'a>>,
}

impl<'a> WidgetQuery<'a> {
    pub const fn new(name: &'a str) -> Self {
        Self {
            name,
            state: None,
            parent: None,
        }
    }

    pub const fn with_state(mut self, state: &'a str) -> Self {
        self.state = Some(state);
        self
    }

    pub const fn with_parent(mut self, parent: &'a WidgetQuery<'a>) -> Self {
        self.parent = Some(parent);
        self
    }
}

/// Parsed theme + configuration container.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeSheet {
    pub root: ThemeNode,
    pub config_root: ThemeNode,
}

impl Default for ThemeSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeSheet {
    pub fn new() -> Self {
        Self {
            root: ThemeNode::new("Root"),
            config_root: ThemeNode::new("Root"),
        }
    }

    /// Parses and returns Rofi's built-in default theme.
    pub fn default_theme() -> Result<Self, ThemeError> {
        let mut sheet = Self::new();
        sheet.parse_str(DEFAULT_THEME_RASI, &ParseOptions::default())?;
        Ok(sheet)
    }

    /// Clears all theme rules while preserving `configuration {}`.
    pub fn reset_theme(&mut self) {
        self.root = ThemeNode::new("Root");
    }

    /// Returns `true` when no theme rules have been loaded (matching upstream `rofi.c:1221`).
    pub fn is_theme_empty(&self) -> bool {
        self.root.properties.is_empty() && self.root.children.is_empty()
    }

    /// Parses `.rasi` source text and merges rules into `self`.
    pub fn parse_str(&mut self, src: &str, opts: &ParseOptions) -> Result<(), ThemeError> {
        parse_into_sheet(self, src, None, opts, &mut Vec::new())
    }

    /// Parses a `.rasi` file and merges rules into `self` (supporting `@import` / `@theme`).
    pub fn parse_file(&mut self, path: &Path, opts: &ParseOptions) -> Result<(), ThemeError> {
        let src = std::fs::read_to_string(path).map_err(|e| ThemeError {
            file: Some(path.display().to_string()),
            line: 1,
            col: 1,
            message: format!("failed to read theme file: {e}"),
        })?;
        let mut stack = vec![fs_canon_or_buf(path)];
        parse_into_sheet(self, &src, Some(path), opts, &mut stack)
    }

    /// Evaluates `@media (...)` nodes against `mon`, merging matching subtrees in place.
    pub fn process_conditionals(&mut self, mon: &MonitorGeometry) {
        process_node_conditionals(&mut self.root, mon, true);
    }

    fn resolve_link<'a>(&'a self, mut prop: &'a PropertyValue) -> Option<&'a PropertyValue> {
        for _ in 0..20 {
            match prop {
                PropertyValue::Link { name, default } => {
                    if let Some(target) = self.root.get_raw_property(name) {
                        prop = target;
                    } else if let Some(def) = default {
                        return Some(def.as_ref());
                    } else {
                        return None;
                    }
                }
                other => return Some(other),
            }
        }
        None
    }

    fn find_widget_chain<'a>(&'a self, name: &str, state: Option<&str>) -> Vec<&'a ThemeNode> {
        let mut chain = vec![&self.root];
        let Some(widget_node) = self.root.find_child(name) else {
            return chain;
        };
        chain.push(widget_node);
        let mut current = widget_node;
        if let Some(st) = state {
            for part in st.split('.').filter(|s| !s.is_empty()) {
                if let Some(next) = current.find_child(part) {
                    chain.push(next);
                    current = next;
                }
            }
        }
        chain
    }

    /// Looks up a property value across widget, its states, Root `*`, `@var` links, and `inherit`.
    pub fn find_property<'a>(
        &'a self,
        query: &WidgetQuery<'_>,
        prop_name: &str,
    ) -> Option<&'a PropertyValue> {
        let mut cur_query = Some(query);
        while let Some(q) = cur_query {
            let chain = self.find_widget_chain(q.name, q.state);
            let mut inherited = false;
            for node in chain.into_iter().rev() {
                if let Some(raw) = node.get_raw_property(prop_name) {
                    if matches!(raw, PropertyValue::Inherit) {
                        inherited = true;
                        break;
                    }
                    if let Some(resolved) = self.resolve_link(raw) {
                        if matches!(resolved, PropertyValue::Inherit) {
                            inherited = true;
                            break;
                        }
                        return Some(resolved);
                    }
                }
            }
            if inherited {
                cur_query = q.parent;
            } else {
                return None;
            }
        }
        None
    }

    /// Looks up the first matching property among `aliases` walking `query -> state -> widget -> *`
    /// and following `query.parent` on `PropertyValue::Inherit` (matching `source/theme.c`).
    pub fn find_property_aliases<'a>(
        &'a self,
        query: &WidgetQuery<'_>,
        aliases: &[&str],
    ) -> Option<&'a PropertyValue> {
        let mut cur_query = Some(query);
        let mut inherited_state = query.state;
        while let Some(q) = cur_query {
            let eff_state = q.state.or(inherited_state);
            let chain = self.find_widget_chain(q.name, eff_state);
            let mut inherited = false;
            'nodes: for node in chain.into_iter().rev() {
                for &prop_name in aliases {
                    if let Some(raw) = node.get_raw_property(prop_name) {
                        if matches!(raw, PropertyValue::Inherit) {
                            inherited = true;
                            break 'nodes;
                        }
                        if let Some(resolved) = self.resolve_link(raw) {
                            if matches!(resolved, PropertyValue::Inherit) {
                                inherited = true;
                                break 'nodes;
                            }
                            return Some(resolved);
                        }
                    }
                }
            }
            if inherited {
                inherited_state = eff_state;
                if let Some(p) = q.parent {
                    cur_query = Some(p);
                } else {
                    return Some(&PropertyValue::Inherit);
                }
            } else {
                return None;
            }
        }
        None
    }

    /// Looks up the first matching property among `aliases` on the most-specific widget/state
    /// node (excluding `*` root), preserving state specificity over alias order.
    pub fn find_widget_property_aliases<'a>(
        &'a self,
        name: &str,
        state: Option<&str>,
        aliases: &[&str],
    ) -> Option<&'a PropertyValue> {
        let chain = self.find_widget_chain(name, state);
        if chain.len() <= 1 {
            return None;
        }
        for node in chain[1..].iter().rev() {
            for &prop_name in aliases {
                if let Some(raw) = node.get_raw_property(prop_name) {
                    if matches!(raw, PropertyValue::Inherit) {
                        return Some(raw);
                    }
                    if let Some(resolved) = self.resolve_link(raw) {
                        return Some(resolved);
                    }
                }
            }
        }
        None
    }

    /// Looks up a property ONLY on the global `*` root node (resolving `@var` links).
    pub fn find_root_property<'a>(&'a self, prop_name: &str) -> Option<&'a PropertyValue> {
        let raw = self.root.get_raw_property(prop_name)?;
        if matches!(raw, PropertyValue::Inherit) {
            return Some(raw);
        }
        self.resolve_link(raw)
    }

    pub fn get_boolean(&self, query: &WidgetQuery<'_>, prop: &str, default: bool) -> bool {
        match self.find_property(query, prop) {
            Some(PropertyValue::Boolean(b)) => *b,
            Some(PropertyValue::Integer(i)) => *i != 0,
            _ => default,
        }
    }

    pub fn get_integer(&self, query: &WidgetQuery<'_>, prop: &str, default: i32) -> i32 {
        match self.find_property(query, prop) {
            Some(PropertyValue::Integer(i)) => *i,
            _ => default,
        }
    }

    pub fn get_double(&self, query: &WidgetQuery<'_>, prop: &str, default: f64) -> f64 {
        match self.find_property(query, prop) {
            Some(PropertyValue::Double(f)) => *f,
            Some(PropertyValue::Integer(i)) => f64::from(*i),
            _ => default,
        }
    }

    pub fn get_string<'a>(
        &'a self,
        query: &WidgetQuery<'_>,
        prop: &str,
        default: &'a str,
    ) -> &'a str {
        match self.find_property(query, prop) {
            Some(PropertyValue::String(s)) => s.as_str(),
            _ => default,
        }
    }

    pub fn get_color(
        &self,
        query: &WidgetQuery<'_>,
        prop: &str,
        default: ThemeColor,
    ) -> ThemeColor {
        match self.find_property(query, prop) {
            Some(PropertyValue::Color(c)) => *c,
            _ => default,
        }
    }

    pub fn get_padding(&self, query: &WidgetQuery<'_>, prop: &str, default: Padding) -> Padding {
        match self.find_property(query, prop) {
            Some(PropertyValue::Padding(p)) => p.clone(),
            Some(PropertyValue::Integer(i)) => Padding::all(Distance::px(f64::from(*i))),
            Some(PropertyValue::Double(f)) => Padding::all(Distance::px(*f)),
            _ => default,
        }
    }

    pub fn get_distance(&self, query: &WidgetQuery<'_>, prop: &str, default: Distance) -> Distance {
        match self.find_property(query, prop) {
            Some(PropertyValue::Padding(p)) => p.left.clone(),
            Some(PropertyValue::Integer(i)) => Distance::px(f64::from(*i)),
            Some(PropertyValue::Double(f)) => Distance::px(*f),
            _ => default,
        }
    }

    pub fn get_position(
        &self,
        query: &WidgetQuery<'_>,
        prop: &str,
        default: WindowLocation,
    ) -> WindowLocation {
        match self.find_property(query, prop) {
            Some(PropertyValue::Position(loc)) => *loc,
            _ => default,
        }
    }

    pub fn get_orientation(
        &self,
        query: &WidgetQuery<'_>,
        prop: &str,
        default: Orientation,
    ) -> Orientation {
        match self.find_property(query, prop) {
            Some(PropertyValue::Orientation(o)) => *o,
            _ => default,
        }
    }

    pub fn get_cursor(
        &self,
        query: &WidgetQuery<'_>,
        prop: &str,
        default: CursorType,
    ) -> CursorType {
        match self.find_property(query, prop) {
            Some(PropertyValue::Cursor(c)) => *c,
            _ => default,
        }
    }

    pub fn get_highlight(
        &self,
        query: &WidgetQuery<'_>,
        prop: &str,
        default: HighlightColorStyle,
    ) -> HighlightColorStyle {
        match self.find_property(query, prop) {
            Some(PropertyValue::Highlight(h)) => *h,
            Some(PropertyValue::Color(c)) => HighlightColorStyle {
                style: highlight_flags::COLOR,
                color: *c,
            },
            _ => default,
        }
    }

    pub fn get_image(&self, query: &WidgetQuery<'_>, prop: &str) -> Option<&ImageValue> {
        match self.find_property(query, prop) {
            Some(PropertyValue::Image(img)) => Some(img),
            _ => None,
        }
    }

    pub fn get_list_strings(&self, query: &WidgetQuery<'_>, prop: &str) -> Option<Vec<String>> {
        match self.find_property(query, prop) {
            Some(PropertyValue::List(items)) => Some(
                items
                    .iter()
                    .filter_map(|v| match v {
                        PropertyValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Applies parsed `configuration { ... }` options onto `cfg`.
    pub fn apply_configuration(&self, cfg: &mut Config) {
        for (name, val) in &self.config_root.properties {
            let Some(resolved) = self.resolve_config_value(val) else {
                continue;
            };
            match name.as_str() {
                "modes" | "modi" | "switchers" => {
                    if let Some(s) = as_config_string(resolved) {
                        cfg.modes = s;
                    }
                }
                "font" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.font = s.clone();
                    }
                }
                "location" => {
                    if let PropertyValue::Integer(i) = resolved {
                        cfg.location = WindowLocation::from_u8((*i).clamp(0, 255) as u8);
                    } else if let PropertyValue::Position(loc) = resolved {
                        cfg.location = *loc;
                    }
                }
                "xoffset" => {
                    if let PropertyValue::Integer(i) = resolved {
                        cfg.x_offset = *i;
                    }
                }
                "yoffset" => {
                    if let PropertyValue::Integer(i) = resolved {
                        cfg.y_offset = *i;
                    }
                }
                "fixed-num-lines" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.fixed_num_lines = *b;
                    }
                }
                "show-icons" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.show_icons = *b;
                        cfg.hide_icons = !*b;
                    }
                }
                "blur-radius" => match resolved {
                    PropertyValue::Integer(n) => {
                        cfg.blur_radius = (*n).clamp(0, 64) as u32;
                    }
                    PropertyValue::Double(r) => {
                        cfg.blur_radius = r.round().clamp(0.0, 64.0) as u32;
                    }
                    PropertyValue::Boolean(b) => {
                        cfg.blur_radius = if *b { 12 } else { 0 };
                    }
                    _ => {}
                },
                "terminal" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.terminal = s.clone();
                    }
                }
                "ssh-client" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.ssh_client = s.clone();
                    }
                }
                "ssh-command" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.ssh_command = s.clone();
                    }
                }
                "run-command" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.run_command = s.clone();
                    }
                }
                "run-list-command" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.run_list_command = s.clone();
                    }
                }
                "run-shell-command" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.run_shell_command = s.clone();
                    }
                }
                "window-command" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.window_command = s.clone();
                    }
                }
                "window-match-fields" => {
                    if let Some(s) = as_config_string(resolved) {
                        cfg.window_match_fields = s;
                    }
                }
                "window-format" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.window_format = s.clone();
                    }
                }
                "drun-match-fields" => {
                    if let Some(s) = as_config_string(resolved) {
                        cfg.drun_match_fields = s;
                    }
                }
                "drun-display-format" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.drun_display_format = s.clone();
                    }
                }
                "drun-show-actions" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.drun_show_actions = *b;
                    }
                }
                "disable-history" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.disable_history = *b;
                    }
                }
                "ignored-prefixes" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.ignored_prefixes = s.clone();
                    }
                }
                "sort" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.sort = *b;
                    }
                }
                "sorting-method" => {
                    if let PropertyValue::String(s) = resolved {
                        if let Some(m) = SortingMethod::parse(s) {
                            cfg.sorting_method = m;
                        }
                    }
                }
                "case-sensitive" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.case_sensitive = *b;
                    }
                }
                "case-smart" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.case_smart = *b;
                    }
                }
                "cycle" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.cycle = *b;
                    }
                }
                "sidebar-mode" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.sidebar_mode = *b;
                    }
                }
                "hover-select" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.hover_select = *b;
                    }
                }
                "auto-select" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.auto_select = *b;
                    }
                }
                "combi-modes" | "combi-modi" => {
                    if let Some(s) = as_config_string(resolved) {
                        cfg.combi_modes = s;
                    }
                }
                "combi-display-format" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.combi_display_format = s.clone();
                    }
                }
                "matching" => {
                    if let PropertyValue::String(s) = resolved {
                        if let Some(m) = MatchingMethod::parse(s) {
                            cfg.matching = m;
                        }
                    }
                }
                "tokenize" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.tokenize = *b;
                    }
                }
                "normalize-match" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.normalize_match = *b;
                    }
                }
                "dpi" => {
                    if let PropertyValue::Integer(i) = resolved {
                        cfg.dpi = *i;
                    }
                }
                "max-history-size" => {
                    if let PropertyValue::Integer(i) = resolved {
                        if *i >= 0 {
                            cfg.max_history_size = *i as usize;
                        }
                    }
                }
                "click-to-exit" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.click_to_exit = *b;
                    }
                }
                "steal-focus" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.steal_focus = *b;
                    }
                }
                "icon-theme" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.icon_theme = Some(s.clone());
                    }
                }
                "drun-categories" => {
                    if let Some(s) = as_config_string(resolved) {
                        cfg.drun_categories = Some(s);
                    }
                }
                "drun-exclude-categories" => {
                    if let Some(s) = as_config_string(resolved) {
                        cfg.drun_exclude_categories = Some(s);
                    }
                }
                "combi-hide-mode-prefix" => {
                    if let PropertyValue::Boolean(b) = resolved {
                        cfg.combi_hide_mode_prefix = *b;
                    }
                }
                "display-drun" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_drun = Some(s.clone());
                    }
                }
                "display-run" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_run = Some(s.clone());
                    }
                }
                "display-window" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_window = Some(s.clone());
                    }
                }
                "display-windowcd" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_windowcd = Some(s.clone());
                    }
                }
                "display-combi" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_combi = Some(s.clone());
                    }
                }
                "display-filebrowser" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_filebrowser = Some(s.clone());
                    }
                }
                "display-recursivebrowser" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_recursivebrowser = Some(s.clone());
                    }
                }
                "display-ssh" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_ssh = Some(s.clone());
                    }
                }
                "display-keys" => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.display_keys = Some(s.clone());
                    }
                }
                "daemon-hotkey" => {
                    if !cfg.daemon_hotkey_set {
                        if let PropertyValue::String(s) = resolved {
                            cfg.daemon_hotkey = s.clone();
                        }
                    }
                }
                "daemon-mode" => {
                    if !cfg.daemon_mode_set {
                        if let PropertyValue::String(s) = resolved {
                            cfg.daemon_mode = s.clone();
                        }
                    }
                }
                other if other.starts_with("kb-") => {
                    if let PropertyValue::String(s) = resolved {
                        cfg.keybindings.set_by_name(other, s);
                    }
                }
                _ => {}
            }
        }
    }

    fn resolve_config_value<'a>(&'a self, val: &'a PropertyValue) -> Option<&'a PropertyValue> {
        match val {
            PropertyValue::Link { name, default } => self
                .config_root
                .get_raw_property(name)
                .or(default.as_deref()),
            other => Some(other),
        }
    }

    /// Formats the active theme in `.rasi` syntax (`-dump-theme`).
    pub fn dump_theme(&self) -> String {
        let mut out = String::from("/**\n * literofi -dump-theme output.\n **/\n");
        if !self.root.properties.is_empty() {
            out.push_str("* {\n");
            format_properties(&mut out, &self.root.properties, 4);
            out.push_str("}\n");
        }
        let mut path = Vec::new();
        for child in &self.root.children {
            dump_node(&mut out, child, &mut path);
        }
        out
    }
}

fn as_config_string(val: &PropertyValue) -> Option<String> {
    match val {
        PropertyValue::String(s) => Some(s.clone()),
        PropertyValue::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|v| match v {
                    PropertyValue::String(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();
            Some(parts.join(","))
        }
        _ => None,
    }
}

pub(crate) fn fs_canon_or_buf(p: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

fn eval_media(cond: MediaCondition, mon: &MonitorGeometry) -> bool {
    let w = f64::from(mon.width);
    let h = f64::from(mon.height).max(1.0);
    match cond {
        MediaCondition::MinWidth(v) => w >= v,
        MediaCondition::MaxWidth(v) => w < v,
        MediaCondition::MinHeight(v) => f64::from(mon.height) >= v,
        MediaCondition::MaxHeight(v) => f64::from(mon.height) < v,
        MediaCondition::MonitorId(id) => f64::from(mon.monitor_id) == f64::from(id),
        MediaCondition::MinAspectRatio(r) => (w / h) >= r,
        MediaCondition::MaxAspectRatio(r) => (w / h) < r,
        MediaCondition::Enabled(b) => b,
    }
}

fn process_node_conditionals(node: &mut ThemeNode, mon: &MonitorGeometry, is_root: bool) {
    let children = std::mem::take(&mut node.children);
    for mut child in children {
        if let Some(cond) = child.media {
            if eval_media(cond, mon) {
                for grand in child.children {
                    node.merge_from(grand, is_root);
                }
            }
        } else {
            process_node_conditionals(&mut child, mon, false);
            node.merge_from(child, is_root);
        }
    }
}

fn dump_node(out: &mut String, node: &ThemeNode, path: &mut Vec<String>) {
    if let Some(cond) = node.media {
        let cond_str = match cond {
            MediaCondition::MinWidth(v) => format!("min-width: {v} px"),
            MediaCondition::MaxWidth(v) => format!("max-width: {v} px"),
            MediaCondition::MinHeight(v) => format!("min-height: {v} px"),
            MediaCondition::MaxHeight(v) => format!("max-height: {v} px"),
            MediaCondition::MonitorId(v) => format!("monitor-id: {v}"),
            MediaCondition::MinAspectRatio(v) => format!("min-aspect-ratio: {v}"),
            MediaCondition::MaxAspectRatio(v) => format!("max-aspect-ratio: {v}"),
            MediaCondition::Enabled(b) => format!("enabled: {b}"),
        };
        let _ = writeln!(out, "@media ( {cond_str} ) {{");
        let mut inner = Vec::new();
        for c in &node.children {
            dump_node(out, c, &mut inner);
        }
        out.push_str("}\n");
        return;
    }

    path.push(node.name.clone());
    if !node.properties.is_empty() {
        let selector = if path.len() == 1 {
            path[0].clone()
        } else {
            format!("{} {}", path[0], path[1..].join("."))
        };
        let _ = writeln!(out, "{selector} {{");
        format_properties(out, &node.properties, 4);
        out.push_str("}\n");
    }
    for c in &node.children {
        dump_node(out, c, path);
    }
    path.pop();
}

fn format_properties(out: &mut String, props: &[(String, PropertyValue)], indent: usize) {
    let max_len = props.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (k, v) in props {
        let pad = max_len.saturating_sub(k.len());
        let val = format_property_value(v);
        let _ = writeln!(out, "{:indent$}{k}: {:pad$}{val};", "", "");
    }
}

fn format_property_value(val: &PropertyValue) -> String {
    match val {
        PropertyValue::Integer(i) => i.to_string(),
        PropertyValue::Double(f) => format!("{f:.4}"),
        PropertyValue::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        PropertyValue::Boolean(b) => b.to_string(),
        PropertyValue::Color(c) => c.to_string(),
        PropertyValue::Image(img) => match img {
            ImageValue::Url { url, scale } => {
                let sc = match scale {
                    ImageScale::None => None,
                    ImageScale::Both => Some("both"),
                    ImageScale::Width => Some("width"),
                    ImageScale::Height => Some("height"),
                };
                match sc {
                    Some(s) => format!("url(\"{url}\", {s})"),
                    None => format!("url(\"{url}\")"),
                }
            }
            ImageValue::LinearGradient { dir, colors } => {
                let cols: Vec<String> = colors.iter().map(ToString::to_string).collect();
                let dir_prefix = match dir {
                    GradientDirection::Right => "to right, ",
                    GradientDirection::Left => "to left, ",
                    GradientDirection::Top => "to top, ",
                    GradientDirection::Bottom => "to bottom, ",
                    GradientDirection::Angle(deg) => {
                        return format!("linear-gradient({deg}deg, {})", cols.join(", "))
                    }
                };
                format!("linear-gradient({dir_prefix}{})", cols.join(", "))
            }
        },
        PropertyValue::Padding(p) => {
            if p.top == p.bottom && p.left == p.right && p.left == p.top {
                p.left.format_rasi()
            } else if p.top == p.bottom && p.left == p.right {
                format!("{} {}", p.top.format_rasi(), p.left.format_rasi())
            } else if p.left == p.right {
                format!(
                    "{} {} {}",
                    p.top.format_rasi(),
                    p.left.format_rasi(),
                    p.bottom.format_rasi()
                )
            } else {
                format!(
                    "{} {} {} {}",
                    p.top.format_rasi(),
                    p.right.format_rasi(),
                    p.bottom.format_rasi(),
                    p.left.format_rasi()
                )
            }
        }
        PropertyValue::Link { name, default } => match default {
            Some(def) => format!("var({name}, {})", format_property_value(def)),
            None => format!("var({name})"),
        },
        PropertyValue::Position(pos) => match pos {
            WindowLocation::Center => "center".to_string(),
            WindowLocation::North => "north".to_string(),
            WindowLocation::South => "south".to_string(),
            WindowLocation::East => "east".to_string(),
            WindowLocation::West => "west".to_string(),
            WindowLocation::NorthEast => "northeast".to_string(),
            WindowLocation::SouthEast => "southeast".to_string(),
            WindowLocation::NorthWest => "northwest".to_string(),
            WindowLocation::SouthWest => "southwest".to_string(),
        },
        PropertyValue::Highlight(h) => {
            let mut parts = Vec::new();
            if h.style & highlight_flags::BOLD != 0 {
                parts.push("bold".to_string());
            }
            if h.style & highlight_flags::UNDERLINE != 0 {
                parts.push("underline".to_string());
            }
            if h.style & highlight_flags::STRIKETHROUGH != 0 {
                parts.push("strikethrough".to_string());
            }
            if h.style & highlight_flags::ITALIC != 0 {
                parts.push("italic".to_string());
            }
            if h.style & highlight_flags::UPPERCASE != 0 {
                parts.push("uppercase".to_string());
            }
            if h.style & highlight_flags::LOWERCASE != 0 {
                parts.push("lowercase".to_string());
            }
            if h.style & highlight_flags::CAPITALIZE != 0 {
                parts.push("capitalize".to_string());
            }
            if h.style & highlight_flags::COLOR != 0 {
                parts.push(h.color.to_string());
            }
            if parts.is_empty() {
                "none".to_string()
            } else {
                parts.join(" ")
            }
        }
        PropertyValue::List(items) => {
            let inner: Vec<String> = items
                .iter()
                .map(|v| match v {
                    PropertyValue::String(s)
                        if !s.is_empty()
                            && s.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') =>
                    {
                        s.clone()
                    }
                    other => format_property_value(other),
                })
                .collect();
            format!("[ {} ]", inner.join(", "))
        }
        PropertyValue::Orientation(o) => match o {
            Orientation::Horizontal => "horizontal".to_string(),
            Orientation::Vertical => "vertical".to_string(),
        },
        PropertyValue::Cursor(c) => match c {
            CursorType::Default => "default".to_string(),
            CursorType::Pointer => "pointer".to_string(),
            CursorType::Text => "text".to_string(),
        },
        PropertyValue::Inherit => "inherit".to_string(),
    }
}
