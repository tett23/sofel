//! Selected glyph and font metrics against HarfBuzz for each extra face
//! (ADR 0031, I05). `hb-dump --sampled-metrics` reads one bounded plan.

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

use sofel_conformance::extra::{Corpus, sample_glyphs};
use sofel_conformance::{hb, paths};
use sofel_font::Font;
use sofel_font::read::sfnt::FontFile;

fn mandatory_glyphs() -> BTreeMap<(String, u32), Vec<u32>> {
    let cases = include_str!("../../../conformance/font-cases.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .map(|parts| {
            (
                parts[0].to_string(),
                parts[1].to_string(),
                parts[2].parse::<u32>().unwrap(),
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
        let set = result.entry((font.clone(), *face)).or_default();
        for part in shaped
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split('|')
        {
            set.insert(part.split_once('=').unwrap().0.parse::<u32>().unwrap());
        }
    }
    result
        .into_iter()
        .map(|(key, ids)| (key, ids.into_iter().collect()))
        .collect()
}

fn plan_for(
    font: &[u8],
    id: &str,
    mandatory: &BTreeMap<(String, u32), Vec<u32>>,
) -> (String, Vec<Vec<u32>>) {
    let count = match FontFile::new(font).unwrap() {
        FontFile::Sfnt(_) => 1,
        FontFile::Collection(c) => c.num_fonts(),
        FontFile::Dfont(d) => d.num_fonts(),
    };
    let mut plan = String::new();
    let mut selected = Vec::new();
    for face in 0..count {
        let f = Font::new(font, face).unwrap();
        let n = f.num_glyphs();
        let mut required = mandatory
            .get(&(id.to_string(), face))
            .cloned()
            .unwrap_or_default();
        if n > 0 {
            required.extend([0, n.saturating_sub(1)]);
        }
        let gids = sample_glyphs(n, &required).unwrap();
        for gid in &gids {
            plan.push_str(&format!("{face} {gid}\n"));
        }
        selected.push(gids);
    }
    (plan, selected)
}

#[cfg(not(target_arch = "wasm32"))]
fn dump(path: &Path, plan: &str) -> String {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let plan_path =
        std::env::temp_dir().join(format!("sofel-extra-metrics-{}.txt", std::process::id()));
    std::fs::write(&plan_path, plan).unwrap();
    let output = Command::new(exe)
        .arg("--sampled-metrics")
        .arg(path)
        .arg(&plan_path)
        .output()
        .unwrap();
    std::fs::remove_file(&plan_path).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn snapshots() -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    let mut current = None;
    let mut text = String::new();
    for line in include_str!("../../../conformance/extra-metrics-hb.txt").lines() {
        if line.starts_with('#') {
            continue;
        }
        if let Some(id) = line.strip_prefix("font ") {
            if let Some(previous) = current.replace(id) {
                out.insert(previous, std::mem::take(&mut text));
            }
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }
    if let Some(previous) = current {
        out.insert(previous, text);
    }
    out
}

#[test]
fn i05_sampled_metrics_match_harfbuzz() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let root = paths::testdata().join("extra-fonts");
    let mandatory = mandatory_glyphs();
    let mut results = Vec::new();
    let snapshots = snapshots();
    assert_eq!(snapshots.len(), corpus.fonts.len());
    for entry in &corpus.fonts {
        let path = root.join(&entry.path);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run scripts/fetch-fonts.sh", path.display()));
        let (_, gids) = plan_for(&bytes, &entry.id, &mandatory);
        let text = &snapshots[entry.id.as_str()];
        let reference = hb::parse(&text).unwrap();
        assert_eq!(reference.len(), gids.len());
        for (face, sample) in reference.iter().zip(gids) {
            assert_eq!(face.scales.len(), 1);
            assert_eq!(face.scales[0].glyphs.len(), sample.len());
            let font = Font::new(&bytes, face.index).unwrap();
            let ours = hb::sofel_face_sampled(&font, face, &sample);
            results.extend(hb::compare(&entry.id, face, &ours));
        }
    }
    let failed = results.iter().filter(|item| !item.ok).collect::<Vec<_>>();
    assert!(
        failed.is_empty(),
        "{} of {} metric items differ:\n{}",
        failed.len(),
        results.len(),
        failed
            .iter()
            .map(|i| format!("{}: {}", i.id, i.detail))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn i05_metric_reference_matches_pinned_harfbuzz() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let root = paths::testdata().join("extra-fonts");
    let mandatory = mandatory_glyphs();
    let snapshots = snapshots();
    for entry in &corpus.fonts {
        let path = root.join(&entry.path);
        let bytes = std::fs::read(&path).unwrap();
        let (plan, _) = plan_for(&bytes, &entry.id, &mandatory);
        assert_eq!(
            dump(&path, &plan),
            snapshots[entry.id.as_str()],
            "{}",
            entry.id
        );
    }
}
