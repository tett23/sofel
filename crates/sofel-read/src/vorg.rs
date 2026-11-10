//! The `VORG` (vertical origin) table, versions 1 and 2 (ADR 0016).
//!
//! Version 1 has 16-bit glyph ids and record count; version 2 (ISO OFF
//! 5th edition, HarfBuzz `VORGFormat1_2<MediumTypes>`) has 24-bit ones.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/vorg>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

/// The `VORG` tag.
pub const TAG: Tag = Tag::new(b"VORG");

#[derive(Clone, Copy, Debug)]
enum Records<'a> {
    Narrow(&'a [[u8; 4]]),
    Wide(&'a [[u8; 5]]),
}

impl Records<'_> {
    fn len(&self) -> usize {
        match self {
            Records::Narrow(r) => r.len(),
            Records::Wide(r) => r.len(),
        }
    }

    fn get(&self, i: usize) -> Option<(u32, i16)> {
        match self {
            Records::Narrow(r) => r.get(i).map(|e| {
                (
                    u32::from(u16::from_be_bytes([e[0], e[1]])),
                    i16::from_be_bytes([e[2], e[3]]),
                )
            }),
            Records::Wide(r) => r.get(i).map(|e| {
                (
                    be::u24([e[0], e[1], e[2]]),
                    i16::from_be_bytes([e[3], e[4]]),
                )
            }),
        }
    }
}

/// A view over the `VORG` table.
#[derive(Clone, Copy, Debug)]
pub struct Vorg<'a> {
    major: u16,
    minor: u16,
    default_y: i16,
    records: Records<'a>,
}

fn layout(major: u16) -> (usize, usize) {
    if major == 2 { (9, 5) } else { (8, 4) }
}

fn needed(data: &[u8]) -> usize {
    let major = be::u16_at(data, 0).unwrap_or(1);
    let (header, record) = layout(major);
    let count = if major == 2 {
        be::u24_at(data, 6).map_or(0, to_usize)
    } else {
        be::u16_at(data, 6).map_or(0, usize::from)
    };
    header.saturating_add(count.saturating_mul(record))
}

impl<'a> Vorg<'a> {
    /// A view over the header and the records.
    ///
    /// Fails with [`ReadError::UnknownVersion`] if `majorVersion` is
    /// neither 1 nor 2, and with [`ReadError::Truncated`] if the header
    /// or the records do not fit.
    pub fn new(data: &'a [u8]) -> Result<Vorg<'a>, ReadError> {
        let truncated = ReadError::truncated(needed(data), data.len());
        let h = data.first_chunk::<8>().ok_or(truncated)?;
        let major = u16::from_be_bytes([h[0], h[1]]);
        let minor = u16::from_be_bytes([h[2], h[3]]);
        let default_y = i16::from_be_bytes([h[4], h[5]]);
        let records = match major {
            1 => {
                let n = usize::from(u16::from_be_bytes([h[6], h[7]]));
                Records::Narrow(be::chunks_at::<4>(data, 8, n).ok_or(truncated)?)
            }
            2 => {
                let n = be::u24_at(data, 6).map(to_usize).ok_or(truncated)?;
                Records::Wide(be::chunks_at::<5>(data, 9, n).ok_or(truncated)?)
            }
            _ => {
                return Err(ReadError::UnknownVersion(u32::from_be_bytes([
                    h[0], h[1], h[2], h[3],
                ])));
            }
        };
        Ok(Vorg {
            major,
            minor,
            default_y,
            records,
        })
    }

    /// `majorVersion`: 1 or 2.
    pub fn major_version(&self) -> u16 {
        self.major
    }
    /// `minorVersion`.
    pub fn minor_version(&self) -> u16 {
        self.minor
    }
    /// `defaultVertOriginY`, in font units.
    pub fn default_vert_origin_y(&self) -> i16 {
        self.default_y
    }
    /// `numVertOriginYMetrics`.
    pub fn num_records(&self) -> usize {
        self.records.len()
    }
    /// Record `index`: its glyph id and `vertOriginY`.
    pub fn record(&self, index: usize) -> Option<(GlyphId, i16)> {
        self.records.get(index).map(|(g, y)| (GlyphId(g), y))
    }

    /// The `vertOriginY` of the record for `glyph`, by binary search
    /// over the records (which must ascend by glyph id).
    pub fn lookup(&self, glyph: GlyphId) -> Option<i16> {
        let (mut lo, mut hi) = (0usize, self.records.len());
        while lo < hi {
            let mid = lo.saturating_add(hi.saturating_sub(lo) / 2);
            let (g, y) = self.records.get(mid)?;
            match g.cmp(&glyph.0) {
                core::cmp::Ordering::Less => lo = mid.saturating_add(1),
                core::cmp::Ordering::Greater => hi = mid,
                core::cmp::Ordering::Equal => return Some(y),
            }
        }
        None
    }

    /// The vertical origin of `glyph`: its record's value, or
    /// `defaultVertOriginY` when it has none.
    pub fn y_origin(&self, glyph: GlyphId) -> i16 {
        self.lookup(glyph).unwrap_or(self.default_y)
    }
}

/// Checks a `VORG` table: an unknown version and truncation (and nothing
/// else then), records whose glyph ids do not ascend, repeated glyph
/// ids, and glyph ids not below `num_glyphs` (when given). Offsets are
/// those of the records.
pub fn validate(data: &[u8], num_glyphs: Option<u32>, report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let vorg = match Vorg::new(data) {
        Ok(v) => v,
        Err(ReadError::UnknownVersion(found)) => return emit(0, Kind::UnknownVersion { found }),
        Err(_) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed(data)),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    let (header, size) = layout(vorg.major_version());
    let mut previous: Option<u32> = None;
    for i in 0..vorg.num_records() {
        let Some((GlyphId(g), _)) = vorg.record(i) else {
            continue;
        };
        let at = header.saturating_add(i.saturating_mul(size));
        let index = to_u32(i);
        match previous {
            Some(p) if g == p => emit(at, Kind::DuplicateGlyph { index, glyph: g }),
            Some(p) if g < p => emit(at, Kind::GlyphsNotSorted { index }),
            _ => {}
        }
        previous = Some(g);
        if let Some(n) = num_glyphs
            && g >= n
        {
            emit(
                at,
                Kind::GlyphIdOutOfRange {
                    glyph: g,
                    num_glyphs: n,
                },
            );
        }
    }
}
