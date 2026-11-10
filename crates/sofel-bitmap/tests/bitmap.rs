//! Strikes, their selection and glyph bitmaps on fonts built with the
//! ADR 0007 builders (ADR 0023).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
#[allow(dead_code)]
mod build;

use build::*;
use sofel_bitmap::{BitmapFormat, StrikeTable, choose_strike, glyph_bitmap, match_strike, strikes};
use sofel_font::Font;
use sofel_font::read::Tag;

fn eblc_font(ppems: &[(u8, u8)]) -> Vec<u8> {
    let image = Buf::new()
        .bytes(&[2, 3, 1, 2, 4])
        .bytes(&[0xE0, 0xA0])
        .done();
    let mut data = Buf::new().u16(2).u16(0);
    data = data.bytes(&image);
    let sizes: Vec<SizeSpec> = ppems
        .iter()
        .map(|&(x, y)| SizeSpec {
            ppem_x: x,
            ppem_y: y,
            start_glyph: 1,
            end_glyph: 1,
            subtables: vec![(1, 1, index_offsets(1, 1, 4, &[0, image.len() as u32]))],
            ..SizeSpec::default()
        })
        .collect();
    glyph_font_with(
        &[Vec::new(), Vec::new()],
        false,
        false,
        vec![(b"EBLC", cblc(2, &sizes)), (b"EBDT", data.done())],
    )
}

#[test]
fn strike_lists_and_selection() {
    let bytes = eblc_font(&[(16, 16), (24, 24), (12, 13), (0, 10), (24, 24)]);
    let font = Font::new(&bytes, 0).unwrap();
    let s: Vec<(u32, u16, u16)> = strikes(&font)
        .map(|s| (s.index, s.x_ppem, s.y_ppem))
        .collect();
    // The strike with a 0 ppem is left out, as FreeType does.
    assert_eq!(s, [(0, 16, 16), (1, 24, 24), (2, 12, 13), (4, 24, 24)]);
    let first = strikes(&font).next().unwrap();
    assert_eq!(
        (first.table, first.bit_depth, first.height),
        (StrikeTable::Eblc, 1, 16)
    );
    // HarfBuzz: the smallest at least as large, else the largest; the
    // first of equal ones; 0 for the largest.
    let c = |p| choose_strike(&font, p).map(|s| s.index);
    assert_eq!(c(14), Some(0));
    assert_eq!(c(16), Some(0));
    assert_eq!(c(13), Some(2));
    assert_eq!(c(30), Some(1));
    assert_eq!(c(0), Some(1));
    // FreeType: rounded ppems equal, in 26.6.
    let m = |x, y| match_strike(&font, x, y).map(|s| s.index);
    assert_eq!(m(16 * 64, 16 * 64), Some(0));
    assert_eq!(m(16 * 64 + 31, 16 * 64 - 32), Some(0));
    assert_eq!(m(16 * 64 + 32, 16 * 64), None);
    assert_eq!(m(12 * 64, 13 * 64), Some(2));
    assert_eq!(m(0, 24 * 64), Some(1));
    assert_eq!(m(13 * 64, 0), None);
    assert_eq!(m(0, 0), None);
}

#[test]
fn ebdt_glyph_data() {
    let bytes = eblc_font(&[(16, 16)]);
    let font = Font::new(&bytes, 0).unwrap();
    let s = strikes(&font).next().unwrap();
    let b = glyph_bitmap(&font, &s, 1).unwrap();
    assert_eq!(b.format, BitmapFormat::Ebdt(1));
    assert_eq!(b.data, &[0xE0, 0xA0]);
    assert_eq!(
        (
            b.metrics.width,
            b.metrics.height,
            b.metrics.hori_bearing_y,
            b.metrics.hori_advance
        ),
        (3, 2, 2, 4)
    );
    assert!(glyph_bitmap(&font, &s, 0).is_none());
}

#[test]
fn sbix_dupe_flip_and_other_graphics() {
    let png = png_header(5, 7);
    let sb = sbix(
        1,
        &[(
            20,
            72,
            vec![
                None,
                Some((1, -2, *b"png ", png.clone())),
                Some((0, 0, *b"flip", vec![0, 1])),
                Some((0, 0, *b"jpg ", vec![1, 2, 3])),
                Some((0, 0, *b"dupe", vec![0, 4])),
            ],
        )],
    );
    let bytes = glyph_font_with(&vec![Vec::new(); 5], false, false, vec![(b"sbix", sb)]);
    let font = Font::new(&bytes, 0).unwrap();
    let s = strikes(&font).next().unwrap();
    assert_eq!(
        (s.table, s.x_ppem, s.resolution, s.bit_depth),
        (StrikeTable::Sbix, 20, 72, 32)
    );
    let b = glyph_bitmap(&font, &s, 1).unwrap();
    assert_eq!(b.format, BitmapFormat::Sbix(Tag::new(b"png ")));
    assert_eq!(
        (
            b.metrics.width,
            b.metrics.height,
            b.metrics.hori_bearing_x,
            b.metrics.hori_bearing_y
        ),
        (5, 7, 1, 5)
    );
    // hmtx advance 1000 at 20 ppem of 1000 units: 20.
    assert_eq!(b.metrics.hori_advance, 20);
    let f = glyph_bitmap(&font, &s, 2).unwrap();
    assert!(f.flipped);
    assert_eq!(f.data, &png[..]);
    let j = glyph_bitmap(&font, &s, 3).unwrap();
    assert_eq!(
        (j.format, j.data, j.metrics.width),
        (BitmapFormat::Sbix(Tag::new(b"jpg ")), &[1u8, 2, 3][..], 0)
    );
    // A dupe of itself is followed 4 times and then given up.
    assert!(glyph_bitmap(&font, &s, 4).is_none());
    assert!(sofel_bitmap::sbix_png(&font, 2, 0).is_none());
    assert_eq!(
        sofel_bitmap::sbix_png(&font, 1, 0)
            .unwrap()
            .pixel_extents
            .y_bearing,
        5
    );
}
