//! CFF and CFF2 glyphs as FreeType's `cff_slot_load` loads them without
//! hinting (ADR 0021): the charstring decoded by FreeType's Adobe engine
//! (`sofel_read::cff::charstring`), the points built as `ps_builder_*`
//! builds them (`src/psaux/psobjs.c`, `src/psaux/psft.c`), then the
//! FontMatrix, its translation and the scale.
//!
//! Ported from FreeType 2.15.0. Copyright 2007-2014 Adobe Systems
//! Incorporated, and Copyright (C) 1996-2026 by David Turner, Robert
//! Wilhelm, and Werner Lemberg. Used under the FreeType License (FTL)
//! with the Adobe patent license; see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::read::GlyphId;
use sofel_font::read::cff::charstring::{self, Context, PathSink};
use sofel_font::read::cff::dict::BlendWeights;
use sofel_font::read::cff::ft;
use sofel_font::var::Location;
use sofel_font::{Cff, Font};
use sofel_geom::Point;

use crate::{CBox, OutlineError, OutlineInfo, Scale, Scratch, TAG_CUBIC, TAG_ON, control_box};

/// FreeType's `PS_Builder` over the scratch's point arrays. `ends`
/// holds an entry for the open contour too, as FreeType's `contours`.
struct Builder<'s> {
    points: &'s mut Vec<Point>,
    tags: &'s mut Vec<u8>,
    ends: &'s mut Vec<usize>,
    path_begun: bool,
}

fn point(p: (i32, i32)) -> Point {
    Point {
        x: p.0 >> 10,
        y: p.1 >> 10,
    }
}

impl Builder<'_> {
    fn add_point(&mut self, p: (i32, i32), on: bool) {
        self.points.push(point(p));
        self.tags.push(if on { TAG_ON } else { TAG_CUBIC });
    }

    /// `ps_builder_add_contour`.
    fn add_contour(&mut self) {
        let n = self.points.len();
        if let Some(last) = self.ends.last_mut() {
            *last = n.wrapping_sub(1);
        }
        self.ends.push(n.wrapping_sub(1));
    }

    /// `ps_builder_start_point`.
    fn start_point(&mut self, p: (i32, i32)) {
        if !self.path_begun {
            self.path_begun = true;
            self.add_contour();
            self.add_point(p, true);
        }
    }

    /// `ps_builder_close_contour`.
    fn close_contour(&mut self) {
        let n_contours = self.ends.len();
        let first = if n_contours <= 1 {
            0
        } else {
            self.ends
                .get(n_contours.saturating_sub(2))
                .map_or(0, |e| e.wrapping_add(1))
        };
        if n_contours > 0 && first == self.points.len() {
            self.ends.pop();
            return;
        }
        let n = self.points.len();
        if n > 1 {
            let (p1, p2) = (self.points.get(first), self.points.last());
            if p1.is_some() && p1 == p2 && self.tags.last() == Some(&TAG_ON) {
                self.points.pop();
                self.tags.pop();
            }
        }
        if n_contours > 0 {
            let n = self.points.len();
            if first == n.wrapping_sub(1) {
                self.ends.pop();
                self.points.pop();
                self.tags.pop();
            } else if let Some(last) = self.ends.last_mut() {
                *last = n.wrapping_sub(1);
            }
        }
    }
}

impl PathSink for Builder<'_> {
    fn move_to(&mut self) {
        self.close_contour();
        self.path_begun = false;
    }

    fn line_to(&mut self, from: (i32, i32), to: (i32, i32)) {
        self.start_point(from);
        self.add_point(to, true);
    }

    fn cubic_to(&mut self, from: (i32, i32), c1: (i32, i32), c2: (i32, i32), to: (i32, i32)) {
        self.start_point(from);
        self.add_point(c1, false);
        self.add_point(c2, false);
        self.add_point(to, true);
    }
}

fn mul_fix(a: i32, b: i64) -> i32 {
    ft::to_i32(ft::mul_fix(i64::from(a), b))
}

/// `cff_slot_load` with the Private DICT and charstring blend at `location`.
pub(crate) fn load_cff_at(
    font: &Font<'_>,
    location: &Location<'_>,
    cff: &Cff<'_>,
    glyph: GlyphId,
    scale: Scale,
    s: &mut Scratch,
) -> Result<OutlineInfo, OutlineError> {
    let scaled = matches!(scale, Scale::Scaled { .. });
    // cf2_checkTransform: FreeType rejects scaling with unitsPerEm above
    // 0x7FFF (Glyph_Too_Big, and the retry fails the same way).
    if scaled && font.units_per_em() > 0x7FFF {
        return Err(OutlineError::InvalidOutline);
    }
    let instance = font.instance(location);
    let blend = instance.ft_blend_weights(cff);
    load_cff_with_blend(cff, glyph, scale, &blend, s, false)
}

/// Decode a CFF 1 charstring at Adobe's hinted path scale. The caller checks
/// that the charstring has no stems before selecting this result.
pub(crate) fn load_cff_native_without_stems(
    font: &Font<'_>,
    location: &Location<'_>,
    cff: &Cff<'_>,
    glyph: GlyphId,
    scale: Scale,
    s: &mut Scratch,
) -> Result<OutlineInfo, OutlineError> {
    if font.units_per_em() > 0x7FFF {
        return Err(OutlineError::InvalidOutline);
    }
    let instance = font.instance(location);
    let blend = instance.ft_blend_weights(cff);
    load_cff_with_blend(cff, glyph, scale, &blend, s, true)
}

pub(crate) fn load_bare_cff(
    cff: &Cff<'_>,
    glyph: GlyphId,
    scale: Scale,
    s: &mut Scratch,
) -> Result<OutlineInfo, OutlineError> {
    if matches!(scale, Scale::Scaled { .. }) && cff.head_units_per_em() > 0x7FFF {
        return Err(OutlineError::InvalidOutline);
    }
    load_cff_with_blend(cff, glyph, scale, &cff.default_blend(), s, false)
}

fn load_cff_with_blend(
    cff: &Cff<'_>,
    glyph: GlyphId,
    scale: Scale,
    blend: &dyn BlendWeights,
    s: &mut Scratch,
    native_without_stems: bool,
) -> Result<OutlineInfo, OutlineError> {
    s.clear();
    if glyph.0 >= cff.num_glyphs() {
        return Err(OutlineError::InvalidGlyphIndex);
    }
    let scaled = matches!(scale, Scale::Scaled { .. });
    let sub = cff
        .subfont_with_blend(glyph, blend)
        .ok_or(OutlineError::InvalidCharString)?;
    let data = cff
        .char_strings()
        .ft_element(glyph.0)
        .ok_or(OutlineError::InvalidGlyphIndex)?;
    let (global_subrs, global_subrs_sorted) = cff.global_subrs();
    let top = cff.top_dict();
    let p = &sub.private;
    let ctx = Context {
        version: cff.version(),
        char_strings: cff.char_strings(),
        charset: cff.charset(),
        global_subrs,
        global_subrs_sorted,
        local_subrs: sub.local_subrs,
        local_subrs_sorted: cff.local_subrs_sorted(sub.fd),
        charstring_type: top.charstring_type,
        default_width_x: p.default_width_x,
        nominal_width_x: p.nominal_width_x,
        max_stack: top.maxstack,
        has_variations: cff.variation_store().is_some_and(|v| v.data_count() != 0),
        vsindex: p.vsindex,
        blend,
        // The face's seed is 0 (ft-dump sets FT_PARAM_TAG_RANDOM_SEED), so
        // the state starts from the Private DICT's initialRandomSeed; it
        // restarts at every draw (ADR 0021).
        random_seed: ft::low32(p.initial_random_seed.cast_unsigned()),
        scale: if native_without_stems {
            let Scale::Scaled { y, .. } = scale else {
                return Err(OutlineError::InvalidOutline);
            };
            crate::ps_native::path_scale(y)
        } else {
            0x400
        },
    };
    let mut builder = Builder {
        points: &mut s.points,
        tags: &mut s.tags,
        ends: &mut s.ends,
        path_begun: false,
    };
    if charstring::decode(data, &ctx, &mut s.cff, &mut builder).is_err() {
        s.clear();
        return Err(OutlineError::InvalidCharString);
    }
    builder.close_contour();
    let m = sub.matrix;
    if !m.is_identity() {
        let [xx, yx, xy, yy] = m.m;
        for pt in &mut s.points {
            let (x, y) = (pt.x, pt.y);
            *pt = Point {
                x: mul_fix(x, xx).wrapping_add(mul_fix(y, xy)),
                y: mul_fix(x, yx).wrapping_add(mul_fix(y, yy)),
            };
        }
    }
    let (ox, oy) = (ft::to_i32(m.offset.0), ft::to_i32(m.offset.1));
    if ox != 0 || oy != 0 {
        for pt in &mut s.points {
            pt.x = pt.x.wrapping_add(ox);
            pt.y = pt.y.wrapping_add(oy);
        }
    }
    let (mut x_scale, mut y_scale) = match scale {
        Scale::Unscaled => (0x1_0000_i64, 0x1_0000_i64),
        Scale::Scaled { x, y } => (i64::from(x.0), i64::from(y.0)),
    };
    let mut force = false;
    if cff.num_font_dicts() > 0 {
        let top_upm = cff.top_matrix().units_per_em.cast_signed();
        let sub_upm = m.units_per_em.cast_signed();
        if top_upm != sub_upm {
            x_scale = ft::mul_div(x_scale, top_upm, sub_upm);
            y_scale = ft::mul_div(y_scale, top_upm, sub_upm);
            force = true;
        }
    }
    if (scaled || force) && !native_without_stems {
        for pt in &mut s.points {
            pt.x = mul_fix(pt.x, x_scale);
            pt.y = mul_fix(pt.y, y_scale);
        }
    }
    Ok(OutlineInfo {
        overlap: false,
        cbox: if s.points.is_empty() {
            CBox::default()
        } else {
            control_box(&s.points)
        },
    })
}
