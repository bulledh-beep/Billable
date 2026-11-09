//! SF Rounded at every weight.
//!
//! The Mac ships SF Rounded as one variable font. Chromium picks weights off its
//! weight axis; GPUI loads the file as a single face, so every weight would come
//! out Regular. Here we cut fixed-weight copies of the system's own file in
//! memory at startup and hand them to GPUI. Nothing is written to disk.
//!
//! Each copy gets its own family name, "Billable Rounded 600" and so on:
//! - names starting with "." belong to the system, and Core Text swaps such a
//!   copy back for its own default-weight font;
//! - Core Text reports every copy as weight 400, so GPUI can't choose among
//!   copies sharing one family by weight. One family per weight sidesteps it.

use std::borrow::Cow;

use allsorts::{
    binary::{
        read::ReadScope,
        write::{WriteBinary, WriteBuffer},
    },
    error::ParseError,
    font_data::FontData,
    tables::{owned, variable_fonts::fvar::FvarTable, Fixed, FontTableProvider, NameTable},
    tag,
    variations::instance,
};
use anyhow::{anyhow, Result};

pub const WEIGHTS: [u16; 4] = [400, 500, 600, 700];

/// Letter-spacing variants the app uses (weight, em): the wordmark (-0.03),
/// page titles and figures (-0.015), section titles (-0.01), the invoice's
/// "Invoice" heading (tracking-tight, -0.025) and its small caps labels (0.08).
/// CSS letter-spacing adds the same space after every character, which is
/// what widening every glyph's advance does, and kerning still applies.
pub const TRACKED: [(u16, f32); 5] = [(700, -0.03), (700, -0.015), (700, -0.01), (600, -0.025), (600, 0.08)];

/// The family with CSS `letter-spacing: {em}em` built in (one of [`TRACKED`];
/// anything else falls back to untracked).
pub fn tracked_family(weight: u16, em: f32) -> String {
    match TRACKED.iter().find(|(w, e)| *w == weight && (*e - em).abs() < 1e-4) {
        Some((w, e)) => format!("Billable Rounded {w} t{e}"),
        None => rounded_family(weight),
    }
}

/// Add `em × unitsPerEm` to every glyph's advance in hmtx.
fn track_advances(font: &mut [u8], em: f32) {
    let be16 = |b: &[u8], o: usize| u16::from_be_bytes([b[o], b[o + 1]]);
    let be32 = |b: &[u8], o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let n = be16(font, 4) as usize;
    let mut table = |tag: &[u8; 4]| (0..n).map(|i| 12 + 16 * i).find(|&r| &font[r..r + 4] == tag).map(|r| be32(font, r + 8) as usize);
    let (Some(head), Some(hhea), Some(hmtx)) = (table(b"head"), table(b"hhea"), table(b"hmtx")) else { return };
    let upm = be16(font, head + 18) as f32;
    let metrics = be16(font, hhea + 34) as usize;
    let delta = (em * upm).round() as i32;
    for i in 0..metrics {
        let o = hmtx + 4 * i;
        let adv = (be16(font, o) as i32 + delta).clamp(0, u16::MAX as i32) as u16;
        font[o..o + 2].copy_from_slice(&adv.to_be_bytes());
    }
}

/// The family to ask GPUI for at a CSS weight (400, 500, 600 or 700).
pub fn rounded_family(weight: u16) -> String {
    let w = WEIGHTS.iter().copied().min_by_key(|w| w.abs_diff(weight)).unwrap_or(500);
    format!("Billable Rounded {w}")
}
const SOURCE: &str = "/System/Library/Fonts/SFNSRounded.ttf";

/// The source font with its `name` table swapped for ours, and without its
/// STAT table: that describes the variable font's weight axis, and Core Text
/// reads every copy as Regular from it.
struct Renamed<'a, P> {
    inner: &'a P,
    name: Vec<u8>,
}

impl<P: FontTableProvider> FontTableProvider for Renamed<'_, P> {
    fn table_data(&self, t: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError> {
        match t {
            tag::NAME => Ok(Some(Cow::Borrowed(&self.name))),
            tag::STAT => Ok(None),
            _ => self.inner.table_data(t),
        }
    }
    fn has_table(&self, t: u32) -> bool {
        t != tag::STAT && self.inner.has_table(t)
    }
    fn table_tags(&self) -> Option<Vec<u32>> {
        self.inner.table_tags().map(|tags| tags.into_iter().filter(|&t| t != tag::STAT).collect())
    }
}

fn err(e: impl std::fmt::Debug) -> anyhow::Error {
    anyhow!("{e:?}")
}

pub fn rounded_instances() -> Result<Vec<Cow<'static, [u8]>>> {
    let buffer = std::fs::read(SOURCE)?;
    let scope = ReadScope::new(&buffer);
    let font = scope.read::<FontData<'_>>().map_err(err)?;
    let source = font.table_provider(0).map_err(err)?;
    let name_data = source.read_table_data(tag::NAME).map_err(err)?;
    let name = ReadScope::new(&name_data).read::<NameTable<'_>>().map_err(err)?;
    let fvar_data = source.read_table_data(tag::FVAR).map_err(err)?;
    let fvar = ReadScope::new(&fvar_data).read::<FvarTable<'_>>().map_err(err)?;

    let mut out = Vec::new();
    let variants: Vec<(u16, Option<f32>)> = WEIGHTS.iter().map(|&w| (w, None)).chain(TRACKED.iter().map(|&(w, e)| (w, Some(e)))).collect();
    for (w, tracking) in variants {
        let family = match tracking {
            Some(e) => tracked_family(w, e),
            None => rounded_family(w),
        };
        let ps = match tracking {
            Some(e) => format!("BillableRounded{w}T{}", (e * 1000.0).round() as i32).replace('-', "m"),
            None => format!("BillableRounded{w}"),
        };
        let mut renamed = owned::NameTable::try_from(&name).map_err(err)?;
        for (id, value) in [
            (NameTable::FONT_FAMILY_NAME, family.as_str()),
            (NameTable::TYPOGRAPHIC_FAMILY_NAME, family.as_str()),
            (NameTable::FULL_FONT_NAME, family.as_str()),
            (NameTable::UNIQUE_FONT_IDENTIFIER, ps.as_str()),
            (NameTable::POSTSCRIPT_NAME, ps.as_str()),
            (NameTable::VARIATIONS_POSTSCRIPT_NAME_PREFIX, ps.as_str()),
        ] {
            renamed.replace_entries(id, value);
        }
        let mut buf = WriteBuffer::new();
        owned::NameTable::write(&mut buf, &renamed).map_err(err)?;
        let provider = Renamed { inner: &source, name: buf.into_inner() };

        let tuple: Vec<Fixed> = fvar
            .axes()
            .map(|axis| if axis.axis_tag == tag::WGHT { Fixed::from(w as f32) } else { axis.default_value })
            .collect();
        let (mut bytes, _) = instance(&provider, &tuple).map_err(|e| anyhow!("instance {w}: {e:?}"))?;
        // allsorts marks TrueType output with Apple's old 'true' tag; GPUI's
        // font loader only takes the standard 0x00010000. Same format otherwise.
        if bytes.starts_with(b"true") {
            bytes[..4].copy_from_slice(&[0, 1, 0, 0]);
        }
        if let Some(e) = tracking {
            track_advances(&mut bytes, e);
        }
        out.push(Cow::Owned(bytes));
    }
    Ok(out)
}
