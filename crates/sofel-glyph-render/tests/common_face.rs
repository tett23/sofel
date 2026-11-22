#![allow(clippy::unwrap_used)]

use sofel_font::{FaceData, read::GlyphId};
use sofel_glyph_render::render_face;
use sofel_outline::Scratch;
use sofel_raster::Rasterizer;

#[path = "../../sofel-cli/tests/support/pfr.rs"]
mod pfr_fixture;

#[test]
fn adr0100_i1_i2_fixed_face_renders_selected_pixels() {
    let data = FaceData::new(include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf")).unwrap();
    let opened = data.open(0).unwrap();
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let image = render_face(
        opened.as_face(),
        GlyphId(1),
        8,
        Some(0),
        &mut scratch,
        &mut raster,
    )
    .unwrap();
    assert_eq!((image.width, image.rows), (8, 8));
    assert_eq!(
        image.pixels.get(..8),
        Some(&[0, 0, 0, 255, 255, 0, 0, 0][..])
    );
}

#[test]
fn adr0100_i4_bitmap_only_pfr_chooses_strike_without_explicit_index() {
    let mut bytes = pfr_fixture::bitmap_only();
    *bytes.get_mut(131).unwrap() = 0;
    let data = FaceData::new(&bytes).unwrap();
    let opened = data.open(0).unwrap();
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let image = render_face(
        opened.as_face(),
        GlyphId(1),
        12,
        None,
        &mut scratch,
        &mut raster,
    )
    .unwrap();
    assert_eq!((image.width, image.rows), (3, 2));
    assert_eq!(image.pixels, [0, 255, 0, 255, 0, 255]);
    assert!(
        render_face(
            opened.as_face(),
            GlyphId(1),
            13,
            None,
            &mut scratch,
            &mut raster
        )
        .is_err()
    );
}
