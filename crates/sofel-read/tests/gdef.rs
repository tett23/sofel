//! GDEF versions 1.0, 1.2, 1.3 and 1.4 and HarfBuzz's blocklist (ADR
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

use sofel_read::gdef::{self, CaretValue, Gdef, glyph_class};
use sofel_read::{GlyphId, ReadError};

fn g(v: u32) -> GlyphId {
    GlyphId(v)
}

fn attach_list() -> Vec<u8> {
    let points = Buf::new().u16(2).u16(3).u16(9).done();
    link(
        Buf::new().u16(0).u16(1).u16(0).done(),
        vec![(0, 2, coverage1(&[7])), (4, 2, points)],
    )
}

fn lig_caret_list(wide: bool) -> Vec<u8> {
    let carets = link(
        Buf::new().u16(3).u16(0).u16(0).u16(0).done(),
        vec![
            (2, 2, Buf::new().u16(1).i16(-40).done()),
            (4, 2, Buf::new().u16(2).u16(5).done()),
            (
                6,
                2,
                link(
                    Buf::new().u16(3).i16(300).u16(0).done(),
                    vec![(4, 2, device(10, 10, 3, &[0x0300]))],
                ),
            ),
        ],
    );
    if wide {
        link(
            Buf::new().u32(0).u24(1).u24(0).done(),
            vec![(0, 4, coverage1(&[8])), (7, 3, carets)],
        )
    } else {
        link(
            Buf::new().u16(0).u16(1).u16(0).done(),
            vec![(0, 2, coverage1(&[8])), (4, 2, carets)],
        )
    }
}

fn mark_sets() -> Vec<u8> {
    link(
        Buf::new().u16(1).u16(2).u32(0).u32(0).done(),
        vec![
            (4, 4, coverage1(&[20, 21])),
            (8, 4, coverage2(&[(30, 39, 0)])),
        ],
    )
}

fn gdef_table(version: u32) -> Vec<u8> {
    let mut b = Buf::new().u32(version).u16(0).u16(0).u16(0).u16(0);
    if version >= 0x0001_0002 {
        b = b.u16(0);
    }
    if version >= 0x0001_0003 {
        b = b.u32(0);
    }
    let mut kids = vec![
        (
            4,
            2,
            class_def2(&[(1, 9, glyph_class::BASE as u16), (20, 39, 3)]),
        ),
        (6, 2, attach_list()),
        (8, 2, lig_caret_list(false)),
        (10, 2, class_def1(20, &[1, 2])),
    ];
    if version >= 0x0001_0002 {
        kids.push((12, 2, mark_sets()));
    }
    link(b.done(), kids)
}

#[test]
fn versions_and_subtables() {
    for version in [0x0001_0000, 0x0001_0002, 0x0001_0003] {
        let d = gdef_table(version);
        let t = Gdef::new(&d).unwrap();
        assert_eq!(t.version(), version);
        assert!(t.has_glyph_classes());
        let classes = t.glyph_class_def().unwrap().unwrap();
        assert_eq!(classes.get(g(5)), glyph_class::BASE);
        assert_eq!(classes.get(g(25)), glyph_class::MARK);
        assert_eq!(classes.get(g(15)), 0);
        let attach = t.mark_attach_class_def().unwrap().unwrap();
        assert_eq!(attach.get(g(21)), 2);

        let al = t.attach_list().unwrap().unwrap();
        assert_eq!(al.len(), 1);
        let p = al.points_of(g(7)).unwrap().unwrap();
        assert_eq!((p.len(), p.get(0), p.get(1)), (2, Some(3), Some(9)));
        assert!(al.points_of(g(6)).unwrap().is_none());

        let lc = t.lig_caret_list().unwrap().unwrap();
        assert_eq!(lc.coverage().unwrap().unwrap().get(g(8)), Some(0));
        let lg = lc.lig_glyph(0).unwrap().unwrap();
        assert_eq!(lg.len(), 3);
        assert!(matches!(
            lg.caret(0).unwrap(),
            Some(CaretValue::Coordinate(-40))
        ));
        assert!(matches!(lg.caret(1).unwrap(), Some(CaretValue::Point(5))));
        let Some(CaretValue::Device(300, Some(dev))) = lg.caret(2).unwrap() else {
            panic!()
        };
        assert_eq!(dev.delta_pixels(10), 3);
        assert_eq!(lg.caret(3).unwrap_err(), ReadError::OutOfBounds);

        let sets = t.mark_glyph_sets().unwrap();
        if version >= 0x0001_0002 {
            let sets = sets.unwrap();
            assert_eq!(sets.len(), 2);
            assert!(sets.covers(0, g(21)));
            assert!(!sets.covers(0, g(22)));
            assert!(sets.covers(1, g(35)));
            assert!(!sets.covers(2, g(35)));
        } else {
            assert!(sets.is_none());
        }
        assert_eq!(
            t.item_var_store_offset(),
            (version >= 0x0001_0003).then_some(0)
        );
    }
}

#[test]
fn version_1_4_uses_the_32_bit_offsets() {
    let mut b = Buf::new().u32(0x0001_0004);
    for _ in 0..5 {
        b = b.u16(0);
    }
    b = b.u32(0);
    for _ in 0..5 {
        b = b.u32(0);
    }
    let d = link(
        b.done(),
        vec![
            (4, 2, class_def1(1, &[3])),
            (18, 4, class_def1(1, &[1])),
            (26, 4, lig_caret_list(true)),
            (34, 4, mark_sets()),
        ],
    );
    let t = Gdef::new(&d).unwrap();
    assert_eq!(t.glyph_class_def().unwrap().unwrap().get(g(1)), 1);
    let lc = t.lig_caret_list().unwrap().unwrap();
    assert_eq!(lc.len(), 1);
    assert_eq!(lc.lig_glyph(0).unwrap().unwrap().len(), 3);
    assert_eq!(t.mark_glyph_sets().unwrap().unwrap().len(), 2);
    assert!(t.attach_list().unwrap().is_none());
}

#[test]
fn header_errors() {
    assert_eq!(
        Gdef::new(&[0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(),
        ReadError::UnknownVersion(0x0002_0000)
    );
    for (version, size) in [(0u16, 12usize), (2, 14), (3, 18), (4, 38)] {
        let mut d = Buf::new().u16(1).u16(version).done();
        d.resize(size - 1, 0);
        assert_eq!(
            Gdef::new(&d).unwrap_err(),
            ReadError::Truncated {
                needed: size,
                available: size - 1
            }
        );
        d.push(0);
        assert!(Gdef::new(&d).is_ok());
    }
    assert_eq!(
        gdef::MarkGlyphSets::new(&[0, 2, 0, 0]).unwrap_err(),
        ReadError::UnknownFormat(2)
    );
    assert_eq!(
        CaretValue::new(&[0, 4, 0, 0]).unwrap_err(),
        ReadError::UnknownFormat(4)
    );
}

/// HarfBuzz 15.1.0's `GDEF::is_blocklisted` cases (`hb-ot-layout.cc`):
/// the lengths of GDEF, GSUB and GPOS.
const BLOCKLIST: [(usize, usize, usize); 40] = [
    (442, 2874, 42038),
    (430, 2874, 40662),
    (442, 2874, 39116),
    (430, 2874, 39374),
    (490, 3046, 41638),
    (478, 3046, 41902),
    (898, 12554, 46470),
    (910, 12566, 47732),
    (928, 23298, 59332),
    (940, 23310, 60732),
    (964, 23836, 60072),
    (976, 23832, 61456),
    (994, 24474, 60336),
    (1006, 24470, 61740),
    (1006, 24576, 61346),
    (1018, 24572, 62828),
    (1006, 24576, 61352),
    (1018, 24572, 62834),
    (832, 7324, 47162),
    (844, 7302, 45474),
    (180, 13054, 7254),
    (192, 12638, 7254),
    (192, 12690, 7254),
    (188, 248, 3852),
    (188, 264, 3426),
    (1058, 47032, 11818),
    (1046, 47030, 12600),
    (1058, 71796, 16770),
    (1046, 71790, 17862),
    (1046, 71788, 17112),
    (1058, 71794, 17514),
    (1330, 109904, 57938),
    (1330, 109904, 58972),
    (1004, 59092, 14836),
    (588, 5078, 14418),
    (588, 5078, 14238),
    (894, 17162, 33960),
    (894, 17154, 34472),
    (816, 7868, 17052),
    (816, 7868, 17138),
];

#[test]
fn every_blocklist_entry_is_rejected() {
    for (a, b, c) in BLOCKLIST {
        assert!(gdef::is_blocklisted(a, b, c), "{a} {b} {c}");
        assert!(!gdef::is_blocklisted(a + 2, b, c));
        assert!(!gdef::is_blocklisted(a, b, c + 2));
    }
    assert!(!gdef::is_blocklisted(0, 0, 0));
    // HarfBuzz does not mask the lengths: a GPOS length that spills into
    // the GSUB bits is ORed into them.
    assert!(gdef::is_blocklisted(442, 2872, 42038 + (2 << 21)));
}
