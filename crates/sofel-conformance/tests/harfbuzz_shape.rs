//! HarfBuzz's shaping tests with `sofel-shape` (ADR 0020, 適合テスト).
//!
//! Every line of `test/shape/data/{in-house,aots,text-rendering-tests}/tests/*.tests`
//! is run as HarfBuzz's `test/shape/run-tests.py` runs it with
//! `hb-shape`, through `sofel_cli::shape::run_with_input` instead of a
//! process: `FONT --unicodes U+... OPTIONS --shaper=S` plus `--verify
//! --unsafe-to-concat --safe-to-insert-tatweel` unless the expectation
//! is `*`; when the output differs and has a `gid`, both sides are read
//! again with `--glyphs --no-glyph-names` and compared as glyph ids.
//! `@shapers=` directives choose the shapers of the lines after them in
//! a file (`ot` by default), as `--shaper=` in a line does; shapers
//! sofel does not have are not run and fail. `@font-funcs=` and
//! `@face-loader=` are ignored, as are the options (sofel has one set of
//! font functions). Lines with absolute font paths (system fonts) are
//! skipped; lines commented out are not tests. The files HarfBuzz's own
//! suite disables (`text-rendering-tests/DISABLED`: HarfBuzz itself
//! fails them, e.g. `CMAP-3.tests`, whose non-Unicode Macintosh cmap
//! HarfBuzz reads as ASCII only) are not run either.
//!
//! The items are `<suite>/<file>:<line>`, with `@<shaper>` for shapers
//! other than `ot`. `conformance/expected-failures.txt` lists the
//! failures, each with one category as its comment (ADR 0020):
//! `shaper:<name>`, `variations`, `aat`, `synthetic`, `cff`, `color`,
//! `out-of-scope:<name>`; and, for the cases in ADR 0044, `dfont` and
//! `sanitize` (see the list). The test checks that each category fits the
//! line: the plan's shaper for `shaper:`, the line's shaper for
//! `out-of-scope:`, the options for `synthetic`, and the tables of the
//! font for `variations` (or the options), `aat`, `cff` and `color`.
//!
//! `SOFEL_SHAPE_VERBOSE=1` prints every failure with both outputs;
//! `SOFEL_SHAPE_FILTER=<substring>` runs the matching items only (and
//! then does not judge the list).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use sofel_conformance::paths;
use sofel_conformance::shape::{self, Subject, option};
use sofel_shape::{Direction, Language, Script, SegmentProperties};

const LIST: &str = "expected-failures.txt";
const SUITES: [&str; 3] = ["in-house", "aots", "text-rendering-tests"];
/// One test: a line of a `.tests` file with one shaper.
struct Case {
    id: String,
    font: PathBuf,
    options: Vec<String>,
    unicodes: String,
    expected: String,
    shaper: String,
}

/// The cases of one `.tests` file, as run-tests.py reads it.
fn cases(suite: &str, file: &Path, skipped: &mut usize) -> Vec<Case> {
    let text = std::fs::read_to_string(file).unwrap();
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    let dir = file.parent().unwrap();
    let mut shapers = vec!["ot".to_string()];
    let mut out = Vec::new();
    for (n, raw) in (1..).zip(text.lines()) {
        let mut line = raw;
        let mut comment = false;
        if let Some(rest) = line.strip_prefix('#') {
            if rest.starts_with(' ') {
                continue;
            }
            comment = true;
            line = rest;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(d) = line.strip_prefix('@') {
            let d = d.split('#').next().unwrap().trim();
            let (command, values) = d.split_once('=').unwrap();
            let values: Vec<String> = values.trim().split(',').map(str::to_string).collect();
            if command.starts_with("shaper") {
                shapers = if command.ends_with('-') {
                    ["ot", "fallback"]
                        .iter()
                        .map(|s| s.to_string())
                        .filter(|s| !values.contains(s))
                        .collect()
                } else {
                    values
                };
            } else {
                assert!(
                    command.starts_with("font-funcs") || command.starts_with("face-loader"),
                    "{}:{n}: unknown directive {d}",
                    file.display()
                );
            }
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        assert_eq!(parts.len(), 4, "{}:{n}: {line}", file.display());
        let font_field = parts[0];
        if font_field.starts_with('/') || font_field.starts_with("\"/") {
            *skipped += 1;
            continue;
        }
        if comment {
            continue;
        }
        let mut line_shaper = None;
        let mut options = Vec::new();
        let mut it = parts[1].split_whitespace();
        while let Some(o) = it.next() {
            let backend = ["--shaper", "--face-loader", "--font-funcs"]
                .into_iter()
                .find(|w| o.starts_with(w));
            match backend {
                Some(w) => {
                    let v = match o.split_once('=') {
                        Some((_, v)) => v.to_string(),
                        None => it.next().unwrap().to_string(),
                    };
                    if w == "--shaper" {
                        line_shaper = Some(v);
                    }
                }
                None => options.push(o.to_string()),
            }
        }
        let these = line_shaper.map_or_else(|| shapers.clone(), |s| vec![s]);
        for shaper in these {
            let id = if shaper == "ot" {
                format!("{suite}/{name}:{n}")
            } else {
                format!("{suite}/{name}:{n}@{shaper}")
            };
            out.push(Case {
                id,
                font: dir.join(font_field),
                options: options.clone(),
                unicodes: parts[2].to_string(),
                expected: parts[3].to_string(),
                shaper,
            });
        }
    }
    out
}

/// What `hb-shape --batch` prints for a command line: the first line of
/// its output, trimmed.
fn shape(args: &[String]) -> String {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    sofel_cli::shape::run_with_input(&args, &mut &b""[..], &mut out, &mut err);
    let out = String::from_utf8_lossy(&out).into_owned();
    out.lines().next().unwrap_or("").trim().to_string()
}

/// Runs a case: `Ok` when it passes, else both outputs.
fn run(case: &Case) -> Result<(), (String, String)> {
    let font = case.font.to_string_lossy().into_owned();
    let mut args = vec![font.clone(), "--unicodes".into(), case.unicodes.clone()];
    args.extend(case.options.iter().cloned());
    args.push(format!("--shaper={}", case.shaper));
    if case.expected != "*" {
        args.extend(
            ["--verify", "--unsafe-to-concat", "--safe-to-insert-tatweel"].map(String::from),
        );
    }
    let glyphs = shape(&args);
    if case.expected == "*" {
        return Ok(());
    }
    let (mut got, mut want) = (glyphs.clone(), case.expected.clone());
    if glyphs != case.expected && glyphs.contains("gid") {
        let reread = |g: &str| {
            shape(&[
                font.clone(),
                "--glyphs".into(),
                "--no-glyph-names".into(),
                g.to_string(),
            ])
        };
        got = reread(&glyphs);
        want = reread(&case.expected);
    }
    if want.is_empty() || got != want {
        return Err((glyphs, case.expected.clone()));
    }
    Ok(())
}

/// The characters of a `--unicodes` list.
fn codepoints(unicodes: &str) -> Vec<u32> {
    unicodes
        .split(',')
        .filter_map(|u| {
            let u = u.trim().trim_start_matches("U+").trim_start_matches("u+");
            u32::from_str_radix(u, 16).ok()
        })
        .collect()
}

#[test]
fn adr0028_trak_ptem_cases_are_aat_not_variations() {
    let data = paths::testdata().join("harfbuzz/test/shape/data");
    paths::require(&data, "HarfBuzz test data").unwrap();
    let mut skipped = 0;
    let cases = cases(
        "in-house",
        &data.join("in-house/tests/aat-trak.tests"),
        &mut skipped,
    );
    let reasons = sofel_conformance::expect::parse_reasons(
        &std::fs::read_to_string(paths::conformance_file(LIST)).unwrap(),
    );
    let ptem_cases: Vec<_> = cases
        .iter()
        .filter(|c| option(&c.options, "--font-ptem").is_some())
        .collect();
    assert_eq!(ptem_cases.len(), 7);
    for case in ptem_cases {
        let font = std::fs::read(&case.font).unwrap();
        let text = codepoints(&case.unicodes);
        let subject = Subject {
            data: &font,
            face: 0,
            options: &case.options,
            shaper: &case.shaper,
            text: &text,
            props: SegmentProperties::default(),
        };
        assert!(subject.misfit("aat").is_none(), "{}", case.id);
        assert!(subject.misfit("variations").is_some(), "{}", case.id);
        assert_eq!(subject.suggest(), "aat?", "{}", case.id);
        assert!(!reasons.contains_key(&case.id));
        assert_eq!(run(case), Ok(()), "{}", case.id);
    }
}

#[test]
fn harfbuzz_shaping_tests() {
    let testdata = paths::testdata();
    let data = testdata.join("harfbuzz/test/shape/data");
    paths::require(&data, "HarfBuzz test data").unwrap_or_else(|e| panic!("{e}"));
    let filter = std::env::var("SOFEL_SHAPE_FILTER").ok();
    let verbose = std::env::var_os("SOFEL_SHAPE_VERBOSE").is_some();
    let mut all = Vec::new();
    let mut skipped = 0usize;
    let mut disabled_files = 0usize;
    for suite in SUITES {
        let mut files: Vec<PathBuf> = std::fs::read_dir(data.join(suite).join("tests"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "tests"))
            .collect();
        files.sort();
        // The files HarfBuzz's own suite does not run, which HarfBuzz
        // fails (`<suite>/DISABLED`, the `disabled_*` lists of meson.build).
        let disabled: Vec<String> = std::fs::read_to_string(data.join(suite).join("DISABLED"))
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect();
        for f in files {
            let rel = format!("tests/{}", f.file_name().unwrap().to_string_lossy());
            if disabled.contains(&rel) {
                disabled_files += 1;
                continue;
            }
            all.extend(cases(suite, &f, &mut skipped));
        }
    }
    if let Some(f) = &filter {
        all.retain(|c| c.id.contains(f.as_str()));
    }
    let started = Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = all.len().div_ceil(threads).max(1);
    let results: Vec<(usize, Result<(), (String, String)>)> = std::thread::scope(|s| {
        let handles: Vec<_> = all
            .chunks(chunk)
            .enumerate()
            .map(|(k, part)| {
                s.spawn(move || {
                    part.iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let r = if ["ot", "fallback"].contains(&c.shaper.as_str()) {
                                run(c)
                            } else {
                                Err((format!("shaper {} not run", c.shaper), c.expected.clone()))
                            };
                            (k * chunk + i, r)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    let elapsed = started.elapsed();
    let mut passes = 0usize;
    let mut flat = Vec::new();
    for (i, r) in &results {
        let case = &all[*i];
        if let Err((got, want)) = r
            && verbose
        {
            println!(
                "FAIL {}: {} {}\n  got:  {got}\n  want: {want}",
                case.id,
                case.unicodes,
                case.options.join(" ")
            );
        }
        passes += usize::from(r.is_ok());
        flat.push((case.id.clone(), r.is_ok()));
    }
    println!(
        "{} cases ({skipped} lines with system fonts skipped, {disabled_files} files disabled by HarfBuzz), {passes} passed, {} failed, in {elapsed:.1?}",
        all.len(),
        all.len() - passes
    );
    let by_id: BTreeMap<&str, &Case> = all.iter().map(|c| (c.id.as_str(), c)).collect();
    let fonts: BTreeMap<&Path, Vec<u8>> = all
        .iter()
        .filter_map(|c| Some((c.font.as_path(), std::fs::read(&c.font).ok()?)))
        .collect();
    let texts: BTreeMap<&str, Vec<u32>> = all
        .iter()
        .map(|c| (c.id.as_str(), codepoints(&c.unicodes)))
        .collect();
    let subject = |i: usize| {
        let id = flat.get(i)?.0.as_str();
        let case = by_id.get(id)?;
        Some(Subject {
            data: fonts.get(case.font.as_path())?,
            face: option(&case.options, "--face-index")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            options: &case.options,
            shaper: &case.shaper,
            text: texts.get(id)?,
            props: SegmentProperties {
                direction: option(&case.options, "--direction")
                    .and_then(|d| Direction::from_bytes(d.as_bytes())),
                script: option(&case.options, "--script")
                    .and_then(|s| Script::from_iso15924(s.as_bytes())),
                language: option(&case.options, "--language")
                    .and_then(|l| Language::new(l.as_bytes())),
            },
        })
    };
    let suggest = std::env::var_os("SOFEL_SHAPE_SUGGEST").is_some();
    let (report, by_category) = shape::judge(&flat, &subject, LIST, suggest);
    for (cat, n) in &by_category {
        println!("  expected failures {cat}: {n}");
    }
    if filter.is_some() {
        return;
    }
    assert!(report.is_empty(), "{report}");
}

#[test]
fn i1_aat_morx_rearranges_all_sixteen_verbs() {
    let data =
        paths::testdata().join("harfbuzz/test/shape/data/text-rendering-tests/tests/MORX-2.tests");
    let mut skipped = 0;
    let suite = cases("text-rendering-tests", &data, &mut skipped);
    assert_eq!(suite.len(), 16);
    for case in suite {
        assert_eq!(run(&case), Ok(()), "{}", case.id);
    }
}

#[test]
fn i1_aat_contextual_morx_updates_marked_and_current_glyphs() {
    let data =
        paths::testdata().join("harfbuzz/test/shape/data/text-rendering-tests/tests/MORX-18.tests");
    let suite = cases("text-rendering-tests", &data, &mut 0);
    assert_eq!(suite.len(), 4);
    for case in suite {
        assert_eq!(run(&case), Ok(()), "{}", case.id);
    }
}

#[test]
fn i1_aat_insertion_uses_marked_and_current_positions() {
    let path =
        paths::testdata().join("harfbuzz/test/shape/data/text-rendering-tests/tests/MORX-29.tests");
    let suite = cases("text-rendering-tests", &path, &mut 0);
    assert_eq!(suite.len(), 4);
    for case in suite {
        assert_eq!(run(&case), Ok(()), "{}", case.id);
    }
}

#[test]
fn i1_aat_ligature_uses_the_component_stack() {
    let path =
        paths::testdata().join("harfbuzz/test/shape/data/text-rendering-tests/tests/MORX-27.tests");
    let suite = cases("text-rendering-tests", &path, &mut 0);
    assert_eq!(suite.len(), 3);
    for case in suite {
        assert_eq!(run(&case), Ok(()), "{}", case.id);
    }
}

#[test]
fn i1_aat_kerx_pairs_and_history_match_reference() {
    let path = paths::testdata().join("harfbuzz/test/shape/data/in-house/tests/aat-kerx.tests");
    let suite = cases("in-house", &path, &mut 0);
    for id in ["in-house/aat-kerx.tests:5", "in-house/aat-kerx.tests:23"] {
        let case = suite.iter().find(|case| case.id == id).unwrap();
        assert_eq!(run(case), Ok(()), "{id}");
    }
}

#[test]
fn i1_aat_kerx_attaches_marks_in_both_directions() {
    let path = paths::testdata().join("harfbuzz/test/shape/data/in-house/tests/aat-kerx.tests");
    let suite = cases("in-house", &path, &mut 0);
    for line in [13, 14, 19, 20] {
        let id = format!("in-house/aat-kerx.tests:{line}");
        let case = suite.iter().find(|case| case.id == id).unwrap();
        assert_eq!(run(case), Ok(()), "{id}");
    }
}

#[test]
fn i1_aat_ltag_matches_whole_language_subtags() {
    let path = paths::testdata().join("harfbuzz/test/shape/data/in-house/tests/aat-morx.tests");
    let suite = cases("in-house", &path, &mut 0);
    for line in 9..=17 {
        let id = format!("in-house/aat-morx.tests:{line}");
        let case = suite.iter().find(|case| case.id == id).unwrap();
        assert_eq!(run(case), Ok(()), "{id}");
    }
}

#[test]
fn i1_legacy_kern_pairs_classes_and_script_selection() {
    for (file, lines) in [
        ("kern-format2", &[1, 2, 3][..]),
        ("per-script-kern-fallback", &[13, 15][..]),
        ("tt-kern-gpos", &[1][..]),
    ] {
        let path = paths::testdata().join(format!(
            "harfbuzz/test/shape/data/in-house/tests/{file}.tests"
        ));
        let suite = cases("in-house", &path, &mut 0);
        for line in lines {
            let id = format!("in-house/{file}.tests:{line}");
            let case = suite.iter().find(|case| case.id == id).unwrap();
            assert_eq!(run(case), Ok(()), "{id}");
        }
    }
}

#[test]
fn i1_legacy_kern_machine_pops_positions_until_the_low_bit() {
    let path = paths::testdata().join("harfbuzz/test/shape/data/in-house/tests/kern-format1.tests");
    let suite = cases("in-house", &path, &mut 0);
    assert!(!suite.is_empty());
    for case in suite {
        assert_eq!(run(&case), Ok(()), "{}", case.id);
    }
}

#[test]
fn i1_legacy_mort_chain_runs_all_five_operations() {
    let path =
        paths::testdata().join("harfbuzz/test/shape/data/text-rendering-tests/tests/MORT-1.tests");
    let suite = cases("text-rendering-tests", &path, &mut 0);
    assert_eq!(suite.len(), 1);
    assert_eq!(run(&suite[0]), Ok(()));
}

#[test]
fn adr0060_i1_aat_shape_without_head() {
    let dir = paths::testdata().join("harfbuzz/test/shape/data/in-house/tests");
    for (file, lines) in [
        ("aat-morx.tests", &[6, 7][..]),
        ("aat-trak.tests", &[9][..]),
    ] {
        let inputs = cases("in-house", &dir.join(file), &mut 0);
        for line in lines {
            let id = format!("in-house/{file}:{line}");
            let case = inputs.iter().find(|c| c.id == id).unwrap();
            assert_eq!(run(case), Ok(()), "{id}");
        }
    }
}

#[test]
fn i1_aat_shape_all_glyph_fields_match_native_harfbuzz() {
    let root = paths::testdata().join("harfbuzz/test/shape/data");
    let mut compared = 0;
    let mut differences = Vec::new();
    for (suite, prefix) in [
        ("in-house", "aat-"),
        ("in-house", "kern-format"),
        ("text-rendering-tests", "MORX-"),
        ("text-rendering-tests", "MORT-"),
    ] {
        let dir = root.join(suite).join("tests");
        let mut files = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(prefix)
            })
            .collect::<Vec<_>>();
        files.sort();
        for file in files {
            for case in cases(suite, &file, &mut 0) {
                let value = |key| option(&case.options, key).unwrap_or("-");
                let flags = if case
                    .options
                    .iter()
                    .any(|s| s == "--remove-default-ignorables")
                {
                    "remove"
                } else {
                    "-"
                };
                let hex = case.unicodes.replace("U+", "");
                let native = std::process::Command::new(paths::hb_dump())
                    .arg("--aat-corpus-shape")
                    .arg(&case.font)
                    .args([
                        "0",
                        &hex,
                        "-",
                        value("--language"),
                        "-",
                        value("--features"),
                        "-",
                        value("--font-size"),
                        value("--font-ptem"),
                        flags,
                    ])
                    .output()
                    .unwrap();
                assert!(native.status.success(), "{}", case.id);
                let expected = String::from_utf8(native.stdout).unwrap();
                let mut args = vec![
                    case.font.to_string_lossy().into_owned(),
                    "--unicodes".into(),
                    case.unicodes.clone(),
                ];
                args.extend(
                    case.options
                        .iter()
                        .filter(|s| {
                            !matches!(s.as_str(), "--no-positions" | "--no-clusters" | "--ned")
                        })
                        .cloned(),
                );
                args.extend(
                    [
                        "--no-glyph-names",
                        "--show-flags",
                        "--unsafe-to-concat",
                        "--safe-to-insert-tatweel",
                    ]
                    .map(String::from),
                );
                let actual = shape(&args);
                if actual != expected.trim_end() {
                    differences.push(format!("{}\n{}\n{}", case.id, expected.trim_end(), actual));
                }
                compared += 1;
            }
        }
    }
    eprintln!("Compared {compared} AAT cases with native HarfBuzz");
    assert!(compared >= 254);
    assert!(
        differences.is_empty(),
        "{}/{} differ\n{}",
        differences.len(),
        compared,
        differences.join("\n")
    );
}
