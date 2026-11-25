//! Core theme AST types (`include/rofi-types.h`, `source/theme.c`).

use crate::colors::ThemeColor;
use literofi_core::config::WindowLocation;
use std::fmt;

/// Error produced when lexing, parsing, or importing `.rasi` themes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeError {
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{file}:{}:{}: {}", self.line, self.col, self.message),
            None => write!(f, "line {}:{} : {}", self.line, self.col, self.message),
        }
    }
}

impl std::error::Error for ThemeError {}

/// Distance unit (`RofiPixelUnit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DistanceUnit {
    #[default]
    Px,
    Mm,
    Em,
    Ch,
    Percent,
}

/// Border/separator stroke style (`RofiLineStyle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineStyle {
    #[default]
    Solid,
    Dash,
}

/// Recursive `calc(...)` expression tree (`RofiDistanceUnit`).
#[derive(Debug, Clone, PartialEq)]
pub enum DistanceExpr {
    Value(f64, DistanceUnit),
    Group(Box<Self>),
    Add(Box<Self>, Box<Self>),
    Sub(Box<Self>, Box<Self>),
    Mul(Box<Self>, Box<Self>),
    Div(Box<Self>, Box<Self>),
    Modulo(Box<Self>, Box<Self>),
    Min(Box<Self>, Box<Self>),
    Max(Box<Self>, Box<Self>),
    Round(Box<Self>, Box<Self>),
    Floor(Box<Self>, Box<Self>),
    Ceil(Box<Self>, Box<Self>),
}

/// Font & screen geometry needed to convert `em`/`ch`/`mm`/`%` into pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistanceMetrics {
    pub monitor_width: f64,
    pub monitor_height: f64,
    pub char_height: f64,
    pub char_width: f64,
    pub dpi: f64,
}

impl Default for DistanceMetrics {
    fn default() -> Self {
        Self {
            monitor_width: 1920.0,
            monitor_height: 1080.0,
            char_height: 16.0,
            char_width: 8.0,
            dpi: 96.0,
        }
    }
}

/// Layout axis (`RofiOrientation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

/// Single edge or size distance paired with stroke style (`RofiDistance`).
#[derive(Debug, Clone, PartialEq)]
pub struct Distance {
    pub expr: DistanceExpr,
    pub style: LineStyle,
}

impl Distance {
    pub const fn px(v: f64) -> Self {
        Self {
            expr: DistanceExpr::Value(v, DistanceUnit::Px),
            style: LineStyle::Solid,
        }
    }

    pub const fn new(v: f64, unit: DistanceUnit, style: LineStyle) -> Self {
        Self {
            expr: DistanceExpr::Value(v, unit),
            style,
        }
    }

    /// Evaluates the distance expression in pixels (`distance_get_pixel`).
    pub fn to_pixels(&self, ori: Orientation, metrics: &DistanceMetrics) -> f64 {
        eval_expr(&self.expr, ori, metrics)
    }

    /// Formats the distance in `.rasi` syntax.
    pub fn format_rasi(&self) -> String {
        let mut s = match &self.expr {
            DistanceExpr::Group(inner) => format!("calc( {} )", format_expr(inner)),
            _ => format_expr(&self.expr),
        };
        if self.style == LineStyle::Dash {
            s.push_str(" dash");
        }
        s
    }
}

fn eval_expr(expr: &DistanceExpr, ori: Orientation, m: &DistanceMetrics) -> f64 {
    match expr {
        DistanceExpr::Value(val, unit) => match unit {
            DistanceUnit::Px => *val,
            DistanceUnit::Mm => val * m.dpi / 25.4,
            DistanceUnit::Em => val * m.char_height,
            DistanceUnit::Ch => val * m.char_width,
            DistanceUnit::Percent => match ori {
                Orientation::Vertical => (val * m.monitor_height) / 100.0,
                Orientation::Horizontal => (val * m.monitor_width) / 100.0,
            },
        },
        DistanceExpr::Group(inner) => eval_expr(inner, ori, m),
        DistanceExpr::Add(a, b) => eval_expr(a, ori, m) + eval_expr(b, ori, m),
        DistanceExpr::Sub(a, b) => eval_expr(a, ori, m) - eval_expr(b, ori, m),
        DistanceExpr::Mul(a, b) => eval_expr(a, ori, m) * eval_expr(b, ori, m),
        DistanceExpr::Div(a, b) => {
            let av = eval_expr(a, ori, m);
            let bv = eval_expr(b, ori, m);
            if bv != 0.0 {
                av / bv
            } else {
                av
            }
        }
        DistanceExpr::Modulo(a, b) => {
            let av = eval_expr(a, ori, m);
            let bv = eval_expr(b, ori, m);
            if bv != 0.0 {
                av % bv
            } else {
                0.0
            }
        }
        DistanceExpr::Min(a, b) => eval_expr(a, ori, m).min(eval_expr(b, ori, m)),
        DistanceExpr::Max(a, b) => eval_expr(a, ori, m).max(eval_expr(b, ori, m)),
        DistanceExpr::Round(a, b) => {
            let av = eval_expr(a, ori, m);
            let bv = eval_expr(b, ori, m);
            if bv != 0.0 {
                (av / bv).round() * bv
            } else {
                av
            }
        }
        DistanceExpr::Floor(a, b) => {
            let av = eval_expr(a, ori, m);
            let bv = eval_expr(b, ori, m);
            if bv != 0.0 {
                (av / bv).floor() * bv
            } else {
                av
            }
        }
        DistanceExpr::Ceil(a, b) => {
            let av = eval_expr(a, ori, m);
            let bv = eval_expr(b, ori, m);
            if bv != 0.0 {
                (av / bv).ceil() * bv
            } else {
                av
            }
        }
    }
}

fn format_expr(expr: &DistanceExpr) -> String {
    match expr {
        DistanceExpr::Value(v, unit) => {
            let u = match unit {
                DistanceUnit::Px => "px",
                DistanceUnit::Mm => "mm",
                DistanceUnit::Em => "em",
                DistanceUnit::Ch => "ch",
                DistanceUnit::Percent => "%",
            };
            format!("{v}{u}")
        }
        DistanceExpr::Group(inner) => format!("( {} )", format_expr(inner)),
        DistanceExpr::Add(a, b) => format!("{} + {}", format_expr(a), format_expr(b)),
        DistanceExpr::Sub(a, b) => format!("{} - {}", format_expr(a), format_expr(b)),
        DistanceExpr::Mul(a, b) => format!("{} * {}", format_expr(a), format_expr(b)),
        DistanceExpr::Div(a, b) => format!("{} / {}", format_expr(a), format_expr(b)),
        DistanceExpr::Modulo(a, b) => format!("{} modulo {}", format_expr(a), format_expr(b)),
        DistanceExpr::Min(a, b) => format!("{} min {}", format_expr(a), format_expr(b)),
        DistanceExpr::Max(a, b) => format!("{} max {}", format_expr(a), format_expr(b)),
        DistanceExpr::Round(a, b) => format!("{} round {}", format_expr(a), format_expr(b)),
        DistanceExpr::Floor(a, b) => format!("{} floor {}", format_expr(a), format_expr(b)),
        DistanceExpr::Ceil(a, b) => format!("{} ceil {}", format_expr(a), format_expr(b)),
    }
}

/// Four-edge box distance (`RofiPadding`).
#[derive(Debug, Clone, PartialEq)]
pub struct Padding {
    pub top: Distance,
    pub right: Distance,
    pub bottom: Distance,
    pub left: Distance,
}

impl Padding {
    pub fn all(d: Distance) -> Self {
        Self {
            top: d.clone(),
            right: d.clone(),
            bottom: d.clone(),
            left: d,
        }
    }
}

/// Mouse cursor shape (`RofiCursorType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorType {
    #[default]
    Default,
    Pointer,
    Text,
}

/// Highlight style bitflags (`RofiHighlightStyle`).
pub mod highlight_flags {
    pub const NONE: u8 = 0;
    pub const BOLD: u8 = 1;
    pub const UNDERLINE: u8 = 2;
    pub const ITALIC: u8 = 4;
    pub const COLOR: u8 = 8;
    pub const STRIKETHROUGH: u8 = 16;
    pub const UPPERCASE: u8 = 32;
    pub const LOWERCASE: u8 = 64;
    pub const CAPITALIZE: u8 = 128;
}

/// Text highlight styling (`RofiHighlightColorStyle`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighlightColorStyle {
    pub style: u8,
    pub color: ThemeColor,
}

impl Default for HighlightColorStyle {
    fn default() -> Self {
        Self {
            style: highlight_flags::NONE,
            color: ThemeColor::BLACK,
        }
    }
}

/// Scaling mode for `url(...)` background images (`RofiScaleType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageScale {
    #[default]
    None,
    Both,
    Height,
    Width,
}

/// Gradient direction (`RofiDirection`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientDirection {
    Right,
    Left,
    Top,
    Bottom,
    Angle(f64),
}

/// Background image property (`RofiImage`).
#[derive(Debug, Clone, PartialEq)]
pub enum ImageValue {
    Url {
        url: String,
        scale: ImageScale,
    },
    LinearGradient {
        dir: GradientDirection,
        colors: Vec<ThemeColor>,
    },
}

/// Conditional `@media` predicate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaCondition {
    MinWidth(f64),
    MaxWidth(f64),
    MinHeight(f64),
    MaxHeight(f64),
    MonitorId(i32),
    MinAspectRatio(f64),
    MaxAspectRatio(f64),
    Enabled(bool),
}

/// Monitor geometry passed into `@media` resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorGeometry {
    pub width: i32,
    pub height: i32,
    pub monitor_id: i32,
}

impl Default for MonitorGeometry {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            monitor_id: 0,
        }
    }
}

/// Typed `.rasi` property value (`Property`).
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Integer(i32),
    Double(f64),
    String(String),
    Boolean(bool),
    Color(ThemeColor),
    Image(ImageValue),
    Padding(Padding),
    Link {
        name: String,
        default: Option<Box<PropertyValue>>,
    },
    Position(WindowLocation),
    Highlight(HighlightColorStyle),
    List(Vec<PropertyValue>),
    Orientation(Orientation),
    Cursor(CursorType),
    Inherit,
}
