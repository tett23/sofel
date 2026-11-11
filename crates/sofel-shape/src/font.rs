//! The font as shaping queries it: HarfBuzz's `hb_font_t` with the `ot`
//! font functions, at a [`Scale`].
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-font.hh` and `src/hb-ot-font.cc`,
//! MIT; see THIRD_PARTY_NOTICES.md.

use sofel_font::read::{GlyphId, device::Device};
use sofel_font::{
    GlyphExtents,
    var::{HbF32, Instance, StoreKind},
};

use crate::scale::Scale;

/// A font with its scale and a source of glyph boxes in font units.
pub(crate) struct ShapeFont<'f, 'a, 'l, 's> {
    pub(crate) instance: Instance<'f, 'a, 'l, 's>,
    pub(crate) font: &'f sofel_font::Font<'a>,
    pub(crate) scale: Scale,
    x_multf: f32,
    y_multf: f32,
    extents: &'f dyn Fn(GlyphId) -> Option<GlyphExtents>,
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
    ) -> ShapeFont<'f, 'a, 'l, 's> {
        let font = instance.font();
        let upem = f32::from(font.hb_units_per_em());
        ShapeFont {
            instance,
            font,
            scale,
            x_multf: i32_to_f32(scale.scale.x_scale()) / upem,
            y_multf: i32_to_f32(scale.scale.y_scale()) / upem,
            extents,
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
        self.font.nominal_glyph_raw(c).map(|g| g.0)
    }
    /// `has_glyph`.
    pub(crate) fn has_glyph(&self, c: u32) -> bool {
        self.font.nominal_glyph_raw(c).is_some()
    }
    /// `get_variation_glyph`, as [`Self::nominal_glyph`].
    pub(crate) fn variation_glyph(&self, c: u32, selector: u32) -> Option<u32> {
        self.font.variation_glyph_raw(c, selector).map(|g| g.0)
    }
    /// `get_glyph_h_advance`.
    pub(crate) fn h_advance(&self, glyph: u32) -> i32 {
        self.scale
            .scale
            .em_scale_x(self.instance.advance(GlyphId(glyph)) as i32)
    }
    /// `get_glyph_v_advance`.
    pub(crate) fn v_advance(&self, glyph: u32) -> i32 {
        self.instance
            .scaled_v_advance(&self.scale.scale, GlyphId(glyph))
    }
    /// `get_glyph_v_origin`.
    pub(crate) fn v_origin(&self, glyph: u32) -> (i32, i32) {
        self.instance
            .scaled_v_origin(&self.scale.scale, GlyphId(glyph), |g| {
                (self.extents)(g).map(|e| self.scale.scale.scale_glyph_extents(e))
            })
    }
    /// `get_glyph_extents`, scaled.
    pub(crate) fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        (self.extents)(GlyphId(glyph)).map(|e| self.scale.scale.scale_glyph_extents(e))
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
