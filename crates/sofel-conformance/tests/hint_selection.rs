//! Compare the loaded outline and advance before rendering (sofel ADR 0064 I1).
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_conformance::paths;
use sofel_font::{Font, read::GlyphId};
use sofel_outline::{HintMode, HintTarget, OutlineError, Outlines, Scratch};
use std::{fmt::Write, process::Command};

#[test]
fn adr0085_a2_harfbuzz_reference_assigns_gsub_style() {
    let styles = sofel_outline::AutoHintCache::style_specs();
    assert_eq!(styles.len(), 87);
    assert_eq!(styles[57].script, *b"Latn");
    assert_eq!(styles[57].feature, None);
    assert!(!styles[57].blue_strings.is_empty());
    let path = paths::testdata().join("extra-fonts/fonts/source-serif.otf");
    let hb_reference = paths::ft_dump_hb();
    let default = Command::new(paths::ft_dump())
        .args(["--auto-style", "109"])
        .arg(&path)
        .output()
        .unwrap();
    let hb = Command::new(hb_reference)
        .args(["--auto-style", "109"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(default.status.success());
    assert!(hb.status.success());
    assert_eq!(default.stdout, b"style 86\n");
    assert_eq!(hb.stdout, b"style 57\n");
}

#[test]
fn adr0085_i2_gsub_default_closure_assigns_unmapped_latin_glyph() {
    let path = paths::testdata().join("extra-fonts/fonts/source-serif.otf");
    let data = std::fs::read(path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let requests = sofel_outline::AutoHintCache::style_specs();
    let requests: Vec<_> = requests
        .iter()
        .map(|style| sofel_shape::AutoHintStyleRequest {
            index: style.index,
            script: style.script,
            ranges: style.ranges,
            nonbase_ranges: style.nonbase_ranges,
            feature: style.feature,
            blue_strings: &style.blue_strings,
            standard: style.standard,
        })
        .collect();
    let styles = sofel_shape::auto_hint_gsub_styles(&font, &requests, 86, 86);
    assert_eq!(styles[109], 57);
}

#[test]
fn adr0085_i2_gsub_style_words_match_harfbuzz_reference() {
    let reference = paths::ft_dump_hb();
    for relative in [
        "extra-fonts/fonts/source-serif.otf",
        "harfbuzz/perf/fonts/Roboto-Regular.ttf",
        "harfbuzz/perf/fonts/Amiri-Regular.ttf",
        "extra-fonts/fonts/amstelvar.ttf",
        "harfbuzz/perf/fonts/NotoSansDuployan-Regular.otf",
        "extra-fonts/fonts/devanagari.ttf",
        "extra-fonts/fonts/hebrew.ttf",
        "extra-fonts/fonts/khmer.ttf",
        "extra-fonts/fonts/myanmar.ttf",
        "extra-fonts/fonts/arabic.ttf",
        "extra-fonts/fonts/stix-math.otf",
        "harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf",
        "harfbuzz/test/api/fonts/Mplus1p-Regular.660E.ttf",
    ] {
        let path = paths::testdata().join(relative);
        let data = std::fs::read(&path).unwrap();
        let font = Font::new(&data, 0).unwrap();
        let specs = sofel_outline::AutoHintCache::style_specs();
        let requests: Vec<_> = specs
            .iter()
            .map(|style| sofel_shape::AutoHintStyleRequest {
                index: style.index,
                script: style.script,
                ranges: style.ranges,
                nonbase_ranges: style.nonbase_ranges,
                feature: style.feature,
                blue_strings: &style.blue_strings,
                standard: style.standard,
            })
            .collect();
        let actual = sofel_shape::auto_hint_gsub_styles(&font, &requests, 86, 57);
        let output = Command::new(&reference)
            .arg("--auto-styles")
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let expected = String::from_utf8(output.stdout).unwrap();
        let mut mismatches = std::collections::BTreeMap::new();
        for (glyph, line) in expected.lines().enumerate() {
            let style = line
                .split_ascii_whitespace()
                .last()
                .unwrap()
                .parse::<u16>()
                .unwrap();
            if actual[glyph] != style {
                *mismatches.entry((actual[glyph], style)).or_insert(0usize) += 1;
            }
        }
        assert!(mismatches.is_empty(), "{relative}: {mismatches:?}");
    }
}

#[test]
fn adr0085_i2_shaped_blue_zone_slot_matches_harfbuzz_reference() {
    let path = paths::testdata().join("extra-fonts/fonts/source-serif.otf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let specs = sofel_outline::AutoHintCache::style_specs();
    let requests: Vec<_> = specs
        .iter()
        .map(|style| sofel_shape::AutoHintStyleRequest {
            index: style.index,
            script: style.script,
            ranges: style.ranges,
            nonbase_ranges: style.nonbase_ranges,
            feature: style.feature,
            blue_strings: &style.blue_strings,
            standard: style.standard,
        })
        .collect();
    let (glyph_styles, clusters) = sofel_shape::auto_hint_shaping_data(
        outlines.instance(),
        &|glyph| outlines.glyph_extents(glyph),
        &requests,
        86,
        86,
    );
    let reverse = sofel_shape::auto_hint_reverse_entries(
        &font,
        &requests,
        &sofel_outline::AutoHintCache::adjustment_codepoints(),
    );
    assert!(reverse.contains(&(14, 1190, 291)));
    assert!(reverse.contains(&(14, 1342, 944)));
    assert!(
        !reverse
            .iter()
            .any(|&(style, glyph, _)| style == 14 && glyph == 1452)
    );
    assert!(
        reverse
            .iter()
            .any(|&(style, glyph, codepoint)| style == 14 && glyph == 1121 && codepoint == 105)
    );
    let data = sofel_outline::AutoHintShapingData {
        glyph_styles,
        reverse,
        clusters: clusters
            .into_iter()
            .map(|cluster| sofel_outline::AutoHintShapedCluster {
                style: cluster.style,
                text: cluster.text,
                glyphs: cluster
                    .glyphs
                    .into_iter()
                    .map(|glyph| sofel_outline::AutoHintShapedGlyph {
                        glyph: glyph.glyph,
                        x_advance: glyph.x_advance,
                        y_offset: glyph.y_offset,
                    })
                    .collect(),
            })
            .collect(),
    };
    let mut cache = sofel_outline::AutoHintCache::new(&outlines)
        .with_shaping_data(data)
        .unwrap();
    let mut scratch = Scratch::default();
    for (target_name, target) in [
        ("normal", HintTarget::Normal),
        ("light", HintTarget::Light),
        ("mono", HintTarget::Mono),
    ] {
        for gid in [109, 547, 1121] {
            let reference = Command::new(paths::ft_dump_hb())
                .args(["--hint-slot", "14", &gid.to_string(), "auto", target_name])
                .arg(&path)
                .output()
                .unwrap();
            assert!(reference.status.success());
            let (_, advance) = outlines
                .load_auto_hinted(GlyphId(gid), 14, target, &mut cache, &mut scratch)
                .unwrap();
            let mut actual = format!("advance {advance} 0\n");
            for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
                writeln!(actual, "point {} {} {tag}", point.x, point.y).unwrap();
            }
            for end in scratch.contour_ends() {
                writeln!(actual, "end {end}").unwrap();
            }
            assert_eq!(actual.as_bytes(), reference.stdout, "{target_name} {gid}");
        }
    }
}

#[test]
fn adr0085_i2_cli_gsub_load_matches_harfbuzz_reference() {
    let path = paths::testdata().join("extra-fonts/fonts/source-serif.otf");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let args = [
        "--gsub", "--ppem", "14", "--glyph", "547", "--target", "normal",
    ]
    .into_iter()
    .map(std::ffi::OsString::from)
    .chain(core::iter::once(path.as_os_str().to_owned()))
    .collect::<Vec<_>>();
    assert_eq!(
        sofel_cli::hint::run(&args, &mut stdout, &mut stderr),
        0,
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    let reference = Command::new(paths::ft_dump_hb())
        .args(["--hint-slot", "14", "547", "auto", "normal"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    assert_eq!(stdout, reference.stdout);
}

#[path = "../../sofel-read/tests/support/build.rs"]
mod cff_build;

#[test]
fn adr0064_i1_native_cff_matches_freetype() {
    use cff_build::{Cff1Spec, Cs, cff1, charstring, otto_font};
    let glyphs = vec![
        charstring(&[Cs::Op(14)]),
        charstring(&[
            Cs::I(0),
            Cs::I(0),
            Cs::Op(21),
            Cs::I(500),
            Cs::Op(6),
            Cs::I(700),
            Cs::Op(7),
            Cs::Op(14),
        ]),
    ];
    let bytes = otto_font(2, vec![(b"CFF ", cff1(&Cff1Spec::new(glyphs)))]);
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0064-native-cff-{}.otf",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    for ppem in 1..=34 {
        for (target_name, target) in [
            ("normal", HintTarget::Normal),
            ("light", HintTarget::Light),
            ("mono", HintTarget::Mono),
            ("lcd", HintTarget::Lcd),
            ("lcd-v", HintTarget::VerticalLcd),
        ] {
            let reference = Command::new(paths::ft_dump())
                .args(["--hint-slot", &ppem.to_string(), "1", "native", target_name])
                .arg(&path)
                .output()
                .unwrap();
            assert!(reference.status.success());
            let (_, advance) = outlines
                .load_with_hint(GlyphId(1), ppem, HintMode::NativeV35, target, &mut scratch)
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
    std::fs::remove_file(path).unwrap();

    // Stems, CFF2 and blends take the same path (sofel ADR 0064).
    for (relative, gid) in [
        ("extra-fonts/fonts/source-serif.otf", 36),
        ("harfbuzz/test/subset/data/fonts/AdobeVFPrototype.otf", 1),
    ] {
        let path = paths::testdata().join(relative);
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let outlines = Outlines::new(font.default_instance());
        for (engine, mode) in [("v35", HintMode::NativeV35), ("v40", HintMode::NativeV40)] {
            let reference = Command::new(paths::ft_dump())
                .args(["--hint-slot", "16", &gid.to_string(), engine, "light"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(reference.status.success());
            let (_, advance) = outlines
                .load_with_hint(GlyphId(gid), 16, mode, HintTarget::Light, &mut scratch)
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
                "{relative} {engine}"
            );
        }
    }
    assert_eq!(
        Outlines::new(Font::new(&bytes, 0).unwrap().default_instance()).load_with_hint(
            GlyphId(9),
            16,
            HintMode::NativeV35,
            HintTarget::Light,
            &mut scratch,
        ),
        Err(OutlineError::InvalidGlyphIndex),
    );
    assert!(scratch.points().is_empty());
}

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
        ("harfbuzz/perf/fonts/Roboto-Regular.ttf", 46, 120),
        (
            "harfbuzz/test/subset/data/fonts/Molengo-Regular.ttf",
            44,
            120,
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
    }
}

#[test]
fn adr0064_i1_latin_light_edges_match_freetype() {
    for (relative, gid, max_ppem) in [
        ("harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf", 3, 200),
        (
            "harfbuzz/test/subset/data/expected/full_instance/Roboto-Variable.default.all.wght=150,wdth=80.ttf",
            70,
            120,
        ),
        (
            "harfbuzz/test/subset/data/expected/full_instance/Roboto-Variable.default.all.wght=300,wdth=90.ttf",
            70,
            120,
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
    }
    // A variable font (gvar) at its default location.
    let path = paths::testdata().join("harfbuzz/test/subset/data/fonts/Roboto-Variable.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let glyph = font.nominal_glyph(u32::from('b')).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    let (_, advance) = outlines
        .load_with_hint(glyph, 16, HintMode::Auto, HintTarget::Light, &mut scratch)
        .unwrap();
    let reference = Command::new(paths::ft_dump())
        .args(["--hint-slot", "16", &glyph.0.to_string(), "auto", "light"])
        .arg(path)
        .output()
        .unwrap();
    assert!(reference.status.success());
    let mut actual = format!("advance {advance} 0\n");
    for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
        writeln!(actual, "point {} {} {tag}", point.x, point.y).unwrap();
    }
    for end in scratch.contour_ends() {
        writeln!(actual, "end {end}").unwrap();
    }
    assert_eq!(actual, String::from_utf8(reference.stdout).unwrap());
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
fn adr0064_i1_auto_varc_glyph_reports_unsupported_and_clears_scratch() {
    // FreeType never auto-hints a glyph that VARC covers (ftobjs.c).
    let path = paths::testdata().join("harfbuzz/test/api/fonts/varc-ac00-ac01.ttf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    let glyph = font.nominal_glyph(0xAC00).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    // Fill the scratch from another font so that clearing it shows.
    let other =
        std::fs::read(paths::testdata().join("harfbuzz/perf/fonts/Roboto-Regular.ttf")).unwrap();
    let other = Font::new(&other, 0).unwrap();
    Outlines::new(other.default_instance())
        .load_with_hint(
            GlyphId(36),
            16,
            HintMode::None,
            HintTarget::Normal,
            &mut scratch,
        )
        .unwrap();
    assert!(!scratch.points().is_empty());
    assert_eq!(
        outlines.load_with_hint(glyph, 16, HintMode::Auto, HintTarget::Normal, &mut scratch),
        Err(OutlineError::AutoHintUnsupported)
    );
    assert!(scratch.points().is_empty());
    assert!(scratch.point_tags().is_empty());
    assert!(scratch.contour_ends().is_empty());
}

#[test]
fn adr0064_i1_auto_styles_match_freetype() {
    // The style of every glyph, from the Unicode cmap without HarfBuzz,
    // as FreeType's glyph-to-script-map gives it before any glyph loads.
    let mut compared = 0;
    for relative in [
        "harfbuzz/test/api/fonts/Roboto-Regular.abcAE.ttf",
        "harfbuzz/test/api/fonts/Mplus1p-Regular.660E.ttf",
        "harfbuzz/perf/fonts/Roboto-Regular.ttf",
        "harfbuzz/perf/fonts/Amiri-Regular.ttf",
        "extra-fonts/fonts/devanagari.ttf",
        "extra-fonts/fonts/khmer.ttf",
        "extra-fonts/fonts/myanmar.ttf",
        "extra-fonts/fonts/hebrew.ttf",
        "extra-fonts/fonts/source-serif.otf",
        "extra-fonts/fonts/stix-math.otf",
    ] {
        let path = paths::testdata().join(relative);
        let data = std::fs::read(&path).unwrap();
        let font = Font::new(&data, 0).unwrap();
        let outlines = Outlines::new(font.default_instance());
        let cache = sofel_outline::AutoHintCache::new(&outlines);
        let reference = Command::new(paths::ft_dump())
            .arg("--auto-styles")
            .arg(&path)
            .output()
            .unwrap();
        assert!(reference.status.success());
        let mut actual = String::new();
        for glyph in 0..font.num_glyphs() {
            let style = cache.glyph_style(GlyphId(glyph)).unwrap();
            writeln!(actual, "style {glyph} {style}").unwrap();
            compared += 1;
        }
        assert_eq!(
            actual,
            String::from_utf8(reference.stdout).unwrap(),
            "{relative}"
        );
    }
    assert!(compared > 10_000);
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
