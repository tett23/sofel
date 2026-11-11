//! Borrowed `gvar`/`GVAR` views and validators (ADR 0027).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_read::gvar::{self, Gvar, PackedDeltas, PackedPoints};
use sofel_read::{Kind, Tag};

fn header(tag: Tag, glyphs: u32, long: bool) -> Vec<u8> {
    let large = tag == gvar::GVAR_TAG;
    let mut data = vec![0, 1, 0, 0, 0, 1, 0, 1];
    let entries = usize::try_from(glyphs).unwrap() + 1;
    let offsets_at = if large { 21 } else { 20 };
    let tuples_at = offsets_at + entries * if long { 4 } else { 2 };
    data.extend_from_slice(&u32::try_from(tuples_at).unwrap().to_be_bytes());
    if large {
        data.extend_from_slice(&[
            u8::try_from(glyphs >> 16).unwrap(),
            u8::try_from((glyphs >> 8) & 0xff).unwrap(),
            u8::try_from(glyphs & 0xff).unwrap(),
        ]);
    } else {
        data.extend_from_slice(&u16::try_from(glyphs).unwrap().to_be_bytes());
    }
    data.extend_from_slice(&(if long { 1u16 } else { 0 }).to_be_bytes());
    data.extend_from_slice(&u32::try_from(tuples_at + 2).unwrap().to_be_bytes());
    data.resize(tuples_at, 0);
    data.extend_from_slice(&0x4000i16.to_be_bytes());
    data
}

#[test]
fn i01_gvar_offsets_and_shared_tuple() {
    let mut data = header(gvar::TAG, 2, false);
    data[24..26].copy_from_slice(&9u16.to_be_bytes());
    data.extend_from_slice(&[
        0, 1, 0, 9, // one tuple; serialized data starts after its header
        0, 8, 0x20, 0, // private points, shared peak tuple 0
        0, // byte between the headers and serialized data
        2, 1, 1, 2, // points 1 and 3
        1, 10, 251,  // x: +10, -5
        0x81, // y: two zeros
        0,    // short-offset padding
    ]);
    let table = Gvar::new(&data, gvar::TAG).unwrap();
    assert_eq!(table.axis_count(), 1);
    assert_eq!(table.glyph_count(), 2);
    assert_eq!(table.shared_tuple(0).unwrap().axis(0), Some(0x4000));
    assert!(table.glyph_data(0).is_none());
    let glyph = table.glyph_data(1).unwrap();
    assert_eq!(glyph.tuple_count(), 1);
    let tuple = glyph.tuple(0).unwrap();
    assert_eq!(tuple.shared_peak_index(), Some(0));
    assert!(tuple.private_points());
    let (points, used) = PackedPoints::parse(tuple.data()).unwrap();
    assert_eq!(points.iter().collect::<Vec<_>>(), vec![1, 3]);
    let (x, x_used) = PackedDeltas::parse(&tuple.data()[used..], 2).unwrap();
    assert_eq!(x.iter().collect::<Vec<_>>(), vec![10, -5]);
    let (y, _) = PackedDeltas::parse(&tuple.data()[used + x_used..], 2).unwrap();
    assert_eq!(y.iter().collect::<Vec<_>>(), vec![0, 0]);
    let (x, y) = tuple.deltas(4, glyph.shared_points()).unwrap();
    assert_eq!(x.iter().collect::<Vec<_>>(), vec![10, -5]);
    assert_eq!(y.iter().collect::<Vec<_>>(), vec![0, 0]);
    assert!(tuple.deltas(3, glyph.shared_points()).is_none());
    let mut diagnostics = Vec::new();
    gvar::validate_with_points(
        &data,
        gvar::TAG,
        Some(1),
        Some(2),
        |glyph| (glyph == 1).then_some(4),
        &mut |d| diagnostics.push(d),
    );
    assert!(diagnostics.is_empty());
    gvar::validate_with_points(
        &data,
        gvar::TAG,
        Some(1),
        Some(2),
        |glyph| (glyph == 1).then_some(3),
        &mut |d| diagnostics.push(d),
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| matches!(d.kind, Kind::PointDeltaCountMismatch { glyph: 1, tuple: 0 }))
    );
}

#[test]
fn i02_gvar_packed_point_and_delta_boundaries() {
    let (all, used) = PackedPoints::parse(&[0]).unwrap();
    assert!(all.is_all());
    assert_eq!(used, 1);
    let (points, used) = PackedPoints::parse(&[0x80, 0x02, 0x80, 0x01, 0x00, 0x00, 0x03]).unwrap();
    assert_eq!(used, 7);
    assert_eq!(points.iter().collect::<Vec<_>>(), vec![256, 259]);
    assert!(PackedPoints::parse(&[2, 1, 1]).is_none());
    assert!(PackedPoints::parse(&[1, 1, 1, 2]).is_none());
    let (deltas, used) = PackedDeltas::parse(&[0, 0xfe, 0x40, 0x01, 0x2c, 0x80], 3).unwrap();
    assert_eq!(used, 6);
    assert_eq!(deltas.iter().collect::<Vec<_>>(), vec![-2, 300, 0]);
    assert!(PackedDeltas::parse(&[0x40, 0x01], 1).is_none());
}

#[test]
fn i03_gvar_gvar24_and_validation() {
    let data = header(gvar::GVAR_TAG, 0x10000, true);
    let table = Gvar::new(&data, gvar::GVAR_TAG).unwrap();
    assert_eq!(table.glyph_count(), 0x10000);
    assert!(table.glyph_data(0x10000).is_none());
    let mut diagnostics = Vec::new();
    gvar::validate(&data, gvar::GVAR_TAG, Some(2), Some(0x10000), &mut |d| {
        diagnostics.push(d)
    });
    assert!(
        diagnostics
            .iter()
            .any(|d| matches!(d.kind, Kind::AxisCountMismatch { .. }))
    );
    diagnostics.clear();
    gvar::validate(&data, gvar::GVAR_TAG, Some(1), Some(3), &mut |d| {
        diagnostics.push(d)
    });
    assert!(
        diagnostics
            .iter()
            .any(|d| matches!(d.kind, Kind::GlyphCountMismatch { .. }))
    );
}

#[test]
fn i04_gvar_embedded_intermediate_and_shared_points() {
    let mut data = header(gvar::TAG, 1, false);
    data[22..24].copy_from_slice(&11u16.to_be_bytes());
    data.extend_from_slice(&[
        0x80, 1, 0, 14, // one tuple with shared points, data starts at byte 14
        0, 4, 0xc0, 0, // four data bytes, embedded peak and intermediate region
        0x40, 0, 0, 0, 0x40, 0, // peak, start, end
        1, 0, 0, // one shared point: number zero
        0, 5, 0, 7, // x and y deltas
        0, // short-offset padding
    ]);
    let table = Gvar::new(&data, gvar::TAG).unwrap();
    let glyph = table.glyph_data(0).unwrap();
    let tuple = glyph.tuple(0).unwrap();
    assert_eq!(tuple.embedded_peak().unwrap().axis(0), Some(0x4000));
    let (start, end) = tuple.intermediate().unwrap();
    assert_eq!(start.axis(0), Some(0));
    assert_eq!(end.axis(0), Some(0x4000));
    let shared = glyph.shared_points().unwrap();
    assert_eq!(shared.iter().collect::<Vec<_>>(), vec![0]);
    let (x, y) = tuple.deltas(4, Some(shared)).unwrap();
    assert_eq!(x.iter().collect::<Vec<_>>(), vec![5]);
    assert_eq!(y.iter().collect::<Vec<_>>(), vec![7]);
}

#[test]
fn adr0047_i01_i02_tuples_keep_order_and_data_boundaries() {
    let glyph_bytes = [
        0, 2, 0, 12, // two tuples, serialized data starts after the headers
        0, 2, 0, 0, // first tuple uses shared peak zero
        0, 2, 0, 0, // second tuple uses shared peak zero
        0xaa, 0xbb, 0xcc, 0xdd,
    ];
    let mut data = header(gvar::TAG, 1, false);
    data[22..24].copy_from_slice(&8u16.to_be_bytes());
    data.extend_from_slice(&glyph_bytes);
    let glyph = Gvar::new(&data, gvar::TAG).unwrap().glyph_data(0).unwrap();
    let tuples = glyph.tuples().collect::<Option<Vec<_>>>().unwrap();
    assert_eq!(tuples.len(), 2);
    for (index, tuple) in tuples.iter().enumerate() {
        let indexed = glyph.tuple(u16::try_from(index).unwrap()).unwrap();
        assert_eq!(tuple.shared_peak_index(), indexed.shared_peak_index());
        assert_eq!(tuple.data(), indexed.data());
    }
    assert_eq!(tuples[0].data(), &[0xaa, 0xbb]);
    assert_eq!(tuples[1].data(), &[0xcc, 0xdd]);

    let mut one = header(gvar::TAG, 1, false);
    one[22..24].copy_from_slice(&5u16.to_be_bytes());
    one.extend_from_slice(&[0, 1, 0, 8, 0, 2, 0, 0, 0xaa, 0xbb]);
    let glyph = Gvar::new(&one, gvar::TAG).unwrap().glyph_data(0).unwrap();
    assert_eq!(
        glyph.tuples().next().unwrap().unwrap().data(),
        &[0xaa, 0xbb]
    );
    assert_eq!(glyph.tuples().count(), 1);

    let mut empty = header(gvar::TAG, 1, false);
    empty[22..24].copy_from_slice(&2u16.to_be_bytes());
    empty.extend_from_slice(&[0, 0, 0, 4]);
    let glyph = Gvar::new(&empty, gvar::TAG).unwrap().glyph_data(0).unwrap();
    assert_eq!(glyph.tuples().count(), 0);
}

#[test]
fn adr0047_i04_broken_tuple_bounds_still_reject_glyph_data() {
    let mut data = header(gvar::TAG, 1, false);
    data[22..24].copy_from_slice(&8u16.to_be_bytes());
    data.extend_from_slice(&[0, 2, 0, 12, 0, 2, 0, 0, 0, 3, 0, 0, 0xaa, 0xbb, 0xcc, 0xdd]);
    assert!(Gvar::new(&data, gvar::TAG).unwrap().glyph_data(0).is_none());
    data[34..36].copy_from_slice(&2u16.to_be_bytes());
    data[26..28].copy_from_slice(&3u16.to_be_bytes());
    assert!(Gvar::new(&data, gvar::TAG).unwrap().glyph_data(0).is_none());

    let mut short_header = header(gvar::TAG, 1, false);
    short_header[22..24].copy_from_slice(&4u16.to_be_bytes());
    short_header.extend_from_slice(&[0, 2, 0, 8, 0, 0, 0, 0]);
    assert!(
        Gvar::new(&short_header, gvar::TAG)
            .unwrap()
            .glyph_data(0)
            .is_none()
    );
}
