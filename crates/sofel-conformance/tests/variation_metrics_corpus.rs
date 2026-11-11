//! Variable metrics at every corpus location against HarfBuzz (ADR 0027).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_wrap
)]

use std::cell::RefCell;
use std::path::Path;
use std::process::Command;

use sofel_conformance::{dump, fonts, hb, paths};
use sofel_font::read::{GlyphId, Tag};
use sofel_font::var::Location;
use sofel_font::{Font, HbScale, MetricsTag};
use sofel_outline::Outlines;

#[path = "support/variation_locations.rs"]
mod variation_locations;

fn sofel_face_at(font: &Font<'_>, location: &Location<'_>, hb: &hb::HbFace) -> hb::HbFace {
    let instance = font.instance(location);
    let outlines = Outlines::new(instance);
    let scratch = RefCell::new(sofel_color::Scratch::default());
    let scales = hb
        .scales
        .iter()
        .map(|reference| {
            let scale = HbScale::for_font(font, reference.x_scale, reference.y_scale);
            let box_at = |glyph: GlyphId| {
                sofel_color::scaled_glyph_extents(
                    &outlines,
                    glyph.0,
                    &scale,
                    &mut scratch.borrow_mut(),
                )
            };
            let metrics = MetricsTag::ALL
                .iter()
                .map(|&tag| {
                    let value = instance.scaled_metrics_position(&scale, tag);
                    let fallback =
                        instance.scaled_metrics_position_with_fallback(&scale, tag, box_at);
                    (
                        String::from_utf8_lossy(&tag.tag().0).into_owned(),
                        value.is_some(),
                        value.unwrap_or(0),
                        fallback,
                    )
                })
                .collect();
            let glyphs = (0..hb.num_glyphs)
                .map(|glyph| {
                    let glyph = GlyphId(glyph);
                    let (origin_x, origin_y) = instance.scaled_v_origin(&scale, glyph, box_at);
                    let box_values =
                        box_at(glyph).map(|e| [e.x_bearing, e.y_bearing, e.width, e.height]);
                    (
                        scale.em_scale_x(instance.advance(glyph) as i32),
                        instance.scaled_v_advance(&scale, glyph),
                        origin_x,
                        origin_y,
                        box_values,
                    )
                })
                .collect();
            let horizontal = instance.scaled_h_extents(&scale);
            let vertical = instance.scaled_v_extents(&scale);
            hb::Scaled {
                x_scale: reference.x_scale,
                y_scale: reference.y_scale,
                h_extents: [
                    horizontal.ascender,
                    horizontal.descender,
                    horizontal.line_gap,
                ],
                v_extents: [vertical.ascender, vertical.descender, vertical.line_gap],
                metrics,
                glyphs,
            }
        })
        .collect();
    hb::HbFace {
        index: hb.index,
        num_glyphs: font.num_glyphs(),
        upem: i32::from(font.hb_units_per_em()),
        scales,
    }
}

fn corpus_paths(data: &Path) -> Vec<std::path::PathBuf> {
    let mut font_paths = fonts::harfbuzz_fonts(data);
    for directory in ["harfbuzz/test/api/fonts", "harfbuzz/perf/fonts"] {
        font_paths.extend(
            std::fs::read_dir(data.join(directory))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_file()),
        );
    }
    font_paths.sort();
    font_paths.dedup();
    font_paths
}

#[test]
fn adr0027_i03_hvar_long_delta_uses_float_advance_addition() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/hvar-long-deltas.ttf");
    let exe = paths::hb_dump();
    paths::require(&path, "HVAR long-delta reference font").unwrap();
    paths::require(&exe, "hb-dump").unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let mut count = 0;
    for position in variation_locations::positions(&font)
        .into_iter()
        .filter(|position| position.id.starts_with("rand:"))
    {
        let output = Command::new(&exe)
            .arg("--metrics")
            .args(position.args())
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", position.id);
        let text = String::from_utf8(output.stdout).unwrap();
        let reference = hb::parse(&text).unwrap();
        let face = &reference[0];
        let location = position.location(&font);
        let ours = sofel_face_at(&font, &location, face);
        for item in hb::compare(&position.id, face, &ours) {
            assert!(item.ok, "{}: {}", item.id, item.detail);
        }
        count += 1;
    }
    assert_eq!(count, 8);
}

#[test]
fn adr0027_i03_harfbuzz_metrics_at_all_corpus_locations() {
    let data = paths::testdata();
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap();
    let filter = std::env::var("SOFEL_METRICS_FILTER").ok();
    let verbose = std::env::var_os("SOFEL_METRICS_VERBOSE").is_some();
    let mut checked = 0usize;
    let mut failures = 0usize;
    for path in corpus_paths(&data) {
        let id = fonts::id(&path, &data);
        if filter.as_ref().is_some_and(|filter| !id.contains(filter)) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        use sofel_read::sfnt::FontFile;
        let faces = match FontFile::new(&bytes) {
            Ok(FontFile::Sfnt(_)) => 1,
            Ok(FontFile::Collection(collection)) => collection.num_fonts(),
            Err(_) => continue,
        };
        for index in 0..faces {
            let Ok(font) = Font::new(&bytes, index) else {
                continue;
            };
            if font.var_axes().next().is_none() {
                continue;
            }
            // VARC variation is outside sofel ADR 0027's implementation scope.
            if font.table(Tag::new(b"VARC")).is_some() {
                continue;
            }
            for position in variation_locations::positions(&font) {
                let output = Command::new(&exe)
                    .arg("--metrics")
                    .args(position.args())
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{id} at {}", position.id);
                let text = String::from_utf8(output.stdout).unwrap();
                let reference = hb::parse(&text).unwrap();
                let face = &reference[index as usize];
                let location = position.location(&font);
                let ours = sofel_face_at(&font, &location, face);
                for item in hb::compare(&format!("{id}@{}", position.id), face, &ours) {
                    if !item.ok {
                        if verbose || failures < 100 {
                            eprintln!("{}: {}", item.id, item.detail);
                        }
                        failures += 1;
                    }
                }
                let baseline_output = Command::new(&exe)
                    .arg("--baselines")
                    .args(position.args())
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(baseline_output.status.success(), "{id} at {}", position.id);
                let baseline_text = String::from_utf8(baseline_output.stdout).unwrap();
                let baseline_reference = hb::parse_baselines(&baseline_text).unwrap();
                let baseline_face = &baseline_reference[index as usize];
                let baseline_ours =
                    hb::sofel_baselines_at(&font, font.instance(&location), baseline_face);
                for item in hb::compare_baselines(
                    &format!("{id}@{}", position.id),
                    baseline_face,
                    &baseline_ours,
                ) {
                    if !item.ok {
                        if verbose || failures < 100 {
                            eprintln!("{}: {}", item.id, item.detail);
                        }
                        failures += 1;
                    }
                }
                checked += 1;
            }
        }
    }
    eprintln!("checked {checked} face-locations, {failures} differences");
    assert!(checked > 0);
    assert_eq!(failures, 0);
}

#[test]
fn adr0027_i03_freetype_bearings_and_face_metrics_at_all_corpus_locations() {
    let data = paths::testdata();
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap();
    let filter = std::env::var("SOFEL_METRICS_FILTER").ok();
    let verbose = std::env::var_os("SOFEL_METRICS_VERBOSE").is_some();
    let mut checked = 0usize;
    let mut refused = 0usize;
    let mut failures = 0usize;
    for path in corpus_paths(&data) {
        let id = fonts::id(&path, &data);
        if filter.as_ref().is_some_and(|filter| !id.contains(filter)) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        use sofel_read::sfnt::FontFile;
        let faces = match FontFile::new(&bytes) {
            Ok(FontFile::Sfnt(_)) => 1,
            Ok(FontFile::Collection(collection)) => collection.num_fonts(),
            Err(_) => continue,
        };
        for index in 0..faces {
            let Ok(font) = Font::new(&bytes, index) else {
                continue;
            };
            if font.var_axes().next().is_none() {
                continue;
            }
            for position in variation_locations::positions(&font) {
                let output = Command::new(&exe)
                    .arg("--metrics")
                    .args(position.args())
                    .arg(&path)
                    .output()
                    .unwrap();
                if !output.status.success() {
                    eprintln!("reference refused {id}#{index}@{}", position.id);
                    refused += 1;
                    continue;
                }
                let text = String::from_utf8(output.stdout).unwrap();
                let reference = dump::parse_ft_dump(&text).unwrap();
                let face = &reference[index as usize];
                if face.error.is_some() {
                    refused += 1;
                    continue;
                }
                let location = position.location(&font);
                let instance = font.instance(&location);
                let metrics = instance.ft_face_metrics();
                let actual = [
                    metrics.ascender,
                    metrics.descender,
                    metrics.height,
                    metrics.max_advance_width,
                    metrics.max_advance_height,
                    metrics.underline_position,
                    metrics.underline_thickness,
                    metrics.bbox[0],
                    metrics.bbox[1],
                    metrics.bbox[2],
                    metrics.bbox[3],
                ];
                let expected: Vec<i64> = actual.iter().map(|&value| i64::from(value)).collect();
                if face.face_metrics != expected {
                    if verbose || failures < 100 {
                        eprintln!(
                            "{id}#{index}@{}:ft:face_metrics: FT {:?}, sofel {:?}",
                            position.id, face.face_metrics, expected
                        );
                    }
                    failures += 1;
                }
                assert!(
                    !face.bearings.is_empty(),
                    "bearing reference missing for {id}"
                );
                for &(glyph, reference_lsb, reference_tsb) in &face.bearings {
                    let glyph = GlyphId(glyph);
                    let actual_lsb = i64::from(instance.ft_left_side_bearing(glyph));
                    if actual_lsb != reference_lsb {
                        if verbose || failures < 100 {
                            eprintln!(
                                "{id}#{index}@{}:ft:left_side_bearing:{}: FT {reference_lsb}, sofel {actual_lsb}",
                                position.id, glyph.0
                            );
                        }
                        failures += 1;
                    }
                    if let Some(reference_tsb) = reference_tsb
                        && let Some(actual_tsb) = instance.ft_top_side_bearing(glyph, 0)
                        && i64::from(actual_tsb) != reference_tsb
                    {
                        if verbose || failures < 100 {
                            eprintln!(
                                "{id}#{index}@{}:ft:top_side_bearing:{}: FT {reference_tsb}, sofel {actual_tsb}",
                                position.id, glyph.0
                            );
                        }
                        failures += 1;
                    }
                }
                checked += 1;
            }
        }
    }
    eprintln!("checked {checked} face-locations, {refused} refused, {failures} differences");
    assert!(checked > 0);
    assert_eq!(failures, 0);
}

#[test]
#[ignore = "FreeType's slow glyph loading advances await ADR 0048"]
fn adr0027_i03_freetype_advances_at_all_corpus_locations() {
    let data = paths::testdata();
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap();
    let filter = std::env::var("SOFEL_METRICS_FILTER").ok();
    let mut checked = 0usize;
    let mut refused = 0usize;
    let mut failures = 0usize;
    for path in corpus_paths(&data) {
        let id = fonts::id(&path, &data);
        if filter.as_ref().is_some_and(|filter| !id.contains(filter)) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let faces = match sofel_read::sfnt::FontFile::new(&bytes) {
            Ok(sofel_read::sfnt::FontFile::Sfnt(_)) => 1,
            Ok(sofel_read::sfnt::FontFile::Collection(collection)) => collection.num_fonts(),
            Err(_) => continue,
        };
        for index in 0..faces {
            let Ok(font) = Font::new(&bytes, index) else {
                continue;
            };
            if font.var_axes().next().is_none() {
                continue;
            }
            for position in variation_locations::positions(&font) {
                let output = Command::new(&exe)
                    .arg("--metrics")
                    .args(position.args())
                    .arg(&path)
                    .output()
                    .unwrap();
                if !output.status.success() {
                    refused += 1;
                    continue;
                }
                let reference =
                    dump::parse_ft_dump(&String::from_utf8(output.stdout).unwrap()).unwrap();
                let face = &reference[index as usize];
                if face.error.is_some() {
                    refused += 1;
                    continue;
                }
                let location = position.location(&font);
                let instance = font.instance(&location);
                for &(glyph, reference) in &face.advances {
                    if instance.ft_h_advance(GlyphId(glyph)).map(i64::from) != reference {
                        failures += 1;
                    }
                }
                for &(glyph, reference) in &face.v_advances {
                    if instance.ft_v_advance(GlyphId(glyph)).map(i64::from) != Some(reference) {
                        failures += 1;
                    }
                }
                checked += 1;
            }
        }
    }
    eprintln!("checked {checked} face-locations, {refused} refused, {failures} differences");
    assert!(checked > 0);
    assert_eq!(failures, 0);
}
