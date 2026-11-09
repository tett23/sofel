//! The `loca` and `LOCA` tables (ADR 0011).
//!
//! `LOCA` has the same two formats as `loca`; both are chosen by
//! `head.indexToLocFormat`. Which byte ranges make up a glyph when the
//! entries are broken is a policy left to the outline layer.

use crate::be::{to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

pub const TAG: Tag = Tag::new(b"loca");
pub const TAG_WIDE: Tag = Tag::new(b"LOCA");

#[derive(Clone, Copy, Debug)]
pub struct Loca<'a> {
    short: &'a [[u8; 2]],
    long: &'a [[u8; 4]],
}

impl<'a> Loca<'a> {
    /// `index_to_loc_format` is 0 (16-bit offsets / 2) or 1 (32-bit).
    pub fn new(data: &'a [u8], index_to_loc_format: i16) -> Result<Loca<'a>, ReadError> {
        match index_to_loc_format {
            0 => Ok(Loca {
                short: data.as_chunks::<2>().0,
                long: &[],
            }),
            1 => Ok(Loca {
                short: &[],
                long: data.as_chunks::<4>().0,
            }),
            f => Err(ReadError::UnknownFormat(u16::from_be_bytes(
                f.to_be_bytes(),
            ))),
        }
    }

    /// The number of whole entries.
    pub fn num_locations(&self) -> usize {
        self.short.len().max(self.long.len())
    }

    /// Entry `index` as a byte offset into the glyph data.
    pub fn offset(&self, index: usize) -> Option<u32> {
        match self.long.get(index) {
            Some(e) => Some(u32::from_be_bytes(*e)),
            None => self
                .short
                .get(index)
                .map(|e| u32::from(u16::from_be_bytes(*e)).saturating_mul(2)),
        }
    }
}

/// Validates `loca` (or `LOCA`, as `tag` says) against the glyph count
/// and the length of the glyph data.
pub fn validate(
    data: &[u8],
    index_to_loc_format: i16,
    num_glyphs: u32,
    glyf_len: usize,
    tag: Tag,
    report: &mut dyn FnMut(Diagnostic),
) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(tag, to_u32(offset), kind));
    let Ok(loca) = Loca::new(data, index_to_loc_format) else {
        return emit(
            0,
            Kind::InvalidIndexToLocFormat {
                found: i32::from(index_to_loc_format),
            },
        );
    };
    let size = if index_to_loc_format == 0 { 2usize } else { 4 };
    let needed = to_usize(num_glyphs).saturating_add(1);
    if loca.num_locations() < needed {
        emit(
            data.len(),
            Kind::Truncated {
                needed: to_u32(needed.saturating_mul(size)),
                available: to_u32(data.len()),
            },
        );
    }
    let mut previous = 0u32;
    for i in 0..loca.num_locations().min(needed) {
        let Some(off) = loca.offset(i) else { break };
        let at = i.saturating_mul(size);
        if to_usize(off) > glyf_len {
            emit(
                at,
                Kind::OffsetOutOfBounds {
                    offset: off,
                    length: 0,
                    available: to_u32(glyf_len),
                },
            );
        } else if i > 0 && off < previous {
            emit(at, Kind::LocaNotSorted { index: to_u32(i) });
        }
        previous = off;
    }
}
