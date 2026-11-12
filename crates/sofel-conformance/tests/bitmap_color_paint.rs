//! Non-strike glyph sizes use the HarfBuzz raster image callback (sofel ADR 0066 I2).
#![allow(clippy::unwrap_used)]
use sofel_cli::glyph::{ColorGlyph, render_color};
use sofel_color::PaintOptions;
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_outline::{Outlines, Scratch};
use sofel_raster::{BgraImage, Rasterizer};
use sofel_read::GlyphId;

#[test]
fn i2_transformed_png_glyphs_match_harfbuzz_raster() {
    for (path, glyph, reference) in [
        (
            "extra-fonts/fonts/color-sbix.ttf",
            19,
            include_bytes!("../../../conformance/color-raster/scaled-sbix-19.bgra").as_slice(),
        ),
        (
            "harfbuzz/test/subset/data/fonts/NotoColorEmoji.subset.ttf",
            1,
            include_bytes!("../../../conformance/color-raster/scaled-cbdt-1.bgra").as_slice(),
        ),
    ] {
        let bytes = std::fs::read(paths::testdata().join(path)).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let mut dst = BgraImage::default();
        let mut raster = Rasterizer::new();
        let result = render_color(
            &Outlines::new(font.default_instance()),
            GlyphId(glyph),
            31,
            &PaintOptions::default(),
            &mut Scratch::default(),
            &mut raster,
            &mut dst,
        );
        assert!(
            matches!(result, Ok(ColorGlyph::Color)),
            "{path}: missing color"
        );
        let (header, pixels) = reference.split_at(
            reference
                .iter()
                .position(|b| *b == b'\n')
                .unwrap()
                .saturating_add(1),
        );
        assert_eq!(
            format!("BGRA {} {} {} {}\n", dst.left, dst.top, dst.width, dst.rows).as_bytes(),
            header,
            "{path}"
        );
        assert_eq!(dst.pixels.len(), pixels.len());
        for (i, (a, b)) in dst.pixels.iter().zip(pixels).enumerate() {
            assert_eq!(a, b, "{path}: byte {i}");
        }
    }
}

#[test]
fn i2_cli_samples_nonmatching_strike_and_honors_no_bitmap() {
    use std::ffi::OsString;
    let font = paths::testdata().join("extra-fonts/fonts/color-sbix.ttf");
    let mut args: Vec<OsString> = ["--color", "--ppem", "31", "--glyph", "19"]
        .into_iter()
        .map(Into::into)
        .collect();
    args.push(font.into_os_string());
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(
        sofel_cli::render::run(&args, &mut out, &mut err),
        0,
        "{}",
        String::from_utf8_lossy(&err)
    );
    assert!(out.starts_with(b"P7\n"));
    args.insert(0, "--no-bitmap".into());
    out.clear();
    let status = sofel_cli::render::run(&args, &mut out, &mut err);
    assert!(status == 1 && out.is_empty() || status == 0 && out.starts_with(b"P5\n"));
}
