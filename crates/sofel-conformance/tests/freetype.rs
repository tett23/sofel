//! sofel against FreeType on HarfBuzz's test fonts (ADR 0008). Failures
//! not listed in conformance/expected-failures-freetype.txt, and listed
//! items that pass, fail the test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use sofel_conformance::compare::{Item, compare};
use sofel_conformance::dump::{parse_ft_dump, sofel_faces};
use sofel_conformance::{expect, paths};

const LIST: &str = "expected-failures-freetype.txt";

const SFNT_MAGIC: [[u8; 4]; 5] = [*b"\x00\x01\x00\x00", *b"OTTO", *b"true", *b"typ1", *b"ttcf"];

/// The sfnt fonts and collections among HarfBuzz's test fonts, sorted.
fn fonts(testdata: &Path) -> Vec<PathBuf> {
    let hb = testdata.join("harfbuzz/test");
    let mut dirs = vec![hb.join("subset/data/fonts")];
    for e in std::fs::read_dir(hb.join("shape/data")).unwrap() {
        dirs.push(e.unwrap().path().join("fonts"));
    }
    let mut out = Vec::new();
    for d in dirs {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries {
            let p = e.unwrap().path();
            let head = std::fs::read(&p)
                .ok()
                .and_then(|b| b.first_chunk::<4>().copied());
            if head.is_some_and(|h| SFNT_MAGIC.contains(&h)) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn items_for(font: &Path, testdata: &Path, ft_dump: &Path) -> Vec<Item> {
    let id = font.strip_prefix(testdata).unwrap().display().to_string();
    let out = Command::new(ft_dump).arg(font).output().unwrap();
    if !out.status.success() {
        // FreeType cannot open the file at all; nothing to compare.
        return Vec::new();
    }
    let ft = parse_ft_dump(&String::from_utf8(out.stdout).unwrap()).unwrap();
    let bytes = std::fs::read(font).unwrap();
    let glyphs_for = |i: u32| {
        ft.iter()
            .find(|f| f.index == i && !f.advances.is_empty())
            .and_then(|f| f.num_glyphs)
    };
    let ours = sofel_faces(&bytes, &glyphs_for);
    compare(&id, &ft, &ours)
}

#[test]
fn agrees_with_freetype() {
    let testdata = paths::testdata();
    let ft_dump = paths::ft_dump();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    paths::require(&ft_dump, "ft-dump").unwrap_or_else(|e| panic!("{e}"));

    let fonts = fonts(&testdata);
    assert!(fonts.len() > 600, "found only {} fonts", fonts.len());
    let items: Vec<Item> = fonts
        .iter()
        .flat_map(|f| items_for(f, &testdata, &ft_dump))
        .collect();

    let mut kinds: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for i in &items {
        let kind = i.id.rsplit_once("#").map_or("", |(_, k)| k);
        let kind = kind
            .split_once(':')
            .map_or(kind, |(_, k)| k)
            .split(':')
            .next()
            .unwrap_or("");
        let e = kinds.entry(kind.to_string()).or_default();
        e.0 += 1;
        e.1 += usize::from(!i.ok);
    }
    eprintln!("{} fonts, {} items", fonts.len(), items.len());
    for (k, (n, failed)) in &kinds {
        eprintln!("  {k}: {n} items, {failed} failed");
    }

    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let expected = expect::parse(&list);
    let problems = expect::judge(items.iter().map(|i| (i.id.as_str(), i.ok)), &expected);
    if !problems.is_empty() {
        let mut msg = expect::report(&problems, LIST);
        for p in &problems {
            if let expect::Problem::Regressed(id) = p
                && let Some(i) = items.iter().find(|i| &i.id == id)
            {
                msg.push_str(&format!("  {}: {}\n", i.id, i.detail));
            }
        }
        let failed = items.iter().filter(|i| !i.ok).count();
        panic!("{msg}\n{} items, {failed} failed", items.len());
    }
}

#[test]
fn validate_command_runs_on_every_font() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let args: Vec<std::ffi::OsString> = fonts(&testdata)
        .into_iter()
        .map(PathBuf::into_os_string)
        .collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = sofel_cli::validate::run(&args, &mut out, &mut err);
    assert!(
        code == 0 || code == 1,
        "exit {code}: {}",
        String::from_utf8_lossy(&err)
    );
    let mut json_out = Vec::new();
    let mut json_args = vec![std::ffi::OsString::from("--json")];
    json_args.extend(args);
    let json_code = sofel_cli::validate::run(&json_args, &mut json_out, &mut err);
    assert_eq!(code, json_code);
    assert_eq!(
        String::from_utf8(out).unwrap().lines().count(),
        String::from_utf8(json_out).unwrap().lines().count()
    );
}
