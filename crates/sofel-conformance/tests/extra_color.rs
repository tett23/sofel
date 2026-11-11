//! Color and bitmap roles of the fixed font corpus (ADR 0031, I05-I06).

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use sofel_conformance::color::{self, compare_color, compare_paint};
use sofel_conformance::expect;
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::{Tag, fvar::Fvar};
use std::collections::BTreeSet;
use std::ffi::OsString;
#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

fn font(name: &str) -> Vec<u8> {
    std::fs::read(paths::testdata().join("extra-fonts/fonts").join(name)).unwrap()
}

fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &byte| {
        (h ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
    })
}

struct VariableColorCase<'a> {
    id: &'a str,
    glyph: u32,
    coordinates: &'a str,
    reference: u64,
}

fn variable_color_cases() -> Vec<VariableColorCase<'static>> {
    include_str!("../../../conformance/extra-color-var-cases.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), 4, "{line}");
            VariableColorCase {
                id: fields[0],
                glyph: fields[1].parse().unwrap(),
                coordinates: fields[2],
                reference: u64::from_str_radix(fields[3], 16).unwrap(),
            }
        })
        .collect()
}

fn variable_color_font() -> Vec<u8> {
    std::fs::read(
        paths::testdata().join("harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf"),
    )
    .unwrap()
}

#[test]
fn i05_variable_colr_cases_cover_each_axis_and_change_paint() {
    let bytes = variable_color_font();
    let font = Font::new(&bytes, 0).unwrap();
    let fvar = Fvar::new(font.table(Tag::new(b"fvar")).unwrap()).unwrap();
    let cases = variable_color_cases();
    let ids = cases.iter().map(|case| case.id).collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), cases.len());
    assert_eq!(fvar.axis_count(), 44);
    assert_eq!(cases.len(), 1 + fvar.axis_count() * 2 + 2);
    assert!(ids.contains("variable-colr.default"));
    assert!(ids.contains("variable-colr.all-min"));
    assert!(ids.contains("variable-colr.all-max"));
    let mut changed = 0;
    for axis in fvar.axes() {
        let tag = String::from_utf8(axis.tag.0.to_vec()).unwrap();
        for (edge, value) in [("min", axis.min), ("max", axis.max)] {
            let id = format!("variable-colr.axis.{tag}.{edge}");
            let case = cases.iter().find(|case| case.id == id).unwrap();
            let setting = case.coordinates.strip_prefix(&format!("{tag}=")).unwrap();
            let fixed = (setting.parse::<f64>().unwrap() * 65536.0).round() as i32;
            assert_eq!(fixed, value.0, "{id}");
            let default = cases
                .iter()
                .find(|default| default.id == "variable-colr.default")
                .unwrap();
            if case.glyph == default.glyph && case.reference != default.reference {
                changed += 1;
            }
        }
    }
    assert!(
        changed > 0,
        "no individual axis changes the paint reference"
    );
}

#[test]
fn i06_variable_colr_matches_reference_or_named_failure() {
    let bytes = variable_color_font();
    let font = Font::new(&bytes, 0).unwrap();
    let file =
        paths::testdata().join("harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf");
    let cases = variable_color_cases();
    let expected = expect::parse(include_str!(
        "../../../conformance/expected-failures-extra-color-var.txt"
    ));
    let reasons = expect::parse_reasons(include_str!(
        "../../../conformance/expected-failures-extra-color-var.txt"
    ));
    let mut scratch = sofel_color::Scratch::default();
    let mut results = Vec::new();
    for case in &cases {
        let (ok, code, err) = if case.coordinates == "-" {
            let actual = color::sofel_paint_text(&font, case.glyph, 0, 0, 0.0, &mut scratch);
            (
                digest(actual.as_bytes()) == case.reference,
                0,
                String::new(),
            )
        } else {
            let args = [
                OsString::from("--glyph"),
                OsString::from(case.glyph.to_string()),
                OsString::from("--variations"),
                OsString::from(case.coordinates),
                file.as_os_str().to_os_string(),
            ];
            let mut out = Vec::new();
            let mut err = Vec::new();
            let code = sofel_cli::paint::run(&args, &mut out, &mut err);
            (
                code == 0 && digest(&out) == case.reference,
                code,
                String::from_utf8(err).unwrap(),
            )
        };
        if !ok && reasons.contains_key(case.id) {
            assert_eq!(code, 2, "{}: {err}", case.id);
            assert!(
                err.contains("unknown option --variations"),
                "{}: {err}",
                case.id
            );
            assert!(reasons[case.id].starts_with("variations:"), "{}", case.id);
        }
        results.push((case.id, ok));
    }
    let problems = expect::judge(results, &expected);
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(
            &problems,
            "conformance/expected-failures-extra-color-var.txt"
        )
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn i06_variable_colr_reference_matches_pinned_harfbuzz() {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap();
    let file =
        paths::testdata().join("harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf");
    for case in variable_color_cases() {
        let output = Command::new(&exe)
            .arg("--paint-var")
            .arg(&file)
            .arg(case.glyph.to_string())
            .arg(case.coordinates)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", case.id);
        let actual = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            actual.trim(),
            format!("{:016x}", case.reference),
            "{}",
            case.id
        );
    }
}

#[test]
fn i05_cff_colr_paint_matches_harfbuzz() {
    let bytes = font("color-cff.otf");
    let font = Font::new(&bytes, 0).unwrap();
    let reference = include_str!("../../../conformance/extra-color-cff-paint-hb.txt");
    let faces = color::parse_paint(reference).unwrap();
    assert_eq!(faces.len(), 1);
    let paints = &faces[0];
    assert!(paints.paints.iter().any(|(_, _, glyph, _)| *glyph == 19));
    let items = compare_paint("color-cff", &font, paints);
    assert_eq!(items.len(), paints.paints.len());
    assert!(
        items.iter().all(|item| item.ok),
        "{:?}",
        items
            .iter()
            .filter(|item| !item.ok)
            .map(|item| (&item.id, &item.detail))
            .collect::<Vec<_>>()
    );
}

#[test]
fn i05_sbix_bitmap_matches_harfbuzz() {
    let bytes = font("color-sbix.ttf");
    let font = Font::new(&bytes, 0).unwrap();
    let color = include_str!("../../../conformance/extra-color-sbix-hb.txt");
    let item = compare_color("color-sbix", 0, &font, color);
    assert!(item.ok, "{}", item.detail);
    let paint = include_str!("../../../conformance/extra-color-sbix-paint-hb.txt");
    let faces = color::parse_paint(paint).unwrap();
    assert_eq!(faces.len(), 1);
    assert!(
        faces[0]
            .paints
            .iter()
            .any(|(_, ppem, glyph, text)| *ppem == 109
                && *glyph == 19
                && text.contains("image type png"))
    );
    let items = compare_paint("color-sbix", &font, &faces[0]);
    assert!(
        items.iter().all(|item| item.ok),
        "{:?}",
        items
            .iter()
            .filter(|item| !item.ok)
            .map(|item| (&item.id, &item.detail))
            .collect::<Vec<_>>()
    );
}

#[test]
fn i05_sbix_png_bytes_match_harfbuzz() {
    let bytes = font("color-sbix.ttf");
    let font = Font::new(&bytes, 0).unwrap();
    let reference = include_str!("../../../conformance/extra-color-sbix-png-hb.txt");
    let mut found = 0;
    for line in reference.lines() {
        let parts = line.split(' ').collect::<Vec<_>>();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "glyph");
        let glyph = parts[1].parse::<u32>().unwrap();
        let len = parts[2].parse::<usize>().unwrap();
        let reference_digest = u64::from_str_radix(parts[3], 16).unwrap();
        let png = sofel_bitmap::sbix_png_data(&font, glyph, 0);
        assert_eq!(png.map_or(0, <[u8]>::len), len, "glyph {glyph}");
        let actual = png.map_or(0, digest);
        assert_eq!(actual, reference_digest, "glyph {glyph}");
        found += usize::from(len > 0);
    }
    assert!(found > 0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn i06_color_references_match_pinned_harfbuzz() {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap();
    for (mode, file, reference) in [
        (
            "--paint",
            "color-cff.otf",
            include_str!("../../../conformance/extra-color-cff-paint-hb.txt"),
        ),
        (
            "--color",
            "color-sbix.ttf",
            include_str!("../../../conformance/extra-color-sbix-hb.txt"),
        ),
        (
            "--paint",
            "color-sbix.ttf",
            include_str!("../../../conformance/extra-color-sbix-paint-hb.txt"),
        ),
        (
            "--png-digest",
            "color-sbix.ttf",
            include_str!("../../../conformance/extra-color-sbix-png-hb.txt"),
        ),
    ] {
        let path = paths::testdata().join("extra-fonts/fonts").join(file);
        let output = Command::new(&exe).arg(mode).arg(path).output().unwrap();
        assert!(output.status.success(), "{mode} {file}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            reference,
            "{mode} {file}"
        );
    }
}

#[test]
fn adr0027_named_paint_samples_match_harfbuzz_at_each_location() {
    let bytes = variable_color_font();
    let font = Font::new(&bytes, 0).unwrap();
    for case in variable_color_cases() {
        let variations: Vec<_> = case
            .coordinates
            .split(',')
            .filter(|v| *v != "-")
            .map(|v| sofel_font::var::Variation::parse(v).unwrap())
            .collect();
        let location = sofel_font::var::Location::new(&font, None, &variations);
        let actual = color::sofel_paint_text_at(
            font.instance(&location),
            case.glyph,
            0,
            0,
            0.0,
            &mut sofel_color::Scratch::default(),
        );
        assert_eq!(digest(actual.as_bytes()), case.reference, "{}", case.id);
    }
}
