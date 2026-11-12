//! AAT tracking, following HarfBuzz 15.1.0's hb-aat-layout-trak-table.hh.
use crate::be;

/// A sanitized tracking table. Both directions use table-relative offsets.
#[derive(Clone, Copy, Debug)]
pub struct Trak<'a> {
    horizontal: Option<Tracks<'a>>,
    vertical: Option<Tracks<'a>>,
}
#[derive(Clone, Copy, Debug)]
struct Tracks<'a> {
    data: &'a [u8],
    tracks: &'a [[u8; 8]],
    sizes: &'a [[u8; 4]],
}
#[allow(clippy::cast_precision_loss)]
fn fixed(raw: i32) -> f32 {
    raw as f32 / 65536.
}

impl<'a> Trak<'a> {
    /// Builds a view after checking the header, tracks, size table and all value arrays.
    pub fn new(data: &'a [u8]) -> Option<Self> {
        data.first_chunk::<12>()?;
        if be::u16_at(data, 0)? != 1 {
            return None;
        }
        let parse = |at| -> Option<Option<Tracks<'a>>> {
            let off = usize::from(be::u16_at(data, at)?);
            if off == 0 {
                return Some(None);
            }
            let body = data.get(off..)?;
            let count = usize::from(be::u16_at(body, 0)?);
            let sizes = usize::from(be::u16_at(body, 2)?);
            let table = be::chunks_at::<4>(data, be::u32_at(body, 4)? as usize, sizes)?;
            let tracks = be::chunks_at::<8>(body, 8, count)?;
            for row in tracks {
                be::chunks_at::<2>(data, usize::from(be::u16_at(row, 6)?), sizes)?;
            }
            Some(Some(Tracks {
                data,
                tracks,
                sizes: table,
            }))
        };
        Some(Self {
            horizontal: parse(6)?,
            vertical: parse(8)?,
        })
    }
    /// Returns the normal track (track coordinate zero) in font units at this point size.
    pub fn tracking(&self, horizontal: bool, ptem: f32) -> f32 {
        let data = if horizontal {
            self.horizontal
        } else {
            self.vertical
        };
        data.and_then(|t| t.value(ptem)).unwrap_or(0.)
    }
}
impl Tracks<'_> {
    fn value(&self, ptem: f32) -> Option<f32> {
        let coord = |row: &[u8; 8]| be::i32_at(row, 0).map(fixed);
        let at_size = |row: &[u8; 8]| -> Option<f32> {
            let values = be::chunks_at::<2>(
                self.data,
                usize::from(be::u16_at(row, 6)?),
                self.sizes.len(),
            )?;
            let mut iter = self.sizes.iter().zip(values).map(|(s, v)| {
                (
                    fixed(i32::from_be_bytes(*s)),
                    f32::from(i16::from_be_bytes(*v)),
                )
            });
            let (mut size, mut val) = iter.next()?;
            if ptem <= size {
                return Some(val);
            }
            for (next, other) in iter {
                if next == ptem {
                    return Some(other);
                }
                if next > ptem {
                    return Some(val + (ptem - size) / (next - size) * (other - val));
                }
                size = next;
                val = other;
            }
            Some(val)
        };
        let mut lower = self.tracks.first()?;
        for row in self.tracks.iter().skip(1) {
            if coord(row)? > 0. {
                break;
            }
            lower = row;
        }
        let mut upper = self.tracks.last()?;
        for row in self.tracks.iter().rev().skip(1) {
            if coord(row)? < 0. {
                break;
            }
            upper = row;
        }
        let (lo, hi) = (coord(lower)?, coord(upper)?);
        let (a, b) = (at_size(lower)?, at_size(upper)?);
        Some(if core::ptr::eq(lower, upper) {
            a
        } else if lo == hi {
            (a + b) * 0.5
        } else {
            a + (-lo / (hi - lo)) * (b - a)
        })
    }
}
