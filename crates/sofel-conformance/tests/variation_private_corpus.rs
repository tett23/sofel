//! CFF2 Private DICT blend at every variation location (ADR 0030 I06).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic
)]

use sofel_conformance::{fonts, paths};
use sofel_font::read::Tag;
use sofel_font::{Cff, Font};
use sofel_read::sfnt::FontFile;
use std::path::Path;
use std::process::Command;

#[path = "support/variation_locations.rs"]
mod variation_locations;

fn corpus_paths(data: &Path) -> Vec<std::path::PathBuf> {
    let mut paths = fonts::harfbuzz_fonts(data);
    for directory in ["harfbuzz/test/api/fonts", "harfbuzz/perf/fonts"] {
        if let Ok(entries) = std::fs::read_dir(data.join(directory)) {
            paths.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|p| p.is_file()),
            );
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

#[test]
fn adr0030_i06_cff2_private_blend_matches_freetype_at_all_locations() {
    let data = paths::testdata();
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap();
    let mut checked = 0usize;
    let mut dictionaries = 0usize;
    let mut failures = 0usize;
    for path in corpus_paths(&data) {
        let bytes = std::fs::read(&path).unwrap();
        let faces = match FontFile::new(&bytes) {
            Ok(FontFile::Sfnt(_)) => 1,
            Ok(FontFile::Collection(collection)) => collection.num_fonts(),
            Err(_) => continue,
        };
        for index in 0..faces {
            let Ok(font) = Font::new(&bytes, index) else {
                continue;
            };
            if font.table(Tag::new(b"CFF2")).is_none() || font.var_axes().next().is_none() {
                continue;
            }
            let id = fonts::id(&path, &data);
            let cff = Cff::new(&font).unwrap();
            for position in variation_locations::positions(&font) {
                let output = Command::new(&exe)
                    .arg("--cff2-private")
                    .args(position.args())
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{id}#{index}@{}: {}",
                    position.id,
                    String::from_utf8_lossy(&output.stderr)
                );
                let text = String::from_utf8(output.stdout).unwrap();
                let marker = format!("face {index}\n");
                let block = text
                    .split(&marker)
                    .nth(1)
                    .unwrap()
                    .split("\nend\n")
                    .next()
                    .unwrap();
                let location = position.location(&font);
                let instance = font.instance(&location);
                let blend = instance.ft_blend_weights(&cff);
                for line in block.lines() {
                    if line.starts_with("private_error") {
                        failures += 1;
                        eprintln!("{id}#{index}@{}:ft:{line}", position.id);
                        continue;
                    }
                    if let Some(values) = line.strip_prefix("private_array ") {
                        let mut words = values.split_ascii_whitespace();
                        let fd: u32 = words.next().unwrap().parse().unwrap();
                        let key = words.next().unwrap();
                        let count: usize = words.next().unwrap().parse().unwrap();
                        let reference: Vec<i64> = words.map(|s| s.parse().unwrap()).collect();
                        assert_eq!(reference.len(), count, "{id}#{index}@{}", position.id);
                        let private = cff.private_dict_with_blend(fd, &blend).unwrap_or_default();
                        let actual = match key {
                            "blue_values" => private.blue_values.as_slice(),
                            "other_blues" => private.other_blues.as_slice(),
                            "family_blues" => private.family_blues.as_slice(),
                            "family_other_blues" => private.family_other_blues.as_slice(),
                            "stem_snap_h" => private.stem_snap_h.as_slice(),
                            "stem_snap_v" => private.stem_snap_v.as_slice(),
                            _ => panic!("unknown Private DICT array {key}"),
                        };
                        if reference != actual {
                            failures += 1;
                            eprintln!(
                                "{id}#{index}@{}:ft:private_array:{fd}:{key}: FT {reference:?}, sofel {actual:?}",
                                position.id
                            );
                        }
                        continue;
                    }
                    let Some(values) = line.strip_prefix("private ") else {
                        continue;
                    };
                    let numbers: Vec<i64> = values
                        .split_ascii_whitespace()
                        .map(|s| s.parse().unwrap())
                        .collect();
                    assert_eq!(numbers.len(), 16, "{id}#{index}@{}", position.id);
                    let fd = u32::try_from(numbers[0]).unwrap();
                    let private = cff.private_dict_with_blend(fd, &blend).unwrap_or_default();
                    let actual = [
                        i64::from(fd),
                        i64::from(private.vsindex),
                        private.std_hw,
                        private.std_vw,
                        private.blue_scale,
                        private.blue_shift,
                        private.blue_fuzz,
                        private.expansion_factor,
                        private.initial_random_seed,
                        private.default_width_x,
                        private.nominal_width_x,
                        i64::from(private.force_bold),
                        private.force_bold_threshold,
                        i64::from(private.len_iv),
                        i64::from(private.language_group),
                        i64::try_from(private.subrs).unwrap_or(-1),
                    ];
                    if numbers != actual {
                        failures += 1;
                        eprintln!(
                            "{id}#{index}@{}:ft:private:{fd}: FT {numbers:?}, sofel {actual:?}",
                            position.id
                        );
                    }
                    dictionaries += 1;
                }
                checked += 1;
            }
        }
    }
    eprintln!(
        "checked {checked} CFF2 face-locations, {dictionaries} Private DICTs, {failures} differences"
    );
    assert!(checked > 0);
    assert!(dictionaries > 0);
    assert_eq!(failures, 0);
}
