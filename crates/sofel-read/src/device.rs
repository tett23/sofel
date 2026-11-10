//! Device and VariationIndex tables (ADR 0017): the header only.
//!
//! A Device table adjusts a value at given ppem sizes (delta formats 1
//! to 3); a VariationIndex table, which has the same header with delta
//! format 0x8000, points into an ItemVariationStore. BASE, GPOS, GDEF
//! and MATH share them. The deltas themselves are not read yet.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/chapter2#device-and-variationindex-tables>

use crate::{ReadError, be};

/// `deltaFormat` of a VariationIndex table.
pub const VARIATION_INDEX: u16 = 0x8000;

/// A view over the header of a Device or VariationIndex table.
#[derive(Clone, Copy, Debug)]
pub struct Device<'a> {
    data: &'a [u8],
    first: u16,
    second: u16,
    delta_format: u16,
}

/// The bytes a Device table needs: the 6-byte header, and for delta
/// formats 1 to 3 the packed deltas from `startSize` to `endSize`
/// (2, 4 or 8 bits each, in 16-bit words). HarfBuzz's
/// `HintingDevice::get_size`: just the header when `startSize` is above
/// `endSize`.
pub fn size(start_size: u16, end_size: u16, delta_format: u16) -> usize {
    match delta_format {
        1..=3 if start_size <= end_size => {
            let shift = 4u16.saturating_sub(delta_format);
            let words = (end_size.saturating_sub(start_size) >> shift).saturating_add(1);
            usize::from(words).saturating_mul(2).saturating_add(6)
        }
        _ => 6,
    }
}

impl<'a> Device<'a> {
    /// A view over the header (and, for delta formats 1 to 3, the
    /// extent of the deltas). Unknown delta formats are accepted with
    /// only the header, as HarfBuzz does.
    ///
    /// Fails with [`ReadError::Truncated`] if the header or the deltas
    /// do not fit.
    pub fn new(data: &'a [u8]) -> Result<Device<'a>, ReadError> {
        let h = data
            .first_chunk::<6>()
            .ok_or(ReadError::truncated(6, data.len()))?;
        let first = u16::from_be_bytes([h[0], h[1]]);
        let second = u16::from_be_bytes([h[2], h[3]]);
        let delta_format = u16::from_be_bytes([h[4], h[5]]);
        let needed = size(first, second, delta_format);
        let data = be::slice_at(data, 0, needed).ok_or(ReadError::truncated(needed, data.len()))?;
        Ok(Device {
            data,
            first,
            second,
            delta_format,
        })
    }

    /// The bytes of the table: the header and the deltas.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `deltaFormat`: 1, 2 or 3 for a Device table, 0x8000 for a
    /// VariationIndex table; other values are reserved.
    pub fn delta_format(&self) -> u16 {
        self.delta_format
    }
    /// `startSize` (the first field), the smallest ppem with a delta.
    pub fn start_size(&self) -> u16 {
        self.first
    }
    /// `endSize` (the second field), the largest ppem with a delta.
    pub fn end_size(&self) -> u16 {
        self.second
    }
    /// `(deltaSetOuterIndex, deltaSetInnerIndex)` of a VariationIndex
    /// table; `None` for other delta formats.
    pub fn variation_index(&self) -> Option<(u16, u16)> {
        (self.delta_format == VARIATION_INDEX).then_some((self.first, self.second))
    }
}
