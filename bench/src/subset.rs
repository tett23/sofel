//! The subsetting items of ADR 0022 (計測): a font subset to glyph ids
//! with `RETAIN_GIDS` (Golem's usage) and to characters with the default
//! flags, by `sofel_subset::subset` and by `hb_subset_or_fail` with
//! `hb_face_reference_blob`. The glyphs and characters are those of the
//! hb-dump comparisons: the characters of `ja-runs.txt` and
//! `en-paragraph.txt`, and for the glyphs, what the cmap maps them to,
//! glyph 1 and the last glyph. Each body makes its input, subsets and
//! hashes the output (FNV-1a), so that the outputs are compared byte for
//! byte before timing.
//!
//!
//! The items with preprocessing (stage 11): the same two subsets through
//! a preprocessed font (`Prepared` and `Plan::with_prepared`; HarfBuzz's
//! `hb_subset_or_fail` on the face `hb_subset_preprocess` returns), made
//! once before timing; and the preprocessing itself (`Prepared::new`,
//! `hb_subset_preprocess`), whose results are not compared (HarfBuzz's is
//! a subset face, sofel's the accelerator's data).

use std::collections::BTreeSet;
use std::path::Path;

use sofel_font::Font;
use sofel_subset::{Flags, Input, Plan, Prepared, SetKind, subset};

use crate::ffi::{HbFont, HbPreprocessed};
use crate::{Bench, Compare};

/// Roboto (glyf), Source Han Sans (CID-keyed CFF) and Noto Nastaliq Urdu
/// (large GSUB and GPOS).
const FONTS: [(&str, &str); 3] = [
    (
        "Roboto-Regular",
        "harfbuzz/test/subset/data/fonts/Roboto-Regular.ttf",
    ),
    (
        "SourceHanSans-Regular_subset",
        "harfbuzz/test/subset/data/fonts/SourceHanSans-Regular_subset.otf",
    ),
    (
        "NotoNastaliqUrdu-Bold",
        "harfbuzz/test/subset/data/fonts/NotoNastaliqUrdu-Bold.ttf",
    ),
];

const TEXTS: [&str; 2] = [
    "harfbuzz/perf/texts/ja-runs.txt",
    "harfbuzz/perf/texts/en-paragraph.txt",
];

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| {
        (h ^ u64::from(x)).wrapping_mul(0x100_0000_01b3)
    })
}

/// sofel's subset of the glyphs or characters with the flags, as the
/// shim's: the hash, 0 on failure.
fn ours(font: &Font<'_>, glyphs: &[u32], unicodes: &[u32], flags: Flags) -> u64 {
    subset(font, &input(glyphs, unicodes, flags)).map_or(0, |b| fnv(&b))
}

fn input(glyphs: &[u32], unicodes: &[u32], flags: Flags) -> Input {
    Input::new()
        .map_set(SetKind::Glyphs, |s| glyphs.iter().for_each(|&g| s.add(g)))
        .map_set(SetKind::Unicodes, |s| {
            unicodes.iter().for_each(|&c| s.add(c))
        })
        .with_flags(flags)
}

/// [`ours`] through a prepared value.
fn ours_prepared(
    font: &Font<'_>,
    prepared: &Prepared,
    glyphs: &[u32],
    unicodes: &[u32],
    flags: Flags,
) -> u64 {
    Plan::with_prepared(font, prepared, &input(glyphs, unicodes, flags))
        .and_then(|p| p.execute(font))
        .map_or(0, |b| fnv(&b))
}

pub fn subset_items(b: &mut Bench, testdata: &Path) {
    let chars: BTreeSet<u32> = TEXTS
        .iter()
        .flat_map(|t| {
            std::fs::read_to_string(testdata.join(t))
                .unwrap_or_else(|e| panic!("{t}: {e}; run scripts/fetch-testdata.sh"))
                .chars()
                .map(u32::from)
                .collect::<Vec<_>>()
        })
        .collect();
    let unicodes: Vec<u32> = chars.iter().copied().collect();
    for (name, path) in FONTS {
        let bytes = std::fs::read(testdata.join(path))
            .unwrap_or_else(|e| panic!("{path}: {e}; run scripts/fetch-testdata.sh"));
        let font = Font::new(&bytes, 0).expect("sofel opens the font");
        let hb = HbFont::open(&bytes).expect("HarfBuzz opens the font");
        let mut glyphs: BTreeSet<u32> = chars
            .iter()
            .filter_map(|&c| font.nominal_glyph(c).map(|g| g.0))
            .collect();
        glyphs.insert(1);
        glyphs.insert(font.num_glyphs().saturating_sub(1));
        let glyphs: Vec<u32> = glyphs.into_iter().collect();
        b.item(
            name,
            format!("subset ({} glyphs, retain-gids)", glyphs.len()),
            &mut || ours(&font, &glyphs, &[], Flags::RETAIN_GIDS),
            None,
            Some((
                &mut || hb.subset(&glyphs, &[], Flags::RETAIN_GIDS.0).0,
                Compare::Yes,
            )),
        );
        b.item(
            name,
            format!("subset ({} characters)", unicodes.len()),
            &mut || ours(&font, &[], &unicodes, Flags::DEFAULT),
            None,
            Some((
                &mut || hb.subset(&[], &unicodes, Flags::DEFAULT.0).0,
                Compare::Yes,
            )),
        );
        let prepared = Prepared::new(&font);
        let preprocessed = HbPreprocessed::new(&hb);
        b.item(
            name,
            format!("subset prepared ({} glyphs, retain-gids)", glyphs.len()),
            &mut || ours_prepared(&font, &prepared, &glyphs, &[], Flags::RETAIN_GIDS),
            None,
            Some((
                &mut || preprocessed.subset(&glyphs, &[], Flags::RETAIN_GIDS.0).0,
                Compare::Yes,
            )),
        );
        b.item(
            name,
            format!("subset prepared ({} characters)", unicodes.len()),
            &mut || ours_prepared(&font, &prepared, &[], &unicodes, Flags::DEFAULT),
            None,
            Some((
                &mut || preprocessed.subset(&[], &unicodes, Flags::DEFAULT.0).0,
                Compare::Yes,
            )),
        );
        b.item(
            name,
            "preprocess".to_string(),
            &mut || {
                let p = Prepared::new(&font);
                core::hint::black_box(&p);
                u64::from(font.num_glyphs())
            },
            None,
            Some((
                &mut || u64::from(HbPreprocessed::new(&hb).glyph_count()),
                Compare::No,
            )),
        );
    }
}
