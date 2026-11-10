//! COLR version 0 glyphs in color against FreeType (ADR 0023):
//! `ft-dump --raster PPEM --color` (FT_LOAD_COLOR) on every face of
//! HarfBuzz's test fonts with color tables, at 12, 24 and 48 px, with
//! palette 0 and the last palette. Failures not listed in
//! conformance/expected-failures-freetype.txt, and listed items that
//! pass, fail the test.

#![allow(clippy::all, clippy::unwrap_used, clippy::panic)]

use std::process::Command;

use sofel_conformance::color;
use sofel_conformance::{expect, fonts, paths};
use sofel_font::Font;

const LIST: &str = "expected-failures-freetype.txt";
const PPEMS: [u16; 3] = [12, 24, 48];

#[test]
fn color_rasters_agree_with_freetype() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    let mut items = Vec::new();
    for path in color::color_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let bytes = std::fs::read(&path).unwrap();
        for ppem in PPEMS {
            let out = Command::new(&exe)
                .args(["--raster", &ppem.to_string(), "--color"])
                .arg(&path)
                .output()
                .unwrap();
            if !out.status.success() {
                continue;
            }
            let text = String::from_utf8(out.stdout).unwrap();
            for (index, part) in color::split_faces(&text) {
                // The face could not be opened or sized.
                if part.lines().nth(1).is_some_and(|l| l.starts_with("error"))
                    || part.lines().any(|l| l == "error size")
                {
                    continue;
                }
                if let Ok(font) = Font::new(&bytes, index) {
                    items.extend(color::compare_color_rasters(&id, index, &font, ppem, &part));
                }
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
        .filter(|id| id.contains(":colr-raster:"))
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
