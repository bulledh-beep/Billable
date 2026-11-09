//! Stamp.tsx: a rubber stamp for the invoice page (PAID, OVERDUE or DRAFT),
//! pressed a little crooked, with worn ink.
//!
//! GPUI can't rotate elements, so the stamp is drawn as an SVG image: the
//! double-ruled box and the DIN Condensed caps turned -8°, with the same
//! speckled wear (feTurbulence through the same colour matrix) as the CSS
//! mask. The letter spacing CSS adds is laid out here by hand. Its press-down
//! animation keeps the scale and fade; the image can't turn, so the -18° →
//! -8° twist of `stamp-thunk` is left out.

use std::{sync::Arc, time::Duration};

use gpui::{
    div, img, px, Animation, AnimationExt as _, AnyElement, Font, FontWeight, Image, ImageFormat, IntoElement,
    ParentElement, SharedString, Styled, TextRun, Window,
};

use crate::{art::RASTER, motion};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StampTone {
    Green,
    Red,
    Gray,
}

impl StampTone {
    fn ink(self) -> &'static str {
        match self {
            StampTone::Green => "#1E7F3C",
            StampTone::Red => "#C4302B",
            StampTone::Gray => "#6E6E73",
        }
    }
}

const FAMILY: &str = "DIN Condensed";
const TURN: f32 = -8.0;

/// A stamp drawn once; the page keeps it while the label stays the same.
#[derive(Clone)]
pub struct Stamp {
    key: String,
    image: Arc<Image>,
    /// The unrotated box (border box), as CSS lays it out.
    box_w: f32,
    box_h: f32,
    /// The image, big enough to hold the turned box.
    img_w: f32,
    img_h: f32,
}

fn din() -> Font {
    Font { weight: FontWeight::BOLD, ..gpui::font(FAMILY) }
}

/// Width of `s` in DIN Condensed Bold, plus CSS letter spacing after every
/// character.
fn spaced_width(window: &Window, s: &str, size: f32, spacing: f32) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    let run = TextRun { len: s.len(), font: din(), color: gpui::black(), background_color: None, underline: None, strikethrough: None };
    let line = window.text_system().shape_line(SharedString::from(s.to_string()), px(size), &[run], None);
    f32::from(line.width) + spacing * s.chars().count() as f32
}

/// Where the baseline sits in a line box `line_height` tall (CSS centres the
/// font's ascent + descent in the line).
fn baseline(window: &Window, size: f32, line_height: f32) -> f32 {
    let ts = window.text_system();
    let id = ts.resolve_font(&din());
    let ascent = f32::from(ts.ascent(id, px(size)));
    let descent = f32::from(ts.descent(id, px(size))).abs();
    (line_height - (ascent + descent)) / 2.0 + ascent
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl Stamp {
    /// What the stamp says; a new key means a new stamp.
    pub fn key_for(label: &str, detail: Option<&str>, tone: StampTone) -> String {
        format!("{}|{}|{tone:?}", label.to_uppercase(), detail.map(str::to_uppercase).unwrap_or_default())
    }

    pub fn new(window: &Window, label: &str, detail: Option<&str>, tone: StampTone) -> Stamp {
        let key = Self::key_for(label, detail, tone);
        let label = label.to_uppercase();
        let detail = detail.map(str::to_uppercase);
        let ink = tone.ink();

        // <span class="text-[34px] leading-[30px] tracking-[0.1em] pl-[0.1em]">
        let (ls_big, ls_small) = (34.0 * 0.1, 11.0 * 0.16);
        let label_w = spaced_width(window, &label, 34.0, ls_big) + ls_big;
        let detail_w = detail.as_deref().map(|d| spaced_width(window, d, 11.0, ls_small) + ls_small).unwrap_or(0.0);
        let content_w = label_w.max(detail_w);
        // border 2.5px, px-3.5, pt-[7px], pb-[5px]; the detail is mt-[3px] leading-3
        let (border, pad_x) = (2.5, 11.375);
        let box_w = content_w + 2.0 * (border + pad_x);
        let detail_h = if detail.is_some() { 3.0 + 9.75 } else { 0.0 };
        let box_h = border + 7.0 + 30.0 + detail_h + 5.0 + border;

        let rad = TURN.abs().to_radians();
        let (sin, cos) = (rad.sin(), rad.cos());
        let margin = 2.0;
        let img_w = (box_w * cos + box_h * sin + 2.0 * margin).ceil();
        let img_h = (box_w * sin + box_h * cos + 2.0 * margin).ceil();

        let left = border + pad_x;
        let label_x = left + (content_w - label_w) / 2.0 + ls_big;
        let label_y = border + 7.0 + baseline(window, 34.0, 30.0);
        let mut texts = format!(
            r#"<text x="{label_x}" y="{label_y}" font-family="{FAMILY}" font-weight="bold" font-size="34" letter-spacing="{ls_big}" fill="{ink}">{}</text>"#,
            escape(&label)
        );
        if let Some(d) = detail.as_deref() {
            let x = left + (content_w - detail_w) / 2.0 + ls_small;
            let y = border + 7.0 + 30.0 + 3.0 + baseline(window, 11.0, 9.75);
            texts.push_str(&format!(
                r#"<text x="{x}" y="{y}" font-family="{FAMILY}" font-weight="bold" font-size="11" letter-spacing="{ls_small}" fill="{ink}">{}</text>"#,
                escape(d)
            ));
        }
        // The 2.5px border with 7px corners, then the inset rings
        // (`inset 0 0 0 2px #fff, inset 0 0 0 3px ink`) inside it.
        let rings = format!(
            r##"<rect x="1.25" y="1.25" width="{w1}" height="{h1}" rx="5.75" fill="none" stroke="{ink}" stroke-width="2.5"/><rect x="5" y="5" width="{w3}" height="{h3}" rx="2" fill="none" stroke="{ink}" stroke-width="1"/><rect x="3.5" y="3.5" width="{w2}" height="{h2}" rx="3" fill="none" stroke="#fff" stroke-width="2"/>"##,
            w1 = box_w - 2.5,
            h1 = box_h - 2.5,
            w2 = box_w - 7.0,
            h2 = box_h - 7.0,
            w3 = box_w - 10.0,
            h3 = box_h - 10.0,
        );
        // Speckled wear: the CSS mask's turbulence, kept where it's opaque.
        let wear = format!(
            r#"<filter id="w" filterUnits="userSpaceOnUse" x="-2" y="-2" width="{fw}" height="{fh}" color-interpolation-filters="sRGB"><feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves="2" seed="7" result="n"/><feColorMatrix in="n" type="matrix" values="0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 -2.6 2.35" result="m"/><feComposite in="SourceGraphic" in2="m" operator="in"/></filter>"#,
            fw = box_w + 4.0,
            fh = box_h + 4.0,
        );
        let (tx, ty) = ((img_w - box_w) / 2.0, (img_h - box_h) / 2.0);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {img_w} {img_h}" width="{pw}" height="{ph}"><defs>{wear}</defs><g transform="translate({tx} {ty}) rotate({TURN} {cx} {cy})"><g filter="url(#w)">{rings}{texts}</g></g></svg>"#,
            pw = (img_w * RASTER).round(),
            ph = (img_h * RASTER).round(),
            cx = box_w / 2.0,
            cy = box_h / 2.0,
        );
        Stamp { key, image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())), box_w, box_h, img_w, img_h }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    /// The stamp placed like `absolute top-[34px] right-[190px]` inside the
    /// paper (a `relative` parent), pressed down a beat after it appears.
    pub fn render(&self, id: &str) -> AnyElement {
        let (top, right) = (34.0, 190.0);
        let (bw, bh, iw, ih) = (self.box_w, self.box_h, self.img_w, self.img_h);
        let place = move |el: gpui::Img, scale: f32| {
            let (w, h) = (iw * scale, ih * scale);
            el.absolute().top(px(top + bh / 2.0 - h / 2.0)).right(px(right + bw / 2.0 - w / 2.0)).w(px(w)).h(px(h))
        };
        let image = img(self.image.clone());
        if motion::reduced() {
            return place(image, 1.0).opacity(0.9).into_any_element();
        }
        // stamp-thunk: 560ms, 280ms in, cubic-bezier(0.3, 1.1, 0.5, 1) between keyframes
        let ease = motion::cubic_bezier(0.3, 1.1, 0.5, 1.0);
        let (delay, dur) = (280.0, 560.0);
        let picture = self.image.clone();
        div()
            .absolute()
            .inset_0()
            .with_animation(SharedString::from(format!("{id}-{}", self.key)), Animation::new(Duration::from_millis((delay + dur) as u64)), move |d, delta| {
                let p = motion::after(delta, delay, dur);
                let seg = |a: f32, b: f32, from: f32, to: f32| from + (to - from) * ease(((p - a) / (b - a)).clamp(0.0, 1.0));
                let scale = if p < 0.55 {
                    seg(0.0, 0.55, 1.7, 0.94)
                } else if p < 0.75 {
                    seg(0.55, 0.75, 0.94, 1.03)
                } else {
                    seg(0.75, 1.0, 1.03, 1.0)
                };
                let opacity = if p < 0.55 { seg(0.0, 0.55, 0.0, 0.95) } else { seg(0.55, 1.0, 0.95, 0.9) };
                d.child(place(img(picture.clone()), scale).opacity(opacity.clamp(0.0, 1.0)))
            })
            .into_any_element()
    }
}
