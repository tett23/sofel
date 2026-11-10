//! Baselines and the BASE MinMax font extents as HarfBuzz 15.1.0 gives
//! them (ADR 0017).
//!
//! Ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372): `src/hb-ot-layout.cc`
//! (`hb_ot_layout_get_baseline`, `hb_ot_layout_get_baseline_with_fallback`,
//! `hb_ot_layout_get_font_extents`), `src/hb-ot-layout-base-table.hh`
//! (`BASE::get_baseline`, `BASE::get_min_max`) and `src/hb-ot-tag.cc`
//! (`hb_ot_tag_to_script`). Copyright © the HarfBuzz authors. Used under
//! HarfBuzz's "Old MIT" license; see THIRD_PARTY_NOTICES.md.

use sofel_read::base::{BaseScript, MinMax};
use sofel_read::{GlyphId, Tag};

use crate::baseline;
use crate::{Font, FontExtents, GlyphExtents, HbScale, MetricsTag};

/// Horizontal or vertical text, for the queries that only need to know
/// which axis (ADR 0017). The four shaping directions map to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Direction {
    /// Left to right or right to left.
    Horizontal,
    /// Top to bottom or bottom to top.
    Vertical,
}

/// `DFLT`, the default script.
const DFLT: Tag = Tag::new(b"DFLT");
/// The null tag, which HarfBuzz's records treat as no record
/// (`has_data`).
const NONE: Tag = Tag([0; 4]);

/// `hb_clamp_to<hb_position_t> (int64_t)`.
fn clamp_i32(v: i64) -> i32 {
    i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0)
}

/// HarfBuzz's `hb_ot_tag_to_script`, as the raw `hb_script_t` value
/// (an ISO 15924 tag; 0 for `DFLT`).
fn tag_to_script(tag: Tag) -> u32 {
    let mut t = tag.to_u32();
    let digit = t & 0xFF;
    if digit == u32::from(b'2') || digit == u32::from(b'3') {
        let new = Tag::from_u32(t & 0xFFFF_FF32);
        let script: &[u8; 4] = match &new.0 {
            b"bng2" => b"Beng",
            b"dev2" => b"Deva",
            b"gjr2" => b"Gujr",
            b"gur2" => b"Guru",
            b"knd2" => b"Knda",
            b"mlm2" => b"Mlym",
            b"ory2" => b"Orya",
            b"tml2" => b"Taml",
            b"tel2" => b"Telu",
            b"mym2" => b"Mymr",
            _ => b"Zzzz",
        };
        return u32::from_be_bytes(*script);
    }
    if tag == DFLT {
        return 0;
    }
    if tag == Tag::new(b"math") {
        return u32::from_be_bytes(*b"Zmth");
    }
    // Spaces at the end repeat the letter before them ('nko ' is Nkoo).
    if t & 0x0000_FF00 == 0x0000_2000 {
        t |= (t >> 8) & 0x0000_FF00;
    }
    if t & 0x0000_00FF == 0x0000_0020 {
        t |= (t >> 8) & 0x0000_00FF;
    }
    t & !0x2000_0000
}

/// The character whose glyph top is the hanging baseline of a script
/// (`hb_ot_layout_get_baseline_with_fallback`), by HarfBuzz's script.
/// To be replaced by the script type of ADR 0019.
const HANGING_CHARACTERS: [(&[u8; 4], u32); 21] = [
    (b"Beng", 0x0995),
    (b"Deva", 0x0915),
    (b"Gujr", 0x0A95),
    (b"Guru", 0x0A15),
    (b"Tibt", 0x0F40),
    (b"Limb", 0x1901),
    (b"Sylo", 0xA807),
    (b"Phag", 0xA840),
    (b"Mtei", 0xABC0),
    (b"Shrd", 0x11191),
    (b"Takr", 0x1168C),
    (b"Modi", 0x1160E),
    (b"Sidd", 0x11590),
    (b"Tirh", 0x1148F),
    (b"Marc", 0x11C72),
    (b"Newa", 0x1140E),
    (b"Soyo", 0x11A5C),
    (b"Zanb", 0x11A0B),
    (b"Dogr", 0x1180A),
    (b"Gong", 0x11D6C),
    (b"Nand", 0x119B0),
];

fn hanging_character(script: Tag) -> Option<u32> {
    let s = tag_to_script(script);
    HANGING_CHARACTERS
        .iter()
        .find(|(t, _)| u32::from_be_bytes(**t) == s)
        .map(|&(_, c)| c)
}

/// The BaseScript of `script` on the axis of `direction`
/// (`BaseScriptList::get_base_script`): its record, else `DFLT`'s;
/// `None` for no record or a null BaseScript (HarfBuzz's `Null`).
fn base_script<'a>(font: &Font<'a>, direction: Direction, script: Tag) -> Option<BaseScript<'a>> {
    let axis = font
        .base?
        .axis(direction == Direction::Vertical)
        .ok()
        .flatten()?;
    let list = axis.base_script_list().ok().flatten()?;
    let found = |t: Tag| list.find(t).filter(|_| t != NONE);
    let i = found(script).or_else(|| found(DFLT))?;
    list.script(i).ok().flatten()
}

impl HbScale {
    /// `em_scale_y` for horizontal text, `em_scale_x` for vertical text
    /// (`BaseCoord::get_coord`).
    fn base_scale(&self, v: i16, direction: Direction) -> i32 {
        match direction {
            Direction::Horizontal => self.em_scale_y(i32::from(v)),
            Direction::Vertical => self.em_scale_x(i32::from(v)),
        }
    }

    fn extents_for(&self, font: &Font<'_>, direction: Direction) -> FontExtents {
        match direction {
            Direction::Horizontal => self.h_extents(font),
            Direction::Vertical => self.v_extents(font),
        }
    }

    /// [`Font::baseline`] at this scale.
    pub fn baseline(
        &self,
        font: &Font<'_>,
        tag: Tag,
        direction: Direction,
        script: Tag,
    ) -> Option<i32> {
        let axis = font
            .base?
            .axis(direction == Direction::Vertical)
            .ok()
            .flatten()?;
        let s = base_script(font, direction, script)?;
        let values = s.base_values().ok().flatten()?;
        let index = axis.base_tag_list().ok().flatten()?.find(tag)?;
        let coord = values.coord(index).ok().flatten()?;
        Some(self.base_scale(coord.coordinate(), direction))
    }

    /// [`Font::baseline_with_fallback`] at this scale; `extents` gives
    /// boxes in font units, which are scaled with
    /// [`HbScale::scale_glyph_extents`].
    pub fn baseline_with_fallback(
        &self,
        font: &Font<'_>,
        tag: Tag,
        direction: Direction,
        script: Tag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        self.baseline_fallback(font, tag, direction, script, &extents)
    }

    fn baseline_fallback(
        &self,
        font: &Font<'_>,
        tag: Tag,
        direction: Direction,
        script: Tag,
        extents: &dyn Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        if let Some(v) = self.baseline(font, tag, direction, script) {
            return v;
        }
        let horizontal = direction == Direction::Horizontal;
        let em = if horizontal {
            self.y_scale()
        } else {
            self.x_scale()
        };
        let glyph_box = |g: GlyphId| extents(g).map(|e| self.scale_glyph_extents(e));
        let again = |t: Tag| self.baseline_fallback(font, t, direction, script, extents);
        let sum = |a: i32, b: i32| i64::from(a).saturating_add(i64::from(b));
        let diff = |a: i32, b: i32| i64::from(a).saturating_sub(i64::from(b));
        match tag {
            baseline::MATH => {
                let minus = font
                    .nominal_glyph(0x2212)
                    .or_else(|| font.nominal_glyph(u32::from('-')));
                match minus.filter(|_| horizontal).and_then(glyph_box) {
                    Some(e) => e.y_bearing.saturating_add(e.height / 2),
                    None => {
                        self.metrics_position_with_fallback(font, MetricsTag::XHeight, extents) / 2
                    }
                }
            }
            baseline::IDEO_FACE_TOP_OR_RIGHT | baseline::IDEO_FACE_BOTTOM_OR_LEFT => {
                let top = again(baseline::IDEO_EMBOX_TOP_OR_RIGHT);
                let bottom = again(baseline::IDEO_EMBOX_BOTTOM_OR_LEFT);
                if tag == baseline::IDEO_FACE_TOP_OR_RIGHT {
                    top.saturating_add(clamp_i32(diff(bottom, top) / 10))
                } else {
                    bottom.saturating_add(clamp_i32(diff(top, bottom) / 10))
                }
            }
            baseline::IDEO_EMBOX_TOP_OR_RIGHT => {
                match self.baseline(font, baseline::IDEO_EMBOX_BOTTOM_OR_LEFT, direction, script) {
                    Some(v) => v.saturating_add(em),
                    None => self.extents_for(font, direction).ascender,
                }
            }
            baseline::IDEO_EMBOX_BOTTOM_OR_LEFT => {
                match self.baseline(font, baseline::IDEO_EMBOX_TOP_OR_RIGHT, direction, script) {
                    Some(v) => v.saturating_sub(em),
                    None => self.extents_for(font, direction).descender,
                }
            }
            baseline::HANGING => {
                let top = hanging_character(script)
                    .filter(|_| horizontal)
                    .and_then(|c| font.nominal_glyph(c))
                    .and_then(glyph_box);
                match top {
                    Some(e) => e.y_bearing,
                    None => clamp_i32(i64::from(em).saturating_mul(6) / 10),
                }
            }
            baseline::IDEO_EMBOX_CENTRAL => {
                let top = again(baseline::IDEO_EMBOX_TOP_OR_RIGHT);
                let bottom = again(baseline::IDEO_EMBOX_BOTTOM_OR_LEFT);
                clamp_i32(sum(top, bottom) / 2)
            }
            baseline::IDEO_FACE_CENTRAL => {
                let top = again(baseline::IDEO_FACE_TOP_OR_RIGHT);
                let bottom = again(baseline::IDEO_FACE_BOTTOM_OR_LEFT);
                clamp_i32(sum(top, bottom) / 2)
            }
            // romn, and tags HarfBuzz does not know.
            _ => 0,
        }
    }

    /// [`Font::layout_font_extents`] at this scale.
    pub fn layout_font_extents(
        &self,
        font: &Font<'_>,
        direction: Direction,
        script: Tag,
        language: Tag,
    ) -> (FontExtents, bool) {
        let Some(s) = base_script(font, direction, script)
            .filter(|s| s.default_min_max_offset() != 0 || s.num_lang_sys() != 0)
        else {
            return (self.extents_for(font, direction), false);
        };
        let mm: Option<MinMax<'_>> = match s.find_lang_sys(language).filter(|_| language != NONE) {
            Some(i) => s.lang_sys_min_max(i).ok().flatten(),
            None => s.default_min_max().ok().flatten(),
        };
        let coord = |c: Option<sofel_read::base::BaseCoord<'_>>| {
            c.map_or(0, |c| self.base_scale(c.coordinate(), direction))
        };
        let (min, max) = mm.map_or((0, 0), |m| {
            (
                coord(m.min_coord().ok().flatten()),
                coord(m.max_coord().ok().flatten()),
            )
        });
        (
            FontExtents {
                ascender: max,
                descender: min,
                line_gap: 0,
            },
            true,
        )
    }
}
