#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
use sofel_font::{Font, read::GlyphId};
use sofel_outline::draw_pfr;

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
fn adr0065_i1_font_driver_pfr_outline_triangle() {
    use sofel_font::geom::{Command, FnPen, Point};
    let program = [0, 0x4f, 0x1e, 80, 0x1b, 60, 0];
    let mut data = pfr();
    data.truncate(121);
    data.extend_from_slice(&program);
    data[34] = 8;
    data[113] = 7;
    let face = Font::from_pfr(&data, 0).unwrap();
    let mut path = Vec::new();
    draw_pfr(&face, GlyphId(1), &mut FnPen(|c| path.push(c))).unwrap();
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

#[test]
fn adr0065_i1_font_driver_pfr_compound_transform_matches_freetype() {
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
    draw_pfr(&face, GlyphId(1), &mut FnPen(|c| path.push(c))).unwrap();
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
fn adr0065_i2_decode_bounds_pfr_rejects_cyclic_outlines_and_truncated_runs() {
    use sofel_font::geom::FnPen;
    let mut data = pfr();
    data.truncate(121);
    data.extend_from_slice(&[0x81, 0, 5, 0, 1]);
    data[34] = 6;
    data[113] = 5;
    let face = Font::from_pfr(&data, 0).unwrap();
    let mut commands = Vec::new();
    assert!(draw_pfr(&face, GlyphId(1), &mut FnPen(|c| commands.push(c))).is_err());
    assert!(commands.is_empty());
}
