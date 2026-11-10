//! The FDSelect: the Font DICT of each glyph of a CID-keyed CFF or a
//! CFF2 font.

use crate::ReadError;
use crate::be::{self, to_usize};

/// A view over an FDSelect: format 0 (one byte per glyph), 3 (16-bit
/// ranges) or 4 (CFF2, 32-bit ranges).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FdSelect<'a> {
    /// One Font DICT index per glyph.
    Format0(&'a [u8]),
    /// The ranges and the sentinel, after `nRanges`: `first` (16 bits)
    /// and `fd` (8 bits) each.
    Format3(&'a [u8]),
    /// The ranges and the sentinel, after `nRanges`: `first` (32 bits)
    /// and `fd` (16 bits) each.
    Format4(&'a [u8]),
}

impl<'a> FdSelect<'a> {
    /// The FDSelect at `offset` in `table` for `num_glyphs` glyphs.
    ///
    /// Fails with [`ReadError::UnknownFormat`] for formats other than 0,
    /// 3 and 4, with [`ReadError::InvalidValue`] for no ranges, and with
    /// [`ReadError::Truncated`] when the glyphs (format 0) or the ranges
    /// and the sentinel do not fit.
    pub fn new(table: &'a [u8], offset: usize, num_glyphs: u32) -> Result<FdSelect<'a>, ReadError> {
        let format = be::u8_at(table, offset).ok_or(ReadError::OutOfBounds)?;
        let at = offset.saturating_add(1);
        let take = |at: usize, len: usize| {
            be::slice_at(table, at, len)
                .ok_or(ReadError::truncated(at.saturating_add(len), table.len()))
        };
        match format {
            0 => take(at, to_usize(num_glyphs)).map(FdSelect::Format0),
            3 => {
                let n = be::u16_at(table, at)
                    .ok_or(ReadError::truncated(at.saturating_add(2), table.len()))?;
                if n == 0 {
                    return Err(ReadError::InvalidValue);
                }
                take(
                    at.saturating_add(2),
                    usize::from(n).saturating_mul(3).saturating_add(2),
                )
                .map(FdSelect::Format3)
            }
            4 => {
                let n = be::u32_at(table, at)
                    .ok_or(ReadError::truncated(at.saturating_add(4), table.len()))?;
                if n == 0 {
                    return Err(ReadError::InvalidValue);
                }
                take(
                    at.saturating_add(4),
                    to_usize(n).saturating_mul(6).saturating_add(4),
                )
                .map(FdSelect::Format4)
            }
            f => Err(ReadError::UnknownFormat(u16::from(f))),
        }
    }

    /// The format: 0, 3 or 4.
    pub fn format(&self) -> u8 {
        match self {
            FdSelect::Format0(_) => 0,
            FdSelect::Format3(_) => 3,
            FdSelect::Format4(_) => 4,
        }
    }

    /// The number of ranges (formats 3 and 4); the glyph count for
    /// format 0.
    pub fn len(&self) -> usize {
        match self {
            FdSelect::Format0(d) => d.len(),
            FdSelect::Format3(d) => d.len().saturating_sub(2) / 3,
            FdSelect::Format4(d) => d.len().saturating_sub(4) / 6,
        }
    }

    /// Whether there are no ranges or glyphs.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Range `i` (formats 3 and 4) as `(first, fd)`; for `i` equal to the
    /// number of ranges, the sentinel as `(sentinel, 0)`.
    pub fn range(&self, i: usize) -> Option<(u32, u16)> {
        match self {
            FdSelect::Format0(_) => None,
            FdSelect::Format3(d) => {
                let at = i.checked_mul(3)?;
                let first = u32::from(be::u16_at(d, at)?);
                Some((
                    first,
                    be::u8_at(d, at.saturating_add(2)).map_or(0, u16::from),
                ))
            }
            FdSelect::Format4(d) => {
                let at = i.checked_mul(6)?;
                let first = be::u32_at(d, at)?;
                Some((first, be::u16_at(d, at.saturating_add(4)).unwrap_or(0)))
            }
        }
    }

    /// The Font DICT index of `glyph`, found as FreeType's
    /// `cff_fd_select_get` finds it: 0 when no range holds it. Format 4,
    /// which FreeType does not read, is searched the same way.
    pub fn fd(&self, glyph: u32) -> u16 {
        match self {
            FdSelect::Format0(d) => d.get(to_usize(glyph)).map_or(0, |&b| u16::from(b)),
            FdSelect::Format3(_) | FdSelect::Format4(_) => {
                let n = self.len();
                let Some((mut first, _)) = self.range(0) else {
                    return 0;
                };
                for i in 0..n {
                    if glyph < first {
                        break;
                    }
                    let (_, fd) = self.range(i).unwrap_or((0, 0));
                    let limit = self.range(i.saturating_add(1)).map_or(0, |r| r.0);
                    if glyph < limit {
                        return fd;
                    }
                    first = limit;
                }
                0
            }
        }
    }
}
