#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
use sofel_font::{Font, read::GlyphId};

// Logical directory at 58, identity logical font at 65, physical font at
// 83, and an empty outline program at GPS-relative offset 1.
fn pfr() -> Vec<u8> {
    let mut data = vec![0; 124];
    data[..4].copy_from_slice(b"PFR0");
    for (offset, width, value) in [
        (4, 2, 4u32),
        (6, 2, 0x0d0a),
        (8, 2, 58),
        (10, 2, 7),
        (12, 2, 58),
        (14, 2, 18),
        (16, 3, 18),
        (19, 3, 65),
        (22, 2, 37),
        (24, 3, 37),
        (27, 3, 83),
        (30, 2, 2),
        (32, 3, 4),
        (35, 3, 120),
        (52, 2, 1),
        (56, 2, 1),
        (58, 2, 1),
        (60, 2, 18),
        (62, 3, 65),
        (65, 3, 65536),
        (74, 3, 65536),
        (78, 2, 37),
        (80, 3, 83),
        (85, 2, 1000),
        (87, 2, 1000),
        (98, 2, 500),
        (110, 2, 1),
        (112, 1, 65),
        (113, 1, 2),
        (114, 2, 1),
    ] {
        data[offset..offset + width].copy_from_slice(&value.to_be_bytes()[4 - width..]);
    }
    data
}

#[test]
fn i1_font_driver_pfr_face_metrics_and_character_mapping() {
    let data = pfr();
    let face = Font::from_pfr(&data, 0).unwrap();
    assert_eq!(face.face_count(), 1);
    assert_eq!(face.glyph_count(), 2);
    assert_eq!(face.nominal_glyph(65), Some(GlyphId(1)));
    assert_eq!(face.nominal_glyph(66), None);
    assert_eq!(face.advance(GlyphId(1)), Some(500));
    assert!(Font::from_pfr(&data, 1).is_err());
}

#[test]
fn i2_decode_bounds_pfr_rejects_truncated_sections_and_glyph_programs() {
    let data = pfr();
    for len in 0..data.len() {
        assert!(Font::from_pfr(&data[..len], 0).is_err(), "length {len}");
    }
    let mut broken = data;
    broken[114..116].copy_from_slice(&u16::MAX.to_be_bytes());
    assert!(Font::from_pfr(&broken, 0).is_err());
}

#[test]
fn i1_font_driver_pfr_outline_triangle() {
    use sofel_font::geom::{Command, FnPen, Point};
    let program = [0, 0x4f, 0x1e, 80, 0x1b, 60, 0];
    let mut data = pfr();
    data.truncate(121);
    data.extend_from_slice(&program);
    data[34] = 8;
    data[113] = 7;
    let face = Font::from_pfr(&data, 0).unwrap();
    let mut path = Vec::new();
    face.draw(GlyphId(1), &mut FnPen(|c| path.push(c))).unwrap();
    assert_eq!(
        path,
        [
            Command::MoveTo(Point { x: 0, y: 0 }),
            Command::LineTo(Point { x: 80, y: 0 }),
            Command::LineTo(Point { x: 80, y: 60 }),
            Command::LineTo(Point { x: 0, y: 0 }),
            Command::Close
        ]
    );
}

fn bitmap_font(program: &[u8]) -> Vec<u8> {
    let mut p = pfr();
    p.truncate(120);
    p[97] = 128;
    p.splice(
        100..100,
        [1, 13, 1, 0, 0, 4, 0, 1, 12, 12, 0, 0, 4, 0, 0, 1],
    );
    let size = u8::try_from(program.len()).unwrap();
    p.extend([65, size, 0, 3, 0, 0, 0]);
    p.extend_from_slice(program);
    for (i, v) in [(23, 53), (26, 53), (79, 53), (34, size + 3), (37, 140)] {
        p[i] = v;
    }
    p
}

#[test]
fn i1_font_driver_pfr_strikes_match_freetype() {
    // Each pair of runs is white then black; pixels continue across scanlines.
    for program in [
        &[4, 0, 0x32, 0xa8][..],
        &[0x44, 0, 0x32, 1, 0x11, 0x11, 0x10],
        &[0x84, 0, 0x32, 0, 1, 1, 1, 1, 1, 1],
    ] {
        let data = bitmap_font(program);
        let face = Font::from_pfr(&data, 0).unwrap();
        let bitmap = face.bitmap(GlyphId(1), 12, 12).unwrap().unwrap();
        assert_eq!(
            (
                bitmap.width,
                bitmap.height,
                bitmap.left,
                bitmap.top,
                bitmap.advance
            ),
            (3, 2, 0, 2, 384)
        );
        assert_eq!(bitmap.pixels, [0x40, 0xa0]);
        assert!(face.bitmap(GlyphId(1), 10, 10).unwrap().is_none());
    }
}

#[test]
fn i1_font_driver_pfr_compound_transform_matches_freetype() {
    use sofel_font::geom::{Command, FnPen, Point};
    let mut data = pfr();
    data.truncate(121);
    data.extend_from_slice(&[
        0, 0x4f, 0x1e, 80, 0x1b, 60, 0, 0x81, 0x1a, 0x08, 0, 10, 251, 7, 0, 1,
    ]);
    data[34] = 17;
    data[113] = 9;
    data[115] = 8;
    let face = Font::from_pfr(&data, 0).unwrap();
    let mut path = Vec::new();
    face.draw(GlyphId(1), &mut FnPen(|c| path.push(c))).unwrap();
    assert_eq!(
        path,
        [
            Command::MoveTo(Point { x: 10, y: -5 }),
            Command::LineTo(Point { x: 50, y: -5 }),
            Command::LineTo(Point { x: 50, y: 55 }),
            Command::LineTo(Point { x: 10, y: -5 }),
            Command::Close
        ]
    );
}

#[test]
fn i2_decode_bounds_pfr_rejects_cyclic_outlines_and_truncated_runs() {
    use sofel_font::geom::FnPen;
    let mut data = pfr();
    data.truncate(121);
    data.extend_from_slice(&[0x81, 0, 5, 0, 1]);
    data[34] = 6;
    data[113] = 5;
    let face = Font::from_pfr(&data, 0).unwrap();
    let mut commands = Vec::new();
    assert!(
        face.draw(GlyphId(1), &mut FnPen(|c| commands.push(c)))
            .is_err()
    );
    assert!(commands.is_empty());
    let short = bitmap_font(&[0x84, 0, 0x32, 0, 3, 1]);
    assert!(
        Font::from_pfr(&short, 0)
            .unwrap()
            .bitmap(GlyphId(1), 12, 12)
            .is_err()
    );
}
