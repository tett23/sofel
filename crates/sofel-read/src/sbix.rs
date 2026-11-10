//! The `sbix` (standard bitmap graphics) table (ADR 0023).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/sbix>

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The `sbix` tag.
pub const TAG: Tag = Tag::new(b"sbix");

/// A view over the `sbix` table: the header and the strike offsets.
#[derive(Clone, Copy, Debug)]
pub struct Sbix<'a> {
    data: &'a [u8],
    version: u16,
    flags: u16,
    strikes: &'a [[u8; 4]],
}

/// One strike: its ppem, resolution and glyph data offsets.
#[derive(Clone, Copy, Debug)]
pub struct Strike<'a> {
    data: &'a [u8],
    offset: usize,
    ppem: u16,
    ppi: u16,
}

/// The data of one glyph in a strike.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Glyph<'a> {
    /// `originOffsetX`.
    pub origin_x: i16,
    /// `originOffsetY`.
    pub origin_y: i16,
    /// `graphicType`: `png `, `jpg `, `tiff`, `dupe`, `flip`, ...
    pub graphic_type: Tag,
    /// The graphic data after the 8-byte header.
    pub data: &'a [u8],
}

impl<'a> Sbix<'a> {
    /// A view over `data`; fails with [`ReadError::Truncated`] when the
    /// header or the strike offsets do not fit.
    pub fn new(data: &'a [u8]) -> Result<Sbix<'a>, ReadError> {
        let h = data
            .first_chunk::<8>()
            .ok_or(ReadError::truncated(8, data.len()))?;
        let n = to_usize(u32::from_be_bytes([h[4], h[5], h[6], h[7]]));
        let strikes = be::chunks_at::<4>(data, 8, n).ok_or(ReadError::truncated(
            8usize.saturating_add(n.saturating_mul(4)),
            data.len(),
        ))?;
        Ok(Sbix {
            data,
            version: u16::from_be_bytes([h[0], h[1]]),
            flags: u16::from_be_bytes([h[2], h[3]]),
            strikes,
        })
    }
    /// The table's bytes.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `version`.
    pub fn version(&self) -> u16 {
        self.version
    }
    /// `flags`.
    pub fn flags(&self) -> u16 {
        self.flags
    }
    /// `numStrikes`.
    pub fn num_strikes(&self) -> usize {
        self.strikes.len()
    }
    /// The offset of strike `i` from the start of the table.
    pub fn strike_offset(&self, i: usize) -> Option<u32> {
        self.strikes.get(i).map(|b| u32::from_be_bytes(*b))
    }
    /// Strike `i`; `None` when its header does not fit.
    pub fn strike(&self, i: usize) -> Option<Strike<'a>> {
        let offset = to_usize(self.strike_offset(i)?);
        let h = be::array_at::<4>(self.data, offset)?;
        Some(Strike {
            data: self.data,
            offset,
            ppem: u16::from_be_bytes([h[0], h[1]]),
            ppi: u16::from_be_bytes([h[2], h[3]]),
        })
    }
}

impl<'a> Strike<'a> {
    /// The strike's offset from the start of the table.
    pub fn offset(&self) -> usize {
        self.offset
    }
    /// `ppem`.
    pub fn ppem(&self) -> u16 {
        self.ppem
    }
    /// `ppi`.
    pub fn ppi(&self) -> u16 {
        self.ppi
    }
    /// `glyphDataOffsets[glyph]` and `glyphDataOffsets[glyph + 1]`,
    /// from the start of the strike.
    pub fn glyph_range(&self, glyph: u32) -> Option<(u32, u32)> {
        let at = self
            .offset
            .checked_add(4)?
            .checked_add(to_usize(glyph).checked_mul(4)?)?;
        Some((
            be::u32_at(self.data, at)?,
            be::u32_at(self.data, at.checked_add(4)?)?,
        ))
    }
    /// The data of `glyph`: `None` when its range is empty or reversed,
    /// shorter than the 8-byte header, or outside the table.
    pub fn glyph(&self, glyph: u32) -> Option<Glyph<'a>> {
        let (start, end) = self.glyph_range(glyph)?;
        if end <= start || end.saturating_sub(start) < 8 {
            return None;
        }
        let at = self.offset.checked_add(to_usize(start))?;
        let bytes = be::slice_at(self.data, at, to_usize(end.saturating_sub(start)))?;
        let h = bytes.first_chunk::<8>()?;
        Some(Glyph {
            origin_x: i16::from_be_bytes([h[0], h[1]]),
            origin_y: i16::from_be_bytes([h[2], h[3]]),
            graphic_type: Tag([h[4], h[5], h[6], h[7]]),
            data: bytes.get(8..).unwrap_or_default(),
        })
    }
}

/// Checks an `sbix` table: truncation (and nothing else then); a version
/// of 0; flags other than 1 and 3 ([`Kind::ReservedBitsSet`]); and, with
/// `num_glyphs`, strikes and glyph data out of bounds, glyph offsets that
/// do not ascend, and `dupe` and `flip` glyphs that point past
/// `num_glyphs`.
pub fn validate(data: &[u8], num_glyphs: Option<u32>, report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let sbix = match Sbix::new(data) {
        Ok(s) => s,
        Err(ReadError::Truncated { needed, available }) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed),
                    available: to_u32(available),
                },
            );
        }
        Err(_) => return,
    };
    if sbix.version() == 0 {
        emit(0, Kind::UnknownVersion { found: 0 });
    }
    if !matches!(sbix.flags(), 1 | 3) {
        emit(
            2,
            Kind::ReservedBitsSet {
                found: u32::from(sbix.flags()),
            },
        );
    }
    let Some(n) = num_glyphs else { return };
    for i in 0..sbix.num_strikes() {
        let at = 8usize.saturating_add(i.saturating_mul(4));
        let off = sbix.strike_offset(i).map_or(0, to_usize);
        let len = to_usize(n)
            .saturating_add(1)
            .saturating_mul(4)
            .saturating_add(4);
        let Some(strike) = sbix
            .strike(i)
            .filter(|_| be::slice_at(data, off, len).is_some())
        else {
            emit(
                at,
                Kind::OffsetOutOfBounds {
                    offset: to_u32(off),
                    length: to_u32(len),
                    available: to_u32(data.len()),
                },
            );
            continue;
        };
        for g in 0..n {
            let Some((start, end)) = strike.glyph_range(g) else {
                break;
            };
            let gat = off
                .saturating_add(4)
                .saturating_add(to_usize(g).saturating_mul(4));
            if end < start {
                emit(
                    gat,
                    Kind::OffsetsNotSorted {
                        index: g.saturating_add(1),
                    },
                );
                continue;
            }
            if end == start {
                continue;
            }
            let len = end.saturating_sub(start);
            if be::slice_at(data, off.saturating_add(to_usize(start)), to_usize(len)).is_none()
                || len < 8
            {
                emit(
                    gat,
                    Kind::OffsetOutOfBounds {
                        offset: to_u32(off.saturating_add(to_usize(start))),
                        length: len,
                        available: to_u32(data.len()),
                    },
                );
                continue;
            }
            if let Some(glyph) = strike.glyph(g)
                && matches!(&glyph.graphic_type.0, b"dupe" | b"flip")
                && let Some(target) = be::u16_at(glyph.data, 0)
                && u32::from(target) >= n
            {
                emit(
                    gat,
                    Kind::GlyphIdOutOfRange {
                        glyph: u32::from(target),
                        num_glyphs: n,
                    },
                );
            }
        }
    }
}
