//! The `vmtx` and `VMTX` (vertical metrics) tables (ADR 0016).
//!
//! They have the structure of `hmtx` and `HMTX`, so they are read with
//! the same view: [`Vmtx`] is [`Hmtx`], whose `advance` is the advance
//! height and `left_side_bearing` the top side bearing.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/vmtx>

use crate::hmtx::{self, Hmtx};
use crate::{Diagnostic, Tag};

/// The `vmtx` tag.
pub const TAG: Tag = Tag::new(b"vmtx");
/// The `VMTX` tag, the variant with 24-bit counts.
pub const TAG_WIDE: Tag = Tag::new(b"VMTX");

/// A view over the `vmtx` or `VMTX` table: the `hmtx` view.
pub type Vmtx<'a> = Hmtx<'a>;

/// Checks a `vmtx` table against `numOfLongVerMetrics` and `numGlyphs`
/// like [`hmtx::validate`], labelling the diagnostics `vmtx`.
pub fn validate(
    data: &[u8],
    num_long_metrics: u32,
    num_glyphs: u32,
    report: &mut dyn FnMut(Diagnostic),
) {
    hmtx::validate_tagged(TAG, data, num_long_metrics, num_glyphs, report);
}
