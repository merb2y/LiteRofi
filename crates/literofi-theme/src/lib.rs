//! Rofi `.rasi` theme lexer, parser, cascade engine, and `-dump-theme` support.

pub mod colors;
pub mod lexer;
pub mod parser;
pub mod sheet;
pub mod types;

pub use colors::{
    cmyk_to_rgb, hsl_to_rgb, hwb_to_rgb, lookup_css_color, parse_hex_color, ThemeColor, CSS_COLORS,
};
pub use parser::ParseOptions;
pub use sheet::{ThemeNode, ThemeSheet, WidgetQuery, DEFAULT_THEME_RASI};
pub use types::{
    highlight_flags, CursorType, Distance, DistanceExpr, DistanceMetrics, DistanceUnit,
    GradientDirection, HighlightColorStyle, ImageScale, ImageValue, LineStyle, MediaCondition,
    MonitorGeometry, Orientation, Padding, PropertyValue, ThemeError,
};
