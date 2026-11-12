//! The four COLRv1 gallery reference images (sofel ADR 0072, I1).
#![allow(clippy::unwrap_used)]
use sofel_cli::glyph::{ColorGlyph, render_color};
use sofel_color::PaintOptions;
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_outline::{Outlines, Scratch};
use sofel_raster::{BgraImage, Rasterizer};
use sofel_read::GlyphId;

fn compare(font_path: &str, glyph: u32, reference: &[u8], require_color: bool) {
    let bytes = std::fs::read(paths::testdata().join(font_path)).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut image = BgraImage::default();
    let mut raster = Rasterizer::new();
    let result = render_color(
        &outlines,
        GlyphId(glyph),
        48,
        &PaintOptions::default(),
        &mut Scratch::default(),
        &mut raster,
        &mut image,
    );
    assert!(
        matches!(result, Ok(ColorGlyph::Color)),
        "{font_path} glyph {glyph}: no color image"
    );
    let (header, pixels) = reference.split_at(
        reference
            .iter()
            .position(|b| *b == b'\n')
            .unwrap()
            .saturating_add(1),
    );
    assert_eq!(
        format!(
            "BGRA {} {} {} {}\n",
            image.left, image.top, image.width, image.rows
        )
        .as_bytes(),
        header
    );
    assert_eq!(image.pixels.len(), pixels.len());
    if require_color {
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] != 0 && (p[0] != p[1] || p[1] != p[2]))
        );
    }
    for (i, (a, b)) in image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(pixels.as_chunks::<4>().0.iter())
        .enumerate()
    {
        let width = usize::try_from(image.width).unwrap();
        assert_eq!(
            a,
            b,
            "glyph {glyph}, BGRA pixel ({},{})",
            i.checked_rem(width).unwrap(),
            i.checked_div(width).unwrap()
        );
    }
}
const CFF: &str = "extra-fonts/fonts/color-cff.otf";
const VARIABLE: &str = "harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf";
#[test]
fn i1_color_cff_19() {
    compare(
        CFF,
        19,
        include_bytes!("../../../conformance/color-raster/cff-19.bgra"),
        true,
    );
}
#[test]
fn i1_color_cff_26() {
    compare(
        CFF,
        26,
        include_bytes!("../../../conformance/color-raster/cff-26.bgra"),
        true,
    );
}
#[test]
fn i1_variable_colr_92() {
    compare(
        VARIABLE,
        92,
        include_bytes!("../../../conformance/color-raster/variable-92.bgra"),
        true,
    );
}
#[test]
fn i1_variable_colr_93() {
    compare(
        VARIABLE,
        93,
        include_bytes!("../../../conformance/color-raster/variable-93.bgra"),
        true,
    );
}

#[test]
fn i2_colr_pixels_sweep() {
    compare(
        VARIABLE,
        27,
        include_bytes!("../../../conformance/color-raster/variable-27.bgra"),
    );
    compare(
        VARIABLE,
        38,
        include_bytes!("../../../conformance/color-raster/variable-38.bgra"),
    );
    compare(
        VARIABLE,
        12,
        include_bytes!("../../../conformance/color-raster/variable-12.bgra"),
    );
}

#[test]
fn i2_colr_pixels_all_28_composite_modes() {
    let mut references: &[u8] = include_bytes!("../../../conformance/color-raster/composites.bgra");
    for glyph in 120..=147 {
        let header = references.iter().position(|b| *b == b'\n').unwrap();
        let fields: Vec<_> = std::str::from_utf8(references.get(..header).unwrap())
            .unwrap()
            .split_whitespace()
            .collect();
        let width: usize = fields.get(3).unwrap().parse().unwrap();
        let rows: usize = fields.get(4).unwrap().parse().unwrap();
        let (image, rest) = references.split_at(
            header
                .checked_add(1)
                .unwrap()
                .checked_add(width.checked_mul(rows).unwrap().checked_mul(4).unwrap())
                .unwrap(),
        );
        compare(VARIABLE, glyph, image, false);
        references = rest;
    }
    assert!(references.is_empty());
}
