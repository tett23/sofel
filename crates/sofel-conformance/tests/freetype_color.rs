//! sofel's raw COLR and CPAL values against FreeType's (ADR 0023):
//! `ft-dump --colr`, and the strikes of `ft-dump --sbit`, on every face of HarfBuzz's test fonts with color
//! tables. Failures not listed in
//! conformance/expected-failures-freetype.txt, and listed items that
//! pass, fail the test.

#![allow(clippy::all, clippy::unwrap_used, clippy::panic)]

use std::process::Command;

use sofel_conformance::color;
use sofel_conformance::{expect, fonts, paths};
use sofel_font::Font;

const LIST: &str = "expected-failures-freetype.txt";

#[test]
fn raw_colors_agree_with_freetype() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    let mut items = Vec::new();
    for path in color::color_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let out = Command::new(&exe)
            .arg("--colr")
            .arg(&path)
            .output()
            .unwrap();
        if !out.status.success() {
            continue;
        }
        let text = String::from_utf8(out.stdout).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        for (index, part) in color::split_faces(&text) {
            if part.lines().any(|l| l.starts_with("error ")) {
                continue;
            }
            if let Ok(font) = Font::new(&bytes, index) {
                items.extend(color::compare_colr(&id, index, &font, &part));
            }
        }
        let out = Command::new(&exe)
            .arg("--sbit")
            .arg(&path)
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        for (index, part) in color::split_faces(&text) {
            if part.lines().any(|l| l.starts_with("error ")) {
                continue;
            }
            if let Ok(font) = Font::new(&bytes, index) {
                items.push(color::compare_strikes(&id, index, &font, &part));
            }
        }
    }
    let failed = items.iter().filter(|i| !i.ok).count();
    eprintln!("{} items, {failed} failed", items.len());
    if let Some(path) = std::env::var_os("SOFEL_FAILED_ITEMS") {
        let text: String = items
            .iter()
            .filter(|i| !i.ok)
            .map(|i| format!("{}\t{}\n", i.id, i.detail.replace('\n', "|")))
            .collect();
        std::fs::write(path, text).unwrap();
    }
    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let expected = expect::parse(&list)
        .into_iter()
        .filter(|id| id.contains(":colr:") || id.ends_with(":cpal") || id.ends_with(":strikes"))
        .collect();
    let problems = expect::judge(items.iter().map(|i| (i.id.as_str(), i.ok)), &expected);
    if !problems.is_empty() {
        let mut msg = expect::report(&problems, LIST);
        for p in problems.iter().take(20) {
            if let expect::Problem::Regressed(id) = p
                && let Some(i) = items.iter().find(|i| &i.id == id)
            {
                msg.push_str(&format!("  {}: {}\n", i.id, i.detail));
            }
        }
        panic!("{msg}");
    }
}

#[test]
fn adr0027_raw_paints_and_clip_boxes_at_design_locations() {
    let settings = include_str!("../../../conformance/extra-color-var-cases.tsv")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .filter_map(|s| s.split('\t').nth(2))
        .filter(|&s| s != "-")
        .map(|s| ("test_glyphs-glyf_colr_1_variable.ttf", s));
    for (name, settings) in [
        ("colr-clip-rounding.ttf", "wght=0.5"),
        ("colr-wide-scale.ttf", "wght=1"),
    ]
    .into_iter()
    .chain(settings)
    {
        let path = paths::testdata().join("harfbuzz/test/api/fonts").join(name);
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let settings_list: Vec<_> = settings
            .split(',')
            .map(|s| sofel_font::var::Variation::parse(s).unwrap())
            .collect();
        let location = sofel_font::var::Location::new(&font, None, &settings_list);
        let output = Command::new(paths::ft_dump())
            .arg("--colr")
            .arg(format!("--variations={settings}"))
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}");
        let text = String::from_utf8(output.stdout).unwrap();
        let faces = color::split_faces(&text);
        assert_eq!(faces.len(), 1);
        let items =
            color::compare_colr_at(name, 0, font.instance(&location), &faces.first().unwrap().1);
        assert!(items.len() > 1);
        for item in items.into_iter().filter(|item| item.id.contains(":colr:")) {
            assert!(item.ok, "{}: {}", item.id, item.detail);
        }
    }
}
