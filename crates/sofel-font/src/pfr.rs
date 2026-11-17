//! A logical PFR face with character mapping and metrics. The face itself is
//! read by `sofel-pfr`; outlines are decoded by `sofel_outline::draw_pfr` and
//! embedded strikes by `sofel_bitmap::pfr_bitmap` (sofel ADR 0081).

use crate::read::{GlyphId, ReadError};
use sofel_pfr::{BitmapProgram, PfrError};

/// A logical PFR face with public character and metrics operations.
#[derive(Debug)]
pub struct PfrFace<'a>(pub(crate) sofel_pfr::PfrFace<'a>);

fn read_error(error: PfrError) -> ReadError {
    match error {
        PfrError::OutOfBounds => ReadError::OutOfBounds,
        PfrError::InvalidValue => ReadError::InvalidValue,
    }
}

impl<'a> PfrFace<'a> {
    /// The face as read by `sofel-pfr`.
    pub fn inner(&self) -> &sofel_pfr::PfrFace<'a> {
        &self.0
    }
    /// Number of logical faces in this resource.
    pub fn face_count(&self) -> u16 {
        self.0.face_count()
    }
    /// Number of glyphs including the missing glyph.
    pub fn glyph_count(&self) -> u32 {
        self.0.glyph_count()
    }
    /// Coordinate units per em of the physical outlines.
    pub fn units_per_em(&self) -> u16 {
        self.0.units_per_em()
    }
    /// Finds a character in the sorted physical character directory.
    pub fn nominal_glyph(&self, code: u32) -> Option<GlyphId> {
        self.0.nominal_glyph(code).map(GlyphId)
    }
    /// Unscaled advance along the physical font's writing direction.
    pub fn advance(&self, glyph: GlyphId) -> Option<i32> {
        self.0.advance(glyph.0)
    }
    /// Finds a bitmap in a strike of the given pixel dimensions.
    /// Returns its program, row direction, raw physical advance and metric resolution.
    pub fn bitmap_program(
        &self,
        glyph: GlyphId,
        x: u16,
        y: u16,
    ) -> Result<Option<BitmapProgram<'a>>, ReadError> {
        self.0.bitmap_program(glyph.0, x, y).map_err(read_error)
    }
    /// All glyph program bytes, including components addressed by a compound glyph.
    pub fn programs(&self) -> &'a [u8] {
        self.0.programs()
    }
    /// Encoded GPS bytes for an outline.
    pub fn glyph_program(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.0.glyph_program(glyph.0)
    }
}

impl<'a> PfrFace<'a> {
    pub(crate) fn read(data: &'a [u8], index: u32) -> Result<Self, ReadError> {
        sofel_pfr::PfrFace::read(data, index)
            .map(Self)
            .map_err(read_error)
    }
}
