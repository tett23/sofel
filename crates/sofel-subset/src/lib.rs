//! Font subsetting with the same output as HarfBuzz 15.1.0's `hb-subset`
//! (ADR 0022).
//!
//! An [`Input`] says what to keep; [`Plan::new`] computes the glyph
//! closure and the glyph mapping; [`Plan::execute`] writes the tables and
//! assembles the font. [`subset`] does both. To subset one font many
//! times, preprocess it once with [`Prepared::new`] and make the plans
//! with [`Plan::with_prepared`].
//!
//! Inputs that this version cannot subset yet are reported as
//! [`SubsetError::Unsupported`] rather than written differently from
//! HarfBuzz.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod args;
mod bits;
mod cmap_accel;
mod error;
mod input;
mod instance;
mod layout;
mod limits;
mod plan;
mod prepared;
mod room;
mod set;
mod tables;

pub use args::{axis_range_from_string, axis_range_to_string};
pub use error::SubsetError;
pub use input::{
    AxisRange, DEFAULT_DROP_TABLES, DEFAULT_LAYOUT_FEATURES, DEFAULT_NO_SUBSET_TABLES, Flags,
    Input, SetKind,
};
pub use plan::Plan;
pub use prepared::Prepared;
pub use set::{INVALID, RangeSet};

use alloc::vec::Vec;

use sofel_font::{Font, HbCff1};

/// Subsets `font` as `input` says (`hb_subset_or_fail` followed by
/// `hb_face_reference_blob`).
///
/// # Errors
///
/// As [`Plan::new`] and [`Plan::execute`].
pub fn subset(font: &Font<'_>, input: &Input) -> Result<Vec<u8>, SubsetError> {
    Plan::new(font, input)?.execute(font)
}

/// The bytes of the CharStrings INDEX of `font`'s `CFF ` table
/// (`hb_subset_cff_get_charstrings_index`): from its count to the end of
/// its data. `None` when HarfBuzz does not read the table.
pub fn cff_charstrings_index<'a>(font: &Font<'a>) -> Option<&'a [u8]> {
    let cff = HbCff1::new(font)?;
    let index = cff.char_strings();
    let start = usize::try_from(index.position()).ok()?;
    let len = u32::from_ne_bytes(index.size_offset().to_ne_bytes());
    let end = start.checked_add(usize::try_from(len).ok()?)?;
    cff.data().get(start..end)
}

/// The charstring of `glyph` in `font`'s `CFF ` table
/// (`hb_subset_cff_get_charstring_data`). `None` when HarfBuzz does not
/// read the table, for a glyph not below the count, and for an empty
/// charstring.
pub fn cff_charstring_data<'a>(font: &Font<'a>, glyph: u32) -> Option<&'a [u8]> {
    HbCff1::new(font)?
        .char_string(glyph)
        .filter(|b| !b.is_empty())
}
