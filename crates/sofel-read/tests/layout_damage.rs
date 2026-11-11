//! Damage events from the original GSUB, GPOS and GDEF validators (ADR 0059).

#![allow(clippy::all, clippy::unwrap_used)]

mod support;
use support::build::*;

use sofel_read::layout::damage::{DamageEvent, inspect_damage};
use sofel_read::sfnt::Sfnt;
use sofel_read::{Diagnostic, Kind, Tag, gdef, gpos, gsub, validate};

fn events(tag: Tag, bytes: &[u8]) -> Vec<DamageEvent> {
    let mut found = Vec::new();
    inspect_damage(tag, bytes, Some(0), &mut |event| found.push(event));
    found
}

fn original(tag: Tag, bytes: &[u8]) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    if tag == gsub::TAG {
        gsub::validate(bytes, Some(0), &mut |d| found.push(d));
    } else if tag == gpos::TAG {
        gpos::validate(bytes, Some(0), &mut |d| found.push(d));
    } else if tag == gdef::TAG {
        gdef::validate(bytes, &mut |d| found.push(d));
    }
    found
}

#[test]
fn inv4_layout_damage_keeps_original_diagnostics_and_siblings() {
    let bytes = Buf::new()
        .u32(0x0001_0000)
        .u16(0x100)
        .u16(0x200)
        .u16(0x300)
        .done();
    for tag in [gsub::TAG, gpos::TAG] {
        let found = events(tag, &bytes);
        let diagnostics: Vec<_> = found
            .iter()
            .filter_map(|event| match event {
                DamageEvent::Diagnostic(d) => Some(*d),
                _ => None,
            })
            .collect();
        assert_eq!(diagnostics, original(tag, &bytes));
        let sources: Vec<_> = found
            .iter()
            .filter_map(|event| match event {
                DamageEvent::Unchecked {
                    source,
                    reason: Kind::OffsetOutOfBounds { .. },
                    ..
                } => Some(*source),
                _ => None,
            })
            .collect();
        assert_eq!(sources, [8, 6, 4]);
    }
}

#[test]
fn inv4_sfnt_damage_keeps_all_original_diagnostics_once() {
    let damaged = Buf::new()
        .u32(0x0001_0000)
        .u16(0x100)
        .u16(0x200)
        .u16(0x300)
        .done();
    let bytes = sfnt(&[(b"GSUB", damaged)]);
    let font = Sfnt::new(&bytes).unwrap();
    let mut expected = Vec::new();
    validate::sfnt(&font, &mut |d| expected.push(d));
    let mut actual = Vec::new();
    let mut extra = Vec::new();
    validate::sfnt_with_layout_damage(&font, &mut |d| actual.push(d), &mut |e| extra.push(e));
    assert_eq!(actual, expected);
    assert_eq!(
        extra
            .iter()
            .filter(|e| matches!(e, DamageEvent::Unchecked { .. }))
            .count(),
        3
    );
    assert!(
        extra
            .iter()
            .all(|e| !matches!(e, DamageEvent::Diagnostic(_)))
    );
}

#[test]
fn inv4_layout_damage_reports_gdef_and_unreadable_header() {
    let bytes = Buf::new()
        .u32(0x0001_0000)
        .u16(0x100)
        .u16(0x200)
        .u16(0)
        .u16(0)
        .done();
    let found = events(gdef::TAG, &bytes);
    assert_eq!(
        found
            .iter()
            .filter(|e| matches!(e, DamageEvent::Diagnostic(_)))
            .count(),
        original(gdef::TAG, &bytes).len()
    );
    assert!(
        found
            .iter()
            .any(|e| matches!(e, DamageEvent::Unchecked { source: 4, .. }))
    );
    assert!(
        found
            .iter()
            .any(|e| matches!(e, DamageEvent::Unchecked { source: 6, .. }))
    );

    let found = events(gsub::TAG, &[0, 1, 0, 0]);
    assert!(found.iter().any(|e| matches!(
        e,
        DamageEvent::Unchecked {
            source: 0,
            reason: Kind::Truncated { .. },
            ..
        }
    )));
}

#[test]
fn inv4_layout_damage_stays_within_report_limit() {
    let n = 1200usize;
    let lig = Buf::new().u16(3).u16(0).done();
    let n16 = u16::try_from(n).unwrap();
    let mut set = Buf::new().u16(n16);
    for _ in 0..n {
        set = set.u16(u16::try_from(2 + 2 * n).unwrap());
    }
    let set = set.bytes(&lig).done();
    let header = 6 + 2 * n;
    let mut sub = Buf::new().u16(1).u16(0).u16(n16);
    for _ in 0..n {
        sub = sub.u16(u16::try_from(header).unwrap());
    }
    let sub = link(
        sub.bytes(&set).done(),
        vec![(2, 2, coverage2(&[(0, n16 - 1, 0)]))],
    );
    let bytes = simple_layout(vec![lookup(4, 0, vec![sub], None)]);
    let found = events(gsub::TAG, &bytes);
    assert!(
        found
            .iter()
            .any(|e| matches!(e, DamageEvent::WorkLimit { table } if *table == gsub::TAG))
    );
    assert_eq!(
        found
            .iter()
            .filter(|e| matches!(e, DamageEvent::WorkLimit { .. }))
            .count(),
        1
    );
}

#[test]
fn inv4_partial_lookup_list_keeps_readable_sibling() {
    let bytes = Buf::new()
        .u32(0x0001_0000)
        .u16(0)
        .u16(0)
        .u16(10)
        .u16(2)
        .u16(0xffff)
        .u8(0)
        .done();
    let found = events(gsub::TAG, &bytes);
    let offsets: Vec<_> = found
        .iter()
        .filter_map(|e| match e {
            DamageEvent::Diagnostic(d) => Some(d.offset),
            _ => None,
        })
        .collect();
    assert!(offsets.contains(&8), "{found:?}");
    assert!(offsets.contains(&12), "{found:?}");
    assert!(found.iter().any(
        |e| matches!(e, DamageEvent::Unchecked { table, source: 14, .. } if *table == gsub::TAG)
    ));
}

#[test]
fn inv4_partial_lookup_list_walks_valid_child_descendants() {
    let mut bytes = vec![0; 29];
    bytes
        .get_mut(0..4)
        .unwrap()
        .copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes
        .get_mut(8..10)
        .unwrap()
        .copy_from_slice(&10u16.to_be_bytes());
    bytes
        .get_mut(10..12)
        .unwrap()
        .copy_from_slice(&20u16.to_be_bytes());
    bytes
        .get_mut(12..14)
        .unwrap()
        .copy_from_slice(&11u16.to_be_bytes());
    bytes
        .get_mut(21..23)
        .unwrap()
        .copy_from_slice(&1u16.to_be_bytes());
    bytes
        .get_mut(25..27)
        .unwrap()
        .copy_from_slice(&1u16.to_be_bytes());
    bytes
        .get_mut(27..29)
        .unwrap()
        .copy_from_slice(&0xffffu16.to_be_bytes());
    let original = original(gsub::TAG, &bytes);
    assert!(original.iter().any(|d| d.offset == 8));
    assert!(!original.iter().any(|d| d.offset == 27));
    let found = events(gsub::TAG, &bytes);
    assert!(
        found
            .iter()
            .any(|e| matches!(e, DamageEvent::Diagnostic(d) if d.offset == 27)),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|e| matches!(e, DamageEvent::Unchecked { source: 27, .. })),
        "{found:?}"
    );
}

#[test]
fn inv4_partial_feature_list_walks_valid_child_descendants() {
    let mut bytes = vec![0; 23];
    bytes
        .get_mut(0..4)
        .unwrap()
        .copy_from_slice(&0x0001_0000u32.to_be_bytes());
    bytes
        .get_mut(6..8)
        .unwrap()
        .copy_from_slice(&10u16.to_be_bytes());
    bytes
        .get_mut(10..12)
        .unwrap()
        .copy_from_slice(&4u16.to_be_bytes());
    bytes.get_mut(12..16).unwrap().copy_from_slice(b"abcd");
    bytes
        .get_mut(16..18)
        .unwrap()
        .copy_from_slice(&9u16.to_be_bytes());
    bytes
        .get_mut(19..21)
        .unwrap()
        .copy_from_slice(&0xffffu16.to_be_bytes());
    assert!(!original(gsub::TAG, &bytes).iter().any(|d| d.offset == 19));
    let found = events(gsub::TAG, &bytes);
    assert!(
        found
            .iter()
            .any(|e| matches!(e, DamageEvent::Diagnostic(d) if d.offset == 19)),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|e| matches!(e, DamageEvent::Unchecked { source: 19, .. })),
        "{found:?}"
    );
}
