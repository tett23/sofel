//! Color glyphs against HarfBuzz (ADR 0023): `hb-dump --paint`,
//! `--color` and `--extents` on every face of HarfBuzz's test fonts with
//! color tables, and the 24 expected outputs of HarfBuzz's
//! `test/api/test-paint.c` (`test/api/results-paint/`), which `hb-dump`
//! must reproduce as well. Failures not listed in
//! conformance/expected-failures-harfbuzz.txt, and listed items that
//! pass, fail the test.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::path::Path;
use std::process::Command;

use sofel_color::Scratch;
use sofel_conformance::color::{self, sofel_paint_text};
use sofel_conformance::compare::Item;
use sofel_conformance::{expect, fonts, paths};
use sofel_font::Font;

const LIST: &str = "expected-failures-harfbuzz.txt";

fn hb_dump(mode: &str, font: &Path) -> Option<String> {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe).arg(mode).arg(font).output().unwrap();
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).unwrap())
}

/// HarfBuzz's `paint_tests` of `test/api/test-paint.c`: font, slant,
/// glyph, result. The tests paint with palette 0.
/// `results-paint/` has 26 files; `test-02` and `testvf02` are not paint
/// text but logs of a test run (TAP output) that no test reads.
const PAINT_TESTS: [(&str, f32, u32, &str); 24] = [
    ("noto_handwriting-cff2_colr_1.otf", 0.0, 10, "hand-10"),
    ("noto_handwriting-cff2_colr_1.otf", 0.2, 10, "hand-10.2"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 6, "test-6"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 10, "test-10"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 92, "test-92"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 106, "test-106"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 116, "test-116"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 123, "test-123"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 154, "test-154"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 165, "test-165"),
    ("test_glyphs-glyf_colr_1.ttf", 0.0, 175, "test-175"),
    ("test_glyphs-glyf_colr_1_variable.ttf", 0.0, 6, "testvf-6"),
    ("test_glyphs-glyf_colr_1_variable.ttf", 0.0, 10, "testvf-10"),
    ("test_glyphs-glyf_colr_1_variable.ttf", 0.0, 92, "testvf-92"),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        106,
        "testvf-106",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        116,
        "testvf-116",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        123,
        "testvf-123",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        154,
        "testvf-154",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        165,
        "testvf-165",
    ),
    (
        "test_glyphs-glyf_colr_1_variable.ttf",
        0.0,
        175,
        "testvf-175",
    ),
    ("bad_colrv1.ttf", 0.0, 154, "bad-154"),
    ("RocherColorGX.abc.ttf", 0.3, 1, "rocher-1"),
    ("RocherColorGX.abc.ttf", 0.3, 2, "rocher-2"),
    ("RocherColorGX.abc.ttf", 0.0, 3, "rocher-3"),
];

/// The 24 results: sofel's text, and `hb-dump --paint=SLANT`'s.
fn results_paint_items(testdata: &Path) -> Vec<Item> {
    let api = testdata.join("harfbuzz/test/api");
    let mut items = Vec::new();
    let mut scratch = Scratch::default();
    for (file, slant, glyph, name) in PAINT_TESTS {
        let path = api.join("fonts").join(file);
        let expected = std::fs::read_to_string(api.join("results-paint").join(name)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let ours = sofel_paint_text(&font, glyph, 0, 0, slant, &mut scratch);
        items.push(Item {
            id: format!("results-paint/{name}"),
            ok: ours == expected,
            detail: format!("expected:\n{expected}sofel:\n{ours}"),
        });
        // hb-dump itself writes HarfBuzz's test text.
        let text = hb_dump(&format!("--paint={slant}"), &path).unwrap();
        let faces = color::parse_paint(&text).unwrap();
        let hb = faces[0]
            .paints
            .iter()
            .find(|(p, pp, g, _)| (*p, *pp, *g) == (0, 0, glyph))
            .map(|x| x.3.clone())
            .unwrap();
        assert_eq!(
            hb, expected,
            "hb-dump --paint does not reproduce results-paint/{name}"
        );
    }
    items
}

#[test]
fn colors_agree_with_harfbuzz() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut items = results_paint_items(&testdata);
    let color_fonts = color::color_fonts(&testdata);
    assert!(
        color_fonts.len() > 30,
        "found only {} color fonts",
        color_fonts.len()
    );
    for path in &color_fonts {
        let id = fonts::id(path, &testdata);
        let bytes = std::fs::read(path).unwrap();
        if let Some(text) = hb_dump("--paint", path) {
            for face in color::parse_paint(&text).unwrap_or_else(|e| panic!("{id}: {e}")) {
                if let Ok(font) = Font::new(&bytes, face.index) {
                    items.extend(color::compare_paint(&id, &font, &face));
                }
            }
        }
        if let Some(text) = hb_dump("--color", path) {
            for (index, part) in color::split_faces(&text) {
                if let Ok(font) = Font::new(&bytes, index) {
                    items.push(color::compare_color(&id, index, &font, &part));
                }
            }
        }
        if let Some(text) = hb_dump("--extents", path) {
            for (index, part) in color::split_faces(&text) {
                if let Ok(font) = Font::new(&bytes, index) {
                    items.extend(color::compare_extents(&id, index, &font, &part));
                }
            }
        }
    }
    let failed = items.iter().filter(|i| !i.ok).count();
    if let Some(path) = std::env::var_os("SOFEL_FAILED_ITEMS") {
        let text: String = items
            .iter()
            .filter(|i| !i.ok)
            .map(|i| format!("{}\t{}\n", i.id, i.detail.replace('\n', "|")))
            .collect();
        std::fs::write(path, text).unwrap();
    }
    eprintln!(
        "{} color fonts, {} items, {failed} failed",
        color_fonts.len(),
        items.len()
    );
    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let expected = expect::parse(&list)
        .into_iter()
        .filter(|id| !id.contains(":hb:"))
        .collect();
    let problems = expect::judge(items.iter().map(|i| (i.id.as_str(), i.ok)), &expected);
    if !problems.is_empty() {
        let mut msg = expect::report(&problems, LIST);
        for p in problems.iter().take(40) {
            if let expect::Problem::Regressed(id) = p
                && let Some(i) = items.iter().find(|i| &i.id == id)
            {
                msg.push_str(&format!("  {}: {}\n", i.id, i.detail));
            }
        }
        panic!("{msg}\n{} items, {failed} failed", items.len());
    }
}

#[test]
fn format_g3_matches_printf() {
    // C's printf("%.3g") on these values.
    let cases: [(f64, &str); 14] = [
        (1000.0, "1e+03"),
        (0.977, "0.977"),
        (-0.0, "-0"),
        (0.0, "0"),
        (1300.0, "1.3e+03"),
        (100.0, "100"),
        (1220.0, "1.22e+03"),
        (999.5, "1e+03"),
        (0.0001, "0.0001"),
        (0.00001234, "1.23e-05"),
        (3.14159, "3.14"),
        (-224.0, "-224"),
        (0.5, "0.5"),
        (2.0f32 as f64 * std::f32::consts::PI as f64, "6.28"),
    ];
    for (v, want) in cases {
        assert_eq!(color::g3(v), want, "{v}");
    }
}
