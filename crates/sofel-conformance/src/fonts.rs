//! The test fonts: the sfnt fonts and collections among HarfBuzz's test
//! fonts (ADR 0008).

use std::path::{Path, PathBuf};

const SFNT_MAGIC: [[u8; 4]; 5] = [*b"\x00\x01\x00\x00", *b"OTTO", *b"true", *b"typ1", *b"ttcf"];

/// The sfnt fonts and collections under the fetched HarfBuzz tests, sorted.
pub fn harfbuzz_fonts(testdata: &Path) -> Vec<PathBuf> {
    let hb = testdata.join("harfbuzz/test");
    let mut dirs = vec![hb.join("subset/data/fonts")];
    if let Ok(entries) = std::fs::read_dir(hb.join("shape/data")) {
        dirs.extend(entries.flatten().map(|e| e.path().join("fonts")));
    }
    let mut out: Vec<PathBuf> = dirs
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|entries| entries.flatten().map(|e| e.path()))
        .filter(|p| {
            std::fs::read(p)
                .ok()
                .and_then(|b| b.first_chunk::<4>().copied())
                .is_some_and(|h| SFNT_MAGIC.contains(&h))
        })
        .collect();
    out.sort();
    out
}

/// The path of `font` relative to the test data, for item identifiers.
pub fn id(font: &Path, testdata: &Path) -> String {
    font.strip_prefix(testdata)
        .unwrap_or(font)
        .display()
        .to_string()
}

/// HarfBuzz's API test fonts with BASE tables (`test/api/fonts/base*.ttf`),
/// sorted. The shaping and subsetting fonts have few BASE tables, so the
/// baseline comparison adds these (ADR 0017).
pub fn harfbuzz_base_fonts(testdata: &Path) -> Vec<PathBuf> {
    let dir = testdata.join("harfbuzz/test/api/fonts");
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("base") && n.ends_with(".ttf"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}
