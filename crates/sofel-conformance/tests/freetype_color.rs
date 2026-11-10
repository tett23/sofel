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
