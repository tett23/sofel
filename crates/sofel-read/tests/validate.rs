#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

mod support;
use support::build::*;

use sofel_read::sfnt::Sfnt;
use sofel_read::{CountWidth, DetailValue, Diagnostic, Severity, Tag};
use sofel_read::{cmap, head, hhea, hmtx, maxp, validate};

fn collect(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    f(&mut |d| out.push(d));
    out
}

fn ids(ds: &[Diagnostic]) -> Vec<(&'static str, Severity, u32)> {
    ds.iter()
        .map(|d| (d.kind.id(), d.severity, d.offset))
        .collect()
}

#[test]
fn valid_font_reports_nothing() {
    let font = sample_font();
    let s = Sfnt::new(&font).unwrap();
    let ds = collect(|r| validate::sfnt(&s, r));
    assert!(ds.is_empty(), "{ds:?}");
}

#[test]
fn head_diagnostics() {
    let data = head(HeadSpec {
        magic: 1,
        units_per_em: 5,
        index_to_loc_format: 2,
        ..Default::default()
    });
    let ds = collect(|r| head::validate(&data, r));
    assert_eq!(
        ids(&ds),
        vec![
            ("bad_magic_number", Severity::Warning, 12),
            ("units_per_em_out_of_range", Severity::Warning, 18),
            ("invalid_index_to_loc_format", Severity::Error, 50),
        ]
    );
    assert!(ds.iter().all(|d| d.table == Tag::new(b"head")));
    let ds = collect(|r| head::validate(&data[..10], r));
    assert_eq!(ids(&ds), vec![("truncated", Severity::Error, 0)]);
}

#[test]
fn details_and_messages() {
    let data = head(HeadSpec {
        magic: 1,
        ..Default::default()
    });
    let ds = collect(|r| head::validate(&data, r));
    let mut details = Vec::new();
    ds[0].kind.details(&mut |k, v| details.push((k, v)));
    assert_eq!(
        details,
        vec![
            ("expected", DetailValue::Uint(0x5F0F_3CF5)),
            ("found", DetailValue::Uint(1))
        ]
    );
    assert!(!format!("{}", ds[0].kind).is_empty());
}

#[test]
fn maxp_and_hhea_diagnostics() {
    let bad = Buf::new().u32(0x0002_0000).u16(3).done();
    let ds = collect(|r| maxp::validate(&bad, CountWidth::U16, r));
    assert_eq!(ids(&ds), vec![("unknown_version", Severity::Error, 0)]);
    let ds = collect(|r| maxp::validate(&maxp_wide05(70_000), CountWidth::U24, r));
    assert!(ds.is_empty());
    assert_eq!(
        collect(|r| maxp::validate(&maxp05(1), CountWidth::U24, r))[0].table,
        Tag::new(b"MAXP")
    );
    let ds = collect(|r| hhea::validate(&hhea(3)[..30], CountWidth::U16, r));
    assert_eq!(ids(&ds), vec![("truncated", Severity::Error, 0)]);
}

#[test]
fn hmtx_diagnostics() {
    let data = hmtx(&[(500, 1)], &[2]);
    let ds = collect(|r| hmtx::validate(&data, 0, 2, r));
    assert_eq!(ids(&ds), vec![("zero_long_metrics", Severity::Error, 0)]);
    let ds = collect(|r| hmtx::validate(&data, 1, 3, r));
    assert_eq!(ids(&ds), vec![("truncated", Severity::Error, 6)]);
    let data = hmtx(&[(500, 1), (500, 1)], &[]);
    let ds = collect(|r| hmtx::validate(&data, 2, 1, r));
    assert_eq!(
        ids(&ds),
        vec![("metrics_exceed_glyphs", Severity::Warning, 0)]
    );
}

fn cmap_ids(sub: Vec<u8>, num_glyphs: Option<u32>) -> Vec<(&'static str, Severity, u32)> {
    let data = cmap(&[(3, 1, sub)]);
    ids(&collect(|r| cmap::validate(&data, num_glyphs, r)))
}

const SUB: u32 = 12; // the only subtable starts after the header and one record

#[test]
fn cmap4_diagnostics() {
    // start > end in segment 0
    let ds = cmap_ids(cmap4(&[seg(0x45, 0x41, 0), seg(0xFFFF, 0xFFFF, 1)]), None);
    assert_eq!(ds, vec![("start_after_end", Severity::Error, SUB + 16 + 4)]);
    // segments out of order
    let ds = cmap_ids(
        cmap4(&[
            seg(0x50, 0x51, 0),
            seg(0x41, 0x42, 0),
            seg(0xFFFF, 0xFFFF, 1),
        ]),
        None,
    );
    assert_eq!(
        ds,
        vec![("segments_not_sorted", Severity::Error, SUB + 16 + 6 + 2)]
    );
    // sorted but overlapping
    let ds = cmap_ids(
        cmap4(&[
            seg(0x41, 0x50, 0),
            seg(0x45, 0x60, 0),
            seg(0xFFFF, 0xFFFF, 1),
        ]),
        None,
    );
    assert_eq!(
        ds,
        vec![("segments_overlap", Severity::Warning, SUB + 16 + 6 + 2)]
    );
    // last segment does not end at 0xFFFF
    let ds = cmap_ids(cmap4(&[seg(0x41, 0x42, 0)]), None);
    assert_eq!(
        ds,
        vec![("last_segment_not_ffff", Severity::Warning, SUB + 14)]
    );
    // glyph ids beyond numGlyphs (tight)
    let ds = cmap_ids(
        cmap4(&[seg(0x41, 0x42, 100), seg(0xFFFF, 0xFFFF, 1)]),
        Some(50),
    );
    assert_eq!(
        ds,
        vec![(
            "glyph_id_out_of_range",
            Severity::Warning,
            SUB + 16 + 2 * 2 * 2
        )]
    );
}

#[test]
fn cmap4_range_offset_outside() {
    let mut sub = cmap4(&[seg_ids(0x41, 0x42, 0, vec![1, 2]), seg(0xFFFF, 0xFFFF, 1)]);
    let ro0 = 16 + 6 * 2;
    sub[ro0..ro0 + 2].copy_from_slice(&u16be(0x7000));
    let ds = cmap_ids(sub, None);
    assert_eq!(
        ds,
        vec![(
            "range_offset_out_of_bounds",
            Severity::Error,
            SUB + ro0 as u32
        )]
    );
}

#[test]
fn cmap4_search_parameters_and_odd_count() {
    let mut sub = cmap4(&[seg(0x41, 0x42, 0), seg(0xFFFF, 0xFFFF, 1)]);
    sub[8..10].copy_from_slice(&u16be(6)); // searchRange
    assert_eq!(
        cmap_ids(sub, None),
        vec![("search_parameters_invalid", Severity::Warning, SUB + 8)]
    );
}

#[test]
fn cmap_group_diagnostics() {
    let ds = cmap_ids(cmap12_13(12, &[(0x50, 0x60, 1), (0x41, 0x42, 1)]), None);
    assert_eq!(
        ds,
        vec![("groups_not_sorted", Severity::Error, SUB + 16 + 12)]
    );
    let ds = cmap_ids(cmap12_13(13, &[(0x50, 0x40, 1)]), None);
    assert_eq!(ds, vec![("start_after_end", Severity::Error, SUB + 16)]);
    let ds = cmap_ids(cmap12_13(12, &[(0x41, 0x42, 9)]), Some(10));
    assert_eq!(
        ds,
        vec![("glyph_id_out_of_range", Severity::Warning, SUB + 16)]
    );
}

#[test]
fn cmap14_diagnostics() {
    let recs = vec![
        Uvs {
            selector: 0xFE01,
            defaults: None,
            mappings: Some(vec![(0x41, 1)]),
        },
        Uvs {
            selector: 0xFE00,
            defaults: None,
            mappings: Some(vec![(0x41, 1)]),
        },
    ];
    let ds = cmap_ids(cmap14_15(false, &recs), None);
    assert_eq!(
        ds,
        vec![("selectors_not_sorted", Severity::Error, SUB + 10 + 11)]
    );
    let recs = vec![Uvs {
        selector: 0xFE00,
        defaults: Some(vec![(0x10_FFFF, 1)]),
        mappings: None,
    }];
    let ds = cmap_ids(cmap14_15(false, &recs), None);
    assert_eq!(
        ds,
        vec![("codepoint_out_of_range", Severity::Error, SUB + 21 + 4)]
    );
}

#[test]
fn cmap_table_level_diagnostics() {
    let ds = cmap_ids(Buf::new().u16(3).u16(4).done(), None);
    assert_eq!(ds, vec![("unknown_format", Severity::Error, SUB)]);
    let mut data = cmap(&[(3, 1, cmap6(0, &[1]))]);
    data[8..12].copy_from_slice(&0x0000_FFFFu32.to_be_bytes());
    let ds = ids(&collect(|r| cmap::validate(&data, None, r)));
    assert_eq!(ds, vec![("offset_out_of_bounds", Severity::Error, 8)]);
    let data = cmap(&[(3, 1, cmap6(0, &[1]))]);
    let ds = ids(&collect(|r| cmap::validate(&data[..10], None, r)));
    assert_eq!(ds, vec![("truncated", Severity::Error, 4)]);
}

#[test]
fn directory_diagnostics() {
    // unsorted records
    let font = sfnt(&[(b"maxp", maxp05(1)), (b"head", head(HeadSpec::default()))]);
    let s = Sfnt::new(&font).unwrap();
    let ds = collect(|r| validate::directory(&s, r));
    assert_eq!(
        ids(&ds),
        vec![("records_not_sorted", Severity::Warning, 12 + 16)]
    );
    assert_eq!(ds[0].table, Tag::new(b"sfnt"));

    // checksum mismatch and out-of-bounds table
    let mut font = sfnt(&[(b"head", head(HeadSpec::default())), (b"maxp", maxp05(1))]);
    font[12 + 4] ^= 0xFF;
    font[12 + 16 + 12..12 + 16 + 16].copy_from_slice(&0x0010_0000u32.to_be_bytes());
    let s = Sfnt::new(&font).unwrap();
    let ds = collect(|r| validate::directory(&s, r));
    assert_eq!(
        ids(&ds),
        vec![
            ("checksum_mismatch", Severity::Warning, 12),
            ("offset_out_of_bounds", Severity::Error, 12 + 16),
        ]
    );
}

#[test]
fn whole_font_validation_uses_context() {
    // numGlyphs 28, but hhea says 30 long metrics.
    let mut tables = sample_tables();
    for (t, d) in tables.iter_mut() {
        if *t == b"hhea" {
            *d = hhea(30);
        }
    }
    let font = sfnt(&tables);
    let s = Sfnt::new(&font).unwrap();
    let ds = collect(|r| validate::sfnt(&s, r));
    let got: Vec<(Tag, &str)> = ds.iter().map(|d| (d.table, d.kind.id())).collect();
    assert!(
        got.contains(&(Tag::new(b"hmtx"), "metrics_exceed_glyphs")),
        "{got:?}"
    );
}

#[test]
fn single_subtables() {
    let data = cmap(&[
        (3, 1, cmap4(&[seg(0x41, 0x42, 0), seg(0xFFFF, 0xFFFF, 1)])),
        (3, 10, cmap12_13(12, &[(0x50, 0x40, 1)])),
    ]);
    let c = sofel_read::cmap::Cmap::new(&data).unwrap();
    let offsets: Vec<u32> = c.records().map(|r| r.offset).collect();
    let ok = collect(|r| cmap::validate_subtable(&data, offsets[0], None, r));
    assert!(ok.is_empty(), "{ok:?}");
    let bad = collect(|r| cmap::validate_subtable(&data, offsets[1], None, r));
    assert_eq!(
        ids(&bad),
        vec![("start_after_end", Severity::Error, offsets[1] + 16)]
    );
    let out = collect(|r| cmap::validate_subtable(&data, 0, None, r));
    assert_eq!(
        ids(&out),
        vec![("offset_out_of_bounds", Severity::Error, 0)]
    );
}
