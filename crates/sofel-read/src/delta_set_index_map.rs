//! Zero-copy DeltaSetIndexMap formats 0 and 1 (ADR 0027).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/otvarcommonformats>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// A map from item or glyph indices to `(outer, inner)` variation indices.
#[derive(Clone, Copy, Debug)]
pub struct DeltaSetIndexMap<'a> {
    format: u8,
    entry_format: u8,
    count: u32,
    entries: &'a [u8],
}

impl<'a> DeltaSetIndexMap<'a> {
    /// Opens format 0 (16-bit count) or 1 (32-bit count).
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        let format = be::u8_at(data, 0).ok_or(ReadError::truncated(4, data.len()))?;
        let header = match format {
            0 => 4,
            1 => 6,
            _ => return Err(ReadError::UnknownFormat(u16::from(format))),
        };
        let entry_format = be::u8_at(data, 1).ok_or(ReadError::truncated(header, data.len()))?;
        if entry_format & 0xc0 != 0 {
            return Err(ReadError::InvalidValue);
        }
        let count = if format == 0 {
            u32::from(be::u16_at(data, 2).ok_or(ReadError::truncated(header, data.len()))?)
        } else {
            be::u32_at(data, 2).ok_or(ReadError::truncated(header, data.len()))?
        };
        let width = usize::from((entry_format >> 4 & 3).saturating_add(1));
        let len = to_usize(count)
            .checked_mul(width)
            .ok_or(ReadError::OutOfBounds)?;
        let end = header.checked_add(len).ok_or(ReadError::OutOfBounds)?;
        let entries =
            be::slice_at(data, header, len).ok_or(ReadError::truncated(end, data.len()))?;
        Ok(Self {
            format,
            entry_format,
            count,
            entries,
        })
    }

    /// Format number, 0 or 1.
    pub fn format(&self) -> u8 {
        self.format
    }

    /// The number of map entries.
    pub fn len(&self) -> u32 {
        self.count
    }

    /// Whether the map has no entries and acts as the identity.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Bytes per entry, in 1..=4.
    pub fn width(&self) -> u8 {
        (self.entry_format >> 4 & 3).saturating_add(1)
    }

    /// Number of low bits carrying the inner index, in 1..=16.
    pub fn inner_bit_count(&self) -> u8 {
        (self.entry_format & 15).saturating_add(1)
    }

    /// Maps `index` to `outer << 16 | inner`; beyond the end uses the last entry.
    /// An empty map returns `index` unchanged.
    pub fn map(&self, index: u32) -> u32 {
        if self.is_empty() {
            return index;
        }
        let width = usize::from(self.width());
        let at = to_usize(index.min(self.count.saturating_sub(1))).saturating_mul(width);
        let mut raw = 0u32;
        for b in self
            .entries
            .get(at..at.saturating_add(width))
            .unwrap_or(&[])
        {
            raw = (raw << 8) | u32::from(*b);
        }
        if raw == u32::MAX {
            return u32::MAX;
        }
        let bits = u32::from(self.inner_bit_count());
        let mask = (1u32 << bits).saturating_sub(1);
        ((raw >> bits) << 16) | (raw & mask)
    }
}

/// Validates a map at `base` of table `tag` without allocating.
pub fn validate(data: &[u8], tag: Tag, base: u32, report: &mut dyn FnMut(Diagnostic)) {
    let kind = match DeltaSetIndexMap::new(data) {
        Ok(_) => return,
        Err(ReadError::UnknownFormat(found)) => Kind::UnknownFormat {
            found: u32::from(found),
        },
        Err(ReadError::InvalidValue) => Kind::UnknownFormat {
            found: u32::from(be::u8_at(data, 1).unwrap_or(0)),
        },
        Err(ReadError::Truncated { needed, available }) => Kind::Truncated {
            needed: to_u32(needed),
            available: to_u32(available),
        },
        Err(_) => Kind::OffsetOutOfBounds {
            offset: base,
            length: 0,
            available: to_u32(data.len()),
        },
    };
    report(Diagnostic::new(tag, base, kind));
}
