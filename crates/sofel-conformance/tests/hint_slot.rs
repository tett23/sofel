//! Compare loaded TrueType slots before rendering (sofel ADR 0064).
#![allow(clippy::unwrap_used)]

use sofel_conformance::paths;
use sofel_font::{
    Font,
    read::{GlyphId, Tag},
    var::{Location, Variation},
};
use sofel_outline::{HintMode, HintTarget, OutlineError, Outlines, Scratch};
use std::{fmt::Write, process::Command};

#[test]
fn adr0064_i1_native_slots_match_freetype() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    check_slots(
        &path,
        &Outlines::new(font.default_instance()),
        &[1, 4, 7],
        None,
        &["v35", "v40"],
    );
}

#[test]
fn adr0064_i1_variable_slots_match_freetype() {
    let path = paths::testdata()
        .join("harfbuzz/test/shape/data/text-rendering-tests/fonts/TestCVARGVARTwo.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let glyphs = b"hon".map(|c| font.nominal_glyph(u32::from(c)).unwrap().0);
    for weight in [28u16, 94] {
        let location = Location::new(
            &font,
            None,
            &[Variation {
                tag: Tag::new(b"wght"),
                value: f32::from(weight),
            }],
        );
        let args = format!("--variations=wght={weight}");
        check_slots(
            &path,
            &Outlines::new(font.instance(&location)),
            &glyphs,
            Some(&args),
            &["v35", "v40"],
        );
    }
}

#[test]
fn adr0064_i1_auto_slots_match_freetype() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    check_slots(
        &path,
        &Outlines::new(font.default_instance()),
        &[0, 1, 2, 3, 4],
        None,
        &["auto"],
    );
}

#[test]
fn adr0064_i1_auto_composite_reports_unsupported() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    assert!(matches!(
        outlines.load_auto_hinted(GlyphId(7), 16, HintTarget::Normal, &mut scratch),
        Err(OutlineError::AutoHintUnsupportedGlyph)
    ));
    assert!(scratch.points().is_empty());
}

#[test]
fn adr0064_i1_unhinted_load_entry_matches_freetype() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    check_slots(
        &path,
        &Outlines::new(font.default_instance()),
        &[1, 4, 5, 7],
        None,
        &["none"],
    );
}

fn check_slots(
    path: &std::path::Path,
    outlines: &Outlines<'_, '_, '_>,
    glyphs: &[u32],
    variation: Option<&str>,
    engines: &[&str],
) {
    let mut scratch = Scratch::default();
    for (name, target) in [
        ("normal", HintTarget::Normal),
        ("light", HintTarget::Light),
        ("mono", HintTarget::Mono),
        ("lcd", HintTarget::Lcd),
        ("lcd-v", HintTarget::VerticalLcd),
    ] {
        for &engine in engines {
            for &gid in glyphs {
                let reference = Command::new(paths::ft_dump())
                    .args(["--hint-slot", "16", &gid.to_string(), engine, name])
                    .args(variation)
                    .arg(path)
                    .output()
                    .unwrap();
                assert!(
                    reference.status.success(),
                    "{}",
                    String::from_utf8_lossy(&reference.stderr)
                );
                let mode = match engine {
                    "none" => HintMode::None,
                    "v35" => HintMode::NativeV35,
                    "v40" => HintMode::NativeV40,
                    "auto" => HintMode::Auto,
                    _ => unreachable!(),
                };
                let loaded = outlines.load_with_hint(GlyphId(gid), 16, mode, target, &mut scratch);
                assert!(loaded.is_ok(), "{engine} {name} {gid}: {loaded:?}");
                let (_, advance) = loaded.unwrap();
                let mut actual = format!("advance {advance} 0\n");
                assert_eq!(scratch.points().len(), scratch.point_tags().len());
                for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                    writeln!(actual, "point {} {} {tag}", point.x, point.y).unwrap();
                }
                for end in scratch.contour_ends() {
                    writeln!(actual, "end {end}").unwrap();
                }
                assert_eq!(
                    actual,
                    String::from_utf8(reference.stdout).unwrap(),
                    "{engine} {name} {gid}"
                );
            }
        }
    }
}
