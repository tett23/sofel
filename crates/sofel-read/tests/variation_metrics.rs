//! HVAR, VVAR and MVAR views (ADR 0027).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use sofel_read::hvar::{Hvar, Vvar};
use sofel_read::mvar::Mvar;
use sofel_read::sfnt::Sfnt;
use sofel_read::{Diagnostic, Kind, Severity, Tag, hvar, mvar};

fn store(items: u16, data_count: u16) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(1u16.to_be_bytes());
    let regions_at = 8 + u32::from(data_count) * 4;
    b.extend(regions_at.to_be_bytes());
    b.extend(data_count.to_be_bytes());
    for i in 0..data_count {
        b.extend((regions_at + 4 + u32::from(i) * 6).to_be_bytes());
    }
    b.extend(0u16.to_be_bytes());
    b.extend(0u16.to_be_bytes());
    for _ in 0..data_count {
        b.extend(items.to_be_bytes());
        b.extend(0u16.to_be_bytes());
        b.extend(0u16.to_be_bytes());
    }
    b
}

fn map() -> [u8; 5] {
    [0, 2, 0, 1, 7]
}

fn metric_table(vertical: bool) -> Vec<u8> {
    let header = if vertical { 24u32 } else { 20u32 };
    let map_at = header + store(8, 1).len() as u32;
    let mut b = Vec::new();
    b.extend(0x0001_0000u32.to_be_bytes());
    b.extend(header.to_be_bytes());
    b.extend(map_at.to_be_bytes());
    b.extend(0u32.to_be_bytes());
    b.extend(0u32.to_be_bytes());
    if vertical {
        b.extend(map_at.to_be_bytes());
    }
    b.extend(store(8, 1));
    b.extend(map());
    b
}

fn report(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> Vec<Diagnostic> {
    let mut v = Vec::new();
    f(&mut |d| v.push(d));
    v
}

#[test]
fn hvar_and_vvar_offsets_and_maps() {
    let h = metric_table(false);
    let view = Hvar::new(&h).unwrap();
    assert_eq!(view.store().unwrap().axis_count(), 0);
    assert_eq!(view.advance_map().unwrap().unwrap().map(2), 7);
    assert!(view.lsb_map().unwrap().is_none());
    assert!(view.rsb_map().unwrap().is_none());
    assert!(report(|r| hvar::validate(&h, hvar::TAG, None, r)).is_empty());
    let v = metric_table(true);
    let view = Vvar::new(&v).unwrap();
    assert_eq!(view.vorg_map().unwrap().unwrap().map(0), 7);
    assert!(report(|r| hvar::validate(&v, hvar::TAG_VVAR, None, r)).is_empty());
    let mut bad = h;
    bad[8..12].copy_from_slice(&999u32.to_be_bytes());
    assert!(!report(|r| hvar::validate(&bad, hvar::TAG, None, r)).is_empty());
    let mut bad_index = metric_table(false);
    let last = bad_index.len() - 1;
    bad_index[last] = 8;
    let errors = report(|r| hvar::validate(&bad_index, hvar::TAG, None, r));
    assert!(errors.iter().any(|d| matches!(
        d.kind,
        Kind::VariationIndexOutOfRange { outer: 1, inner: 0 }
    ) && d.severity == Severity::Error));
}

#[test]
fn adr0027_metric_maps_allow_sentinel_empty_and_implicit_indices() {
    let mut implicit = metric_table(false);
    implicit[8..12].copy_from_slice(&0u32.to_be_bytes());
    assert!(report(|r| hvar::validate(&implicit, hvar::TAG, None, r)).is_empty());

    let mut empty = metric_table(false);
    let map_at = empty.len() - map().len();
    empty[map_at + 2..map_at + 4].copy_from_slice(&0u16.to_be_bytes());
    assert!(report(|r| hvar::validate(&empty, hvar::TAG, None, r)).is_empty());

    let mut sentinel = metric_table(false);
    let map_at = sentinel.len() - map().len();
    sentinel[map_at + 1] = 0x30;
    sentinel.extend([0xff, 0xff, 0xff]);
    sentinel[map_at + 4..map_at + 8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(report(|r| hvar::validate(&sentinel, hvar::TAG, None, r)).is_empty());
}

fn mvar_table() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(0x0001_0000u32.to_be_bytes());
    b.extend(0u16.to_be_bytes());
    b.extend(8u16.to_be_bytes());
    b.extend(2u16.to_be_bytes());
    b.extend(28u16.to_be_bytes());
    b.extend(b"hasc");
    b.extend(0u16.to_be_bytes());
    b.extend(1u16.to_be_bytes());
    b.extend(b"hlgp");
    b.extend(2u16.to_be_bytes());
    b.extend(3u16.to_be_bytes());
    b.extend(store(4, 3));
    b
}

#[test]
fn mvar_sorted_lookup_and_validator() {
    let b = mvar_table();
    let view = Mvar::new(&b).unwrap();
    assert_eq!(view.record_count(), 2);
    assert_eq!(
        view.find(Tag::new(b"hasc")).map(|r| (r.outer, r.inner)),
        Some((0, 1))
    );
    assert_eq!(
        view.find(Tag::new(b"hlgp")).map(|r| (r.outer, r.inner)),
        Some((2, 3))
    );
    assert!(view.find(Tag::new(b"hdsC")).is_none());
    assert!(view.store().is_ok());
    assert!(report(|r| mvar::validate(&b, None, r)).is_empty());
    let mut unsorted = b;
    unsorted[20..24].copy_from_slice(b"aaaa");
    let bad = report(|r| mvar::validate(&unsorted, None, r));
    assert!(bad.iter().any(
        |d| matches!(d.kind, Kind::TagsNotSorted { index: 1 }) && d.severity == Severity::Error
    ));
}

#[test]
fn adr0027_sfnt_registers_metric_variation_validators() {
    let tables: [(&[u8; 4], &[u8]); 3] = [
        (b"HVAR", &[0, 2, 0, 0]),
        (b"MVAR", &[0, 2, 0, 0]),
        (b"VVAR", &[0, 2, 0, 0]),
    ];
    let mut font = vec![0, 1, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0];
    let mut offset = 12 + 3 * 16;
    for (tag, bytes) in tables {
        font.extend(tag);
        font.extend(sofel_read::validate::checksum(Tag::new(tag), bytes).to_be_bytes());
        font.extend((offset as u32).to_be_bytes());
        font.extend((bytes.len() as u32).to_be_bytes());
        offset += bytes.len();
    }
    for (_, bytes) in tables {
        font.extend(bytes);
    }
    let sfnt = Sfnt::new(&font).unwrap();
    let diagnostics = report(|r| sofel_read::validate::sfnt(&sfnt, r));
    for tag in [b"HVAR", b"MVAR", b"VVAR"] {
        assert!(diagnostics.iter().any(|d| d.table == Tag::new(tag)
            && matches!(d.kind, Kind::UnknownVersion { found: 0x0002_0000 })));
    }
}
