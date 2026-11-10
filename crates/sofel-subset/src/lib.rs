//! Font subsetting with the same output as HarfBuzz 15.1.0's `hb-subset`
//! (ADR 0022).
//!
//! An [`Input`] says what to keep; [`Plan::new`] computes the glyph
//! closure and the glyph mapping; [`Plan::execute`] writes the tables and
//! assembles the font. [`subset`] does both.
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
mod set;
mod tables;

pub use args::{axis_range_from_string, axis_range_to_string};
pub use error::SubsetError;
pub use input::{
    AxisRange, DEFAULT_DROP_TABLES, DEFAULT_LAYOUT_FEATURES, DEFAULT_NO_SUBSET_TABLES, Flags,
    Input, SetKind,
};
pub use plan::Plan;
pub use set::{INVALID, RangeSet};

use alloc::vec::Vec;

use sofel_font::Font;

/// Subsets `font` as `input` says (`hb_subset_or_fail` followed by
/// `hb_face_reference_blob`).
///
/// # Errors
///
/// As [`Plan::new`] and [`Plan::execute`].
pub fn subset(font: &Font<'_>, input: &Input) -> Result<Vec<u8>, SubsetError> {
    Plan::new(font, input)?.execute(font)
}
