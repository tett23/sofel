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

/// Records drawing commands.
#[derive(Default)]
struct Record(Vec<sofel_geom::Command>);

impl sofel_geom::Pen for Record {
    fn move_to(&mut self, p: sofel_geom::Point) {
        self.0.push(sofel_geom::Command::MoveTo(p));
    }
    fn line_to(&mut self, p: sofel_geom::Point) {
        self.0.push(sofel_geom::Command::LineTo(p));
    }
    fn quad_to(&mut self, c: sofel_geom::Point, p: sofel_geom::Point) {
        self.0.push(sofel_geom::Command::QuadTo(c, p));
    }
    fn cubic_to(&mut self, c1: sofel_geom::Point, c2: sofel_geom::Point, p: sofel_geom::Point) {
        self.0.push(sofel_geom::Command::CubicTo(c1, c2, p));
    }
    fn close(&mut self) {
        self.0.push(sofel_geom::Command::Close);
    }
}

/// The unscaled outline of `glyph`, or the error.
fn outline(font: &Font<'_>, glyph: u32) -> Result<Vec<sofel_geom::Command>, String> {
    let mut pen = Record::default();
    let mut scratch = sofel_outline::Scratch::default();
    sofel_outline::draw(
        font,
        sofel_read::GlyphId(glyph),
        sofel_outline::Scale::Unscaled,
        &mut scratch,
        &mut pen,
    )
    .map(|_| pen.0)
    .map_err(|e| format!("{e:?}"))
}

/// ADR 0022 輪郭と送り幅の保存, for every axis pinned to its default:
/// each glyph of every variable test font, subset with all glyphs and
/// `NOTDEF_OUTLINE`, has the outline and the advance of the source
/// font's default instance. Fonts whose tables this version cannot
/// subset yet are skipped (and counted).
#[test]
fn pinned_outlines_and_advances() {
    use sofel_subset::{Flags, Input, RangeSet, SetKind, SubsetError};
    paths::require(&data(), "HarfBuzz test data").unwrap_or_else(|e| panic!("{e}"));
    let mut fonts: Vec<_> = std::fs::read_dir(data().join("fonts"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    fonts.sort();
    let (mut checked, mut skipped) = (0, Vec::new());
    for path in fonts {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(font) = Font::new(&bytes, 0) else {
            continue;
        };
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let input = Input::new()
            .with_set(SetKind::Glyphs, RangeSet::all())
            .with_flags(Flags::NOTDEF_OUTLINE);
        let Some(input) = input.with_all_axes_pinned_to_default(&font) else {
            continue;
        };
        let out = match sofel_subset::subset(&font, &input) {
            Ok(o) => o,
            Err(SubsetError::Unsupported { .. }) => {
                skipped.push(name);
                continue;
            }
            Err(e) => panic!("{name}: {e}"),
        };
        let pinned = Font::new(&out, 0).unwrap();
        assert_eq!(pinned.num_glyphs(), font.num_glyphs(), "{name}");
        for g in 0..font.num_glyphs() {
            let gid = sofel_read::GlyphId(g);
            assert_eq!(
                outline(&pinned, g),
                outline(&font, g),
                "{name}: outline of glyph {g}"
            );
            assert_eq!(
                pinned.advance(gid),
                font.advance(gid),
                "{name}: advance of glyph {g}"
            );
        }
        // The output validates when the source does.
        if validate_errors(&bytes).is_empty() {
            assert!(
                validate_errors(&out).is_empty(),
                "{name}: {:?}",
                validate_errors(&out)
            );
        }
        checked += 1;
    }
    eprintln!(
        "pinned outlines: {checked} fonts checked, skipped (unsupported): {}",
        skipped.join(" ")
    );
    assert!(checked > 0);
}
