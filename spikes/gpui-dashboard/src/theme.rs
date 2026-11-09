//! Billable's colour tokens, copied from src/renderer/index.css.

use gpui::{point, px, BoxShadow, Hsla, Rgba};

pub fn rgb3(r: u8, g: u8, b: u8) -> Hsla {
    Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }.into()
}

pub fn hex(v: u32) -> Hsla {
    rgb3((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// CSS `color-mix(in srgb, a p%, b)`.
pub fn mix(a: Hsla, b: Hsla, p: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    Rgba {
        r: a.r * p + b.r * (1.0 - p),
        g: a.g * p + b.g * (1.0 - p),
        b: a.b * p + b.b * (1.0 - p),
        a: a.a * p + b.a * (1.0 - p),
    }
    .into()
}

pub fn css_hex(c: Hsla) -> String {
    let c = c.to_rgb();
    format!("#{:02x}{:02x}{:02x}", (c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8)
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub bg: Hsla,
    pub sidebar: Hsla,
    pub panel: Hsla,
    pub panel2: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    pub fg: Hsla,
    pub fg2: Hsla,
    pub fg3: Hsla,
    pub fg4: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_text: Hsla,
    pub green: Hsla,
    pub blue: Hsla,
    pub red: Hsla,
    pub amber: Hsla,
}

impl Theme {
    pub fn dark() -> Self {
        Theme {
            dark: true,
            bg: rgb3(24, 24, 27),
            sidebar: rgb3(28, 28, 32),
            panel: rgb3(33, 33, 38),
            panel2: rgb3(41, 41, 47),
            line: rgb3(52, 52, 59),
            line_strong: rgb3(70, 70, 79),
            fg: rgb3(244, 244, 246),
            fg2: rgb3(212, 212, 218),
            fg3: rgb3(150, 150, 161),
            fg4: rgb3(104, 104, 114),
            accent: rgb3(255, 146, 23),
            accent_hover: rgb3(255, 164, 60),
            accent_text: rgb3(255, 170, 76),
            green: rgb3(76, 214, 118),
            blue: rgb3(90, 170, 255),
            red: rgb3(255, 104, 104),
            amber: rgb3(255, 196, 64),
        }
    }

    pub fn light() -> Self {
        Theme {
            dark: false,
            bg: rgb3(246, 246, 248),
            sidebar: rgb3(255, 255, 255),
            panel: rgb3(255, 255, 255),
            panel2: rgb3(248, 248, 250),
            line: rgb3(233, 233, 237),
            line_strong: rgb3(214, 214, 220),
            fg: rgb3(32, 32, 38),
            fg2: rgb3(72, 72, 82),
            fg3: rgb3(118, 118, 130),
            fg4: rgb3(168, 168, 178),
            accent: rgb3(255, 140, 10),
            accent_hover: rgb3(255, 158, 50),
            accent_text: rgb3(214, 100, 0),
            green: rgb3(34, 150, 72),
            blue: rgb3(24, 122, 230),
            red: rgb3(226, 54, 62),
            amber: rgb3(196, 108, 0),
        }
    }

    /// `--shadow-card`. GPUI has no inset shadows, so the dark theme's 1px
    /// inner highlight is left out.
    pub fn shadow_card(&self) -> Vec<BoxShadow> {
        if self.dark {
            vec![shadow(0.0, 1.0, 2.0, 0.0, Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.3 })]
        } else {
            vec![
                shadow(0.0, 1.0, 2.0, 0.0, Rgba { r: 20.0 / 255.0, g: 20.0 / 255.0, b: 40.0 / 255.0, a: 0.04 }),
                shadow(0.0, 6.0, 18.0, -10.0, Rgba { r: 20.0 / 255.0, g: 20.0 / 255.0, b: 40.0 / 255.0, a: 0.12 }),
            ]
        }
    }

    /// `.btn-primary`'s glow (its inset top highlight can't be drawn).
    pub fn shadow_primary(&self) -> Vec<BoxShadow> {
        vec![
            shadow(0.0, 1.0, 2.0, 0.0, Rgba { r: 200.0 / 255.0, g: 90.0 / 255.0, b: 0.0, a: 0.3 }),
            shadow(0.0, 4.0, 12.0, -4.0, Rgba { r: 1.0, g: 140.0 / 255.0, b: 10.0 / 255.0, a: 0.45 }),
        ]
    }
}

pub fn shadow(x: f32, y: f32, blur: f32, spread: f32, color: Rgba) -> BoxShadow {
    BoxShadow { color: color.into(), offset: point(px(x), px(y)), blur_radius: px(blur), spread_radius: px(spread) }
}
