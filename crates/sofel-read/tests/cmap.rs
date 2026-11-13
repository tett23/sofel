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

use sofel_read::cmap::{Cmap, Subtable, VariantGlyph};
use sofel_read::{GlyphId, ReadError};

fn only(sub: Vec<u8>) -> Vec<u8> {
    cmap(&[(3, 1, sub)])
}

fn subtable(data: &[u8]) -> Subtable<'_> {
    let c = Cmap::new(data).unwrap();
    let r = c.records().next().unwrap();
    r.subtable().unwrap()
}

fn g(v: u32) -> Option<GlyphId> {
    Some(GlyphId(v))
}

/// Every pair from `mappings()` agrees with `glyph()`, codepoints are
/// strictly ascending, and no pair has glyph 0.
fn check_mappings(s: &Subtable<'_>) -> Vec<(u32, u32)> {
    let pairs: Vec<(u32, u32)> = s.mappings().map(|(c, gid)| (c, gid.0)).collect();
    for w in pairs.windows(2) {
        assert!(w[0].0 < w[1].0, "not ascending: {w:?}");
    }
    for (c, gid) in &pairs {
        assert_ne!(*gid, 0);
        assert_eq!(s.glyph(*c), g(*gid), "codepoint {c:#x}");
    }
    for limit in pairs
        .iter()
        .take(4)
        .flat_map(|(c, _)| [c.saturating_sub(1), *c])
    {
        let bounded: Vec<_> = s
            .mappings_through(limit)
            .map(|(c, gid)| (c, gid.0))
            .collect();
        assert_eq!(
            bounded,
            pairs
                .iter()
                .copied()
                .take_while(|(c, _)| *c <= limit)
                .collect::<Vec<_>>()
        );
    }
    pairs
}

#[test]
fn records() {
    let data = sample_font_cmap();
    let c = Cmap::new(&data).unwrap();
    assert_eq!(c.version(), 0);
    let recs: Vec<(u16, u16)> = c
        .records()
        .map(|r| (r.platform_id, r.encoding_id))
        .collect();
    assert_eq!(recs, vec![(3, 1), (3, 10)]);
    assert_eq!(c.find(3, 10).unwrap().unwrap().format(), 12);
    assert!(c.find(1, 0).is_none());
}

fn sample_font_cmap() -> Vec<u8> {
    sample_tables()
        .into_iter()
        .find(|(t, _)| *t == b"cmap")
        .unwrap()
        .1
}

#[test]
fn truncated_record_arrays_keep_whole_records() {
    let data = sample_font_cmap();
    let c = Cmap::new(&data[..4 + 8 + 7]).unwrap();
    assert_eq!(c.records().count(), 1);
    assert!(matches!(
        Cmap::new(&data[..3]),
        Err(ReadError::Truncated { .. })
    ));
}

#[test]
fn format0() {
    let mut ids = [0u8; 256];
    ids[0x41] = 5;
    ids[0xFF] = 9;
    let data = only(cmap0(&ids));
    let s = subtable(&data);
    assert_eq!(s.format(), 0);
    assert_eq!(s.glyph(0x41), g(5));
    assert_eq!(s.glyph(0xFF), g(9));
    assert_eq!(s.glyph(0x42), None);
    assert_eq!(s.glyph(0x100), None);
    assert_eq!(check_mappings(&s), vec![(0x41, 5), (0xFF, 9)]);
}

#[test]
fn format2() {
    let mut keys = [0u16; 256];
    keys[0x81] = 1;
    // sub header 0: single bytes 0x20..=0x22 -> 3, 4, 0(unmapped)
    // sub header 1: 0x81 0x40..=0x41 -> (10 + 5), (11 + 5)
    let data = only(cmap2(
        &keys,
        &[(0x20, 0, vec![3, 4, 0]), (0x40, 5, vec![10, 11])],
    ));
    let s = subtable(&data);
    assert_eq!(s.format(), 2);
    assert_eq!(s.glyph(0x20), g(3));
    assert_eq!(s.glyph(0x21), g(4));
    assert_eq!(s.glyph(0x22), None);
    assert_eq!(s.glyph(0x1F), None);
    assert_eq!(s.glyph(0x8140), g(15));
    assert_eq!(s.glyph(0x8141), g(16));
    assert_eq!(s.glyph(0x8142), None);
    // 0x81 is a lead byte, not a single-byte code.
    assert_eq!(s.glyph(0x81), None);
    // 0x20 has subHeader 0, so it is not a lead byte.
    assert_eq!(s.glyph(0x2040), None);
    assert_eq!(s.glyph(0x1_0000), None);
    assert_eq!(
        check_mappings(&s),
        vec![(0x20, 3), (0x21, 4), (0x8140, 15), (0x8141, 16)]
    );
}

#[test]
fn format4_delta_and_range_offset() {
    let data = only(cmap4(&[
        seg(0x41, 0x43, 10 - 0x41),
        seg_ids(0x61, 0x63, 0, vec![20, 0, 22]),
        seg_ids(0x100, 0x100, 5, vec![7]),
        seg(0xFFFF, 0xFFFF, 1),
    ]));
    let s = subtable(&data);
    assert_eq!(s.format(), 4);
    assert_eq!(s.glyph(0x41), g(10));
    assert_eq!(s.glyph(0x43), g(12));
    assert_eq!(s.glyph(0x44), None);
    assert_eq!(s.glyph(0x61), g(20));
    assert_eq!(s.glyph(0x62), None);
    assert_eq!(s.glyph(0x63), g(22));
    assert_eq!(s.glyph(0x100), g(12));
    assert_eq!(s.glyph(0xFFFF), None);
    assert_eq!(s.glyph(0x1_0000), None);
    assert_eq!(
        check_mappings(&s),
        vec![
            (0x41, 10),
            (0x42, 11),
            (0x43, 12),
            (0x61, 20),
            (0x63, 22),
            (0x100, 12)
        ]
    );
}

#[test]
fn format4_delta_wraps_modulo_65536() {
    let data = only(cmap4(&[seg(0xFFF0, 0xFFF2, 0x20), seg(0xFFFF, 0xFFFF, 1)]));
    let s = subtable(&data);
    assert_eq!(s.glyph(0xFFF0), g(0x10));
    assert_eq!(s.glyph(0xFFF2), g(0x12));
}

#[test]
fn format4_ignores_declared_length() {
    // FreeType reads to the end of the cmap table, whatever `length` says.
    let mut sub = cmap4(&[seg(0x41, 0x41, 1), seg(0xFFFF, 0xFFFF, 1)]);
    sub[2..4].copy_from_slice(&u16be(4));
    let data = only(sub);
    assert_eq!(subtable(&data).glyph(0x41), g(0x42));
}

#[test]
fn format4_last_segment_pointing_outside() {
    // The final 0xFFFF segment's idRangeOffset points past the table.
    let mut sub = cmap4(&[seg(0x41, 0x41, 1), seg(0xFFFF, 0xFFFF, 1)]);
    let n = 2;
    let ro_last = 16 + 6 * n + 2; // idRangeOffset[1]
    sub[ro_last..ro_last + 2].copy_from_slice(&u16be(0x4000));
    let data = only(sub);
    let s = subtable(&data);
    assert_eq!(s.glyph(0xFFFF), None);
    assert_eq!(s.glyph(0x41), g(0x42));
}

#[test]
fn format6() {
    let data = only(cmap6(0x30, &[5, 0, 7]));
    let s = subtable(&data);
    assert_eq!(s.format(), 6);
    assert_eq!(s.glyph(0x30), g(5));
    assert_eq!(s.glyph(0x31), None);
    assert_eq!(s.glyph(0x32), g(7));
    assert_eq!(s.glyph(0x33), None);
    assert_eq!(s.glyph(0x2F), None);
    assert_eq!(check_mappings(&s), vec![(0x30, 5), (0x32, 7)]);
}

#[test]
fn format8() {
    let mut is32 = [0u8; 8192];
    // 0xD800 is the high half of 32-bit codes.
    is32[0xD800 >> 3] |= 0x80 >> (0xD800 & 7);
    let data = only(cmap8(
        &is32,
        &[(0x41, 0x42, 3), (0xD800_DC00, 0xD800_DC01, 9)],
    ));
    let s = subtable(&data);
    assert_eq!(s.format(), 8);
    assert_eq!(s.glyph(0x41), g(3));
    assert_eq!(s.glyph(0x42), g(4));
    assert_eq!(s.glyph(0xD800_DC01), g(10));
    assert_eq!(s.glyph(0x43), None);
    assert_eq!(
        check_mappings(&s),
        vec![(0x41, 3), (0x42, 4), (0xD800_DC00, 9), (0xD800_DC01, 10)]
    );
}

#[test]
fn format10() {
    let data = only(cmap10(0x1_0000, &[4, 0, 6]));
    let s = subtable(&data);
    assert_eq!(s.format(), 10);
    assert_eq!(s.glyph(0x1_0000), g(4));
    assert_eq!(s.glyph(0x1_0001), None);
    assert_eq!(s.glyph(0x1_0002), g(6));
    assert_eq!(s.glyph(0x1_0003), None);
    assert_eq!(s.glyph(0xFFFF), None);
    assert_eq!(check_mappings(&s), vec![(0x1_0000, 4), (0x1_0002, 6)]);
}

#[test]
fn format12() {
    let data = only(cmap12_13(
        12,
        &[(0x41, 0x43, 1), (0x1F600, 0x1F601, 0x1_0000)],
    ));
    let s = subtable(&data);
    assert_eq!(s.format(), 12);
    assert_eq!(s.glyph(0x40), None);
    assert_eq!(s.glyph(0x43), g(3));
    assert_eq!(s.glyph(0x44), None);
    assert_eq!(s.glyph(0x1F601), g(0x1_0001));
    assert_eq!(s.glyph(0x10_FFFF), None);
    assert_eq!(
        check_mappings(&s),
        vec![
            (0x41, 1),
            (0x42, 2),
            (0x43, 3),
            (0x1F600, 0x1_0000),
            (0x1F601, 0x1_0001)
        ]
    );
}

#[test]
fn format12_start_glyph_zero_skips_first() {
    let data = only(cmap12_13(12, &[(0x41, 0x42, 0)]));
    let s = subtable(&data);
    assert_eq!(s.glyph(0x41), None);
    assert_eq!(s.glyph(0x42), g(1));
    assert_eq!(check_mappings(&s), vec![(0x42, 1)]);
}

#[test]
fn format13() {
    let data = only(cmap12_13(13, &[(0x41, 0x43, 7), (0x50, 0x50, 0)]));
    let s = subtable(&data);
    assert_eq!(s.format(), 13);
    assert_eq!(s.glyph(0x41), g(7));
    assert_eq!(s.glyph(0x43), g(7));
    assert_eq!(s.glyph(0x50), None);
    assert_eq!(check_mappings(&s), vec![(0x41, 7), (0x42, 7), (0x43, 7)]);
}

fn uvs_records() -> Vec<Uvs> {
    vec![
        Uvs {
            selector: 0xFE00,
            defaults: Some(vec![(0x4E00, 2)]),
            mappings: Some(vec![(0x5000, 40)]),
        },
        Uvs {
            selector: 0xE0100,
            defaults: None,
            mappings: Some(vec![(0x4E00, 0x1_2345)]),
        },
    ]
}

#[test]
fn format14() {
    let mut recs = uvs_records();
    recs[1].mappings = Some(vec![(0x4E00, 41)]);
    let data = only(cmap14_15(false, &recs));
    let s = subtable(&data);
    assert_eq!(s.format(), 14);
    assert_eq!(s.glyph(0x4E00), None);
    assert_eq!(s.variant(0x4E00, 0xFE00), Some(VariantGlyph::Default));
    assert_eq!(s.variant(0x4E02, 0xFE00), Some(VariantGlyph::Default));
    assert_eq!(s.variant(0x4E03, 0xFE00), None);
    assert_eq!(
        s.variant(0x5000, 0xFE00),
        Some(VariantGlyph::Glyph(GlyphId(40)))
    );
    assert_eq!(
        s.variant(0x4E00, 0xE0100),
        Some(VariantGlyph::Glyph(GlyphId(41)))
    );
    assert_eq!(s.variant(0x4E00, 0xFE01), None);
    assert_eq!(s.mappings().count(), 0);
}

#[test]
fn format15_has_24_bit_glyphs() {
    let data = only(cmap14_15(true, &uvs_records()));
    let s = subtable(&data);
    assert_eq!(s.format(), 15);
    assert_eq!(
        s.variant(0x4E00, 0xE0100),
        Some(VariantGlyph::Glyph(GlyphId(0x1_2345)))
    );
    assert_eq!(s.variant(0x4E01, 0xFE00), Some(VariantGlyph::Default));
}

#[test]
fn unknown_and_truncated_subtables() {
    let data = only(Buf::new().u16(3).u16(4).done());
    let c = Cmap::new(&data).unwrap();
    assert!(matches!(
        c.records().next().unwrap().subtable(),
        Err(ReadError::UnknownFormat(3))
    ));

    let mut sub = cmap12_13(12, &[(0x41, 0x43, 1)]);
    sub.truncate(sub.len() - 1);
    let data = only(sub);
    let c = Cmap::new(&data).unwrap();
    assert!(matches!(
        c.records().next().unwrap().subtable(),
        Err(ReadError::Truncated { .. })
    ));

    let mut data = only(cmap6(0, &[1]));
    data[8..12].copy_from_slice(&0x0000_FFFFu32.to_be_bytes());
    let c = Cmap::new(&data).unwrap();
    assert!(c.records().next().unwrap().subtable().is_err());
}

#[test]
fn language() {
    let data = only(cmap6(0, &[1]));
    assert_eq!(subtable(&data).language(), 0);
}
