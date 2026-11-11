#![allow(clippy::all, clippy::indexing_slicing, clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
#[allow(dead_code)]
mod build;

use build::*;
use sofel_cli::glyph::{ColorGlyph, color_layers, render, render_color};
use sofel_color::{PaintOptions, Palette};
use sofel_font::Font;
use sofel_font::geom::Color;
use sofel_font::read::GlyphId;
use sofel_outline::{Outlines, Scratch};
use sofel_raster::{BgraImage, Rasterizer, blend_layer};

fn square(size: i16) -> Vec<u8> {
    simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, size),
            Pt::On(size, size),
            Pt::On(size, 0),
        ]],
        false,
        &[],
    )
}

fn font_for(base: (u16, u16, u16), layers: &[(u16, u16)], broken_layer: bool) -> Vec<u8> {
    glyph_font_with(
        &[
            Vec::new(),
            square(1000),
            square(500),
            square(1000),
            if broken_layer {
                vec![1, 2, 3, 4]
            } else {
                square(250)
            },
        ],
        true,
        false,
        vec![
            (b"COLR", colr_v0(&[base], layers)),
            (
                b"CPAL",
                cpal(
                    2,
                    &[0, 2],
                    &[
                        [0, 0, 255, 128],
                        [0, 255, 0, 128],
                        [255, 0, 0, 255],
                        [0, 255, 255, 255],
                    ],
                    None,
                ),
            ),
        ],
    )
}

#[test]
fn inv1_color_layers() {
    let cases: &[((u16, u16, u16), &[(u16, u16)], &[(u32, Option<u16>)])] = &[
        ((3, 0, 0), &[], &[]),
        ((3, 0, 1), &[(1, 0xffff)], &[(1, None)]),
        ((3, 0, 2), &[(1, 0), (2, 1)], &[(1, Some(0)), (2, Some(1))]),
        ((3, 0, 3), &[(1, 0), (2, 1)], &[]),
        ((3, 0, 3), &[(1, 0), (6, 0), (2, 1)], &[(1, Some(0))]),
        ((3, 0, 2), &[(1, 0), (2, 2)], &[(1, Some(0))]),
    ];
    for &(base, layers, expected) in cases {
        let data = font_for(base, layers, false);
        let font = Font::new(&data, 0).unwrap();
        assert_eq!(color_layers(&font, 3), expected);
    }
}

#[test]
fn inv2_color_pixels() {
    let data = font_for((3, 0, 2), &[(1, 0), (2, 0xffff)], false);
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let mut actual = BgraImage::default();
    for (palette, first_color, foreground) in [
        (
            Palette::Font(0),
            [0, 0, 255, 128],
            Color::new(0, 0, 255, 128),
        ),
        (
            Palette::Font(99),
            [0, 0, 255, 128],
            Color::new(0, 0, 255, 128),
        ),
        (
            Palette::Font(1),
            [255, 0, 0, 255],
            Color::new(0, 0, 255, 128),
        ),
        (
            Palette::Custom(&[]),
            [0, 0, 0, 0],
            Color::new(0, 0, 255, 128),
        ),
    ] {
        let options = PaintOptions {
            palette,
            foreground,
            ppem: 12,
        };
        assert!(matches!(
            render_color(
                &outlines,
                GlyphId(3),
                12,
                &options,
                &mut scratch,
                &mut raster,
                &mut actual
            ),
            Ok(ColorGlyph::Color)
        ));
        let mut expected = BgraImage::default();
        for (glyph, bgra) in [
            (1, first_color),
            (2, [foreground.b, foreground.g, foreground.r, foreground.a]),
        ] {
            let bitmap = render(&outlines, GlyphId(glyph), 12, &mut scratch, &mut raster).unwrap();
            blend_layer(&mut expected, &bitmap, bgra);
        }
        assert_eq!(actual, expected);
    }
}

#[test]
fn inv3_gray_fallback() {
    for (base, layers, broken) in [
        ((3, 0, 0), vec![], false),
        ((3, 0, 3), vec![(1, 0), (2, 0)], false),
        ((3, 0, 2), vec![(1, 0), (4, 0)], true),
    ] {
        let data = font_for(base, &layers, broken);
        let font = Font::new(&data, 0).unwrap();
        let outlines = Outlines::new(font.default_instance());
        let mut scratch = Scratch::default();
        let mut raster = Rasterizer::new();
        let mut image = BgraImage {
            pixels: vec![55; 4],
            width: 1,
            rows: 1,
            ..Default::default()
        };
        let options = PaintOptions::default();
        let expected = render(&outlines, GlyphId(3), 12, &mut scratch, &mut raster).unwrap();
        let expected_pixels = expected.pixels.to_vec();
        let expected_box = (expected.left, expected.top, expected.width, expected.rows);
        let actual = render_color(
            &outlines,
            GlyphId(3),
            12,
            &options,
            &mut scratch,
            &mut raster,
            &mut image,
        )
        .unwrap();
        let ColorGlyph::Gray(bitmap) = actual else {
            panic!("expected grayscale fallback")
        };
        assert_eq!(
            (bitmap.left, bitmap.top, bitmap.width, bitmap.rows),
            expected_box
        );
        assert_eq!(bitmap.pixels, expected_pixels);
        assert!(image.pixels.is_empty());
        assert_eq!((image.width, image.rows), (0, 0));
    }

    let data = glyph_font_with(
        &[Vec::new(), square(1000), square(500), vec![1, 2, 3, 4]],
        true,
        false,
        vec![(b"COLR", colr_v0(&[(3, 0, 1)], &[(1, 0xffff)]))],
    );
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let mut image = BgraImage {
        pixels: vec![55; 4],
        width: 1,
        rows: 1,
        ..Default::default()
    };
    let result = render_color(
        &outlines,
        GlyphId(3),
        12,
        &PaintOptions::default(),
        &mut scratch,
        &mut raster,
        &mut image,
    );
    assert!(result.is_err());
    assert!(image.pixels.is_empty());
}
