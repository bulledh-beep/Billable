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

pub const WEIGHTS: [u16; 3] = [500, 600, 700];

/// The family to ask GPUI for at a CSS weight (500, 600 or 700).
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
    for w in WEIGHTS {
        let family = rounded_family(w);
        let ps = format!("BillableRounded{w}");
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
        out.push(Cow::Owned(bytes));
    }
    Ok(out)
}
