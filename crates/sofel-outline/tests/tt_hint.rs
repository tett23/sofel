//! FreeType v35, FT_LOAD_TARGET_MONO, square sizes from 1 to 96 ppem.
#![allow(clippy::unwrap_used)]

use sofel_font::{Font, read::GlyphId};
use sofel_outline::{Outlines, Scratch};
use std::path::PathBuf;

#[test]
fn adr0064_i1_roboto_mono_points_match_freetype() {
    check_glyphs("Roboto-Regular.abc.ttf", 1..4, 0x0669_0115_d204_3f9e);
}

#[test]
fn adr0064_i1_composite_mono_points_match_freetype() {
    check_glyphs("Roboto-Regular.abcAE.ttf", 7..8, 0xdcd1cf4ee1a62177);
}

fn check_glyphs(name: &str, glyphs: core::ops::Range<u32>, expected: u64) {
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
    // FNV-1a of the size, glyph ID, point count, then interleaved x/y
    // coordinates from FT_Load_Glyph, all encoded as little-endian i32.
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
                .load_truetype_mono(GlyphId(glyph), ppem, &mut scratch)
                .unwrap();
            add(i32::from(ppem));
            add(i32::try_from(glyph).unwrap());
            add(i32::try_from(scratch.points().len()).unwrap());
            for point in scratch.points() {
                add(point.x);
                add(point.y);
            }
        }
    }
    assert_eq!(hash, expected);
}
