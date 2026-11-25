//! `.rasi` tokenizer preserving exact byte spans, line/column positions,
//! C++ `//` comments, and nested `/* /* ... */ */` block comments.

use crate::types::ThemeError;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Int(i32),
    Float(f64),
    String(String),
    Link(String),   // @name
    EnvVar(String), // ${VAR}
    Import,         // @import
    OptImport,      // ?import
    ThemeDir,       // @theme
    MediaDir,       // @media
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Semicolon,
    Comma,
    Dot,
    Hash,
    Star,
    Plus,
    Minus,
    Slash,
    Percent,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

pub fn tokenize(src: &str, filename: Option<&str>) -> Result<Vec<Token>, ThemeError> {
    let src = src.strip_prefix('\u{FEFF}').unwrap_or(src);
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut tokens = Vec::new();

    let err = |l: usize, c: usize, msg: String| ThemeError {
        file: filename.map(str::to_string),
        line: l,
        col: c,
        message: msg,
    };

    while i < len {
        let b = bytes[i];
        // Skip whitespace
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
            continue;
        }

        // Line comment //
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
            i += 2;
            col += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
                col += 1;
            }
            continue;
        }

        // Nested block comment /* ... */ (unclosed at EOF accepted cleanly per upstream)
        if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
            i += 2;
            col += 2;
            let mut depth = 1usize;
            while i < len && depth > 0 {
                if bytes[i] == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                    col += 2;
                } else if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                    col += 2;
                } else if bytes[i] == b'\n' {
                    line += 1;
                    col = 1;
                    i += 1;
                } else {
                    let ch = src[i..].chars().next().unwrap_or('\0');
                    i += ch.len_utf8();
                    col += 1;
                }
            }
            continue;
        }

        let start = i;
        let tline = line;
        let tcol = col;

        // Quoted string literals
        if b == b'"' || b == b'\'' {
            let quote = b;
            i += 1;
            col += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < len {
                let c = bytes[i];
                if c == quote {
                    i += 1;
                    col += 1;
                    closed = true;
                    break;
                }
                if c == b'\\' && i + 1 < len {
                    i += 1;
                    col += 1;
                    let esc = bytes[i];
                    let ch = match esc {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\x08',
                        b'f' => '\x0c',
                        b'\\' => '\\',
                        b'"' => '"',
                        b'\'' => '\'',
                        other => char::from(other),
                    };
                    value.push(ch);
                    i += 1;
                    col += 1;
                } else if c == b'\n' {
                    return Err(err(
                        tline,
                        tcol,
                        "newline inside string literal".to_string(),
                    ));
                } else {
                    let ch = src[i..].chars().next().unwrap_or('\0');
                    value.push(ch);
                    i += ch.len_utf8();
                    col += 1;
                }
            }
            if !closed {
                return Err(err(tline, tcol, "unterminated string literal".to_string()));
            }
            tokens.push(Token {
                kind: TokenKind::String(value),
                start,
                end: i,
                line: tline,
                col: tcol,
            });
            continue;
        }

        // Directives & links starting with '@'
        if b == b'@' {
            i += 1;
            col += 1;
            let wstart = i;
            while i < len && is_word_cont(bytes[i]) {
                i += 1;
                col += 1;
            }
            let word = &src[wstart..i];
            if word.is_empty() {
                return Err(err(tline, tcol, "unexpected `@`".to_string()));
            }
            let kind = match word.to_ascii_lowercase().as_str() {
                "import" => TokenKind::Import,
                "theme" => TokenKind::ThemeDir,
                "media" => TokenKind::MediaDir,
                _ => TokenKind::Link(word.to_string()),
            };
            tokens.push(Token {
                kind,
                start,
                end: i,
                line: tline,
                col: tcol,
            });
            continue;
        }

        // Optional import `?import`
        if b == b'?' && src[i..].len() >= 7 && src[i..i + 7].eq_ignore_ascii_case("?import") {
            let after = bytes.get(i + 7).copied().unwrap_or(b' ');
            if !is_word_cont(after) {
                i += 7;
                col += 7;
                tokens.push(Token {
                    kind: TokenKind::OptImport,
                    start,
                    end: i,
                    line: tline,
                    col: tcol,
                });
                continue;
            }
        }

        // Environment variable `${NAME}`
        if b == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
            if let Some(rel_close) = src[i + 2..].find('}') {
                let name = &src[i + 2..i + 2 + rel_close];
                if !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                {
                    let adv = 2 + rel_close + 1;
                    i += adv;
                    col += adv;
                    tokens.push(Token {
                        kind: TokenKind::EnvVar(name.to_string()),
                        start,
                        end: i,
                        line: tline,
                        col: tcol,
                    });
                    continue;
                }
            }
        }

        // Numbers (`123`, `12.34`) - note `.` alone is Dot unless digits follow immediately `.5`
        if b.is_ascii_digit() || (b == b'.' && i + 1 < len && bytes[i + 1].is_ascii_digit()) {
            let mut has_dot = false;
            if b == b'.' {
                has_dot = true;
                i += 1;
                col += 1;
            }
            while i < len && bytes[i].is_ascii_digit() {
                i += 1;
                col += 1;
            }
            if !has_dot && i + 1 < len && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
                has_dot = true;
                i += 1;
                col += 1;
                while i < len && bytes[i].is_ascii_digit() {
                    i += 1;
                    col += 1;
                }
            }
            let raw = &src[start..i];
            let kind = if has_dot {
                let v = raw
                    .parse::<f64>()
                    .map_err(|_| err(tline, tcol, format!("invalid float `{raw}`")))?;
                TokenKind::Float(v)
            } else {
                let v = raw
                    .parse::<i32>()
                    .map_err(|_| err(tline, tcol, format!("invalid integer `{raw}`")))?;
                TokenKind::Int(v)
            };
            tokens.push(Token {
                kind,
                start,
                end: i,
                line: tline,
                col: tcol,
            });
            continue;
        }

        // Words / Identifiers
        if is_word_start(b) {
            while i < len && is_word_cont(bytes[i]) {
                i += 1;
                col += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Ident(src[start..i].to_string()),
                start,
                end: i,
                line: tline,
                col: tcol,
            });
            continue;
        }

        // Single-character punctuation
        let kind = match b {
            b'{' => TokenKind::LBrace,
            b'}' => TokenKind::RBrace,
            b'(' => TokenKind::LParen,
            b')' => TokenKind::RParen,
            b'[' => TokenKind::LBracket,
            b']' => TokenKind::RBracket,
            b':' => TokenKind::Colon,
            b';' => TokenKind::Semicolon,
            b',' => TokenKind::Comma,
            b'.' => TokenKind::Dot,
            b'#' => TokenKind::Hash,
            b'*' => TokenKind::Star,
            b'+' => TokenKind::Plus,
            b'-' => TokenKind::Minus,
            b'/' => TokenKind::Slash,
            b'%' => TokenKind::Percent,
            other => {
                return Err(err(
                    tline,
                    tcol,
                    format!("unexpected character `{}`", char::from(other)),
                ));
            }
        };
        i += 1;
        col += 1;
        tokens.push(Token {
            kind,
            start,
            end: i,
            line: tline,
            col: tcol,
        });
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        start: len,
        end: len,
        line,
        col,
    });
    Ok(tokens)
}

fn is_word_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_word_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}
