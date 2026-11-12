//! AAT 16-bit lookup tables as used by noncontextual glyph substitution.
use crate::be;

/// A validated AAT lookup with 16-bit values (formats 0, 2, 4, 6, 8).
#[derive(Clone, Copy, Debug)]
pub struct AatLookup<'a> {
    data: &'a [u8],
    records: &'a [u8],
    format: u16,
    first: u16,
    stride: usize,
}
impl<'a> AatLookup<'a> {
    /// Checks every record and all value arrays. Format 0 uses the face glyph count.
    pub fn new(data: &'a [u8], glyphs: u32) -> Option<Self> {
        let format = be::u16_at(data, 0)?;
        let (at, count, stride, first) = match format {
            0 => (2, glyphs as usize, 2, 0),
            8 => (
                6,
                usize::from(be::u16_at(data, 4)?),
                2,
                be::u16_at(data, 2)?,
            ),
            2 | 4 | 6 => {
                let stride = usize::from(be::u16_at(data, 2)?);
                if stride < (if format == 6 { 4 } else { 6 }) {
                    return None;
                }
                (12, usize::from(be::u16_at(data, 4)?), stride, 0)
            }
            _ => return None,
        };
        let records = be::slice_at(data, at, count.checked_mul(stride)?)?;
        if format == 4 {
            for r in records.chunks_exact(stride) {
                let (first, last) = (be::u16_at(r, 2)?, be::u16_at(r, 0)?);
                if first == 0xffff && last == 0xffff {
                    continue;
                }
                let count = usize::from(last.checked_sub(first)?).saturating_add(1);
                be::chunks_at::<2>(data, usize::from(be::u16_at(r, 4)?), count)?;
            }
        }
        Some(Self {
            data,
            records,
            format,
            first,
            stride,
        })
    }
    /// Returns the substitution for a glyph, if a record covers it.
    pub fn get(&self, glyph: u32) -> Option<u16> {
        let glyph = u16::try_from(glyph).ok()?;
        if matches!(self.format, 0 | 8) {
            return self
                .records
                .as_chunks::<2>()
                .0
                .get(usize::from(glyph.checked_sub(self.first)?))
                .copied()
                .map(u16::from_be_bytes);
        }
        let mut lo = 0usize;
        let mut hi = self.records.len().checked_div(self.stride)?;
        while lo < hi {
            let mid = usize::midpoint(lo, hi);
            let row = self.records.get(mid.checked_mul(self.stride)?..)?;
            let last = be::u16_at(row, 0)?;
            let first = if self.format == 6 {
                last
            } else {
                be::u16_at(row, 2)?
            };
            if glyph < first {
                hi = mid;
            } else if glyph > last {
                lo = mid.saturating_add(1);
            } else {
                return match self.format {
                    6 => be::u16_at(row, 2),
                    2 => be::u16_at(row, 4),
                    4 => be::u16_at(
                        self.data,
                        usize::from(be::u16_at(row, 4)?)
                            .checked_add(2 * usize::from(glyph.checked_sub(first)?))?,
                    ),
                    _ => None,
                };
            }
        }
        None
    }
}
