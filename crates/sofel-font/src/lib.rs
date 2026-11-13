//! One font, read once (ADR 0010).
//!
//! [`Font`] opens an sfnt font or a font in a TTC/OTC collection and keeps
//! what shaping and drawing share: the glyph count, the cmap subtable used
//! for characters, and the horizontal metrics. The policies for broken or
//! missing data follow HarfBuzz 15.1.0, the reference for shaping
//! (ADR 0005). `sofel-read` is re-exported as [`read`]; other crates use
//! table views through it instead of reading bytes (ADR 0003). Likewise
//! `sofel-geom` is re-exported as [`geom`] (ADR 0023).
//!
//! [`crate::var::Instance::ft_left_side_bearing`] reads the left side bearing the way
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
//!         font.default_instance().advance(glyph),
//!         font.units_per_em(),
//!     );
//!     Ok(())
//! }
//! ```

#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

extern crate alloc;

pub use sofel_geom as geom;
pub use sofel_read as read;
pub use sofel_type1::{face::Type1Face, source::Type1Source};

mod bare_cff;
mod base_sanitize;
pub mod baseline;
mod bitmap_face;
pub use bitmap_face::{BitmapFace, BitmapFormat};
mod cff;
mod ft;
mod ft_glyf;
mod glyf_data;
mod hb_glyf;
pub use ft_glyf::FtAdvanceScratch;
pub use hb_glyf::GlyphScratch;
mod hb_cff;
mod layout;
pub mod layout_query;
mod metrics;
mod pfr;
pub use pfr::{PfrBitmap, PfrFace};
mod name_language;
mod names;
pub mod remap;
mod tables;
pub mod var;

pub use bare_cff::BareCffFace;
pub use cff::{Cff, CffError, CffMatrix, CidError, PsFontInfo, Ros, Subfont, fs_type};
pub use ft::FtFaceMetrics;
pub use hb_cff::{HbCff1, HbCff2};
pub use layout::Direction;
pub use metrics::{FontExtents, GlyphExtents, HbScale, MetricsTag};
pub use names::{GlyphNames, Language, NameEntry, NameLength, Names, SfntName};
pub use sofel_type1::cid::CidFace;

use core::fmt;

use sofel_read::base::Base;
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
    CountWidth, GlyphId, ReadError, Tag, base, cmap, glyf, head, hhea, hmtx, maxp, os2, post, vhea,
    vmtx, vorg,
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
    #[inline]
    fn advance(&self, glyph: GlyphId, default: u16) -> u16 {
        if glyph.0 < self.num_bearings {
            return self
                .hmtx
                .as_ref()
                .and_then(|h| h.advance(glyph))
                .unwrap_or(0);
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
        crate::ft::ft_metrics_bearing(self.hmtx, self.long_metrics, glyph)
    }
}

/// The tag of `sbix`.
fn sbix_tag() -> Tag {
    Tag::new(b"sbix")
}

/// FreeType's `has_outline` (`sfnt_load_face`): `glyf`, `GLYF`, `CFF `
/// or `CFF2`.
fn has_outline(sfnt: &Sfnt<'_>) -> bool {
    [b"glyf", b"GLYF", b"CFF ", b"CFF2"]
        .iter()
        .any(|t| sfnt.table(Tag::new(t)).is_some())
}

/// A font. Built once by [`Font::new`]; immutable and cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Font<'a> {
    sfnt: Sfnt<'a>,
    face_index: u32,
    head: Head<'a>,
    num_glyphs: u32,
    metrics: Metrics<'a>,
    vmetrics: Metrics<'a>,
    has_variable_h_extents: bool,
    ft_metrics: FtMetrics<'a>,
    cmap: Option<Subtable<'a>>,
    remap: Remap,
    uvs: Option<Subtable<'a>>,
    os2: Option<Os2<'a>>,
    hhea: Option<Hhea<'a>>,
    vhea: Option<Hhea<'a>>,
    post: Option<Post<'a>>,
    vorg: Option<Vorg<'a>>,
    glyf: Option<glyf_data::Glyf<'a>>,
    /// `BASE`, when HarfBuzz's sanitizer keeps it (ADR 0017).
    base: Option<Base<'a>>,
}

/// A Type 42 wrapper's PostScript glyph indices mapped to its embedded font.
pub mod type42;

/// The OpenType layout table whose FeatureList is queried (sofel ADR 0075).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutTable {
    /// Glyph substitution table.
    Gsub,
    /// Glyph positioning table.
    Gpos,
}

impl LayoutTable {
    fn tag(self) -> Tag {
        match self {
            Self::Gsub => Tag::new(b"GSUB"),
            Self::Gpos => Tag::new(b"GPOS"),
        }
    }
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

/// How HarfBuzz looks characters up in the chosen subtable
/// (`cmap::accelerator_t`, ADR 0016).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Remap {
    /// As they are.
    None,
    /// A symbol font ((3, 0)): U+0000..U+00FF also as U+F000..U+F0FF.
    Symbol,
    /// A symbol font of OS/2's simplified Arabic font page.
    ArabicSimplified,
    /// A symbol font of OS/2's traditional Arabic font page.
    ArabicTraditional,
    /// (1, 0): ASCII, then Unicode converted to MacRoman.
    MacRoman,
    /// Another Macintosh encoding: ASCII only.
    Ascii,
}

fn best_subtable<'a>(c: &Cmap<'a>) -> Option<(Subtable<'a>, (u16, u16))> {
    CMAP_ORDER.iter().find_map(|&(p, e)| {
        c.records()
            .filter(|r| r.platform_id == p && (e == ANY || r.encoding_id == e))
            .find_map(|r| r.subtable().ok().filter(|s| !matches!(s.format(), 14 | 15)))
            .map(|s| (s, (p, e)))
    })
}

/// HarfBuzz's `unicode_to_macroman`. Like HarfBuzz on little-endian
/// machines, it looks up the low 16 bits of the code point (HarfBuzz
/// reads the 32-bit key through a 16-bit comparison).
fn unicode_to_macroman(c: u32) -> Option<u32> {
    let key = u16::try_from(c & 0xFFFF).ok()?;
    MACROMAN
        .binary_search_by_key(&key, |&(u, _)| u)
        .ok()
        .and_then(|i| MACROMAN.get(i))
        .map(|&(_, m)| u32::from(m))
}

/// `(Unicode, MacRoman)` of HarfBuzz's `unicode_to_macroman`, sorted.
const MACROMAN: [(u16, u8); 128] = [
    (0x00A0, 0xCA),
    (0x00A1, 0xC1),
    (0x00A2, 0xA2),
    (0x00A3, 0xA3),
    (0x00A5, 0xB4),
    (0x00A7, 0xA4),
    (0x00A8, 0xAC),
    (0x00A9, 0xA9),
    (0x00AA, 0xBB),
    (0x00AB, 0xC7),
    (0x00AC, 0xC2),
    (0x00AE, 0xA8),
    (0x00AF, 0xF8),
    (0x00B0, 0xA1),
    (0x00B1, 0xB1),
    (0x00B4, 0xAB),
    (0x00B5, 0xB5),
    (0x00B6, 0xA6),
    (0x00B7, 0xE1),
    (0x00B8, 0xFC),
    (0x00BA, 0xBC),
    (0x00BB, 0xC8),
    (0x00BF, 0xC0),
    (0x00C0, 0xCB),
    (0x00C1, 0xE7),
    (0x00C2, 0xE5),
    (0x00C3, 0xCC),
    (0x00C4, 0x80),
    (0x00C5, 0x81),
    (0x00C6, 0xAE),
    (0x00C7, 0x82),
    (0x00C8, 0xE9),
    (0x00C9, 0x83),
    (0x00CA, 0xE6),
    (0x00CB, 0xE8),
    (0x00CC, 0xED),
    (0x00CD, 0xEA),
    (0x00CE, 0xEB),
    (0x00CF, 0xEC),
    (0x00D1, 0x84),
    (0x00D2, 0xF1),
    (0x00D3, 0xEE),
    (0x00D4, 0xEF),
    (0x00D5, 0xCD),
    (0x00D6, 0x85),
    (0x00D8, 0xAF),
    (0x00D9, 0xF4),
    (0x00DA, 0xF2),
    (0x00DB, 0xF3),
    (0x00DC, 0x86),
    (0x00DF, 0xA7),
    (0x00E0, 0x88),
    (0x00E1, 0x87),
    (0x00E2, 0x89),
    (0x00E3, 0x8B),
    (0x00E4, 0x8A),
    (0x00E5, 0x8C),
    (0x00E6, 0xBE),
    (0x00E7, 0x8D),
    (0x00E8, 0x8F),
    (0x00E9, 0x8E),
    (0x00EA, 0x90),
    (0x00EB, 0x91),
    (0x00EC, 0x93),
    (0x00ED, 0x92),
    (0x00EE, 0x94),
    (0x00EF, 0x95),
    (0x00F1, 0x96),
    (0x00F2, 0x98),
    (0x00F3, 0x97),
    (0x00F4, 0x99),
    (0x00F5, 0x9B),
    (0x00F6, 0x9A),
    (0x00F7, 0xD6),
    (0x00F8, 0xBF),
    (0x00F9, 0x9D),
    (0x00FA, 0x9C),
    (0x00FB, 0x9E),
    (0x00FC, 0x9F),
    (0x00FF, 0xD8),
    (0x0131, 0xF5),
    (0x0152, 0xCE),
    (0x0153, 0xCF),
    (0x0178, 0xD9),
    (0x0192, 0xC4),
    (0x02C6, 0xF6),
    (0x02C7, 0xFF),
    (0x02D8, 0xF9),
    (0x02D9, 0xFA),
    (0x02DA, 0xFB),
    (0x02DB, 0xFE),
    (0x02DC, 0xF7),
    (0x02DD, 0xFD),
    (0x03A9, 0xBD),
    (0x03C0, 0xB9),
    (0x2013, 0xD0),
    (0x2014, 0xD1),
    (0x2018, 0xD4),
    (0x2019, 0xD5),
    (0x201A, 0xE2),
    (0x201C, 0xD2),
    (0x201D, 0xD3),
    (0x201E, 0xE3),
    (0x2020, 0xA0),
    (0x2021, 0xE0),
    (0x2022, 0xA5),
    (0x2026, 0xC9),
    (0x2030, 0xE4),
    (0x2039, 0xDC),
    (0x203A, 0xDD),
    (0x2044, 0xDA),
    (0x20AC, 0xDB),
    (0x2122, 0xAA),
    (0x2202, 0xB6),
    (0x2206, 0xC6),
    (0x220F, 0xB8),
    (0x2211, 0xB7),
    (0x221A, 0xC3),
    (0x221E, 0xB0),
    (0x222B, 0xBA),
    (0x2248, 0xC5),
    (0x2260, 0xAD),
    (0x2264, 0xB2),
    (0x2265, 0xB3),
    (0x25CA, 0xD7),
    (0xF8FF, 0xF0),
    (0xFB01, 0xDE),
    (0xFB02, 0xDF),
];

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
    /// Opens a logical face of a Portable Font Resource.
    pub fn from_pfr(data: &'a [u8], index: u32) -> Result<PfrFace<'a>, FontError> {
        read::pfr::PfrFace::read(data, index)
            .map(PfrFace)
            .map_err(FontError::Read)
    }

    /// Opens a Type 42 wrapper and its embedded sfnt.
    ///
    /// The source must outlive the returned face. Its PostScript encoding and
    /// CharStrings determine the public glyph indices.
    pub fn from_type42(
        source: &'a sofel_type1::type42::Type42Source,
        index: u32,
    ) -> Result<type42::Type42Face<'a>, FontError> {
        type42::Type42Face::new(source, index)
    }

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
        Self::open(data, index, None)
    }

    /// Opens a shaping face. When the header is unavailable, HarfBuzz uses
    /// 1000 design units per em. Table queries still return the source bytes.
    pub fn for_shaping(data: &'a [u8], index: u32) -> Result<Self, FontError> {
        // Only the typed header view needs a default. Other tables continue
        // to be read and validated independently from the original directory.
        const HEADER: [u8; 54] = {
            let mut bytes = [0; 54];
            bytes[1] = 1;
            bytes[18] = 3;
            bytes[19] = 232;
            bytes
        };
        Self::open(data, index, Head::new(&HEADER).ok())
    }

    fn open(data: &'a [u8], index: u32, fallback: Option<Head<'a>>) -> Result<Self, FontError> {
        let sfnt = match FontFile::new(data).map_err(FontError::Read)? {
            FontFile::Sfnt(s) if index == 0 => s,
            FontFile::Sfnt(_) => return Err(FontError::IndexOutOfRange),
            FontFile::Collection(c) if index < c.num_fonts() => {
                c.font(index).map_err(FontError::Read)?
            }
            FontFile::Collection(_) => return Err(FontError::IndexOutOfRange),
            FontFile::Dfont(d) if index < d.num_fonts() => {
                d.font(index).map_err(FontError::Read)?
            }
            FontFile::Dfont(_) => return Err(FontError::IndexOutOfRange),
        };
        // FreeType reads Apple's `bhed` instead of `head` for fonts
        // without outlines, unless they have `sbix` (sfobjs.c).
        let apple_sbit = !has_outline(&sfnt) && sfnt.table(sbix_tag()).is_none();
        let head = apple_sbit
            .then(|| sfnt.table(head::TAG_BHED).and_then(|d| Head::new(d).ok()))
            .flatten()
            .or_else(|| sfnt.table(head::TAG).and_then(|d| Head::new(d).ok()))
            .or(fallback)
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
        let best = cmap_table.as_ref().and_then(best_subtable);
        let os2_view = sfnt.table(os2::TAG).and_then(|d| Os2::new(d).ok());
        // OS2::get_font_page: the high byte of fsSelection in version 0.
        let page = os2_view
            .filter(|o| o.version() == 0)
            .map_or(0, |o| o.fs_selection() & 0xFF00);
        let remap = match best.map(|(_, k)| k) {
            Some((3, 0)) => match page {
                0 => Remap::Symbol,
                0xB200 => Remap::ArabicSimplified,
                0xB300 => Remap::ArabicTraditional,
                _ => Remap::None,
            },
            Some((1, 0)) => Remap::MacRoman,
            Some((1, _)) => Remap::Ascii,
            _ => Remap::None,
        };
        let cmap = best.map(|(s, _)| s);
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

        let has_variable_h_extents = sfnt
            .table(sofel_read::mvar::TAG)
            .and_then(|data| sofel_read::mvar::Mvar::new(data).ok())
            .is_some_and(|mvar| {
                [b"hasc", b"hdsc", b"hlgp"]
                    .iter()
                    .any(|tag| mvar.find(Tag::new(tag)).is_some())
            });

        Ok(Font {
            sfnt,
            face_index: index,
            head,
            num_glyphs,
            metrics,
            vmetrics,
            has_variable_h_extents,
            ft_metrics: FtMetrics::new(&sfnt),
            cmap,
            remap,
            uvs,
            os2: os2_view,
            hhea: dual_hea(&sfnt, hhea::TAG, hhea::TAG_WIDE),
            vhea: dual_hea(&sfnt, vhea::TAG, vhea::TAG_WIDE),
            post,
            vorg: sfnt.table(vorg::TAG).and_then(|d| Vorg::new(d).ok()),
            glyf: glyf_data::Glyf::new(&sfnt, &head, num_glyphs),
            base: sfnt
                .table(base::TAG)
                .filter(|d| base_sanitize::sane(d))
                .and_then(|d| Base::new(d).ok()),
        })
    }

    /// Whether the font has outlines as FreeType decides
    /// (`sfnt_load_face`): `glyf`, `GLYF`, `CFF ` or `CFF2`, and no `CBLC`
    /// or `CBDT` (FreeType ignores the outlines of fonts with color
    /// bitmaps).
    pub fn has_outlines(&self) -> bool {
        has_outline(&self.sfnt)
            && self.sfnt.table(Tag::new(b"CBLC")).is_none()
            && self.sfnt.table(Tag::new(b"CBDT")).is_none()
    }

    /// Whether the font has embedded bitmaps: `CBLC`, `EBLC`, `bloc` or
    /// `sbix` (the tables FreeType reads strikes from).
    pub fn has_bitmaps(&self) -> bool {
        [b"CBLC", b"EBLC", b"bloc", b"sbix"]
            .iter()
            .any(|t| self.sfnt.table(Tag::new(t)).is_some())
    }

    /// Whether HarfBuzz can read vertical advances from `vmtx` or `VMTX`.
    pub fn has_vertical_metrics(&self) -> bool {
        self.vmetrics.has_data()
    }

    /// The sfnt the font was opened from: for a collection, the one font
    /// chosen by the index.
    pub fn sfnt(&self) -> Sfnt<'a> {
        self.sfnt
    }

    /// The effective header. [`Font::for_shaping`] supplies a default when needed.
    pub fn head(&self) -> Head<'a> {
        self.head
    }

    /// `numGlyphs` of `MAXP` if present, else of `maxp`, else 0.
    pub fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }

    /// `unitsPerEm` as stored, even outside 16..=16384.
    #[inline]
    pub fn units_per_em(&self) -> u16 {
        self.head.units_per_em()
    }

    /// The bytes of a table.
    pub fn table(&self, tag: Tag) -> Option<&'a [u8]> {
        self.sfnt.table(tag)
    }

    /// Raw layout bytes retained by HarfBuzz's shaping-time checks.
    pub fn shaping_table(&self, tag: Tag) -> Option<&'a [u8]> {
        use sofel_read::layout::sanitize::{Admission, assess};
        self.table(tag)
            .filter(|d| assess(tag, Some(d)) == Admission::Accepted)
    }

    /// Copy a page of FeatureList tags in record order, preserving duplicates.
    /// Returns `(written, total)`; a missing or rejected table is empty.
    pub fn feature_tags(&self, table: LayoutTable, start: u32, out: &mut [Tag]) -> (usize, usize) {
        let Some(features) = self
            .shaping_table(table.tag())
            .and_then(|data| sofel_read::layout::LayoutTable::new(data).ok())
            .and_then(|layout| layout.feature_list().ok().flatten())
        else {
            return (0, 0);
        };
        let total = features.len();
        let start = usize::try_from(start).unwrap_or(usize::MAX).min(total);
        let mut written = 0usize;
        for (dest, index) in out.iter_mut().zip(start..total) {
            let Some(tag) = features.tag(index) else {
                break;
            };
            *dest = tag;
            written = written.saturating_add(1);
        }
        (written, total)
    }

    /// The glyph for a character, from the chosen cmap subtable (in
    /// HarfBuzz's order of preference, `find_best_subtable`), looked up
    /// as HarfBuzz does (ADR 0016): in a symbol font ((3, 0)), a
    /// character without a glyph is looked up again as U+F000 + c for
    /// U+0000..U+00FF, or through HarfBuzz's Arabic PUA maps when OS/2's
    /// font page is simplified or traditional Arabic; with (1, 0),
    /// characters past ASCII are converted to MacRoman; with other
    /// Macintosh encodings only ASCII is looked up. Glyph ids not below
    /// the glyph count count as unmapped (ADR 0010). `None` also when the
    /// font has no usable subtable.
    pub fn nominal_glyph(&self, c: u32) -> Option<GlyphId> {
        self.lookup(c).filter(|g| g.0 < self.num_glyphs)
    }

    /// The glyph for a character as HarfBuzz's `hb_font_get_nominal_glyph`
    /// with the `ot` font functions gives it, for shaping (ADR 0020):
    /// [`Font::nominal_glyph`] with every glyph id the subtable gives,
    /// glyph ids not below the glyph count included (their advances are
    /// 0 and they have no outlines), and nothing from subtables of
    /// formats 2 and 8, which HarfBuzz does not look up
    /// (`CmapSubtable::get_glyph`).
    pub fn nominal_glyph_raw(&self, c: u32) -> Option<GlyphId> {
        if matches!(self.cmap.map(|s| s.format()), Some(2 | 8)) {
            return None;
        }
        self.lookup(c)
    }

    /// The lookup of [`Font::nominal_glyph`], before the glyph count.
    fn lookup(&self, c: u32) -> Option<GlyphId> {
        let s = self.cmap?;
        let again = |m: u32| (m != 0).then(|| s.glyph(m)).flatten();
        match self.remap {
            Remap::None => s.glyph(c),
            Remap::Symbol => s
                .glyph(c)
                .or_else(|| (c <= 0xFF).then(|| s.glyph(0xF000 | c)).flatten()),
            Remap::ArabicSimplified => s
                .glyph(c)
                .or_else(|| again(tables::arabic_pua::simp_map(c))),
            Remap::ArabicTraditional => s
                .glyph(c)
                .or_else(|| again(tables::arabic_pua::trad_map(c))),
            Remap::MacRoman => (c < 0x80)
                .then(|| s.glyph(c))
                .flatten()
                .or_else(|| unicode_to_macroman(c).and_then(|m| s.glyph(m))),
            Remap::Ascii => (c < 0x80).then(|| s.glyph(c)).flatten(),
        }
    }

    /// The glyph for the variation sequence `c` + `selector`, from the
    /// (0, 5) subtable: format 15 if present, else the last format 14. A
    /// default variation falls back to [`Font::nominal_glyph`]; glyph ids
    /// not below the glyph count count as unmapped.
    pub fn variation_glyph(&self, c: u32, selector: u32) -> Option<GlyphId> {
        match self.uvs?.variant(c, selector)? {
            VariantGlyph::Default => self.lookup(c),
            VariantGlyph::Glyph(g) => Some(g),
        }
        .filter(|g| g.0 < self.num_glyphs)
    }

    /// [`Font::variation_glyph`] with every glyph id, as HarfBuzz's
    /// `hb_font_get_variation_glyph` gives it (see
    /// [`Font::nominal_glyph_raw`]).
    pub fn variation_glyph_raw(&self, c: u32, selector: u32) -> Option<GlyphId> {
        match self.uvs?.variant(c, selector)? {
            VariantGlyph::Default => self.nominal_glyph_raw(c),
            VariantGlyph::Glyph(g) => Some(g),
        }
    }

    /// `unitsPerEm` as HarfBuzz uses it (`hb_face_get_upem`): 1000 when
    /// the stored value is outside 16..=16384.
    #[inline]
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
}
