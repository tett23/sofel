//! Synthetic GSUB feature coverage for root ADR 0085 I2.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "../../sofel-shape/tests/support/mod.rs"]
mod support;

use sofel_font::Font;
use sofel_outline::AutoHintCache;
use std::process::Command;

#[test]
fn adr0085_i2_synthetic_smcp_sups_subs_and_unreferenced_lookup_match_freetype() {
    let map: Vec<_> = (32..=126)
        .map(|codepoint| (codepoint, u16::try_from(codepoint - 31).unwrap()))
        .collect();
    let source = |character: u8| u16::from(character) - 31;
    let gsub = support::layout(
        &[(b"latn", vec![support::lang(None, &[0, 1, 2])])],
        &[(b"smcp", vec![0]), (b"subs", vec![1]), (b"sups", vec![2])],
        &[
            support::lookup(1, 0, vec![support::single_subst(&[(source(b'f'), 100)])]),
            support::lookup(1, 0, vec![support::single_subst(&[(source(b'i'), 101)])]),
            support::lookup(1, 0, vec![support::single_subst(&[(source(b'j'), 102)])]),
            support::lookup(1, 0, vec![support::single_subst(&[(source(b'b'), 103)])]),
        ],
    );
    let font_bytes = support::font(
        104,
        &map,
        &[500; 104],
        vec![
            (b"GSUB", gsub),
            (b"glyf", Vec::new()),
            (b"loca", vec![0; 210]),
        ],
    );
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0085-synthetic-{}.ttf",
        std::process::id()
    ));
    std::fs::write(&path, &font_bytes).unwrap();
    let reference = sofel_conformance::paths::ft_dump_hb();
    let output = Command::new(reference)
        .arg("--auto-styles")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let font = Font::new(&font_bytes, 0).unwrap();
    let specs = AutoHintCache::style_specs();
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
    let expected: Vec<u16> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            line.split_ascii_whitespace()
                .last()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect();
    assert_eq!(actual, expected);
    let style = |feature: &[u8; 4]| {
        specs
            .iter()
            .find(|style| style.script == *b"Latn" && style.feature == Some(*feature))
            .unwrap()
            .index
    };
    assert_eq!(actual[100] & 0x1fff, style(b"smcp"));
    assert_eq!(actual[101] & 0x1fff, style(b"subs"));
    assert_eq!(actual[102] & 0x1fff, style(b"sups"));
    assert_eq!(actual[103] & 0x1fff, 86);
    for glyph in [100, 101, 102, 103] {
        for target in ["normal", "light", "mono", "lcd", "lcd-v"] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let args = [
                "--gsub",
                "--ppem",
                "14",
                "--glyph",
                &glyph.to_string(),
                "--target",
                target,
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
            let reference = Command::new(sofel_conformance::paths::ft_dump_hb())
                .args(["--hint-slot", "14", &glyph.to_string(), "auto", target])
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            assert_eq!(stdout, reference.stdout, "glyph {glyph} target {target}");
        }
    }
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn adr0085_i1_default_cmap_only_matches_prior_reference() {
    let path = sofel_conformance::paths::testdata().join("extra-fonts/fonts/source-serif.otf");
    for glyph in [109, 547, 1121] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let args = [
            "--ppem",
            "14",
            "--glyph",
            &glyph.to_string(),
            "--target",
            "normal",
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
        let reference = Command::new(
            std::env::var_os("SOFEL_FT_DUMP").unwrap_or_else(|| "target/ft-dump/ft-dump".into()),
        )
        .args(["--hint-slot", "14", &glyph.to_string(), "auto", "normal"])
        .arg(&path)
        .output()
        .unwrap();
        assert!(reference.status.success());
        assert_eq!(stdout, reference.stdout, "glyph {glyph}");
    }
}

#[test]
fn adr0085_i2_gsub_absent_face_matches_harfbuzz_freetype() {
    let font_bytes = support::font(
        3,
        &[(u32::from(b'A'), 1), (u32::from(b'a'), 2)],
        &[500; 3],
        vec![(b"glyf", Vec::new()), (b"loca", vec![0; 8])],
    );
    let path =
        std::env::temp_dir().join(format!("sofel-adr0085-no-gsub-{}.ttf", std::process::id()));
    std::fs::write(&path, font_bytes).unwrap();
    for glyph in [1, 2] {
        for target in ["normal", "light", "mono", "lcd", "lcd-v"] {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let args = [
                "--gsub",
                "--ppem",
                "14",
                "--glyph",
                &glyph.to_string(),
                "--target",
                target,
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
            let reference = Command::new(sofel_conformance::paths::ft_dump_hb())
                .args(["--hint-slot", "14", &glyph.to_string(), "auto", target])
                .arg(&path)
                .output()
                .unwrap();
            assert!(reference.status.success());
            assert_eq!(stdout, reference.stdout, "glyph {glyph} target {target}");
        }
    }
    std::fs::remove_file(path).unwrap();
}
