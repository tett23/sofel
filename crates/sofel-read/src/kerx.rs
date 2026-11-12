//! AAT extended kerning subtable boundaries and version 3 glyph coverage.
use crate::{Diagnostic, Kind, Tag, be};

/// The extended kerning table tag.
pub const TAG: Tag = Tag(*b"kerx");

/// The validated subtable directory of a kerx table.
#[derive(Clone, Copy, Debug)]
pub struct Kerx<'a> {
    data: &'a [u8],
    count: u32,
}

fn truncated(at: usize, needed: usize, available: usize) -> Diagnostic {
    Diagnostic::new(
        TAG,
        be::to_u32(at),
        Kind::Truncated {
            needed: be::to_u32(needed),
            available: be::to_u32(available),
        },
    )
}

impl<'a> Kerx<'a> {
    /// Checks subtable headers, declared lengths and trailing glyph coverage.
    /// The format-specific payload is interpreted by the positioning engine.
    pub fn new(data: &'a [u8], glyphs: u32) -> Result<Self, Diagnostic> {
        let body = data.get(8..).ok_or_else(|| truncated(0, 8, data.len()))?;
        let version = be::u32_at(data, 0).unwrap_or_default();
        if version >> 16 < 2 {
            return Err(Diagnostic::new(
                TAG,
                0,
                Kind::UnknownVersion { found: version },
            ));
        }
        let count = be::u32_at(data, 4).unwrap_or_default();
        if count > 1024 {
            return Err(Diagnostic::new(TAG, 4, Kind::WorkLimitExceeded));
        }
        let mut tail = body;
        for _ in 0..count {
            let at = data.len().saturating_sub(tail.len());
            let length = be::u32_at(tail, 0).ok_or_else(|| truncated(at, 12, tail.len()))?;
            if length < 12 {
                return Err(truncated(at, 12, length as usize));
            }
            tail = tail
                .get(length as usize..)
                .ok_or_else(|| truncated(at, length as usize, tail.len()))?;
        }
        if version >> 16 >= 3 {
            let base = data.len().saturating_sub(tail.len());
            check_glyph_coverage(tail, count, glyphs).map_err(|(at, kind)| {
                Diagnostic::new(TAG, be::to_u32(base.saturating_add(at)), kind)
            })?;
        }
        Ok(Self { data: body, count })
    }

    /// Yields each bounded subtable in file order. HarfBuzz permits the
    /// last subtable's payload to use the remaining table bytes.
    pub fn subtables(self) -> impl Iterator<Item = &'a [u8]> {
        let mut tail = self.data;
        (0..self.count).map_while(move |i| {
            let length = be::u32_at(tail, 0)? as usize;
            let (declared, rest) = tail.split_at_checked(length)?;
            let sub = if i.saturating_add(1) == self.count {
                tail
            } else {
                declared
            };
            tail = rest;
            Some(sub)
        })
    }
}

/// Checks the coverage shared by kerx and the metamorphosis tables.
/// Errors carry the offset of the offending field relative to this array.
pub fn check_glyph_coverage(data: &[u8], tables: u32, glyphs: u32) -> Result<(), (usize, Kind)> {
    let offsets = be::chunks_at::<4>(data, 0, tables as usize).ok_or((
        0,
        Kind::Truncated {
            needed: tables.saturating_mul(4),
            available: be::to_u32(data.len()),
        },
    ))?;
    let length = glyphs.div_ceil(8);
    for (i, offset) in offsets.iter().enumerate() {
        let offset = u32::from_be_bytes(*offset);
        if offset != 0
            && offset != u32::MAX
            && data
                .get(offset as usize..)
                .and_then(|s| s.get(..length as usize))
                .is_none()
        {
            return Err((
                i.saturating_mul(4),
                Kind::OffsetOutOfBounds {
                    offset,
                    length,
                    available: be::to_u32(data.len()),
                },
            ));
        }
    }
    Ok(())
}
