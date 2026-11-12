//! Checked AAT lookup tables for substitution and positioning.
use crate::be;

/// A validated AAT lookup with 16-bit values (formats 0, 2, 4, 6, 8).
#[derive(Clone, Copy, Debug)]
pub struct AatLookup<'a>(AatIndexLookup<'a>);

/// A lookup whose unsigned values are used as class or matrix indices.
#[derive(Clone, Copy, Debug)]
pub struct AatIndexLookup<'a> {
    width: usize,
    data: &'a [u8],
    records: &'a [u8],
    format: u16,
    first: u16,
    stride: usize,
}
impl<'a> AatLookup<'a> {
    /// Checks every record and all value arrays. Format 0 uses the face glyph count.
    pub fn new(data: &'a [u8], glyphs: u32) -> Option<Self> {
        let lookup = AatIndexLookup::new(data, glyphs, false)?;
        Some(Self(lookup))
    }
    pub(crate) fn check_offsets(&self, mut check: impl FnMut(u16) -> Option<()>) -> Option<()> {
        let t = self.0;
        let mut budget = t.data.len().saturating_mul(64).clamp(16384, 0x3fff_ffff);
        for row in t.records.chunks_exact(t.stride) {
            match t.format {
                0 | 8 => check(be::u16_at(row, 0)?)?,
                2 => check(be::u16_at(row, 4)?)?,
                6 => check(be::u16_at(row, 2)?)?,
                4 => {
                    let count = usize::from(be::u16_at(row, 0)?.checked_sub(be::u16_at(row, 2)?)?)
                        .checked_add(1)?;
                    // Multiple segments may reference the same input. Bound
                    // repeated visits as in the HarfBuzz sanitizer.
                    budget = budget.checked_sub(count)?;
                    let at = usize::from(be::u16_at(row, 4)?);
                    for value in be::chunks_at::<2>(t.data, at, count)? {
                        check(u16::from_be_bytes(*value))?;
                    }
                }
                _ => return None,
            }
        }
        (t.format != 10).then_some(())
    }

    /// Returns the glyph's pointer-addressable 16-bit value.
    pub fn get(&self, glyph: u32) -> Option<u16> {
        if self.0.format == 10 {
            return None;
        }
        u16::try_from(self.0.get(glyph)?).ok()
    }
}

impl<'a> AatIndexLookup<'a> {
    /// Validates a lookup of 16-bit or 32-bit unsigned integers.
    pub fn new(data: &'a [u8], glyphs: u32, long: bool) -> Option<Self> {
        let width: usize = if long { 4 } else { 2 };
        let format = be::u16_at(data, 0)?;
        let (at, count, stride, first) = match format {
            0 => (2, glyphs as usize, width, 0),
            8 => (
                6,
                usize::from(be::u16_at(data, 4)?),
                width,
                be::u16_at(data, 2)?,
            ),
            10 => {
                let size = usize::from(be::u16_at(data, 2)?);
                if !(1..=4).contains(&size) {
                    return None;
                }
                (
                    8,
                    usize::from(be::u16_at(data, 6)?),
                    size,
                    be::u16_at(data, 4)?,
                )
            }
            2 | 4 | 6 => {
                let stride = usize::from(be::u16_at(data, 2)?);
                let minimum = match format {
                    2 => width.checked_add(4)?,
                    6 => width.checked_add(2)?,
                    _ => 6,
                };
                if stride < minimum {
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
                be::slice_at(
                    data,
                    usize::from(be::u16_at(r, 4)?),
                    count.checked_mul(width)?,
                )?;
            }
        }
        Some(Self {
            data,
            records,
            width,
            format,
            first,
            stride,
        })
    }
    /// Returns the value for a covered glyph, including packed format 10 values.
    pub fn get(&self, glyph: u32) -> Option<u32> {
        let glyph = u16::try_from(glyph).ok()?;
        if matches!(self.format, 0 | 8 | 10) {
            let index = usize::from(glyph.checked_sub(self.first)?);
            let bytes = be::slice_at(self.records, index.checked_mul(self.stride)?, self.stride)?;
            let value = bytes.iter().fold(0u32, |v, b| (v << 8) | u32::from(*b));
            return Some(if self.width == 2 {
                value & 0xffff
            } else {
                value
            });
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
                    6 => self.word(row, 2),
                    2 => self.word(row, 4),
                    4 => self.word(
                        self.data,
                        usize::from(be::u16_at(row, 4)?).checked_add(
                            self.width
                                .checked_mul(usize::from(glyph.checked_sub(first)?))?,
                        )?,
                    ),
                    _ => None,
                };
            }
        }
        None
    }
    fn word(&self, bytes: &[u8], at: usize) -> Option<u32> {
        if self.width == 2 {
            be::u16_at(bytes, at).map(u32::from)
        } else {
            be::u32_at(bytes, at)
        }
    }
}
