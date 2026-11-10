//! sofel's subsetting against HarfBuzz's for inputs without an expected
//! file in HarfBuzz's tests (ADR 0022, 突き合わせ): `hb-dump --subset`
//! writes what `hb_subset_or_fail` gives for `hb-subset` arguments, and
//! sofel's output for the same arguments (through `sofel_cli::subset`)
//! must be the same bytes, or both must fail.
//!
//! - `hb-dump/golem/<font>:keep`: Golem's usage (ADR 0022, Golem) on every
//!   font of `test/subset/data/fonts/`: the glyphs that the characters of
//!   HarfBuzz's `perf/texts/ja-runs.txt` and `en-paragraph.txt` map to,
//!   glyph 1 and the last glyph, with `--retain-gids`.
//! - `hb-dump/golem/<font>:pin`: the same with every axis pinned to its
//!   default (`--instance=*=drop`), for the fonts with an `fvar` table
//!   (HarfBuzz refuses to pin a font without axes).
//! - `hb-dump/repack/<tests file>`: the repacking tests of
//!   `test/subset/data/repack_tests/` with `run-repack-tests.py`'s
//!   arguments.
//!
//! Expected failures are listed with their reasons in
//! `conformance/expected-failures-subset-hb.txt`. With
//! `SOFEL_SUBSET_HB_REPORT=<file>` the test writes the details of every
//! difference there.

#![allow(
    clippy::all,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::path::Path;
use std::process::Command;

use sofel_conformance::{expect, paths, subset};
use sofel_font::Font;
use sofel_read::Tag;

fn data() -> std::path::PathBuf {
    paths::testdata().join("harfbuzz/test/subset/data")
}

/// HarfBuzz's output for `args`, or why it failed.
fn harfbuzz(exe: &Path, args: &[String], out: &Path) -> Result<Vec<u8>, String> {
    let _ = std::fs::remove_file(out);
    let r = Command::new(exe)
        .arg("--subset")
        .args(args)
        .arg(format!("--output-file={}", out.display()))
        .output()
        .map_err(|e| e.to_string())?;
    if !r.status.success() {
        return Err(format!(
            "hb-dump exited with {}: {}",
            r.status,
            String::from_utf8_lossy(&r.stderr).trim()
        ));
    }
    std::fs::read(out).map_err(|e| e.to_string())
}

/// Whether sofel and HarfBuzz agree, with the details when not.
fn compare(ours: &Result<Vec<u8>, String>, theirs: &Result<Vec<u8>, String>) -> Option<String> {
    match (ours, theirs) {
        (Ok(o), Ok(t)) if o == t => None,
        (Ok(o), Ok(t)) => Some(subset::diff(t, o)),
        (Err(_), Err(_)) => None,
        (Err(e), Ok(_)) => Some(format!("sofel failed: {e}")),
        (Ok(_), Err(e)) => Some(format!("sofel succeeded, HarfBuzz failed: {e}")),
    }
}

#[test]
fn subsetting_agrees_with_harfbuzz() {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    paths::require(&data(), "HarfBuzz test data").unwrap_or_else(|e| panic!("{e}"));
    let dir = std::env::temp_dir().join(format!("sofel-subset-hb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out");

    // (id, arguments)
    let mut cases: Vec<(String, Vec<String>)> = Vec::new();
    let texts: Vec<String> = subset::GOLEM_TEXTS
        .iter()
        .map(|t| std::fs::read_to_string(paths::testdata().join(t)).unwrap())
        .collect();
    for path in subset::fonts(&data()).unwrap() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap();
        // A font sofel cannot open gets an empty glyph set; the arguments
        // are still compared (both must fail).
        let (glyphs, variable) = match Font::new(&bytes, 0) {
            Ok(font) => (
                subset::golem_glyphs(&font, &texts),
                font.table(Tag::new(b"fvar")).is_some(),
            ),
            Err(_) => (vec![1], false),
        };
        cases.push((
            format!("hb-dump/golem/{name}:keep"),
            subset::golem_args(&path, &glyphs, false),
        ));
        if variable {
            cases.push((
                format!("hb-dump/golem/{name}:pin"),
                subset::golem_args(&path, &glyphs, true),
            ));
        }
    }
    for case in subset::repack_cases(&data()).unwrap() {
        cases.push((format!("hb-dump/repack/{}", case.name), case.args()));
    }

    let mut results = Vec::new();
    let mut detail = Vec::new();
    let mut both_failed = 0;
    for (id, args) in &cases {
        let theirs = harfbuzz(&exe, args, &out);
        let ours = subset::run_sofel(args);
        if ours.is_err() && theirs.is_err() {
            both_failed += 1;
        }
        let problem = compare(&ours, &theirs);
        if let Some(p) = &problem {
            detail.push(format!("{id}: {p}"));
        }
        results.push((id.clone(), problem.is_none()));
    }
    let _ = std::fs::remove_dir_all(&dir);
    let passed = results.iter().filter(|r| r.1).count();
    eprintln!(
        "subset vs hb-dump: {passed} of {} agree ({both_failed} where both fail)",
        results.len()
    );
    if let Ok(path) = std::env::var("SOFEL_SUBSET_HB_REPORT") {
        std::fs::write(path, detail.join("\n")).unwrap();
    }
    let list = paths::conformance_file("expected-failures-subset-hb.txt");
    let expected = expect::parse(&std::fs::read_to_string(&list).unwrap_or_default());
    let problems = expect::judge(results.iter().map(|(i, p)| (i.as_str(), *p)), &expected);
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(&problems, "expected-failures-subset-hb.txt")
    );
}
