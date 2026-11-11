//! The `--shape` pairs of the HarfBuzz comparison (ADR 0020, テスト設計
//! 決定性) hash to the same value natively and on wasm32: CI runs this
//! test in both places against the committed hash, and the conformance
//! job compares the same pairs with HarfBuzz natively. Skipped without
//! the test data unless `SOFEL_REQUIRE_TESTDATA` is set; `SOFEL_TESTDATA`
//! points to it (default: the repository's `target/testdata`). Debug
//! builds skip it too unless `SOFEL_REQUIRE_TESTDATA` is set, since it
//! shapes half a million lines.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::path::PathBuf;

use sofel_font::Font;
use sofel_shape::{Direction, Feature, Scale, SegmentProperties, ShapePlan, UnicodeBuffer, shape};

/// The pairs of `harfbuzz_shape_dump.rs`: font, text, directions,
/// features.
const PAIRS: &[(&str, &str, &[&str], &[&str])] = &[
    (
        "perf/fonts/Roboto-Regular.ttf",
        "perf/texts/en-thelittleprince.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/Roboto-Regular.ttf",
        "perf/texts/en-words.txt",
        &["auto"],
        &[""],
    ),
    (
        "test/subset/data/fonts/SourceSerifVariable-Roman.ttf",
        "perf/texts/react-dom.txt",
        &["auto"],
        &[""],
    ),
    (
        "test/subset/data/fonts/NotoSansDevanagari-Regular.ttf",
        "perf/texts/hi-words.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/Amiri-Regular.ttf",
        "perf/texts/fa-thelittleprince.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/NotoNastaliqUrdu-Regular.ttf",
        "perf/texts/fa-thelittleprince.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/NotoNastaliqUrdu-Regular.ttf",
        "perf/texts/fa-words.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/Gulzar-Regular.ttf",
        "perf/texts/fa-thelittleprince.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/Gulzar-Regular.ttf",
        "perf/texts/fa-words.txt",
        &["auto"],
        &[""],
    ),
    (
        "perf/fonts/NotoSansDuployan-Regular.otf",
        "perf/texts/duployan.txt",
        &["auto"],
        &[""],
    ),
    (
        "test/subset/data/fonts/Mplus1p-Regular.ttf",
        "perf/texts/ja-runs.txt",
        &["ltr", "ttb"],
        &["", "vrt2", "-vert", "palt", "vkrn"],
    ),
];

/// FNV-1a over the glyph, cluster, flags and position of every glyph of
/// every line of every pair.
const EXPECTED: u64 = 0x5868_1607_f09f_2ea0;

fn fnv(h: &mut u64, v: i64) {
    for b in v.to_le_bytes() {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x100_0000_01b3);
    }
}

#[test]
fn shape_hash() {
    let required = std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_some();
    if cfg!(debug_assertions) && !required {
        eprintln!("a debug build; skipped");
        return;
    }
    let dir = std::env::var_os("SOFEL_TESTDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata"))
        .join("harfbuzz");
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &(font, text, dirs, feats) in PAIRS {
        let (Ok(bytes), Ok(text)) = (std::fs::read(dir.join(font)), std::fs::read(dir.join(text)))
        else {
            if required {
                panic!("{font}, {text}: missing");
            }
            eprintln!("{font}: no test data; skipped");
            return;
        };
        let font = Font::new(&bytes, 0).unwrap();
        let scale = Scale::unscaled(&font);
        for &d in dirs {
            for &f in feats {
                let features: Vec<Feature> = f
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| Feature::parse(s.as_bytes()).unwrap())
                    .collect();
                let mut plans: Vec<ShapePlan<'_>> = Vec::new();
                let mut buffer = UnicodeBuffer::new();
                let mut lines: Vec<&[u8]> = text.split(|&c| c == b'\n').collect();
                if text.last() == Some(&b'\n') {
                    lines.pop();
                }
                for line in lines {
                    buffer.push_utf8(line, 0, None);
                    let props = SegmentProperties {
                        direction: Direction::from_bytes(d.as_bytes()).filter(|_| d != "auto"),
                        ..SegmentProperties::default()
                    }
                    .guess(&buffer, None);
                    let i = match plans.iter().position(|p| p.key().0 == props) {
                        Some(i) => i,
                        None => {
                            plans.push(ShapePlan::new(font.default_instance(), props, &features));
                            plans.len() - 1
                        }
                    };
                    let glyphs = shape(font.default_instance(), &scale, &plans[i], buffer);
                    for (g, p) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
                        for v in [g.glyph, g.cluster, g.flags.0] {
                            fnv(&mut h, i64::from(v));
                        }
                        for v in [p.x_advance, p.y_advance, p.x_offset, p.y_offset] {
                            fnv(&mut h, i64::from(v));
                        }
                    }
                    fnv(&mut h, -1);
                    buffer = glyphs.into_unicode();
                }
            }
        }
    }
    assert_eq!(h, EXPECTED, "hash {h:#018x}");
}
