//! Font-wide and glyph metrics as HarfBuzz 15.1.0 gives them, and
//! HarfBuzz's scaling (ADR 0016).
//!
//! Ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372): `src/hb-ot-font.cc`
//! (`hb_ot_get_font_h_extents`, `hb_ot_get_glyph_v_advances`,
//! `hb_ot_get_glyph_v_origins`), `src/hb-ot-metrics.cc`, `src/hb-font.hh`
//! (`em_mult`, `em_multf`, `scale_glyph_extents`,
//! `get_h_extents_with_fallback`) and `src/OT/glyf/GlyphHeader.hh`.
//! Copyright © the HarfBuzz authors. Used under HarfBuzz's "Old MIT"
//! license; see THIRD_PARTY_NOTICES.md.

use sofel_read::GlyphId;
use sofel_read::os2::{self, Os2};
use sofel_read::vorg::Vorg;

use crate::Font;
use sofel_geom::float::{
    ceilf, f32_to_i64, floorf, hb_clamp_to_i32, hb_roundf32, i32_to_f32, i64_to_f32,
};

/// A font's ascender, descender and line gap (`hb_font_extents_t`).
/// The descender is negative below the baseline.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FontExtents {
    /// Distance from the baseline to the top.
    pub ascender: i32,
    /// Distance from the baseline to the bottom; negative below it.
    pub descender: i32,
    /// Suggested gap between lines.
    pub line_gap: i32,
}

pub use sofel_geom::GlyphExtents;

/// The font-wide metrics HarfBuzz's `hb_ot_metrics_get_position` knows
/// (`hb_ot_metrics_tag_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MetricsTag {
    /// `hasc`: horizontal ascender.
    HorizontalAscender,
    /// `hdsc`: horizontal descender.
    HorizontalDescender,
    /// `hlgp`: horizontal line gap.
    HorizontalLineGap,
    /// `hcla`: horizontal clipping ascent (`usWinAscent`).
    HorizontalClippingAscent,
    /// `hcld`: horizontal clipping descent (`usWinDescent`).
    HorizontalClippingDescent,
    /// `vasc`: vertical ascender.
    VerticalAscender,
    /// `vdsc`: vertical descender.
    VerticalDescender,
    /// `vlgp`: vertical line gap.
    VerticalLineGap,
    /// `hcrs`: horizontal caret rise.
    HorizontalCaretRise,
    /// `hcrn`: horizontal caret run.
    HorizontalCaretRun,
    /// `hcof`: horizontal caret offset.
    HorizontalCaretOffset,
    /// `vcrs`: vertical caret rise.
    VerticalCaretRise,
    /// `vcrn`: vertical caret run.
    VerticalCaretRun,
    /// `vcof`: vertical caret offset.
    VerticalCaretOffset,
    /// `xhgt`: x-height.
    XHeight,
    /// `cpht`: cap height.
    CapHeight,
    /// `sbxs`: subscript em x size.
    SubscriptEmXSize,
    /// `sbys`: subscript em y size.
    SubscriptEmYSize,
    /// `sbxo`: subscript em x offset.
    SubscriptEmXOffset,
    /// `sbyo`: subscript em y offset.
    SubscriptEmYOffset,
    /// `spxs`: superscript em x size.
    SuperscriptEmXSize,
    /// `spys`: superscript em y size.
    SuperscriptEmYSize,
    /// `spxo`: superscript em x offset.
    SuperscriptEmXOffset,
    /// `spyo`: superscript em y offset.
    SuperscriptEmYOffset,
    /// `strs`: strikeout size.
    StrikeoutSize,
    /// `stro`: strikeout offset.
    StrikeoutOffset,
    /// `unds`: underline size.
    UnderlineSize,
    /// `undo`: underline offset.
    UnderlineOffset,
}

impl MetricsTag {
    /// Every tag, in the order of `hb_ot_metrics_tag_t`'s documentation.
    pub const ALL: [MetricsTag; 28] = [
        MetricsTag::HorizontalAscender,
        MetricsTag::HorizontalDescender,
        MetricsTag::HorizontalLineGap,
        MetricsTag::HorizontalClippingAscent,
        MetricsTag::HorizontalClippingDescent,
        MetricsTag::VerticalAscender,
        MetricsTag::VerticalDescender,
        MetricsTag::VerticalLineGap,
        MetricsTag::HorizontalCaretRise,
        MetricsTag::HorizontalCaretRun,
        MetricsTag::HorizontalCaretOffset,
        MetricsTag::VerticalCaretRise,
        MetricsTag::VerticalCaretRun,
        MetricsTag::VerticalCaretOffset,
        MetricsTag::XHeight,
        MetricsTag::CapHeight,
        MetricsTag::SubscriptEmXSize,
        MetricsTag::SubscriptEmYSize,
        MetricsTag::SubscriptEmXOffset,
        MetricsTag::SubscriptEmYOffset,
        MetricsTag::SuperscriptEmXSize,
        MetricsTag::SuperscriptEmYSize,
        MetricsTag::SuperscriptEmXOffset,
        MetricsTag::SuperscriptEmYOffset,
        MetricsTag::StrikeoutSize,
        MetricsTag::StrikeoutOffset,
        MetricsTag::UnderlineSize,
        MetricsTag::UnderlineOffset,
    ];

    /// The four-letter tag of `hb_ot_metrics_tag_t` (`hasc`, `xhgt`, …).
    pub fn tag(self) -> sofel_read::Tag {
        use MetricsTag::*;
        sofel_read::Tag::new(match self {
            HorizontalAscender => b"hasc",
            HorizontalDescender => b"hdsc",
            HorizontalLineGap => b"hlgp",
            HorizontalClippingAscent => b"hcla",
            HorizontalClippingDescent => b"hcld",
            VerticalAscender => b"vasc",
            VerticalDescender => b"vdsc",
            VerticalLineGap => b"vlgp",
            HorizontalCaretRise => b"hcrs",
            HorizontalCaretRun => b"hcrn",
            HorizontalCaretOffset => b"hcof",
            VerticalCaretRise => b"vcrs",
            VerticalCaretRun => b"vcrn",
            VerticalCaretOffset => b"vcof",
            XHeight => b"xhgt",
            CapHeight => b"cpht",
            SubscriptEmXSize => b"sbxs",
            SubscriptEmYSize => b"sbys",
            SubscriptEmXOffset => b"sbxo",
            SubscriptEmYOffset => b"sbyo",
            SuperscriptEmXSize => b"spxs",
            SuperscriptEmYSize => b"spys",
            SuperscriptEmXOffset => b"spxo",
            SuperscriptEmYOffset => b"spyo",
            StrikeoutSize => b"strs",
            StrikeoutOffset => b"stro",
            UnderlineSize => b"unds",
            UnderlineOffset => b"undo",
        })
    }

    /// The metric with a four-letter tag; `None` for unknown tags.
    pub fn from_tag(tag: sofel_read::Tag) -> Option<MetricsTag> {
        MetricsTag::ALL.into_iter().find(|t| t.tag() == tag)
    }
}

/// HarfBuzz's scale of a font (`hb_font_t`'s `x_scale` and `y_scale`):
/// converts font units to the caller's units with HarfBuzz's own
/// arithmetic, so that scaled values match HarfBuzz bit for bit.
///
/// Integer values are scaled with `em_mult`
/// (`(v × mult + 32768) >> 16`), values HarfBuzz keeps as floats with
/// `em_multf` (`roundf(v × multf)`), and glyph boxes with
/// `scale_glyph_extents`. The queries take the value HarfBuzz scales
/// with each of them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct HbScale {
    x_scale: i32,
    y_scale: i32,
    x_mult: i64,
    y_mult: i64,
    x_multf: f32,
    y_multf: f32,
}

/// `(hb_position_t) v` for an `int64_t`: the low 32 bits.
fn wrap_i32(v: i64) -> i32 {
    let b = v.to_le_bytes();
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// `hb_clamp_to<hb_position_t> (int64_t)`.
fn clamp_i32(v: i64) -> i32 {
    i32::try_from(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0)
}

/// HarfBuzz's `_fix_ascender_descender`: ascenders positive, descenders
/// negative.
fn fix_sign(v: f32, tag: MetricsTag) -> f32 {
    match tag {
        MetricsTag::HorizontalAscender | MetricsTag::VerticalAscender => v.abs(),
        MetricsTag::HorizontalDescender | MetricsTag::VerticalDescender => -v.abs(),
        _ => v,
    }
}

impl HbScale {
    /// The scale `x_scale` × `y_scale` for a font with `units_per_em`
    /// (HarfBuzz's `hb_font_t::changed`; pass
    /// [`Font::hb_units_per_em`]). `x_multf` is `x_scale / upem` and
    /// `x_mult` is `x_scale × 0x10000 / upem` computed in `f32` and
    /// truncated toward zero, as HarfBuzz does.
    pub fn new(x_scale: i32, y_scale: i32, units_per_em: u16) -> HbScale {
        let upem = f32::from(units_per_em);
        let mult = |s: i32| f32_to_i64(i64_to_f32(i64::from(s).saturating_mul(0x10000)) / upem);
        HbScale {
            x_scale,
            y_scale,
            x_mult: mult(x_scale),
            y_mult: mult(y_scale),
            x_multf: i32_to_f32(x_scale) / upem,
            y_multf: i32_to_f32(y_scale) / upem,
        }
    }

    /// The scale `x_scale` × `y_scale` for `font`.
    pub fn for_font(font: &Font<'_>, x_scale: i32, y_scale: i32) -> HbScale {
        HbScale::new(x_scale, y_scale, font.hb_units_per_em())
    }

    /// HarfBuzz's default scale, which leaves font units as they are:
    /// both scales `unitsPerEm` (as [`Font::hb_units_per_em`]).
    pub fn unscaled(font: &Font<'_>) -> HbScale {
        let upem = i32::from(font.hb_units_per_em());
        HbScale::for_font(font, upem, upem)
    }

    /// `x_scale`.
    pub fn x_scale(&self) -> i32 {
        self.x_scale
    }
    /// `y_scale`.
    pub fn y_scale(&self) -> i32 {
        self.y_scale
    }
    /// `x_mult` of HarfBuzz's `hb_font_t`: `(int64_t) x_scale × 0x10000 / upem`,
    /// where `upem` is a `float`, so the division is done in `float` and
    /// truncated toward zero (ADR 0026).
    pub fn x_mult(&self) -> i64 {
        self.x_mult
    }
    /// `y_mult`, as [`HbScale::x_mult`].
    pub fn y_mult(&self) -> i64 {
        self.y_mult
    }

    /// `em_scale_x`: `(v × x_mult + 32768) >> 16`.
    pub fn em_scale_x(&self, v: i32) -> i32 {
        em_mult(v, self.x_mult)
    }
    /// `em_scale_y`: `(v × y_mult + 32768) >> 16`.
    pub fn em_scale_y(&self, v: i32) -> i32 {
        em_mult(v, self.y_mult)
    }
    /// `em_scalef_x`: `roundf(v × x_multf)`, with HarfBuzz's `roundf`
    /// ([`hb_roundf32`]: `floor((double) x + 0.5)`, not C's `roundf`).
    pub fn em_scalef_x(&self, v: f32) -> i32 {
        hb_clamp_to_i32(hb_roundf32(v * self.x_multf))
    }
    /// `em_scalef_y`: `roundf(v × y_multf)`.
    pub fn em_scalef_y(&self, v: f32) -> i32 {
        hb_clamp_to_i32(hb_roundf32(v * self.y_multf))
    }

    /// `scale_glyph_extents`: the corners scaled in `f32`, the top-left
    /// rounded down and the bottom-right up.
    pub fn scale_glyph_extents(&self, e: GlyphExtents) -> GlyphExtents {
        let x1 = i32_to_f32(e.x_bearing) * self.x_multf;
        let y1 = i32_to_f32(e.y_bearing) * self.y_multf;
        let x2 = x1 + i32_to_f32(e.width) * self.x_multf;
        let y2 = y1 + i32_to_f32(e.height) * self.y_multf;
        let x_bearing = hb_clamp_to_i32(f64::from(floorf(x1)));
        let y_bearing = hb_clamp_to_i32(f64::from(floorf(y1)));
        GlyphExtents {
            x_bearing,
            y_bearing,
            width: hb_clamp_to_i32(f64::from(ceilf(x2)) - f64::from(x_bearing)),
            height: hb_clamp_to_i32(f64::from(ceilf(y2)) - f64::from(y_bearing)),
        }
    }

    /// [`crate::var::Instance::h_extents`] at this scale; the fallback is computed from
    /// `y_scale` (`get_h_extents_with_fallback`).
    pub fn h_extents(&self, font: &Font<'_>) -> FontExtents {
        let get = |t| self.common(font, t);
        match (
            get(MetricsTag::HorizontalAscender),
            get(MetricsTag::HorizontalDescender),
            get(MetricsTag::HorizontalLineGap),
        ) {
            (Some(ascender), Some(descender), Some(line_gap)) => FontExtents {
                ascender,
                descender,
                line_gap,
            },
            _ => {
                // y_scale * .8 in double, truncated: exactly 4y/5 toward 0.
                let ascender = wrap_i32(i64::from(self.y_scale).saturating_mul(4) / 5);
                FontExtents {
                    ascender,
                    descender: ascender.wrapping_sub(self.y_scale),
                    line_gap: 0,
                }
            }
        }
    }

    /// [`crate::var::Instance::v_extents`] at this scale; the fallback is computed from
    /// `x_scale` (`get_v_extents_with_fallback`).
    pub fn v_extents(&self, font: &Font<'_>) -> FontExtents {
        let get = |t| self.common(font, t);
        match (
            get(MetricsTag::VerticalAscender),
            get(MetricsTag::VerticalDescender),
            get(MetricsTag::VerticalLineGap),
        ) {
            (Some(ascender), Some(descender), Some(line_gap)) => FontExtents {
                ascender,
                descender,
                line_gap,
            },
            _ => {
                let ascender = self.x_scale / 2;
                FontExtents {
                    ascender,
                    descender: ascender.wrapping_sub(self.x_scale),
                    line_gap: 0,
                }
            }
        }
    }

    /// The horizontal advance at this scale (`hb_font_get_glyph_h_advance`).
    pub fn h_advance(&self, font: &Font<'_>, glyph: GlyphId) -> i32 {
        #[allow(clippy::cast_possible_wrap)] // hb_position_t receives HarfBuzz's unsigned advance.
        let advance = font.default_instance().advance(glyph) as i32;
        self.em_scale_x(advance)
    }

    /// [`crate::var::Instance::v_advance`] at this scale.
    pub fn v_advance(&self, font: &Font<'_>, glyph: GlyphId) -> i32 {
        if font.vmetrics.has_data() {
            let adv = font.vmetrics.advance(glyph, font.hb_units_per_em());
            return self.em_scale_y(0i32.wrapping_sub(i32::from(adv)));
        }
        let e = self.h_extents(font);
        e.descender.wrapping_sub(e.ascender)
    }

    /// [`crate::var::Instance::v_origin`] at this scale; `extents` gives boxes in font
    /// units, which are scaled with [`HbScale::scale_glyph_extents`].
    pub fn v_origin(
        &self,
        font: &Font<'_>,
        glyph: GlyphId,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> (i32, i32) {
        self.v_origin_scaled(font, glyph, |g| {
            extents(g).map(|e| self.scale_glyph_extents(e))
        })
    }

    /// [`HbScale::v_origin`] with boxes that `extents` gives at this
    /// scale already, as HarfBuzz's `hb_font_get_glyph_extents` does for
    /// color glyphs (ADR 0023).
    pub fn v_origin_scaled(
        &self,
        font: &Font<'_>,
        glyph: GlyphId,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> (i32, i32) {
        let x = self.h_advance(font, glyph) / 2;
        if let Some(vorg) = font.vorg {
            return (x, self.em_scalef_y(i32_to_f32(vorg_y(&vorg, glyph))));
        }
        if font.vmetrics.has_data()
            && let Some(glyf) = font.glyf
        {
            let top = if glyph.0 >= glyf.num_glyphs() {
                0
            } else {
                let tsb = |g: GlyphId| font.vmetrics.bearing(g);
                glyf.phantom_top(glyph, &tsb)
                    .unwrap_or(i32::from(font.hb_units_per_em()))
            };
            return (x, self.em_scalef_y(i32_to_f32(top)));
        }
        let h = self.h_extents(font);
        let advance = i64::from(h.ascender).saturating_sub(i64::from(h.descender));
        let y = match extents(glyph) {
            Some(e) => clamp_i32(
                i64::from(e.y_bearing)
                    .saturating_add(advance.saturating_add(i64::from(e.height)) >> 1),
            ),
            None => h.ascender,
        };
        (x, y)
    }

    /// [`crate::var::Instance::glyf_extents`] at this scale.
    pub fn glyf_extents(&self, font: &Font<'_>, glyph: GlyphId) -> Option<GlyphExtents> {
        let glyf = font.glyf?;
        if glyph.0 >= glyf.num_glyphs() {
            return None;
        }
        let g = glyf.glyph(glyph);
        if g.is_empty() {
            return Some(GlyphExtents::default());
        }
        let (x_min, x_max) = (i32::from(g.x_min), i32::from(g.x_max));
        let (y_min, y_max) = (i32::from(g.y_min), i32::from(g.y_max));
        let e = GlyphExtents {
            x_bearing: i32::from(font.metrics.bearing(glyph)),
            y_bearing: y_min.max(y_max),
            width: x_min.max(x_max).wrapping_sub(x_min.min(x_max)),
            height: y_min.min(y_max).wrapping_sub(y_min.max(y_max)),
        };
        Some(self.scale_glyph_extents(e))
    }

    /// The four values `_hb_ot_metrics_get_position_common` handles.
    fn common(&self, font: &Font<'_>, tag: MetricsTag) -> Option<i32> {
        self.common_with_var(font, tag, 0.0)
    }

    pub(crate) fn common_with_var(
        &self,
        font: &Font<'_>,
        tag: MetricsTag,
        delta: f32,
    ) -> Option<i32> {
        use MetricsTag::*;
        let os2 = hb_os2(font);
        let typo = os2.filter(|o| o.fs_selection() & os2::USE_TYPO_METRICS != 0);
        let y = |v: i16| self.em_scalef_y(fix_sign(f32::from(v) + delta, tag));
        let x = |v: i16| self.em_scalef_x(fix_sign(f32::from(v) + delta, tag));
        match tag {
            HorizontalAscender => typo
                .map(|o| y(o.s_typo_ascender()))
                .or_else(|| font.hhea.map(|h| y(h.ascender()))),
            HorizontalDescender => typo
                .map(|o| y(o.s_typo_descender()))
                .or_else(|| font.hhea.map(|h| y(h.descender()))),
            HorizontalLineGap => typo
                .map(|o| y(o.s_typo_line_gap()))
                .or_else(|| font.hhea.map(|h| y(h.line_gap()))),
            VerticalAscender => font.vhea.map(|h| x(h.ascender())),
            VerticalDescender => font.vhea.map(|h| x(h.descender())),
            VerticalLineGap => font.vhea.map(|h| x(h.line_gap())),
            _ => None,
        }
    }

    /// [`crate::var::Instance::metrics_position`] at this scale.
    pub fn metrics_position(&self, font: &Font<'_>, tag: MetricsTag) -> Option<i32> {
        self.metrics_position_with_var(font, tag, 0.0)
    }

    pub(crate) fn metrics_position_with_var(
        &self,
        font: &Font<'_>,
        tag: MetricsTag,
        delta: f32,
    ) -> Option<i32> {
        use MetricsTag::*;
        let os2 = hb_os2(font);
        let x = |v: i16| self.em_scalef_x(f32::from(v) + delta);
        let y = |v: i16| self.em_scalef_y(f32::from(v) + delta);
        let yu = |v: u16| self.em_scalef_y(f32::from(v) + delta);
        // OS2V2Tail::has_data: sxHeight or sCapHeight is non-zero.
        let v2 =
            os2.filter(|o| o.sx_height().unwrap_or(0) != 0 || o.s_cap_height().unwrap_or(0) != 0);
        let post = font.post;
        match tag {
            HorizontalAscender | HorizontalDescender | HorizontalLineGap | VerticalAscender
            | VerticalDescender | VerticalLineGap => self.common_with_var(font, tag, delta),
            HorizontalClippingAscent => os2.map(|o| yu(o.us_win_ascent())),
            HorizontalClippingDescent => os2.map(|o| yu(o.us_win_descent())),
            HorizontalCaretRise => font.hhea.map(|h| y(h.caret_slope_rise())),
            HorizontalCaretRun => font.hhea.map(|h| x(h.caret_slope_run())),
            HorizontalCaretOffset => font.hhea.map(|h| x(h.caret_offset())),
            VerticalCaretRise => font.vhea.map(|h| x(h.caret_slope_rise())),
            VerticalCaretRun => font.vhea.map(|h| y(h.caret_slope_run())),
            VerticalCaretOffset => font.vhea.map(|h| y(h.caret_offset())),
            XHeight => v2.map(|o| y(o.sx_height().unwrap_or(0))),
            CapHeight => v2.map(|o| y(o.s_cap_height().unwrap_or(0))),
            SubscriptEmXSize => os2.map(|o| x(o.y_subscript_x_size())),
            SubscriptEmYSize => os2.map(|o| y(o.y_subscript_y_size())),
            SubscriptEmXOffset => os2.map(|o| x(o.y_subscript_x_offset())),
            SubscriptEmYOffset => os2.map(|o| y(o.y_subscript_y_offset())),
            SuperscriptEmXSize => os2.map(|o| x(o.y_superscript_x_size())),
            SuperscriptEmYSize => os2.map(|o| y(o.y_superscript_y_size())),
            SuperscriptEmXOffset => os2.map(|o| x(o.y_superscript_x_offset())),
            SuperscriptEmYOffset => os2.map(|o| y(o.y_superscript_y_offset())),
            StrikeoutSize => os2.map(|o| y(o.y_strikeout_size())),
            StrikeoutOffset => os2.map(|o| y(o.y_strikeout_position())),
            UnderlineSize => post.map(|p| y(p.underline_thickness())),
            UnderlineOffset => post.map(|p| y(p.underline_position())),
        }
    }

    /// [`crate::var::Instance::metrics_position_with_fallback`] at this scale; `extents`
    /// gives boxes in font units.
    pub fn metrics_position_with_fallback(
        &self,
        font: &Font<'_>,
        tag: MetricsTag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        self.with_fallback(font, tag, &|g| {
            extents(g).map(|e| self.scale_glyph_extents(e))
        })
    }

    /// [`HbScale::metrics_position_with_fallback`] with boxes that
    /// `extents` gives at this scale already (ADR 0023).
    pub fn metrics_position_with_fallback_scaled(
        &self,
        font: &Font<'_>,
        tag: MetricsTag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        self.with_fallback(font, tag, &extents)
    }

    /// `extents` gives boxes at this scale.
    fn with_fallback(
        &self,
        font: &Font<'_>,
        tag: MetricsTag,
        extents: &dyn Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        self.with_fallback_using(font, tag, extents, &|tag| self.metrics_position(font, tag))
    }

    pub(crate) fn with_fallback_using(
        &self,
        font: &Font<'_>,
        tag: MetricsTag,
        extents: &dyn Fn(GlyphId) -> Option<GlyphExtents>,
        metric: &dyn Fn(MetricsTag) -> Option<i32>,
    ) -> i32 {
        use MetricsTag::*;
        if let Some(v) = metric(tag)
            && (!matches!(tag, StrikeoutSize | UnderlineSize) || v != 0)
        {
            return v;
        }
        let (xs, ys) = (i64::from(self.x_scale), i64::from(self.y_scale));
        let glyph_box = |c: char| font.nominal_glyph(u32::from(c)).and_then(extents);
        match tag {
            HorizontalAscender | HorizontalClippingAscent => self.h_extents(font).ascender,
            HorizontalDescender | HorizontalClippingDescent => self.h_extents(font).descender,
            HorizontalLineGap => self.h_extents(font).line_gap,
            VerticalAscender => self.v_extents(font).ascender,
            VerticalDescender => self.v_extents(font).descender,
            VerticalLineGap => self.v_extents(font).line_gap,
            HorizontalCaretRise | VerticalCaretRise => 1,
            HorizontalCaretRun | VerticalCaretRun | HorizontalCaretOffset | VerticalCaretOffset => {
                0
            }
            XHeight => glyph_box('x').map_or(self.y_scale / 2, |e| e.y_bearing),
            CapHeight => match glyph_box('O') {
                Some(e) => clamp_i32(
                    i64::from(e.height).saturating_add(i64::from(e.y_bearing).saturating_mul(2)),
                ),
                None => clamp_i32(ys.saturating_mul(2) / 3),
            },
            StrikeoutSize | UnderlineSize => self.y_scale / 18,
            StrikeoutOffset => {
                self.with_fallback_using(font, HorizontalAscender, extents, metric) / 2
            }
            UnderlineOffset => {
                // hb_saturate_neg.
                self.y_scale.checked_neg().unwrap_or(i32::MAX) / 18
            }
            SubscriptEmXSize | SuperscriptEmXSize => clamp_i32(xs.saturating_mul(10) / 12),
            SubscriptEmYSize | SuperscriptEmYSize => clamp_i32(ys.saturating_mul(10) / 12),
            SubscriptEmXOffset | SuperscriptEmXOffset => 0,
            SubscriptEmYOffset | SuperscriptEmYOffset => self.y_scale / 5,
        }
    }
}

/// `em_mult`: `(hb_position_t) ((v * mult + 32768) >> 16)`.
fn em_mult(v: i32, mult: i64) -> i32 {
    wrap_i32(i64::from(v).wrapping_mul(mult).wrapping_add(32768) >> 16)
}

/// The OS/2 table when HarfBuzz uses it: it can be read and has data
/// (`OS2::has_data`: a non-zero weight class, width class, first or last
/// character index).
fn hb_os2<'a>(font: &Font<'a>) -> Option<Os2<'a>> {
    font.os2.filter(|o| {
        o.us_weight_class() != 0
            || o.us_width_class() != 0
            || o.us_first_char_index() != 0
            || o.us_last_char_index() != 0
    })
}

/// `VORG::get_y_origin`.
fn vorg_y(vorg: &Vorg<'_>, glyph: GlyphId) -> i32 {
    i32::from(vorg.y_origin(glyph))
}
