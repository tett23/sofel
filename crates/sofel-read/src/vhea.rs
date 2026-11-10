//! The `vhea` and `VHEA` (vertical header) tables (ADR 0016).
//!
//! They have the structure of `hhea` and `HHEA`, so they are read with
//! the same view: [`Vhea`] is [`Hhea`], whose horizontal names stand for
//! the vertical fields (`ascender` for `vertTypoAscender`,
//! `advance_width_max` for `advanceHeightMax`,
//! `number_of_h_metrics` for `numOfLongVerMetrics`, and so on).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/vhea>

use crate::hhea::{self, Hhea};
use crate::{CountWidth, Diagnostic, Tag};

/// The `vhea` tag.
pub const TAG: Tag = Tag::new(b"vhea");
/// The `VHEA` tag, the variant with 24-bit counts.
pub const TAG_WIDE: Tag = Tag::new(b"VHEA");

/// A view over the `vhea` or `VHEA` table: the `hhea` view.
pub type Vhea<'a> = Hhea<'a>;

/// Checks a `vhea` (`U16`) or `VHEA` (`U24`) table like
/// [`hhea::validate`], labelling the diagnostics with its tag.
pub fn validate(data: &[u8], width: CountWidth, report: &mut dyn FnMut(Diagnostic)) {
    let tag = match width {
        CountWidth::U16 => TAG,
        CountWidth::U24 => TAG_WIDE,
    };
    hhea::validate_tagged(tag, data, width, report);
}
