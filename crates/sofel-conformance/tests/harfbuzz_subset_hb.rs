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
//! - `hb-dump/golem/<font>:pin-downgrade`: the same with
//!   `--downgrade-cff2`, for the fonts with a `CFF2` table.
//! - `hb-dump/golem/<font>:keep+prepared` and `:pin+prepared`: the same
//!   with `--preprocess` (`hb_subset_preprocess` in HarfBuzz,
//!   `sofel_subset::Prepared` in sofel).
//! - `hb-dump/cff2/<font>:desubroutinize`: keep a CFF2 VariationStore and
//!   blends while flattening subroutines in three variable fonts.
//! - `hb-dump/cff2/<font>:keep`: retain variations in a CFF2 font without
//!   global or local subroutines.
//! - `hb-dump/glyphs/<font>`: glyphs by name (`--glyphs`, ADR 0022 stage
//!   12) on every font with glyph names: up to 24 named glyphs spread
//!   over the font, `gid1` and `uni0041`, then the second name's glyph
//!   removed by id and added back by name, with `--retain-gids`.
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

/// The `--glyphs` arguments of `hb-dump/glyphs/<font>`, or `None` for a
/// font with fewer than two named glyphs.
fn glyph_name_args(path: &Path, font: &Font<'_>) -> Option<Vec<String>> {
    let mut buf = vec![0u32; font.glyph_names_len()];
    let names = font.glyph_names(&mut buf)?;
    let n = font.num_glyphs();
    let step = (n / 24).max(1) as usize;
    let named: Vec<(u32, String)> = (1..n)
        .step_by(step)
        .filter_map(|g| {
            let s = names.glyph_name(sofel_read::GlyphId(g))?;
            let s = std::str::from_utf8(s).ok()?;
            // Names with the option's separators cannot be given.
            (!s.is_empty() && !s.contains([' ', ',']) && s.is_ascii()).then(|| (g, s.to_string()))
        })
        .collect();
    let [(_, _), (second_gid, second), ..] = named.as_slice() else {
        return None;
    };
    let mut list: Vec<String> = named.iter().map(|(_, s)| s.clone()).collect();
    list.push("gid1".to_string());
    if font.nominal_glyph_raw(0x41).is_some() {
        list.push("uni0041".to_string());
    }
    Some(vec![
        format!("--font-file={}", path.display()),
        format!("--glyphs={}", list.join(",")),
        format!("--gids-={second_gid}"),
        format!("--glyphs+={second}"),
        "--retain-gids".to_string(),
    ])
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
        let (glyphs, variable, cff2) = match Font::new(&bytes, 0) {
            Ok(font) => (
                subset::golem_glyphs(&font, &texts),
                font.table(Tag::new(b"fvar")).is_some(),
                font.table(Tag::new(b"CFF2")).is_some(),
            ),
            Err(_) => (vec![1], false, false),
        };
        if let Some(args) = Font::new(&bytes, 0)
            .ok()
            .and_then(|font| glyph_name_args(&path, &font))
        {
            cases.push((format!("hb-dump/glyphs/{name}"), args));
        }
        for pin in [false, true].into_iter().filter(|&p| !p || variable) {
            let kind = if pin { "pin" } else { "keep" };
            let args = subset::golem_args(&path, &glyphs, pin);
            let mut preprocessed = args.clone();
            preprocessed.push("--preprocess".to_string());
            cases.push((format!("hb-dump/golem/{name}:{kind}"), args));
            cases.push((
                format!("hb-dump/golem/{name}:{kind}+prepared"),
                preprocessed,
            ));
        }
        if variable && cff2 {
            let mut args = subset::golem_args(&path, &glyphs, true);
            args.push("--downgrade-cff2".to_string());
            cases.push((format!("hb-dump/golem/{name}:pin-downgrade"), args));
        }
    }
    for font in [
        "AdobeVFPrototype.otf",
        "AdobeVFPrototype_vsindex.otf",
        "Cantarell-VF-ABC.otf",
    ] {
        let path = data().join("fonts").join(font);
        cases.push((
            format!("hb-dump/cff2/{font}:desubroutinize"),
            vec![
                format!("--font-file={}", path.display()),
                "--gids=0-3".to_string(),
                "--desubroutinize".to_string(),
                "--drop-tables+=GDEF,HVAR,VVAR".to_string(),
            ],
        ));
    }
    let no_subrs = paths::testdata()
        .join("harfbuzz/test/api/fonts/AdobeVFPrototype.ac.nosubrs.otf");
    cases.push((
        "hb-dump/cff2/AdobeVFPrototype.ac.nosubrs.otf:keep".to_string(),
        vec![
            format!("--font-file={}", no_subrs.display()),
            "--gids=0-2".to_string(),
            "--drop-tables+=GDEF,HVAR,VVAR".to_string(),
        ],
    ));
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
