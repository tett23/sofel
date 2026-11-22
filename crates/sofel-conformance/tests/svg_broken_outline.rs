//! A valid SVG remains drawable when its fallback glyf is broken.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic
)]

use sofel_color::PaintOptions;
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_glyph_render::{ColorGlyph, render_color};
use sofel_outline::{Outlines, Scale, Scratch};
use sofel_raster::{BgraImage, Rasterizer};
use sofel_read::GlyphId;

fn table(bytes: &[u8], tag: &[u8; 4]) -> (usize, usize, usize) {
    let count = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    for i in 0..count {
        let record = 12 + i * 16;
        if &bytes[record..record + 4] == tag {
            let offset = u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap());
            let len = u32::from_be_bytes(bytes[record + 12..record + 16].try_into().unwrap());
            return (
                record,
                usize::try_from(offset).unwrap(),
                usize::try_from(len).unwrap(),
            );
        }
    }
    panic!("missing table");
}

fn broken_glyf(mut bytes: Vec<u8>) -> Vec<u8> {
    let (glyf_record, glyf_offset, glyf_len) = table(&bytes, b"glyf");
    let (_, loca_offset, _) = table(&bytes, b"loca");
    let (_, head_offset, _) = table(&bytes, b"head");
    assert_eq!(&bytes[head_offset + 50..head_offset + 52], &[0, 0]);
    assert_eq!(glyf_len, 42);
    assert_eq!(&bytes[loca_offset + 38..loca_offset + 42], &[0, 21, 0, 21]);
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    let new_offset = bytes.len();
    bytes.extend_from_within(glyf_offset..glyf_offset + glyf_len);
    // One contour claims 1001 points, but its 20-byte glyf record has no flags.
    bytes.extend_from_slice(&[
        0, 1, 0, 0, 0, 0, 0, 100, 0, 100, 3, 232, 0, 0, 0, 0, 0, 0, 0, 0,
    ]);
    bytes[glyf_record + 8..glyf_record + 12]
        .copy_from_slice(&u32::try_from(new_offset).unwrap().to_be_bytes());
    bytes[glyf_record + 12..glyf_record + 16].copy_from_slice(&62_u32.to_be_bytes());
    for gid in 20..=28 {
        let at = loca_offset + gid * 2;
        bytes[at..at + 2].copy_from_slice(&31_u16.to_be_bytes());
    }
    bytes
}

fn image(bytes: &[u8]) -> Result<BgraImage, sofel_glyph_render::RenderError> {
    let font = Font::new(bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut dst = BgraImage::default();
    let mut raster = Rasterizer::new();
    let result = render_color(
        &outlines,
        GlyphId(19),
        16,
        &PaintOptions::default(),
        &mut Scratch::default(),
        &mut raster,
        &mut dst,
    );
    assert!(matches!(result, Ok(ColorGlyph::Color)), "{result:?}");
    Ok(dst)
}

#[test]
fn adr0066_i3_svg_paints_despite_broken_fallback_outline() {
    let original =
        std::fs::read(paths::testdata().join("extra-fonts/fonts/color-svg.ttf")).unwrap();
    let expected = image(&original).unwrap();
    assert!(expected.width > 0 && expected.rows > 0);
    let broken = broken_glyf(original);
    let font = Font::new(&broken, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    assert!(
        outlines
            .load(
                GlyphId(19),
                Scale::pixels(outlines.font(), 16, 16),
                &mut Scratch::default()
            )
            .is_err()
    );
    let actual = image(&broken).unwrap();
    assert_eq!(
        (actual.left, actual.top, actual.width, actual.rows),
        (expected.left, expected.top, expected.width, expected.rows)
    );
    assert_eq!(actual.pixels, expected.pixels);
}
