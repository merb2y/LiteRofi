//! CSS named colors (`source/css-colors.c`) and color space conversions
//! (`lexer/theme-parser.y`: hex, argb, rgb/rgba, hsl/hsla, hwb, cmyk).

use std::fmt;

/// RGBA color with normalized channels in `0.0..=1.0` (`ThemeColor`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl ThemeColor {
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    pub fn with_alpha(mut self, a: f64) -> Self {
        self.a = a.clamp(0.0, 1.0);
        self
    }

    pub fn to_rgba_u8(self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }
}

impl fmt::Display for ThemeColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, _] = self.to_rgba_u8();
        let a_pct = (self.a.clamp(0.0, 1.0) * 100.0).round() as u32;
        write!(f, "rgba ( {r}, {g}, {b}, {a_pct} % )")
    }
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex_byte(hi: u8, lo: u8) -> Option<f64> {
    let h = hex_nibble(hi)?;
    let l = hex_nibble(lo)?;
    Some(f64::from((h << 4) | l) / 255.0)
}

/// Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, or `argb:AARRGGBB`.
pub fn parse_hex_color(spec: &str) -> Result<ThemeColor, String> {
    if let Some(rest) = spec
        .strip_prefix("argb:")
        .or_else(|| spec.strip_prefix("ARGB:"))
    {
        let b = rest.as_bytes();
        if b.len() != 8 {
            return Err(format!(
                "invalid argb color `{spec}`: requires 8 hex digits (AARRGGBB)"
            ));
        }
        let a = hex_byte(b[0], b[1]).ok_or_else(|| format!("invalid hex in `{spec}`"))?;
        let r = hex_byte(b[2], b[3]).ok_or_else(|| format!("invalid hex in `{spec}`"))?;
        let g = hex_byte(b[4], b[5]).ok_or_else(|| format!("invalid hex in `{spec}`"))?;
        let bl = hex_byte(b[6], b[7]).ok_or_else(|| format!("invalid hex in `{spec}`"))?;
        return Ok(ThemeColor::new(r, g, bl, a));
    }

    let hex = spec
        .strip_prefix('#')
        .ok_or_else(|| format!("expected hex color starting with `#`, got `{spec}`"))?;
    let b = hex.as_bytes();
    match b.len() {
        3 => Ok(ThemeColor::new(
            hex_byte(b[0], b[0]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[1], b[1]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[2], b[2]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            1.0,
        )),
        4 => Ok(ThemeColor::new(
            hex_byte(b[0], b[0]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[1], b[1]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[2], b[2]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[3], b[3]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
        )),
        6 => Ok(ThemeColor::new(
            hex_byte(b[0], b[1]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[2], b[3]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[4], b[5]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            1.0,
        )),
        8 => Ok(ThemeColor::new(
            hex_byte(b[0], b[1]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[2], b[3]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[4], b[5]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
            hex_byte(b[6], b[7]).ok_or_else(|| format!("invalid hex in `{spec}`"))?,
        )),
        _ => Err(format!("invalid hex color `{spec}`")),
    }
}

fn hue2rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

/// Converts HSL (`h_turn` in `0.0..=1.0`, `s`, `l`, `a` in `0.0..=1.0`) matching `theme-parser.y`.
pub fn hsl_to_rgb(h_turn: f64, s: f64, l: f64, a: f64) -> ThemeColor {
    if s.abs() < 0.001 {
        ThemeColor::new(l, l, l, a)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        ThemeColor::new(
            hue2rgb(p, q, h_turn + 1.0 / 3.0),
            hue2rgb(p, q, h_turn),
            hue2rgb(p, q, h_turn - 1.0 / 3.0),
            a,
        )
    }
}

/// Converts HWB (`h_turn`, `w`, `b`, `a` in `0.0..=1.0`) matching `theme-parser.y`.
pub fn hwb_to_rgb(h_turn: f64, w: f64, b: f64, a: f64) -> ThemeColor {
    let base = hsl_to_rgb(h_turn, 1.0, 0.5, a);
    let factor = 1.0 - w - b;
    ThemeColor::new(
        base.r * factor + w,
        base.g * factor + w,
        base.b * factor + w,
        a,
    )
}

/// Converts CMYK (`c`, `m`, `y`, `k`, `a` in `0.0..=1.0`) matching `theme-parser.y`.
pub fn cmyk_to_rgb(c: f64, m: f64, y: f64, k: f64, a: f64) -> ThemeColor {
    ThemeColor::new(
        (1.0 - c) * (1.0 - k),
        (1.0 - m) * (1.0 - k),
        (1.0 - y) * (1.0 - k),
        a,
    )
}

/// Case-insensitive lookup across the 148 CSS named colors from `source/css-colors.c`.
pub fn lookup_css_color(name: &str) -> Option<ThemeColor> {
    CSS_COLORS
        .iter()
        .find(|&&(n, _, _, _)| n.eq_ignore_ascii_case(name))
        .map(|&(_, r, g, b)| {
            ThemeColor::new(
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
                1.0,
            )
        })
}

/// Complete 148 CSS named colors from `reference/rofi/source/css-colors.c`.
pub const CSS_COLORS: &[(&str, u8, u8, u8)] = &[
    ("AliceBlue", 0xF0, 0xF8, 0xFF),
    ("AntiqueWhite", 0xFA, 0xEB, 0xD7),
    ("Aqua", 0x00, 0xFF, 0xFF),
    ("Aquamarine", 0x7F, 0xFF, 0xD4),
    ("Azure", 0xF0, 0xFF, 0xFF),
    ("Beige", 0xF5, 0xF5, 0xDC),
    ("Bisque", 0xFF, 0xE4, 0xC4),
    ("Black", 0x00, 0x00, 0x00),
    ("BlanchedAlmond", 0xFF, 0xEB, 0xCD),
    ("Blue", 0x00, 0x00, 0xFF),
    ("BlueViolet", 0x8A, 0x2B, 0xE2),
    ("Brown", 0xA5, 0x2A, 0x2A),
    ("BurlyWood", 0xDE, 0xB8, 0x87),
    ("CadetBlue", 0x5F, 0x9E, 0xA0),
    ("Chartreuse", 0x7F, 0xFF, 0x00),
    ("Chocolate", 0xD2, 0x69, 0x1E),
    ("Coral", 0xFF, 0x7F, 0x50),
    ("CornflowerBlue", 0x64, 0x95, 0xED),
    ("Cornsilk", 0xFF, 0xF8, 0xDC),
    ("Crimson", 0xDC, 0x14, 0x3C),
    ("Cyan", 0x00, 0xFF, 0xFF),
    ("DarkBlue", 0x00, 0x00, 0x8B),
    ("DarkCyan", 0x00, 0x8B, 0x8B),
    ("DarkGoldenRod", 0xB8, 0x86, 0x0B),
    ("DarkGray", 0xA9, 0xA9, 0xA9),
    ("DarkGrey", 0xA9, 0xA9, 0xA9),
    ("DarkGreen", 0x00, 0x64, 0x00),
    ("DarkKhaki", 0xBD, 0xB7, 0x6B),
    ("DarkMagenta", 0x8B, 0x00, 0x8B),
    ("DarkOliveGreen", 0x55, 0x6B, 0x2F),
    ("DarkOrange", 0xFF, 0x8C, 0x00),
    ("DarkOrchid", 0x99, 0x32, 0xCC),
    ("DarkRed", 0x8B, 0x00, 0x00),
    ("DarkSalmon", 0xE9, 0x96, 0x7A),
    ("DarkSeaGreen", 0x8F, 0xBC, 0x8F),
    ("DarkSlateBlue", 0x48, 0x3D, 0x8B),
    ("DarkSlateGray", 0x2F, 0x4F, 0x4F),
    ("DarkSlateGrey", 0x2F, 0x4F, 0x4F),
    ("DarkTurquoise", 0x00, 0xCE, 0xD1),
    ("DarkViolet", 0x94, 0x00, 0xD3),
    ("DeepPink", 0xFF, 0x14, 0x93),
    ("DeepSkyBlue", 0x00, 0xBF, 0xFF),
    ("DimGray", 0x69, 0x69, 0x69),
    ("DimGrey", 0x69, 0x69, 0x69),
    ("DodgerBlue", 0x1E, 0x90, 0xFF),
    ("FireBrick", 0xB2, 0x22, 0x22),
    ("FloralWhite", 0xFF, 0xFA, 0xF0),
    ("ForestGreen", 0x22, 0x8B, 0x22),
    ("Fuchsia", 0xFF, 0x00, 0xFF),
    ("Gainsboro", 0xDC, 0xDC, 0xDC),
    ("GhostWhite", 0xF8, 0xF8, 0xFF),
    ("Gold", 0xFF, 0xD7, 0x00),
    ("GoldenRod", 0xDA, 0xA5, 0x20),
    ("Gray", 0x80, 0x80, 0x80),
    ("Grey", 0x80, 0x80, 0x80),
    ("Green", 0x00, 0x80, 0x00),
    ("GreenYellow", 0xAD, 0xFF, 0x2F),
    ("HoneyDew", 0xF0, 0xFF, 0xF0),
    ("HotPink", 0xFF, 0x69, 0xB4),
    ("IndianRed", 0xCD, 0x5C, 0x5C),
    ("Indigo", 0x4B, 0x00, 0x82),
    ("Ivory", 0xFF, 0xFF, 0xF0),
    ("Khaki", 0xF0, 0xE6, 0x8C),
    ("Lavender", 0xE6, 0xE6, 0xFA),
    ("LavenderBlush", 0xFF, 0xF0, 0xF5),
    ("LawnGreen", 0x7C, 0xFC, 0x00),
    ("LemonChiffon", 0xFF, 0xFA, 0xCD),
    ("LightBlue", 0xAD, 0xD8, 0xE6),
    ("LightCoral", 0xF0, 0x80, 0x80),
    ("LightCyan", 0xE0, 0xFF, 0xFF),
    ("LightGoldenRodYellow", 0xFA, 0xFA, 0xD2),
    ("LightGray", 0xD3, 0xD3, 0xD3),
    ("LightGrey", 0xD3, 0xD3, 0xD3),
    ("LightGreen", 0x90, 0xEE, 0x90),
    ("LightPink", 0xFF, 0xB6, 0xC1),
    ("LightSalmon", 0xFF, 0xA0, 0x7A),
    ("LightSeaGreen", 0x20, 0xB2, 0xAA),
    ("LightSkyBlue", 0x87, 0xCE, 0xFA),
    ("LightSlateGray", 0x77, 0x88, 0x99),
    ("LightSlateGrey", 0x77, 0x88, 0x99),
    ("LightSteelBlue", 0xB0, 0xC4, 0xDE),
    ("LightYellow", 0xFF, 0xFF, 0xE0),
    ("Lime", 0x00, 0xFF, 0x00),
    ("LimeGreen", 0x32, 0xCD, 0x32),
    ("Linen", 0xFA, 0xF0, 0xE6),
    ("Magenta", 0xFF, 0x00, 0xFF),
    ("Maroon", 0x80, 0x00, 0x00),
    ("MediumAquaMarine", 0x66, 0xCD, 0xAA),
    ("MediumBlue", 0x00, 0x00, 0xCD),
    ("MediumOrchid", 0xBA, 0x55, 0xD3),
    ("MediumPurple", 0x93, 0x70, 0xDB),
    ("MediumSeaGreen", 0x3C, 0xB3, 0x71),
    ("MediumSlateBlue", 0x7B, 0x68, 0xEE),
    ("MediumSpringGreen", 0x00, 0xFA, 0x9A),
    ("MediumTurquoise", 0x48, 0xD1, 0xCC),
    ("MediumVioletRed", 0xC7, 0x15, 0x85),
    ("MidnightBlue", 0x19, 0x19, 0x70),
    ("MintCream", 0xF5, 0xFF, 0xFA),
    ("MistyRose", 0xFF, 0xE4, 0xE1),
    ("Moccasin", 0xFF, 0xE4, 0xB5),
    ("NavajoWhite", 0xFF, 0xDE, 0xAD),
    ("Navy", 0x00, 0x00, 0x80),
    ("OldLace", 0xFD, 0xF5, 0xE6),
    ("Olive", 0x80, 0x80, 0x00),
    ("OliveDrab", 0x6B, 0x8E, 0x23),
    ("Orange", 0xFF, 0xA5, 0x00),
    ("OrangeRed", 0xFF, 0x45, 0x00),
    ("Orchid", 0xDA, 0x70, 0xD6),
    ("PaleGoldenRod", 0xEE, 0xE8, 0xAA),
    ("PaleGreen", 0x98, 0xFB, 0x98),
    ("PaleTurquoise", 0xAF, 0xEE, 0xEE),
    ("PaleVioletRed", 0xDB, 0x70, 0x93),
    ("PapayaWhip", 0xFF, 0xEF, 0xD5),
    ("PeachPuff", 0xFF, 0xDA, 0xB9),
    ("Peru", 0xCD, 0x85, 0x3F),
    ("Pink", 0xFF, 0xC0, 0xCB),
    ("Plum", 0xDD, 0xA0, 0xDD),
    ("PowderBlue", 0xB0, 0xE0, 0xE6),
    ("Purple", 0x80, 0x00, 0x80),
    ("RebeccaPurple", 0x66, 0x33, 0x99),
    ("Red", 0xFF, 0x00, 0x00),
    ("RosyBrown", 0xBC, 0x8F, 0x8F),
    ("RoyalBlue", 0x41, 0x69, 0xE1),
    ("SaddleBrown", 0x8B, 0x45, 0x13),
    ("Salmon", 0xFA, 0x80, 0x72),
    ("SandyBrown", 0xF4, 0xA4, 0x60),
    ("SeaGreen", 0x2E, 0x8B, 0x57),
    ("SeaShell", 0xFF, 0xF5, 0xEE),
    ("Sienna", 0xA0, 0x52, 0x2D),
    ("Silver", 0xC0, 0xC0, 0xC0),
    ("SkyBlue", 0x87, 0xCE, 0xEB),
    ("SlateBlue", 0x6A, 0x5A, 0xCD),
    ("SlateGray", 0x70, 0x80, 0x90),
    ("SlateGrey", 0x70, 0x80, 0x90),
    ("Snow", 0xFF, 0xFA, 0xFA),
    ("SpringGreen", 0x00, 0xFF, 0x7F),
    ("SteelBlue", 0x46, 0x82, 0xB4),
    ("Tan", 0xD2, 0xB4, 0x8C),
    ("Teal", 0x00, 0x80, 0x80),
    ("Thistle", 0xD8, 0xBF, 0xD8),
    ("Tomato", 0xFF, 0x63, 0x47),
    ("Turquoise", 0x40, 0xE0, 0xD0),
    ("Violet", 0xEE, 0x82, 0xEE),
    ("Wheat", 0xF5, 0xDE, 0xB3),
    ("White", 0xFF, 0xFF, 0xFF),
    ("WhiteSmoke", 0xF5, 0xF5, 0xF5),
    ("Yellow", 0xFF, 0xFF, 0x00),
    ("YellowGreen", 0x9A, 0xCD, 0x32),
];
