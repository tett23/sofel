//! Every Unicode scalar's nominal glyph in the fixed extra corpus
//! (ADR 0031, I04-I06). The HarfBuzz digests are fixed inputs on wasm.

#![allow(clippy::all, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use sofel_conformance::extra::{Corpus, nominal_digest};
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::sfnt::FontFile;

#[test]
fn i06_all_scalar_nominal_glyphs_match_harfbuzz() {
    let corpus = Corpus::parse(
        include_str!("../../../fonts.lock"),
        include_str!("../../../conformance/fonts.tsv"),
    )
    .unwrap();
    let hashes = include_str!("../../../conformance/extra-cmap-hashes.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let mut parts = line.split('\t');
            (
                (
                    parts.next().unwrap().to_string(),
                    parts.next().unwrap().parse::<u32>().unwrap(),
                ),
                u64::from_str_radix(parts.next().unwrap(), 16).unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let root = paths::testdata().join("extra-fonts");
    let mut faces = 0;
    for entry in &corpus.fonts {
        let path = root.join(&entry.path);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run scripts/fetch-fonts.sh", path.display()));
        let count = match FontFile::new(&bytes).unwrap() {
            FontFile::Sfnt(_) => 1,
            FontFile::Collection(c) => c.num_fonts(),
            FontFile::Dfont(d) => d.num_fonts(),
        };
        assert_eq!(
            count,
            if entry.id == "source-han" { 45 } else { 1 },
            "{}",
            entry.id
        );
        for face in 0..count {
            let font = Font::new(&bytes, face).unwrap();
            let got = nominal_digest(|c| font.nominal_glyph_raw(c).map_or(0, |g| g.0));
            let want = hashes.get(&(entry.id.clone(), face)).unwrap();
            assert_eq!(got, *want, "{}#{face}: nominal cmap differs", entry.id);
            faces += 1;
        }
    }
    assert_eq!(faces, 57);
    assert_eq!(hashes.len(), 57);
}
