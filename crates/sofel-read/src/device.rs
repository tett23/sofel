//! Device and VariationIndex tables (ADR 0017, ADR 0020).
//!
//! A Device table adjusts a value at given ppem sizes (delta formats 1
//! to 3); a VariationIndex table, which has the same header with delta
//! format 0x8000, points into an ItemVariationStore. BASE, GPOS, GDEF
//! and MATH share them. [`Device::delta_pixels`] reads the deltas as
//! HarfBuzz does; VariationIndex tables give no delta (variations are
//! not read yet).
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

    /// The delta in pixels at `ppem` (HarfBuzz
    /// `HintingDevice::get_delta_pixels`): the signed 2-, 4- or 8-bit
    /// value for `ppem` in delta formats 1 to 3, and 0 outside
    /// `startSize..=endSize` or for other delta formats (VariationIndex
    /// included).
    pub fn delta_pixels(&self, ppem: u32) -> i32 {
        let f = u32::from(self.delta_format);
        if !(1..=3).contains(&f) {
            return 0;
        }
        let (start, end) = (u32::from(self.first), u32::from(self.second));
        if ppem < start || ppem > end {
            return 0;
        }
        let s = ppem.wrapping_sub(start);
        let shift = 4u32.wrapping_sub(f);
        let word = be::u16_at(
            self.data,
            be::to_usize(s >> shift).saturating_mul(2).saturating_add(6),
        )
        .map_or(0, u32::from);
        let per_word_mask = (1u32 << shift).wrapping_sub(1);
        let bits = word >> 16u32.wrapping_sub(((s & per_word_mask).wrapping_add(1)) << f);
        let mask = 0xFFFFu32 >> 16u32.wrapping_sub(1 << f);
        let delta = bits & mask;
        let half = mask.wrapping_add(1) >> 1;
        let delta = i32::try_from(delta).unwrap_or(0);
        if delta >= i32::try_from(half).unwrap_or(0) {
            delta
                .wrapping_sub(i32::try_from(mask).unwrap_or(0))
                .wrapping_sub(1)
        } else {
            delta
        }
    }
}
