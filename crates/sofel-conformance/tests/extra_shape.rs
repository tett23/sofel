//! Fixed shaping cases on the new font corpus, also run on wasm (ADR 0031,
//! I05-I06). The HarfBuzz output is fixed by the corpus's pinned source.

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;

use sofel_conformance::extra::Corpus;
use sofel_conformance::{expect, paths};
use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::fvar::Fvar;

#[derive(Debug)]
struct Case<'a> {
    id: &'a str,
    font: &'a str,
    face: &'a str,
    codepoints: &'a str,
    script: &'a str,
    language: &'a str,
    direction: &'a str,
    feature: &'a str,
    coordinates: &'a str,
}

fn cases() -> Vec<Case<'static>> {
    include_str!("../../../conformance/font-cases.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            assert_eq!(parts.len(), 9, "{line}");
            Case {
                id: parts[0],
                font: parts[1],
                face: parts[2],
                codepoints: parts[3],
                script: parts[4],
                language: parts[5],
                direction: parts[6],
                feature: parts[7],
                coordinates: parts[8],
            }
        })
        .collect()
}

fn references() -> BTreeMap<&'static str, &'static str> {
    include_str!("../../../conformance/font-case-hb.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once('\t').unwrap())
        .collect()
}

#[test]
fn i05_cases_have_feature_effect_and_fixed_references() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let cases = cases();
    let refs = references();
    assert_eq!(cases.len(), 256);
    assert_eq!(refs.len(), cases.len());
    let ids = cases.iter().map(|case| case.id).collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), cases.len());
    for font in corpus
        .fonts
        .iter()
        .filter(|entry| entry.features.iter().any(|f| f == "shape"))
    {
        let own = cases
            .iter()
            .filter(|case| case.font == font.id)
            .collect::<Vec<_>>();
        assert!(!own.is_empty(), "{} has no shaping case", font.id);
        assert!(own.iter().all(|case| refs.contains_key(case.id)));
        let on = own.iter().find(|case| case.id.ends_with("-on")).unwrap();
        let off = own.iter().find(|case| case.id.ends_with("-off")).unwrap();
        assert_ne!(
            refs[on.id], refs[off.id],
            "{} feature has no effect",
            font.id
        );
    }
    let root = paths::testdata().join("extra-fonts");
    let mut axis_count = 0;
    let mut variable_fonts = 0;
    for entry in corpus
        .fonts
        .iter()
        .filter(|entry| entry.features.iter().any(|feature| feature == "variable"))
    {
        let bytes = std::fs::read(root.join(&entry.path)).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let fvar = Fvar::new(font.table(Tag::new(b"fvar")).unwrap()).unwrap();
        variable_fonts += 1;
        axis_count += fvar.axis_count();
        let own = cases
            .iter()
            .filter(|case| case.font == entry.id)
            .collect::<Vec<_>>();
        assert!(
            own.iter()
                .any(|case| case.id.ends_with(".default") && case.coordinates == "-")
        );
        assert!(
            own.iter()
                .any(|case| case.id.ends_with(".all-min") && case.coordinates != "-")
        );
        assert!(
            own.iter()
                .any(|case| case.id.ends_with(".all-max") && case.coordinates != "-")
        );
        for axis in fvar.axes() {
            let tag = axis.tag.to_string();
            for endpoint in ["min", "max"] {
                let suffix = format!(".axis.{tag}.{endpoint}");
                assert!(
                    own.iter().any(|case| case.id.ends_with(&suffix)
                        && case.coordinates.starts_with(&format!("{tag}="))),
                    "{} {suffix}",
                    entry.id
                );
            }
        }
    }
    assert_eq!((variable_fonts, axis_count), (8, 105));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn i05_reference_matches_pinned_harfbuzz() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let fonts = corpus
        .fonts
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let refs = references();
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let root = paths::testdata().join("extra-fonts");
    for case in cases() {
        let path = root.join(&fonts[case.font].path);
        let output = std::process::Command::new(&exe)
            .args([
                "--corpus-shape",
                path.to_str().unwrap(),
                case.face,
                case.codepoints,
                case.script,
                case.language,
                case.direction,
                case.feature,
                case.coordinates,
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            case.id,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim_end(),
            refs[case.id],
            "{}",
            case.id
        );
    }
}

#[test]
fn i06_shape_cases_match_harfbuzz() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let fonts = corpus
        .fonts
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let refs = references();
    let root = paths::testdata().join("extra-fonts");
    let mut results = Vec::new();
    for case in cases() {
        let entry = fonts.get(case.font).unwrap();
        let path = root.join(&entry.path);
        assert!(
            path.is_file(),
            "{} missing; run scripts/fetch-fonts.sh",
            path.display()
        );
        let mut args = vec![
            OsString::from(path),
            OsString::from(format!("--face-index={}", case.face)),
            OsString::from(format!("--unicodes={}", case.codepoints)),
            OsString::from(format!("--script={}", case.script)),
            OsString::from(format!("--language={}", case.language)),
            OsString::from(format!("--direction={}", case.direction)),
            OsString::from("--no-glyph-names"),
            OsString::from("--show-flags"),
        ];
        if case.feature != "-" {
            args.push(OsString::from(format!("--features={}", case.feature)));
        }
        if case.coordinates != "-" {
            args.push(OsString::from(format!("--variations={}", case.coordinates)));
        }
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = sofel_cli::shape::run_with_input(&args, &mut &b""[..], &mut out, &mut err);
        let got = String::from_utf8(out).unwrap();
        let want = refs.get(case.id).unwrap();
        let ok = code == 0 && got.trim_end() == *want;
        results.push((
            case.id.to_string(),
            ok,
            code,
            String::from_utf8_lossy(&err).into_owned(),
            format!(
                "exit {code}, got {}, expected {}, stderr {}",
                got.trim_end(),
                want,
                String::from_utf8_lossy(&err)
            ),
        ));
    }
    let expected = expect::parse(include_str!(
        "../../../conformance/expected-failures-extra-fonts.txt"
    ));
    let reasons = expect::parse_reasons(include_str!(
        "../../../conformance/expected-failures-extra-fonts.txt"
    ));
    for (id, ok, code, stderr, _) in &results {
        if !ok {
            let reason = reasons.get(id).map(String::as_str).unwrap_or("");
            if reason.starts_with("variations:") {
                assert_eq!(*code, 2, "{id}: expected unsupported variation option");
                assert!(
                    stderr.contains("--variations is not supported"),
                    "{id}: {stderr}"
                );
            } else if reason.starts_with("shaper:") {
                assert_eq!(*code, 0, "{id}: expected a shaper difference");
            }
        }
    }
    let problems = expect::judge(
        results.iter().map(|(id, ok, _, _, _)| (id.as_str(), *ok)),
        &expected,
    );
    let failed = results.iter().filter(|(_, ok, _, _, _)| !ok).count();
    eprintln!(
        "extra shaping: {} cases, {} passed, {failed} failed",
        results.len(),
        results.len() - failed
    );
    assert!(
        problems.is_empty(),
        "{}\n{}",
        expect::report(&problems, "conformance/expected-failures-extra-fonts.txt"),
        results
            .iter()
            .filter(|(_, ok, _, _, _)| !ok)
            .map(|(id, _, _, _, detail)| format!("{id}: {detail}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
