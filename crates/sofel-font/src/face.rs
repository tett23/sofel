//! Common character, metric and bitmap queries for opened font faces (root ADR 0100).

use alloc::vec::Vec;
use sofel_bitmapfont::Glyph;
use sofel_read::{GlyphId, cbdt, cblc, sbix};

use crate::{
    BareCffFace, BitmapFace, CidFace, FaceError, Font, PfrFace, Type1Face, type42::Type42Face,
};

fn scale(value: i32, ppem: u16, upem: u16) -> Result<i32, FaceError> {
    if ppem == 0 || upem == 0 {
        return Err(FaceError::InvalidSize);
    }
    let numerator = i64::from(ppem)
        .checked_mul(64 * 65536)
        .ok_or(FaceError::Overflow)?;
    let divisor = i64::from(upem);
    let fixed = i32::try_from(
        numerator
            .checked_add(divisor / 2)
            .and_then(|n| n.checked_div(divisor))
            .ok_or(FaceError::Overflow)?,
    )
    .map_err(|_| FaceError::Overflow)?;
    let product = i64::from(value)
        .checked_mul(i64::from(fixed))
        .ok_or(FaceError::Overflow)?;
    let rounded = product
        .checked_add(0x8000)
        .and_then(|n| n.checked_add(product >> 63))
        .ok_or(FaceError::Overflow)?
        >> 16;
    i32::try_from(rounded).map_err(|_| FaceError::Overflow)
}

fn sfnt_strikes(font: &Font<'_>) -> Vec<(u32, u32, u32)> {
    let location = [cblc::TAG_CBLC, cblc::TAG_EBLC, cblc::TAG_BLOC]
        .into_iter()
        .find_map(|tag| font.table(tag));
    if let Some(bytes) = location {
        let data = [cbdt::TAG_CBDT, cbdt::TAG_EBDT, cbdt::TAG_BDAT]
            .into_iter()
            .find_map(|tag| font.table(tag));
        let Some(header) = bytes.first_chunk::<8>() else {
            return Vec::new();
        };
        let [a, b, c, d, e, f, g, h] = *header;
        let version = u32::from_be_bytes([a, b, c, d]);
        let count = u32::from_be_bytes([e, f, g, h]);
        let known = version & 0xffff_0000 == 0x0002_0000
            || version & 0xffff == 0x0000_0200
            || version & 0xffff_0000 == 0x0003_0000
            || version & 0xffff == 0x0000_0300;
        if !known || count >= 0x10000 || data.is_none_or(<[u8]>::is_empty) {
            return Vec::new();
        }
        let Ok(view) = cblc::Cblc::new_clamped(bytes) else {
            return Vec::new();
        };
        return (0..view.num_sizes())
            .filter_map(|index| {
                let size = view.size(index)?;
                (size.ppem_x != 0 && size.ppem_y != 0).then_some((
                    u32::try_from(index).ok()?,
                    u32::from(size.ppem_x) * 64,
                    u32::from(size.ppem_y) * 64,
                ))
            })
            .collect();
    }
    let Some(bytes) = font.table(sbix::TAG) else {
        return Vec::new();
    };
    let Ok(view) = sbix::Sbix::new(bytes) else {
        return Vec::new();
    };
    if view.version() < 1 || !matches!(view.flags(), 1 | 3) || view.num_strikes() >= 0x10000 {
        return Vec::new();
    }
    (0..view.num_strikes())
        .filter_map(|index| {
            let offset = usize::try_from(view.strike_offset(index)?).ok()?;
            (offset.checked_add(4)? <= bytes.len()).then_some(())?;
            let ppem = view.strike(index)?.ppem();
            (ppem != 0).then_some((
                u32::try_from(index).ok()?,
                u32::from(ppem) * 64,
                u32::from(ppem) * 64,
            ))
        })
        .collect()
}

fn sfnt_bitmap_advance(font: &Font<'_>, index: u32, glyph: GlyphId) -> Option<i32> {
    let (raw_index, x, _) = *sfnt_strikes(font).get(usize::try_from(index).ok()?)?;
    let location = [cblc::TAG_CBLC, cblc::TAG_EBLC, cblc::TAG_BLOC]
        .into_iter()
        .find_map(|tag| font.table(tag));
    let Some(bytes) = location else {
        let ppem = u16::try_from(x / 64).ok()?;
        return scale(
            i32::try_from(font.default_instance().advance(glyph)).ok()?,
            ppem,
            font.units_per_em(),
        )
        .ok();
    };
    let view = cblc::Cblc::new_clamped(bytes).ok()?;
    let size = view.size(usize::try_from(raw_index).ok()?)?;
    let record = (0..size.number_of_index_subtables)
        .filter_map(|j| view.index_record(&size, j))
        .find(|r| u32::from(r.first_glyph) <= glyph.0 && glyph.0 <= u32::from(r.last_glyph))?;
    let located = view.locate(&record, glyph.0)?;
    let data = [cbdt::TAG_CBDT, cbdt::TAG_EBDT, cbdt::TAG_BDAT]
        .into_iter()
        .find_map(|tag| font.table(tag))?;
    let image = cbdt::Cbdt::new(data)
        .ok()?
        .image(located.offset, located.length)?;
    let metrics = match located.image_format {
        1 | 2 | 8 | 17 => cbdt::GlyphMetrics::small(image)?,
        6 | 7 | 9 | 18 => cbdt::GlyphMetrics::big(image)?,
        5 | 19 => cbdt::GlyphMetrics::big(located.metrics.as_ref()?)?,
        _ => return None,
    };
    i32::from(metrics.hori_advance).checked_mul(64)
}

/// A borrowed, opened face in any supported format.
#[derive(Clone, Copy, Debug)]
pub enum Face<'a> {
    /// An sfnt, collection or dfont face.
    Sfnt(&'a Font<'a>),
    /// A standalone CFF or CEF face.
    BareCff(&'a BareCffFace<'a>),
    /// A Type 1 PFA or PFB face.
    Type1(&'a Type1Face<'a>),
    /// A CID-keyed Type 1 face.
    Cid(&'a CidFace),
    /// A Type 42 face with its PostScript glyph order.
    Type42(&'a Type42Face<'a>),
    /// A Portable Font Resource logical face.
    Pfr(&'a PfrFace<'a>),
    /// A BDF, PCF, FNT or FON fixed-strike face.
    Bitmap(&'a BitmapFace),
}

impl<'a> Face<'a> {
    fn default_strike(self, ppem: u16) -> Option<u32> {
        let ppem = u32::from(ppem);
        (0..self.strike_count()).find(|&index| {
            self.strike(index).is_some_and(|(x, y)| {
                (x.saturating_add(32) / 64, y.saturating_add(32) / 64) == (ppem, ppem)
            })
        })
    }

    /// Number of public glyph indices, including the missing glyph.
    pub fn glyph_count(self) -> u32 {
        match self {
            Self::Sfnt(face) => face.num_glyphs(),
            Self::BareCff(face) => face.num_glyphs(),
            Self::Type1(face) => u32::try_from(face.num_glyphs()).unwrap_or(u32::MAX),
            Self::Cid(face) => u32::try_from(face.num_glyphs()).unwrap_or(u32::MAX),
            Self::Type42(face) => face.num_glyphs(),
            Self::Pfr(face) => face.glyph_count(),
            Self::Bitmap(face) => u32::try_from(face.glyph_count()).unwrap_or(u32::MAX),
        }
    }

    /// Finds a glyph by Unicode code point when the face has a Unicode mapping.
    pub fn nominal_glyph(self, codepoint: u32) -> Option<GlyphId> {
        match self {
            Self::Sfnt(face) => face.nominal_glyph(codepoint),
            Self::BareCff(face) => face.nominal_glyph(codepoint),
            Self::Type1(face) => face
                .unicode_glyph(codepoint)
                .and_then(|id| u32::try_from(id).ok())
                .map(GlyphId),
            Self::Cid(_) => None,
            Self::Type42(face) => face.nominal_glyph(codepoint),
            Self::Pfr(face) => face.nominal_glyph(codepoint),
            Self::Bitmap(face) => face.nominal_glyph(codepoint),
        }
    }

    /// Number of fixed bitmap strikes exposed by this face.
    pub fn strike_count(self) -> u32 {
        match self {
            Self::Sfnt(face) => u32::try_from(sfnt_strikes(face).len()).unwrap_or(u32::MAX),
            Self::Type42(face) => {
                u32::try_from(sfnt_strikes(&face.inner_font()).len()).unwrap_or(u32::MAX)
            }
            Self::Bitmap(_) => 1,
            Self::Pfr(face) => face.strike_count(),
            _ => 0,
        }
    }

    /// Horizontal and vertical strike sizes in 26.6 pixels per em.
    pub fn strike(self, index: u32) -> Option<(u32, u32)> {
        match self {
            Self::Sfnt(face) => sfnt_strikes(face)
                .get(usize::try_from(index).ok()?)
                .map(|(_, x, y)| (*x, *y)),
            Self::Type42(face) => sfnt_strikes(&face.inner_font())
                .get(usize::try_from(index).ok()?)
                .map(|(_, x, y)| (*x, *y)),
            Self::Bitmap(face) if index == 0 => Some(face.ppem_26_6()),
            Self::Pfr(face) => face.strike(index),
            _ => None,
        }
    }

    fn check_size(self, ppem: u16, strike: Option<u32>) -> Result<(), FaceError> {
        if ppem == 0 {
            return Err(FaceError::InvalidSize);
        }
        if let Some(index) = strike {
            let (x, y) = self.strike(index).ok_or(FaceError::InvalidStrike)?;
            let ppem = u32::from(ppem);
            if (x.saturating_add(32) / 64, y.saturating_add(32) / 64) != (ppem, ppem) {
                return Err(FaceError::InvalidStrike);
            }
        } else if matches!(self, Self::Bitmap(_))
            || matches!(self, Self::Sfnt(face) if !face.has_outlines())
            || matches!(self, Self::Type42(face) if !face.inner_font().has_outlines())
            || matches!(self, Self::Pfr(face) if face.bitmap_only())
        {
            self.check_size(
                ppem,
                Some(self.default_strike(ppem).ok_or(FaceError::InvalidStrike)?),
            )?;
        }
        Ok(())
    }

    /// Horizontal advance in signed 26.6 pixels at a size and optional strike.
    pub fn h_advance(
        self,
        glyph: GlyphId,
        ppem: u16,
        strike: Option<u32>,
    ) -> Result<i32, FaceError> {
        self.check_size(ppem, strike)?;
        let strike = match self {
            Self::Sfnt(face) if strike.is_none() && !face.has_outlines() => {
                self.default_strike(ppem)
            }
            Self::Type42(face) if strike.is_none() && !face.inner_font().has_outlines() => {
                self.default_strike(ppem)
            }
            Self::Pfr(face) if strike.is_none() && face.bitmap_only() => self.default_strike(ppem),
            _ => strike,
        };
        if glyph.0 >= self.glyph_count() {
            return Err(FaceError::InvalidGlyph);
        }
        let (value, upem) = match self {
            Self::Sfnt(face) => {
                if let Some(index) = strike
                    && let Some(value) = sfnt_bitmap_advance(face, index, glyph)
                {
                    return Ok(value);
                }
                (
                    i32::try_from(face.default_instance().advance(glyph))
                        .map_err(|_| FaceError::Overflow)?,
                    face.units_per_em(),
                )
            }
            Self::BareCff(face) => (
                face.h_advance(glyph).ok_or(FaceError::InvalidGlyph)?,
                u16::try_from(face.cff().top_matrix().units_per_em)
                    .map_err(|_| FaceError::Overflow)?,
            ),
            Self::Type1(face) => (
                face.glyph_metrics(usize::try_from(glyph.0).map_err(|_| FaceError::Overflow)?)
                    .ok_or(FaceError::InvalidGlyph)?
                    .advance_x,
                1000,
            ),
            Self::Cid(face) => (
                face.glyph_metrics(usize::try_from(glyph.0).map_err(|_| FaceError::Overflow)?)
                    .ok_or(FaceError::InvalidGlyph)?
                    .advance_x,
                face.units_per_em(),
            ),
            Self::Type42(face) => {
                if let Some(index) = strike
                    && let Some(embedded) = face.embedded_glyph(glyph)
                    && let Some(value) = sfnt_bitmap_advance(&face.inner_font(), index, embedded)
                {
                    return Ok(value);
                }
                (
                    i32::try_from(face.h_advance(glyph).ok_or(FaceError::InvalidGlyph)?)
                        .map_err(|_| FaceError::Overflow)?,
                    face.units_per_em(),
                )
            }
            Self::Pfr(face) => {
                if let Some(index) = strike {
                    let (x, y) = face.strike(index).ok_or(FaceError::InvalidStrike)?;
                    if let Some(advance) = face
                        .bitmap_advance(
                            glyph,
                            u16::try_from(x / 64).map_err(|_| FaceError::Overflow)?,
                            u16::try_from(y / 64).map_err(|_| FaceError::Overflow)?,
                        )
                        .map_err(|_| FaceError::Invalid)?
                    {
                        return Ok(advance);
                    }
                }
                (
                    face.advance(glyph).ok_or(FaceError::InvalidGlyph)?,
                    face.units_per_em(),
                )
            }
            Self::Bitmap(face) => {
                return face
                    .glyph(glyph)
                    .ok_or(FaceError::InvalidGlyph)?
                    .advance
                    .checked_mul(64)
                    .ok_or(FaceError::Overflow);
            }
        };
        scale(value, ppem, upem)
    }

    /// Pair adjustment in signed 26.6 pixels at a size and optional strike.
    pub fn kerning(
        self,
        left: GlyphId,
        right: GlyphId,
        ppem: u16,
        strike: Option<u32>,
    ) -> Result<(i32, i32), FaceError> {
        self.check_size(ppem, strike)?;
        if left.0 >= self.glyph_count() || right.0 >= self.glyph_count() {
            return Err(FaceError::InvalidGlyph);
        }
        let (raw, upem) = match self {
            Self::Type1(face) => (
                face.kerning(
                    usize::try_from(left.0).map_err(|_| FaceError::Overflow)?,
                    usize::try_from(right.0).map_err(|_| FaceError::Overflow)?,
                ),
                1000,
            ),
            Self::Sfnt(face) => (
                (crate::face_kern::sfnt_pair(face, left, right)?, 0),
                face.units_per_em(),
            ),
            Self::BareCff(face) => (
                (0, 0),
                u16::try_from(face.cff().top_matrix().units_per_em)
                    .map_err(|_| FaceError::Overflow)?,
            ),
            Self::Cid(face) => ((0, 0), face.units_per_em()),
            Self::Type42(face) => (
                (
                    crate::face_kern::sfnt_pair(
                        &face.inner_font(),
                        face.embedded_glyph(left).ok_or(FaceError::InvalidGlyph)?,
                        face.embedded_glyph(right).ok_or(FaceError::InvalidGlyph)?,
                    )?,
                    0,
                ),
                face.units_per_em(),
            ),
            Self::Pfr(face) => (
                (face.kerning(left, right).ok_or(FaceError::InvalidGlyph)?, 0),
                face.units_per_em(),
            ),
            Self::Bitmap(_) => return Ok((0, 0)),
        };
        Ok((scale(raw.0, ppem, upem)?, scale(raw.1, ppem, upem)?))
    }

    /// Packed pixels and bitmap metrics for a fixed-strike glyph.
    pub fn bitmap(self, glyph: GlyphId) -> Option<&'a Glyph> {
        match self {
            Self::Bitmap(face) => face.glyph(glyph),
            _ => None,
        }
    }
}

impl<'a> From<&'a BitmapFace> for Face<'a> {
    fn from(value: &'a BitmapFace) -> Self {
        Self::Bitmap(value)
    }
}

impl<'a> From<&'a BareCffFace<'a>> for Face<'a> {
    fn from(value: &'a BareCffFace<'a>) -> Self {
        Self::BareCff(value)
    }
}

impl<'a> From<&'a Font<'a>> for Face<'a> {
    fn from(value: &'a Font<'a>) -> Self {
        Self::Sfnt(value)
    }
}

impl<'a> From<&'a Type1Face<'a>> for Face<'a> {
    fn from(value: &'a Type1Face<'a>) -> Self {
        Self::Type1(value)
    }
}

impl<'a> From<&'a CidFace> for Face<'a> {
    fn from(value: &'a CidFace) -> Self {
        Self::Cid(value)
    }
}

impl<'a> From<&'a Type42Face<'a>> for Face<'a> {
    fn from(value: &'a Type42Face<'a>) -> Self {
        Self::Type42(value)
    }
}

impl<'a> From<&'a PfrFace<'a>> for Face<'a> {
    fn from(value: &'a PfrFace<'a>) -> Self {
        Self::Pfr(value)
    }
}
