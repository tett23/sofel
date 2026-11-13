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
