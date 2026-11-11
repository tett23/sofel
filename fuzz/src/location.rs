use sofel_read::GlyphId;

/// A mode, a coordinate count and big-endian words precede the SFNT.
/// The same instance supplies all glyph queries for this font.
pub fn check_location(input: &[u8]) {
    use sofel_color::{PaintOptions, Recording, paint_recording_at};
    use sofel_font::var::Location;
    let Some((header, rest)) = input.split_first_chunk::<2>() else {
        return;
    };
    let Some((words, data)) = rest.split_at_checked(usize::from(header[1]) * 4) else {
        return;
    };
    let Ok(font) = sofel_font::Font::new(data, 0) else {
        return;
    };
    let values = words
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(i32::from_be_bytes);
    let location = match header[0] % 4 {
        0 => Location::from_design(
            &font,
            &values.map(|v| f32::from_bits(v as u32)).collect::<Vec<_>>(),
        ),
        1 => Location::from_normalized_2_14(&font, &values.collect::<Vec<_>>()),
        2 => Location::from_normalized_16_16(
            &font,
            &values.map(sofel_read::Fixed).collect::<Vec<_>>(),
        ),
        _ => Location::named_instance(&font, values.clone().next().unwrap_or_default() as u16),
    };
    let instance = font.instance(&location);
    let outlines = sofel_outline::Outlines::new(instance);
    let boxes = sofel_outline::HbExtents::new(instance);
    let mut scratch = sofel_outline::Scratch::default();
    let mut paint_scratch = sofel_color::Scratch::default();
    let mut recording = Recording::new();
    for id in 0..font.num_glyphs() {
        let glyph = GlyphId(id);
        let _ = outlines.draw(
            glyph,
            sofel_outline::Scale::Unscaled,
            &mut scratch,
            &mut sofel_geom::FnPen(|_| {}),
        );
        let _ = instance.advance(glyph);
        let _ = instance.v_advance(glyph);
        let _ = instance.v_origin(glyph, |g| boxes.glyph_extents(g));
        let _ = boxes.glyph_extents(glyph);
        recording.clear();
        let _ = paint_recording_at(
            instance,
            id,
            &PaintOptions::default(),
            &mut paint_scratch,
            &mut recording,
        );
    }
}
