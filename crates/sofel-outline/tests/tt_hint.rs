//! FreeType v35, FT_LOAD_TARGET_MONO, square sizes from 1 to 96 ppem.
#![allow(clippy::unwrap_used)]

use sofel_font::{Font, read::GlyphId};
use sofel_outline::{HintTarget, Outlines, Scratch};
use std::path::PathBuf;

#[test]
fn adr0064_i1_native_v35_load_targets_match_freetype() {
    // FT_LOAD_NO_AUTOHINT | FT_LOAD_NO_BITMAP with each FT_LOAD_TARGET_*.
    for target in [
        HintTarget::Normal,
        HintTarget::Light,
        HintTarget::Mono,
        HintTarget::Lcd,
        HintTarget::VerticalLcd,
    ] {
        let expected = if target == HintTarget::Mono {
            0x67c3_ef45_e494_26a6
        } else {
            0xd38e_020e_b16e_6420
        };
        check_target("Roboto-Regular.abc.ttf", 1..4, expected, target);
    }
}

#[test]
fn adr0064_i1_roboto_mono_points_match_freetype() {
    check_glyphs("Roboto-Regular.abc.ttf", 1..4, 0x67c3_ef45_e494_26a6);
}

#[test]
fn adr0064_i1_composite_mono_points_match_freetype() {
    check_glyphs("Roboto-Regular.abcAE.ttf", 7..8, 0xce92_1611_1d00_726f);
}

fn check_glyphs(name: &str, glyphs: core::ops::Range<u32>, expected: u64) {
    check_target(name, glyphs, expected, HintTarget::Mono);
}

fn check_target(name: &str, glyphs: core::ops::Range<u32>, expected: u64, target: HintTarget) {
    let dir = std::env::var_os("SOFEL_TESTDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata"));
    let path = dir.join("harfbuzz/test/api/fonts").join(name);
    let Ok(data) = std::fs::read(&path) else {
        assert!(
            std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_none(),
            "missing {}",
            path.display()
        );
        return;
    };
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    // FNV-1a of the size, glyph ID, point count, then x/y/tag
    // for each point and count/end indices for the contours (little-endian i32).
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut add = |v: i32| {
        for byte in v.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    };
    for ppem in 1..=96 {
        for glyph in glyphs.clone() {
            outlines
                .load_truetype_v35(GlyphId(glyph), ppem, target, &mut scratch)
                .unwrap();
            add(i32::from(ppem));
            add(i32::try_from(glyph).unwrap());
            add(i32::try_from(scratch.points().len()).unwrap());
            assert_eq!(scratch.point_tags().len(), scratch.points().len());
            for (point, &tag) in scratch.points().iter().zip(scratch.point_tags()) {
                add(point.x);
                add(point.y);
                add(i32::from(tag));
            }
            add(i32::try_from(scratch.contour_ends().len()).unwrap());
            for &end in scratch.contour_ends() {
                add(i32::try_from(end).unwrap());
            }
        }
    }
    assert_eq!(hash, expected);
}
