//! The charset index (ADR 0025): `CharsetIndex` gives the same SIDs and
//! glyphs as `Charset` for every charset form.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

mod support;

use sofel_read::cff::Charset;
use support::build::Buf;

/// A table with three padding bytes, then the charset at offset 3.
fn table(format: u8, body: &[u8]) -> Vec<u8> {
    Buf::new().bytes(&[0, 0, 0]).u8(format).bytes(body).done()
}

fn format1(ranges: &[(u16, u8)]) -> Vec<u8> {
    let body: Vec<u8> = ranges
        .iter()
        .flat_map(|&(first, left)| {
            let [a, b] = first.to_be_bytes();
            [a, b, left]
        })
        .collect();
    table(1, &body)
}

fn format2(ranges: &[(u16, u16)]) -> Vec<u8> {
    let body: Vec<u8> = ranges
        .iter()
        .flat_map(|&(first, left)| {
            let [a, b] = first.to_be_bytes();
            let [c, d] = left.to_be_bytes();
            [a, b, c, d]
        })
        .collect();
    table(2, &body)
}

/// Every glyph's SID through the index equals `Charset::sid`, and every
/// SID's first glyph through the index equals `Charset::glyph`.
fn check(c: &Charset<'_>) {
    let mut starts = vec![0u32; c.num_ranges()];
    let index = c.index_into(&mut starts).unwrap();
    let n = c.num_glyphs();
    for g in 0..n.saturating_add(3) {
        assert_eq!(index.sid(g), c.sid(g), "glyph {g}");
    }
    // Charset::glyph walks every glyph, so large charsets are sampled.
    let step = (n as usize / 1000).max(1);
    let mut sids: Vec<u16> = c.sids().step_by(step).collect();
    sids.extend([0, 1, 2, 0xFFFE, 0xFFFF, 300, 1000]);
    for s in sids {
        assert_eq!(index.glyph(s), c.glyph(s), "sid {s}");
    }
}

#[test]
fn predefined_charsets() {
    for (offset, n) in [(0u64, 229u32), (1, 166), (2, 87), (0, 1), (1, 10)] {
        let c = Charset::new(&[], offset, n).unwrap();
        assert_eq!(c.num_ranges(), 0);
        check(&c);
    }
}

#[test]
fn format0() {
    let t = table(0, &[0, 5, 1, 0, 0, 5, 0, 7]);
    let c = Charset::new(&t, 3, 5).unwrap();
    assert_eq!(c.num_ranges(), 0);
    check(&c);
}

#[test]
fn format1_ranges() {
    let t = format1(&[(10, 2), (20, 0), (5, 255), (12, 3)]);
    let c = Charset::new(&t, 3, 1 + 3 + 1 + 256 + 4).unwrap();
    assert_eq!(c.num_ranges(), 4);
    check(&c);
}

#[test]
fn format2_ranges() {
    let t = format2(&[(100, 1000), (5000, 0), (7, 4000)]);
    let c = Charset::new(&t, 3, 1 + 1001 + 1 + 4001).unwrap();
    assert_eq!(c.num_ranges(), 3);
    check(&c);
}

#[test]
fn range_cut_at_0xffff() {
    // FreeType's rescue: the first range would pass 0xFFFF, so it is cut
    // to SIDs 0xFFFE and 0xFFFF and the next range starts at glyph 3.
    let t = format2(&[(0xFFFE, 5), (1, 1)]);
    let c = Charset::new(&t, 3, 5).unwrap();
    assert_eq!(c.num_ranges(), 2);
    check(&c);
    let t = format1(&[(0xFFFF, 200), (3, 0)]);
    let c = Charset::new(&t, 3, 3).unwrap();
    assert_eq!(c.num_ranges(), 2);
    check(&c);
}

#[test]
fn single_range() {
    let t = format1(&[(1, 9)]);
    let c = Charset::new(&t, 3, 11).unwrap();
    assert_eq!(c.num_ranges(), 1);
    check(&c);
}

#[test]
fn glyphs_end_inside_the_last_range() {
    // The ranges hold 1 + 3 + 6 glyphs; the font has 6.
    let t = format1(&[(10, 2), (40, 5)]);
    let c = Charset::new(&t, 3, 6).unwrap();
    assert_eq!(c.num_ranges(), 2);
    check(&c);
    // SIDs past the last glyph are not found.
    let mut starts = [0u32; 2];
    let index = c.index_into(&mut starts).unwrap();
    assert_eq!(index.glyph(42), None);
    assert_eq!(c.glyph(42), None);
    assert_eq!(index.glyph(41), Some(5));
}

#[test]
fn ranges_after_the_glyphs_are_not_counted() {
    let t = format1(&[(10, 2), (40, 5), (90, 0)]);
    let c = Charset::new(&t, 3, 4).unwrap();
    assert_eq!(c.num_ranges(), 1);
    check(&c);
}

#[test]
fn repeated_sids_give_the_first_glyph() {
    let t = format1(&[(10, 3), (8, 4), (10, 0)]);
    let c = Charset::new(&t, 3, 11).unwrap();
    check(&c);
    let mut starts = [0u32; 3];
    let index = c.index_into(&mut starts).unwrap();
    assert_eq!(index.glyph(10), Some(1));
    assert_eq!(index.glyph(8), Some(5));
    assert_eq!(index.glyph(12), Some(3));
}

#[test]
fn glyph_zero_and_no_glyphs() {
    let t = format1(&[(0, 3)]);
    let c = Charset::new(&t, 3, 5).unwrap();
    check(&c);
    let c = Charset::new(&t, 3, 0).unwrap();
    assert_eq!(c.num_ranges(), 0);
    check(&c);
    let c = Charset::new(&t, 3, 1).unwrap();
    assert_eq!(c.num_ranges(), 0);
    check(&c);
}

#[test]
fn starts_are_the_first_glyphs_of_the_ranges() {
    let t = format2(&[(100, 1000), (5000, 0), (7, 40000)]);
    let c = Charset::new(&t, 3, 2000).unwrap();
    let mut starts = [u32::MAX; 4];
    c.index_into(&mut starts).unwrap();
    assert_eq!(starts, [1, 1002, 1003, u32::MAX]);
}

#[test]
fn array_length() {
    let t = format1(&[(10, 2), (20, 0), (30, 4)]);
    let c = Charset::new(&t, 3, 10).unwrap();
    assert_eq!(c.num_ranges(), 3);
    // Too short: nothing written, the needed length returned.
    let mut short = [7u32; 2];
    assert_eq!(c.index_into(&mut short).err(), Some(3));
    assert_eq!(short, [7, 7]);
    let mut empty: [u32; 0] = [];
    assert_eq!(c.index_into(&mut empty).err(), Some(3));
    // Exactly long enough, and longer.
    let mut exact = [0u32; 3];
    assert_eq!(c.index_into(&mut exact).unwrap().sid(9), Some(34));
    let mut long = [0u32; 8];
    assert_eq!(c.index_into(&mut long).unwrap().sid(9), Some(34));
    // Charsets without ranges take an empty array.
    let c = Charset::new(&[], 0, 10).unwrap();
    assert_eq!(c.index_into(&mut empty).unwrap().sid(9), Some(9));
}

#[test]
fn index_is_copy_send_sync() {
    fn assert<T: Copy + Send + Sync>() {}
    assert::<sofel_read::cff::CharsetIndex<'static>>();
}

/// A small deterministic generator for the exhaustive comparison.
fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 33
}

#[test]
fn random_charsets_agree() {
    let mut s = 0x5eed_u64;
    for round in 0u32..120 {
        let wide = round.is_multiple_of(2);
        let ranges = 1 + next(&mut s) % 20;
        let mut total = 1u64;
        let t = if wide {
            let r: Vec<(u16, u16)> = (0..ranges)
                .map(|_| {
                    let first = (next(&mut s) % 0x10000) as u16;
                    let left = (next(&mut s) % 300) as u16;
                    total += u64::from(left.min(0xFFFF - first)) + 1;
                    (first, left)
                })
                .collect();
            format2(&r)
        } else {
            let r: Vec<(u16, u8)> = (0..ranges)
                .map(|_| {
                    let first = if next(&mut s).is_multiple_of(4) {
                        0xFFFF - (next(&mut s) % 300) as u16
                    } else {
                        (next(&mut s) % 2000) as u16
                    };
                    let left = (next(&mut s) % 256) as u8;
                    total += u64::from(u16::from(left).min(0xFFFF - first)) + 1;
                    (first, left)
                })
                .collect();
            format1(&r)
        };
        let n = 1 + next(&mut s) % total;
        let c = Charset::new(&t, 3, n as u32).unwrap();
        check(&c);
    }
}
