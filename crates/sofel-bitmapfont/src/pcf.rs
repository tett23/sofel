//! Bounds-checked PCF metrics, encoding and bitmap reader (sofel ADR 0065).
// Table sizes and counts are checked before indexing; Clippy cannot follow
// those relationships through the parsing stages.
#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
use alloc::vec::Vec;

use crate::Glyph;

pub use crate::noalloc::pcf::PcfError;
use crate::noalloc::pcf::{RawBitmap, directory_error_offset, scan};

/// A single-strike PCF face with one-based glyph IDs.
#[derive(Clone, Debug)]
pub struct PcfFont {
    glyphs: Vec<Glyph>,
    mappings: Vec<(u32, u32)>,
    default_glyph: usize,
    x_ppem_26_6: u32,
    y_ppem_26_6: u32,
}

impl PcfFont {
    /// Whether the PCF magic is present.
    pub fn is_pcf(data: &[u8]) -> bool {
        data.starts_with(b"\x01fcp")
    }

    /// Parse a PCF face and identify a damaged table directory field.
    /// Errors within a table use the file start when no directory field is damaged.
    ///
    /// Allocates the glyph list, each glyph's repacked pixels and the character map (ADR 0094).
    pub fn parse_positioned(data: &[u8]) -> Result<Self, (PcfError, usize)> {
        Self::parse(data).map_err(|error| (error, directory_error_offset(data)))
    }

    /// Parse a PCF face with a single monochrome strike.
    ///
    /// Allocates the glyph list, each glyph's repacked pixels and the character map (ADR 0094).
    pub fn parse(data: &[u8]) -> Result<Self, PcfError> {
        let mut glyphs = Vec::new();
        let mut mappings = Vec::new();
        let summary = scan(
            data,
            &mut |head, raw| glyphs.push(Glyph::from_head(head, unpack(raw, head.width))),
            &mut |code, id| mappings.push((code, id)),
        )?;
        let default_glyph = mappings
            .iter()
            .find(|(code, _)| *code == summary.default_code)
            .map(|(_, id)| *id as usize - 1)
            .unwrap_or(0);
        Ok(Self {
            glyphs,
            mappings,
            default_glyph,
            x_ppem_26_6: summary.x_ppem_26_6,
            y_ppem_26_6: summary.y_ppem_26_6,
        })
    }

    /// Validate a PCF face without allocating (sofel ADR 0096).
    pub fn validate(data: &[u8], report: &mut dyn FnMut(crate::noalloc::Problem)) {
        crate::noalloc::pcf::validate(data, report);
    }

    /// Number of glyph IDs including the missing-glyph slot.
    pub fn num_glyphs(&self) -> usize {
        self.glyphs.len() + 1
    }
    /// The fixed strike size in 26.6 pixels.
    pub fn ppem_26_6(&self) -> (u32, u32) {
        (self.x_ppem_26_6, self.y_ppem_26_6)
    }
    /// Map a codepoint to a one-based glyph ID.
    pub fn nominal_glyph(&self, codepoint: u32) -> Option<u32> {
        self.mappings
            .iter()
            .find(|(code, _)| *code == codepoint)
            .map(|(_, id)| *id)
    }
    /// Return the glyph for an ID, with ID zero using the default character.
    pub fn glyph(&self, id: u32) -> Option<&Glyph> {
        self.glyphs.get(if id == 0 {
            self.default_glyph
        } else {
            usize::try_from(id - 1).ok()?
        })
    }
}

/// Put the stored pixels of a glyph in packed rows of `ceil(width / 8)`
/// bytes, most significant sample first.
fn unpack(raw: RawBitmap<'_>, width: u16) -> Vec<u8> {
    let keep = usize::from(width).div_ceil(8);
    let mut bitmap = raw.bytes.to_vec();
    if raw.reverse_bits {
        for byte in &mut bitmap {
            *byte = byte.reverse_bits();
        }
    }
    if let Some(unit) = raw.unit {
        for chunk in bitmap.chunks_exact_mut(unit) {
            chunk.reverse();
        }
    }
    let mut packed = Vec::new();
    if raw.row_bytes != 0 {
        for row in bitmap.chunks_exact(raw.row_bytes) {
            packed.extend_from_slice(row.get(..keep).unwrap_or(row));
        }
    }
    packed
}
