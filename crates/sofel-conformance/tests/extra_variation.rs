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
use sofel_font::read::{GlyphId, Tag};
use sofel_font::var::{Location, Variation};
use sofel_outline::{Scale, Scratch};

struct Case<'a> {
    id: &'a str,
    font: &'a str,
    glyph: u32,
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
        let parsed = sofel_outline::Outlines::new(font.default_instance());
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

#[test]
fn adr0027_i07_gvar_nondefault_outline() {
    let reference = snapshots(include_str!(
        "../../../conformance/extra-variation-outline-ft.txt"
    ));
    let mut scratch = Scratch::default();
    for (case_id, filename, glyph, weight) in [
        ("arabic.wght.max", "arabic.ttf", 15, 900.0),
        ("amstelvar.wght.max", "amstelvar.ttf", 405, 1000.0),
    ] {
        let path = paths::testdata().join("extra-fonts/fonts").join(filename);
        let bytes = std::fs::read(path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let location = Location::new(
            &font,
            None,
            &[Variation {
                tag: Tag::new(b"wght"),
                value: weight,
            }],
        );
        let outlines = sofel_outline::Outlines::new(font.instance(&location));
        let actual = sofel_glyph(&outlines, glyph, Scale::Unscaled, &mut scratch);
        let expected = &parse(&reference[case_id]).unwrap()[&0].glyphs[&glyph];
        assert_eq!(&actual, &expected[1..], "{case_id}");
    }
}

#[test]
fn adr0027_i07_gvar_all_sampled_locations() {
    let reference = snapshots(include_str!(
        "../../../conformance/extra-variation-outline-ft.txt"
    ));
    let mut scratch = Scratch::default();
    for case in cases()
        .into_iter()
        .filter(|case| case.font.ends_with(".ttf") && case.coordinates != "-")
    {
        let path = paths::testdata().join("extra-fonts/fonts").join(case.font);
        let bytes = std::fs::read(path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let variations = case
            .coordinates
            .split(',')
            .map(|part| {
                let (tag, value) = part.split_once('=').unwrap();
                Variation {
                    tag: Tag::new(tag.as_bytes().try_into().unwrap()),
                    value: value.parse().unwrap(),
                }
            })
            .collect::<Vec<_>>();
        let location = Location::new(&font, None, &variations);
        let outlines = sofel_outline::Outlines::new(font.instance(&location));
        let actual = sofel_glyph(&outlines, case.glyph, Scale::Unscaled, &mut scratch);
        let expected = &parse(&reference[case.id]).unwrap()[&0].glyphs[&case.glyph];
        assert_eq!(&actual, &expected[1..], "{}", case.id);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn adr0027_i07_gvar_scaled_outline() {
    let path = paths::testdata().join("extra-fonts/fonts/arabic.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::new(
        &font,
        None,
        &[Variation {
            tag: Tag::new(b"wght"),
            value: 900.0,
        }],
    );
    let outlines = sofel_outline::Outlines::new(font.instance(&location));
    let mut scratch = Scratch::default();
    let plan = std::env::temp_dir().join(format!("sofel-gvar-scaled-{}.plan", std::process::id()));
    std::fs::write(&plan, "0 5\n0 6\n0 7\n0 15\n").unwrap();
    let output = Command::new(paths::ft_dump())
        .arg("--sampled-outline-var")
        .arg("16")
        .arg(&path)
        .arg(&plan)
        .arg("wght=900")
        .output()
        .unwrap();
    std::fs::remove_file(plan).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference = String::from_utf8(output.stdout).unwrap();
    let parsed = parse(&reference).unwrap();
    for glyph in [5, 6, 7, 15] {
        let actual = sofel_glyph(&outlines, glyph, Scale::pixels(&font, 16, 16), &mut scratch);
        let expected = &parsed[&0].glyphs[&glyph];
        assert_eq!(&actual, &expected[1..], "glyph {glyph}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn adr0027_i07_gvar_phantom_advance_without_hvar() {
    let path = paths::testdata().join("extra-fonts/fonts/arabic.ttf");
    let mut bytes = std::fs::read(path).unwrap();
    let count = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    let directory = &mut bytes[12..12 + count * 16];
    let record = directory
        .chunks_exact_mut(16)
        .find(|record| &record[..4] == b"HVAR")
        .unwrap();
    record[..4].copy_from_slice(b"XVAR");
    let modified = std::env::temp_dir().join(format!("sofel-no-hvar-{}.ttf", std::process::id()));
    std::fs::write(&modified, &bytes).unwrap();
    let output = Command::new(paths::hb_dump())
        .arg("--var-metrics")
        .arg(&modified)
        .arg("15")
        .arg("wght=900")
        .output()
        .unwrap();
    std::fs::remove_file(modified).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let faces = hb::parse(&String::from_utf8(output.stdout).unwrap()).unwrap();
    let expected_advance = faces[0].scales[0].glyphs[0].0;
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::new(
        &font,
        None,
        &[Variation {
            tag: Tag::new(b"wght"),
            value: 900.0,
        }],
    );
    assert_eq!(
        font.instance(&location).advance(GlyphId(15)).cast_signed(),
        expected_advance
    );
}

#[test]
fn adr0027_i07_gvar_vertical_phantoms() {
    let path = paths::testdata().join("extra-fonts/fonts/amstelvar.ttf");
    let bytes = std::fs::read(path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::new(
        &font,
        None,
        &[Variation {
            tag: Tag::new(b"wght"),
            value: 1000.0,
        }],
    );
    let reference = snapshots(include_str!(
        "../../../conformance/extra-variation-metrics-hb.txt"
    ));
    let faces = hb::parse(&reference["amstelvar.wght.max"]).unwrap();
    let values = faces[0].scales[0].glyphs[0];
    let instance = font.instance(&location);
    assert_eq!(instance.v_advance(GlyphId(405)), values.1);
    assert_eq!(
        instance
            .v_origin(GlyphId(405), |g| instance.glyf_extents(g))
            .1,
        values.3
    );
}

#[test]
fn adr0027_i07_gvar_sampled_metrics() {
    let reference = snapshots(include_str!(
        "../../../conformance/extra-variation-metrics-hb.txt"
    ));
    for case in cases()
        .into_iter()
        .filter(|case| case.font.ends_with(".ttf") && case.coordinates != "-")
    {
        let path = paths::testdata().join("extra-fonts/fonts").join(case.font);
        let bytes = std::fs::read(path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let variations = case
            .coordinates
            .split(',')
            .map(|part| {
                let (tag, value) = part.split_once('=').unwrap();
                Variation {
                    tag: Tag::new(tag.as_bytes().try_into().unwrap()),
                    value: value.parse().unwrap(),
                }
            })
            .collect::<Vec<_>>();
        let location = Location::new(&font, None, &variations);
        let instance = font.instance(&location);
        let expected = hb::parse(&reference[case.id]).unwrap()[0].scales[0].glyphs[0];
        let glyph = GlyphId(case.glyph);
        let origin = instance.v_origin(glyph, |g| instance.glyf_extents(g));
        let extents = instance
            .glyf_extents(glyph)
            .map(|e| [e.x_bearing, e.y_bearing, e.width, e.height]);
        assert_eq!(
            instance.advance(glyph).cast_signed(),
            expected.0,
            "{}.h_advance",
            case.id
        );
        assert_eq!(
            instance.v_advance(glyph),
            expected.1,
            "{}.v_advance",
            case.id
        );
        assert_eq!(origin, (expected.2, expected.3), "{}.v_origin", case.id);
        assert_eq!(extents, expected.4, "{}.extents", case.id);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn adr0027_i07_gvar_composite_extents() {
    let path = paths::testdata().join("extra-fonts/fonts/arabic.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::new(
        &font,
        None,
        &[Variation {
            tag: Tag::new(b"wght"),
            value: 900.0,
        }],
    );
    for glyph in [5, 6, 7, 15] {
        let actual = font.instance(&location).glyf_extents(GlyphId(glyph));
        let output = Command::new(paths::hb_dump())
            .arg("--var-metrics")
            .arg(&path)
            .arg(glyph.to_string())
            .arg("wght=900")
            .output()
            .unwrap();
        assert!(output.status.success());
        let reference = hb::parse(&String::from_utf8(output.stdout).unwrap()).unwrap();
        let expected = reference[0].scales[0].glyphs[0].4;
        assert_eq!(
            actual.map(|e| [e.x_bearing, e.y_bearing, e.width, e.height]),
            expected,
            "glyph {glyph}"
        );
    }
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
