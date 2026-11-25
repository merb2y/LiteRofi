//! Tokenized matching engines, scoring algorithms (Levenshtein, Rofi Fuzzy,
//! FZF v2), and UTF-8 / accent-normalization utilities ported from `source/helper.c`.

use crate::config::MatchingMethod;
use regex::{Regex, RegexBuilder};
use unicode_general_category::{get_general_category, GeneralCategory};
use unicode_normalization::UnicodeNormalization;

/// Max input code point length supported by Rofi fuzzy & fzf-v2 scorers (`source/helper.c:898`).
pub const FUZZY_SCORER_MAX_LENGTH: usize = 256;
pub const ROFI_FUZZY_MIN_SCORE: i32 = i32::MIN / 2;
pub const FZF_V2_MIN_SCORE: i32 = i32::MIN / 2;

const LEADING_GAP_SCORE: i32 = -4;
const GAP_SCORE: i32 = -5;
const WORD_START_SCORE: i32 = 50;
const NON_WORD_SCORE: i32 = 40;
const CAMEL_SCORE: i32 = WORD_START_SCORE + GAP_SCORE - 1;
const CONSECUTIVE_SCORE: i32 = WORD_START_SCORE + GAP_SCORE;
const PATTERN_NON_START_MULTIPLIER: i32 = 1;
const PATTERN_START_MULTIPLIER: i32 = 2;

const FZF_V2_SCORE_MATCH: i32 = 16;
const FZF_V2_SCORE_GAP_START: i32 = -3;
const FZF_V2_SCORE_GAP_EXTENSION: i32 = -1;
const FZF_V2_BONUS_BOUNDARY: i32 = 8;
const FZF_V2_BONUS_NON_WORD: i32 = 8;
const FZF_V2_BONUS_CAMEL_123: i32 = FZF_V2_BONUS_BOUNDARY + FZF_V2_SCORE_GAP_EXTENSION;
const FZF_V2_BONUS_CONSECUTIVE: i32 = -(FZF_V2_SCORE_GAP_START + FZF_V2_SCORE_GAP_EXTENSION);
const FZF_V2_BONUS_FIRST_CHAR_MULTIPLIER: i32 = 2;
const FZF_V2_BONUS_BOUNDARY_WHITE: i32 = FZF_V2_BONUS_BOUNDARY + 2;
const FZF_V2_BONUS_BOUNDARY_DELIMITER: i32 = FZF_V2_BONUS_BOUNDARY + 1;

/// Configuration controlling tokenization and matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchOptions {
    pub method: MatchingMethod,
    pub case_sensitive: bool,
    pub tokenize: bool,
    pub negate_char: char,
    pub normalize_match: bool,
}

impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            method: MatchingMethod::Normal,
            case_sensitive: false,
            tokenize: true,
            negate_char: '-',
            normalize_match: false,
        }
    }
}

/// A compiled single search token (optionally negated).
#[derive(Debug, Clone)]
pub struct TokenMatcher {
    pub regex: Regex,
    pub invert: bool,
}

/// NFKD-normalizes text and strips Unicode combining mark characters (`utf8_helper_simplify_string`).
pub fn simplify_utf8(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.nfkd() {
        let cat = get_general_category(ch);
        if !matches!(
            cat,
            GeneralCategory::NonspacingMark
                | GeneralCategory::SpacingMark
                | GeneralCategory::EnclosingMark
        ) {
            out.push(ch);
        }
    }
    out
}

/// Converts Latin-1 (ISO-8859-1) bytes directly to UTF-8 (`rofi_latin_to_utf8_strdup`).
pub fn rofi_latin_to_utf8(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        out.push(char::from(b));
    }
    out
}

/// Lossy UTF-8 decoder replacing each invalid byte/sequence with `\u{FFFD}`,
/// matching GLib's `rofi_force_utf8`.
pub fn rofi_force_utf8(mut bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(valid) => {
                out.push_str(valid);
                break;
            }
            Err(err) => {
                let valid_len = err.valid_up_to();
                if valid_len > 0 {
                    if let Ok(prefix) = std::str::from_utf8(&bytes[..valid_len]) {
                        out.push_str(prefix);
                    }
                }
                out.push('\u{FFFD}');
                let step = err.error_len().unwrap_or(1);
                bytes = &bytes[valid_len + step..];
            }
        }
    }
    out
}

/// Determines effective case sensitivity accounting for `-case-smart` (`parse_case_sensitivity`).
pub fn parse_case_sensitivity(input: &str, case_sensitive: bool, case_smart: bool) -> bool {
    if case_smart {
        input.chars().any(char::is_uppercase)
    } else {
        case_sensitive
    }
}

fn compile_regex(pattern: &str, case_sensitive: bool, normalize_match: bool) -> Option<Regex> {
    let owned;
    let effective = if normalize_match {
        owned = simplify_utf8(pattern);
        owned.as_str()
    } else {
        pattern
    };
    RegexBuilder::new(effective)
        .case_insensitive(!case_sensitive)
        .build()
        .ok()
}

fn glob_to_regex(input: &str) -> String {
    let escaped = regex::escape(input);
    let mut out = String::with_capacity(escaped.len() + 8);
    let mut chars = escaped.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek().copied() {
                Some('*') => {
                    chars.next();
                    out.push_str(".*");
                }
                Some('?') => {
                    chars.next();
                    out.push_str(r"\S");
                }
                Some(next) => {
                    chars.next();
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn fuzzy_to_regex(input: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx > 0 {
            out.push_str(".*?");
        }
        out.push('(');
        let mut buf = [0u8; 4];
        out.push_str(&regex::escape(ch.encode_utf8(&mut buf)));
        out.push(')');
    }
    out
}

fn prefix_to_regex(input: &str) -> String {
    format!(r"\b{}", regex::escape(input))
}

fn create_token(token: &str, opts: &MatchOptions) -> TokenMatcher {
    let mut invert = false;
    let mut slice = token;
    if opts.negate_char != '\0' && slice.starts_with(opts.negate_char) {
        invert = true;
        slice = &slice[opts.negate_char.len_utf8()..];
    }

    let regex = match opts.method {
        MatchingMethod::Glob => {
            let pat = glob_to_regex(slice);
            compile_regex(&pat, opts.case_sensitive, opts.normalize_match)
        }
        MatchingMethod::Regex => compile_regex(slice, opts.case_sensitive, opts.normalize_match)
            .or_else(|| {
                let esc = regex::escape(slice);
                compile_regex(&esc, opts.case_sensitive, opts.normalize_match)
            }),
        MatchingMethod::Fuzzy => {
            let pat = fuzzy_to_regex(slice);
            compile_regex(&pat, opts.case_sensitive, opts.normalize_match)
        }
        MatchingMethod::Prefix => {
            let pat = prefix_to_regex(slice);
            compile_regex(&pat, opts.case_sensitive, opts.normalize_match)
        }
        MatchingMethod::Normal => {
            let pat = regex::escape(slice);
            compile_regex(&pat, opts.case_sensitive, opts.normalize_match)
        }
    }
    .unwrap_or_else(|| {
        // Guaranteed valid literal fallback regex.
        RegexBuilder::new(&regex::escape(slice))
            .case_insensitive(!opts.case_sensitive)
            .build()
            .unwrap_or_else(|_| Regex::new("a^").expect("constant non-matching regex"))
    });

    TokenMatcher { regex, invert }
}

/// Tokenizes `input` according to `opts` (`helper_tokenize`). Empty input yields an empty slice.
pub fn tokenize(input: &str, opts: &MatchOptions) -> Vec<TokenMatcher> {
    if input.is_empty() {
        return Vec::new();
    }
    if !opts.tokenize {
        return vec![create_token(input, opts)];
    }
    input
        .split(' ')
        .filter(|part| !part.is_empty())
        .map(|part| create_token(part, opts))
        .collect()
}

/// Checks whether `input` matches all compiled tokens (`helper_token_match`).
pub fn token_match(tokens: &[TokenMatcher], input: &str, normalize_match: bool) -> bool {
    if tokens.is_empty() {
        return true;
    }
    let normalized;
    let hay = if normalize_match {
        normalized = simplify_utf8(input);
        normalized.as_str()
    } else {
        input
    };
    for tok in tokens {
        let matched = tok.regex.is_match(hay) ^ tok.invert;
        if !matched {
            return false;
        }
    }
    true
}

/// Returns byte ranges of matched substrings/captures for UI highlighting.
pub fn token_match_ranges(
    tokens: &[TokenMatcher],
    input: &str,
    normalize_match: bool,
) -> Vec<(usize, usize)> {
    if tokens.is_empty() || normalize_match {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    for tok in tokens {
        if tok.invert {
            continue;
        }
        if let Some(caps) = tok.regex.captures(input) {
            if caps.len() > 1 {
                for m in caps.iter().skip(1).flatten() {
                    ranges.push((m.start(), m.end()));
                }
            } else if let Some(m) = caps.get(0) {
                ranges.push((m.start(), m.end()));
            }
        }
    }
    ranges
}

/// Unicode character-based Levenshtein edit distance (`source/helper.c:819`).
pub fn levenshtein(needle: &str, haystack: &str, case_sensitive: bool) -> u32 {
    let n_chars: Vec<char> = needle
        .chars()
        .map(|c| {
            if case_sensitive {
                c
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        })
        .collect();
    let h_chars: Vec<char> = haystack
        .chars()
        .map(|c| {
            if case_sensitive {
                c
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        })
        .collect();

    let n_len = n_chars.len();
    let mut column: Vec<u32> = (0..=n_len as u32).collect();

    for (x_idx, &hc) in h_chars.iter().enumerate() {
        let x = (x_idx + 1) as u32;
        column[0] = x;
        let mut lastdiag = x - 1;
        for (y_idx, &nc) in n_chars.iter().enumerate() {
            let y = y_idx + 1;
            let olddiag = column[y];
            let cost = u32::from(nc != hc);
            column[y] = (column[y] + 1).min(column[y - 1] + 1).min(lastdiag + cost);
            lastdiag = olddiag;
        }
    }
    column[n_len]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RofiCharClass {
    Lower,
    Upper,
    Digit,
    NonWord,
}

fn rofi_char_class(c: char) -> RofiCharClass {
    if c.is_lowercase() {
        RofiCharClass::Lower
    } else if c.is_uppercase() {
        RofiCharClass::Upper
    } else if c.is_ascii_digit() || c.is_numeric() {
        RofiCharClass::Digit
    } else {
        RofiCharClass::NonWord
    }
}

fn rofi_transition_score(prev: RofiCharClass, curr: RofiCharClass) -> i32 {
    if prev == RofiCharClass::NonWord && curr != RofiCharClass::NonWord {
        WORD_START_SCORE
    } else if (prev == RofiCharClass::Lower && curr == RofiCharClass::Upper)
        || (prev != RofiCharClass::Digit && curr == RofiCharClass::Digit)
    {
        CAMEL_SCORE
    } else if curr == RofiCharClass::NonWord {
        NON_WORD_SCORE
    } else {
        0
    }
}

fn chars_equal_fold(a: char, b: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        a == b
    } else {
        a.to_lowercase().eq(b.to_lowercase())
    }
}

/// Rofi's native fuzzy scorer (`rofi_scorer_fuzzy_evaluate`, lower is better).
pub fn fuzzy_score_rofi(pattern: &str, text: &str, case_sensitive: bool) -> i32 {
    let s_chars: Vec<char> = text.chars().collect();
    let slen = s_chars.len();
    if slen > FUZZY_SCORER_MAX_LENGTH {
        return -ROFI_FUZZY_MIN_SCORE;
    }

    let mut score = Vec::with_capacity(slen);
    let mut dp = vec![ROFI_FUZZY_MIN_SCORE; slen];
    let mut prev = RofiCharClass::NonWord;
    for &sc in &s_chars {
        let cur = rofi_char_class(sc);
        score.push(rofi_transition_score(prev, cur));
        prev = cur;
    }

    let mut pfirst = true;
    let mut pstart = true;
    let mut uleft = 0i32;
    let mut ulefts = 0i32;

    for pc in pattern.chars() {
        if pc.is_whitespace() {
            pstart = true;
            continue;
        }
        let mut lefts = ROFI_FUZZY_MIN_SCORE;
        for (si, &sc) in s_chars.iter().enumerate() {
            let left = dp[si];
            lefts = (lefts + GAP_SCORE).max(left);
            if chars_equal_fold(pc, sc, case_sensitive) {
                let t = score[si]
                    * if pstart {
                        PATTERN_START_MULTIPLIER
                    } else {
                        PATTERN_NON_START_MULTIPLIER
                    };
                dp[si] = if pfirst {
                    LEADING_GAP_SCORE * (si as i32) + t
                } else {
                    (uleft + CONSECUTIVE_SCORE).max(ulefts + t)
                };
            } else {
                dp[si] = ROFI_FUZZY_MIN_SCORE;
            }
            uleft = left;
            ulefts = lefts;
        }
        pfirst = false;
        pstart = false;
    }

    let mut best = ROFI_FUZZY_MIN_SCORE;
    for val in dp {
        best = (best + GAP_SCORE).max(val);
    }
    -best
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FzfV2CharClass {
    White = 0,
    NonWord = 1,
    Delimiter = 2,
    Lower = 3,
    Upper = 4,
    Letter = 5,
    Number = 6,
}

fn fzf_v2_char_class(c: char) -> FzfV2CharClass {
    if c.is_lowercase() {
        FzfV2CharClass::Lower
    } else if c.is_uppercase() {
        FzfV2CharClass::Upper
    } else if c.is_numeric() {
        FzfV2CharClass::Number
    } else if c.is_alphabetic() {
        FzfV2CharClass::Letter
    } else if c.is_whitespace() {
        FzfV2CharClass::White
    } else if matches!(c, '/' | ',' | ':' | ';' | '|') {
        FzfV2CharClass::Delimiter
    } else {
        FzfV2CharClass::NonWord
    }
}

fn fzf_v2_bonus_for(prev: FzfV2CharClass, cur: FzfV2CharClass) -> i32 {
    if cur >= FzfV2CharClass::NonWord {
        match prev {
            FzfV2CharClass::White => return FZF_V2_BONUS_BOUNDARY_WHITE,
            FzfV2CharClass::Delimiter => return FZF_V2_BONUS_BOUNDARY_DELIMITER,
            FzfV2CharClass::NonWord => return FZF_V2_BONUS_BOUNDARY,
            _ => {}
        }
    }
    if (prev == FzfV2CharClass::Lower && cur == FzfV2CharClass::Upper)
        || (prev != FzfV2CharClass::Number && cur == FzfV2CharClass::Number)
    {
        return FZF_V2_BONUS_CAMEL_123;
    }
    match cur {
        FzfV2CharClass::NonWord | FzfV2CharClass::Delimiter => FZF_V2_BONUS_NON_WORD,
        FzfV2CharClass::White => FZF_V2_BONUS_BOUNDARY_WHITE,
        _ => 0,
    }
}

fn fold_char(c: char, case_sensitive: bool) -> char {
    if case_sensitive {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

fn fzf_v2_term(pat: &[char], tmatch: &[char], bonuses: &[i32], case_sensitive: bool) -> i32 {
    let m = pat.len();
    let n = tmatch.len();
    if m == 0 {
        return 0;
    }
    if m > n {
        return FZF_V2_MIN_SCORE;
    }

    let mut h = vec![0i32; m * n];
    let mut c_mat = vec![0i32; m * n];
    let mut f = vec![0usize; m];

    let pchar0 = fold_char(pat[0], case_sensitive);
    let mut prev_h0 = 0i32;
    let mut in_gap = false;
    let mut pidx = 0usize;
    let mut last_idx = 0usize;
    let mut pchar_adv = pchar0;
    let mut max_score = 0i32;

    for off in 0..n {
        if tmatch[off] == pchar_adv {
            if pidx < m {
                f[pidx] = off;
                pidx += 1;
                pchar_adv = fold_char(pat[pidx.min(m - 1)], case_sensitive);
            }
            last_idx = off;
        }
        if tmatch[off] == pchar0 {
            let bonus = bonuses[off];
            let sc = FZF_V2_SCORE_MATCH + bonus * FZF_V2_BONUS_FIRST_CHAR_MULTIPLIER;
            h[off] = sc;
            c_mat[off] = 1;
            if m == 1 && sc > max_score {
                max_score = sc;
                if bonus >= FZF_V2_BONUS_BOUNDARY {
                    break;
                }
            }
            in_gap = false;
        } else {
            let v = prev_h0
                + if in_gap {
                    FZF_V2_SCORE_GAP_EXTENSION
                } else {
                    FZF_V2_SCORE_GAP_START
                };
            h[off] = v.max(0);
            c_mat[off] = 0;
            in_gap = true;
        }
        prev_h0 = h[off];
    }

    if pidx != m {
        return FZF_V2_MIN_SCORE;
    }
    if m == 1 {
        return max_score;
    }

    for i in 1..m {
        let pchar = fold_char(pat[i], case_sensitive);
        let first_j = f[i];
        in_gap = false;
        let row_off = i * n;
        let prev_off = (i - 1) * n;
        for j in first_j..=last_idx {
            let mut s1 = 0i32;
            let mut consecutive = 0i32;
            let hleft = if j > first_j { h[row_off + j - 1] } else { 0 };
            let s2 = hleft
                + if in_gap {
                    FZF_V2_SCORE_GAP_EXTENSION
                } else {
                    FZF_V2_SCORE_GAP_START
                };
            if pchar == tmatch[j] {
                let mut b = bonuses[j];
                s1 = h[prev_off + j - 1] + FZF_V2_SCORE_MATCH;
                consecutive = c_mat[prev_off + j - 1] + 1;
                if consecutive > 1 {
                    let fb = bonuses[j + 1 - (consecutive as usize)];
                    if b >= FZF_V2_BONUS_BOUNDARY && b > fb {
                        consecutive = 1;
                    } else {
                        b = b.max(FZF_V2_BONUS_CONSECUTIVE).max(fb);
                    }
                }
                if s1 + b < s2 {
                    s1 += bonuses[j];
                    consecutive = 0;
                } else {
                    s1 += b;
                }
            }
            c_mat[row_off + j] = consecutive;
            in_gap = s1 < s2;
            let sc = s1.max(s2).max(0);
            if i == m - 1 && sc > max_score {
                max_score = sc;
            }
            h[row_off + j] = sc;
        }
    }
    max_score
}

/// FZF `FuzzyMatchV2` port (`rofi_scorer_fzf_v2_evaluate`, higher is better; `FZF_V2_MIN_SCORE` on miss).
pub fn fuzzy_score_fzf_v2(
    pattern: &str,
    text: &str,
    case_sensitive: bool,
    normalize_match: bool,
) -> i32 {
    let npat;
    let ntxt;
    let (eff_pat, eff_txt) = if normalize_match {
        npat = simplify_utf8(pattern);
        ntxt = simplify_utf8(text);
        (npat.as_str(), ntxt.as_str())
    } else {
        (pattern, text)
    };

    let txt: Vec<char> = eff_txt.chars().collect();
    if txt.len() > FUZZY_SCORER_MAX_LENGTH {
        return FZF_V2_MIN_SCORE;
    }

    let mut tmatch = Vec::with_capacity(txt.len());
    let mut bonuses = Vec::with_capacity(txt.len());
    let mut prev_class = FzfV2CharClass::White;
    for &ch in &txt {
        let c = fzf_v2_char_class(ch);
        tmatch.push(fold_char(ch, case_sensitive));
        bonuses.push(fzf_v2_bonus_for(prev_class, c));
        prev_class = c;
    }

    let mut total = 0i32;
    let mut any_term = false;
    for term in eff_pat.split([' ', '\t', '\n', '\r', '\x0b', '\x0c']) {
        if term.is_empty() {
            continue;
        }
        any_term = true;
        let pat: Vec<char> = term.chars().collect();
        let sc = fzf_v2_term(&pat, &tmatch, &bonuses, case_sensitive);
        if sc == FZF_V2_MIN_SCORE {
            return FZF_V2_MIN_SCORE;
        }
        total += sc;
    }
    if any_term {
        total
    } else {
        0
    }
}

/// Packs `(score, trimmed_codepoint_len)` into an ascending integer sort key
/// (`rofi_scorer_fzf_v2_sort_key`).
pub fn fzf_v2_sort_key(score: i32, text: &str) -> i32 {
    let clamped_score = score.clamp(0, i32::from(u16::MAX)) as u32;
    let by_score = u32::from(u16::MAX) - clamped_score;
    let trimmed_len = text.trim().chars().count().min(0x7FFF) as u32;
    ((by_score << 15) | trimmed_len) as i32
}
