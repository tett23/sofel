//! Variation data readers and their bounds (ADR 0027, ADR 0030 I05).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_read::item_variation::ItemVariationStore;
use sofel_read::{
    Diagnostic, Kind, Severity, Tag, delta_set_index_map::DeltaSetIndexMap, item_variation,
};

fn store(word_count: u16, rows: &[u8], items: u16) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(1u16.to_be_bytes());
    out.extend(12u32.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend(16u32.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend(3u16.to_be_bytes());
    out.extend(items.to_be_bytes());
    out.extend(word_count.to_be_bytes());
    out.extend(3u16.to_be_bytes());
    for i in 0..3u16 {
        out.extend(i.to_be_bytes());
    }
    out.extend(rows);
    out
}

fn diagnostics(bytes: &[u8]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    item_variation::validate(bytes, Tag::new(b"HVAR"), 7, None, &mut |d| out.push(d));
    out
}

#[test]
fn i05_delta_rows_and_boundaries() {
    let mut rows = Vec::new();
    rows.extend((-300i16).to_be_bytes());
    rows.extend(400i16.to_be_bytes());
    rows.push((-5i8).to_be_bytes()[0]);
    rows.extend(1000i16.to_be_bytes());
    rows.extend((-2000i16).to_be_bytes());
    rows.push(7);
    let bytes = store(2, &rows, 2);
    let view = ItemVariationStore::new(&bytes).unwrap();
    let data = view.item_data(0).unwrap();
    assert_eq!((data.item_count, data.word_delta_count), (2, 2));
    assert_eq!(
        [data.delta(0, 0), data.delta(0, 1), data.delta(0, 2)],
        [Some(-300), Some(400), Some(-5)]
    );
    assert_eq!(
        [data.delta(1, 0), data.delta(1, 1), data.delta(1, 2)],
        [Some(1000), Some(-2000), Some(7)]
    );
    assert_eq!(data.delta(2, 0), None);
    assert_eq!(data.delta(1, 3), None);
    assert_eq!(view.delta(0, 1, 2), Some(7));
    assert_eq!(view.delta(1, 0, 0), None);
    assert!(diagnostics(&bytes).is_empty());
    let cut = &bytes[..bytes.len() - 1];
    assert_eq!(
        ItemVariationStore::new(cut)
            .unwrap()
            .item_data(0)
            .unwrap()
            .delta(1, 2),
        None
    );
    assert!(diagnostics(cut).iter().any(|d| d.offset == 35
        && d.severity == Severity::Error
        && matches!(
            d.kind,
            Kind::Truncated {
                needed: 10,
                available: 9
            }
        )));
}

#[test]
fn i05_long_words_and_bad_word_count() {
    let mut rows = Vec::new();
    rows.extend((-100_000i32).to_be_bytes());
    rows.extend(2_000_000i32.to_be_bytes());
    rows.extend((-1234i16).to_be_bytes());
    let bytes = store(0x8002, &rows, 1);
    let data = ItemVariationStore::new(&bytes)
        .unwrap()
        .item_data(0)
        .unwrap();
    assert_eq!(
        [data.delta(0, 0), data.delta(0, 1), data.delta(0, 2)],
        [Some(-100_000), Some(2_000_000), Some(-1234)]
    );
    assert!(diagnostics(&bytes).is_empty());
    let bad = store(0x8004, &rows, 1);
    assert!(diagnostics(&bad).iter().any(|d| d.offset == 25
        && d.severity == Severity::Error
        && matches!(
            d.kind,
            Kind::WordDeltaCountOutOfRange { found: 4, count: 3 }
        )));
}

#[test]
fn delta_set_index_map_formats_and_empty() {
    let format0 = [0, 0x11, 0, 2, 0x12, 0x34, 0xab, 0xcd];
    let map = DeltaSetIndexMap::new(&format0).unwrap();
    assert_eq!(
        (map.format(), map.len(), map.width(), map.inner_bit_count()),
        (0, 2, 2, 2)
    );
    assert_eq!(map.map(0), (0x1234 >> 2) << 16);
    assert_eq!(map.map(9), (0xabcd >> 2) << 16 | (0xabcd & 3));
    let format1 = [1, 0x30, 0, 0, 0, 1, 0, 0, 0x01, 0x23];
    let map = DeltaSetIndexMap::new(&format1).unwrap();
    assert_eq!((map.format(), map.width(), map.len()), (1, 4, 1));
    assert_eq!(map.map(0), (0x123 >> 1) << 16 | 1);
    let empty = DeltaSetIndexMap::new(&[0, 0, 0, 0]).unwrap();
    assert_eq!(empty.map(0x12345), 0x12345);
    assert!(DeltaSetIndexMap::new(&[2, 0, 0, 0]).is_err());
    assert!(DeltaSetIndexMap::new(&format0[..7]).is_err());
    for (format, entry, raw, expected) in [
        (0u8, 0u8, vec![0b101], (2u32 << 16) | 1),
        (0, 0x1f, vec![0x12, 0x34], 0x1234),
        (0, 0x22, vec![0x01, 0x23, 0x45], (0x012345 >> 3) << 16 | 5),
        (1, 0x3f, vec![0x12, 0x34, 0xab, 0xcd], 0x1234_abcd),
    ] {
        let mut bytes = vec![format, entry];
        if format == 0 {
            bytes.extend(1u16.to_be_bytes());
        } else {
            bytes.extend(1u32.to_be_bytes());
        }
        bytes.extend(raw);
        assert_eq!(DeltaSetIndexMap::new(&bytes).unwrap().map(0), expected);
    }
}
