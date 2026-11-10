//! The encoding of a CFF font that is not CID-keyed: codes 0 to 255 to
//! glyphs.
//!
//! Read as FreeType 2.15.0 reads it (`cff_encoding_load` in
//! `src/cff/cffload.c`). In sfnt-wrapped CFF the encoding is not used to
//! map characters; it is read for FreeType's load check, the validator
//! and, later, bare CFF fonts (ADR 0021).

use super::Charset;
use super::data::{EXPERT_ENCODING, STANDARD_ENCODING};
use crate::ReadError;
use crate::be;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind<'a> {
    Predefined(&'static [u16; 256]),
    /// The codes of glyphs 1 and up.
    Format0(&'a [u8]),
    /// The ranges: first code and codes left.
    Format1(&'a [u8]),
}

/// A view over an encoding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Encoding<'a> {
    kind: Kind<'a>,
    /// The supplements: code and SID, three bytes each.
    supplements: &'a [u8],
    num_glyphs: u32,
}

impl<'a> Encoding<'a> {
    /// The encoding given by the Top DICT value `offset` (0 and 1 for the
    /// predefined Standard and Expert encodings, else from the start of
    /// `table`) for `num_glyphs` glyphs. Fails, as FreeType does, when
    /// the format (without the supplement bit 0x80) is neither 0 nor 1
    /// ([`ReadError::UnknownFormat`]) or the data is cut
    /// ([`ReadError::Truncated`]).
    pub fn new(table: &'a [u8], offset: u64, num_glyphs: u32) -> Result<Encoding<'a>, ReadError> {
        let predefined = |set| Encoding {
            kind: Kind::Predefined(set),
            supplements: &[],
            num_glyphs,
        };
        match offset {
            0 => return Ok(predefined(&STANDARD_ENCODING)),
            1 => return Ok(predefined(&EXPERT_ENCODING)),
            _ => {}
        }
        let at = usize::try_from(offset).map_err(|_| ReadError::OutOfBounds)?;
        let truncated = |needed: usize| ReadError::truncated(needed, table.len());
        let format = be::u8_at(table, at).ok_or(truncated(at.saturating_add(1)))?;
        let count = usize::from(
            be::u8_at(table, at.saturating_add(1)).ok_or(truncated(at.saturating_add(2)))?,
        );
        let start = at.saturating_add(2);
        let len = match format & 0x7F {
            0 => count,
            1 => count.saturating_mul(2),
            f => return Err(ReadError::UnknownFormat(u16::from(f))),
        };
        let data = be::slice_at(table, start, len).ok_or(truncated(start.saturating_add(len)))?;
        let kind = if format & 0x7F == 0 {
            Kind::Format0(data)
        } else {
            Kind::Format1(data)
        };
        let mut supplements: &[u8] = &[];
        if format & 0x80 != 0 {
            let s = start.saturating_add(len);
            let n = usize::from(be::u8_at(table, s).ok_or(truncated(s.saturating_add(1)))?);
            let s = s.saturating_add(1);
            supplements = be::slice_at(table, s, n.saturating_mul(3))
                .ok_or(truncated(s.saturating_add(n.saturating_mul(3))))?;
        }
        Ok(Encoding {
            kind,
            supplements,
            num_glyphs,
        })
    }

    /// The format byte's format, 0 or 1; `None` for a predefined
    /// encoding.
    pub fn format(&self) -> Option<u8> {
        match self.kind {
            Kind::Predefined(_) => None,
            Kind::Format0(_) => Some(0),
            Kind::Format1(_) => Some(1),
        }
    }

    /// The supplements as `(code, SID)`.
    pub fn supplements(&self) -> impl Iterator<Item = (u8, u16)> + 'a {
        self.supplements
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| (c[0], u16::from_be_bytes([c[1], c[2]])))
    }

    /// The glyph of `code` as FreeType's `encoding->codes` gives it, with
    /// `charset` the font's charset; 0 for none. Later entries win over
    /// earlier ones; a supplement whose SID is in no glyph changes
    /// nothing. A predefined encoding maps a code to the first glyph with
    /// the code's SID.
    pub fn glyph(&self, code: u8, charset: &Charset<'_>) -> u32 {
        let mut glyph = 0u32;
        match self.kind {
            Kind::Predefined(set) => {
                let sid = set.get(usize::from(code)).copied().unwrap_or(0);
                if sid != 0 {
                    glyph = charset.glyph(sid).unwrap_or(0);
                }
                return glyph;
            }
            Kind::Format0(codes) => {
                for (j, &c) in (1u32..).zip(codes) {
                    if c == code && j < self.num_glyphs {
                        glyph = j;
                    }
                }
            }
            Kind::Format1(ranges) => {
                let mut i = 1u32;
                for r in ranges.as_chunks::<2>().0 {
                    let left = u32::from(r[1]).saturating_add(1);
                    let first = u32::from(r[0]);
                    for k in 0..left {
                        let g = i.saturating_add(k);
                        let c = first.saturating_add(k);
                        if g < self.num_glyphs && c == u32::from(code) {
                            glyph = g;
                        }
                    }
                    i = i.saturating_add(left);
                }
            }
        }
        for (c, sid) in self.supplements() {
            if c == code
                && let Some(g) = charset.glyph(sid)
            {
                glyph = g;
            }
        }
        glyph
    }
}
