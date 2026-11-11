//! CFF2 geometry at every ADR 0027 location against pinned FreeType.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use sofel_conformance::outline::{scale_for, sofel_glyph};
use sofel_conformance::{fonts, paths};
use sofel_font::Font;
use sofel_font::read::{Tag, sfnt::FontFile};
use sofel_outline::{Outlines, Scratch};
use sofel_raster::Rasterizer;

#[allow(dead_code)]
#[path = "support/ft_stream.rs"]
mod ft_stream;
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

#[test]
fn adr0027_cff2_geometry_matches_freetype_at_all_corpus_locations() {
    let data = paths::testdata();
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap();
    let mut candidates = 0u64;
    let mut faces = 0u64;
    let mut positions = 0u64;
    let mut checked = [0u64; 2];
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
            if sfnt.table(Tag::new(b"CFF2")).is_none() || sfnt.table(Tag::new(b"fvar")).is_none() {
                continue;
            }
            candidates += 1;
            let font = Font::new(&bytes, face_index)
                .unwrap_or_else(|error| panic!("{}#{face_index}: {error}", path.display()));
            assert!(
                font.var_axes().next().is_some(),
                "{}#{face_index}: no axes",
                path.display()
            );
            faces += 1;
            let name = fonts::id(&path, &data);
            let face_positions = variation_locations::positions(&font);
            eprintln!("{name}#{face_index}: {} positions", face_positions.len());
            for position in face_positions {
                positions += 1;
                let location = position.location(&font);
                let outlines = Outlines::new(font.instance(&location));
                let mut scratch = Scratch::default();
                let mut raster = Rasterizer::new();
                let raster_sizes = if position.id == "default"
                    || position.id.starts_with("named:")
                    || matches!(position.id.as_str(), "rand:0" | "rand:1")
                {
                    &["12", "24", "48"][..]
                } else {
                    &[]
                };
                for (mode, size) in ["unscaled", "16", "48"]
                    .into_iter()
                    .map(|size| ("--outline", size))
                    .chain(raster_sizes.iter().map(|size| ("--raster", *size)))
                {
                    let scale = scale_for(&font, size);
                    let mut glyphs = 0u32;
                    let ran = ft_stream::reference(
                        Command::new(&exe)
                            .args([mode, size])
                            .args(position.args())
                            .arg(&path),
                        |face, glyph, expected| {
                            if face != face_index {
                                return;
                            }
                            let ours = if mode == "--raster" {
                                sofel_conformance::raster::sofel_glyph(
                                    &outlines,
                                    glyph,
                                    size.parse().unwrap(),
                                    &mut scratch,
                                    &mut raster,
                                )
                            } else {
                                sofel_glyph(&outlines, glyph, scale, &mut scratch)
                            };
                            if ours != expected {
                                differences += 1;
                                if differences <= 20 {
                                    eprintln!(
                                        "difference {name}#{face}:{mode}:{size}:{glyph}@{}",
                                        position.id
                                    );
                                }
                            }
                            glyphs += 1;
                        },
                    )
                    .unwrap_or_else(|error| panic!("{name}#{face_index}@{}: {error}", position.id));
                    assert!(
                        ran,
                        "FreeType rejected {name}#{face_index}:{mode}:{size}@{}",
                        position.id
                    );
                    assert_eq!(
                        glyphs,
                        font.num_glyphs(),
                        "{name}#{face_index}:{mode}:{size}@{}",
                        position.id
                    );
                    checked[usize::from(mode == "--raster")] += u64::from(glyphs);
                }
            }
        }
    }
    eprintln!(
        "CFF2: {candidates} candidates, {faces} faces, {positions} positions, {} outlines, {} rasters, {differences} differences",
        checked[0], checked[1]
    );
    assert!(faces > 0, "CFF2 corpus is empty");
    assert_eq!(faces, candidates, "some CFF2 faces were not checked");
    assert_eq!(differences, 0);
}
