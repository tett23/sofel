//! The CID queries and the PostScript font information against FreeType
//! on HarfBuzz's test fonts (ADR 0021): `<font>#<face>:cid` and
//! `<font>#<face>:ps-info`.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

use std::path::Path;
use std::process::Command;

use sofel_conformance::cff_info::{parse, sofel_cid, sofel_ps_info};
use sofel_conformance::{expect, fonts, paths};
use sofel_font::Font;

const LIST: &str = "expected-failures-freetype.txt";

fn ft_dump(mode: &str, font: &Path) -> Option<String> {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe).arg(mode).arg(font).output().unwrap();
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).unwrap())
}

fn first_diff(a: &[String], b: &[String]) -> String {
    let i = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    format!("FreeType {:?} / sofel {:?}", a.get(i), b.get(i))
}

#[test]
fn cid_and_ps_info_agree_with_freetype() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let mut items: Vec<(String, bool, String)> = Vec::new();
    for path in fonts::harfbuzz_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let bytes = std::fs::read(&path).unwrap();
        for (mode, item) in [("--cid", "cid"), ("--ps-info", "ps-info")] {
            let Some(text) = ft_dump(mode, &path) else {
                continue;
            };
            for (face, ft) in parse(&text) {
                let Ok(font) = Font::new(&bytes, face) else {
                    continue;
                };
                let ours = if item == "cid" {
                    let n = ft.iter().filter(|l| l.starts_with("cid ")).count();
                    sofel_cid(&font, n.saturating_sub(1) as u32)
                } else {
                    sofel_ps_info(&font)
                };
                let ok = ours == ft;
                let detail = if ok {
                    String::new()
                } else {
                    first_diff(&ft, &ours)
                };
                items.push((format!("{id}#{face}:{item}"), ok, detail));
            }
        }
    }
    let failed = items.iter().filter(|i| !i.1).count();
    eprintln!("{} cid and ps-info items, {failed} failed", items.len());
    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let mut expected = expect::parse(&list);
    expected.retain(|id| id.ends_with(":cid") || id.ends_with(":ps-info"));
    let problems = expect::judge(
        items.iter().map(|(id, ok, _)| (id.as_str(), *ok)),
        &expected,
    );
    if !problems.is_empty() {
        let mut msg = expect::report(&problems[..problems.len().min(100)], LIST);
        for p in problems.iter().take(30) {
            if let expect::Problem::Regressed(id) = p
                && let Some(i) = items.iter().find(|i| &i.0 == id)
            {
                msg.push_str(&format!("  {id}: {}\n", i.2));
            }
        }
        panic!("{msg}\n{} problems", problems.len());
    }
}
