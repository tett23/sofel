//! The font as shaping queries it: HarfBuzz's `hb_font_t` with the `ot`
//! font functions, at a [`Scale`].
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-font.hh` and `src/hb-ot-font.cc`,
//! MIT; see THIRD_PARTY_NOTICES.md.

use alloc::boxed::Box;
use core::cell::RefCell;

use sofel_font::read::{GlyphId, device::Device};
use sofel_font::{
    GlyphExtents, GlyphScratch,
    var::{AdvanceCache, HbF32, Instance, StoreKind},
};

use crate::scale::Scale;
use sofel_geom::float::{ceilf, floorf, hb_clamp_to_i32};

use crate::outline::GlyphOutline;

type VariationGlyphFn<'a> = dyn Fn(u32, u32) -> Option<u32> + 'a;
type OriginFn<'a> = dyn Fn(u32) -> Option<(i32, i32)> + 'a;

/// Per-query font functions and scale, with an optional parent font.
///
/// A child inherits the parent's settings at creation. An unset function
/// delegates to the parent; a set function supplies a value in this font's
/// scale, as with HarfBuzz's `hb_font_create_sub_font`.
pub struct SubFont<'p> {
    parent: Option<&'p SubFont<'p>>,
    scale: Scale,
    nominal_glyph: Option<Box<dyn Fn(u32) -> Option<u32> + 'p>>,
    variation_glyph: Option<Box<VariationGlyphFn<'p>>>,
    h_advance: Option<Box<dyn Fn(u32) -> i32 + 'p>>,
    v_advance: Option<Box<dyn Fn(u32) -> i32 + 'p>>,
    h_origin: Option<Box<OriginFn<'p>>>,
    v_origin: Option<Box<OriginFn<'p>>>,
    glyph_extents: Option<Box<dyn Fn(u32) -> Option<GlyphExtents> + 'p>>,
    outline: Option<Box<dyn Fn(u32) -> Option<GlyphOutline> + 'p>>,
    x_embolden: f32,
    y_embolden: f32,
    embolden_in_place: bool,
    slant: f32,
}

impl<'p> SubFont<'p> {
    /// A root font using the built-in OpenType functions for unset queries.
    pub fn new(scale: Scale) -> Self {
        Self {
            parent: None,
            scale,
            nominal_glyph: None,
            variation_glyph: None,
            h_advance: None,
            v_advance: None,
            h_origin: None,
            v_origin: None,
            glyph_extents: None,
            outline: None,
            x_embolden: 0.0,
            y_embolden: 0.0,
            embolden_in_place: false,
            slant: 0.0,
        }
    }

    /// Create a child, copying the scale and synthetic settings.
    pub fn child(&'p self) -> Self {
        Self {
            parent: Some(self),
            scale: self.scale,
            nominal_glyph: None,
            variation_glyph: None,
            h_advance: None,
            v_advance: None,
            h_origin: None,
            v_origin: None,
            glyph_extents: None,
            outline: None,
            x_embolden: self.x_embolden,
            y_embolden: self.y_embolden,
            embolden_in_place: self.embolden_in_place,
            slant: self.slant,
        }
    }

    /// Set the scale and pixels per em used by this font.
    pub fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
    }

    /// The current scale and pixels per em.
    pub fn scale(&self) -> Scale {
        self.scale
    }

    /// Override the nominal Unicode-to-glyph lookup.
    pub fn set_nominal_glyph_func(&mut self, func: impl Fn(u32) -> Option<u32> + 'p) {
        self.nominal_glyph = Some(Box::new(func));
    }

    /// Override the variation-selector glyph lookup.
    pub fn set_variation_glyph_func(&mut self, func: impl Fn(u32, u32) -> Option<u32> + 'p) {
        self.variation_glyph = Some(Box::new(func));
    }

    /// Override the horizontal advance in this font's scaled units.
    pub fn set_h_advance_func(&mut self, func: impl Fn(u32) -> i32 + 'p) {
        self.h_advance = Some(Box::new(func));
    }

    /// Override the vertical advance in this font's scaled units.
    pub fn set_v_advance_func(&mut self, func: impl Fn(u32) -> i32 + 'p) {
        self.v_advance = Some(Box::new(func));
    }

    /// Override the horizontal origin in this font's scaled units.
    pub fn set_h_origin_func(&mut self, func: impl Fn(u32) -> Option<(i32, i32)> + 'p) {
        self.h_origin = Some(Box::new(func));
    }

    /// Override the vertical origin in this font's scaled units.
    pub fn set_v_origin_func(&mut self, func: impl Fn(u32) -> Option<(i32, i32)> + 'p) {
        self.v_origin = Some(Box::new(func));
    }

    /// Override the glyph box in this font's scaled units.
    pub fn set_glyph_extents_func(&mut self, func: impl Fn(u32) -> Option<GlyphExtents> + 'p) {
        self.glyph_extents = Some(Box::new(func));
    }

    /// Override the glyph outline in this font's scaled units.
    pub fn set_outline_func(&mut self, func: impl Fn(u32) -> Option<GlyphOutline> + 'p) {
        self.outline = Some(Box::new(func));
    }

    /// Draw a glyph through this font's outline function, then apply
    /// HarfBuzz's synthetic slant and bold settings.
    pub fn glyph_outline(&self, glyph: u32) -> Option<GlyphOutline> {
        let mut outline = self.raw_outline(glyph)?;
        let x_scale = self.scale.scale.x_scale();
        let y_scale = self.scale.scale.y_scale();
        let slant_xy = if y_scale == 0 {
            0.0
        } else {
            self.slant * (x_scale as f32) / (y_scale as f32)
        };
        if slant_xy != 0.0 {
            let (xo, yo) = self.raw_h_origin(glyph, &|_, _| None).unwrap_or((0, 0));
            outline.translate(-(xo as f32), -(yo as f32));
            outline.slant(slant_xy);
            outline.translate(xo as f32, yo as f32);
        }
        let x_strength = round_to_i32((x_scale.saturating_abs() as f32) * self.x_embolden);
        let y_strength = round_to_i32((y_scale.saturating_abs() as f32) * self.y_embolden);
        if x_strength != 0 || y_strength != 0 {
            let mut x_shift = if self.embolden_in_place {
                0.0
            } else {
                (x_strength as f32) / 2.0
            };
            let mut y_shift = (y_strength as f32) / 2.0;
            if x_scale < 0 {
                x_shift = -x_shift;
            }
            if y_scale < 0 {
                y_shift = -y_shift;
            }
            outline.embolden(x_strength as f32, y_strength as f32, x_shift, y_shift);
        }
        Some(outline)
    }

    #[allow(clippy::arithmetic_side_effects)] // HarfBuzz scales draw coordinates by f32 ratios.
    fn raw_outline(&self, glyph: u32) -> Option<GlyphOutline> {
        if let Some(func) = &self.outline {
            return func(glyph);
        }
        if let Some(parent) = self.parent {
            let mut outline = parent.raw_outline(glyph)?;
            let x = if parent.scale.scale.x_scale() == 0 {
                0.0
            } else {
                (self.scale.scale.x_scale() as f32) / (parent.scale.scale.x_scale() as f32)
            };
            let y = if parent.scale.scale.y_scale() == 0 {
                0.0
            } else {
                (self.scale.scale.y_scale() as f32) / (parent.scale.scale.y_scale() as f32)
            };
            outline.scale(x, y);
            return Some(outline);
        }
        None
    }

    /// Set HarfBuzz's synthetic bold fractions and in-place setting.
    pub fn set_synthetic_bold(&mut self, x: f32, y: f32, in_place: bool) {
        self.x_embolden = x;
        self.y_embolden = y;
        self.embolden_in_place = in_place;
    }

    /// Set HarfBuzz's synthetic slant ratio.
    pub fn set_synthetic_slant(&mut self, slant: f32) {
        self.slant = slant;
    }

    fn nominal(&self, cp: u32, base: &dyn Fn(u32) -> Option<u32>) -> Option<u32> {
        if let Some(func) = &self.nominal_glyph {
            func(cp)
        } else if let Some(parent) = self.parent {
            parent.nominal(cp, base)
        } else {
            base(cp)
        }
    }

    fn variation(
        &self,
        cp: u32,
        selector: u32,
        base: &dyn Fn(u32, u32) -> Option<u32>,
    ) -> Option<u32> {
        if let Some(func) = &self.variation_glyph {
            func(cp, selector)
        } else if let Some(parent) = self.parent {
            parent.variation(cp, selector, base)
        } else {
            base(cp, selector)
        }
    }

    fn raw_h_advance(&self, glyph: u32, base: &dyn Fn(Scale, u32) -> i32) -> i32 {
        if let Some(func) = &self.h_advance {
            func(glyph)
        } else if let Some(parent) = self.parent {
            let value = parent.raw_h_advance(glyph, base);
            parent_scale_distance(
                value,
                parent.scale.scale.x_scale(),
                self.scale.scale.x_scale(),
            )
        } else {
            base(self.scale, glyph)
        }
    }

    fn raw_v_advance(&self, glyph: u32, base: &dyn Fn(Scale, u32) -> i32) -> i32 {
        if let Some(func) = &self.v_advance {
            func(glyph)
        } else if let Some(parent) = self.parent {
            let value = parent.raw_v_advance(glyph, base);
            parent_scale_distance(
                value,
                parent.scale.scale.y_scale(),
                self.scale.scale.y_scale(),
            )
        } else {
            base(self.scale, glyph)
        }
    }

    fn raw_v_origin(
        &self,
        glyph: u32,
        base: &dyn Fn(Scale, u32) -> Option<(i32, i32)>,
    ) -> Option<(i32, i32)> {
        if let Some(func) = &self.v_origin {
            func(glyph)
        } else if let Some(parent) = self.parent {
            parent.raw_v_origin(glyph, base).map(|(x, y)| {
                (
                    parent_scale_distance(
                        x,
                        parent.scale.scale.x_scale(),
                        self.scale.scale.x_scale(),
                    ),
                    parent_scale_distance(
                        y,
                        parent.scale.scale.y_scale(),
                        self.scale.scale.y_scale(),
                    ),
                )
            })
        } else {
            base(self.scale, glyph)
        }
    }

    /// Query a glyph's horizontal origin with parent delegation and scale conversion.
    pub fn horizontal_origin(&self, glyph: u32) -> Option<(i32, i32)> {
        self.raw_h_origin(glyph, &|_, _| None)
            .map(|origin| self.synthetic_origin(origin))
    }

    fn raw_h_origin(
        &self,
        glyph: u32,
        base: &dyn Fn(Scale, u32) -> Option<(i32, i32)>,
    ) -> Option<(i32, i32)> {
        if let Some(func) = &self.h_origin {
            func(glyph)
        } else if let Some(parent) = self.parent {
            parent.raw_h_origin(glyph, base).map(|(x, y)| {
                (
                    parent_scale_distance(
                        x,
                        parent.scale.scale.x_scale(),
                        self.scale.scale.x_scale(),
                    ),
                    parent_scale_distance(
                        y,
                        parent.scale.scale.y_scale(),
                        self.scale.scale.y_scale(),
                    ),
                )
            })
        } else {
            base(self.scale, glyph)
        }
    }

    fn raw_glyph_extents(
        &self,
        glyph: u32,
        base: &dyn Fn(Scale, u32) -> Option<GlyphExtents>,
    ) -> Option<GlyphExtents> {
        if let Some(func) = &self.glyph_extents {
            func(glyph)
        } else if let Some(parent) = self.parent {
            parent.raw_glyph_extents(glyph, base).map(|e| GlyphExtents {
                x_bearing: parent_scale_distance(
                    e.x_bearing,
                    parent.scale.scale.x_scale(),
                    self.scale.scale.x_scale(),
                ),
                y_bearing: parent_scale_distance(
                    e.y_bearing,
                    parent.scale.scale.y_scale(),
                    self.scale.scale.y_scale(),
                ),
                width: parent_scale_distance(
                    e.width,
                    parent.scale.scale.x_scale(),
                    self.scale.scale.x_scale(),
                ),
                height: parent_scale_distance(
                    e.height,
                    parent.scale.scale.y_scale(),
                    self.scale.scale.y_scale(),
                ),
            })
        } else {
            base(self.scale, glyph)
        }
    }

    #[allow(clippy::arithmetic_side_effects)] // HarfBuzz negates the signed strength.
    fn synthetic_h_advance(&self, advance: i32) -> i32 {
        if self.embolden_in_place || self.x_embolden == 0.0 || advance == 0 {
            return advance;
        }
        let x_scale = self.scale.scale.x_scale();
        let strength = round_to_i32((x_scale.saturating_abs() as f32) * self.x_embolden);
        advance.wrapping_add(if x_scale < 0 { -strength } else { strength })
    }

    #[allow(clippy::arithmetic_side_effects)] // HarfBuzz negates the signed strength.
    fn synthetic_v_advance(&self, advance: i32) -> i32 {
        if self.embolden_in_place || self.y_embolden == 0.0 || advance == 0 {
            return advance;
        }
        let y_scale = self.scale.scale.y_scale();
        let strength = round_to_i32((y_scale.saturating_abs() as f32) * self.y_embolden);
        advance.wrapping_add(if y_scale < 0 { -strength } else { strength })
    }

    #[allow(clippy::arithmetic_side_effects)] // HarfBuzz applies signed strengths to the two coordinates.
    fn synthetic_origin(&self, (x, y): (i32, i32)) -> (i32, i32) {
        if self.embolden_in_place {
            return (x, y);
        }
        let x_scale = self.scale.scale.x_scale();
        let y_scale = self.scale.scale.y_scale();
        let x_strength = round_to_i32((x_scale.saturating_abs() as f32) * self.x_embolden);
        let y_strength = round_to_i32((y_scale.saturating_abs() as f32) * self.y_embolden);
        (
            x.wrapping_add(if x_scale < 0 { -x_strength } else { x_strength }),
            y.wrapping_add(if y_scale < 0 { -y_strength } else { y_strength }),
        )
    }

    #[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)] // HarfBuzz floorf/ceilf and bounded i32 coordinates.
    fn synthetic_glyph_extents(&self, mut extents: GlyphExtents) -> GlyphExtents {
        let x_scale = self.scale.scale.x_scale();
        let y_scale = self.scale.scale.y_scale();
        let slant_xy = if y_scale == 0 {
            0.0
        } else {
            self.slant * (x_scale as f32) / (y_scale as f32)
        };
        if slant_xy != 0.0 {
            let y1 = i64::from(extents.y_bearing);
            let y2 = y1 + i64::from(extents.height);
            let a = (y1 as f32) * slant_xy;
            let b = (y2 as f32) * slant_xy;
            let x1 = i64::from(extents.x_bearing) + floorf(a.min(b)) as i64;
            let x2 =
                i64::from(extents.x_bearing) + i64::from(extents.width) + ceilf(a.max(b)) as i64;
            extents.x_bearing = hb_clamp_to_i32(x1 as f64);
            extents.width = hb_clamp_to_i32((x2 - x1) as f64);
        }
        let x_strength = round_to_i32((x_scale.saturating_abs() as f32) * self.x_embolden);
        let y_strength = round_to_i32((y_scale.saturating_abs() as f32) * self.y_embolden);
        if x_strength != 0 || y_strength != 0 {
            let y_shift = if y_scale < 0 { -y_strength } else { y_strength };
            extents.y_bearing =
                hb_clamp_to_i32((i64::from(extents.y_bearing) + i64::from(y_shift)) as f64);
            extents.height =
                hb_clamp_to_i32((i64::from(extents.height) - i64::from(y_shift)) as f64);
            let x_shift = if x_scale < 0 { -x_strength } else { x_strength };
            if self.embolden_in_place {
                extents.x_bearing =
                    hb_clamp_to_i32((i64::from(extents.x_bearing) - i64::from(x_shift / 2)) as f64);
            }
            extents.width = hb_clamp_to_i32((i64::from(extents.width) + i64::from(x_shift)) as f64);
        }
        extents
    }
}

/// `hb_font_t::parent_scale_x_distance`: multiply before division and
/// truncate toward zero, except when the parent scale is zero.
#[allow(clippy::arithmetic_side_effects)] // parent != 0 and i32*i32 fits i64.
fn parent_scale_distance(value: i32, parent: i32, child: i32) -> i32 {
    if parent == 0 || parent == child {
        return value;
    }
    let scaled = i64::from(value)
        .wrapping_mul(i64::from(child))
        .wrapping_div(i64::from(parent));
    let b = scaled.to_le_bytes();
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// A font with its scale and a source of glyph boxes in font units.
pub(crate) struct ShapeFont<'f, 'a, 'l, 's> {
    pub(crate) instance: Instance<'f, 'a, 'l, 's>,
    pub(crate) font: &'f sofel_font::Font<'a>,
    pub(crate) scale: Scale,
    x_multf: f32,
    y_multf: f32,
    extents: &'f dyn Fn(GlyphId) -> Option<GlyphExtents>,
    glyph_scratch: RefCell<GlyphScratch>,
    advance_cache: Option<Box<AdvanceCache<'f, 'a, 'l, 's>>>,
    subfont: Option<&'f SubFont<'f>>,
}

pub(crate) use sofel_geom::float::i32_to_f32;

/// HarfBuzz's `roundf` (`floor ((double) v + .5)`) cast to
/// `hb_position_t`, clamped.
pub(crate) fn round_to_i32(v: f32) -> i32 {
    sofel_geom::float::hb_clamp_to_i32(sofel_geom::float::hb_roundf32(v))
}

impl<'f, 'a, 'l, 's> ShapeFont<'f, 'a, 'l, 's> {
    pub(crate) fn new(
        instance: Instance<'f, 'a, 'l, 's>,
        scale: Scale,
        extents: &'f dyn Fn(GlyphId) -> Option<GlyphExtents>,
        subfont: Option<&'f SubFont<'f>>,
    ) -> ShapeFont<'f, 'a, 'l, 's> {
        let font = instance.font();
        let upem = f32::from(font.hb_units_per_em());
        let advance_cache = instance
            .location()
            .hb_nonzero()
            .then(|| Box::new(AdvanceCache::new(instance)));
        ShapeFont {
            instance,
            font,
            scale,
            x_multf: i32_to_f32(scale.scale.x_scale()) / upem,
            y_multf: i32_to_f32(scale.scale.y_scale()) / upem,
            extents,
            glyph_scratch: RefCell::new(GlyphScratch::default()),
            advance_cache,
            subfont,
        }
    }

    /// `x_scale`.
    pub(crate) fn x_scale(&self) -> i32 {
        self.scale.scale.x_scale()
    }
    /// `y_scale`.
    pub(crate) fn y_scale(&self) -> i32 {
        self.scale.scale.y_scale()
    }

    /// `get_nominal_glyph`: glyph ids not below the glyph count
    /// included, as HarfBuzz's `ot` font functions give them.
    pub(crate) fn nominal_glyph(&self, c: u32) -> Option<u32> {
        self.subfont.map_or_else(
            || self.font.nominal_glyph_raw(c).map(|g| g.0),
            |subfont| subfont.nominal(c, &|cp| self.font.nominal_glyph_raw(cp).map(|g| g.0)),
        )
    }
    /// `has_glyph`.
    pub(crate) fn has_glyph(&self, c: u32) -> bool {
        self.nominal_glyph(c).is_some()
    }
    /// `get_variation_glyph`, as [`Self::nominal_glyph`].
    pub(crate) fn variation_glyph(&self, c: u32, selector: u32) -> Option<u32> {
        self.subfont.map_or_else(
            || self.font.variation_glyph_raw(c, selector).map(|g| g.0),
            |subfont| {
                subfont.variation(c, selector, &|cp, vs| {
                    self.font.variation_glyph_raw(cp, vs).map(|g| g.0)
                })
            },
        )
    }
    /// `get_glyph_h_advance`.
    pub(crate) fn h_advance(&self, glyph: u32) -> i32 {
        let base = |scale: Scale, glyph: u32| {
            let advance = self.advance_cache.as_ref().map_or_else(
                || self.instance.advance(GlyphId(glyph)),
                |cache| {
                    cache.advance_with_scratch(GlyphId(glyph), &mut self.glyph_scratch.borrow_mut())
                },
            );
            scale.scale.em_scale_x(advance.cast_signed())
        };
        self.subfont.map_or_else(
            || base(self.scale, glyph),
            |subfont| subfont.synthetic_h_advance(subfont.raw_h_advance(glyph, &base)),
        )
    }
    /// `get_glyph_v_advance`.
    pub(crate) fn v_advance(&self, glyph: u32) -> i32 {
        let base = |scale: Scale, glyph: u32| {
            let mut scratch = self.glyph_scratch.borrow_mut();
            self.instance
                .scaled_v_advance_with_scratch(&scale.scale, GlyphId(glyph), &mut scratch)
        };
        self.subfont.map_or_else(
            || base(self.scale, glyph),
            |subfont| subfont.synthetic_v_advance(subfont.raw_v_advance(glyph, &base)),
        )
    }
    /// `get_glyph_v_origin`.
    pub(crate) fn v_origin(&self, glyph: u32) -> (i32, i32) {
        let base = |scale: Scale, glyph: u32| {
            let mut scratch = self.glyph_scratch.borrow_mut();
            Some(self.instance.scaled_v_origin_with_scratch(
                &scale.scale,
                GlyphId(glyph),
                |g| (self.extents)(g).map(|e| scale.scale.scale_glyph_extents(e)),
                &mut scratch,
            ))
        };
        self.subfont
            .map_or_else(
                || base(self.scale, glyph),
                |subfont| {
                    subfont
                        .raw_v_origin(glyph, &base)
                        .map(|origin| subfont.synthetic_origin(origin))
                },
            )
            .unwrap_or((0, 0))
    }
    /// `get_glyph_extents`, scaled.
    pub(crate) fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        let base = |scale: Scale, glyph: u32| {
            (self.extents)(GlyphId(glyph)).map(|e| scale.scale.scale_glyph_extents(e))
        };
        self.subfont.map_or_else(
            || base(self.scale, glyph),
            |subfont| {
                if (subfont.slant != 0.0 || subfont.x_embolden != 0.0 || subfont.y_embolden != 0.0)
                    && let Some(outline) = subfont.glyph_outline(glyph)
                {
                    Some(outline.extents())
                } else {
                    subfont
                        .raw_glyph_extents(glyph, &base)
                        .map(|e| subfont.synthetic_glyph_extents(e))
                }
            },
        )
    }
    /// VariationIndex uses GDEF ItemVariationStore independently of ppem.
    fn variation_delta(&self, device: Option<&Device<'_>>) -> Option<f32> {
        if !self.instance.location().hb_nonzero() {
            return None;
        }
        let (outer, inner) = device?.variation_index()?;
        Some(
            self.instance
                .item_delta::<HbF32>(StoreKind::Gdef, outer, inner)
                .unwrap_or(0.0),
        )
    }

    pub(crate) fn x_device_delta(&self, device: Option<&Device<'_>>) -> i32 {
        self.variation_delta(device).map_or_else(
            || self.scale.x_delta(device),
            |delta| self.scale.scale.em_scalef_x(delta),
        )
    }

    pub(crate) fn y_device_delta(&self, device: Option<&Device<'_>>) -> i32 {
        self.variation_delta(device).map_or_else(
            || self.scale.y_delta(device),
            |delta| self.scale.scale.em_scalef_y(delta),
        )
    }

    pub(crate) fn x_device_fdelta(&self, device: Option<&Device<'_>>) -> f32 {
        self.variation_delta(device).map_or_else(
            || i32_to_f32(self.scale.x_delta(device)),
            |delta| delta * self.x_multf,
        )
    }

    pub(crate) fn y_device_fdelta(&self, device: Option<&Device<'_>>) -> f32 {
        self.variation_delta(device).map_or_else(
            || i32_to_f32(self.scale.y_delta(device)),
            |delta| delta * self.y_multf,
        )
    }

    /// `em_scale_x`.
    pub(crate) fn em_scale_x(&self, v: i16) -> i32 {
        self.scale.scale.em_scale_x(i32::from(v))
    }
    /// `em_scale_y`.
    pub(crate) fn em_scale_y(&self, v: i16) -> i32 {
        self.scale.scale.em_scale_y(i32::from(v))
    }
    /// `em_fscale_x`.
    pub(crate) fn em_fscale_x(&self, v: i16) -> f32 {
        f32::from(v) * self.x_multf
    }
    /// `em_fscale_y`.
    pub(crate) fn em_fscale_y(&self, v: i16) -> f32 {
        f32::from(v) * self.y_multf
    }
}
