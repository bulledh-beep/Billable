//! Billable's drawings as SVG, ported from src/renderer/components/Illustrations.tsx
//! and ProductMarks.tsx. GPUI rasterises SVG images at their stated size, so
//! each one is written out at RASTER times its on-screen size to stay sharp
//! on a Retina display, then drawn back down.

use std::{collections::HashMap, sync::Arc};

use gpui::{Image, ImageFormat};

pub const RASTER: f32 = 3.0;

const ORANGE: &str = "#FF9600";
const ORANGE_DEEP: &str = "#E07B00";
const ORANGE_LIGHT: &str = "#FFB84D";
const YELLOW: &str = "#FFC800";
const YELLOW_DEEP: &str = "#E5A800";
const YELLOW_LIGHT: &str = "#FFE066";
const GREEN: &str = "#58CC02";
const GREEN_DEEP: &str = "#46A302";
const GREEN_LIGHT: &str = "#89E219";
const BLUE: &str = "#1CB0F6";
const BLUE_DEEP: &str = "#1899D6";
const BLUE_LIGHT: &str = "#84D8FF";
const RED: &str = "#FF4B4B";
const RED_DEEP: &str = "#EA2B2B";
const PURPLE: &str = "#CE82FF";
const PURPLE_DEEP: &str = "#A568CC";
const GREY: &str = "#AFAFAF";
const GREY_DEEP: &str = "#8F8F8F";
const INK: &str = "#4B4B4B";
const WHITE: &str = "#FFFFFF";
const BLUSH: &str = "#FF9EB0";

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Mood {
    Idle,
    Happy,
    Wave,
    Sleepy,
    Worried,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Home,
    Clients,
    Projects,
    Time,
    Billing,
    Reports,
    Commissions,
    Tax,
    Receipt,
    Settings,
    Send,
    Alert,
    Trophy,
    Hourglass,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Glyph {
    ChevronRight,
    ChevronRightBold,
    Play,
    Pause,
    ChevronsUpDown,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Art {
    Mascot { mood: Mood, blink: bool },
    Icon(Icon),
    BillableMark,
    ContentMark,
    Glyph(Glyph, String),
}

fn wrap(view_box: &str, size: f32, body: &str) -> String {
    let px = (size * RASTER).round();
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}" width="{px}" height="{px}">{body}</svg>"#)
}

/// The images, made once each.
#[derive(Default)]
pub struct Gallery {
    cache: HashMap<(Art, u32), Arc<Image>>,
}

impl Gallery {
    pub fn get(&mut self, art: Art, size: f32) -> Arc<Image> {
        let key = (art.clone(), (size * 10.0) as u32);
        self.cache
            .entry(key)
            .or_insert_with(|| {
                let svg = match &art {
                    Art::Mascot { mood, blink } => mascot(*mood, *blink, size),
                    Art::Icon(icon) => wrap("0 0 32 32", size, icon_body(*icon).as_str()),
                    Art::BillableMark => wrap("0 0 512 512", size, BILLABLE_MARK),
                    Art::ContentMark => wrap("0 0 512 512", size, CONTENT_MARK),
                    Art::Glyph(g, color) => glyph(*g, color, size),
                };
                Arc::new(Image::from_bytes(ImageFormat::Svg, swap_red_blue(&svg).into_bytes()))
            })
            .clone()
    }
}

/// GPUI 0.2.2 hands SVG pixels to the GPU as RGBA where it expects BGRA, so
/// red and blue come out swapped (PNGs are converted; SVGs aren't). Writing
/// every colour with red and blue already swapped cancels it out.
fn swap_red_blue(svg: &str) -> String {
    let b = svg.as_bytes();
    let mut out = String::with_capacity(svg.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' {
            let hex_len = b[i + 1..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            if hex_len == 6 {
                let h = &svg[i + 1..i + 7];
                out.push('#');
                out.push_str(&h[4..6]);
                out.push_str(&h[2..4]);
                out.push_str(&h[0..2]);
                i += 7;
                continue;
            }
            if hex_len == 3 {
                let h = &svg[i + 1..i + 4];
                out.push('#');
                out.push_str(&h[2..3]);
                out.push_str(&h[1..2]);
                out.push_str(&h[0..1]);
                i += 4;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

fn glyph(g: Glyph, color: &str, size: f32) -> String {
    let stroke = |w: f32, d: &str| format!(r#"<g fill="none" stroke="{color}" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round">{d}</g>"#);
    let body = match g {
        Glyph::ChevronRight => stroke(2.0, r#"<path d="m9 18 6-6-6-6"/>"#),
        Glyph::ChevronRightBold => stroke(3.0, r#"<path d="m9 18 6-6-6-6"/>"#),
        Glyph::ChevronsUpDown => stroke(2.0, r#"<path d="m7 15 5 5 5-5"/><path d="m7 9 5-5 5 5"/>"#),
        Glyph::Play => format!(r#"<polygon points="6 3 20 12 6 21 6 3" fill="{color}" stroke="{color}" stroke-width="2" stroke-linejoin="round"/>"#),
        Glyph::Pause => format!(r#"<g fill="{color}"><rect x="14" y="4" width="4" height="16" rx="1"/><rect x="6" y="4" width="4" height="16" rx="1"/></g>"#),
    };
    wrap("0 0 24 24", size, &body)
}

fn mascot(mood: Mood, blink: bool, size: f32) -> String {
    let mut s = String::new();
    s += r##"<ellipse cx="60" cy="113" rx="28" ry="4.5" fill="#000" opacity="0.1"/>"##;
    s += &format!(r#"<ellipse cx="45" cy="104" rx="11" ry="7" fill="{ORANGE_DEEP}"/><ellipse cx="75" cy="104" rx="11" ry="7" fill="{ORANGE_DEEP}"/>"#);
    for (rot, cx, x) in [(-24, 30, 0), (24, 90, 60)] {
        s += &format!(
            r#"<g transform="rotate({rot} {cx} 26)"><path d="M{a} 32 a13 13 0 0 1 26 0 z" fill="{YELLOW}"/><path d="M{a} 32 h26 v2.5 a2 2 0 0 1 -2 2 h-22 a2 2 0 0 1 -2 -2 z" fill="{YELLOW_DEEP}"/><ellipse cx="{e}" cy="25" rx="3" ry="4.5" fill="{YELLOW_LIGHT}" transform="rotate(-20 {e} 25)"/></g>"#,
            a = 17 + x,
            e = 24.5 + x as f32
        );
    }
    s += &format!(r#"<rect x="55" y="11" width="10" height="9" rx="3" fill="{ORANGE_DEEP}"/>"#);
    let arm = |d: &str| format!(r#"<path d="{d}" stroke="{ORANGE}" stroke-width="9" stroke-linecap="round" fill="none"/>"#);
    match mood {
        Mood::Happy => {
            s += &arm("M24 58 q-12 -8 -10 -24");
            s += &arm("M96 58 q12 -8 10 -24");
        }
        Mood::Wave => {
            s += &arm("M20 70 q-8 8 -5 18");
            s += &arm("M97 58 q13 -6 13 -22");
        }
        _ => {
            s += &arm("M20 70 q-8 8 -5 18");
            s += &arm("M100 70 q8 8 5 18");
        }
    }
    s += &format!(r#"<circle cx="60" cy="62" r="44" fill="{ORANGE}"/><path d="M104 62 a44 44 0 0 1 -80 25 a46 46 0 0 0 80 -25 z" fill="{ORANGE_DEEP}" opacity="0.45"/><ellipse cx="36" cy="36" rx="9" ry="5.5" fill="{WHITE}" opacity="0.35" transform="rotate(-38 36 36)"/>"#);
    s += &format!(r#"<circle cx="60" cy="65" r="33" fill="{WHITE}"/>"#);
    for a in [0, 90, 180, 270] {
        s += &format!(r#"<rect x="58" y="35" width="4" height="7" rx="2" fill="{ORANGE_LIGHT}" transform="rotate({a} 60 65)"/>"#);
    }
    let line = |d: &str, w: f32| format!(r#"<path d="{d}" stroke="{INK}" stroke-width="{w}" stroke-linecap="round" fill="none"/>"#);
    match mood {
        Mood::Happy => {
            s += &line("M42 62 q6 -9 12 0", 4.5);
            s += &line("M66 62 q6 -9 12 0", 4.5);
        }
        Mood::Sleepy => {
            s += &line("M42 60 q6 5 12 0", 4.0);
            s += &line("M66 60 q6 5 12 0", 4.0);
        }
        _ => {
            let ry = if mood == Mood::Worried { 7.0 } else { 8.5 };
            let squash = if blink { r#" transform="translate(0 59) scale(1 0.1) translate(0 -59)""# } else { "" };
            s += &format!(
                r#"<g{squash}><ellipse cx="48" cy="59" rx="6.5" ry="{ry}" fill="{INK}"/><ellipse cx="72" cy="59" rx="6.5" ry="{ry}" fill="{INK}"/><circle cx="50.3" cy="55.5" r="2.4" fill="{WHITE}"/><circle cx="74.3" cy="55.5" r="2.4" fill="{WHITE}"/></g>"#
            );
            if mood == Mood::Worried {
                s += &line("M40 47 l12 3", 3.0);
                s += &line("M80 47 l-12 3", 3.0);
            }
        }
    }
    s += &format!(r#"<ellipse cx="39" cy="73" rx="5.5" ry="3.2" fill="{BLUSH}" opacity="0.75"/><ellipse cx="81" cy="73" rx="5.5" ry="3.2" fill="{BLUSH}" opacity="0.75"/>"#);
    match mood {
        Mood::Happy | Mood::Wave => {
            s += &format!(r##"<path d="M49 72 q11 16 22 0 z" fill="{INK}"/><path d="M54.5 79 q5.5 -4 11 0 q-5.5 5 -11 0 z" fill="#FF6B81"/>"##);
        }
        Mood::Sleepy => s += &format!(r#"<ellipse cx="60" cy="77" rx="4" ry="3" fill="{INK}"/>"#),
        Mood::Worried => s += &line("M52 80 q8 -6 16 0", 3.5),
        Mood::Idle => s += &line("M52 75 q8 7 16 0", 3.5),
    }
    if mood == Mood::Sleepy {
        // The app sets these z's in type; drawn as strokes here.
        s += &format!(r#"<path d="M93 18 h8 l-8 9 h8 M103 7 h6 l-6 7 h6" stroke="{BLUE}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#);
    }
    wrap("0 0 120 120", size, &s)
}

fn icon_body(icon: Icon) -> String {
    match icon {
        Icon::Home => format!(r#"<rect x="7" y="13" width="18" height="15" rx="3.5" fill="{YELLOW}"/><path d="M7 24h18v.5a3.5 3.5 0 0 1-3.5 3.5h-11A3.5 3.5 0 0 1 7 24.5z" fill="{YELLOW_DEEP}"/><rect x="13" y="19" width="6" height="9" rx="2" fill="{ORANGE_DEEP}"/><path d="M3.5 15.5 16 5l12.5 10.5" stroke="{RED}" stroke-width="4.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#),
        Icon::Clients => format!(r#"<circle cx="21.5" cy="10.5" r="4.5" fill="{PURPLE}"/><path d="M14 26v-2.5a7.5 7.5 0 0 1 15 0V26a1.5 1.5 0 0 1-1.5 1.5h-12A1.5 1.5 0 0 1 14 26z" fill="{PURPLE_DEEP}"/><circle cx="12" cy="12" r="5.5" fill="{BLUE}"/><path d="M3 27v-2.8a9 9 0 0 1 18 0V27a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 27z" fill="{BLUE_DEEP}"/><ellipse cx="10" cy="10" rx="1.6" ry="2.2" fill="{WHITE}" opacity="0.4" transform="rotate(-30 10 10)"/>"#),
        Icon::Projects => format!(r#"<path d="M3.5 9A2.5 2.5 0 0 1 6 6.5h6.2c.8 0 1.5.4 2 1l1.6 2H26A2.5 2.5 0 0 1 28.5 12v2h-25z" fill="{YELLOW_DEEP}"/><rect x="3.5" y="11.5" width="25" height="15" rx="3" fill="{YELLOW}"/><path d="M3.5 23h25v.5a3 3 0 0 1-3 3h-19a3 3 0 0 1-3-3z" fill="{YELLOW_DEEP}" opacity="0.7"/><rect x="7" y="14.5" width="9" height="2.6" rx="1.3" fill="{YELLOW_LIGHT}"/>"#),
        Icon::Time => format!(r#"<rect x="13.5" y="2.5" width="5" height="4" rx="1.5" fill="{ORANGE_DEEP}"/><circle cx="16" cy="18" r="12" fill="{ORANGE}"/><circle cx="16" cy="18" r="8.5" fill="{WHITE}"/><path d="M16 18v-5M16 18l3.5 2.2" stroke="{INK}" stroke-width="2.4" stroke-linecap="round"/><circle cx="16" cy="18" r="1.6" fill="{ORANGE}"/>"#),
        Icon::Billing => format!(r#"<rect x="3" y="8" width="26" height="17" rx="3.5" fill="{GREEN}"/><path d="M3 21.5h26v.5a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3z" fill="{GREEN_DEEP}"/><circle cx="16" cy="16" r="4.6" fill="{GREEN_LIGHT}"/><path d="M16 13.2v5.6M17.6 14.3c-.4-.5-1-.7-1.6-.7-.9 0-1.6.5-1.6 1.2 0 1.6 3.3.9 3.3 2.5 0 .7-.8 1.2-1.7 1.2-.7 0-1.3-.3-1.7-.8" stroke="{GREEN_DEEP}" stroke-width="1.3" stroke-linecap="round" fill="none"/><circle cx="7.5" cy="12" r="1.4" fill="{GREEN_LIGHT}"/><circle cx="24.5" cy="20" r="1.4" fill="{GREEN_LIGHT}"/>"#),
        Icon::Reports => format!(r#"<rect x="4" y="16" width="6.5" height="11" rx="2" fill="{BLUE}"/><rect x="12.75" y="10" width="6.5" height="17" rx="2" fill="{GREEN}"/><rect x="21.5" y="4" width="6.5" height="23" rx="2" fill="{ORANGE}"/><rect x="4" y="24" width="6.5" height="3" rx="1.5" fill="{BLUE_DEEP}"/><rect x="12.75" y="24" width="6.5" height="3" rx="1.5" fill="{GREEN_DEEP}"/><rect x="21.5" y="24" width="6.5" height="3" rx="1.5" fill="{ORANGE_DEEP}"/>"#),
        Icon::Commissions => format!(r#"<ellipse cx="13" cy="22" rx="9.5" ry="4.5" fill="{YELLOW_DEEP}"/><rect x="3.5" y="17" width="19" height="5" fill="{YELLOW_DEEP}"/><ellipse cx="13" cy="17" rx="9.5" ry="4.5" fill="{YELLOW}"/><ellipse cx="13" cy="12.5" rx="9.5" ry="4.5" fill="{YELLOW_DEEP}"/><rect x="3.5" y="8" width="19" height="4.5" fill="{YELLOW_DEEP}"/><ellipse cx="13" cy="8" rx="9.5" ry="4.5" fill="{YELLOW}"/><circle cx="23" cy="20" r="6.5" fill="{ORANGE}"/><circle cx="23" cy="20" r="4" fill="{ORANGE_LIGHT}"/>"#),
        Icon::Tax => {
            let mut keys = String::new();
            for r in 0..3 {
                for c in 0..3 {
                    let last = r == 2 && c == 2;
                    keys += &format!(
                        r#"<rect x="{}" y="{}" width="3.4" height="2.6" rx="1" fill="{}" opacity="{}"/>"#,
                        9.5 + c as f32 * 4.6,
                        15.0 + r as f32 * 3.8,
                        if last { ORANGE } else { WHITE },
                        if last { 1.0 } else { 0.85 }
                    );
                }
            }
            format!(r#"<rect x="6" y="3" width="20" height="26" rx="4" fill="{PURPLE}"/><path d="M6 25h20v.5a3.5 3.5 0 0 1-3.5 3.5h-13A3.5 3.5 0 0 1 6 25.5z" fill="{PURPLE_DEEP}"/><rect x="9.5" y="6.5" width="13" height="6" rx="1.8" fill="{WHITE}"/>{keys}"#)
        }
        Icon::Receipt => format!(r#"<path d="M7 4.5A1.5 1.5 0 0 1 8.5 3h15A1.5 1.5 0 0 1 25 4.5V28l-3-2-3 2-3-2-3 2-3-2-3 2z" fill="{BLUE_LIGHT}"/><rect x="10.5" y="8" width="11" height="2.6" rx="1.3" fill="{BLUE_DEEP}"/><rect x="10.5" y="13" width="7" height="2.6" rx="1.3" fill="{BLUE}"/><rect x="10.5" y="18" width="9" height="2.6" rx="1.3" fill="{BLUE}"/>"#),
        Icon::Settings => {
            let teeth: String = (0..8).map(|i| format!(r#"<rect x="13.5" y="2.5" width="5" height="7" rx="1.8" fill="{GREY}" transform="rotate({} 16 16)"/>"#, i * 45)).collect();
            format!(r#"{teeth}<circle cx="16" cy="16" r="10" fill="{GREY}"/><path d="M26 16a10 10 0 0 1-18.7 5 10.4 10.4 0 0 0 18.7-5z" fill="{GREY_DEEP}"/><circle cx="16" cy="16" r="4.2" fill="{WHITE}"/>"#)
        }
        Icon::Send => format!(r#"<path d="M3.5 14.5 28.5 4l-6 23-7.5-7.5z" fill="{BLUE}"/><path d="M15 19.5 28.5 4 12.8 21.8z" fill="{BLUE_DEEP}"/><path d="M12.8 21.8 15 19.5l3 3-4.6 3.9z" fill="{BLUE_DEEP}"/>"#),
        Icon::Alert => format!(r#"<circle cx="16" cy="16" r="13" fill="{RED}"/><path d="M29 16a13 13 0 0 1-24.6 5.9A13.5 13.5 0 0 0 29 16z" fill="{RED_DEEP}"/><rect x="14" y="8" width="4" height="10" rx="2" fill="{WHITE}"/><circle cx="16" cy="22.5" r="2.2" fill="{WHITE}"/>"#),
        Icon::Trophy => format!(r#"<path d="M8 7H4.5v3a5 5 0 0 0 5 5M24 7h3.5v3a5 5 0 0 1-5 5" stroke="{YELLOW_DEEP}" stroke-width="2.6" fill="none" stroke-linecap="round"/><path d="M8 4h16v8a8 8 0 0 1-16 0z" fill="{YELLOW}"/><path d="M24 11a8 8 0 0 1-15.2 3.5A8 8 0 0 0 24 12z" fill="{YELLOW_DEEP}"/><rect x="14" y="19" width="4" height="4" fill="{YELLOW_DEEP}"/><rect x="9.5" y="23" width="13" height="5" rx="2" fill="{ORANGE}"/><ellipse cx="12" cy="8" rx="1.6" ry="2.6" fill="{WHITE}" opacity="0.5"/>"#),
        Icon::Hourglass => format!(r#"<rect x="6" y="3" width="20" height="4" rx="2" fill="{ORANGE_DEEP}"/><rect x="6" y="25" width="20" height="4" rx="2" fill="{ORANGE_DEEP}"/><path d="M9 7h14v2.5c0 3-3 5-5.5 6.5 2.5 1.5 5.5 3.5 5.5 6.5V25H9v-2.5c0-3 3-5 5.5-6.5C12 14.5 9 12.5 9 9.5z" fill="{BLUE_LIGHT}"/><path d="M12 23.5c0-2 2-3.3 4-4.3 2 1 4 2.3 4 4.3V25h-8z" fill="{ORANGE}"/><path d="M12.5 9.5h7c-.6 1.4-2 2.5-3.5 3.4-1.5-.9-2.9-2-3.5-3.4z" fill="{ORANGE}"/>"#),
    }
}

const BILLABLE_MARK: &str = r##"<defs><linearGradient id="body" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFB13D"/><stop offset="1" stop-color="#FF8A00"/></linearGradient><linearGradient id="rim" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="0.45"/><stop offset="0.5" stop-color="#ffffff" stop-opacity="0.06"/><stop offset="1" stop-color="#ffffff" stop-opacity="0.12"/></linearGradient><mask id="apart" maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512"><rect width="512" height="512" fill="#ffffff"/><circle cx="256" cy="290" r="158" fill="#000000"/></mask></defs><rect width="512" height="512" rx="120" fill="url(#body)"/><rect x="2.5" y="2.5" width="507" height="507" rx="117.5" fill="none" stroke="url(#rim)" stroke-width="5"/><g mask="url(#apart)" fill="#ffffff"><rect x="236" y="96" width="40" height="44" rx="12"/><g transform="rotate(-32 160 150)"><path d="M106 164a54 54 0 0 1 108 0z"/></g><g transform="rotate(32 352 150)"><path d="M298 164a54 54 0 0 1 108 0z"/></g><path d="M180 392l-30 52M332 392l30 52" fill="none" stroke="#ffffff" stroke-width="30" stroke-linecap="round"/></g><circle cx="256" cy="290" r="138" fill="#ffffff"/><g fill="#FFC98A"><rect x="248" y="170" width="16" height="30" rx="8"/><rect x="248" y="380" width="16" height="30" rx="8"/><rect x="136" y="282" width="30" height="16" rx="8"/><rect x="346" y="282" width="30" height="16" rx="8"/></g><path d="M256 290L202 248" stroke="#F27C00" stroke-width="28" stroke-linecap="round"/><path d="M256 290L338 222" stroke="#F27C00" stroke-width="24" stroke-linecap="round"/><circle cx="256" cy="290" r="20" fill="#D96A00"/>"##;

const CONTENT_MARK: &str = r##"<defs><linearGradient id="body" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#232327"/><stop offset="1" stop-color="#0a0a0b"/></linearGradient><linearGradient id="rim" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="0.22"/><stop offset="0.5" stop-color="#ffffff" stop-opacity="0.03"/><stop offset="1" stop-color="#ffffff" stop-opacity="0.06"/></linearGradient><clipPath id="slate"><rect x="118" y="92" width="276" height="54" rx="14"/></clipPath></defs><rect width="512" height="512" rx="120" fill="url(#body)"/><rect x="2.5" y="2.5" width="507" height="507" rx="117.5" fill="none" stroke="url(#rim)" stroke-width="5"/><g transform="translate(0 22)"><g transform="rotate(-8 256 120)"><rect x="118" y="92" width="276" height="54" rx="14" fill="#ffffff"/><g clip-path="url(#slate)" fill="#1c1c20"><path d="M150 92h34l-30 54h-34z"/><path d="M218 92h34l-30 54h-34z"/><path d="M286 92h34l-30 54h-34z"/><path d="M354 92h34l-30 54h-34z"/></g></g><path d="M162 246a32 32 0 0 1 64 0v30q-32-16-64 0z" fill="#ffffff"/><path d="M286 246a32 32 0 0 1 64 0v30q-32-16-64 0z" fill="#ffffff"/><path d="M216 344q40 26 80 0" fill="none" stroke="#ffffff" stroke-width="15" stroke-linecap="round"/></g>"##;
