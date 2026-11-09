//! Billable's colour tokens, from src/renderer/index.css (`:root.dark` and
//! `:root.light`). The active palette is a GPUI global, swapped when the
//! appearance setting or the Mac's appearance changes.

use gpui::{point, px, App, BoxShadow, Global, Hsla, Rgba};

pub fn rgb3(r: u8, g: u8, b: u8) -> Hsla {
    Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }.into()
}

pub fn rgba4(r: u8, g: u8, b: u8, a: f32) -> Rgba {
    Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}

pub fn hex(v: u32) -> Hsla {
    rgb3((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// "#F5A623" / "F5A623" / "#fff" → colour (grey if it can't be read).
pub fn parse_hex(s: &str) -> Hsla {
    let s = s.trim().trim_start_matches('#');
    let v = match s.len() {
        3 => u32::from_str_radix(&s.chars().flat_map(|c| [c, c]).collect::<String>(), 16).ok(),
        6 => u32::from_str_radix(s, 16).ok(),
        _ => None,
    };
    v.map(hex).unwrap_or(hex(0xAFAFAF))
}

/// CSS `color-mix(in srgb, a p, b)`.
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

/// CSS `filter: brightness(f)` on a flat colour.
pub fn brighten(c: Hsla, f: f32) -> Hsla {
    let c = c.to_rgb();
    Rgba { r: (c.r * f).min(1.0), g: (c.g * f).min(1.0), b: (c.b * f).min(1.0), a: c.a }.into()
}

pub fn css_hex(c: Hsla) -> String {
    let c = c.to_rgb();
    format!("#{:02x}{:02x}{:02x}", (c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8)
}

pub fn shadow(x: f32, y: f32, blur: f32, spread: f32, color: Rgba) -> BoxShadow {
    BoxShadow { color: color.into(), offset: point(px(x), px(y)), blur_radius: px(blur), spread_radius: px(spread), inset: false }
}

pub fn inset(x: f32, y: f32, blur: f32, spread: f32, color: Rgba) -> BoxShadow {
    BoxShadow { inset: true, ..shadow(x, y, blur, spread, color) }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    pub dark: bool,
    pub bg: Hsla,
    pub sidebar: Hsla,
    pub panel: Hsla,
    pub panel2: Hsla,
    pub field: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    pub fg: Hsla,
    pub fg2: Hsla,
    pub fg3: Hsla,
    pub fg4: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_deep: Hsla,
    pub accent_text: Hsla,
    pub green: Hsla,
    pub green_deep: Hsla,
    pub blue: Hsla,
    pub blue_deep: Hsla,
    pub red: Hsla,
    pub red_deep: Hsla,
    pub amber: Hsla,
    pub violet: Hsla,
    pub gray: Hsla,
    /// `--rim`: white in dark, black in light (hairline rings).
    pub rim: Hsla,
}

impl Global for Theme {}

impl Theme {
    pub fn dark() -> Self {
        Theme {
            dark: true,
            bg: rgb3(24, 24, 27),
            sidebar: rgb3(28, 28, 32),
            panel: rgb3(33, 33, 38),
            panel2: rgb3(41, 41, 47),
            field: rgb3(40, 40, 46),
            line: rgb3(52, 52, 59),
            line_strong: rgb3(70, 70, 79),
            fg: rgb3(244, 244, 246),
            fg2: rgb3(212, 212, 218),
            fg3: rgb3(150, 150, 161),
            fg4: rgb3(104, 104, 114),
            accent: rgb3(255, 146, 23),
            accent_hover: rgb3(255, 164, 60),
            accent_deep: rgb3(214, 112, 0),
            accent_text: rgb3(255, 170, 76),
            green: rgb3(76, 214, 118),
            green_deep: rgb3(52, 170, 90),
            blue: rgb3(90, 170, 255),
            blue_deep: rgb3(60, 140, 230),
            red: rgb3(255, 104, 104),
            red_deep: rgb3(230, 70, 70),
            amber: rgb3(255, 196, 64),
            violet: rgb3(190, 140, 255),
            gray: rgb3(150, 150, 161),
            rim: rgb3(255, 255, 255),
        }
    }

    pub fn light() -> Self {
        Theme {
            dark: false,
            bg: rgb3(246, 246, 248),
            sidebar: rgb3(255, 255, 255),
            panel: rgb3(255, 255, 255),
            panel2: rgb3(248, 248, 250),
            field: rgb3(244, 244, 247),
            line: rgb3(233, 233, 237),
            line_strong: rgb3(214, 214, 220),
            fg: rgb3(32, 32, 38),
            fg2: rgb3(72, 72, 82),
            fg3: rgb3(118, 118, 130),
            fg4: rgb3(168, 168, 178),
            accent: rgb3(255, 140, 10),
            accent_hover: rgb3(255, 158, 50),
            accent_deep: rgb3(214, 106, 0),
            accent_text: rgb3(214, 100, 0),
            green: rgb3(34, 150, 72),
            green_deep: rgb3(24, 120, 56),
            blue: rgb3(24, 122, 230),
            blue_deep: rgb3(16, 100, 200),
            red: rgb3(226, 54, 62),
            red_deep: rgb3(196, 36, 44),
            amber: rgb3(196, 108, 0),
            violet: rgb3(136, 80, 220),
            gray: rgb3(118, 118, 130),
            rim: rgb3(0, 0, 0),
        }
    }

    /// `--shadow-card`
    pub fn shadow_card(&self) -> Vec<BoxShadow> {
        if self.dark {
            vec![inset(0.0, 1.0, 0.0, 0.0, rgba4(255, 255, 255, 0.03)), shadow(0.0, 1.0, 2.0, 0.0, rgba4(0, 0, 0, 0.3))]
        } else {
            vec![shadow(0.0, 1.0, 2.0, 0.0, rgba4(20, 20, 40, 0.04)), shadow(0.0, 6.0, 18.0, -10.0, rgba4(20, 20, 40, 0.12))]
        }
    }

    /// `--shadow-pop`: menus, popovers, modals, toasts.
    pub fn shadow_pop(&self) -> Vec<BoxShadow> {
        if self.dark {
            vec![
                shadow(0.0, 0.0, 0.0, 1.0, rgba4(255, 255, 255, 0.08)),
                shadow(0.0, 18.0, 44.0, -12.0, rgba4(0, 0, 0, 0.7)),
                shadow(0.0, 4.0, 12.0, 0.0, rgba4(0, 0, 0, 0.3)),
            ]
        } else {
            vec![
                shadow(0.0, 0.0, 0.0, 1.0, rgba4(20, 20, 40, 0.06)),
                shadow(0.0, 16.0, 40.0, -10.0, rgba4(20, 20, 40, 0.22)),
                shadow(0.0, 3.0, 8.0, 0.0, rgba4(20, 20, 40, 0.06)),
            ]
        }
    }

    /// `.btn-primary`
    pub fn shadow_primary(&self) -> Vec<BoxShadow> {
        vec![
            inset(0.0, 1.0, 0.0, 0.0, rgba4(255, 255, 255, 0.25)),
            shadow(0.0, 1.0, 2.0, 0.0, rgba4(200, 90, 0, 0.3)),
            shadow(0.0, 4.0, 12.0, -4.0, rgba4(255, 140, 10, 0.45)),
        ]
    }

    /// The tone colours used by badges and status text.
    pub fn tone(&self, tone: Tone) -> Hsla {
        match tone {
            Tone::Accent => self.accent_text,
            Tone::Green => self.green,
            Tone::Blue => self.blue,
            Tone::Red => self.red,
            Tone::Amber => self.amber,
            Tone::Violet => self.violet,
            Tone::Gray => self.fg2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Accent,
    Green,
    Blue,
    Red,
    Amber,
    Violet,
    Gray,
}

/// The active palette.
pub fn theme(cx: &App) -> Theme {
    cx.try_global::<Theme>().copied().unwrap_or_else(Theme::dark)
}
