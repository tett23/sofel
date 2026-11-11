#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

mod support;
use support::build::*;

use sofel_read::head::Head;
use sofel_read::hhea::Hhea;
use sofel_read::hmtx::Hmtx;
use sofel_read::maxp::Maxp;
use sofel_read::{CountWidth, Fixed, GlyphId, LongDateTime, ReadError};

#[test]
fn head_fields() {
    let data = head(HeadSpec {
        units_per_em: 2048,
        index_to_loc_format: 1,
        ..Default::default()
    });
    let h = Head::new(&data).unwrap();
    assert_eq!(h.major_version(), 1);
    assert_eq!(h.minor_version(), 0);
    assert_eq!(h.font_revision(), Fixed(0x0002_8000));
    assert_eq!(h.checksum_adjustment(), 0);
    assert_eq!(h.magic_number(), 0x5F0F_3CF5);
    assert_eq!(h.flags(), 0x000B);
    assert_eq!(h.units_per_em(), 2048);
    assert_eq!(h.created(), LongDateTime(3_000_000_000));
    assert_eq!(h.modified(), LongDateTime(3_100_000_000));
    assert_eq!(
        (h.x_min(), h.y_min(), h.x_max(), h.y_max()),
        (-50, -200, 950, 800)
    );
    assert_eq!(h.mac_style(), 1);
    assert_eq!(h.lowest_rec_ppem(), 9);
    assert_eq!(h.font_direction_hint(), 2);
    assert_eq!(h.index_to_loc_format(), 1);
    assert_eq!(h.glyph_data_format(), 0);
}

#[test]
fn head_errors() {
    let data = head(HeadSpec::default());
    assert!(matches!(
        Head::new(&data[..53]),
        Err(ReadError::Truncated {
            needed: 54,
            available: 53
        })
    ));
    let data = head(HeadSpec {
        major: 2,
        ..Default::default()
    });
    assert!(matches!(
        Head::new(&data),
        Err(ReadError::UnknownVersion(0x0002_0000))
    ));
}

#[test]
fn maxp_versions() {
    let m = Maxp::new(&maxp05(7), CountWidth::U16).unwrap();
    assert_eq!(m.version(), 0x0000_5000);
    assert_eq!(m.num_glyphs(), 7);
    assert!(m.truetype().is_none());

    let m = Maxp::new(&maxp10(9), CountWidth::U16).unwrap();
    assert_eq!(m.num_glyphs(), 9);
    let t = m.truetype().unwrap();
    assert_eq!(t.max_points, 1);
    assert_eq!(t.max_contours, 2);
    assert_eq!(t.max_composite_points, 3);
    assert_eq!(t.max_composite_contours, 4);
    assert_eq!(t.max_zones, 5);
    assert_eq!(t.max_twilight_points, 6);
    assert_eq!(t.max_storage, 7);
    assert_eq!(t.max_function_defs, 8);
    assert_eq!(t.max_instruction_defs, 9);
    assert_eq!(t.max_stack_elements, 10);
    assert_eq!(t.max_size_of_instructions, 11);
    assert_eq!(t.max_component_elements, 12);
    assert_eq!(t.max_component_depth, 13);
}

#[test]
fn maxp_errors() {
    assert!(matches!(
        Maxp::new(&maxp05(7)[..5], CountWidth::U16),
        Err(ReadError::Truncated { .. })
    ));
    assert!(matches!(
        Maxp::new(&maxp10(7)[..31], CountWidth::U16),
        Err(ReadError::Truncated { .. })
    ));
    let bad = Buf::new().u32(0x0002_0000).u16(3).done();
    assert!(matches!(
        Maxp::new(&bad, CountWidth::U16),
        Err(ReadError::UnknownVersion(0x0002_0000))
    ));
}

#[test]
fn wide_maxp() {
    let m = Maxp::new(&maxp_wide05(70_000), CountWidth::U24).unwrap();
    assert_eq!(m.num_glyphs(), 70_000);
}

#[test]
fn hhea_fields() {
    let data = hhea(20);
    let h = Hhea::new(&data, CountWidth::U16).unwrap();
    assert_eq!(h.major_version(), 1);
    assert_eq!((h.ascender(), h.descender(), h.line_gap()), (800, -200, 90));
    assert_eq!(h.advance_width_max(), 1200);
    assert_eq!(h.min_left_side_bearing(), -30);
    assert_eq!(h.min_right_side_bearing(), -40);
    assert_eq!(h.x_max_extent(), 1100);
    assert_eq!(
        (h.caret_slope_rise(), h.caret_slope_run(), h.caret_offset()),
        (1, 0, 5)
    );
    assert_eq!(h.metric_data_format(), 0);
    assert_eq!(h.number_of_h_metrics(), 20);
    assert!(matches!(
        Hhea::new(&hhea(20)[..35], CountWidth::U16),
        Err(ReadError::Truncated { .. })
    ));
    let data = hhea_wide(70_000);
    let h = Hhea::new(&data, CountWidth::U24).unwrap();
    assert_eq!(h.number_of_h_metrics(), 70_000);
}

#[test]
fn inv1_wide_metric_count() {
    for count in [0, 65_536, 16_777_216, u32::MAX] {
        for tag in [b"HHEA", b"VHEA"] {
            let mut bytes = hhea(0);
            bytes.truncate(34);
            bytes.extend_from_slice(&count.to_be_bytes());
            bytes.push(0xAA);
            let result = if tag == b"HHEA" {
                Hhea::new(&bytes, CountWidth::U24)
                    .unwrap()
                    .number_of_h_metrics()
            } else {
                sofel_read::vhea::Vhea::new(&bytes, CountWidth::U24)
                    .unwrap()
                    .number_of_h_metrics()
            };
            assert_eq!(result, count, "{tag:?}");
        }
    }
}

#[test]
fn inv2_truncated_wide_metric_count() {
    for tag in [b"HHEA", b"VHEA"] {
        for suffix_len in 0..4 {
            let mut bytes = hhea(0);
            bytes.truncate(34);
            bytes.extend_from_slice(&[0, 1, 0][..suffix_len.min(3)]);
            assert_eq!(bytes.len(), 34 + suffix_len);
            let result = if tag == b"HHEA" {
                Hhea::new(&bytes, CountWidth::U24).map(|_| ())
            } else {
                sofel_read::vhea::Vhea::new(&bytes, CountWidth::U24).map(|_| ())
            };
            assert!(matches!(
                result,
                Err(ReadError::Truncated { needed: 38, .. })
            ));
        }
    }
}

#[test]
fn inv3_narrow_metric_count() {
    for tag in [b"hhea", b"vhea"] {
        let mut bytes = hhea(0);
        bytes.truncate(34);
        bytes.extend_from_slice(&[0, 1]);
        let count = if tag == b"hhea" {
            Hhea::new(&bytes, CountWidth::U16)
                .unwrap()
                .number_of_h_metrics()
        } else {
            sofel_read::vhea::Vhea::new(&bytes, CountWidth::U16)
                .unwrap()
                .number_of_h_metrics()
        };
        assert_eq!(count, 1, "{tag:?}");
    }
}

#[test]
fn hmtx_ranges() {
    let data = hmtx(&[(500, 10), (600, 20)], &[30, -40]);
    let h = Hmtx::new(&data, 2, 4).unwrap();
    assert_eq!(h.advance(GlyphId(0)), Some(500));
    assert_eq!(h.left_side_bearing(GlyphId(1)), Some(20));
    // numberOfHMetrics <= gid < numGlyphs: last advance, trailing lsb.
    assert_eq!(h.advance(GlyphId(2)), Some(600));
    assert_eq!(h.left_side_bearing(GlyphId(3)), Some(-40));
    // gid >= numGlyphs
    assert_eq!(h.advance(GlyphId(4)), None);
    assert_eq!(h.left_side_bearing(GlyphId(4)), None);
}

#[test]
fn hmtx_errors_and_partial_data() {
    let data = hmtx(&[(500, 10), (600, 20)], &[30]);
    assert!(matches!(
        Hmtx::new(&data[..7], 2, 4),
        Err(ReadError::Truncated { .. })
    ));
    assert!(matches!(
        Hmtx::new(&data, 0, 4),
        Err(ReadError::InvalidValue)
    ));
    // The trailing lsb array is shorter than numGlyphs needs.
    let h = Hmtx::new(&data, 2, 4).unwrap();
    assert_eq!(h.left_side_bearing(GlyphId(2)), Some(30));
    assert_eq!(h.left_side_bearing(GlyphId(3)), None);
    assert_eq!(h.advance(GlyphId(3)), Some(600));
}

#[test]
fn wide_hmtx() {
    let lsbs: Vec<i16> = (0..69_998).map(|g| (g % 100) as i16).collect();
    let data = hmtx(&[(500, 1), (700, 2)], &lsbs);
    let h = Hmtx::new(&data, 2, 70_000).unwrap();
    assert_eq!(h.advance(GlyphId(69_999)), Some(700));
    assert_eq!(
        h.left_side_bearing(GlyphId(69_999)),
        Some((69_997 % 100) as i16)
    );
    assert_eq!(h.advance(GlyphId(70_000)), None);
}
