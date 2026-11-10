//! One font, read once (ADR 0010).
//!
//! [`Font`] opens an sfnt font or a font in a TTC/OTC collection and keeps
//! what shaping and drawing share: the glyph count, the cmap subtable used
//! for characters, and the horizontal metrics. The policies for broken or
//! missing data follow HarfBuzz 15.1.0, the reference for shaping
//! (ADR 0005). `sofel-read` is re-exported as [`read`]; other crates use
//! table views through it instead of reading bytes (ADR 0003).
//!
//! [`Font::ft_left_side_bearing`] reads the left side bearing the way
//! FreeType does for outlines instead (ADR 0018). It is ported from
//! FreeType 2.15.0 (commit 37515cf56ba3349656049edbed89ffc6f6466f2d):
//! `src/sfnt/ttmtx.c` (`tt_face_load_hhea`, `tt_face_load_hmtx`,
//! `tt_face_get_metrics`) and `src/sfnt/sfobjs.c` (`sfnt_load_face`).
//! Copyright (C) 1996-2026 by David Turner, Robert Wilhelm, and Werner
//! Lemberg. Used under the FreeType License (FTL); see
//! THIRD_PARTY_NOTICES.md.
//!
//! # Example
//!
//! Open a font, map a character to its glyph and get the advance:
//!
//! ```no_run
//! use sofel_font::Font;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let data = std::fs::read("font.ttf")?;
//!     let font = Font::new(&data, 0)?;
//!     let glyph = font.nominal_glyph(u32::from('A')).ok_or("no glyph for A")?;
//!     println!(
//!         "glyph {} advances {} of {} units per em",
//!         glyph.0,
//!         font.advance(glyph),
//!         font.units_per_em(),
//!     );
//!     Ok(())
//! }
//! ```

#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub use sofel_read as read;

mod float;
mod ft;
mod glyf_data;
mod metrics;
mod name_language;
mod names;
mod tables;

pub use ft::FtFaceMetrics;
pub use metrics::{FontExtents, GlyphExtents, HbScale, MetricsTag};
pub use names::{GlyphNames, Language, NameEntry, NameLength, Names, SfntName};

use core::fmt;

use sofel_read::cmap::{Cmap, Subtable, VariantGlyph};
use sofel_read::head::Head;
use sofel_read::hhea::Hhea;
use sofel_read::hmtx::Hmtx;
use sofel_read::maxp::Maxp;
use sofel_read::os2::Os2;
use sofel_read::post::Post;
use sofel_read::sfnt::{FontFile, Sfnt};
use sofel_read::vorg::Vorg;
use sofel_read::{
    CountWidth, GlyphId, ReadError, Tag, cmap, glyf, head, hhea, hmtx, maxp, os2, post, vhea, vmtx,
    vorg,
};

/// Why a font could not be opened.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontError {
    /// The file is not an sfnt or collection that can be read.
    Read(ReadError),
    /// The font has no readable `head` table.
    NoHead,
    /// No font with this index in the file.
    IndexOutOfRange,
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::Read(e) => write!(f, "{e}"),
            FontError::NoHead => f.write_str("no readable head table"),
            FontError::IndexOutOfRange => f.write_str("no font with this index"),
        }
    }
}

impl core::error::Error for FontError {}

/// Horizontal or vertical metrics, with the counts HarfBuzz derives from
/// the data (`hb-ot-hmtx-table.hh`, `accelerator_t`).
#[derive(Clone, Copy, Debug)]
struct Metrics<'a> {
    hmtx: Option<Hmtx<'a>>,
    /// Glyphs below this have an advance in the table.
    num_bearings: u32,
    /// 0 when there is no metrics data at all.
    num_advances: u32,
}

impl<'a> Metrics<'a> {
    fn new(data: Option<&'a [u8]>, long_metrics: u32, num_glyphs: u32) -> Metrics<'a> {
        let mut len = data.map_or(0, |d| u32::try_from(d.len()).unwrap_or(u32::MAX)) & !1;
        let nlm = long_metrics.min(len / 4);
        len = len.saturating_sub(nlm.saturating_mul(4));
        let mut nb = num_glyphs.max(nlm);
        if nb.saturating_sub(nlm).saturating_mul(2) > len {
            nb = nlm.saturating_add(len / 2);
        }
        len = len.saturating_sub(nb.saturating_sub(nlm).saturating_mul(2));
        if nlm == 0 {
            nb = 0;
        }
        let num_advances = nb.saturating_add(len / 2);
        let hmtx = data
            .and_then(|d| Hmtx::new(d, nlm, nb).ok())
            .filter(|_| nlm > 0);
        Metrics {
            hmtx,
            num_bearings: nb,
            num_advances,
        }
    }

    /// HarfBuzz's pair of tables (`hb_dual_accelerator_t`): the wide
    /// tables when they give any metrics, otherwise the narrow ones. The
    /// header must pass HarfBuzz's checks (version 1, all fields
    /// present); otherwise it counts as giving no long metrics.
    fn dual(sfnt: &Sfnt<'a>, tags: [(Tag, Tag, CountWidth); 2], num_glyphs: u32) -> Metrics<'a> {
        let one = |(hea_tag, mtx_tag, width): (Tag, Tag, CountWidth)| {
            let long = hea(sfnt, hea_tag, width).map_or(0, |h| h.number_of_h_metrics());
            Metrics::new(sfnt.table(mtx_tag), long, num_glyphs)
        };
        let [narrow, wide] = tags;
        let upper = one(wide);
        if upper.num_bearings != 0 {
            upper
        } else {
            one(narrow)
        }
    }

    /// HarfBuzz's `get_advance_without_var_unscaled`; `default` when the
    /// table gives no advances at all.
    fn advance(&self, glyph: GlyphId, default: u16) -> u16 {
        if glyph.0 < self.num_bearings {
            return self.hmtx.and_then(|h| h.advance(glyph)).unwrap_or(0);
        }
        if self.num_advances == 0 {
            return default;
        }
        0
    }

    /// HarfBuzz's `get_leading_bearing_without_var_unscaled`: 0 past
    /// the bearings.
    fn bearing(&self, glyph: GlyphId) -> i16 {
        if glyph.0 >= self.num_bearings {
            return 0;
        }
        self.hmtx
            .and_then(|h| h.left_side_bearing(glyph))
            .unwrap_or(0)
    }

    /// HarfBuzz's `has_data`.
    fn has_data(&self) -> bool {
        self.num_bearings != 0
    }
}

/// An `hhea`-shaped header that can be read: version 1 and all fields
/// present (HarfBuzz's `_hea::sanitize`).
fn hea<'a>(sfnt: &Sfnt<'a>, tag: Tag, width: CountWidth) -> Option<Hhea<'a>> {
    sfnt.table(tag).and_then(|d| Hhea::new(d, width).ok())
}

/// HarfBuzz's pair of headers: the wide one when it is there.
fn dual_hea<'a>(sfnt: &Sfnt<'a>, narrow: Tag, wide: Tag) -> Option<Hhea<'a>> {
    hea(sfnt, wide, CountWidth::U24).or_else(|| hea(sfnt, narrow, CountWidth::U16))
}

/// The horizontal metrics as FreeType reads them (`tt_face_load_hhea`,
/// `tt_face_load_hmtx`): the `hmtx` or `HMTX` bytes and
/// `numberOfHMetrics`, unchecked.
#[derive(Clone, Copy, Debug, Default)]
struct FtMetrics<'a> {
    hmtx: &'a [u8],
    long_metrics: u32,
}

impl<'a> FtMetrics<'a> {
    /// With `GLYF`, `HHEA` and `HMTX`; otherwise `hhea` and `hmtx`, or
    /// `HHEA` and `HMTX` when there is no `hhea`. Empty when the header
    /// cannot be read or the metrics table is missing (FreeType then
    /// fails to open the face).
    fn new(sfnt: &Sfnt<'a>) -> FtMetrics<'a> {
        let wide = sfnt.table(glyf::TAG_WIDE).is_some() || sfnt.table(hhea::TAG).is_none();
        let (hhea_tag, hmtx_tag, width) = if wide {
            (hhea::TAG_WIDE, hmtx::TAG_WIDE, CountWidth::U24)
        } else {
            (hhea::TAG, hmtx::TAG, CountWidth::U16)
        };
        let long_metrics = sfnt
            .table(hhea_tag)
            .and_then(|d| Hhea::new(d, width).ok())
            .map(|h| h.number_of_h_metrics());
        match (long_metrics, sfnt.table(hmtx_tag)) {
            (Some(long_metrics), Some(hmtx)) => FtMetrics { hmtx, long_metrics },
            _ => FtMetrics::default(),
        }
    }

    /// `tt_face_get_metrics` for the bearing, without variations.
    fn left_side_bearing(&self, glyph: GlyphId) -> i16 {
        let read = |pos: u64| {
            let at = usize::try_from(pos).ok()?;
            let b = self.hmtx.get(at..at.checked_add(2)?)?;
            Some(i16::from_be_bytes([*b.first()?, *b.get(1)?]))
        };
        let size = u64::try_from(self.hmtx.len()).unwrap_or(u64::MAX);
        let (g, k) = (u64::from(glyph.0), u64::from(self.long_metrics));
        let bearing = if k == 0 {
            None
        } else if g < k {
            // The whole long metric must be in the table.
            let pos = g.saturating_mul(4);
            (pos.saturating_add(4) <= size)
                .then(|| read(pos.saturating_add(2)))
                .flatten()
        } else if k.saturating_sub(1).saturating_mul(4).saturating_add(2) > size {
            // The last advance is not readable.
            None
        } else {
            read(
                k.saturating_mul(4)
                    .saturating_add(g.saturating_sub(k).saturating_mul(2)),
            )
        };
        bearing.unwrap_or(0)
    }
}

/// A font. Built once by [`Font::new`]; immutable and cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Font<'a> {
    sfnt: Sfnt<'a>,
    head: Head<'a>,
    num_glyphs: u32,
    metrics: Metrics<'a>,
    vmetrics: Metrics<'a>,
    ft_metrics: FtMetrics<'a>,
    cmap: Option<Subtable<'a>>,
    uvs: Option<Subtable<'a>>,
    os2: Option<Os2<'a>>,
    hhea: Option<Hhea<'a>>,
    vhea: Option<Hhea<'a>>,
    post: Option<Post<'a>>,
    vorg: Option<Vorg<'a>>,
    glyf: Option<glyf_data::Glyf<'a>>,
}

/// HarfBuzz's order of preference (`find_best_subtable`). `ANY` matches
/// every encoding.
const ANY: u16 = 0xFFFF;
const CMAP_ORDER: [(u16, u16); 11] = [
    (3, 0),
    (3, 10),
    (0, 6),
    (0, 4),
    (3, 1),
    (0, 3),
    (0, 2),
    (0, 1),
    (0, 0),
    (1, 0),
    (1, ANY),
];

fn best_subtable<'a>(c: &Cmap<'a>) -> Option<Subtable<'a>> {
    CMAP_ORDER.iter().find_map(|&(p, e)| {
        c.records()
            .filter(|r| r.platform_id == p && (e == ANY || r.encoding_id == e))
            .find_map(|r| r.subtable().ok().filter(|s| !matches!(s.format(), 14 | 15)))
    })
}

/// The (0, 5) variation sequence subtable: format 15 first, otherwise the
/// last format 14 (`find_variation_selector_subtable`).
fn uvs_subtable<'a>(c: &Cmap<'a>) -> Option<Subtable<'a>> {
    let mut format14 = None;
    for s in c
        .records()
        .filter(|r| r.platform_id == 0 && r.encoding_id == 5)
        .filter_map(|r| r.subtable().ok())
    {
        match s.format() {
            15 => return Some(s),
            14 => format14 = Some(s),
            _ => {}
        }
    }
    format14
}

impl<'a> Font<'a> {
    /// Opens font `index` of `data`: 0 for a single sfnt, any index of a
    /// collection. Reads the glyph count, picks the cmap subtables and
    /// prepares the horizontal metrics once; a missing or broken `maxp`,
    /// `hhea`, `hmtx` or `cmap` is not an error.
    ///
    /// # Errors
    ///
    /// [`FontError::Read`] if `data` is not a readable sfnt or collection
    /// (or the chosen font in it cannot be read),
    /// [`FontError::IndexOutOfRange`] if there is no font `index`, and
    /// [`FontError::NoHead`] if the font has no readable `head`.
    pub fn new(data: &'a [u8], index: u32) -> Result<Font<'a>, FontError> {
        let sfnt = match FontFile::new(data).map_err(FontError::Read)? {
            FontFile::Sfnt(s) if index == 0 => s,
            FontFile::Sfnt(_) => return Err(FontError::IndexOutOfRange),
            FontFile::Collection(c) if index < c.num_fonts() => {
                c.font(index).map_err(FontError::Read)?
            }
            FontFile::Collection(_) => return Err(FontError::IndexOutOfRange),
        };
        let head = sfnt
            .table(head::TAG)
            .and_then(|d| Head::new(d).ok())
            .ok_or(FontError::NoHead)?;

        let glyphs = |tag, width| {
            sfnt.table(tag)
                .and_then(|d| Maxp::new(d, width).ok())
                .map(|m| m.num_glyphs())
        };
        let num_glyphs = glyphs(maxp::TAG_WIDE, CountWidth::U24)
            .or_else(|| glyphs(maxp::TAG, CountWidth::U16))
            .unwrap_or(0);

        let metrics = Metrics::dual(
            &sfnt,
            [
                (hhea::TAG, hmtx::TAG, CountWidth::U16),
                (hhea::TAG_WIDE, hmtx::TAG_WIDE, CountWidth::U24),
            ],
            num_glyphs,
        );
        let vmetrics = Metrics::dual(
            &sfnt,
            [
                (vhea::TAG, vmtx::TAG, CountWidth::U16),
                (vhea::TAG_WIDE, vmtx::TAG_WIDE, CountWidth::U24),
            ],
            num_glyphs,
        );

        let cmap_table = sfnt.table(cmap::TAG).and_then(|d| Cmap::new(d).ok());
        let cmap = cmap_table.as_ref().and_then(best_subtable);
        let uvs = cmap_table.as_ref().and_then(uvs_subtable);

        let post = sfnt
            .table(post::TAG)
            .and_then(|d| Post::new(d).ok())
            .filter(|p| {
                matches!(
                    p.version(),
                    post::VERSION_1 | post::VERSION_2 | post::VERSION_3
                )
            });

        Ok(Font {
            sfnt,
            head,
            num_glyphs,
            metrics,
            vmetrics,
            ft_metrics: FtMetrics::new(&sfnt),
            cmap,
            uvs,
            os2: sfnt.table(os2::TAG).and_then(|d| Os2::new(d).ok()),
            hhea: dual_hea(&sfnt, hhea::TAG, hhea::TAG_WIDE),
            vhea: dual_hea(&sfnt, vhea::TAG, vhea::TAG_WIDE),
            post,
            vorg: sfnt.table(vorg::TAG).and_then(|d| Vorg::new(d).ok()),
            glyf: glyf_data::Glyf::new(&sfnt, &head, num_glyphs),
        })
    }

    /// The sfnt the font was opened from: for a collection, the one font
    /// chosen by the index.
    pub fn sfnt(&self) -> Sfnt<'a> {
        self.sfnt
    }

    /// The `head` table, which [`Font::new`] requires.
    pub fn head(&self) -> Head<'a> {
        self.head
    }

    /// `numGlyphs` of `MAXP` if present, else of `maxp`, else 0.
    pub fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }

    /// `unitsPerEm` as stored, even outside 16..=16384.
    pub fn units_per_em(&self) -> u16 {
        self.head.units_per_em()
    }

    /// The bytes of a table.
    pub fn table(&self, tag: Tag) -> Option<&'a [u8]> {
        self.sfnt.table(tag)
    }

    /// The glyph for a character, from the chosen cmap subtable (in
    /// HarfBuzz's order of preference, `find_best_subtable`). Glyph ids
    /// not below the glyph count count as unmapped. `None` also when the
    /// font has no usable subtable.
    pub fn nominal_glyph(&self, c: u32) -> Option<GlyphId> {
        self.cmap?.glyph(c).filter(|g| g.0 < self.num_glyphs)
    }

    /// The glyph for the variation sequence `c` + `selector`, from the
    /// (0, 5) subtable: format 15 if present, else the last format 14. A
    /// default variation falls back to [`Font::nominal_glyph`]; glyph ids
    /// not below the glyph count count as unmapped.
    pub fn variation_glyph(&self, c: u32, selector: u32) -> Option<GlyphId> {
        match self.uvs?.variant(c, selector)? {
            VariantGlyph::Default => self.nominal_glyph(c),
            VariantGlyph::Glyph(g) => (g.0 < self.num_glyphs).then_some(g),
        }
    }

    /// The horizontal advance in font units, as HarfBuzz gives it: from
    /// the table for glyphs it covers, 0 beyond, and `unitsPerEm / 2`
    /// (with an out-of-range `unitsPerEm` taken as 1000) when the font
    /// has no metrics data at all.
    pub fn advance(&self, glyph: GlyphId) -> u16 {
        self.metrics.advance(glyph, self.hb_units_per_em() / 2)
    }

    /// `unitsPerEm` as HarfBuzz uses it (`hb_face_get_upem`): 1000 when
    /// the stored value is outside 16..=16384.
    pub fn hb_units_per_em(&self) -> u16 {
        let upem = self.units_per_em();
        if (16..=16384).contains(&upem) {
            upem
        } else {
            1000
        }
    }

    /// The `OS/2` table, if it can be read (ADR 0016).
    pub fn os2(&self) -> Option<Os2<'a>> {
        self.os2
    }

    /// The embedding permissions, as FreeType's `FT_Get_FSType_Flags`
    /// gives them for sfnt fonts: `fsType` of `OS/2`, or 0 when the
    /// table cannot be read or its version is 0xFFFF (FreeType's mark of
    /// a missing table).
    pub fn fs_type(&self) -> u16 {
        self.os2
            .filter(|o| o.version() != 0xFFFF)
            .map_or(0, |o| o.fs_type())
    }

    /// The font's horizontal ascender, descender (negative below the
    /// baseline) and line gap in font units, as HarfBuzz's
    /// `hb_font_get_extents_for_direction` gives them for horizontal
    /// text at the default scale: from `OS/2` with `USE_TYPO_METRICS`,
    /// else from `hhea` (`HHEA` when present), with the signs fixed; when
    /// a value is missing, the ascender is 0.8 em, the descender the
    /// ascender minus 1 em and the line gap 0.
    pub fn h_extents(&self) -> FontExtents {
        HbScale::unscaled(self).h_extents(self)
    }

    /// The vertical extents in font units, as HarfBuzz's
    /// `hb_font_get_extents_for_direction` gives them for vertical text
    /// at the default scale: from `vhea` (`VHEA` when present), with the
    /// signs fixed; when a value is missing, the ascender is half an em,
    /// the descender the ascender minus 1 em and the line gap 0.
    pub fn v_extents(&self) -> FontExtents {
        HbScale::unscaled(self).v_extents(self)
    }

    /// The vertical advance in font units, negative downward as in
    /// HarfBuzz (`hb_font_get_glyph_v_advance`): minus the `vmtx` advance,
    /// or, without `vmtx` data, the descender minus the ascender of
    /// [`Font::h_extents`].
    pub fn v_advance(&self, glyph: GlyphId) -> i32 {
        HbScale::unscaled(self).v_advance(self, glyph)
    }

    /// The vertical origin `(x, y)` in font units, as HarfBuzz's
    /// `hb_font_get_glyph_v_origin` gives it: x is half the horizontal
    /// advance; y comes from `VORG` if present, else (with `vmtx` and
    /// glyf data) from glyf's top phantom point (the glyph's `yMax` plus
    /// its top side bearing, from the last USE_MY_METRICS component for
    /// composites; 1 em when the glyph cannot be read), else from the box
    /// that `extents` gives (centered between the ascender and descender
    /// of [`Font::h_extents`]; the ascender when `extents` gives `None`).
    ///
    /// `extents` is only called in that last case. Pass
    /// [`Font::glyf_extents`] for glyf fonts; other outline formats get
    /// their boxes from the layers above (ADR 0016).
    pub fn v_origin(
        &self,
        glyph: GlyphId,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> (i32, i32) {
        HbScale::unscaled(self).v_origin(self, glyph, extents)
    }

    /// The box of a glyf glyph in font units, as HarfBuzz's
    /// `hb_font_get_glyph_extents` gives it for glyf fonts without
    /// variations, from the glyph header and `hmtx` only: `x_bearing` is
    /// the left side bearing (FreeType shifts outlines so, ADR 0018),
    /// `y_bearing` the top, `width` positive and `height` negative; all 0
    /// for empty glyphs. `None` without glyf data or for glyph ids past
    /// it.
    pub fn glyf_extents(&self, glyph: GlyphId) -> Option<GlyphExtents> {
        HbScale::unscaled(self).glyf_extents(self, glyph)
    }

    /// A font-wide metric in font units, as HarfBuzz's
    /// `hb_ot_metrics_get_position` gives it at the default scale.
    /// `None` when the font does not have it.
    pub fn metrics_position(&self, tag: MetricsTag) -> Option<i32> {
        HbScale::unscaled(self).metrics_position(self, tag)
    }

    /// A font-wide metric in font units, as HarfBuzz's
    /// `hb_ot_metrics_get_position_with_fallback` gives it at the default
    /// scale: [`Font::metrics_position`], or a value made up from other
    /// metrics when the font does not have it (or has a zero underline
    /// or strikeout size). The x-height and cap height fallbacks use the
    /// boxes of the glyphs for `x` and `O` from `extents`.
    pub fn metrics_position_with_fallback(
        &self,
        tag: MetricsTag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        HbScale::unscaled(self).metrics_position_with_fallback(self, tag, extents)
    }

    /// The left side bearing in font units, as FreeType reads it for
    /// outlines (`tt_face_get_metrics`), which [`Font::advance`] does not
    /// follow: from the long metric below `numberOfHMetrics`, else from
    /// the `leftSideBearings` array, whatever the glyph count; 0 when the
    /// value is past the table, when the last advance is, and when there
    /// is no `hhea` and `hmtx`. With `GLYF`, it reads `HHEA` and `HMTX`;
    /// otherwise `hhea` and `hmtx`, or `HHEA` and `HMTX` without `hhea`.
    /// `sofel-outline` uses it for FreeType's phantom point 1 (ADR 0018).
    pub fn ft_left_side_bearing(&self, glyph: GlyphId) -> i16 {
        self.ft_metrics.left_side_bearing(glyph)
    }
}
