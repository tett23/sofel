#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::read::{GlyphId, Tag};
use sofel_font::{Font, FontError};

fn with(
    tables: Vec<(&'static [u8; 4], Vec<u8>)>,
    replace: &[(&'static [u8; 4], Vec<u8>)],
) -> Vec<u8> {
    let mut tables: Vec<_> = tables
        .into_iter()
        .filter(|(t, _)| !replace.iter().any(|(r, _)| r == t))
        .collect();
    tables.extend(replace.iter().cloned());
    // An empty replacement removes the table.
    tables.retain(|(_, d)| !d.is_empty());
    tables.sort_by_key(|(t, _)| **t);
    sfnt(&tables)
}

fn without(tag: &[u8; 4]) -> Vec<u8> {
    let tables: Vec<_> = sample_tables()
        .into_iter()
        .filter(|(t, _)| *t != tag)
        .collect();
    sfnt(&tables)
}

fn g(v: u32) -> Option<GlyphId> {
    Some(GlyphId(v))
}

#[test]
fn opens_fonts_and_collections() {
    let data = sample_font();
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.num_glyphs(), 28);
    assert_eq!(f.units_per_em(), 1000);
    assert!(f.table(Tag::new(b"cmap")).is_some());
    assert!(matches!(
        Font::new(&data, 1),
        Err(FontError::IndexOutOfRange)
    ));

    let mut second = sample_tables();
    for (t, d) in second.iter_mut() {
        if *t == b"maxp" {
            *d = maxp05(9);
        }
    }
    let c = ttc(false, &[sample_tables(), second]);
    assert_eq!(Font::new(&c, 0).unwrap().num_glyphs(), 28);
    assert_eq!(Font::new(&c, 1).unwrap().num_glyphs(), 9);
    assert!(matches!(Font::new(&c, 2), Err(FontError::IndexOutOfRange)));

    assert!(matches!(
        Font::new(&without(b"head"), 0),
        Err(FontError::NoHead)
    ));
    assert!(matches!(
        Font::new(b"not a font at all", 0),
        Err(FontError::Read(_))
    ));
}

#[test]
fn glyph_count_prefers_maxp_wide() {
    assert_eq!(Font::new(&without(b"maxp"), 0).unwrap().num_glyphs(), 0);
    let both = with(sample_tables(), &[(b"MAXP", maxp_wide05(70_000))]);
    assert_eq!(Font::new(&both, 0).unwrap().num_glyphs(), 70_000);
    let only_wide = with(
        sample_tables()
            .into_iter()
            .filter(|(t, _)| *t != b"maxp")
            .collect(),
        &[(b"MAXP", maxp_wide05(5))],
    );
    assert_eq!(Font::new(&only_wide, 0).unwrap().num_glyphs(), 5);
}

/// A cmap whose records (in the given order) each map 'A' to a distinct glyph.
fn cmap_with(records: &[(u16, u16)]) -> Vec<u8> {
    let subs: Vec<(u16, u16, Vec<u8>)> = records
        .iter()
        .enumerate()
        .map(|(i, &(p, e))| {
            (
                p,
                e,
                cmap4(&[
                    seg(0x41, 0x41, (i as i16) + 1 - 0x41),
                    seg(0xFFFF, 0xFFFF, 1),
                ]),
            )
        })
        .collect();
    cmap(&subs)
}

fn chosen(records: &[(u16, u16)]) -> Option<GlyphId> {
    let data = with(sample_tables(), &[(b"cmap", cmap_with(records))]);
    Font::new(&data, 0).unwrap().nominal_glyph(0x41)
}

#[test]
fn subtables_are_chosen_in_harfbuzz_order() {
    let order: [(u16, u16); 10] = [
        (3, 0),
        (3, 10),
        (0, 6),
        (0, 4),
        (3, 1),
        (0, 3),
        (0, 2),
        (0, 1),
        (0, 0),
        (1, 0),
    ];
    // Each entry beats every later one, whichever comes first in the table.
    for (i, &better) in order.iter().enumerate() {
        for &worse in &order[i + 1..] {
            assert_eq!(chosen(&[worse, better]), g(2), "{better:?} over {worse:?}");
            assert_eq!(chosen(&[better, worse]), g(1), "{better:?} over {worse:?}");
        }
    }
    // Any Mac encoding comes last.
    assert_eq!(chosen(&[(1, 7)]), g(1));
    assert_eq!(chosen(&[(1, 7), (0, 0)]), g(2));
    // Unknown platforms are not used.
    assert_eq!(chosen(&[(2, 1)]), None);
}

#[test]
fn unreadable_subtables_are_skipped() {
    let mut broken = cmap6(0x41, &[3]);
    broken[2..4].copy_from_slice(&u16be(0xFFF0)); // length beyond the table
    let table = cmap(&[(3, 1, broken), (3, 10, cmap12_13(12, &[(0x41, 0x41, 4)]))]);
    let data = with(sample_tables(), &[(b"cmap", table)]);
    // (3,10) wins anyway; check that a broken best candidate is skipped too.
    assert_eq!(Font::new(&data, 0).unwrap().nominal_glyph(0x41), g(4));
    let mut broken = cmap12_13(12, &[(0x41, 0x41, 4)]);
    broken[4..8].copy_from_slice(&0xFFFF_FFF0u32.to_be_bytes());
    let table = cmap(&[(3, 1, cmap6(0x41, &[3])), (3, 10, broken)]);
    let data = with(sample_tables(), &[(b"cmap", table)]);
    assert_eq!(Font::new(&data, 0).unwrap().nominal_glyph(0x41), g(3));
}

#[test]
fn nominal_glyphs_stay_below_the_glyph_count() {
    let table = cmap(&[(3, 10, cmap12_13(12, &[(0x41, 0x42, 27)]))]);
    let data = with(sample_tables(), &[(b"cmap", table)]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.nominal_glyph(0x41), g(27));
    assert_eq!(f.nominal_glyph(0x42), None); // glyph 28 == numGlyphs
    assert_eq!(f.nominal_glyph(0x43), None);
}

#[test]
fn variation_sequences() {
    let uvs = cmap14_15(
        false,
        &[Uvs {
            selector: 0xFE00,
            defaults: Some(vec![(0x41, 0)]),
            mappings: Some(vec![(0x42, 5), (0x43, 99)]),
        }],
    );
    let table = cmap(&[
        (0, 5, uvs),
        (
            3,
            1,
            cmap4(&[seg(0x41, 0x43, 1 - 0x41), seg(0xFFFF, 0xFFFF, 1)]),
        ),
    ]);
    let data = with(sample_tables(), &[(b"cmap", table)]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.variation_glyph(0x41, 0xFE00), g(1)); // default: nominal
    assert_eq!(f.variation_glyph(0x42, 0xFE00), g(5));
    assert_eq!(f.variation_glyph(0x43, 0xFE00), None); // 99 >= numGlyphs
    assert_eq!(f.variation_glyph(0x44, 0xFE00), None);
    assert_eq!(f.variation_glyph(0x41, 0xFE01), None);
}

#[test]
fn advances_follow_harfbuzz() {
    let data = sample_font();
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.advance(GlyphId(0)), 500);
    assert_eq!(f.advance(GlyphId(19)), 519);
    assert_eq!(f.advance(GlyphId(27)), 519); // last long metric
    assert_eq!(f.advance(GlyphId(28)), 0); // >= numGlyphs
    // No metrics at all: unitsPerEm / 2.
    assert_eq!(
        Font::new(&without(b"hmtx"), 0).unwrap().advance(GlyphId(3)),
        500
    );
    // hhea missing but hmtx present: no long metrics, but data, so 0.
    assert_eq!(
        Font::new(&without(b"hhea"), 0).unwrap().advance(GlyphId(3)),
        0
    );
    // Out-of-range unitsPerEm counts as 1000 for the default.
    let odd = with(
        sample_tables(),
        &[
            (
                b"head",
                head(HeadSpec {
                    units_per_em: 5,
                    ..Default::default()
                }),
            ),
            (b"hmtx", Vec::new()),
        ],
    );
    let f = Font::new(&odd, 0).unwrap();
    assert_eq!(f.units_per_em(), 5);
    assert_eq!(f.advance(GlyphId(0)), 500);
}

#[test]
fn truncated_metrics_are_used_as_far_as_they_go() {
    // numberOfHMetrics 3 but only two long metrics and one bearing present.
    let table = hmtx(&[(100, 0), (200, 0)], &[0]);
    let data = with(sample_tables(), &[(b"hhea", hhea(3)), (b"hmtx", table)]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.advance(GlyphId(1)), 200);
    assert_eq!(f.advance(GlyphId(2)), 200); // a bearing exists for glyph 2
    assert_eq!(f.advance(GlyphId(3)), 0); // beyond the data
}

#[test]
fn wide_metrics_win() {
    let wide = hmtx(&[(700, 0)], &[0; 27]);
    let data = with(sample_tables(), &[(b"HHEA", hhea_wide(1)), (b"HMTX", wide)]);
    assert_eq!(Font::new(&data, 0).unwrap().advance(GlyphId(5)), 700);
}

#[test]
fn fonts_are_plain_shareable_values() {
    fn check<T: Send + Sync + Copy>() {}
    check::<Font<'static>>();
}

#[test]
fn left_side_bearings_follow_freetype() {
    let lsb = |data: &[u8], glyph| {
        Font::new(data, 0)
            .unwrap()
            .ft_left_side_bearing(GlyphId(glyph))
    };
    let data = sample_font();
    assert_eq!(lsb(&data, 5), 5); // long metric
    assert_eq!(lsb(&data, 25), -25); // leftSideBearings
    assert_eq!(lsb(&data, 28), 0); // past the table
    // Bearings past numGlyphs are still read when the table has them.
    let long: Vec<(u16, i16)> = (0..20).map(|g| (500, g as i16)).collect();
    let extra = with(
        sample_tables(),
        &[(
            b"hmtx",
            hmtx(&long, &[-20, -21, -22, -23, -24, -25, -26, -27, 77]),
        )],
    );
    assert_eq!(lsb(&extra, 28), 77);
    // A long metric that does not fit: 0, even if part of it is there.
    let cut = with(
        sample_tables(),
        &[
            (b"hhea", hhea(3)),
            (b"hmtx", vec![0, 1, 0, 9, 0, 1, 0, 8, 0, 1]),
        ],
    );
    assert_eq!(lsb(&cut, 1), 8);
    assert_eq!(lsb(&cut, 2), 0);
    // Past numberOfHMetrics, the last advance must be readable.
    let cut = with(
        sample_tables(),
        &[
            (b"hhea", hhea(3)),
            (b"hmtx", vec![0, 1, 0, 9, 0, 1, 0, 8, 0]),
        ],
    );
    assert_eq!(lsb(&cut, 3), 0);
    // No hhea, no hmtx, or numberOfHMetrics 0: 0.
    assert_eq!(lsb(&without(b"hhea"), 5), 0);
    assert_eq!(lsb(&without(b"hmtx"), 5), 0);
    assert_eq!(lsb(&with(sample_tables(), &[(b"hhea", hhea(0))]), 5), 0);
}

#[test]
fn left_side_bearing_tables_follow_freetype() {
    let wide = || hmtx(&[(700, 40)], &[41; 27]);
    // hhea and hmtx win over HHEA and HMTX (unlike the advances).
    let both = with(
        sample_tables(),
        &[(b"HHEA", hhea_wide(1)), (b"HMTX", wide())],
    );
    let lsb = |data: &[u8], glyph| {
        Font::new(data, 0)
            .unwrap()
            .ft_left_side_bearing(GlyphId(glyph))
    };
    assert_eq!(lsb(&both, 5), 5);
    // Without hhea, HHEA and HMTX.
    let upper = with(
        sample_tables(),
        &[
            (b"hhea", Vec::new()),
            (b"HHEA", hhea_wide(1)),
            (b"HMTX", wide()),
        ],
    );
    assert_eq!(lsb(&upper, 5), 41);
    // With GLYF, only HHEA and HMTX.
    let glyf_wide = |extra: &[(&'static [u8; 4], Vec<u8>)]| {
        let mut r = vec![(b"GLYF", vec![0u8; 4])];
        r.extend(extra.iter().cloned());
        with(sample_tables(), &r)
    };
    assert_eq!(lsb(&glyf_wide(&[]), 5), 0);
    assert_eq!(
        lsb(&glyf_wide(&[(b"HHEA", hhea_wide(1)), (b"HMTX", wide())]), 0),
        40
    );
    assert_eq!(
        lsb(&glyf_wide(&[(b"HHEA", hhea_wide(1)), (b"HMTX", wide())]), 5),
        41
    );
}
