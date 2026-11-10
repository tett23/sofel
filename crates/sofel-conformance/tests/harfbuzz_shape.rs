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
//! `out-of-scope:<name>`; and, pending a decision, `dfont` and
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
