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
            0x574c_9134_3a56_d1c3
        } else {
            0x93e7_1033_6c3d_97e1
        };
        check_target("Roboto-Regular.abc.ttf", 1..4, expected, target, false);
    }
}

#[test]
fn adr0064_i1_roboto_mono_points_match_freetype() {
    check_glyphs("Roboto-Regular.abc.ttf", 1..4, 0x574c_9134_3a56_d1c3);
}

#[test]
fn adr0064_i1_composite_mono_points_match_freetype() {
    check_glyphs("Roboto-Regular.abcAE.ttf", 7..8, 0x7ff2_198e_1634_dd43);
}

fn check_glyphs(name: &str, glyphs: core::ops::Range<u32>, expected: u64) {
    check_target(name, glyphs, expected, HintTarget::Mono, false);
}

fn check_target(
    name: &str,
    glyphs: core::ops::Range<u32>,
    expected: u64,
    target: HintTarget,
    v40: bool,
) {
    let relative_path = PathBuf::from("harfbuzz/test/api/fonts").join(name);
    check_sizes(relative_path, Some(glyphs), 1..=96, expected, target, v40);
}

fn check_sizes(
    relative_path: PathBuf,
    glyphs: Option<core::ops::Range<u32>>,
    sizes: core::ops::RangeInclusive<u16>,
    expected: u64,
    target: HintTarget,
    v40: bool,
) {
    let Some(data) = fixture(relative_path) else {
        return;
    };
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    // FNV-1a of the size, glyph ID, point count, then x/y/tag
    // for each point, horizontal advance, and count/end indices for the contours (little-endian i32).
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut add = |v: i32| {
        for byte in v.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    };
    let glyphs = glyphs.unwrap_or(0..font.num_glyphs());
    for ppem in sizes {
        for glyph in glyphs.clone() {
            let (_, advance) = if v40 {
                outlines.load_truetype_v40(GlyphId(glyph), ppem, target, &mut scratch)
            } else {
                outlines.load_truetype_v35_with_advance(GlyphId(glyph), ppem, target, &mut scratch)
            }
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
            add(advance);
            add(i32::try_from(scratch.contour_ends().len()).unwrap());
            for &end in scratch.contour_ends() {
                add(i32::try_from(end).unwrap());
            }
        }
    }
    assert_eq!(hash, expected);
}

#[test]
fn adr0064_i1_native_v40_load_targets_match_freetype() {
    for target in [
        HintTarget::Normal,
        HintTarget::Light,
        HintTarget::Mono,
        HintTarget::Lcd,
        HintTarget::VerticalLcd,
    ] {
        let expected = if target == HintTarget::Mono {
            0x574c_9134_3a56_d1c3
        } else {
            0xeb5c_a042_4c4b_d436
        };
        let composite = match target {
            HintTarget::Mono => 0x7ff2_198e_1634_dd43,
            HintTarget::Normal | HintTarget::Light => 0x0181_6453_fe1d_548e,
            _ => 0x4db1_d897_a0d3_f126,
        };
        check_target("Roboto-Regular.abcAE.ttf", 7..8, composite, target, true);
        check_target("Roboto-Regular.abc.ttf", 1..4, expected, target, true);
    }
}

#[test]
fn adr0064_i1_full_roboto_at_16_ppem_matches_freetype_in_every_target() {
    for (target, v40_hash) in [
        (HintTarget::Normal, 0xda26_0ff5_f510_50ea),
        (HintTarget::Light, 0xda26_0ff5_f510_50ea),
        (HintTarget::Mono, 0x740a_d04b_e7d6_0ce1),
        (HintTarget::Lcd, 0x0abe_79a5_0b4c_59d9),
        (HintTarget::VerticalLcd, 0xda26_0ff5_f510_50ea),
    ] {
        for (v40, hash) in [(false, 0x740a_d04b_e7d6_0ce1), (true, v40_hash)] {
            check_sizes(
                "harfbuzz/perf/fonts/Roboto-Regular.ttf".into(),
                None,
                16..=16,
                hash,
                target,
                v40,
            );
        }
    }
}

fn fixture(relative_path: PathBuf) -> Option<Vec<u8>> {
    let dir = std::env::var_os("SOFEL_TESTDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata"));
    let path = dir.join(relative_path);
    let Ok(data) = std::fs::read(&path) else {
        assert!(
            std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_none(),
            "missing {}",
            path.display()
        );
        return None;
    };
    Some(data)
}

#[test]
fn adr0064_i1_v35_uses_hdmx_device_width() {
    let Some(mut data) = fixture("harfbuzz/test/api/fonts/Roboto-Regular.abc.ttf".into()) else {
        return;
    };
    let font = Font::new(&data, 0).unwrap();
    let table = font.table(sofel_font::read::Tag::new(b"hdmx")).unwrap();
    let offset = table.as_ptr() as usize - data.as_ptr() as usize;
    assert_eq!(table[8], 9);
    data[offset + 9] = 42; // Maximum width in the 9 ppem record.
    data[offset + 11] = 42; // Width for glyph 1.
    let font = Font::new(&data, 0).unwrap();
    let outline = Outlines::new(font.default_instance());
    let (_, width) = outline
        .load_truetype_v35_with_advance(GlyphId(1), 9, HintTarget::Mono, &mut Scratch::default())
        .unwrap();
    assert_eq!(width, 42 * 64);
}
