//! GPOS lookup types 1 to 9 in every format, the 24-bit ones included,
//! every ValueFormat bit combination, and Anchor formats 1 to 3 (ADR
//! 0020).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

mod support;
use support::build::*;

use sofel_read::gpos::{self, Anchor, PosSubtable, ValueFormat, ValueRecord};
use sofel_read::layout::{Lookup, SequenceContext};
use sofel_read::{GlyphId, ReadError};

fn g(v: u32) -> GlyphId {
    GlyphId(v)
}

fn first(ty: u16, sub: Vec<u8>) -> Vec<u8> {
    lookup(ty, 0, vec![sub], None)
}

fn read(lookup_bytes: &[u8]) -> PosSubtable<'_> {
    gpos::subtable(&Lookup::new(lookup_bytes).unwrap(), 0).unwrap()
}

/// The values of a record of `format`: 1, 2, 3, 4 for the adjustments
/// and, for the device bits, the offset `device_at` (from the base) of a
/// Device table.
fn record(format: u16, device_at: u16) -> Buf {
    let mut b = Buf::new();
    for bit in 0..8 {
        if format & (1 << bit) != 0 {
            b = if bit < 4 {
                b.i16(bit as i16 + 1)
            } else {
                b.u16(device_at)
            };
        }
    }
    b
}

fn check_record(v: &ValueRecord<'_>, format: u16) {
    let has = |bit: u16| format & bit != 0;
    assert_eq!(v.format, ValueFormat(format));
    assert_eq!(v.x_placement, if has(1) { 1 } else { 0 });
    assert_eq!(v.y_placement, if has(2) { 2 } else { 0 });
    assert_eq!(v.x_advance, if has(4) { 3 } else { 0 });
    assert_eq!(v.y_advance, if has(8) { 4 } else { 0 });
    for (bit, d) in [
        (0x10, v.x_placement_device),
        (0x20, v.y_placement_device),
        (0x40, v.x_advance_device),
        (0x80, v.y_advance_device),
    ] {
        assert_eq!(d.is_some(), has(bit), "{format:#x} {bit:#x}");
        if let Some(d) = d {
            assert_eq!(d.delta_pixels(12), 1);
        }
    }
}

#[test]
fn every_value_format_bit_combination() {
    // The low eight bits select fields; the high eight are ignored
    // (0x0F00 was for multiple masters, 0xF000 is reserved).
    for low in 0..=0xFFu16 {
        for high in [0, 0x0100, 0xF000] {
            let format = low | high;
            let f = ValueFormat(format);
            assert_eq!(f.len(), low.count_ones() as usize);
            assert_eq!(f.size(), 2 * low.count_ones() as usize);
            assert_eq!(f.has_device(), low & 0xF0 != 0);
            // SinglePos format 1: format, coverage, valueFormat, value,
            // then the Device table and the Coverage table.
            let device_at = (6 + f.size()) as u16;
            let mut b = Buf::new().u16(1).u16(0).u16(format);
            b = b.bytes(&record(format, device_at).done());
            b = b.bytes(&device(12, 12, 3, &[0x0100]));
            let sub = link(b.done(), vec![(2, 2, coverage1(&[1]))]);
            let l = first(1, sub);
            let PosSubtable::Single { coverage, value } = read(&l) else {
                panic!()
            };
            assert_eq!(coverage.get(g(1)), Some(0));
            check_record(&value, format);
        }
    }
}

#[test]
fn single_formats() {
    let f = 0x0045; // x placement, x advance, x advance device
    for format in [1u16, 3] {
        let cw = if format == 1 { 2 } else { 4 };
        let device_at = (4 + cw + 6) as u16;
        let b = zeros(Buf::new().u16(format), cw)
            .u16(f)
            .bytes(&record(f, device_at).done());
        let b = b.bytes(&device(12, 12, 3, &[0x0100]));
        let l = first(1, link(b.done(), vec![(2, cw, coverage1(&[5]))]));
        let PosSubtable::Single { coverage, value } = read(&l) else {
            panic!()
        };
        assert_eq!(coverage.get(g(5)), Some(0));
        check_record(&value, f);
    }
    for format in [2u16, 4] {
        let (cw, nw) = if format == 2 { (2, 2) } else { (4, 3) };
        let mut b = zeros(Buf::new().u16(format), cw).u16(f);
        b = if nw == 3 { b.u24(2) } else { b.u16(2) };
        let device_at = (4 + cw + nw + 12) as u16;
        b = b
            .bytes(&record(f, device_at).done())
            .bytes(&record(f, device_at).done());
        b = b.bytes(&device(12, 12, 3, &[0x0100]));
        let l = first(1, link(b.done(), vec![(2, cw, coverage1(&[5, 6]))]));
        let PosSubtable::SingleArray { values, .. } = read(&l) else {
            panic!()
        };
        assert_eq!(values.len(), 2);
        check_record(&values.get(1).unwrap(), f);
        assert!(values.get(2).is_none());
    }
}

#[test]
fn pair_formats_1_and_3() {
    // Devices in a PairSet are from the start of the PairSet.
    let (f1, f2) = (0x0004u16, 0x0041u16);
    for format in [1u16, 3] {
        let (cw, w) = if format == 1 { (2, 2) } else { (4, 3) };
        let glyph = |b: Buf, v: u32| if w == 3 { b.u24(v) } else { b.u16(v as u16) };
        let size = w + 2 + 4;
        let device_at = (w + 2 * size) as u16;
        let mut set = glyph(Buf::new(), 2);
        for second in [7u32, 0x1_0009] {
            let second = if w == 2 { second & 0xFFFF } else { second };
            set = glyph(set, second);
            set = set.bytes(&record(f1, device_at).done());
            set = set.bytes(&record(f2, device_at).done());
        }
        set = set.bytes(&device(12, 12, 3, &[0x0100]));
        let mut b = zeros(Buf::new().u16(format), cw).u16(f1).u16(f2);
        b = glyph(b, 1);
        b = zeros(b, w);
        let sub = link(
            b.done(),
            vec![(2, cw, coverage1(&[1])), (2 + cw + 4 + w, w, set.done())],
        );
        let l = first(2, sub);
        let PosSubtable::PairGlyphs {
            format1,
            format2,
            pair_sets,
            ..
        } = read(&l)
        else {
            panic!()
        };
        assert_eq!((format1, format2), (ValueFormat(f1), ValueFormat(f2)));
        let set = pair_sets.get(0).unwrap().unwrap();
        assert_eq!(set.len(), 2);
        let (a, b) = set.find(g(7)).unwrap();
        check_record(&a, f1);
        check_record(&b, f2);
        let second = if w == 3 { 0x1_0009 } else { 9 };
        assert!(set.find(g(second)).is_some());
        assert!(set.find(g(8)).is_none());
        assert_eq!(set.record(1).unwrap().0, g(second));
    }
}

#[test]
fn pair_formats_2_and_4() {
    let (f1, f2) = (0x0004u16, 0x0001u16);
    for format in [2u16, 4] {
        let cw = if format == 2 { 2 } else { 4 };
        let mut b = zeros(Buf::new().u16(format), cw).u16(f1).u16(f2);
        b = zeros(b, 2 * cw).u16(2).u16(3);
        for i in 0..6i16 {
            b = b.i16(10 * i).i16(10 * i + 1);
        }
        let sub = link(
            b.done(),
            vec![
                (2, cw, coverage1(&[1, 2])),
                (6 + cw, cw, class_def2(&[(2, 2, 1)])),
                (6 + 2 * cw, cw, class_def2(&[(5, 5, 2)])),
            ],
        );
        let l = first(2, sub);
        let PosSubtable::PairClasses {
            class_def1,
            class_def2,
            class1_count,
            class2_count,
            values,
            ..
        } = read(&l)
        else {
            panic!()
        };
        assert_eq!((class1_count, class2_count), (2, 3));
        let (c1, c2) = (class_def1.get(g(2)), class_def2.get(g(5)));
        assert_eq!((c1, c2), (1, 2));
        let (a, b) = values.get(c1 as u16, c2 as u16).unwrap();
        // Row 1, column 2: pair 5.
        assert_eq!((a.x_advance, b.x_placement), (50, 51));
        assert!(values.get(2, 0).is_none());
        assert!(values.get(0, 3).is_none());
    }
}

fn anchor1(x: i16, y: i16) -> Vec<u8> {
    Buf::new().u16(1).i16(x).i16(y).done()
}

#[test]
fn anchor_formats() {
    let d = anchor1(-5, 7);
    assert!(matches!(
        Anchor::new(&d).unwrap(),
        Anchor::Format1 { x: -5, y: 7 }
    ));
    let d = Buf::new().u16(2).i16(1).i16(2).u16(9).done();
    assert!(matches!(
        Anchor::new(&d).unwrap(),
        Anchor::Format2 {
            x: 1,
            y: 2,
            point: 9
        }
    ));
    let d = link(
        Buf::new().u16(3).i16(1).i16(2).u16(0).u16(0).done(),
        vec![(6, 2, device(12, 12, 3, &[0xFE00])), (8, 2, vec![])],
    );
    let Anchor::Format3 {
        x,
        y,
        x_device,
        y_device,
    } = Anchor::new(&d).unwrap()
    else {
        panic!()
    };
    assert_eq!((x, y), (1, 2));
    assert_eq!(x_device.unwrap().delta_pixels(12), -2);
    assert!(y_device.is_none());
    assert_eq!(
        Anchor::new(&[0, 4, 0, 0, 0, 0]).unwrap_err(),
        ReadError::UnknownFormat(4)
    );
    assert!(matches!(
        Anchor::new(&[0, 2, 0, 0, 0, 0]),
        Err(ReadError::Truncated { .. })
    ));
}

#[test]
fn cursive_formats() {
    for format in [1u16, 2] {
        let (cw, w) = if format == 1 { (2, 2) } else { (4, 3) };
        let mut b = zeros(Buf::new().u16(format), cw);
        b = if w == 3 { b.u24(1) } else { b.u16(1) };
        b = zeros(b, 2 * w);
        let sub = link(
            b.done(),
            vec![
                (2, cw, coverage1(&[3])),
                (2 + cw + w, w, anchor1(10, 20)),
                (2 + cw + 2 * w, w, vec![]),
            ],
        );
        let l = first(3, sub);
        let PosSubtable::Cursive {
            coverage,
            entry_exits,
        } = read(&l)
        else {
            panic!()
        };
        assert_eq!(coverage.get(g(3)), Some(0));
        let (entry, exit) = entry_exits.get(0).unwrap();
        assert!(matches!(
            entry.unwrap(),
            Some(Anchor::Format1 { x: 10, y: 20 })
        ));
        assert!(exit.unwrap().is_none());
        assert!(entry_exits.get(1).is_none());
    }
}

/// A MarkArray of `(class, anchor)` with counts and offsets of `w` bytes.
fn mark_array(w: usize, marks: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut b = if w == 3 {
        Buf::new().u24(marks.len() as u32)
    } else {
        Buf::new().u16(marks.len() as u16)
    };
    for (c, _) in marks {
        b = zeros(b.u16(*c), w);
    }
    let links = marks
        .iter()
        .enumerate()
        .map(|(i, (_, a))| (w + (2 + w) * i + 2, w, a.clone()))
        .collect();
    link(b.done(), links)
}

/// An AnchorMatrix with a row count of `row_w` bytes and offsets of `w`.
fn anchor_matrix(row_w: usize, w: usize, rows: Vec<Vec<Vec<u8>>>) -> Vec<u8> {
    let cols = rows.first().map_or(0, |r| r.len());
    let mut b = if row_w == 3 {
        Buf::new().u24(rows.len() as u32)
    } else {
        Buf::new().u16(rows.len() as u16)
    };
    b = zeros(b, w * cols * rows.len());
    let links = rows
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, a)| (row_w + w * i, w, a))
        .collect();
    link(b.done(), links)
}

fn mark_subtable(format: u16, second: Vec<u8>) -> Vec<u8> {
    let (cw, w) = if format == 1 { (2, 2) } else { (4, 3) };
    let b = zeros(zeros(Buf::new().u16(format), 2 * cw).u16(2), 2 * cw);
    link(
        b.done(),
        vec![
            (2, cw, coverage1(&[10])),
            (2 + cw, cw, coverage1(&[1])),
            (4 + 2 * cw, cw, mark_array(w, &[(1, anchor1(5, 6))])),
            (4 + 3 * cw, cw, second),
        ],
    )
}

#[test]
fn mark_attachment_formats() {
    for format in [1u16, 2] {
        let w = if format == 1 { 2 } else { 3 };
        // Mark-to-base: the row count is 24-bit in format 2.
        let bases = anchor_matrix(w, w, vec![vec![vec![], anchor1(100, 200)]]);
        let l = first(4, mark_subtable(format, bases));
        let PosSubtable::MarkBase {
            mark_coverage,
            base_coverage,
            class_count,
            marks,
            bases,
        } = read(&l)
        else {
            panic!()
        };
        assert_eq!(
            (mark_coverage.get(g(10)), base_coverage.get(g(1))),
            (Some(0), Some(0))
        );
        assert_eq!(class_count, 2);
        let (class, anchor) = marks.get(0).unwrap();
        assert_eq!(class, 1);
        assert!(matches!(
            anchor.unwrap(),
            Some(Anchor::Format1 { x: 5, y: 6 })
        ));
        assert_eq!(bases.rows(), 1);
        assert!(bases.get(0, 0).unwrap().is_none());
        assert!(matches!(
            bases.get(0, 1).unwrap(),
            Some(Anchor::Format1 { x: 100, .. })
        ));
        assert!(bases.get(0, 2).unwrap().is_none());
        assert!(bases.get(1, 0).unwrap().is_none());

        // Mark-to-mark: the row count stays 16-bit.
        let mark2s = anchor_matrix(2, w, vec![vec![anchor1(1, 1), anchor1(2, 2)]]);
        let l = first(6, mark_subtable(format, mark2s));
        let PosSubtable::MarkMark { mark2s, .. } = read(&l) else {
            panic!()
        };
        assert!(matches!(
            mark2s.get(0, 1).unwrap(),
            Some(Anchor::Format1 { x: 2, .. })
        ));

        // Mark-to-ligature: a LigatureArray of matrices, a row per
        // component.
        let attach = anchor_matrix(
            2,
            w,
            vec![vec![anchor1(7, 0), vec![]], vec![vec![], anchor1(8, 0)]],
        );
        let mut la = if w == 3 {
            Buf::new().u24(1)
        } else {
            Buf::new().u16(1)
        };
        la = zeros(la, w);
        let ligs = link(la.done(), vec![(w, w, attach)]);
        let l = first(5, mark_subtable(format, ligs));
        let PosSubtable::MarkLig { ligatures, .. } = read(&l) else {
            panic!()
        };
        assert_eq!(ligatures.len(), 1);
        let m = ligatures.get(0).unwrap().unwrap();
        assert_eq!(m.rows(), 2);
        assert!(matches!(
            m.get(1, 1).unwrap(),
            Some(Anchor::Format1 { x: 8, .. })
        ));
    }
    // A MarkArray that cannot be read is HarfBuzz's empty one.
    let mut l = first(4, mark_subtable(1, anchor_matrix(2, 2, vec![])));
    // Point markArrayOffset past the subtable.
    let sub = 8;
    l[sub + 8..sub + 10].copy_from_slice(&0x7000u16.to_be_bytes());
    let PosSubtable::MarkBase { marks, .. } = read(&l) else {
        panic!()
    };
    assert!(marks.is_empty());
}

#[test]
fn context_and_extension() {
    let b = zeros(Buf::new().u16(3).u16(1).u16(0), 2);
    let ctx = link(b.done(), vec![(6, 2, coverage1(&[1]))]);
    let l = first(7, ctx.clone());
    assert!(matches!(
        read(&l),
        PosSubtable::Context(SequenceContext::Coverages { .. })
    ));
    let l = lookup(9, 0, vec![extension(7, ctx)], None);
    let lk = Lookup::new(&l).unwrap();
    assert!(matches!(
        gpos::subtable(&lk, 0).unwrap(),
        PosSubtable::Context(SequenceContext::Coverages { .. })
    ));
    assert_eq!(gpos::effective_type(&lk), 7);
    let nested = lookup(9, 0, vec![extension(9, extension(1, vec![0, 1]))], None);
    assert_eq!(
        gpos::subtable(&Lookup::new(&nested).unwrap(), 0).unwrap_err(),
        ReadError::InvalidValue
    );
    let chain = Buf::new().u16(3).u16(0).u16(1).u16(0).u16(0).u16(0).done();
    let chain = link(chain, vec![(6, 2, coverage1(&[1]))]);
    assert!(matches!(
        read(&first(8, chain)),
        PosSubtable::ChainContext(_)
    ));
}
