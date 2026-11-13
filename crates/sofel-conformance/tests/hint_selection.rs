//! Compare the loaded outline and advance before rendering (sofel ADR 0064 I1).
#![allow(clippy::unwrap_used)]

use sofel_conformance::paths;
use sofel_font::{Font, read::GlyphId};
use sofel_outline::{HintMode, HintTarget, OutlineError, Outlines, Scratch};
use std::{fmt::Write, process::Command};

#[test]
fn adr0064_i1_load_selection_matches_freetype() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    for (target_name, target) in [
        ("normal", HintTarget::Normal),
        ("light", HintTarget::Light),
        ("mono", HintTarget::Mono),
        ("lcd", HintTarget::Lcd),
        ("lcd-v", HintTarget::VerticalLcd),
    ] {
        for (engine, mode) in [
            ("none", HintMode::None),
            ("v35", HintMode::NativeV35),
            ("v40", HintMode::NativeV40),
        ] {
            for gid in [1, 4, 5, 7] {
                let reference = Command::new(paths::ft_dump())
                    .args(["--hint-slot", "16", &gid.to_string(), engine, target_name])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    reference.status.success(),
                    "{}",
                    String::from_utf8_lossy(&reference.stderr)
                );
                let (_, advance) = outlines
                    .load_with_hint(GlyphId(gid), 16, mode, target, &mut scratch)
                    .unwrap();
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
                    "{engine} {target_name} {gid}"
                );
            }
        }
    }
}

#[test]
fn adr0064_i1_auto_empty_slots_match_freetype() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    for ppem in [8, 16, 24] {
        for (target_name, target) in [
            ("normal", HintTarget::Normal),
            ("light", HintTarget::Light),
            ("mono", HintTarget::Mono),
            ("lcd", HintTarget::Lcd),
            ("lcd-v", HintTarget::VerticalLcd),
        ] {
            for gid in [0, 1] {
                let reference = Command::new(paths::ft_dump())
                    .args([
                        "--hint-slot",
                        &ppem.to_string(),
                        &gid.to_string(),
                        "auto",
                        target_name,
                    ])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    reference.status.success(),
                    "{}",
                    String::from_utf8_lossy(&reference.stderr)
                );
                let (_, advance) = outlines
                    .load_with_hint(GlyphId(gid), ppem, HintMode::Auto, target, &mut scratch)
                    .unwrap();
                assert!(scratch.points().is_empty());
                assert!(scratch.contour_ends().is_empty());
                assert_eq!(
                    format!("advance {advance} 0\n"),
                    String::from_utf8(reference.stdout).unwrap(),
                    "{target_name} {gid} {ppem}"
                );
            }
        }
    }
}

#[test]
fn adr0064_i1_auto_nonempty_reports_unsupported_and_clears_scratch() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    outlines
        .load_with_hint(
            GlyphId(2),
            16,
            HintMode::None,
            HintTarget::Normal,
            &mut scratch,
        )
        .unwrap();
    assert!(!scratch.points().is_empty());
    assert_eq!(
        outlines.load_with_hint(
            GlyphId(2),
            16,
            HintMode::Auto,
            HintTarget::Normal,
            &mut scratch
        ),
        Err(OutlineError::AutoHintUnsupported)
    );
    assert!(scratch.points().is_empty());
    assert!(scratch.point_tags().is_empty());
    assert!(scratch.contour_ends().is_empty());
}

#[test]
fn adr0064_i1_reference_auto_style_distinguishes_cmap_and_fallback() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    for (gid, expected) in [
        (0, 86),
        (1, 8249),
        (2, 8249),
        (3, 8249),
        (4, 8249),
        (5, 86),
        (6, 86),
        (7, 8249),
    ] {
        let output = Command::new(paths::ft_dump())
            .args(["--auto-style", &gid.to_string()])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("style {expected}\n")
        );
    }
}
