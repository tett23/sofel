//! One font, read once (ADR 0010).
//!
//! [`Font`] opens an sfnt font or a font in a TTC/OTC collection and keeps
//! what shaping and drawing share: the glyph count, the cmap subtable used
//! for characters, and the horizontal metrics. The policies for broken or
//! missing data follow HarfBuzz 15.1.0, the reference for shaping
//! (ADR 0005). `sofel-read` is re-exported as [`read`]; other crates use
//! table views through it instead of reading bytes (ADR 0003).

#![no_std]
#![forbid(unsafe_code)]

pub use sofel_read as read;

use core::fmt;

use sofel_read::cmap::{Cmap, Subtable, VariantGlyph};
use sofel_read::head::Head;
use sofel_read::hhea::Hhea;
use sofel_read::hmtx::Hmtx;
use sofel_read::maxp::Maxp;
use sofel_read::sfnt::{FontFile, Sfnt};
use sofel_read::{CountWidth, GlyphId, ReadError, Tag, cmap, head, hhea, hmtx, maxp};

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

/// Horizontal metrics, with the counts HarfBuzz derives from the data
/// (`hb-ot-hmtx-table.hh`, `accelerator_t`).
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
}

/// A font. Built once by [`Font::new`]; immutable and cheap to copy.
#[derive(Clone, Copy, Debug)]
pub struct Font<'a> {
    sfnt: Sfnt<'a>,
    head: Head<'a>,
    num_glyphs: u32,
    metrics: Metrics<'a>,
    cmap: Option<Subtable<'a>>,
    uvs: Option<Subtable<'a>>,
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
    /// collection.
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

        let wide = sfnt.table(hhea::TAG_WIDE).is_some() && sfnt.table(hmtx::TAG_WIDE).is_some();
        let (hhea_tag, hmtx_tag, width) = if wide {
            (hhea::TAG_WIDE, hmtx::TAG_WIDE, CountWidth::U24)
        } else {
            (hhea::TAG, hmtx::TAG, CountWidth::U16)
        };
        let long_metrics = sfnt
            .table(hhea_tag)
            .and_then(|d| Hhea::new(d, width).ok())
            .map_or(0, |h| h.number_of_h_metrics());
        let metrics = Metrics::new(sfnt.table(hmtx_tag), long_metrics, num_glyphs);

        let cmap_table = sfnt.table(cmap::TAG).and_then(|d| Cmap::new(d).ok());
        let cmap = cmap_table.as_ref().and_then(best_subtable);
        let uvs = cmap_table.as_ref().and_then(uvs_subtable);

        Ok(Font {
            sfnt,
            head,
            num_glyphs,
            metrics,
            cmap,
            uvs,
        })
    }

    pub fn sfnt(&self) -> Sfnt<'a> {
        self.sfnt
    }

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

    /// The glyph for a character, from the chosen cmap subtable. Glyph
    /// ids not below the glyph count count as unmapped.
    pub fn nominal_glyph(&self, c: u32) -> Option<GlyphId> {
        self.cmap?.glyph(c).filter(|g| g.0 < self.num_glyphs)
    }

    /// The glyph for the variation sequence `c` + `selector`.
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
        let m = &self.metrics;
        if glyph.0 < m.num_bearings {
            return m.hmtx.and_then(|h| h.advance(glyph)).unwrap_or(0);
        }
        if m.num_advances == 0 {
            let upem = self.units_per_em();
            let upem = if (16..=16384).contains(&upem) {
                upem
            } else {
                1000
            };
            return upem / 2;
        }
        0
    }
}
