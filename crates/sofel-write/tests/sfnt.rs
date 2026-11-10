//! The sfnt assembly against hand-computed values (ADR 0022).

#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use sofel_read::sfnt::Sfnt;
use sofel_read::{Diagnostic, Severity, Tag, validate};
use sofel_write::{WriteError, build_sfnt, build_sfnt_ordered, checksum};

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([d[at], d[at + 1]])
}
fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

#[test]
fn search_values() {
    // (count, searchRange, entrySelector, rangeShift)
    for (n, sr, es, rs) in [
        (0u16, 16, 0, 0),
        (1, 16, 0, 0),
        (2, 32, 1, 0),
        (3, 32, 1, 16),
        (4, 64, 2, 0),
        (5, 64, 2, 16),
        (16, 256, 4, 0),
        (17, 256, 4, 16),
    ] {
        let names: Vec<[u8; 4]> = (0..n)
            .map(|i| [b'a', b'b', b'0' + (i / 10) as u8, b'0' + (i % 10) as u8])
            .collect();
        let tables: Vec<(Tag, &[u8])> = names.iter().map(|t| (Tag(*t), &b"x"[..])).collect();
        let out = build_sfnt(&tables).unwrap();
        assert_eq!(u16_at(&out, 4), n);
        assert_eq!(
            (u16_at(&out, 6), u16_at(&out, 8), u16_at(&out, 10)),
            (sr, es, rs),
            "{n}"
        );
    }
}

#[test]
fn layout_padding_and_checksums() {
    let a = [1u8, 2, 3, 4, 5];
    let b = [9u8; 4];
    let c = [7u8; 5];
    let out = build_sfnt(&[
        (Tag::new(b"zzzz"), &a),
        (Tag::new(b"bbbb"), &b),
        (Tag::new(b"aaaa"), &c),
    ])
    .unwrap();
    assert_eq!(u32_at(&out, 0), 0x0001_0000);
    // Records sorted by tag; data by length then tag: bbbb (4), aaaa (5), zzzz (5).
    let dir = 12 + 3 * 16;
    let rec = |i: usize| {
        (
            &out[12 + 16 * i..16 + 16 * i],
            u32_at(&out, 16 + 16 * i),
            u32_at(&out, 20 + 16 * i),
            u32_at(&out, 24 + 16 * i),
        )
    };
    assert_eq!(rec(0).0, b"aaaa");
    assert_eq!(rec(1).0, b"bbbb");
    assert_eq!(rec(2).0, b"zzzz");
    assert_eq!(rec(1).2, dir as u32);
    assert_eq!(rec(0).2, dir as u32 + 4);
    assert_eq!(rec(2).2, dir as u32 + 12);
    assert_eq!(rec(0).3, 5);
    assert_eq!(out.len(), dir + 4 + 8 + 8);
    assert_eq!(&out[dir + 12..dir + 20], &[1, 2, 3, 4, 5, 0, 0, 0]);
    assert_eq!(rec(2).1, 0x0102_0304 + 0x0500_0000);
    assert_eq!(rec(1).1, 0x0909_0909);
}

#[test]
fn cff_version_and_order() {
    let out = build_sfnt(&[
        (Tag::new(b"CFF "), &[1u8; 8][..]),
        (Tag::new(b"aaaa"), &[2u8; 4][..]),
    ])
    .unwrap();
    assert_eq!(&out[..4], b"OTTO");
    let out = build_sfnt_ordered(
        &[
            (Tag::new(b"CFF2"), &[1u8; 8][..]),
            (Tag::new(b"aaaa"), &[2u8; 4][..]),
        ],
        &[Tag::new(b"none"), Tag::new(b"CFF2")],
    )
    .unwrap();
    assert_eq!(&out[..4], b"OTTO");
    // CFF2 first despite being longer.
    assert_eq!(u32_at(&out, 12 + 8), 44);
    assert_eq!(u32_at(&out, 28 + 8), 52);
}

#[test]
fn head_checksum_adjustment() {
    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[8..12].copy_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    let out = build_sfnt(&[
        (Tag::new(b"head"), &head),
        (Tag::new(b"maxp"), &[0, 0, 0x50, 0, 0, 1][..]),
    ])
    .unwrap();
    // The record's checksum is computed with the adjustment as 0.
    let head_at = u32_at(&out, 12 + 8) as usize;
    let mut zeroed = out[head_at..head_at + 56].to_vec();
    zeroed[8..12].fill(0);
    assert_eq!(u32_at(&out, 12 + 4), checksum(&zeroed));
    // The whole file sums to 0xB1B0AFBA.
    assert_eq!(checksum(&out), 0xB1B0_AFBA);
    let font = Sfnt::new(&out).unwrap();
    let mut errors = 0;
    validate::sfnt(&font, &mut |d: Diagnostic| {
        errors += usize::from(d.severity == Severity::Error)
    });
    assert_eq!(errors, 0);
}

#[test]
fn errors() {
    assert_eq!(
        build_sfnt(&[(Tag::new(b"aaaa"), &[][..]), (Tag::new(b"aaaa"), &[][..])]),
        Err(WriteError::DuplicateTable(Tag::new(b"aaaa")))
    );
    let names: Vec<[u8; 4]> = (0..65535u32).map(|i| i.to_be_bytes()).collect();
    let tables: Vec<(Tag, &[u8])> = names.iter().map(|t| (Tag(*t), &[][..])).collect();
    assert_eq!(build_sfnt(&tables), Err(WriteError::TooManyTables));
}
