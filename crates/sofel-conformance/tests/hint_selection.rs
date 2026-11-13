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
fn adr0064_i1_auto_unlinked_fallback_outline_matches_freetype() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    for ppem in 1..=200 {
        for (target_name, target) in [
            ("normal", HintTarget::Normal),
            ("light", HintTarget::Light),
            ("mono", HintTarget::Mono),
            ("lcd", HintTarget::Lcd),
            ("lcd-v", HintTarget::VerticalLcd),
        ] {
            let reference = Command::new(paths::ft_dump())
                .args(["--hint-slot", &ppem.to_string(), "5", "auto", target_name])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            let (_, advance) = outlines
                .load_with_hint(GlyphId(5), ppem, HintMode::Auto, target, &mut scratch)
                .unwrap();
            let mut actual = format!("advance {advance} 0\n");
            for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                writeln!(actual, "point {} {} {tag}", point.x, point.y).unwrap();
            }
            for end in scratch.contour_ends() {
                writeln!(actual, "end {end}").unwrap();
            }
            assert_eq!(
                actual,
                String::from_utf8(reference.stdout).unwrap(),
                "{target_name} {ppem}"
            );
        }
    }
}

#[test]
fn adr0064_i1_latin_cap_rectangle_edges_match_freetype() {
    for (relative, gid, max_ppem) in [
        ("harfbuzz/perf/fonts/Roboto-Regular.ttf", 46, 76),
        (
            "harfbuzz/test/subset/data/fonts/Molengo-Regular.ttf",
            44,
            86,
        ),
    ] {
        let path = paths::testdata().join(relative);
        let data = std::fs::read(&path).unwrap();
        let font = Font::new(&data, 0).unwrap();
        assert_eq!(font.nominal_glyph(u32::from('I')), Some(GlyphId(gid)));
        let outlines = Outlines::new(font.default_instance());
        let mut scratch = Scratch::default();
        for ppem in 1..=max_ppem {
            for (target_name, target) in [("light", HintTarget::Light), ("lcd", HintTarget::Lcd)] {
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
                assert!(reference.status.success());
                let (_, advance) = outlines
                    .load_with_hint(GlyphId(gid), ppem, HintMode::Auto, target, &mut scratch)
                    .unwrap();
                let mut actual = format!("advance {advance} 0\n");
                for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                    writeln!(actual, "point {} {} {tag}", point.x, point.y).unwrap();
                }
                for end in scratch.contour_ends() {
                    writeln!(actual, "end {end}").unwrap();
                }
                assert_eq!(
                    actual,
                    String::from_utf8(reference.stdout).unwrap(),
                    "{relative} {target_name} {ppem}"
                );
            }
        }
        assert_eq!(
            outlines.load_with_hint(
                GlyphId(gid),
                max_ppem + 1,
                HintMode::Auto,
                HintTarget::Light,
                &mut scratch
            ),
            Err(OutlineError::AutoHintUnsupported),
            "{relative}"
        );
    }
}

#[test]
fn adr0064_i1_latin_light_edges_match_freetype() {
    for (relative, gid, max_ppem) in [
        ("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf", 3, 200),
        (
            "harfbuzz/test/subset/data/expected/full_instance/Roboto-Variable.default.all.wght=150,wdth=80.ttf",
            70,
            34,
        ),
        (
            "harfbuzz/test/subset/data/expected/full_instance/Roboto-Variable.default.all.wght=300,wdth=90.ttf",
            70,
            34,
        ),
    ] {
        let path = paths::testdata().join(relative);
        let data = std::fs::read(&path).unwrap();
        let font = Font::new(&data, 0).unwrap();
        assert_eq!(font.nominal_glyph(u32::from('b')), Some(GlyphId(gid)));
        let outlines = Outlines::new(font.default_instance());
        let mut scratch = Scratch::default();
        for ppem in 1..=max_ppem {
            for (target_name, target) in [("light", HintTarget::Light), ("lcd", HintTarget::Lcd)] {
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
                assert!(reference.status.success());
                let (_, advance) = outlines
                    .load_with_hint(GlyphId(gid), ppem, HintMode::Auto, target, &mut scratch)
                    .unwrap();
                let mut actual = format!("advance {advance} 0\n");
                for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                    writeln!(actual, "point {} {} {tag}", point.x, point.y).unwrap();
                }
                for end in scratch.contour_ends() {
                    writeln!(actual, "end {end}").unwrap();
                }
                assert_eq!(
                    actual,
                    String::from_utf8(reference.stdout).unwrap(),
                    "{relative} {target_name} {ppem}"
                );
            }
        }
        if max_ppem == 34 {
            assert_eq!(
                outlines.load_with_hint(
                    GlyphId(gid),
                    35,
                    HintMode::Auto,
                    HintTarget::Light,
                    &mut scratch
                ),
                Err(OutlineError::AutoHintUnsupported)
            );
        }
    }
    let variable_path =
        paths::testdata().join("harfbuzz/test/subset/data/fonts/Roboto-Variable.ttf");
    let variable_data = std::fs::read(&variable_path).unwrap();
    let variable_font = Font::new(&variable_data, 0).unwrap();
    let variable_b = variable_font.nominal_glyph(u32::from('b')).unwrap();
    let variable_outlines = Outlines::new(variable_font.default_instance());
    let mut scratch = Scratch::default();
    assert_eq!(
        variable_outlines.load_with_hint(
            variable_b,
            16,
            HintMode::Auto,
            HintTarget::Light,
            &mut scratch
        ),
        Err(OutlineError::AutoHintUnsupported)
    );
    assert!(scratch.points().is_empty());
}

#[test]
fn adr0064_i1_latin_d_linked_stems_match_freetype() {
    for relative in [
        "harfbuzz/test/subset/data/expected/full_instance/Roboto-Variable.default.all.wght=150,wdth=80.ttf",
        "harfbuzz/test/subset/data/expected/full_instance/Roboto-Variable.default.all.wght=300,wdth=90.ttf",
    ] {
        let path = paths::testdata().join(relative);
        let data = std::fs::read(&path).unwrap();
        let font = Font::new(&data, 0).unwrap();
        let gid = font.nominal_glyph(u32::from('d')).unwrap();
        assert_eq!(gid, GlyphId(72));
        let outlines = Outlines::new(font.default_instance());
        let mut scratch = Scratch::default();
        for ppem in 1..=34 {
            for (target_name, target) in [("light", HintTarget::Light), ("lcd", HintTarget::Lcd)] {
                let reference = Command::new(paths::ft_dump())
                    .args(["--hint-slot", &ppem.to_string(), "72", "auto", target_name])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(reference.status.success());
                let (_, advance) = outlines
                    .load_with_hint(gid, ppem, HintMode::Auto, target, &mut scratch)
                    .unwrap();
                let mut actual = format!("advance {advance} 0\n");
                for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                    writeln!(actual, "point {} {} {tag}", point.x, point.y).unwrap();
                }
                for end in scratch.contour_ends() {
                    writeln!(actual, "end {end}").unwrap();
                }
                assert_eq!(
                    actual,
                    String::from_utf8(reference.stdout).unwrap(),
                    "{relative} {target_name} {ppem}"
                );
            }
        }
    }
}

#[test]
fn adr0064_i1_auto_non_latin_reports_unsupported_and_clears_scratch() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/Mplus1p-Regular.660E.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    outlines
        .load_with_hint(
            GlyphId(1),
            16,
            HintMode::None,
            HintTarget::Normal,
            &mut scratch,
        )
        .unwrap();
    assert!(!scratch.points().is_empty());
    assert_eq!(
        outlines.load_with_hint(
            GlyphId(1),
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
