//! The charset: the SID (in CID-keyed fonts, the CID) of each glyph.
//!
//! Read as FreeType 2.15.0 reads it (`cff_charset_load` in
//! `src/cff/cffload.c`).

use super::data::{EXPERT_CHARSET, EXPERT_SUBSET_CHARSET, ISO_ADOBE_CHARSET};
use crate::ReadError;
use crate::be::{self, to_usize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind<'a> {
    Predefined(&'static [u16]),
    /// The SIDs of glyphs 1 and up.
    Format0(&'a [u8]),
    /// The ranges, after the format byte.
    Format1(&'a [u8]),
    Format2(&'a [u8]),
}

/// A view over a charset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Charset<'a> {
    kind: Kind<'a>,
    num_glyphs: u32,
}

impl<'a> Charset<'a> {
    /// The charset given by the Top DICT value `offset` (0 to 2 for the
    /// predefined ISOAdobe, Expert and ExpertSubset charsets, else from
    /// the start of `table`) for `num_glyphs` glyphs.
    ///
    /// As FreeType: a predefined charset needs at most 229, 166 or 87
    /// glyphs ([`ReadError::InvalidValue`]); formats 0, 1 and 2 must hold
    /// every glyph ([`ReadError::Truncated`]); other formats fail with
    /// [`ReadError::UnknownFormat`]. Building a format 1 or 2 view reads
    /// its ranges once.
    pub fn new(table: &'a [u8], offset: u64, num_glyphs: u32) -> Result<Charset<'a>, ReadError> {
        let n = to_usize(num_glyphs);
        let predefined = |set: &'static [u16]| {
            if n > set.len() {
                Err(ReadError::InvalidValue)
            } else {
                Ok(Charset {
                    kind: Kind::Predefined(set),
                    num_glyphs,
                })
            }
        };
        match offset {
            0 => return predefined(&ISO_ADOBE_CHARSET),
            1 => return predefined(&EXPERT_CHARSET),
            2 => return predefined(&EXPERT_SUBSET_CHARSET),
            _ => {}
        }
        let at = usize::try_from(offset).map_err(|_| ReadError::OutOfBounds)?;
        let format = be::u8_at(table, at).ok_or(ReadError::OutOfBounds)?;
        let rest = table.get(at.saturating_add(1)..).unwrap_or_default();
        let kind = match format {
            0 => {
                let len = n.saturating_sub(1).saturating_mul(2);
                let sids = rest.get(..len).ok_or(ReadError::truncated(
                    at.saturating_add(1).saturating_add(len),
                    table.len(),
                ))?;
                Kind::Format0(sids)
            }
            1 | 2 => {
                let wide = format == 2;
                let used = ranges_len(rest, wide, num_glyphs).ok_or(ReadError::truncated(
                    at.saturating_add(1)
                        .saturating_add(rest.len())
                        .saturating_add(1),
                    table.len(),
                ))?;
                let ranges = rest.get(..used).unwrap_or_default();
                if wide {
                    Kind::Format2(ranges)
                } else {
                    Kind::Format1(ranges)
                }
            }
            f => return Err(ReadError::UnknownFormat(u16::from(f))),
        };
        Ok(Charset { kind, num_glyphs })
    }

    /// The number of glyphs the charset was built for.
    pub fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }

    /// The format: 0, 1 or 2; `None` for a predefined charset.
    pub fn format(&self) -> Option<u8> {
        match self.kind {
            Kind::Predefined(_) => None,
            Kind::Format0(_) => Some(0),
            Kind::Format1(_) => Some(1),
            Kind::Format2(_) => Some(2),
        }
    }

    /// The bytes of a format 0, 1 or 2 charset after the format byte, as
    /// far as they are used.
    pub fn bytes(&self) -> &'a [u8] {
        match self.kind {
            Kind::Predefined(_) => &[],
            Kind::Format0(b) | Kind::Format1(b) | Kind::Format2(b) => b,
        }
    }

    /// The SID of `glyph` (in CID-keyed fonts, its CID): 0 for glyph 0,
    /// `None` for glyphs not below the glyph count. Formats 1 and 2 take
    /// time in proportion to the ranges before the glyph.
    pub fn sid(&self, glyph: u32) -> Option<u16> {
        if glyph >= self.num_glyphs {
            return None;
        }
        if glyph == 0 {
            return Some(0);
        }
        match self.kind {
            Kind::Predefined(set) => set.get(to_usize(glyph)).copied(),
            Kind::Format0(b) => be::u16_at(b, to_usize(glyph).saturating_sub(1).saturating_mul(2)),
            Kind::Format1(_) | Kind::Format2(_) => self.sids().nth(to_usize(glyph)),
        }
    }

    /// The SIDs of every glyph, in glyph id order.
    pub fn sids(&self) -> Sids<'a> {
        Sids {
            charset: *self,
            glyph: 0,
            at: 0,
            first: 0,
            left: 0,
        }
    }

    /// The first glyph whose SID is `sid`, searching in glyph id order.
    pub fn glyph(&self, sid: u16) -> Option<u32> {
        (0u32..)
            .zip(self.sids())
            .find(|&(_, s)| s == sid)
            .map(|(g, _)| g)
    }
}

/// The ranges a format 1 or 2 charset uses for `num_glyphs` glyphs, as
/// FreeType reads them: `None` when they do not all fit.
fn ranges_len(rest: &[u8], wide: bool, num_glyphs: u32) -> Option<usize> {
    let size = if wide { 4 } else { 3 };
    let mut j = 1u32;
    let mut at = 0usize;
    while j < num_glyphs {
        let first = be::u16_at(rest, at)?;
        let left = range_left(rest, at, wide)?;
        let left = trim(first, left);
        at = at.saturating_add(size);
        j = j.saturating_add(left.saturating_add(1));
    }
    Some(at)
}

fn range_left(rest: &[u8], at: usize, wide: bool) -> Option<u32> {
    if wide {
        be::u16_at(rest, at.saturating_add(2)).map(u32::from)
    } else {
        be::u8_at(rest, at.saturating_add(2)).map(u32::from)
    }
}

/// FreeType's "rescue": a range whose SIDs would pass 0xFFFF is cut there.
fn trim(first: u16, left: u32) -> u32 {
    let max = 0xFFFFu32.saturating_sub(u32::from(first));
    left.min(max)
}

/// The SIDs of a charset in glyph id order; see [`Charset::sids`].
#[derive(Clone, Debug)]
pub struct Sids<'a> {
    charset: Charset<'a>,
    glyph: u32,
    /// The next range (formats 1 and 2).
    at: usize,
    /// The SID of the next glyph in the current range.
    first: u32,
    /// The glyphs left in the current range.
    left: u32,
}

impl Iterator for Sids<'_> {
    type Item = u16;

    fn next(&mut self) -> Option<u16> {
        let g = self.glyph;
        if g >= self.charset.num_glyphs {
            return None;
        }
        self.glyph = g.saturating_add(1);
        if g == 0 {
            return Some(0);
        }
        match self.charset.kind {
            Kind::Predefined(set) => set.get(to_usize(g)).copied(),
            Kind::Format0(b) => be::u16_at(b, to_usize(g).saturating_sub(1).saturating_mul(2)),
            Kind::Format1(b) | Kind::Format2(b) => {
                let wide = matches!(self.charset.kind, Kind::Format2(_));
                if self.left == 0 {
                    let first = be::u16_at(b, self.at)?;
                    let left = trim(first, range_left(b, self.at, wide)?);
                    self.at = self.at.saturating_add(if wide { 4 } else { 3 });
                    self.first = u32::from(first);
                    self.left = left.saturating_add(1);
                }
                let sid = u16::try_from(self.first).ok();
                self.first = self.first.saturating_add(1);
                self.left = self.left.saturating_sub(1);
                sid
            }
        }
    }
}
