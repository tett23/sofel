//! The `cmap` table.
//!
//! Subtables are read the way FreeType reads them: the data of a
//! subtable extends to the end of the `cmap` table, and the checks that
//! FreeType makes at `FT_VALIDATE_DEFAULT` are the ones that make a
//! subtable unreadable here (ADR 0007, ADR 0008). Choosing a subtable,
//! remapping symbol fonts and converting Mac encodings are left to
//! `sofel-font`.

mod check;
mod formats;

use crate::be::{self, to_usize};
use crate::{GlyphId, ReadError, Tag};

pub use check::validate;
use formats::{Format0, Format2, Format4, Format6, Format8, Format10, Groups, Uvs};

pub const TAG: Tag = Tag::new(b"cmap");

const HEADER: usize = 4;
const RECORD: usize = 8;

#[derive(Clone, Copy, Debug)]
pub struct Cmap<'a> {
    data: &'a [u8],
    version: u16,
    records: &'a [[u8; RECORD]],
}

/// One encoding record.
#[derive(Clone, Copy, Debug)]
pub struct EncodingRecord<'a> {
    pub platform_id: u16,
    pub encoding_id: u16,
    /// Offset of the subtable from the start of the `cmap` table.
    pub offset: u32,
    cmap: &'a [u8],
}

impl<'a> EncodingRecord<'a> {
    pub fn subtable(&self) -> Result<Subtable<'a>, ReadError> {
        Subtable::new(self.cmap, self.offset)
    }
}

impl<'a> Cmap<'a> {
    /// Reads the header and the encoding records that are wholly present;
    /// like FreeType, a cut-short record array keeps its whole records.
    pub fn new(data: &'a [u8]) -> Result<Cmap<'a>, ReadError> {
        let h = data
            .first_chunk::<HEADER>()
            .ok_or(ReadError::truncated(HEADER, data.len()))?;
        let version = u16::from_be_bytes([h[0], h[1]]);
        let declared = usize::from(u16::from_be_bytes([h[2], h[3]]));
        let all = be::tail_at(data, HEADER)
            .unwrap_or_default()
            .as_chunks::<RECORD>()
            .0;
        let records = all.get(..declared).unwrap_or(all);
        Ok(Cmap {
            data,
            version,
            records,
        })
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn records(&self) -> impl Iterator<Item = EncodingRecord<'a>> + 'a {
        let cmap = self.data;
        self.records.iter().map(move |r| EncodingRecord {
            platform_id: u16::from_be_bytes([r[0], r[1]]),
            encoding_id: u16::from_be_bytes([r[2], r[3]]),
            offset: u32::from_be_bytes([r[4], r[5], r[6], r[7]]),
            cmap,
        })
    }

    /// The subtable of the first record with this platform and encoding.
    pub fn find(
        &self,
        platform_id: u16,
        encoding_id: u16,
    ) -> Option<Result<Subtable<'a>, ReadError>> {
        self.records()
            .find(|r| r.platform_id == platform_id && r.encoding_id == encoding_id)
            .map(|r| r.subtable())
    }
}

/// The result of a variation sequence lookup (formats 14 and 15).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VariantGlyph {
    /// Use the glyph that the default mapping gives the base character.
    Default,
    /// Use this glyph.
    Glyph(GlyphId),
}

/// A cmap subtable of any supported format.
#[derive(Clone, Copy, Debug)]
pub enum Subtable<'a> {
    Format0(Format0<'a>),
    Format2(Format2<'a>),
    Format4(Format4<'a>),
    Format6(Format6<'a>),
    Format8(Format8<'a>),
    Format10(Format10<'a>),
    Format12(Groups<'a>),
    Format13(Groups<'a>),
    Format14(Uvs<'a>),
    Format15(Uvs<'a>),
}

impl<'a> Subtable<'a> {
    /// The subtable at `offset` in the `cmap` table `cmap`.
    pub fn new(cmap: &'a [u8], offset: u32) -> Result<Subtable<'a>, ReadError> {
        let off = to_usize(offset);
        if off == 0 {
            return Err(ReadError::OutOfBounds);
        }
        let data = be::tail_at(cmap, off).ok_or(ReadError::OutOfBounds)?;
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        Ok(match format {
            0 => Subtable::Format0(Format0::new(data)?),
            2 => Subtable::Format2(Format2::new(data)?),
            4 => Subtable::Format4(Format4::new(data)?),
            6 => Subtable::Format6(Format6::new(data)?),
            8 => Subtable::Format8(Format8::new(data)?),
            10 => Subtable::Format10(Format10::new(data)?),
            12 => Subtable::Format12(Groups::new(data, false)?),
            13 => Subtable::Format13(Groups::new(data, true)?),
            14 => Subtable::Format14(Uvs::new(data, false)?),
            15 => Subtable::Format15(Uvs::new(data, true)?),
            other => return Err(ReadError::UnknownFormat(other)),
        })
    }

    pub fn format(&self) -> u16 {
        match self {
            Subtable::Format0(_) => 0,
            Subtable::Format2(_) => 2,
            Subtable::Format4(_) => 4,
            Subtable::Format6(_) => 6,
            Subtable::Format8(_) => 8,
            Subtable::Format10(_) => 10,
            Subtable::Format12(_) => 12,
            Subtable::Format13(_) => 13,
            Subtable::Format14(_) => 14,
            Subtable::Format15(_) => 15,
        }
    }

    /// The language field (0 for formats 14 and 15, which have none).
    pub fn language(&self) -> u32 {
        match self {
            Subtable::Format0(s) => s.language(),
            Subtable::Format2(s) => s.language(),
            Subtable::Format4(s) => s.language(),
            Subtable::Format6(s) => s.language(),
            Subtable::Format8(s) => s.language(),
            Subtable::Format10(s) => s.language(),
            Subtable::Format12(s) | Subtable::Format13(s) => s.language(),
            Subtable::Format14(_) | Subtable::Format15(_) => 0,
        }
    }

    /// The glyph for `codepoint`, or `None` when the code is unmapped or
    /// maps to glyph 0. Glyph ids are returned as stored, even when they
    /// are not below `numGlyphs`.
    pub fn glyph(&self, codepoint: u32) -> Option<GlyphId> {
        let g = match self {
            Subtable::Format0(s) => s.glyph(codepoint),
            Subtable::Format2(s) => s.glyph(codepoint),
            Subtable::Format4(s) => s.glyph(codepoint),
            Subtable::Format6(s) => s.glyph(codepoint),
            Subtable::Format8(s) => s.glyph(codepoint),
            Subtable::Format10(s) => s.glyph(codepoint),
            Subtable::Format12(s) | Subtable::Format13(s) => s.glyph(codepoint),
            Subtable::Format14(_) | Subtable::Format15(_) => None,
        }?;
        (g != 0).then_some(GlyphId(g))
    }

    /// The glyph for the variation sequence `codepoint` + `selector`
    /// (formats 14 and 15 only).
    pub fn variant(&self, codepoint: u32, selector: u32) -> Option<VariantGlyph> {
        match self {
            Subtable::Format14(s) | Subtable::Format15(s) => s.variant(codepoint, selector),
            _ => None,
        }
    }

    /// The mapped `(codepoint, glyph)` pairs in ascending code order.
    /// Glyph 0 is skipped. Every pair agrees with [`Subtable::glyph`].
    pub fn mappings(&self) -> Mappings<'a> {
        Mappings {
            sub: *self,
            range: 0,
            next: 0,
        }
    }

    /// The `index`-th code range to look at when listing mappings.
    fn range(&self, index: usize) -> Option<(u32, u32)> {
        match self {
            Subtable::Format0(_) => (index == 0).then_some((0, 0xFF)),
            Subtable::Format2(s) => s.range(index),
            Subtable::Format4(s) => s.range(index),
            Subtable::Format6(s) => (index == 0).then(|| s.range()).flatten(),
            Subtable::Format8(s) => s.range(index),
            Subtable::Format10(s) => (index == 0).then(|| s.range()).flatten(),
            Subtable::Format12(s) | Subtable::Format13(s) => s.range(index),
            Subtable::Format14(_) | Subtable::Format15(_) => None,
        }
    }
}

/// Iterator returned by [`Subtable::mappings`].
#[derive(Clone, Debug)]
pub struct Mappings<'a> {
    sub: Subtable<'a>,
    range: usize,
    /// The smallest code not yet looked at.
    next: u64,
}

impl Iterator for Mappings<'_> {
    type Item = (u32, GlyphId);

    fn next(&mut self) -> Option<(u32, GlyphId)> {
        loop {
            let (start, end) = self.sub.range(self.range)?;
            let from = self.next.max(u64::from(start));
            if from > u64::from(end) {
                self.range = self.range.checked_add(1)?;
                continue;
            }
            let cp = u32::try_from(from).ok()?;
            self.next = from.checked_add(1)?;
            if let Some(g) = self.sub.glyph(cp) {
                return Some((cp, g));
            }
        }
    }
}
