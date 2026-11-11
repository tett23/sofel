//! Variable metrics at every corpus location against HarfBuzz (ADR 0027).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation
)]

use std::path::Path;
use std::process::Command;

use sofel_conformance::{dump, fonts, paths};
use sofel_font::Font;
use sofel_font::read::GlyphId;

#[path = "support/variation_locations.rs"]
mod variation_locations;

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
fn adr0048_i02_freetype_advances_at_all_corpus_locations() {
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
            Ok(FontFile::Dfont(dfont)) => dfont.num_fonts(),
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
                let text = text
                    .lines()
                    .filter(|line| !line.starts_with("bearing "))
                    .collect::<Vec<_>>()
                    .join("\n");
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
                let mut advance_scratch = sofel_font::FtAdvanceScratch::default();
                for &(glyph, reference) in &face.advances {
                    let actual = instance
                        .ft_h_advance_with_scratch(GlyphId(glyph), &mut advance_scratch)
                        .map(i64::from);
                    if reference != actual {
                        if verbose || failures < 100 {
                            eprintln!(
                                "{id}#{index}@{}:ft:h_advance:{glyph}: FT {reference:?}, sofel {actual:?}",
                                position.id
                            );
                        }
                        failures += 1;
                    }
                }
                for &(glyph, reference) in &face.v_advances {
                    let actual = instance
                        .ft_v_advance_with_scratch(GlyphId(glyph), &mut advance_scratch)
                        .map(i64::from);
                    if Some(reference) != actual {
                        if verbose || failures < 100 {
                            eprintln!(
                                "{id}#{index}@{}:ft:v_advance:{glyph}: FT {reference}, sofel {actual:?}",
                                position.id
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
fn adr0048_i01_first_single_and_batch_advances_match() {
    let data = paths::testdata();
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap();
    let names = [
        "harfbuzz/test/api/fonts/Mada-VF.ttf",
        "harfbuzz/test/subset/data/fonts/RobotoFlex-Variable.ttf",
    ];
    let mut checked = 0usize;
    let mut differences = 0usize;
    for name in names {
        let path = data.join(name);
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let position = variation_locations::positions(&font)
            .into_iter()
            .find(|position| position.id == "named:0")
            .unwrap();
        let location = position.location(&font);
        let instance = font.instance(&location);
        let output = Command::new(&exe)
            .arg("--advances")
            .args(position.args())
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        let mut scratch = sofel_font::FtAdvanceScratch::default();
        for line in text.lines().filter(|line| line.starts_with("advance ")) {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let glyph = fields[3].parse::<u32>().unwrap();
            let error = fields[4].parse::<u32>().unwrap();
            let value = fields[5].parse::<i64>().unwrap();
            let expected = (error == 0)
                .then_some(value)
                .and_then(|value| u16::try_from(value).ok());
            let actual = if fields[1] == "h" {
                instance.ft_h_advance_with_scratch(GlyphId(glyph), &mut scratch)
            } else {
                instance.ft_v_advance_with_scratch(GlyphId(glyph), &mut scratch)
            };
            if expected != actual {
                if differences < 20 {
                    eprintln!("{name}: {line}: sofel {actual:?}");
                }
                differences += 1;
            }
            checked += 1;
        }
    }
    eprintln!("checked {checked} advances, {differences} differences");
    assert_eq!(checked, 5904);
    assert_eq!(differences, 0);
}
