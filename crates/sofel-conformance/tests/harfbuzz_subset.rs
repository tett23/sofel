//! HarfBuzz's subsetting tests (`test/subset/data/tests/*.tests`, ADR
//! 0022): each case runs through `sofel_cli::subset` and `sofel-subset`
//! with `run-tests.py`'s arguments, and its output must equal HarfBuzz's
//! expected file byte for byte (`:bytes`) and pass the validator
//! (`:validate`). Expected failures are listed with their reasons in
//! `conformance/expected-failures-subset.txt`.

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
    let args = case.args()?;
    let opts = sofel_cli::subset::parse(&args)?;
    let bytes = std::fs::read(&case.font).map_err(|e| e.to_string())?;
    let font = Font::new(&bytes, 0).map_err(|e| format!("{e:?}"))?;
    let input = sofel_cli::subset::resolve(&opts, &font)?;
    sofel_subset::subset(&font, &input).map_err(|e| e.to_string())
}

fn validate_errors(bytes: &[u8]) -> Vec<String> {
    let Ok(s) = Sfnt::new(bytes) else {
        return vec!["not an sfnt".to_string()];
    };
    let mut errors = Vec::new();
    validate::sfnt(&s, &mut |d: Diagnostic| {
        if d.severity == Severity::Error {
            errors.push(format!("{}@{}: {:?}", d.table, d.offset, d.kind));
        }
    });
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
