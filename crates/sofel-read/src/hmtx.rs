//! The `hmtx` and `HMTX` tables.
//!
//! The structure is the same for both; only the counts that come from
//! `hhea`/`HHEA` and `maxp`/`MAXP` differ in width.

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

pub const TAG: Tag = Tag::new(b"hmtx");
pub const TAG_WIDE: Tag = Tag::new(b"HMTX");

#[derive(Clone, Copy, Debug)]
pub struct Hmtx<'a> {
    long: &'a [[u8; 4]],
    bearings: &'a [[u8; 2]],
    num_glyphs: u32,
}

impl<'a> Hmtx<'a> {
    /// `num_long_metrics` is `numberOfHMetrics` from `hhea`, `num_glyphs`
    /// is `numGlyphs` from `maxp`. The long metrics must all be present;
    /// the trailing left side bearings may be cut short.
    pub fn new(
        data: &'a [u8],
        num_long_metrics: u32,
        num_glyphs: u32,
    ) -> Result<Hmtx<'a>, ReadError> {
        if num_long_metrics == 0 && num_glyphs > 0 {
            return Err(ReadError::InvalidValue);
        }
        let n = to_usize(num_long_metrics);
        let long = be::chunks_at::<4>(data, 0, n)
            .ok_or(ReadError::truncated(n.saturating_mul(4), data.len()))?;
        let rest = be::tail_at(data, n.saturating_mul(4)).unwrap_or_default();
        let wanted = to_usize(num_glyphs.saturating_sub(num_long_metrics));
        let bearings = rest.as_chunks::<2>().0;
        let bearings = bearings.get(..wanted).unwrap_or(bearings);
        Ok(Hmtx {
            long,
            bearings,
            num_glyphs,
        })
    }

    pub fn advance(&self, glyph: GlyphId) -> Option<u16> {
        if glyph.0 >= self.num_glyphs {
            return None;
        }
        let i = to_usize(glyph.0).min(self.long.len().checked_sub(1)?);
        self.long.get(i).map(|m| u16::from_be_bytes([m[0], m[1]]))
    }

    pub fn left_side_bearing(&self, glyph: GlyphId) -> Option<i16> {
        if glyph.0 >= self.num_glyphs {
            return None;
        }
        let i = to_usize(glyph.0);
        match self.long.get(i) {
            Some(m) => Some(i16::from_be_bytes([m[2], m[3]])),
            None => {
                let j = i.checked_sub(self.long.len())?;
                self.bearings.get(j).map(|b| i16::from_be_bytes(*b))
            }
        }
    }
}

/// `tag` is [`TAG`] or [`TAG_WIDE`]; it only labels the diagnostics.
pub fn validate(
    data: &[u8],
    num_long_metrics: u32,
    num_glyphs: u32,
    report: &mut dyn FnMut(Diagnostic),
) {
    validate_tagged(TAG, data, num_long_metrics, num_glyphs, report);
}

pub fn validate_tagged(
    tag: Tag,
    data: &[u8],
    num_long_metrics: u32,
    num_glyphs: u32,
    report: &mut dyn FnMut(Diagnostic),
) {
    let mut emit = |offset: u32, kind: Kind| report(Diagnostic::new(tag, offset, kind));
    if num_long_metrics == 0 {
        if num_glyphs > 0 {
            emit(0, Kind::ZeroLongMetrics);
        }
        return;
    }
    if num_long_metrics > num_glyphs {
        emit(
            0,
            Kind::MetricsExceedGlyphs {
                metrics: num_long_metrics,
                glyphs: num_glyphs,
            },
        );
    }
    let needed = u64::from(num_long_metrics)
        .saturating_mul(4)
        .saturating_add(u64::from(num_glyphs.saturating_sub(num_long_metrics)).saturating_mul(2));
    let available = to_u32(data.len());
    if u64::from(available) < needed {
        emit(
            available,
            Kind::Truncated {
                needed: u32::try_from(needed).unwrap_or(u32::MAX),
                available,
            },
        );
    }
}
