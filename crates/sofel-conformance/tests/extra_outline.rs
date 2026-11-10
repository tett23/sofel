//! Selected FreeType outlines on every extra face (ADR 0031, I05-I06).

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

use sofel_conformance::extra::{Corpus, sample_glyphs};
use sofel_conformance::outline::{has_glyf, parse, sofel_glyph};
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_font::read::sfnt::FontFile;
use sofel_outline::{Scale, Scratch};

fn mandatory() -> BTreeMap<(String, u32), Vec<u32>> {
    let cases = include_str!("../../../conformance/font-cases.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .map(|p| {
            (
                p[0].to_string(),
                p[1].to_string(),
                p[2].parse::<u32>().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let mut result = BTreeMap::<(String, u32), BTreeSet<u32>>::new();
    for line in include_str!("../../../conformance/font-case-hb.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (id, shaped) = line.split_once('\t').unwrap();
        let (_, font, face) = cases.iter().find(|(case, _, _)| case == id).unwrap();
        for glyph in shaped
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split('|')
        {
            result
                .entry((font.clone(), *face))
                .or_default()
                .insert(glyph.split_once('=').unwrap().0.parse().unwrap());
        }
    }
    result
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect()
}

fn plan(
    bytes: &[u8],
    id: &str,
    required: &BTreeMap<(String, u32), Vec<u32>>,
) -> (String, Vec<Vec<u32>>) {
    let count = match FontFile::new(bytes).unwrap() {
        FontFile::Sfnt(_) => 1,
        FontFile::Collection(c) => c.num_fonts(),
    };
    let mut text = String::new();
    let mut all = Vec::new();
    for face in 0..count {
        let font = Font::new(bytes, face).unwrap();
        let n = font.num_glyphs();
        let mut must = required
            .get(&(id.to_string(), face))
            .cloned()
            .unwrap_or_default();
        if n > 0 {
            must.extend([0, n.saturating_sub(1)]);
        }
        let gids = sample_glyphs(n, &must).unwrap();
        for gid in &gids {
            text.push_str(&format!("{face} {gid}\n"));
        }
        all.push(gids);
    }
    (text, all)
}

fn snapshots() -> BTreeMap<&'static str, String> {
    let mut map = BTreeMap::new();
    let mut current = None;
    let mut text = String::new();
    for line in include_str!("../../../conformance/extra-outline-ft.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        if let Some(id) = line.strip_prefix("font ") {
            if let Some(old) = current.replace(id) {
                map.insert(old, std::mem::take(&mut text));
            }
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }
    if let Some(old) = current {
        map.insert(old, text);
    }
    map
}

#[test]
fn i05_sampled_outlines_match_freetype() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let snapshots = snapshots();
    let required = mandatory();
    let root = paths::testdata().join("extra-fonts");
    let mut scratch = Scratch::default();
    let mut compared = 0;
    let mut differences = Vec::new();
    for entry in &corpus.fonts {
        let bytes = std::fs::read(root.join(&entry.path)).unwrap();
        let (_, samples) = plan(&bytes, &entry.id, &required);
        let faces = parse(&snapshots[entry.id.as_str()]).unwrap();
        assert_eq!(faces.len(), samples.len());
        for (face_index, gids) in samples.iter().enumerate() {
            let index = u32::try_from(face_index).unwrap();
            let face = faces.get(&index).unwrap();
            let font = Font::new(&bytes, index).unwrap();
            assert_eq!(face.num_glyphs, font.num_glyphs());
            assert_eq!(face.glyphs.keys().copied().collect::<Vec<_>>(), *gids);
            assert!(
                has_glyf(&font),
                "{}#{face_index}: no comparable outline",
                entry.id
            );
            let outlines = sofel_outline::Outlines::new(&font);
            for gid in gids {
                compared += 1;
                let mut ours = vec![format!("advance {}", font.advance(GlyphId(*gid)))];
                ours.extend(sofel_glyph(&outlines, *gid, Scale::Unscaled, &mut scratch));
                if ours != face.glyphs[gid] {
                    differences.push(format!(
                        "{}#{face_index}:{gid}: FreeType {:?} / sofel {:?}",
                        entry.id,
                        face.glyphs[gid].first(),
                        ours.first()
                    ));
                }
            }
        }
    }
    eprintln!(
        "{compared} sampled FreeType outlines; {} differ",
        differences.len()
    );
    assert!(
        differences.is_empty(),
        "{}",
        differences
            .into_iter()
            .take(100)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn i06_outline_reference_matches_pinned_freetype() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let required = mandatory();
    let snapshots = snapshots();
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap();
    let root = paths::testdata().join("extra-fonts");
    for entry in &corpus.fonts {
        let path = root.join(&entry.path);
        let bytes = std::fs::read(&path).unwrap();
        let (plan, _) = plan(&bytes, &entry.id, &required);
        let plan_path = std::env::temp_dir().join(format!(
            "sofel-extra-outline-{}-{}.txt",
            std::process::id(),
            entry.id
        ));
        std::fs::write(&plan_path, plan).unwrap();
        let output = Command::new(&exe)
            .arg("--sampled-outline")
            .arg("unscaled")
            .arg(&path)
            .arg(&plan_path)
            .output()
            .unwrap();
        std::fs::remove_file(plan_path).unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            entry.id,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            snapshots[entry.id.as_str()],
            "{}",
            entry.id
        );
    }
}
