//! Recursive-descent `.rasi` parser matching `reference/rofi/lexer/theme-parser.y`.

use crate::colors::{
    cmyk_to_rgb, hsl_to_rgb, hwb_to_rgb, lookup_css_color, parse_hex_color, ThemeColor,
};
use crate::lexer::{tokenize, Token, TokenKind};
use crate::sheet::{fs_canon_or_buf, ThemeNode, ThemeSheet, DEFAULT_THEME_RASI};
use crate::types::{
    highlight_flags, CursorType, Distance, DistanceExpr, DistanceUnit, GradientDirection,
    HighlightColorStyle, ImageScale, ImageValue, LineStyle, MediaCondition, Orientation, Padding,
    PropertyValue, ThemeError,
};
use literofi_core::config::WindowLocation;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};

/// Options controlling `@import` / `@theme` search paths and `@media (enabled: dmenu)`.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub search_dirs: Vec<PathBuf>,
    pub dmenu_mode: bool,
}

pub(crate) fn parse_into_sheet(
    sheet: &mut ThemeSheet,
    src: &str,
    current_file: Option<&Path>,
    opts: &ParseOptions,
    import_stack: &mut Vec<PathBuf>,
) -> Result<(), ThemeError> {
    let src = src.strip_prefix('\u{FEFF}').unwrap_or(src);
    let filename = current_file.map(|p| p.display().to_string());
    let tokens = tokenize(src, filename.as_deref())?;
    let mut parser = Parser {
        src,
        tokens: &tokens,
        pos: 0,
        current_dir: current_file.and_then(|p| p.parent()).map(Path::to_path_buf),
        filename: filename.as_deref(),
        opts,
        import_stack,
    };

    while !parser.at_eof() {
        parser.parse_top_entry(sheet)?;
    }
    Ok(())
}

fn extract_rofi_subpath(spec: &str) -> Option<&str> {
    let trimmed = spec.trim();
    for marker in [".config/rofi/", ".config\\rofi\\", "/rofi/", "\\rofi\\"] {
        if let Some(idx) = trimmed.find(marker) {
            let sub = &trimmed[idx + marker.len()..];
            if !sub.is_empty() {
                return Some(sub);
            }
        }
    }
    None
}

fn resolve_import_path(
    spec: &str,
    current_dir: Option<&Path>,
    search_dirs: &[PathBuf],
) -> Option<PathBuf> {
    let expanded = expand_tilde_spec(spec);
    let raw = Path::new(&expanded);
    if raw.is_absolute() || raw.has_root() || expanded.starts_with('/') {
        if let Some(found) = try_extensions(raw) {
            return Some(found);
        }
        // Fallback for Linux `.config/rofi/<subpath>` imports (e.g. `wallust/colors-rofi.rasi`)
        if let Some(sub) = extract_rofi_subpath(spec) {
            let sub_path = Path::new(sub);
            if let Some(dir) = current_dir {
                let mut cur = Some(dir);
                for _ in 0..5 {
                    let Some(anc) = cur else {
                        break;
                    };
                    if let Some(found) = try_extensions(&anc.join(sub_path)) {
                        return Some(found);
                    }
                    cur = anc.parent();
                }
            }
            for dir in search_dirs {
                if let Some(found) = try_extensions(&dir.join(sub_path)) {
                    return Some(found);
                }
                if let Some(parent) = dir.parent() {
                    if let Some(found) = try_extensions(&parent.join(sub_path)) {
                        return Some(found);
                    }
                }
            }
        }
        // Fallback for Linux absolute theme paths (e.g. `/usr/share/rofi/themes/tokyonight.rasi`)
        if let Some(fname) = raw.file_name() {
            if let Some(dir) = current_dir {
                if let Some(found) = try_extensions(&dir.join(fname)) {
                    return Some(found);
                }
            }
            for dir in search_dirs {
                if let Some(found) = try_extensions(&dir.join(fname)) {
                    return Some(found);
                }
            }
        }
        return None;
    }
    if let Some(dir) = current_dir {
        if let Some(found) = try_extensions(&dir.join(raw)) {
            return Some(found);
        }
    }
    for dir in search_dirs {
        if let Some(found) = try_extensions(&dir.join(raw)) {
            return Some(found);
        }
    }
    try_extensions(raw)
}

fn expand_tilde_spec(spec: &str) -> String {
    let trimmed = spec.trim();
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        for env_key in ["USERPROFILE", "HOME"] {
            if let Ok(home) = std::env::var(env_key) {
                if !home.trim().is_empty() {
                    return PathBuf::from(home)
                        .join(rest)
                        .to_string_lossy()
                        .into_owned();
                }
            }
        }
    }
    trimmed.to_string()
}

fn try_extensions(base: &Path) -> Option<PathBuf> {
    if base.is_file() {
        return Some(base.to_path_buf());
    }
    if base.extension().is_none() {
        for ext in ["rasi", "rasinc"] {
            let cand = base.with_extension(ext);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

struct Parser<'a> {
    src: &'a str,
    tokens: &'a [Token],
    pos: usize,
    current_dir: Option<PathBuf>,
    filename: Option<&'a str>,
    opts: &'a ParseOptions,
    import_stack: &'a mut Vec<PathBuf>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &'a Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_at(&self, offset: usize) -> Option<&'a Token> {
        self.tokens.get(self.pos + offset)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn advance(&mut self) -> &'a Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if !matches!(tok.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        tok
    }

    fn err_here(&self, msg: impl Into<String>) -> ThemeError {
        let t = self.peek();
        ThemeError {
            file: self.filename.map(str::to_string),
            line: t.line,
            col: t.col,
            message: msg.into(),
        }
    }

    fn err_at(&self, tok: &Token, msg: impl Into<String>) -> ThemeError {
        ThemeError {
            file: self.filename.map(str::to_string),
            line: tok.line,
            col: tok.col,
            message: msg.into(),
        }
    }

    fn expect_lbrace(&mut self) -> Result<(), ThemeError> {
        if matches!(self.peek().kind, TokenKind::LBrace) {
            self.advance();
            Ok(())
        } else {
            Err(self.err_here("expected `{`"))
        }
    }

    fn expect_rbrace(&mut self) -> Result<(), ThemeError> {
        if matches!(self.peek().kind, TokenKind::RBrace) {
            self.advance();
            Ok(())
        } else {
            Err(self.err_here("expected `}`"))
        }
    }

    fn expect_lparen(&mut self) -> Result<(), ThemeError> {
        if matches!(self.peek().kind, TokenKind::LParen) {
            self.advance();
            Ok(())
        } else {
            Err(self.err_here("expected `(`"))
        }
    }

    fn expect_rparen(&mut self) -> Result<(), ThemeError> {
        if matches!(self.peek().kind, TokenKind::RParen) {
            self.advance();
            Ok(())
        } else {
            Err(self.err_here("expected `)`"))
        }
    }

    fn expect_colon(&mut self) -> Result<(), ThemeError> {
        if matches!(self.peek().kind, TokenKind::Colon) {
            self.advance();
            Ok(())
        } else {
            Err(self.err_here("expected `:`"))
        }
    }

    fn expect_semicolon(&mut self) -> Result<(), ThemeError> {
        if matches!(self.peek().kind, TokenKind::Semicolon) {
            self.advance();
            Ok(())
        } else {
            Err(self.err_here("expected `;`"))
        }
    }

    fn parse_top_entry(&mut self, sheet: &mut ThemeSheet) -> Result<(), ThemeError> {
        match &self.peek().kind {
            TokenKind::Import | TokenKind::OptImport => {
                let optional = matches!(self.peek().kind, TokenKind::OptImport);
                let dir_tok = self.advance();
                let target = match &self.peek().kind {
                    TokenKind::String(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    _ => return Err(self.err_at(dir_tok, "expected quoted path after import")),
                };
                self.load_import(&target, optional, sheet)
            }
            TokenKind::ThemeDir => {
                let dir_tok = self.advance();
                let target = match &self.peek().kind {
                    TokenKind::String(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    _ => return Err(self.err_at(dir_tok, "expected quoted name after `@theme`")),
                };
                sheet.reset_theme();
                if target.eq_ignore_ascii_case("default") {
                    let mut dummy = Vec::new();
                    parse_into_sheet(sheet, DEFAULT_THEME_RASI, None, self.opts, &mut dummy)
                } else if target.trim().is_empty()
                    || target.trim() == "/dev/null"
                    || target.trim().eq_ignore_ascii_case("nul")
                {
                    Ok(())
                } else {
                    self.load_import(&target, false, sheet)
                }
            }
            TokenKind::Ident(word) if word.eq_ignore_ascii_case("configuration") => {
                self.advance();
                self.expect_lbrace()?;
                let mut tmp = ThemeNode::new("Root");
                self.parse_config_body(&mut tmp)?;
                self.expect_rbrace()?;
                for (k, v) in tmp.properties {
                    sheet.config_root.set_property(k, v);
                }
                for child in tmp.children {
                    sheet.config_root.merge_from(child, false);
                }
                Ok(())
            }
            TokenKind::Star => {
                self.advance();
                self.expect_lbrace()?;
                let props = self.parse_property_list()?;
                self.expect_rbrace()?;
                for (k, v) in props {
                    sheet.root.set_property(k, v);
                }
                Ok(())
            }
            TokenKind::MediaDir => {
                let node = self.parse_media_block()?;
                sheet.root.merge_from(node, true);
                Ok(())
            }
            _ => {
                let nodes = self.parse_selector_block()?;
                for node in nodes {
                    sheet.root.merge_from(node, true);
                }
                Ok(())
            }
        }
    }

    fn load_import(
        &mut self,
        target: &str,
        optional: bool,
        sheet: &mut ThemeSheet,
    ) -> Result<(), ThemeError> {
        let Some(path) =
            resolve_import_path(target, self.current_dir.as_deref(), &self.opts.search_dirs)
        else {
            if target.trim().eq_ignore_ascii_case("default")
                || target.trim().eq_ignore_ascii_case("default.rasi")
            {
                return parse_into_sheet(
                    sheet,
                    crate::DEFAULT_THEME_RASI,
                    None,
                    self.opts,
                    self.import_stack,
                );
            }
            if optional {
                return Ok(());
            }
            return Err(self.err_here(format!("failed to locate imported theme `{target}`")));
        };
        let canon = fs_canon_or_buf(&path);
        if self.import_stack.contains(&canon) {
            return Err(self.err_here(format!("cyclic `@import` detected for `{target}`")));
        }
        let src = std::fs::read_to_string(&path)
            .map_err(|e| self.err_here(format!("failed to read `{}`: {e}", path.display())))?;
        self.import_stack.push(canon);
        let res = parse_into_sheet(sheet, &src, Some(&path), self.opts, self.import_stack);
        self.import_stack.pop();
        res
    }

    fn parse_config_body(&mut self, node: &mut ThemeNode) -> Result<(), ThemeError> {
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            let names = self.parse_selector_paths()?;
            if matches!(self.peek().kind, TokenKind::LBrace) {
                self.advance();
                let mut inner = ThemeNode::new("sub");
                self.parse_config_body(&mut inner)?;
                self.expect_rbrace()?;
                for path in names {
                    let mut target = &mut *node;
                    for seg in path {
                        target = target.find_or_create_child(&seg);
                    }
                    for (k, v) in &inner.properties {
                        target.set_property(k.clone(), v.clone());
                    }
                    for c in &inner.children {
                        target.merge_from(c.clone(), false);
                    }
                }
            } else if matches!(self.peek().kind, TokenKind::Colon) {
                if names.len() != 1 || names[0].len() != 1 {
                    return Err(self.err_here("invalid property name in configuration block"));
                }
                self.advance();
                let prop_val = self.parse_property_rhs()?;
                self.expect_semicolon()?;
                node.set_property(names[0][0].clone(), prop_val);
            } else {
                return Err(self.err_here("expected `:` or `{` in configuration block"));
            }
        }
        Ok(())
    }

    fn parse_media_block(&mut self) -> Result<ThemeNode, ThemeError> {
        let tok = self.advance();
        self.expect_lparen()?;
        let media_type = match &self.peek().kind {
            TokenKind::Ident(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => return Err(self.err_at(tok, "expected `@media` feature name")),
        };
        self.expect_colon()?;

        let cond = match media_type.to_ascii_lowercase().as_str() {
            "enabled" => {
                let b = self.parse_media_bool()?;
                MediaCondition::Enabled(b)
            }
            "monitor-id" => {
                let n = self.parse_signed_number()?;
                if matches!(&self.peek().kind, TokenKind::Ident(u) if u.eq_ignore_ascii_case("px"))
                {
                    self.advance();
                }
                MediaCondition::MonitorId(n as i32)
            }
            "min-width" => MediaCondition::MinWidth(self.parse_media_px()?),
            "max-width" => MediaCondition::MaxWidth(self.parse_media_px()?),
            "min-height" => MediaCondition::MinHeight(self.parse_media_px()?),
            "max-height" => MediaCondition::MaxHeight(self.parse_media_px()?),
            "min-aspect-ratio" => MediaCondition::MinAspectRatio(self.parse_signed_number()?),
            "max-aspect-ratio" => MediaCondition::MaxAspectRatio(self.parse_signed_number()?),
            other => return Err(self.err_here(format!("unknown `@media` condition `{other}`"))),
        };
        self.expect_rparen()?;
        self.expect_lbrace()?;

        let mut media_node = ThemeNode::new("@media");
        media_node.media = Some(cond);
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            if matches!(self.peek().kind, TokenKind::Star) {
                self.advance();
                self.expect_lbrace()?;
                let props = self.parse_property_list()?;
                self.expect_rbrace()?;
                let star = media_node.find_or_create_child("*");
                for (k, v) in props {
                    star.set_property(k, v);
                }
            } else if matches!(self.peek().kind, TokenKind::MediaDir) {
                let sub = self.parse_media_block()?;
                media_node.children.push(sub);
            } else {
                for child in self.parse_selector_block()? {
                    media_node.merge_from(child, false);
                }
            }
        }
        self.expect_rbrace()?;
        Ok(media_node)
    }

    fn parse_media_px(&mut self) -> Result<f64, ThemeError> {
        let n = self.parse_signed_number()?;
        if matches!(&self.peek().kind, TokenKind::Ident(u) if u.eq_ignore_ascii_case("px") || u.eq_ignore_ascii_case("mm"))
        {
            self.advance();
        }
        Ok(n)
    }

    fn parse_media_bool(&mut self) -> Result<bool, ThemeError> {
        match &self.peek().kind {
            TokenKind::Ident(s) if s.eq_ignore_ascii_case("true") => {
                self.advance();
                Ok(true)
            }
            TokenKind::Ident(s) if s.eq_ignore_ascii_case("false") => {
                self.advance();
                Ok(false)
            }
            TokenKind::Ident(s) if s.eq_ignore_ascii_case("dmenu") => {
                self.advance();
                Ok(self.opts.dmenu_mode)
            }
            TokenKind::Ident(s) if s.eq_ignore_ascii_case("env") => {
                self.advance();
                self.expect_lparen()?;
                let var_name = if matches!(self.peek().kind, TokenKind::Comma) {
                    None
                } else {
                    match &self.peek().kind {
                        TokenKind::Ident(v) => {
                            let v = v.clone();
                            self.advance();
                            Some(v)
                        }
                        _ => return Err(self.err_here("expected environment variable name")),
                    }
                };
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.advance();
                } else {
                    return Err(self.err_here("expected `,` in `env()`"));
                }
                let fallback = self.parse_media_bool()?;
                self.expect_rparen()?;
                if let Some(name) = var_name {
                    if let Ok(val) = std::env::var(&name) {
                        let t = val.trim();
                        if t.eq_ignore_ascii_case("true") {
                            return Ok(true);
                        }
                        if t.eq_ignore_ascii_case("false") {
                            return Ok(false);
                        }
                    }
                }
                Ok(fallback)
            }
            _ => Err(self.err_here("expected boolean in `@media` condition")),
        }
    }

    fn parse_selector_block(&mut self) -> Result<Vec<ThemeNode>, ThemeError> {
        if matches!(self.peek().kind, TokenKind::Hash) {
            self.advance();
        }
        let paths = self.parse_selector_paths()?;
        self.expect_lbrace()?;
        let props = self.parse_property_list()?;
        self.expect_rbrace()?;

        let mut roots = Vec::with_capacity(paths.len());
        for path in paths {
            let Some((first, rest)) = path.split_first() else {
                continue;
            };
            let mut root = ThemeNode::new(first.clone());
            let mut cur = &mut root;
            for seg in rest {
                cur = cur.find_or_create_child(seg);
            }
            for (k, v) in &props {
                cur.set_property(k.clone(), v.clone());
            }
            roots.push(root);
        }
        Ok(roots)
    }

    fn parse_selector_paths(&mut self) -> Result<Vec<Vec<String>>, ThemeError> {
        let mut paths = Vec::new();
        loop {
            if matches!(
                self.peek().kind,
                TokenKind::LBrace | TokenKind::Colon | TokenKind::RBrace | TokenKind::Eof
            ) {
                break;
            }
            if matches!(self.peek().kind, TokenKind::Hash) {
                self.advance();
            }
            let mut path = Vec::new();
            while !matches!(
                self.peek().kind,
                TokenKind::Comma
                    | TokenKind::LBrace
                    | TokenKind::Colon
                    | TokenKind::RBrace
                    | TokenKind::Eof
            ) {
                if matches!(self.peek().kind, TokenKind::Dot) {
                    self.advance();
                    continue;
                }
                match &self.peek().kind {
                    TokenKind::Ident(name) => {
                        path.push(name.clone());
                        self.advance();
                    }
                    TokenKind::Star => {
                        path.push("*".to_string());
                        self.advance();
                    }
                    _ => {
                        return Err(self.err_here("unexpected token in selector name"));
                    }
                }
            }
            if !path.is_empty() {
                paths.push(path);
            }
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        if paths.is_empty() {
            return Err(self.err_here("expected selector before `{`"));
        }
        Ok(paths)
    }

    fn parse_property_list(&mut self) -> Result<Vec<(String, PropertyValue)>, ThemeError> {
        let mut props = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            let prop_name = match &self.peek().kind {
                TokenKind::Ident(s) => {
                    let s = if s == "color" {
                        "text-color".to_string()
                    } else {
                        s.clone()
                    };
                    self.advance();
                    s
                }
                _ => return Err(self.err_here("expected property name")),
            };
            self.expect_colon()?;
            let val = self.parse_property_rhs()?;
            self.expect_semicolon()?;
            props.push((prop_name, val));
        }
        Ok(props)
    }

    fn parse_property_rhs(&mut self) -> Result<PropertyValue, ThemeError> {
        match &self.peek().kind {
            TokenKind::Ident(w) if w.eq_ignore_ascii_case("var") => {
                self.advance();
                self.expect_lparen()?;
                let name = match &self.peek().kind {
                    TokenKind::Ident(n) => {
                        let n = n.clone();
                        self.advance();
                        n
                    }
                    _ => return Err(self.err_here("expected variable name inside `var(...)`")),
                };
                let default = if matches!(self.peek().kind, TokenKind::Comma) {
                    self.advance();
                    Some(Box::new(self.parse_property_element()?))
                } else {
                    None
                };
                self.expect_rparen()?;
                Ok(PropertyValue::Link { name, default })
            }
            TokenKind::Ident(w) if w.eq_ignore_ascii_case("env") => {
                self.advance();
                self.expect_lparen()?;
                let (env_val, has_first) = if matches!(self.peek().kind, TokenKind::Comma) {
                    (None, false)
                } else if let TokenKind::Ident(var_name) = &self.peek().kind {
                    let var_name = var_name.clone();
                    self.advance();
                    let resolved = std::env::var(&var_name)
                        .ok()
                        .filter(|s| !s.is_empty())
                        .map(PropertyValue::String);
                    (resolved, true)
                } else {
                    let val = self.parse_property_element()?;
                    (Some(val), true)
                };
                if matches!(self.peek().kind, TokenKind::Comma) {
                    self.advance();
                    let fallback = self.parse_property_element()?;
                    self.expect_rparen()?;
                    Ok(env_val.unwrap_or(fallback))
                } else if has_first {
                    self.expect_rparen()?;
                    env_val.ok_or_else(|| self.err_here("`env(...)` requires fallback value"))
                } else {
                    Err(self.err_here("expected fallback inside `env(...)`"))
                }
            }
            _ => self.parse_property_element(),
        }
    }

    fn parse_property_element(&mut self) -> Result<PropertyValue, ThemeError> {
        // Handle `${ENV_VAR}` direct substitution
        if let TokenKind::EnvVar(name) = &self.peek().kind {
            let name = name.clone();
            let tok = self.advance();
            if let Ok(env_str) = std::env::var(&name) {
                let sub_tokens = tokenize(&env_str, self.filename)?;
                let mut dummy_stack = Vec::new();
                let mut sub_parser = Parser {
                    src: &env_str,
                    tokens: &sub_tokens,
                    pos: 0,
                    current_dir: self.current_dir.clone(),
                    filename: self.filename,
                    opts: self.opts,
                    import_stack: &mut dummy_stack,
                };
                return sub_parser.parse_property_element();
            }
            return Err(self.err_at(tok, format!("environment variable `{name}` not set")));
        }

        // Handle `inherit`
        if matches!(&self.peek().kind, TokenKind::Ident(w) if w == "inherit") {
            self.advance();
            return Ok(PropertyValue::Inherit);
        }

        // Handle booleans
        if matches!(&self.peek().kind, TokenKind::Ident(w) if w == "true" || w == "false") {
            let b = matches!(&self.peek().kind, TokenKind::Ident(w) if w == "true");
            self.advance();
            return Ok(PropertyValue::Boolean(b));
        }

        // Handle quoted strings
        if let TokenKind::String(s) = &self.peek().kind {
            let s = s.clone();
            self.advance();
            return Ok(PropertyValue::String(s));
        }

        // Handle `@link`
        if let TokenKind::Link(name) = &self.peek().kind {
            let name = name.clone();
            self.advance();
            return Ok(PropertyValue::Link {
                name,
                default: None,
            });
        }

        // Handle `[ item, item, ... ]` list
        if matches!(self.peek().kind, TokenKind::LBracket) {
            return self.parse_list_value();
        }

        // Handle `url(...)` / `linear-gradient(...)`
        if let TokenKind::Ident(w) = &self.peek().kind {
            if w.eq_ignore_ascii_case("url") {
                return self.parse_url_image();
            }
            if w.eq_ignore_ascii_case("linear-gradient") {
                return self.parse_linear_gradient();
            }
        }

        // Handle orientations / cursors
        if let TokenKind::Ident(w) = &self.peek().kind {
            match w.to_ascii_lowercase().as_str() {
                "horizontal" => {
                    self.advance();
                    return Ok(PropertyValue::Orientation(Orientation::Horizontal));
                }
                "vertical" => {
                    self.advance();
                    return Ok(PropertyValue::Orientation(Orientation::Vertical));
                }
                "default" => {
                    self.advance();
                    return Ok(PropertyValue::Cursor(CursorType::Default));
                }
                "pointer" => {
                    self.advance();
                    return Ok(PropertyValue::Cursor(CursorType::Pointer));
                }
                "text" => {
                    self.advance();
                    return Ok(PropertyValue::Cursor(CursorType::Text));
                }
                _ => {}
            }
        }

        // Handle positions (`center`, `north`, `south`, `east`, `west` & combos)
        if self.looks_like_position() {
            return self.parse_position();
        }

        // Handle highlight style + optional color (`bold italic #fff`)
        if self.looks_like_highlight_keyword() {
            return self.parse_highlight();
        }

        // Handle color specs (`#...`, `argb:...`, `transparent`, `rgb/hsl/hwb/cmyk`, named CSS colors)
        if self.looks_like_color() {
            let color = self.parse_color()?;
            return Ok(PropertyValue::Color(color));
        }

        // Handle numeric / distance / calc / 1..4-edge padding
        if self.looks_like_distance_or_number() {
            return self.parse_number_or_padding();
        }

        Err(self.err_here("unexpected property value"))
    }

    fn parse_list_value(&mut self) -> Result<PropertyValue, ThemeError> {
        self.advance(); // '['
        let mut items = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBracket | TokenKind::Eof) {
            let item = if let TokenKind::Ident(w) = &self.peek().kind {
                if !w.eq_ignore_ascii_case("true")
                    && !w.eq_ignore_ascii_case("false")
                    && !w.eq_ignore_ascii_case("inherit")
                    && !w.eq_ignore_ascii_case("calc")
                    && !w.eq_ignore_ascii_case("url")
                    && !w.eq_ignore_ascii_case("linear-gradient")
                    && !matches!(
                        self.peek_at(1).map(|t| &t.kind),
                        Some(TokenKind::LParen | TokenKind::Slash)
                    )
                    && lookup_css_color(w).is_none()
                    && !w.eq_ignore_ascii_case("transparent")
                {
                    let s = w.clone();
                    self.advance();
                    PropertyValue::String(s)
                } else if w.eq_ignore_ascii_case("calc")
                    && !matches!(self.peek_at(1).map(|t| &t.kind), Some(TokenKind::LParen))
                {
                    self.advance();
                    PropertyValue::String("calc".to_string())
                } else {
                    self.parse_property_element()?
                }
            } else {
                self.parse_property_element()?
            };
            items.push(item);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        if !matches!(self.peek().kind, TokenKind::RBracket) {
            return Err(self.err_here("expected `]` to close list"));
        }
        self.advance();
        Ok(PropertyValue::List(items))
    }

    fn parse_url_image(&mut self) -> Result<PropertyValue, ThemeError> {
        self.advance(); // 'url'
        self.expect_lparen()?;
        let url = match &self.peek().kind {
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => return Err(self.err_here("expected quoted URL inside `url(...)`")),
        };
        let scale = if matches!(self.peek().kind, TokenKind::Comma) {
            self.advance();
            match &self.peek().kind {
                TokenKind::Ident(w) => {
                    let s = match w.to_ascii_lowercase().as_str() {
                        "both" => ImageScale::Both,
                        "width" => ImageScale::Width,
                        "height" => ImageScale::Height,
                        "none" => ImageScale::None,
                        other => {
                            return Err(self.err_here(format!("unknown `url()` scale `{other}`")));
                        }
                    };
                    self.advance();
                    s
                }
                _ => return Err(self.err_here("expected scaling keyword inside `url(...)`")),
            }
        } else {
            ImageScale::None
        };
        self.expect_rparen()?;
        Ok(PropertyValue::Image(ImageValue::Url { url, scale }))
    }

    fn parse_linear_gradient(&mut self) -> Result<PropertyValue, ThemeError> {
        self.advance(); // 'linear-gradient'
        self.expect_lparen()?;
        let mut dir = GradientDirection::Right;
        if matches!(&self.peek().kind, TokenKind::Ident(w) if w.eq_ignore_ascii_case("to")) {
            self.advance();
            dir = match &self.peek().kind {
                TokenKind::Ident(w) => {
                    let d = match w.to_ascii_lowercase().as_str() {
                        "left" => GradientDirection::Left,
                        "right" => GradientDirection::Right,
                        "top" => GradientDirection::Top,
                        "bottom" => GradientDirection::Bottom,
                        other => {
                            return Err(
                                self.err_here(format!("unknown gradient direction `{other}`"))
                            )
                        }
                    };
                    self.advance();
                    d
                }
                _ => return Err(self.err_here("expected direction after `to`")),
            };
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
            } else {
                return Err(self.err_here("expected `,` after gradient direction"));
            }
        } else if self.looks_like_angle_prefix() {
            let turn = self.parse_angle_in_turns()?;
            dir = GradientDirection::Angle(turn * 360.0);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
            } else {
                return Err(self.err_here("expected `,` after gradient angle"));
            }
        }

        let mut colors = Vec::new();
        loop {
            colors.push(self.parse_color()?);
            if matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect_rparen()?;
        Ok(PropertyValue::Image(ImageValue::LinearGradient {
            dir,
            colors,
        }))
    }

    fn looks_like_angle_prefix(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Int(_) | TokenKind::Float(_))
    }

    fn looks_like_position(&self) -> bool {
        matches!(
            &self.peek().kind,
            TokenKind::Ident(w)
                if {
                    let lower = w.to_ascii_lowercase();
                    lower == "center" || position_bits(&lower).is_some()
                }
        )
    }

    fn parse_position(&mut self) -> Result<PropertyValue, ThemeError> {
        let mut mask = 0u8;
        let first = match &self.peek().kind {
            TokenKind::Ident(w) => w.to_ascii_lowercase(),
            _ => return Err(self.err_here("expected position")),
        };
        if first == "center" {
            self.advance();
            return Ok(PropertyValue::Position(WindowLocation::Center));
        }
        let first_bits = position_bits(&first).ok_or_else(|| self.err_here("invalid position"))?;
        mask |= first_bits;
        self.advance();
        let has_vert = first_bits & (1 | 4) != 0;
        let has_horiz = first_bits & (2 | 8) != 0;
        if !(has_vert && has_horiz) {
            if let TokenKind::Ident(second) = &self.peek().kind {
                let s = second.to_ascii_lowercase();
                if let Some(bits) = position_bits(&s) {
                    if (first_bits & (1 | 4) != 0 && bits & (2 | 8) != 0)
                        || (first_bits & (2 | 8) != 0 && bits & (1 | 4) != 0)
                    {
                        mask |= bits;
                        self.advance();
                    }
                }
            }
        }
        Ok(PropertyValue::Position(WindowLocation::from_u8(mask)))
    }

    fn looks_like_highlight_keyword(&self) -> bool {
        matches!(
            &self.peek().kind,
            TokenKind::Ident(w)
                if matches!(
                    w.to_ascii_lowercase().as_str(),
                    "none"
                        | "bold"
                        | "underline"
                        | "strikethrough"
                        | "italic"
                        | "uppercase"
                        | "lowercase"
                        | "capitalize"
                )
        )
    }

    fn parse_highlight(&mut self) -> Result<PropertyValue, ThemeError> {
        let mut style = highlight_flags::NONE;
        while let TokenKind::Ident(w) = &self.peek().kind {
            let flag = match w.to_ascii_lowercase().as_str() {
                "none" => highlight_flags::NONE,
                "bold" => highlight_flags::BOLD,
                "underline" => highlight_flags::UNDERLINE,
                "strikethrough" => highlight_flags::STRIKETHROUGH,
                "italic" => highlight_flags::ITALIC,
                "uppercase" => highlight_flags::UPPERCASE,
                "lowercase" => highlight_flags::LOWERCASE,
                "capitalize" => highlight_flags::CAPITALIZE,
                _ => break,
            };
            style |= flag;
            self.advance();
        }
        let mut color = ThemeColor::BLACK;
        if self.looks_like_color() {
            color = self.parse_color()?;
            style |= highlight_flags::COLOR;
        }
        Ok(PropertyValue::Highlight(HighlightColorStyle {
            style,
            color,
        }))
    }

    fn looks_like_color(&self) -> bool {
        match &self.peek().kind {
            TokenKind::Hash => true,
            TokenKind::Ident(w) => {
                let lw = w.to_ascii_lowercase();
                if lw == "transparent"
                    || matches!(
                        lw.as_str(),
                        "rgb" | "rgba" | "hsl" | "hsla" | "hwb" | "cmyk"
                    )
                    || lookup_css_color(w).is_some()
                {
                    return true;
                }
                lw == "argb" && matches!(self.peek_at(1).map(|t| &t.kind), Some(TokenKind::Colon))
            }
            _ => false,
        }
    }

    fn parse_color(&mut self) -> Result<ThemeColor, ThemeError> {
        if matches!(self.peek().kind, TokenKind::Hash) {
            let start_tok = self.advance();
            let mut end = start_tok.end;
            while matches!(self.peek().kind, TokenKind::Int(_) | TokenKind::Ident(_))
                && self.peek().start == end
            {
                end = self.advance().end;
            }
            let raw = &self.src[start_tok.start..end];
            return parse_hex_color(raw).map_err(|m| self.err_at(start_tok, m));
        }

        let ident_tok = self.peek().clone();
        let word = match &ident_tok.kind {
            TokenKind::Ident(w) => w.clone(),
            _ => return Err(self.err_at(&ident_tok, "expected color")),
        };
        let lw = word.to_ascii_lowercase();

        if lw == "argb" && matches!(self.peek_at(1).map(|t| &t.kind), Some(TokenKind::Colon)) {
            let start = self.advance().start;
            let mut end = self.advance().end; // ':'
            while matches!(self.peek().kind, TokenKind::Int(_) | TokenKind::Ident(_))
                && self.peek().start == end
            {
                end = self.advance().end;
            }
            let raw = &self.src[start..end];
            return parse_hex_color(raw).map_err(|m| self.err_at(&ident_tok, m));
        }

        if lw == "transparent" {
            self.advance();
            return Ok(ThemeColor::TRANSPARENT);
        }

        if lw == "rgb" || lw == "rgba" {
            self.advance();
            return self.parse_rgba();
        }
        if lw == "hsl" || lw == "hsla" {
            self.advance();
            return self.parse_hsl();
        }
        if lw == "hwb" {
            self.advance();
            return self.parse_hwb();
        }
        if lw == "cmyk" {
            self.advance();
            return self.parse_cmyk();
        }

        if let Some(c) = lookup_css_color(&word) {
            self.advance();
            let alpha = self.parse_opt_alpha_ws()?;
            return Ok(c.with_alpha(alpha));
        }

        Err(self.err_at(&ident_tok, format!("unknown color `{word}`")))
    }

    fn parse_rgba(&mut self) -> Result<ThemeColor, ThemeError> {
        self.expect_lparen()?;
        let v1 = self.parse_color_num()?;
        let is_pct = matches!(self.peek().kind, TokenKind::Percent);
        if is_pct {
            self.advance();
        }
        let comma_sep = matches!(self.peek().kind, TokenKind::Comma);
        if comma_sep {
            self.advance();
        }
        let v2 = self.parse_color_num()?;
        if is_pct {
            if !matches!(self.peek().kind, TokenKind::Percent) {
                return Err(self.err_here("expected `%` for green channel"));
            }
            self.advance();
        }
        if comma_sep {
            if !matches!(self.peek().kind, TokenKind::Comma) {
                return Err(self.err_here("expected `,` in `rgba(...)`"));
            }
            self.advance();
        }
        let v3 = self.parse_color_num()?;
        if is_pct {
            if !matches!(self.peek().kind, TokenKind::Percent) {
                return Err(self.err_here("expected `%` for blue channel"));
            }
            self.advance();
        }

        let alpha = if comma_sep {
            self.parse_opt_alpha_comma()?
        } else {
            self.parse_opt_alpha_ws()?
        };
        self.expect_rparen()?;

        if is_pct {
            self.check_range(v1, 0.0, 100.0)?;
            self.check_range(v2, 0.0, 100.0)?;
            self.check_range(v3, 0.0, 100.0)?;
            Ok(ThemeColor::new(v1 / 100.0, v2 / 100.0, v3 / 100.0, alpha))
        } else {
            self.check_range(v1, 0.0, 255.0)?;
            self.check_range(v2, 0.0, 255.0)?;
            self.check_range(v3, 0.0, 255.0)?;
            Ok(ThemeColor::new(v1 / 255.0, v2 / 255.0, v3 / 255.0, alpha))
        }
    }

    fn parse_hsl(&mut self) -> Result<ThemeColor, ThemeError> {
        self.expect_lparen()?;
        let h = self.parse_angle_in_turns()?;
        let comma_sep = matches!(self.peek().kind, TokenKind::Comma);
        if comma_sep {
            self.advance();
        }
        let s = self.parse_color_unit()?;
        if comma_sep {
            if !matches!(self.peek().kind, TokenKind::Comma) {
                return Err(self.err_here("expected `,` in `hsl(...)`"));
            }
            self.advance();
        }
        let l = self.parse_color_unit()?;
        let a = if comma_sep {
            self.parse_opt_alpha_comma()?
        } else {
            self.parse_opt_alpha_ws()?
        };
        self.expect_rparen()?;
        Ok(hsl_to_rgb(h, s, l, a))
    }

    fn parse_hwb(&mut self) -> Result<ThemeColor, ThemeError> {
        self.expect_lparen()?;
        let h = self.parse_angle_in_turns()?;
        let comma_sep = matches!(self.peek().kind, TokenKind::Comma);
        if comma_sep {
            self.advance();
        }
        let w = self.parse_color_unit()?;
        if comma_sep {
            if !matches!(self.peek().kind, TokenKind::Comma) {
                return Err(self.err_here("expected `,` in `hwb(...)`"));
            }
            self.advance();
        }
        let b = self.parse_color_unit()?;
        let a = if comma_sep {
            self.parse_opt_alpha_comma()?
        } else {
            self.parse_opt_alpha_ws()?
        };
        self.expect_rparen()?;
        Ok(hwb_to_rgb(h, w, b, a))
    }

    fn parse_cmyk(&mut self) -> Result<ThemeColor, ThemeError> {
        self.expect_lparen()?;
        let c = self.parse_color_unit()?;
        let comma_sep = matches!(self.peek().kind, TokenKind::Comma);
        if comma_sep {
            self.advance();
        }
        let m = self.parse_color_unit()?;
        if comma_sep {
            if !matches!(self.peek().kind, TokenKind::Comma) {
                return Err(self.err_here("expected `,` in `cmyk(...)`"));
            }
            self.advance();
        }
        let y = self.parse_color_unit()?;
        if comma_sep {
            if !matches!(self.peek().kind, TokenKind::Comma) {
                return Err(self.err_here("expected `,` in `cmyk(...)`"));
            }
            self.advance();
        }
        let k = self.parse_color_unit()?;
        let a = if comma_sep {
            self.parse_opt_alpha_comma()?
        } else {
            self.parse_opt_alpha_ws()?
        };
        self.expect_rparen()?;
        Ok(cmyk_to_rgb(c, m, y, k, a))
    }

    fn parse_color_num(&mut self) -> Result<f64, ThemeError> {
        match &self.peek().kind {
            TokenKind::Int(i) => {
                let v = f64::from(*i);
                self.advance();
                Ok(v)
            }
            TokenKind::Float(f) => {
                let v = *f;
                self.advance();
                Ok(v)
            }
            _ => Err(self.err_here("expected color numeric value")),
        }
    }

    fn parse_color_unit(&mut self) -> Result<f64, ThemeError> {
        let v = self.parse_color_num()?;
        if matches!(self.peek().kind, TokenKind::Percent) {
            self.advance();
            self.check_range(v, 0.0, 100.0)?;
            Ok(v / 100.0)
        } else {
            self.check_range(v, 0.0, 1.0)?;
            Ok(v)
        }
    }

    fn parse_angle_in_turns(&mut self) -> Result<f64, ThemeError> {
        let v = self.parse_color_num()?;
        if let TokenKind::Ident(unit) = &self.peek().kind {
            let lu = unit.to_ascii_lowercase();
            match lu.as_str() {
                "deg" => {
                    self.advance();
                    self.check_range(v, 0.0, 360.0)?;
                    return Ok(v / 360.0);
                }
                "rad" => {
                    self.advance();
                    self.check_range(v, 0.0, 2.0 * PI)?;
                    return Ok(v / (2.0 * PI));
                }
                "grad" => {
                    self.advance();
                    self.check_range(v, 0.0, 400.0)?;
                    return Ok(v / 400.0);
                }
                "turn" => {
                    self.advance();
                    self.check_range(v, 0.0, 1.0)?;
                    return Ok(v);
                }
                _ => {}
            }
        }
        self.check_range(v, 0.0, 360.0)?;
        Ok(v / 360.0)
    }

    fn parse_opt_alpha_comma(&mut self) -> Result<f64, ThemeError> {
        if matches!(self.peek().kind, TokenKind::Comma) {
            self.advance();
            self.parse_color_unit()
        } else {
            Ok(1.0)
        }
    }

    fn parse_opt_alpha_ws(&mut self) -> Result<f64, ThemeError> {
        if matches!(self.peek().kind, TokenKind::Slash) {
            self.advance();
            self.parse_color_unit()
        } else {
            Ok(1.0)
        }
    }

    fn check_range(&self, v: f64, lo: f64, hi: f64) -> Result<(), ThemeError> {
        if v < lo || v > hi {
            Err(self.err_here(format!("value {v} out of range [{lo}, {hi}]")))
        } else {
            Ok(())
        }
    }

    fn looks_like_distance_or_number(&self) -> bool {
        match &self.peek().kind {
            TokenKind::Int(_) | TokenKind::Float(_) | TokenKind::Minus => true,
            TokenKind::Ident(w) if w.eq_ignore_ascii_case("calc") => true,
            _ => false,
        }
    }

    fn parse_signed_number(&mut self) -> Result<f64, ThemeError> {
        let neg = if matches!(self.peek().kind, TokenKind::Minus) {
            self.advance();
            true
        } else {
            false
        };
        let val = match &self.peek().kind {
            TokenKind::Int(i) => {
                let v = f64::from(*i);
                self.advance();
                v
            }
            TokenKind::Float(f) => {
                let v = *f;
                self.advance();
                v
            }
            _ => return Err(self.err_here("expected number")),
        };
        Ok(if neg { -val } else { val })
    }

    fn parse_number_or_padding(&mut self) -> Result<PropertyValue, ThemeError> {
        // Look at first token to distinguish bare Integer/Double when no unit, no line-style,
        // and no second distance follows.
        if !matches!(&self.peek().kind, TokenKind::Ident(w) if w.eq_ignore_ascii_case("calc")) {
            let saved = self.pos;
            let neg = if matches!(self.peek().kind, TokenKind::Minus) {
                self.advance();
                true
            } else {
                false
            };
            let bare_opt = match &self.peek().kind {
                TokenKind::Int(i) => {
                    let v = if neg { -*i } else { *i };
                    self.advance();
                    Some(PropertyValue::Integer(v))
                }
                TokenKind::Float(f) => {
                    let v = if neg { -*f } else { *f };
                    self.advance();
                    Some(PropertyValue::Double(v))
                }
                _ => None,
            };
            if let Some(bare) = bare_opt {
                let next_starts_second_dist = self.looks_like_distance_or_number();
                let next_is_unit_or_style = matches!(self.peek().kind, TokenKind::Percent)
                    || matches!(
                        &self.peek().kind,
                        TokenKind::Ident(w)
                            if matches!(
                                w.to_ascii_lowercase().as_str(),
                                "px" | "mm" | "em" | "ch" | "solid" | "dash"
                            )
                    );
                if !next_starts_second_dist && !next_is_unit_or_style {
                    return Ok(bare);
                }
            }
            self.pos = saved;
        }

        let mut items = Vec::with_capacity(4);
        items.push(self.parse_distance_zero()?);
        for _ in 0..3 {
            if self.looks_like_distance_or_number() {
                items.push(self.parse_distance_zero()?);
            } else {
                break;
            }
        }
        let padding = match items.as_slice() {
            [a] => Padding::all(a.clone()),
            [tb, lr] => Padding {
                top: tb.clone(),
                right: lr.clone(),
                bottom: tb.clone(),
                left: lr.clone(),
            },
            [t, lr, b] => Padding {
                top: t.clone(),
                right: lr.clone(),
                bottom: b.clone(),
                left: lr.clone(),
            },
            [t, r, b, l] => Padding {
                top: t.clone(),
                right: r.clone(),
                bottom: b.clone(),
                left: l.clone(),
            },
            _ => unreachable!(),
        };
        Ok(PropertyValue::Padding(padding))
    }

    fn parse_distance_zero(&mut self) -> Result<Distance, ThemeError> {
        if matches!(&self.peek().kind, TokenKind::Ident(w) if w.eq_ignore_ascii_case("calc")) {
            self.advance();
            self.expect_lparen()?;
            let inner = self.parse_calc_minmax()?;
            self.expect_rparen()?;
            let style = self.parse_opt_line_style();
            return Ok(Distance {
                expr: DistanceExpr::Group(Box::new(inner)),
                style,
            });
        }
        let val = self.parse_signed_number()?;
        let unit = self.parse_opt_unit().unwrap_or(DistanceUnit::Px);
        let style = self.parse_opt_line_style();
        Ok(Distance::new(val, unit, style))
    }

    fn parse_opt_unit(&mut self) -> Option<DistanceUnit> {
        if matches!(self.peek().kind, TokenKind::Percent) {
            self.advance();
            return Some(DistanceUnit::Percent);
        }
        if let TokenKind::Ident(w) = &self.peek().kind {
            let u = match w.to_ascii_lowercase().as_str() {
                "px" => Some(DistanceUnit::Px),
                "mm" => Some(DistanceUnit::Mm),
                "em" => Some(DistanceUnit::Em),
                "ch" => Some(DistanceUnit::Ch),
                _ => None,
            };
            if u.is_some() {
                self.advance();
                return u;
            }
        }
        None
    }

    fn parse_opt_line_style(&mut self) -> LineStyle {
        if let TokenKind::Ident(w) = &self.peek().kind {
            if w.eq_ignore_ascii_case("dash") {
                self.advance();
                return LineStyle::Dash;
            }
            if w.eq_ignore_ascii_case("solid") {
                self.advance();
                return LineStyle::Solid;
            }
        }
        LineStyle::Solid
    }

    fn parse_calc_minmax(&mut self) -> Result<DistanceExpr, ThemeError> {
        let mut lhs = self.parse_calc_addsub()?;
        while let TokenKind::Ident(op) = &self.peek().kind {
            let kind = op.to_ascii_lowercase();
            let make = match kind.as_str() {
                "min" => DistanceExpr::Min,
                "max" => DistanceExpr::Max,
                "round" => DistanceExpr::Round,
                "floor" => DistanceExpr::Floor,
                "ceil" => DistanceExpr::Ceil,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_calc_addsub()?;
            lhs = make(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_calc_addsub(&mut self) -> Result<DistanceExpr, ThemeError> {
        let mut lhs = self.parse_calc_muldiv()?;
        loop {
            match &self.peek().kind {
                TokenKind::Plus => {
                    self.advance();
                    let rhs = self.parse_calc_muldiv()?;
                    lhs = DistanceExpr::Add(Box::new(lhs), Box::new(rhs));
                }
                TokenKind::Minus => {
                    self.advance();
                    let rhs = self.parse_calc_muldiv()?;
                    lhs = DistanceExpr::Sub(Box::new(lhs), Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_calc_muldiv(&mut self) -> Result<DistanceExpr, ThemeError> {
        let mut lhs = self.parse_calc_atom()?;
        loop {
            match &self.peek().kind {
                TokenKind::Star => {
                    self.advance();
                    let rhs = self.parse_calc_atom()?;
                    lhs = DistanceExpr::Mul(Box::new(lhs), Box::new(rhs));
                }
                TokenKind::Slash => {
                    self.advance();
                    let rhs = self.parse_calc_atom()?;
                    lhs = DistanceExpr::Div(Box::new(lhs), Box::new(rhs));
                }
                TokenKind::Ident(op) if op == "modulo" => {
                    self.advance();
                    let rhs = self.parse_calc_atom()?;
                    lhs = DistanceExpr::Modulo(Box::new(lhs), Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_calc_atom(&mut self) -> Result<DistanceExpr, ThemeError> {
        if matches!(self.peek().kind, TokenKind::LParen) {
            self.advance();
            let inner = self.parse_calc_minmax()?;
            self.expect_rparen()?;
            return Ok(DistanceExpr::Group(Box::new(inner)));
        }
        let val = self.parse_signed_number()?;
        let unit = self.parse_opt_unit().unwrap_or(DistanceUnit::Px);
        Ok(DistanceExpr::Value(val, unit))
    }
}

fn position_bits(s: &str) -> Option<u8> {
    match s {
        "north" => Some(1),
        "east" => Some(2),
        "south" => Some(4),
        "west" => Some(8),
        "northeast" | "eastnorth" | "north-east" | "east-north" => Some(1 | 2),
        "southeast" | "eastsouth" | "south-east" | "east-south" => Some(4 | 2),
        "southwest" | "westsouth" | "south-west" | "west-south" => Some(4 | 8),
        "northwest" | "westnorth" | "north-west" | "west-north" => Some(1 | 8),
        _ => None,
    }
}
