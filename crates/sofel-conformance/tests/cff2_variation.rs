//! CFF2 boxes at variable locations against the pinned HarfBuzz (ADR 0027).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::path::Path;
use std::process::Command;

use sofel_conformance::outline::{parse, scale_for, sofel_glyph};
use sofel_conformance::paths;
use sofel_font::read::{GlyphId, Tag};
use sofel_font::var::{Location, Variation};
use sofel_font::{Font, HbCff2};
use sofel_outline::{Outlines, Scratch, cff2_extents_at};

fn compare(font_path: &Path, positions: &[(&str, &[u8; 4], f32)]) -> usize {
    let exe = paths::hb_dump();
    paths::require(font_path, "CFF2 reference font").unwrap();
    paths::require(&exe, "hb-dump").unwrap();
    let bytes = std::fs::read(font_path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let cff = HbCff2::new(&font).unwrap();
    let mut total = 0;
    for &(label, tag, value) in positions {
        let location = Location::new(
            &font,
            None,
            &[Variation {
                tag: Tag::new(tag),
                value,
            }],
        );
        let instance = font.instance(&location);
        let outlines = Outlines::new(instance);
        let output = Command::new(&exe)
            .arg("--extents")
            .arg(format!("--variations={label}"))
            .arg(font_path)
            .output()
            .unwrap();
        assert!(output.status.success(), "hb-dump failed for {label}");
        let expected = String::from_utf8(output.stdout).unwrap();
        let mut count = 0;
        for line in expected.lines().filter(|line| line.starts_with("extents ")) {
            let parts: Vec<_> = line.split_whitespace().collect();
            let glyph: u32 = parts[1].parse().unwrap();
            let actual = cff2_extents_at(&cff, glyph, &instance);
            let found: i32 = parts[2].parse().unwrap();
            let numbers: Vec<i32> = parts[3..]
                .iter()
                .map(|value| value.parse().unwrap())
                .collect();
            let expected = (found != 0).then(|| sofel_font::GlyphExtents {
                x_bearing: numbers[0],
                y_bearing: numbers[1],
                width: numbers[2],
                height: numbers[3],
            });
            assert_eq!(
                actual,
                expected,
                "{label} glyph {glyph} {}",
                font_path.display()
            );
            assert_eq!(
                outlines.glyph_extents(GlyphId(glyph)),
                expected,
                "public {label} glyph {glyph}"
            );
            count += 1;
        }
        assert_eq!(
            count,
            usize::try_from(font.num_glyphs()).unwrap(),
            "{label}"
        );
        assert!(cff2_extents_at(&cff, font.num_glyphs(), &instance).is_none());
        total += count;
    }
    total
}

#[test]
fn adr0027_i04_cff2_extents_match_harfbuzz() {
    let small = paths::testdata()
        .join("harfbuzz/test/subset/data/fonts/SourceSerif4Variable-Roman-HelloWorld.otf");
    assert_eq!(
        compare(
            &small,
            &[
                ("wght=200", b"wght", 200.0),
                ("wght=400", b"wght", 400.0),
                ("wght=900", b"wght", 900.0),
                ("opsz=8", b"opsz", 8.0),
                ("opsz=72", b"opsz", 72.0),
            ],
        ),
        45,
    );
    let full = paths::testdata().join("extra-fonts/fonts/source-serif.otf");
    assert_eq!(
        compare(
            &full,
            &[
                ("wght=200", b"wght", 200.0),
                ("wght=900", b"wght", 900.0),
                ("opsz=72", b"opsz", 72.0),
            ],
        ),
        4392,
    );
}

#[test]
fn adr0027_i04_cff2_outlines_match_freetype() {
    let font_path = paths::testdata()
        .join("harfbuzz/test/subset/data/fonts/SourceSerif4Variable-Roman-HelloWorld.otf");
    let exe = paths::ft_dump();
    paths::require(&font_path, "CFF2 reference font").unwrap();
    paths::require(&exe, "ft-dump").unwrap();
    let bytes = std::fs::read(&font_path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let mut scratch = Scratch::default();
    let mut checked = 0;
    for &(label, tag, value) in &[
        ("wght=200", b"wght", 200.0),
        ("wght=900", b"wght", 900.0),
        ("opsz=72", b"opsz", 72.0),
    ] {
        let location = Location::new(
            &font,
            None,
            &[Variation {
                tag: Tag::new(tag),
                value,
            }],
        );
        let outlines = Outlines::new(font.instance(&location));
        for size in ["unscaled", "48"] {
            let output = Command::new(&exe)
                .args(["--outline", size])
                .arg(format!("--variations={label}"))
                .arg(&font_path)
                .output()
                .unwrap();
            assert!(output.status.success(), "ft-dump failed for {label} {size}");
            let expected = parse(&String::from_utf8(output.stdout).unwrap()).unwrap();
            let face = expected.get(&0).unwrap();
            assert_eq!(face.num_glyphs, font.num_glyphs());
            for glyph in 0..font.num_glyphs() {
                assert_eq!(
                    sofel_glyph(&outlines, glyph, scale_for(&font, size), &mut scratch),
                    *face.glyphs.get(&glyph).unwrap(),
                    "{label} {size} glyph {glyph}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 54);
}
