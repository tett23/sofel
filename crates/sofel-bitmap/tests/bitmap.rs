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

#[test]
fn bitmap_only_fonts_open() {
    use sofel_bitmap::{PixelMode, bitmap_layout, draw_bitmap};
    // Apple's bloc/bdat with bhed and no head.
    let image = Buf::new()
        .bytes(&[2, 3, 1, 2, 4])
        .bytes(&[0xA0, 0x40])
        .done();
    let data = Buf::new().u16(2).u16(0).bytes(&image).done();
    let loc = cblc(
        2,
        &[SizeSpec {
            start_glyph: 1,
            end_glyph: 1,
            subtables: vec![(1, 1, index_offsets(1, 1, 4, &[0, image.len() as u32]))],
            ..SizeSpec::default()
        }],
    );
    let mut t: Vec<(&[u8; 4], Vec<u8>)> = sample_tables()
        .into_iter()
        .map(|(tag, d)| (if tag == b"head" { b"bhed" } else { tag }, d))
        .collect();
    t.push((b"bdat", data));
    t.push((b"bloc", loc));
    t.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&t);
    let font = Font::new(&bytes, 0).unwrap();
    assert!(!font.has_outlines() && font.has_bitmaps());
    let s = strikes(&font).next().unwrap();
    assert_eq!(s.table, StrikeTable::Bloc);
    let l = bitmap_layout(&font, &s, 1).unwrap();
    assert_eq!(
        (l.width, l.rows, l.pitch, l.pixel_mode),
        (3, 2, 1, PixelMode::Mono)
    );
    let mut px = [0xFFu8; 2];
    draw_bitmap(&font, &s, 1, &mut px).unwrap();
    assert_eq!(px, [0xA0, 0x40]);
    assert_eq!(
        draw_bitmap(&font, &s, 1, &mut [0u8; 1]),
        Err(sofel_bitmap::BitmapError::BufferTooSmall(2))
    );
    assert_eq!(
        bitmap_layout(&font, &s, 2),
        Err(sofel_bitmap::BitmapError::Missing)
    );
    // An outline font with CBLC/CBDT: FreeType ignores the outlines.
    let square = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(0, 9), Pt::On(9, 9), Pt::On(9, 0)]],
        false,
        &[],
    );
    let bytes = glyph_font_with(
        &[Vec::new(), square],
        false,
        false,
        vec![(b"CBLC", cblc(3, &[])), (b"CBDT", vec![0, 3, 0, 0])],
    );
    let font = Font::new(&bytes, 0).unwrap();
    assert!(!font.has_outlines());
    let bytes = glyph_font(&[Vec::new()], false, false);
    assert!(Font::new(&bytes, 0).unwrap().has_outlines());
}

#[test]
fn i1_sbit_png_decodes_pixels_and_flips_logical_row() {
    let png = vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 244, 34, 127, 138, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 208,
        192, 240, 159, 225, 63, 0, 14, 125, 3, 126, 234, 246, 147, 192, 0, 0, 0, 0, 73, 69, 78, 68,
        174, 66, 96, 130,
    ];
    let sb = sbix(
        1,
        &[(
            20,
            72,
            vec![
                None,
                Some((1, -2, *b"png ", png)),
                Some((0, 0, *b"flip", vec![0, 1])),
            ],
        )],
    );
    let bytes = glyph_font_with(&vec![Vec::new(); 3], false, false, vec![(b"sbix", sb)]);
    let font = Font::new(&bytes, 0).unwrap();
    let strike = strikes(&font).next().unwrap();
    for (id, expected) in [
        (1, [0, 0, 128, 128, 0, 255, 0, 255]),
        (2, [0, 255, 0, 255, 0, 0, 128, 128]),
    ] {
        let layout = sofel_bitmap::bitmap_layout(&font, &strike, id).unwrap();
        assert_eq!((layout.width, layout.rows, layout.pitch), (2, 1, 8));
        let mut out = [99; 9];
        assert_eq!(
            sofel_bitmap::draw_bitmap(&font, &strike, id, &mut out).unwrap(),
            layout
        );
        assert_eq!(out[..8], expected);
        assert_eq!(out[8], 99);
    }
}
