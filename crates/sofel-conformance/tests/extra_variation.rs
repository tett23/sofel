//! Non-default metric and outline cases of ADR 0031 (I05-I06).

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

use sofel_conformance::expect;
use sofel_conformance::hb;
use sofel_conformance::outline::{parse, sofel_glyph};
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_outline::{Scale, Scratch};

struct Case<'a> {
    id: &'a str,
    font: &'a str,
    glyph: u32,
    #[cfg(not(target_arch = "wasm32"))]
    coordinates: &'a str,
}

fn cases() -> Vec<Case<'static>> {
    include_str!("../../../conformance/extra-variation-cases.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), 4, "{line}");
            Case {
                id: fields[0],
                font: fields[1],
                glyph: fields[2].parse().unwrap(),
                #[cfg(not(target_arch = "wasm32"))]
                coordinates: fields[3],
            }
        })
        .collect()
}

fn snapshots(text: &'static str) -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    let mut current = None;
    let mut body = String::new();
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        if let Some(id) = line.strip_prefix("case ") {
            if let Some(previous) = current.replace(id) {
                out.insert(previous, std::mem::take(&mut body));
            }
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(previous) = current {
        out.insert(previous, body);
    }
    out
}

#[test]
fn i05_nondefault_metrics_and_outlines_match_or_named_failure() {
    let cases = cases();
    let metrics = snapshots(include_str!(
        "../../../conformance/extra-variation-metrics-hb.txt"
    ));
    let outlines = snapshots(include_str!(
        "../../../conformance/extra-variation-outline-ft.txt"
    ));
    let expected = expect::parse(include_str!(
        "../../../conformance/expected-failures-extra-variation.txt"
    ));
    let reasons = expect::parse_reasons(include_str!(
        "../../../conformance/expected-failures-extra-variation.txt"
    ));
    assert_eq!(cases.len(), 10);
    assert_eq!(metrics.len(), cases.len());
    assert_eq!(outlines.len(), cases.len());
    let mut scratch = Scratch::default();
    let mut results = Vec::new();
    for case in &cases {
        let path = paths::testdata().join("extra-fonts/fonts").join(case.font);
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();

        let hb_reference = hb::parse(&metrics[case.id]).unwrap();
        assert_eq!(hb_reference.len(), 1);
        assert_eq!(hb_reference[0].scales.len(), 1);
        assert_eq!(hb_reference[0].scales[0].glyphs.len(), 1);
        let ours = hb::sofel_face_sampled(&font, &hb_reference[0], &[case.glyph]);
        let metric_items = hb::compare(case.id, &hb_reference[0], &ours);
        let metric_ok = metric_items.iter().all(|item| item.ok);
        results.push((format!("{}.metrics", case.id), metric_ok));

        let ft_reference = parse(&outlines[case.id]).unwrap();
        let ft_glyph = &ft_reference[&0].glyphs[&case.glyph];
        let mut outline = vec![format!(
            "advance {}",
            font.default_instance().advance(GlyphId(case.glyph))
        )];
        let parsed = sofel_outline::Outlines::new(&font);
        outline.extend(sofel_glyph(
            &parsed,
            case.glyph,
            Scale::Unscaled,
            &mut scratch,
        ));
        results.push((format!("{}.outline", case.id), &outline == ft_glyph));
    }
    for (id, ok) in &results {
        if !ok {
            assert!(
                reasons
                    .get(id)
                    .is_some_and(|reason| reason.starts_with("variations:")),
                "{id}: unexplained metric or outline difference"
            );
        }
    }
    let problems = expect::judge(results.iter().map(|(id, ok)| (id.as_str(), *ok)), &expected);
    let failed = results.iter().filter(|(_, ok)| !ok).count();
    eprintln!(
        "extra variation: {} cases, {failed} expected failures",
        results.len()
    );
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(
            &problems,
            "conformance/expected-failures-extra-variation.txt"
        )
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn i06_nondefault_references_match_pinned_harfbuzz_and_freetype() {
    let hb_dump = paths::hb_dump();
    let ft_dump = paths::ft_dump();
    paths::require(&hb_dump, "hb-dump").unwrap();
    paths::require(&ft_dump, "ft-dump").unwrap();
    let metrics = snapshots(include_str!(
        "../../../conformance/extra-variation-metrics-hb.txt"
    ));
    let outlines = snapshots(include_str!(
        "../../../conformance/extra-variation-outline-ft.txt"
    ));
    for case in cases() {
        let path = paths::testdata().join("extra-fonts/fonts").join(case.font);
        let output = Command::new(&hb_dump)
            .arg("--var-metrics")
            .arg(&path)
            .arg(case.glyph.to_string())
            .arg(case.coordinates)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", case.id);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            metrics[case.id],
            "{}",
            case.id
        );

        let plan = std::env::temp_dir().join(format!(
            "sofel-extra-variation-{}-{}.plan",
            std::process::id(),
            case.id
        ));
        std::fs::write(&plan, format!("0 {}\n", case.glyph)).unwrap();
        let mut command = Command::new(&ft_dump);
        command.arg(if case.coordinates == "-" {
            "--sampled-outline"
        } else {
            "--sampled-outline-var"
        });
        command.arg("unscaled").arg(&path).arg(&plan);
        if case.coordinates != "-" {
            command.arg(case.coordinates);
        }
        let output = command.output().unwrap();
        std::fs::remove_file(plan).unwrap();
        assert!(output.status.success(), "{}", case.id);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            outlines[case.id],
            "{}",
            case.id
        );
    }
}
