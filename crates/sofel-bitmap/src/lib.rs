//! Embedded bitmap glyphs (ADR 0023): the strikes of `CBLC`, `EBLC`,
//! `bloc` and `sbix`, their selection, and each glyph's image data and
//! metrics, including premultiplied BGRA from embedded PNG images.
//!
//! [`strikes`], [`match_strike`] and [`glyph_bitmap`] follow FreeType
//! 2.15.0; they are ported from `src/sfnt/ttsbit.c`
//! (`tt_face_load_sbit`, `tt_face_load_strike_metrics`,
//! `tt_sbit_decoder_load_image`, `tt_sbit_decoder_load_bitmap`,
//! `tt_face_load_sbix_image`), `src/sfnt/sfobjs.c` (the
//! `available_sizes`), `src/sfnt/ttmtx.c` (`tt_face_get_metrics`) and
//! `src/base/ftobjs.c` (`FT_Match_Size`). Copyright (C) 1996-2026 by
//! David Turner, Robert Wilhelm, and Werner Lemberg. Used under the
//! FreeType License (FTL); see THIRD_PARTY_NOTICES.md.
//!
//! [`choose_strike`] and the PNG glyphs for painting ([`cbdt_png`],
//! [`sbix_png`]) and their boxes ([`cbdt_extents`], [`sbix_extents`])
//! follow HarfBuzz 15.1.0's `src/OT/Color/CBDT/CBDT.hh` and
//! `src/OT/Color/sbix/sbix.hh`. Copyright © the HarfBuzz authors. Used
//! under HarfBuzz's "Old MIT" license; see THIRD_PARTY_NOTICES.md.
//!
//! # Example
//!
//! List the strikes and get a glyph's bitmap data at 16 pixels:
//!
//! ```no_run
//! use sofel_bitmap::{glyph_bitmap, match_strike, strikes};
//! use sofel_font::Font;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let data = std::fs::read("font.ttf")?;
//!     let font = Font::new(&data, 0)?;
//!     for s in strikes(&font) {
//!         println!("{} x {} ppem", s.x_ppem, s.y_ppem);
//!     }
//!     if let Some(strike) = match_strike(&font, 16 * 64, 16 * 64) {
//!         if let Some(b) = glyph_bitmap(&font, &strike, 3) {
//!             println!("{:?}: {} bytes, {:?}", b.format, b.data.len(), b.metrics);
//!         }
//!     }
//!     Ok(())
//! }
//! ```

#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

extern crate alloc;

mod convert;
mod decode;
mod embolden;

pub use convert::{BitmapView, ConvertError, ConvertedBitmap, convert_bitmap};
pub use decode::{BitmapError, BitmapLayout, PixelMode};
pub use embolden::{
    BitmapSlotMetrics, EmboldenedBitmap, EmboldenedBitmapSlot, embolden_bitmap,
    embolden_bitmap_slot,
};

use sofel_font::geom::float::{hb_clamp_to_i32, hb_roundf32, i32_to_f32};
use sofel_font::geom::{GlyphExtents, mul_div};
use sofel_font::read::cbdt::{self, Cbdt, GlyphMetrics};
use sofel_font::read::cblc::{self, BitmapSize, Cblc, IndexRecord};
use sofel_font::read::hhea::{self, Hhea};
use sofel_font::read::sbix::{self, Sbix};
use sofel_font::read::{CountWidth, Tag, hmtx, os2, vhea, vmtx};
use sofel_font::{Font, HbScale};

/// The table a strike comes from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StrikeTable {
    /// `CBLC` with `CBDT`.
    Cblc,
    /// `EBLC` with `EBDT`.
    Eblc,
    /// Apple's `bloc` with `bdat`.
    Bloc,
    /// `sbix`.
    Sbix,
}

/// A strike: FreeType's `FT_Bitmap_Size` and where it comes from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Strike {
    /// The strike's index in its table (BitmapSize record or `sbix`
    /// strike).
    pub index: u32,
    /// The table.
    pub table: StrikeTable,
    /// Horizontal pixels per em.
    pub x_ppem: u16,
    /// Vertical pixels per em.
    pub y_ppem: u16,
    /// `bitDepth` (1, 2, 4, 8 or 32); 32 for `sbix`.
    pub bit_depth: u8,
    /// `sbix`'s `ppi`; 0 for the other tables.
    pub resolution: u16,
    /// `FT_Bitmap_Size.height`: the strike's text height in pixels.
    pub height: i16,
    /// `FT_Bitmap_Size.width`: the average width in pixels.
    pub width: i16,
}

/// A glyph's bitmap metrics in pixels, as FreeType's
/// `TT_SBit_MetricsRec`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct BitmapMetrics {
    /// Width.
    pub width: u16,
    /// Height (rows).
    pub height: u16,
    /// Horizontal left bearing: `bitmap_left`.
    pub hori_bearing_x: i16,
    /// Horizontal top bearing: `bitmap_top`.
    pub hori_bearing_y: i16,
    /// Horizontal advance.
    pub hori_advance: u16,
    /// Vertical left bearing.
    pub vert_bearing_x: i16,
    /// Vertical top bearing.
    pub vert_bearing_y: i16,
    /// Vertical advance.
    pub vert_advance: u16,
}

/// The format of a glyph's image data.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BitmapFormat {
    /// An `EBDT`, `CBDT` or `bdat` image format (1, 2, 5, 6, 7, 8, 9,
    /// 17, 18 or 19).
    Ebdt(u16),
    /// An `sbix` graphic type (`png `, `jpg `, `tiff`, ...).
    Sbix(Tag),
}

/// A glyph's undecoded image and its metrics.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphBitmap<'a> {
    /// The format.
    pub format: BitmapFormat,
    /// The image data: for `EBDT` formats the bytes after the metrics
    /// (for format 8 after the padding byte too; for formats 17 to 19
    /// the PNG file after its length); for `sbix` the graphic after the
    /// 8-byte header.
    pub data: &'a [u8],
    /// The metrics. For `sbix` PNG graphics the size comes from the PNG
    /// header; for other `sbix` graphics it is 0.
    pub metrics: BitmapMetrics,
    /// The strike's bit depth.
    pub bit_depth: u8,
    /// `true` for an `sbix` glyph reached through `flip`: the image is
    /// to be mirrored horizontally.
    pub flipped: bool,
}

/// A PNG glyph for painting, as HarfBuzz finds it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PngGlyph<'a> {
    /// The PNG data.
    pub data: &'a [u8],
    /// The box in pixels of the strike (HarfBuzz's `get_extents` with
    /// `scale` false).
    pub pixel_extents: GlyphExtents,
}

/// FreeType's location table: `CBLC`, else `EBLC`, else `bloc`, else
/// `sbix` (`tt_face_load_sbit`).
#[derive(Clone, Copy, Debug)]
enum Source<'a> {
    Eblc(StrikeTable, Cblc<'a>, &'a [u8]),
    Sbix(Sbix<'a>),
}

fn ft_source<'a>(font: &Font<'a>) -> Option<Source<'a>> {
    let found = [
        (cblc::TAG_CBLC, StrikeTable::Cblc),
        (cblc::TAG_EBLC, StrikeTable::Eblc),
        (cblc::TAG_BLOC, StrikeTable::Bloc),
    ]
    .into_iter()
    .find_map(|(tag, kind)| font.table(tag).map(|d| (kind, d)));
    let data_table = || {
        [cbdt::TAG_CBDT, cbdt::TAG_EBDT, cbdt::TAG_BDAT]
            .into_iter()
            .find_map(|t| font.table(t))
            .filter(|d| !d.is_empty())
    };
    if let Some((kind, d)) = found {
        let h = d.first_chunk::<8>()?;
        let version = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
        let known = version & 0xFFFF_0000 == 0x0002_0000
            || version & 0xFFFF == 0x0000_0200
            || version & 0xFFFF_0000 == 0x0003_0000
            || version & 0xFFFF == 0x0000_0300;
        let n = u32::from_be_bytes([h[4], h[5], h[6], h[7]]);
        if !known || n >= 0x10000 {
            return None;
        }
        // FreeType counts the strikes that fit.
        let view = Cblc::new_clamped(d).ok()?;
        return Some(Source::Eblc(kind, view, data_table()?));
    }
    let d = font.table(sbix::TAG)?;
    let version = u16::from_be_bytes([*d.first()?, *d.get(1)?]);
    let flags = u16::from_be_bytes([*d.get(2)?, *d.get(3)?]);
    let n = u32::from_be_bytes([*d.get(4)?, *d.get(5)?, *d.get(6)?, *d.get(7)?]);
    if version < 1 || !matches!(flags, 1 | 3) || n >= 0x10000 {
        return None;
    }
    Sbix::new(d).ok().map(Source::Sbix)
}

/// FreeType's `tt_face_load_strike_metrics` → `FT_Bitmap_Size`: `None`
/// when the strike's metrics cannot be read or a ppem is 0.
fn strike_of(font: &Font<'_>, source: &Source<'_>, index: u32) -> Option<Strike> {
    let upem = i32::from(font.head().units_per_em());
    // FreeType does not load OS/2 for Apple's bitmap-only fonts (`bhed`
    // instead of `head`); its zeroed OS/2 then gives an average width 0.
    let apple_sbit = !font.has_outlines()
        && font.table(sofel_font::read::head::TAG_BHED).is_some()
        && font.table(sbix::TAG).is_none();
    let (avg, em) = match font.os2() {
        _ if apple_sbit => (0, upem),
        Some(o) if upem != 0 => (i32::from(o.x_avg_char_width()), upem),
        _ => (1, 1),
    };
    let width = |x_ppem: i32| {
        let w = avg
            .saturating_mul(x_ppem)
            .saturating_add(em / 2)
            .checked_div(em)
            .unwrap_or(0);
        short(w)
    };
    let strike = match source {
        Source::Eblc(kind, view, _) => {
            let s = view.size(usize::try_from(index).ok()?)?;
            let (x_ppem, y_ppem) = (u16::from(s.ppem_x), u16::from(s.ppem_y));
            let mut ascender = i32::from(s.hori.ascender).saturating_mul(64);
            let mut descender = i32::from(s.hori.descender).saturating_mul(64);
            let (max_before, min_after) = (s.hori.max_before_bl, s.hori.min_after_bl);
            if descender > 0 {
                if min_after < 0 {
                    descender = descender.saturating_neg();
                }
            } else if descender == 0 && ascender == 0 {
                if max_before != 0 || min_after != 0 {
                    ascender = i32::from(max_before).saturating_mul(64);
                    descender = i32::from(min_after).saturating_mul(64);
                } else {
                    ascender = i32::from(y_ppem).saturating_mul(64);
                    descender = 0;
                }
            }
            let mut height = ascender.saturating_sub(descender);
            if height == 0 {
                height = i32::from(y_ppem).saturating_mul(64);
            }
            Strike {
                index,
                table: *kind,
                x_ppem,
                y_ppem,
                bit_depth: s.bit_depth,
                resolution: 0,
                height: short(height >> 6),
                width: width(i32::from(x_ppem)),
            }
        }
        Source::Sbix(view) => {
            let i = usize::try_from(index).ok()?;
            let off = view.strike_offset(i)?;
            if usize::try_from(off).ok()?.checked_add(4)? > view.data().len() {
                return None;
            }
            let st = view.strike(i)?;
            let ppem = st.ppem();
            let scale = sofel_font::geom::div_fix(i32::from(ppem).saturating_mul(64), upem);
            let (asc, desc, gap) = ft_hhea(font);
            let height =
                sofel_font::geom::mul_fix(asc.saturating_sub(desc).saturating_add(gap), scale);
            Strike {
                index,
                table: StrikeTable::Sbix,
                x_ppem: ppem,
                y_ppem: ppem,
                bit_depth: 32,
                resolution: st.ppi(),
                height: short(height >> 6),
                width: width(i32::from(ppem)),
            }
        }
    };
    (strike.x_ppem != 0 && strike.y_ppem != 0).then_some(strike)
}

/// `(FT_Short) v`: the low 16 bits.
fn short(v: i32) -> i16 {
    let [a, b, _, _] = v.to_le_bytes();
    i16::from_le_bytes([a, b])
}

/// FreeType's `face->horizontal` ascender, descender and line gap: from
/// `hhea` (0 when it cannot be read).
fn ft_hhea(font: &Font<'_>) -> (i32, i32, i32) {
    font.table(hhea::TAG)
        .and_then(|d| Hhea::new(d, CountWidth::U16).ok())
        .map_or((0, 0, 0), |h| {
            (
                i32::from(h.ascender()),
                i32::from(h.descender()),
                i32::from(h.line_gap()),
            )
        })
}

/// FreeType's strikes (`FT_Face.available_sizes`), in order: those of
/// `CBLC`, else `EBLC`, else `bloc` (with a data table `CBDT`, `EBDT`
/// or `bdat`), else `sbix`, leaving out strikes whose metrics cannot be
/// read or whose ppem is 0.
pub fn strikes<'a>(font: &Font<'a>) -> impl Iterator<Item = Strike> + 'a {
    let font = *font;
    let source = ft_source(&font);
    let count = match &source {
        Some(Source::Eblc(_, v, _)) => v.num_sizes(),
        Some(Source::Sbix(v)) => v.num_strikes(),
        None => 0,
    };
    (0..u32::try_from(count).unwrap_or(0))
        .filter_map(move |i| source.as_ref().and_then(|s| strike_of(&font, s, i)))
}

/// The strike HarfBuzz would pick for `ppem` among [`strikes`]: the
/// smallest whose ppem (the larger of x and y) is at least `ppem`, else
/// the largest; for `ppem` 0 the largest. Of strikes with equal ppem the
/// first wins.
pub fn choose_strike(font: &Font<'_>, ppem: u16) -> Option<Strike> {
    let key = |s: &Strike| s.x_ppem.max(s.y_ppem);
    let requested = if ppem == 0 { u32::MAX } else { u32::from(ppem) };
    let best = |pick: &dyn Fn(&Strike, &Strike) -> bool, filter: &dyn Fn(&Strike) -> bool| {
        strikes(font)
            .filter(|s| filter(s))
            .fold(None::<Strike>, |b, s| match b {
                Some(b) if !pick(&s, &b) => Some(b),
                _ => Some(s),
            })
    };
    best(&|s, b| key(s) < key(b), &|s| u32::from(key(s)) >= requested)
        .or_else(|| best(&|s, b| key(s) > key(b), &|_| true))
}

/// FreeType's `FT_Match_Size` for a nominal request of `x_ppem` ×
/// `y_ppem` pixels in 26.6 (a 0 takes the other value): the first
/// strike whose rounded ppems equal the rounded request; `None` when
/// none does or the request rounds to 0.
pub fn match_strike(font: &Font<'_>, x_ppem: u32, y_ppem: u32) -> Option<Strike> {
    let (w, h) = match (x_ppem, y_ppem) {
        (w, 0) => (w, w),
        (0, h) => (h, h),
        wh => wh,
    };
    let round = |v: u32| v.saturating_add(32) & !63;
    let (w, h) = (round(w), round(h));
    if w == 0 || h == 0 {
        return None;
    }
    strikes(font).find(|s| {
        round(u32::from(s.y_ppem).saturating_mul(64)) == h
            && round(u32::from(s.x_ppem).saturating_mul(64)) == w
    })
}

fn metrics_of(m: &GlyphMetrics) -> BitmapMetrics {
    BitmapMetrics {
        width: u16::from(m.width),
        height: u16::from(m.height),
        hori_bearing_x: i16::from(m.hori_bearing_x),
        hori_bearing_y: i16::from(m.hori_bearing_y),
        hori_advance: u16::from(m.hori_advance),
        vert_bearing_x: i16::from(m.vert_bearing_x),
        vert_bearing_y: i16::from(m.vert_bearing_y),
        vert_advance: u16::from(m.vert_advance),
    }
}

/// FreeType's search for the index subtable of `glyph` in strike `s`:
/// the first record whose range covers it.
fn find_record(view: &Cblc<'_>, s: &BitmapSize, glyph: u32) -> Option<IndexRecord> {
    (0..s.number_of_index_subtables)
        .map_while(|j| view.index_record(s, j))
        .find(|r| u32::from(r.first_glyph) <= glyph && glyph <= u32::from(r.last_glyph))
}

/// The image data and metrics of `glyph` in `strike`, as FreeType's
/// `FT_Load_Glyph` reads them without decoding (`tt_face_load_sbit_image`
/// with `FT_LOAD_BITMAP_METRICS_ONLY` for the metrics). `None` when the
/// glyph has no bitmap there, or the data cannot be read; for `sbix`,
/// `dupe` and `flip` are followed up to 4 times.
pub fn glyph_bitmap<'a>(font: &Font<'a>, strike: &Strike, glyph: u32) -> Option<GlyphBitmap<'a>> {
    match ft_source(font)? {
        Source::Eblc(_, view, data) => {
            let s = view.size(usize::try_from(strike.index).ok()?)?;
            let r = find_record(&view, &s, glyph)?;
            let loc = view.locate(&r, glyph)?;
            let image = Cbdt::new(data).ok()?.image(loc.offset, loc.length)?;
            let index_metrics = loc.metrics.as_ref().and_then(|m| GlyphMetrics::big(m));
            let (metrics, body) = match loc.image_format {
                1 | 2 | 8 | 17 => (GlyphMetrics::small(image)?, image.get(5..)?),
                6 | 7 | 9 | 18 => (GlyphMetrics::big(image)?, image.get(8..)?),
                5 | 19 => (index_metrics?, image),
                _ => return None,
            };
            let body = match loc.image_format {
                8 => body.get(1..)?,
                17..=19 => {
                    let len =
                        usize::try_from(u32::from_be_bytes(*body.first_chunk::<4>()?)).ok()?;
                    body.get(4..)?.get(..len)?
                }
                _ => body,
            };
            if !matches!(s.bit_depth, 1 | 2 | 4 | 8 | 32) {
                return None;
            }
            Some(GlyphBitmap {
                format: BitmapFormat::Ebdt(loc.image_format),
                data: body,
                metrics: metrics_of(&metrics),
                bit_depth: s.bit_depth,
                flipped: false,
            })
        }
        Source::Sbix(view) => {
            let st = view.strike(usize::try_from(strike.index).ok()?)?;
            let (mut g, mut flipped) = (glyph, false);
            for depth in 0..=4 {
                if g > font.num_glyphs() {
                    return None;
                }
                let found = st.glyph(g)?;
                match &found.graphic_type.0 {
                    b"flip" | b"dupe" => {
                        if depth == 4 {
                            return None;
                        }
                        if &found.graphic_type.0 == b"flip" {
                            flipped = !flipped;
                        }
                        g = u32::from(u16::from_be_bytes(*found.data.first_chunk::<2>()?));
                    }
                    _ => {
                        let (w, h) = if &found.graphic_type.0 == b"png " {
                            png_size(found.data)
                        } else {
                            (0, 0)
                        };
                        let height = u16::try_from(h).unwrap_or(u16::MAX);
                        let x_ppem = i32::from(strike.x_ppem);
                        let upem = i32::from(font.head().units_per_em());
                        let adv = |a: u16| {
                            u16::try_from(mul_div(i32::from(a), x_ppem, upem)).unwrap_or(0)
                        };
                        return Some(GlyphBitmap {
                            format: BitmapFormat::Sbix(found.graphic_type),
                            data: found.data,
                            metrics: BitmapMetrics {
                                width: u16::try_from(w).unwrap_or(u16::MAX),
                                height,
                                hori_bearing_x: found.origin_x,
                                hori_bearing_y: found
                                    .origin_y
                                    .wrapping_add(i16::from_ne_bytes(height.to_ne_bytes())),
                                hori_advance: adv(ft_advance(font, g, false)),
                                vert_bearing_x: found.origin_x,
                                vert_bearing_y: found.origin_y,
                                vert_advance: adv(ft_v_advance(font, g)),
                            },
                            bit_depth: 32,
                            flipped,
                        });
                    }
                }
            }
            None
        }
    }
}

/// The layout and metrics of `glyph`'s bitmap in `strike`, as FreeType's
/// `FT_Load_Glyph` with `FT_LOAD_BITMAP_METRICS_ONLY` gives them for
/// `EBDT`, `CBDT` and `bdat` (`tt_sbit_decoder_load_image`), compound
/// glyphs included. Embedded PNG images use premultiplied BGRA.
pub fn bitmap_layout(
    font: &Font<'_>,
    strike: &Strike,
    glyph: u32,
) -> Result<BitmapLayout, BitmapError> {
    if let Some(image) = glyph_bitmap(font, strike, glyph)
        && (matches!(image.format, BitmapFormat::Ebdt(17..=19))
            || image.format == BitmapFormat::Sbix(Tag::new(b"png ")))
    {
        return png_bitmap(font, strike, glyph, &image, None);
    }
    match ft_source(font) {
        Some(Source::Eblc(_, view, data)) => decode::decode(
            &view,
            data,
            usize::try_from(strike.index).unwrap_or(usize::MAX),
            glyph,
            None,
        ),
        Some(Source::Sbix(_)) => Err(BitmapError::Png),
        None => Err(BitmapError::Missing),
    }
}

/// Decodes `glyph`'s bitmap in `strike` into `out` as FreeType's
/// `FT_Load_Glyph` with `FT_LOAD_COLOR` does: [`BitmapLayout::len`]
/// bytes, rows top first, `pitch` bytes each; 1, 2 and 4-bit pixels
/// most significant bit first, 32-bit pixels as stored (BGRA). Image
/// formats 1, 2, 5, 6, 7 and the compound formats 8 and 9; PNG images
/// are written as premultiplied BGRA. [`bitmap_layout`] gives the size first.
pub fn draw_bitmap(
    font: &Font<'_>,
    strike: &Strike,
    glyph: u32,
    out: &mut [u8],
) -> Result<BitmapLayout, BitmapError> {
    if let Some(image) = glyph_bitmap(font, strike, glyph)
        && (matches!(image.format, BitmapFormat::Ebdt(17..=19))
            || image.format == BitmapFormat::Sbix(Tag::new(b"png ")))
    {
        return png_bitmap(font, strike, glyph, &image, Some(out));
    }
    match ft_source(font) {
        Some(Source::Eblc(_, view, data)) => decode::decode(
            &view,
            data,
            usize::try_from(strike.index).unwrap_or(usize::MAX),
            glyph,
            Some(out),
        ),
        Some(Source::Sbix(_)) => Err(BitmapError::Png),
        None => Err(BitmapError::Missing),
    }
}

// ttgload.c shifts a horizontal sbix bitmap for simple outline glyphs.
fn sbix_origin_shift(font: &Font<'_>, strike: &Strike, glyph: u32) -> Option<(i16, i16)> {
    use sofel_font::geom::{div_fix, mul_fix};
    use sofel_font::read::{GlyphId, glyf, loca};
    let offsets =
        loca::Loca::new(font.table(loca::TAG)?, font.head().index_to_loc_format()).ok()?;
    let index = usize::try_from(glyph).ok()?;
    let bytes = font
        .table(glyf::TAG)?
        .get(offsets.offset(index)? as usize..offsets.offset(index.checked_add(1)?)? as usize)?;
    let header = glyf::Glyph::new(bytes).ok()?;
    if header.number_of_contours() <= 0 {
        return None;
    }
    let units = i32::from(font.head().units_per_em());
    let bearing = font.default_instance().ft_left_side_bearing(GlyphId(glyph));
    let shift = |v: i16, ppem: u16| {
        i16::try_from(mul_fix(i32::from(v), div_fix(i32::from(ppem) << 6, units)) >> 6).ok()
    };
    Some((
        shift(bearing, strike.x_ppem)?,
        shift(header.y_min(), strike.y_ppem)?,
    ))
}

fn png_bitmap(
    font: &Font<'_>,
    strike: &Strike,
    glyph: u32,
    image: &GlyphBitmap<'_>,
    out: Option<&mut [u8]>,
) -> Result<BitmapLayout, BitmapError> {
    let (width, rows) = png_size(image.data);
    if width == 0
        || rows == 0
        || width > 0x7fff
        || rows > 0x7fff
        || width != u32::from(image.metrics.width)
        || rows != u32::from(image.metrics.height)
    {
        return Err(BitmapError::Invalid);
    }
    let mut metrics = image.metrics;
    if matches!(image.format, BitmapFormat::Sbix(_))
        && let Some((x, y)) = sbix_origin_shift(font, strike, glyph)
    {
        metrics.hori_bearing_x = metrics.hori_bearing_x.saturating_add(x);
        metrics.hori_bearing_y = metrics.hori_bearing_y.saturating_add(y);
    }
    let layout = BitmapLayout {
        width: image.metrics.width,
        rows: image.metrics.height,
        pitch: width.saturating_mul(4),
        pixel_mode: PixelMode::Bgra,
        metrics,
    };
    if let Some(out) = out {
        let bytes = out
            .get_mut(..layout.len())
            .ok_or(BitmapError::BufferTooSmall(layout.len()))?;
        let decoded =
            sofel_png::decode(image.data, layout.len() / 4).map_err(|_| BitmapError::Invalid)?;
        bytes.copy_from_slice(&decoded.bgra);
        if image.flipped {
            for row in bytes.chunks_exact_mut(layout.pitch as usize) {
                row.as_chunks_mut::<4>().0.reverse();
            }
        }
    }
    Ok(layout)
}

/// The width and height in a PNG header (IHDR at its fixed position);
/// 0 when the data is too short.
fn png_size(data: &[u8]) -> (u32, u32) {
    let at = |k: usize| {
        data.get(k..k.saturating_add(4))
            .and_then(|b| b.first_chunk::<4>())
            .map_or(0, |b| u32::from_be_bytes(*b))
    };
    if data.len() < 29 {
        (0, 0)
    } else {
        (at(16), at(20))
    }
}

/// FreeType's `tt_face_get_metrics` advance from `hmtx` (or `vmtx`).
fn ft_advance(font: &Font<'_>, glyph: u32, vertical: bool) -> u16 {
    let (hea_tag, mtx_tag) = if vertical {
        (vhea::TAG, vmtx::TAG)
    } else {
        (hhea::TAG, hmtx::TAG)
    };
    let Some(k) = font
        .table(hea_tag)
        .and_then(|d| Hhea::new(d, CountWidth::U16).ok())
        .map(|h| h.number_of_h_metrics())
    else {
        return 0;
    };
    let Some(table) = font.table(mtx_tag) else {
        return 0;
    };
    if k == 0 {
        return 0;
    }
    let idx = if glyph < k {
        glyph
    } else {
        k.saturating_sub(1)
    };
    let pos = usize::try_from(idx).unwrap_or(usize::MAX).saturating_mul(4);
    table
        .get(pos..pos.saturating_add(4))
        .and_then(|b| b.first_chunk::<2>())
        .map_or(0, |b| u16::from_be_bytes(*b))
}

/// The vertical advance FreeType gives `sbix` glyphs: from `vmtx` when
/// the font has vertical metrics, else `|sTypoAscender -
/// sTypoDescender|` of `OS/2`, else `|ascender - descender|` of `hhea`.
fn ft_v_advance(font: &Font<'_>, glyph: u32) -> u16 {
    if font.table(vhea::TAG).is_some() && font.table(vmtx::TAG).is_some() {
        return ft_advance(font, glyph, true);
    }
    let diff = match font.table(os2::TAG).and_then(|d| os2::Os2::new(d).ok()) {
        Some(o) => i32::from(o.s_typo_ascender()).saturating_sub(i32::from(o.s_typo_descender())),
        None => {
            let (a, d, _) = ft_hhea(font);
            a.saturating_sub(d)
        }
    };
    u16::try_from(diff.unsigned_abs() & 0xFFFF).unwrap_or(0)
}

// ------------------------------------------------------------- HarfBuzz

/// HarfBuzz's CBDT accelerator: `CBLC` (version 2 or 3) and `CBDT`
/// (version 2 or 3).
fn hb_cbdt<'a>(font: &Font<'a>) -> Option<(Cblc<'a>, Cbdt<'a>)> {
    let loc = Cblc::new(font.table(cblc::TAG_CBLC)?)
        .ok()
        .filter(|c| matches!(c.major_version(), 2 | 3))?;
    let data = Cbdt::new(font.table(cbdt::TAG_CBDT)?)
        .ok()
        .filter(|c| matches!(c.major_version(), 2 | 3))?;
    Some((loc, data))
}

/// Whether HarfBuzz reads `CBDT` (`CBDT::accelerator_t::has_data`):
/// `CBLC` and `CBDT` of version 2 or 3.
pub fn has_hb_cbdt(font: &Font<'_>) -> bool {
    hb_cbdt(font).is_some()
}

/// Whether HarfBuzz reads `sbix` (`sbix::has_data`): version 1 or more.
pub fn has_hb_sbix(font: &Font<'_>) -> bool {
    font.table(sbix::TAG)
        .and_then(|d| Sbix::new(d).ok())
        .is_some_and(|s| s.version() >= 1)
}

/// HarfBuzz's `choose_strike` over a list of `(ppem, index)` in table
/// order; `ppem` 0 picks the largest.
fn hb_choose(ppems: impl Iterator<Item = (u16, usize)> + Clone, ppem: u16) -> Option<usize> {
    let requested = if ppem == 0 {
        1u32 << 30
    } else {
        u32::from(ppem)
    };
    let first_of = |p: u16| ppems.clone().find(|&(q, _)| q == p).map(|(_, i)| i);
    let above = ppems
        .clone()
        .map(|(p, _)| p)
        .filter(|&p| u32::from(p) >= requested)
        .min();
    let p = above.or_else(|| ppems.clone().map(|(p, _)| p).max())?;
    first_of(p)
}

/// The strike HarfBuzz's CBDT accelerator chooses for `ppem`.
fn hb_cbdt_strike(view: &Cblc<'_>, ppem: u16) -> Option<BitmapSize> {
    let ppems = (0..view.num_sizes())
        .filter_map(|i| view.size(i).map(|s| (u16::from(s.ppem_x.max(s.ppem_y)), i)));
    let i = hb_choose(ppems, ppem)?;
    view.size(i)
}

/// HarfBuzz's CBDT image of `glyph`: its format, its bytes and the
/// strike; only index formats 1 and 3.
fn hb_cbdt_image<'a>(
    font: &Font<'a>,
    glyph: u32,
    ppem: u16,
) -> Option<(u16, &'a [u8], BitmapSize)> {
    let (view, data) = hb_cbdt(font)?;
    let s = hb_cbdt_strike(&view, ppem)?;
    if s.ppem_x == 0 || s.ppem_y == 0 || glyph > 0xFFFF {
        return None;
    }
    let r = find_record(&view, &s, glyph)?;
    let (index_format, _, _) = view.index_header(&r)?;
    if !matches!(index_format, 1 | 3) {
        return None;
    }
    let loc = view.locate(&r, glyph)?;
    let image = data.image(loc.offset, loc.length)?;
    Some((loc.image_format, image, s))
}

/// HarfBuzz's `CBDT::accelerator_t::get_extents` with `scale` false:
/// the box in strike pixels from formats 17 and 18.
fn hb_cbdt_pixel_extents(format: u16, image: &[u8]) -> Option<GlyphExtents> {
    let m = match format {
        17 if image.len() >= 9 => GlyphMetrics::small(image)?,
        18 if image.len() >= 12 => GlyphMetrics::big(image)?,
        _ => return None,
    };
    Some(GlyphExtents {
        x_bearing: i32::from(m.hori_bearing_x),
        y_bearing: i32::from(m.hori_bearing_y),
        width: i32::from(m.width),
        height: i32::from(m.height).wrapping_neg(),
    })
}

/// HarfBuzz's `roundf` clamped to `i32` (`hb_clamp_to<hb_position_t>`;
/// ADR 0026).
fn hb_round(v: f32) -> i32 {
    hb_clamp_to_i32(hb_roundf32(v))
}

/// HarfBuzz's CBDT glyph box at `scale` (`get_extents` with `scale`
/// true): the strike's metrics scaled as if they were font units, then
/// multiplied by `upem / ppem` per axis and rounded. Only image formats
/// 17 and 18 have one. The strike is chosen for `ppem` (0: the largest).
pub fn cbdt_extents(
    font: &Font<'_>,
    glyph: u32,
    ppem: u16,
    scale: &HbScale,
) -> Option<GlyphExtents> {
    let (format, image, s) = hb_cbdt_image(font, glyph, ppem)?;
    let e = scale.scale_glyph_extents(hb_cbdt_pixel_extents(format, image)?);
    let upem = i32_to_f32(i32::from(font.hb_units_per_em()));
    let (xs, ys) = (upem / f32::from(s.ppem_x), upem / f32::from(s.ppem_y));
    Some(GlyphExtents {
        x_bearing: hb_round(i32_to_f32(e.x_bearing) * xs),
        y_bearing: hb_round(i32_to_f32(e.y_bearing) * ys),
        width: hb_round(i32_to_f32(e.width) * xs),
        height: hb_round(i32_to_f32(e.height) * ys),
    })
}

/// The PNG data of `glyph` in `CBDT` as HarfBuzz's `reference_png`
/// gives it: image formats 17, 18 and 19, cut at the end of the table.
pub fn cbdt_png_data<'a>(font: &Font<'a>, glyph: u32, ppem: u16) -> Option<&'a [u8]> {
    let (format, image, _) = hb_cbdt_image(font, glyph, ppem)?;
    let header: usize = match format {
        17 => 5,
        18 => 8,
        19 => 0,
        _ => return None,
    };
    let len = u32::from_be_bytes(*image.get(header..)?.first_chunk::<4>()?);
    let rest = image.get(header.saturating_add(4)..)?;
    let data = rest.get(..usize::try_from(len).ok()?.min(rest.len()))?;
    (!data.is_empty()).then_some(data)
}

/// The PNG of `glyph` in `CBDT` as HarfBuzz paints it
/// ([`cbdt_png_data`] with the box of `get_extents` with `scale` false):
/// image formats 17 and 18 (format 19 has no box and is not painted).
pub fn cbdt_png<'a>(font: &Font<'a>, glyph: u32, ppem: u16) -> Option<PngGlyph<'a>> {
    let (format, image, _) = hb_cbdt_image(font, glyph, ppem)?;
    let pixel_extents = hb_cbdt_pixel_extents(format, image)?;
    Some(PngGlyph {
        data: cbdt_png_data(font, glyph, ppem)?,
        pixel_extents,
    })
}

/// The PNG data of `glyph` in `sbix` as HarfBuzz's `reference_png` gives
/// it.
pub fn sbix_png_data<'a>(font: &Font<'a>, glyph: u32, ppem: u16) -> Option<&'a [u8]> {
    hb_sbix_png(font, glyph, ppem).map(|(d, _, _, _)| d)
}

/// HarfBuzz's sbix strike for `ppem` and its PNG for `glyph`: the bytes,
/// the origin offsets and the strike's ppem (`reference_png`, which
/// follows `dupe` up to 8 times and takes only `png `).
fn hb_sbix_png<'a>(font: &Font<'a>, glyph: u32, ppem: u16) -> Option<(&'a [u8], i16, i16, u16)> {
    let view = Sbix::new(font.table(sbix::TAG)?)
        .ok()
        .filter(|s| s.version() >= 1)?;
    let ppems = (0..view.num_strikes()).filter_map(|i| view.strike(i).map(|s| (s.ppem(), i)));
    let st = view.strike(hb_choose(ppems, ppem)?)?;
    if st.ppem() == 0 {
        return None;
    }
    let mut g = glyph;
    for _ in 0..=8 {
        if g >= font.num_glyphs() {
            return None;
        }
        let found = st.glyph(g)?;
        if &found.graphic_type.0 == b"dupe" {
            g = u32::from(u16::from_be_bytes(*found.data.first_chunk::<2>()?));
            continue;
        }
        if &found.graphic_type.0 != b"png " || found.data.is_empty() {
            return None;
        }
        return Some((found.data, found.origin_x, found.origin_y, st.ppem()));
    }
    None
}

/// HarfBuzz's sbix PNG box in pixels (`get_png_extents` with `scale`
/// false); `None` for a PNG of 65536 or more pixels a side.
fn hb_sbix_pixel_extents(data: &[u8], x: i16, y: i16) -> Option<GlyphExtents> {
    let (w, h) = png_size(data);
    if w >= 65536 || h >= 65536 {
        return None;
    }
    let (w, h) = (i32::try_from(w).ok()?, i32::try_from(h).ok()?);
    Some(GlyphExtents {
        x_bearing: i32::from(x),
        y_bearing: h.saturating_add(i32::from(y)),
        width: w,
        height: h.wrapping_neg(),
    })
}

/// HarfBuzz's sbix glyph box at `scale` (`get_png_extents` with `scale`
/// true): the pixels times `upem / ppem`, rounded, then scaled.
pub fn sbix_extents(
    font: &Font<'_>,
    glyph: u32,
    ppem: u16,
    scale: &HbScale,
) -> Option<GlyphExtents> {
    let (data, x, y, strike_ppem) = hb_sbix_png(font, glyph, ppem)?;
    let e = hb_sbix_pixel_extents(data, x, y)?;
    let f = i32_to_f32(i32::from(font.hb_units_per_em())) / f32::from(strike_ppem);
    let e = GlyphExtents {
        x_bearing: hb_round(i32_to_f32(e.x_bearing) * f),
        y_bearing: hb_round(i32_to_f32(e.y_bearing) * f),
        width: hb_round(i32_to_f32(e.width) * f),
        height: hb_round(i32_to_f32(e.height) * f),
    };
    Some(scale.scale_glyph_extents(e))
}

/// The PNG of `glyph` in `sbix` as HarfBuzz paints it, with its box in
/// the strike's pixels.
pub fn sbix_png<'a>(font: &Font<'a>, glyph: u32, ppem: u16) -> Option<PngGlyph<'a>> {
    let (data, x, y, _) = hb_sbix_png(font, glyph, ppem)?;
    Some(PngGlyph {
        data,
        pixel_extents: hb_sbix_pixel_extents(data, x, y)?,
    })
}
