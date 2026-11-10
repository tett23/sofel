//! HarfBuzz's subsetting tests (`test/subset/data/tests/*.tests`, ADR
//! 0022): each case runs through `sofel_cli::subset` and `sofel-subset`
//! with `run-tests.py`'s arguments, and its output must equal HarfBuzz's
//! expected file byte for byte (`:bytes`) and pass the validator
//! (`:validate`; for a CFF font also its `CFF ` table on its own, as
//! PDF embeds it). Expected failures are listed with their reasons in
//! `conformance/expected-failures-subset.txt`.
//!
//! The repacking tests (`test/subset/data/repack_tests/`) run with
//! `run-repack-tests.py`'s arguments; each must succeed (`:success`) and
//! pass the validator (`:validate`). Nothing here needs `hb-dump` or
//! `ft-dump`, so that the test also runs on wasm32-wasip1 (ADR 0022,
//! 適合テスト); the comparisons with `hb-dump` are in
//! `harfbuzz_subset_hb.rs`.

#![allow(
    clippy::all,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use sofel_conformance::{expect, paths, subset};
use sofel_font::Font;
use sofel_read::sfnt::Sfnt;
use sofel_read::{Diagnostic, Severity, validate};

fn data() -> std::path::PathBuf {
    paths::testdata().join("harfbuzz/test/subset/data")
}

/// The output of one case, or why it failed.
fn run(case: &subset::Case) -> Result<Vec<u8>, String> {
    subset::run_sofel(&case.args()?)
}

fn validate_errors(bytes: &[u8]) -> Vec<String> {
    let Ok(s) = Sfnt::new(bytes) else {
        return vec!["not an sfnt".to_string()];
    };
    let mut errors = Vec::new();
    let mut report = |d: Diagnostic| {
        if d.severity == Severity::Error {
            errors.push(format!("{}@{}: {:?}", d.table, d.offset, d.kind));
        }
    };
    validate::sfnt(&s, &mut report);
    // ADR 0022: the CFF table taken out of the font is valid by itself.
    if let Some(cff) = s.table(sofel_read::cff::TAG) {
        sofel_read::cff::validate(cff, None, &mut report);
    }
    errors
}

#[test]
fn harfbuzz_subset_tests() {
    paths::require(&data(), "HarfBuzz test data").unwrap_or_else(|e| panic!("{e}"));
    let cases = subset::all_cases(&data()).unwrap();
    let mut results = Vec::new();
    let mut detail = Vec::new();
    for case in &cases {
        let id = case.id();
        let out = run(case);
        let expected = std::fs::read(case.expected(&data()));
        let (bytes_ok, validate_ok) = match (&out, &expected) {
            (Ok(o), Ok(e)) => {
                if o != e {
                    detail.push(format!("{id}:bytes {}", subset::diff(e, o)));
                }
                let errs = validate_errors(o);
                if !errs.is_empty() {
                    detail.push(format!("{id}:validate {}", errs.join("; ")));
                }
                (o == e, errs.is_empty())
            }
            (Err(err), _) => {
                detail.push(format!("{id}: {err}"));
                (false, false)
            }
            (_, Err(err)) => {
                detail.push(format!("{id}: no expected file: {err}"));
                (false, false)
            }
        };
        results.push((format!("{id}:bytes"), bytes_ok));
        results.push((format!("{id}:validate"), validate_ok));
    }
    // ADR 0022: the repacking tests (run-repack-tests.py's arguments) must
    // succeed and pass the validator; hb-dump compares their bytes
    // (harfbuzz_subset_hb.rs).
    for case in subset::repack_cases(&data()).unwrap() {
        let id = format!("repack_tests/{}", case.name);
        let out = subset::run_sofel(&case.args());
        let validate_ok = match &out {
            Ok(o) => {
                let errs = validate_errors(o);
                if !errs.is_empty() {
                    detail.push(format!("{id}:validate {}", errs.join("; ")));
                }
                errs.is_empty()
            }
            Err(err) => {
                detail.push(format!("{id}: {err}"));
                false
            }
        };
        results.push((format!("{id}:success"), out.is_ok()));
        results.push((format!("{id}:validate"), validate_ok));
    }
    let passed = results.iter().filter(|r| r.1).count();
    eprintln!("subset: {passed} of {} items passed", results.len());
    if let Ok(path) = std::env::var("SOFEL_SUBSET_REPORT") {
        std::fs::write(path, detail.join("\n")).unwrap();
    }
    let list = paths::conformance_file("expected-failures-subset.txt");
    let expected = expect::parse(&std::fs::read_to_string(&list).unwrap_or_default());
    let problems = expect::judge(results.iter().map(|(i, p)| (i.as_str(), *p)), &expected);
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(&problems, "expected-failures-subset.txt")
    );
}
