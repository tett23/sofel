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
    pub(crate) fn bitmap_only(&self) -> bool {
        !self.0.has_outlines()
    }

    pub(crate) fn bitmap_advance(
        &self,
        glyph: GlyphId,
        x: u16,
        y: u16,
    ) -> Result<Option<i32>, ReadError> {
        let Some(program) = self.bitmap_program(glyph, x, y)? else {
            return Ok(None);
        };
        let bytes = program.bytes;
        let flags = *bytes.first().ok_or(ReadError::OutOfBounds)?;
        let bearing_len = match flags & 3 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => 6,
        };
        let size_len = match (flags >> 2) & 3 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let offset = 1usize
            .checked_add(bearing_len)
            .and_then(|n| n.checked_add(size_len))
            .ok_or(ReadError::OutOfBounds)?;
        let raw = match (flags >> 4) & 3 {
            0 => {
                if program.units == 0 {
                    return Err(ReadError::InvalidValue);
                }
                crate::geom::mul_div(
                    i32::from(x).saturating_mul(256),
                    i32::from(program.advance),
                    i32::from(program.units),
                )
            }
            1 => i32::from(i8::from_be_bytes([*bytes
                .get(offset)
                .ok_or(ReadError::OutOfBounds)?]))
            .saturating_mul(256),
            2 => {
                let pair = bytes
                    .get(offset..)
                    .and_then(|b| b.first_chunk::<2>())
                    .ok_or(ReadError::OutOfBounds)?;
                i32::from(i16::from_be_bytes(*pair))
            }
            _ => {
                let triple = bytes
                    .get(offset..)
                    .and_then(|b| b.first_chunk::<3>())
                    .ok_or(ReadError::OutOfBounds)?;
                let [a, b, c] = *triple;
                i32::from_be_bytes([if a & 0x80 != 0 { 0xff } else { 0 }, a, b, c])
            }
        };
        Ok(Some(((raw >> 2).saturating_add(32)) & !63))
    }

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
    /// Number of embedded bitmap strikes.
    pub fn strike_count(&self) -> u32 {
        self.0.strike_count()
    }
    /// Horizontal and vertical ppem in 26.6 pixels for an embedded strike.
    pub fn strike(&self, index: u32) -> Option<(u32, u32)> {
        self.0.strike(index)
    }
    /// Unscaled horizontal adjustment from PFR physical kerning pairs.
    pub fn kerning(&self, left: GlyphId, right: GlyphId) -> Option<i32> {
        self.0.kerning(left.0, right.0)
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
