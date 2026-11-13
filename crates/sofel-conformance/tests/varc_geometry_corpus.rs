//! VARC outlines at every sofel ADR 0027 location against pinned HarfBuzz
//! (sofel ADR 0067, invariant 1).

#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use sofel_conformance::{fonts, paths};
use sofel_font::Font;
use sofel_font::read::{GlyphId, Tag, sfnt::FontFile, varc::Varc};
use sofel_outline::{HbOutlineCommand, Outlines};

#[path = "support/variation_locations.rs"]
mod variation_locations;

fn corpus_paths(data: &Path) -> Vec<PathBuf> {
    let mut paths = fonts::harfbuzz_fonts(data);
    for directory in ["harfbuzz/test/api/fonts", "harfbuzz/perf/fonts"] {
        if let Ok(entries) = std::fs::read_dir(data.join(directory)) {
            paths.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_file()),
            );
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn path_line(command: HbOutlineCommand) -> String {
    let point = |p: sofel_geom::PointF| format!("{:08x} {:08x}", p.x.to_bits(), p.y.to_bits());
    match command {
        HbOutlineCommand::MoveTo(p) => format!("M {}", point(p)),
        HbOutlineCommand::LineTo(p) => format!("L {}", point(p)),
        HbOutlineCommand::QuadTo(c, p) => format!("Q {} {}", point(c), point(p)),
        HbOutlineCommand::CubicTo(a, b, p) => {
            format!("C {} {} {}", point(a), point(b), point(p))
        }
        HbOutlineCommand::Close => "Z".to_string(),
    }
}

fn parse_paths(text: &str) -> BTreeMap<u32, Vec<String>> {
    let mut paths = BTreeMap::new();
    let mut current = None;
    let mut lines = Vec::new();
    for line in text.lines() {
        if let Some(glyph) = line.strip_prefix("glyph ") {
            assert!(current.is_none(), "nested glyph");
            current = Some(glyph.parse().unwrap());
        } else if line == "end" {
            let glyph = current.take().expect("end without glyph");
            assert!(paths.insert(glyph, std::mem::take(&mut lines)).is_none());
        } else {
            assert!(current.is_some(), "command outside glyph");
            lines.push(line.to_string());
        }
    }
    assert!(current.is_none(), "unfinished glyph");
    paths
}

#[test]
fn adr0067_i1_varc_paths_match_harfbuzz_at_all_corpus_locations() {
    let data = paths::testdata();
    let exe = std::env::var_os("SOFEL_HB_VARC_REFERENCE").map_or_else(
        || paths::root().join("target/hb-varc-reference/hb-varc-reference"),
        PathBuf::from,
    );
    paths::require(&exe, "hb-varc-reference").unwrap();
    let mut candidates = 0u64;
    let mut faces = 0u64;
    let mut positions = 0u64;
    let mut glyphs = 0u64;
    let mut differences = 0u64;
    for path in corpus_paths(&data) {
        let bytes = std::fs::read(&path).unwrap();
        let parsed = match FontFile::new(&bytes) {
            Ok(file) => file,
            Err(_) => continue,
        };
        let face_count = match &parsed {
            FontFile::Sfnt(_) => 1,
            FontFile::Collection(collection) => collection.num_fonts(),
            FontFile::Dfont(dfont) => dfont.num_fonts(),
        };
        for face_index in 0..face_count {
            let sfnt = match &parsed {
                FontFile::Sfnt(sfnt) => *sfnt,
                FontFile::Collection(collection) => match collection.font(face_index) {
                    Ok(sfnt) => sfnt,
                    Err(_) => continue,
                },
                FontFile::Dfont(dfont) => match dfont.font(face_index) {
                    Ok(sfnt) => sfnt,
                    Err(_) => continue,
                },
            };
            let Some(varc_data) = sfnt.table(Tag::new(b"VARC")) else {
                continue;
            };
            if sfnt.table(Tag::new(b"fvar")).is_none() {
                continue;
            }
            candidates += 1;
            let font = Font::new(&bytes, face_index)
                .unwrap_or_else(|error| panic!("{}#{face_index}: {error}", path.display()));
            let varc = Varc::new(varc_data).unwrap();
            let covered = (0..font.num_glyphs())
                .filter(|&glyph| varc.coverage().get(GlyphId(glyph)).is_some())
                .collect::<Vec<_>>();
            assert!(!covered.is_empty(), "{}#{face_index}", path.display());
            let glyph_list = covered
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            faces += 1;
            let name = fonts::id(&path, &data);
            let mut face_differences = 0u64;
            for position in variation_locations::positions(&font) {
                positions += 1;
                let output = Command::new(&exe)
                    .arg(&path)
                    .arg(face_index.to_string())
                    .arg(&glyph_list)
                    .args(position.args())
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{name}#{face_index}@{}: {}",
                    position.id,
                    String::from_utf8_lossy(&output.stderr)
                );
                let reference = parse_paths(&String::from_utf8(output.stdout).unwrap());
                assert_eq!(
                    reference.len(),
                    covered.len(),
                    "{name}#{face_index}@{}",
                    position.id
                );
                let location = position.location(&font);
                let outlines = Outlines::new(font.instance(&location));
                for &glyph in &covered {
                    glyphs += 1;
                    let actual = outlines
                        .varc_glyph_path(GlyphId(glyph), sofel_font::HbScale::unscaled(&font))
                        .map_or_else(
                            || vec!["error".to_string()],
                            |path| path.into_iter().map(path_line).collect(),
                        );
                    let expected = &reference[&glyph];
                    if &actual != expected {
                        differences += 1;
                        face_differences += 1;
                        if differences <= 30 {
                            let index = actual
                                .iter()
                                .zip(expected)
                                .position(|(actual, expected)| actual != expected)
                                .unwrap_or(0);
                            eprintln!(
                                "difference {name}#{face_index}:{glyph}@{} length {} / {} command {index}: {:?} / {:?}",
                                position.id,
                                actual.len(),
                                expected.len(),
                                actual.get(index),
                                expected.get(index)
                            );
                        }
                    }
                }
            }
            eprintln!(
                "{name}#{face_index}: {} covered glyphs, {face_differences} differences",
                covered.len()
            );
        }
    }
    eprintln!(
        "VARC: {candidates} candidates, {faces} faces, {positions} positions, {glyphs} glyphs, {differences} differences"
    );
    assert!(faces > 0, "VARC corpus is empty");
    assert_eq!(faces, candidates);
    assert_eq!(differences, 0);
}
