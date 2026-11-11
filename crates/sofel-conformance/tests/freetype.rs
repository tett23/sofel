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
use sofel_conformance::dump::{add_font_values, parse_ft_dump, sofel_faces};
use sofel_conformance::{expect, paths};

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

const LIST: &str = "expected-failures-freetype.txt";

const SFNT_MAGIC: [[u8; 4]; 6] = [
    *b"\x00\x01\x00\x00",
    *b"OTTO",
    *b"true",
    *b"typ1",
    *b"ttcf",
    *b"\x00\x00\x01\x00",
];

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
    let mut ours = sofel_faces(&bytes, &glyphs_for);
    for o in ours.iter_mut().filter(|o| o.error.is_none()) {
        if let (Ok(font), Some(f)) = (
            sofel_font::Font::new(&bytes, o.index),
            ft.iter().find(|f| f.index == o.index),
        ) {
            add_font_values(o, &font, f);
        }
    }
    compare(&id, &ft, &ours)
}

#[test]
fn inv1_dfont_faces_match_freetype() {
    let ft_dump = std::env::var_os("SOFEL_FT_DUMP")
        .map(PathBuf::from)
        .unwrap_or_else(paths::ft_dump);
    paths::require(&ft_dump, "ft-dump").unwrap();
    let first = build::glyph_font(&[vec![], vec![]], true, false);
    let second = build::glyph_font(&[vec![], vec![], vec![]], true, false);
    let data = build::dfont(&[(b"sfnt", vec![(14116, first), (3, second)])]);
    let path = std::env::temp_dir().join(format!("sofel-dfont-{}.dfont", std::process::id()));
    std::fs::write(&path, &data).unwrap();
    let output = Command::new(ft_dump).arg(&path).output().unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ft = parse_ft_dump(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(ft.len(), 2);
    let glyphs_for = |face| {
        ft.iter()
            .find(|f| f.index == face)
            .and_then(|f| f.num_glyphs)
    };
    let ours = sofel_faces(&data, &glyphs_for);
    assert_eq!(ours.len(), 2);
    let items = compare("generated-dfont", &ft, &ours);
    let selected: Vec<_> = items
        .iter()
        .filter(|i| i.id.contains(":cmap:") || i.id.ends_with(":hmtx"))
        .collect();
    assert!(!selected.is_empty(), "no cmap or metrics were compared");
    for item in &selected {
        assert!(item.ok, "{}: {}", item.id, item.detail);
    }
    eprintln!(
        "generated dfont: {} faces, {} cmap/metrics items matched",
        ft.len(),
        selected.len()
    );
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
    // Outline and raster items have their own tests (ADR 0011, 0012).
    let mut expected = expect::parse(&list);
    // ... and the COLR and CPAL items too (ADR 0023, freetype_color.rs).
    // Variable-font reference queries have their own test (ADR 0027).
    expected.retain(|id| {
        !id.contains(":outline:")
            && !id.contains(":raster:")
            && !id.contains(":colr:")
            && !id.contains(":colr-raster:")
            && !id.contains(":var:")
            && !id.ends_with(":cpal")
            && !id.ends_with(":strikes")
    });
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
