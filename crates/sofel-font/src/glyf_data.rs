//! glyf glyph headers as HarfBuzz 15.1.0 reads them for metrics
//! (`src/OT/glyf/glyf.hh` `accelerator_t`, `src/OT/glyf/Glyph.hh`),
//! without the outline layer (ADR 0016): the box of the header and the
//! top phantom point. Ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372), `src/OT/glyf/` and
//! `src/hb-decycler.hh`. Copyright © the HarfBuzz authors. Used under
//! HarfBuzz's "Old MIT" license; see THIRD_PARTY_NOTICES.md.

use sofel_read::be;
use sofel_read::head::Head;
use sofel_read::sfnt::Sfnt;
use sofel_read::{GlyphId, glyf, loca};

/// `HB_MAX_NESTING_LEVEL`.
const MAX_NESTING: u32 = 64;

const USE_MY_METRICS: u16 = 0x0200;
const MORE_COMPONENTS: u16 = 0x0020;
const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
const GID_IS_24_BIT: u16 = 0x2000;

/// The glyf and loca tables as HarfBuzz's accelerator holds them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Glyf<'a> {
    glyf: &'a [u8],
    loca: &'a [u8],
    short: bool,
    extended: bool,
    /// The glyphs loca covers, at most the font's glyph count.
    num_glyphs: u32,
}

/// The parts of a glyph HarfBuzz's metrics use.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Glyph<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) contours: i16,
    pub(crate) x_min: i16,
    pub(crate) y_min: i16,
    pub(crate) x_max: i16,
    pub(crate) y_max: i16,
}

impl Glyph<'_> {
    /// A glyph with no data: HarfBuzz's empty glyph over its all-zero
    /// header.
    const EMPTY: Glyph<'static> = Glyph {
        bytes: &[],
        contours: 0,
        x_min: 0,
        y_min: 0,
        x_max: 0,
        y_max: 0,
    };

    pub(crate) fn is_empty(&self) -> bool {
        self.contours == 0
    }
}

impl<'a> Glyf<'a> {
    /// `None` when HarfBuzz has no glyf data: unknown `indexToLocFormat`
    /// or `glyphDataFormat` (above 1), or no glyph that loca covers.
    pub(crate) fn new(sfnt: &Sfnt<'a>, head: &Head<'a>, num_glyphs: u32) -> Option<Glyf<'a>> {
        if head.index_to_loc_format() > 1 || head.glyph_data_format() > 1 {
            return None;
        }
        let short = head.index_to_loc_format() == 0;
        let wide = (sfnt.table(glyf::TAG_WIDE), sfnt.table(loca::TAG_WIDE));
        let (glyf, loca, extended) = match wide {
            (Some(g), Some(l)) if !g.is_empty() && !l.is_empty() => (g, l, true),
            _ => (
                sfnt.table(glyf::TAG).unwrap_or_default(),
                sfnt.table(loca::TAG).unwrap_or_default(),
                false,
            ),
        };
        let entries = if short {
            loca.len() / 2
        } else {
            loca.len() / 4
        };
        let covered = u32::try_from(entries.max(1).saturating_sub(1)).unwrap_or(u32::MAX);
        let num_glyphs = covered.min(num_glyphs);
        (num_glyphs != 0).then_some(Glyf {
            glyf,
            loca,
            short,
            extended,
            num_glyphs,
        })
    }

    pub(crate) fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }

    pub(crate) fn extended(&self) -> bool {
        self.extended
    }

    fn location(&self, index: u32) -> Option<usize> {
        let i = usize::try_from(index).ok()?;
        if self.short {
            let at = i.checked_mul(2)?;
            be::u16_at(self.loca, at).map(|v| usize::from(v).saturating_mul(2))
        } else {
            let at = i.checked_mul(4)?;
            be::u32_at(self.loca, at).map(be::to_usize)
        }
    }

    /// HarfBuzz's `glyph_for_gid`: the empty glyph for glyph ids past the
    /// data and for bad loca entries.
    pub(crate) fn glyph(&self, gid: GlyphId) -> Glyph<'a> {
        if gid.0 >= self.num_glyphs {
            return Glyph::EMPTY;
        }
        let (Some(start), Some(end)) =
            (self.location(gid.0), self.location(gid.0.saturating_add(1)))
        else {
            return Glyph::EMPTY;
        };
        if start > end || end > self.glyf.len() {
            return Glyph::EMPTY;
        }
        let bytes = self.glyf.get(start..end).unwrap_or_default();
        let Some(h) = bytes.first_chunk::<10>() else {
            // Shorter than a header: HarfBuzz reads its all-zero header.
            return Glyph {
                bytes,
                ..Glyph::EMPTY
            };
        };
        let i = |a: u8, b: u8| i16::from_be_bytes([a, b]);
        Glyph {
            bytes,
            contours: i(h[0], h[1]),
            x_min: i(h[2], h[3]),
            y_min: i(h[4], h[5]),
            x_max: i(h[6], h[7]),
            y_max: i(h[8], h[9]),
        }
    }

    /// The components of a composite glyph as `(flags, glyph id)`, as
    /// HarfBuzz's `composite_iter_t` walks them: each record must fit in
    /// the glyph's bytes, and the walk stops after a record without
    /// MORE_COMPONENTS.
    pub(crate) fn components(&self, glyph: &Glyph<'a>) -> Components<'a> {
        Components {
            bytes: glyph.bytes,
            at: Some(10),
            extended: self.extended,
        }
    }

    /// HarfBuzz's top phantom point of glyph `gid` without variations
    /// (`Glyph::get_points` with `phantom_only`): the header's `yMax`
    /// plus the top side bearing, or for a composite glyph with
    /// USE_MY_METRICS components, that of the last such component
    /// (recursively). `None` when HarfBuzz fails to read the glyph.
    ///
    /// `tsb` gives the top side bearing of a glyph id (0 past the
    /// bearings, also for the all-ones id HarfBuzz uses for components
    /// past the glyph data).
    pub(crate) fn phantom_top(&self, gid: GlyphId, tsb: &dyn Fn(GlyphId) -> i16) -> Option<i32> {
        self.phantom_at(gid, tsb, 0, &mut Decycler::default())
    }

    fn phantom_at(
        &self,
        gid: GlyphId,
        tsb: &dyn Fn(GlyphId) -> i16,
        depth: u32,
        decycler: &mut Decycler,
    ) -> Option<i32> {
        if depth > MAX_NESTING {
            return None;
        }
        // Components past the data are HarfBuzz's empty glyph with the
        // id -1.
        let (glyph, gid) = if gid.0 < self.num_glyphs {
            (self.glyph(gid), gid)
        } else {
            (Glyph::EMPTY, GlyphId(u32::MAX))
        };
        let own = i32::from(glyph.y_max).saturating_add(i32::from(tsb(gid)));
        if glyph.contours > 0 {
            // SimpleGlyph::get_contour_points with phantom_only: the end
            // points and the instruction length must be there, and the
            // last end point must count at least one point per contour.
            let n = usize::from(glyph.contours.unsigned_abs());
            let last = be::u16_at(
                glyph.bytes,
                10usize.saturating_add(n.saturating_sub(1).saturating_mul(2)),
            )?;
            be::u16_at(glyph.bytes, 10usize.saturating_add(n.saturating_mul(2)))?;
            let points = u32::from(last).saturating_add(1);
            if points < u32::from(glyph.contours.unsigned_abs()) {
                return None;
            }
            return Some(own);
        }
        if glyph.contours == 0 {
            return Some(own);
        }
        let level = decycler.enter();
        let metrics = self
            .components(&glyph)
            .filter(|&(flags, _)| flags & USE_MY_METRICS != 0)
            .filter(|&(_, g)| decycler.visit(level, g))
            .last();
        let result = match metrics {
            None => Some(own),
            Some((_, g)) => {
                decycler.visit(level, g);
                self.phantom_at(GlyphId(g), tsb, depth.saturating_add(1), decycler)
            }
        };
        decycler.leave();
        result
    }
}

/// HarfBuzz's `hb_decycler_t` over the composite levels of one glyph:
/// a tortoise-and-hare check that compares a component's glyph id with
/// the one being expanded at half the current level's depth.
#[derive(Debug)]
struct Decycler {
    values: [u32; 72],
    levels: usize,
}

impl Default for Decycler {
    fn default() -> Decycler {
        Decycler {
            values: [0; 72],
            levels: 0,
        }
    }
}

impl Decycler {
    fn enter(&mut self) -> usize {
        let level = self.levels;
        self.levels = self.levels.saturating_add(1);
        level
    }

    fn leave(&mut self) {
        self.levels = self.levels.saturating_sub(1);
    }

    fn visit(&mut self, level: usize, value: u32) -> bool {
        if let Some(v) = self.values.get_mut(level) {
            *v = value;
        }
        let tortoise = level / 2;
        tortoise == level || self.values.get(tortoise) != Some(&value)
    }
}

/// See [`Glyf::components`].
#[derive(Clone, Debug)]
pub(crate) struct Components<'a> {
    bytes: &'a [u8],
    at: Option<usize>,
    extended: bool,
}

impl Iterator for Components<'_> {
    type Item = (u16, u32);

    fn next(&mut self) -> Option<(u16, u32)> {
        let at = self.at?;
        self.at = None;
        let flags = be::u16_at(self.bytes, at)?;
        // The record's minimum size is 4 bytes.
        be::slice_at(self.bytes, at, 4)?;
        let wide = self.extended && flags & GID_IS_24_BIT != 0;
        let mut size = 4usize;
        if wide {
            size = size.saturating_add(1);
        }
        size = size.saturating_add(if flags & ARG_1_AND_2_ARE_WORDS != 0 {
            4
        } else {
            2
        });
        if flags & WE_HAVE_A_SCALE != 0 {
            size = size.saturating_add(2);
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            size = size.saturating_add(4);
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            size = size.saturating_add(8);
        }
        be::slice_at(self.bytes, at, size)?;
        let gid_at = at.saturating_add(2);
        let gid = if wide {
            be::u24_at(self.bytes, gid_at)?
        } else {
            u32::from(be::u16_at(self.bytes, gid_at)?)
        };
        if flags & MORE_COMPONENTS != 0 {
            self.at = Some(at.saturating_add(size));
        }
        Some((flags, gid))
    }
}
