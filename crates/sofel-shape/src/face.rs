//! Common face shaping at 26.6 pixels (root ADR 0100).

use sofel_font::{Face, FaceError, Font, HbScale, read::GlyphId};

use crate::{
    Feature, GlyphBuffer, ResolvedProperties, Scale, ShapePlan, SubFont, UnicodeBuffer,
    shape_with_extents, shape_with_subfont,
};

const EMPTY_SFNT: [u8; 12] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

/// Shapes a common face with positions in signed 26.6 pixels.
pub fn shape_face(
    face: Face<'_>,
    ppem: u16,
    strike: Option<u32>,
    props: ResolvedProperties,
    features: &[Feature],
    buffer: UnicodeBuffer,
) -> Result<GlyphBuffer, FaceError> {
    face.h_advance(GlyphId(0), ppem, strike)?;
    if let Face::Sfnt(font) = face {
        let instance = font.default_instance();
        let size = i32::from(ppem).saturating_mul(64);
        let scale = Scale::new(
            HbScale::for_font(font, size, size),
            u32::from(ppem),
            u32::from(ppem),
        );
        let plan = ShapePlan::new(instance, props, features);
        return Ok(shape_with_extents(
            instance,
            &scale,
            &plan,
            buffer,
            &|glyph| instance.glyf_extents(glyph),
        ));
    }
    let dummy = Font::for_shaping(&EMPTY_SFNT, 0).map_err(|_| FaceError::Invalid)?;
    let instance = dummy.default_instance();
    let size = i32::from(ppem).saturating_mul(64);
    let scale = Scale::new(
        HbScale::for_font(&dummy, size, size),
        u32::from(ppem),
        u32::from(ppem),
    );
    let plan = ShapePlan::new(instance, props, features);
    let mut subfont = SubFont::new(scale);
    subfont.set_nominal_glyph_func(move |cp| face.nominal_glyph(cp).map(|g| g.0));
    subfont.set_h_advance_func(move |g| face.h_advance(GlyphId(g), ppem, strike).unwrap_or(0));
    subfont.set_h_kerning_func(move |l, r| {
        face.kerning(GlyphId(l), GlyphId(r), ppem, strike)
            .map_or(0, |pair| pair.0)
    });
    Ok(shape_with_subfont(instance, &subfont, &plan, buffer))
}
