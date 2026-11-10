//! The OpenType layout common tables (ADR 0020): Coverage and ClassDef
//! formats 1 to 4, Device deltas, the script, feature and lookup lists,
//! and the GSUB/GPOS header versions 1.0 to 1.2.

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

use sofel_read::device::Device;
use sofel_read::layout::{
    ClassDef, Coverage, LayoutTable, Lookup, LookupList, ScriptList, lookup_flag,
};
use sofel_read::{GlyphId, ReadError, Tag};

fn g(v: u32) -> GlyphId {
    GlyphId(v)
}

fn ranges(c: &Coverage<'_>) -> Vec<(u32, u32)> {
    c.ranges().map(|(a, b)| (a.0, b.0)).collect()
}

#[test]
fn coverage_format1() {
    let d = coverage1(&[2, 5, 9]);
    let c = Coverage::new(&d).unwrap();
    assert_eq!((c.format(), c.num_records()), (1, 3));
    assert_eq!(c.get(g(2)), Some(0));
    assert_eq!(c.get(g(5)), Some(1));
    assert_eq!(c.get(g(9)), Some(2));
    for out in [0, 1, 3, 8, 10, 0x10002] {
        assert_eq!(c.get(g(out)), None, "{out}");
    }
    assert_eq!(ranges(&c), vec![(2, 2), (5, 5), (9, 9)]);
    assert_eq!(c.glyph(1), Some(g(5)));
    assert_eq!(c.range(0), None);
}

#[test]
fn coverage_format2() {
    let d = coverage2(&[(10, 12, 0), (20, 20, 3), (40, 30, 4)]);
    let c = Coverage::new(&d).unwrap();
    assert_eq!((c.format(), c.num_records()), (2, 3));
    assert_eq!(c.get(g(10)), Some(0));
    assert_eq!(c.get(g(11)), Some(1));
    assert_eq!(c.get(g(12)), Some(2));
    assert_eq!(c.get(g(20)), Some(3));
    for out in [9, 13, 19, 21, 35] {
        assert_eq!(c.get(g(out)), None, "{out}");
    }
    // A range that starts after its end covers nothing.
    assert_eq!(ranges(&c), vec![(10, 12), (20, 20)]);
    assert_eq!(c.range(1), Some((g(20), g(20), 3)));
}

#[test]
fn coverage_format3_and_4_have_24_bit_glyphs() {
    let d = coverage3(&[0x1_0000, 0x1_0005, 0xFF_FFFF]);
    let c = Coverage::new(&d).unwrap();
    assert_eq!((c.format(), c.num_records()), (3, 3));
    assert_eq!(c.get(g(0x1_0000)), Some(0));
    assert_eq!(c.get(g(0x1_0005)), Some(1));
    assert_eq!(c.get(g(0xFF_FFFF)), Some(2));
    assert_eq!(c.get(g(0x1_0001)), None);
    assert_eq!(c.get(g(0)), None);

    let d = coverage4(&[(0x1_0000, 0x1_0002, 0), (0x2_0000, 0x2_0000, 0x1_0000)]);
    let c = Coverage::new(&d).unwrap();
    assert_eq!((c.format(), c.num_records()), (4, 2));
    assert_eq!(c.get(g(0x1_0000)), Some(0));
    assert_eq!(c.get(g(0x1_0002)), Some(2));
    assert_eq!(c.get(g(0x1_0003)), None);
    assert_eq!(c.get(g(0x2_0000)), Some(0x1_0000));
    assert_eq!(c.get(g(0x1_FFFF)), None);
    assert_eq!(c.get(g(0x2_0001)), None);
    assert_eq!(ranges(&c), vec![(0x1_0000, 0x1_0002), (0x2_0000, 0x2_0000)]);
}

#[test]
fn coverage_errors_and_empty() {
    assert_eq!(
        Coverage::new(&[0, 5, 0, 0]).unwrap_err(),
        ReadError::UnknownFormat(5)
    );
    assert!(matches!(
        Coverage::new(&[0, 1, 0, 2, 0, 1]),
        Err(ReadError::Truncated { .. })
    ));
    assert!(matches!(
        Coverage::new(&[0, 3, 0, 0]),
        Err(ReadError::Truncated { .. })
    ));
    let e = Coverage::empty();
    assert_eq!((e.format(), e.num_records(), e.get(g(0))), (0, 0, None));
}

#[test]
fn unsorted_coverage_gives_what_harfbuzz_finds() {
    // HarfBuzz's binary search probes the middle first, then moves on
    // the comparison: in [5, 2, 9], 2 is found, 5 is not.
    let d = coverage1(&[5, 2, 9]);
    let c = Coverage::new(&d).unwrap();
    assert_eq!(c.get(g(2)), Some(1));
    assert_eq!(c.get(g(5)), None);
    assert_eq!(c.get(g(9)), Some(2));
}

#[test]
fn class_def_formats() {
    let d = class_def1(10, &[1, 2, 3]);
    let c = ClassDef::new(&d).unwrap();
    assert_eq!(
        (c.format(), c.start_glyph(), c.num_records()),
        (1, g(10), 3)
    );
    let classes: Vec<u32> = [9, 10, 11, 12, 13].iter().map(|&x| c.get(g(x))).collect();
    assert_eq!(classes, vec![0, 1, 2, 3, 0]);

    let d = class_def2(&[(5, 7, 1), (20, 30, 2)]);
    let c = ClassDef::new(&d).unwrap();
    let classes: Vec<u32> = [4, 5, 7, 8, 19, 20, 30, 31]
        .iter()
        .map(|&x| c.get(g(x)))
        .collect();
    assert_eq!(classes, vec![0, 1, 1, 0, 0, 2, 2, 0]);

    let d = class_def3(0x1_0000, &[0x1_0001, 2]);
    let c = ClassDef::new(&d).unwrap();
    assert_eq!(c.format(), 3);
    assert_eq!(c.get(g(0xFFFF)), 0);
    assert_eq!(c.get(g(0x1_0000)), 0x1_0001);
    assert_eq!(c.get(g(0x1_0001)), 2);
    assert_eq!(c.get(g(0x1_0002)), 0);

    let d = class_def4(&[(0x1_0000, 0x1_0010, 7)]);
    let c = ClassDef::new(&d).unwrap();
    assert_eq!(c.format(), 4);
    assert_eq!(c.get(g(0xFFFF)), 0);
    assert_eq!(c.get(g(0x1_0000)), 7);
    assert_eq!(c.get(g(0x1_0010)), 7);
    assert_eq!(c.get(g(0x1_0011)), 0);

    assert_eq!(ClassDef::empty().get(g(3)), 0);
    assert_eq!(
        ClassDef::new(&[0, 9]).unwrap_err(),
        ReadError::UnknownFormat(9)
    );
    assert!(matches!(
        ClassDef::new(&[0, 1, 0, 0, 0, 2, 0, 1]),
        Err(ReadError::Truncated { .. })
    ));
}

#[test]
fn device_deltas() {
    // Format 1: 2-bit values 1, -1, 0, -2, 1, 1, -1, 0 for ppem 1 to 8.
    let d = device(1, 8, 1, &[0b01_11_00_10_01_01_11_00]);
    let dev = Device::new(&d).unwrap();
    let got: Vec<i32> = (0..=9).map(|p| dev.delta_pixels(p)).collect();
    assert_eq!(got, vec![0, 1, -1, 0, -2, 1, 1, -1, 0, 0]);

    // Format 2: 4-bit values 1, -1, 7, -8 for ppem 10 to 13.
    let d = device(10, 13, 2, &[0x1F78]);
    let dev = Device::new(&d).unwrap();
    let got: Vec<i32> = (9..=14).map(|p| dev.delta_pixels(p)).collect();
    assert_eq!(got, vec![0, 1, -1, 7, -8, 0]);

    // Format 3: 8-bit values 127, -128, -1 for ppem 5 to 7.
    let d = device(5, 7, 3, &[0x7F80, 0xFF00]);
    let dev = Device::new(&d).unwrap();
    let got: Vec<i32> = (4..=8).map(|p| dev.delta_pixels(p)).collect();
    assert_eq!(got, vec![0, 127, -128, -1, 0]);

    // A VariationIndex table and unknown formats give no delta.
    let d = device(1, 2, 0x8000, &[]);
    let dev = Device::new(&d).unwrap();
    assert_eq!(dev.variation_index(), Some((1, 2)));
    assert_eq!(dev.delta_pixels(1), 0);
    let d = device(1, 2, 4, &[0xFFFF]);
    assert_eq!(Device::new(&d).unwrap().delta_pixels(1), 0);
}

fn sample_layout(version: u32) -> Vec<u8> {
    let scripts = script_list(vec![
        (b"DFLT", script(lang_sys(0xFFFF, &[0]), vec![])),
        (
            b"latn",
            script(
                lang_sys(1, &[0, 1]),
                vec![(b"DEU ", lang_sys(0xFFFF, &[1])), (b"TRK ", vec![])],
            ),
        ),
    ]);
    let features = feature_list(&[(b"liga", &[0, 1]), (b"locl", &[1])]);
    let lookups = lookup_list(
        vec![
            lookup(
                1,
                lookup_flag::IGNORE_MARKS,
                vec![vec![0, 1, 0, 0, 0, 0]],
                None,
            ),
            lookup(
                4,
                lookup_flag::USE_MARK_FILTERING_SET | 0x0300,
                vec![vec![0, 1], vec![0, 2]],
                Some(7),
            ),
        ],
        version >= 0x0001_0002,
    );
    gsubgpos(version, scripts, features, lookups)
}

#[test]
fn lists_and_lookups() {
    for version in [0x0001_0000, 0x0001_0001, 0x0001_0002] {
        let d = sample_layout(version);
        let t = LayoutTable::new(&d).unwrap();
        assert_eq!(t.version(), version);
        assert_eq!(
            t.feature_variations_offset(),
            (version >= 0x0001_0001).then_some(0)
        );
        assert_eq!(t.has_wide_lookup_list(), version >= 0x0001_0002);

        let scripts = t.script_list().unwrap().unwrap();
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts.tag(1), Some(Tag::new(b"latn")));
        assert_eq!(scripts.find(Tag::new(b"latn")), Some(1));
        assert_eq!(scripts.find(Tag::new(b"cyrl")), None);
        let latn = scripts.script(1).unwrap().unwrap();
        let default = latn.default_lang_sys().unwrap().unwrap();
        assert_eq!(default.required_feature_index(), 1);
        assert_eq!(default.num_features(), 2);
        assert_eq!(default.feature_index(1), Some(1));
        assert_eq!(default.feature_index(2), None);
        assert_eq!(latn.num_lang_sys(), 2);
        assert_eq!(latn.lang_sys_tag(0), Some(Tag::new(b"DEU ")));
        assert_eq!(latn.find_lang_sys(Tag::new(b"TRK ")), Some(1));
        assert_eq!(latn.lang_sys(1).unwrap().is_none(), true);
        let deu = latn.lang_sys(0).unwrap().unwrap();
        assert_eq!(deu.required_feature_index(), 0xFFFF);
        assert_eq!(latn.lang_sys(2).unwrap_err(), ReadError::OutOfBounds);
        let dflt = scripts.script(0).unwrap().unwrap();
        assert_eq!(dflt.num_lang_sys(), 0);

        let features = t.feature_list().unwrap().unwrap();
        assert_eq!(features.len(), 2);
        assert_eq!(features.tag(0), Some(Tag::new(b"liga")));
        let liga = features.feature(0).unwrap().unwrap();
        assert_eq!(liga.feature_params_offset(), 0);
        assert_eq!(liga.num_lookups(), 2);
        assert_eq!(liga.lookup_index(1), Some(1));
        assert_eq!(features.feature(2).unwrap_err(), ReadError::OutOfBounds);

        let lookups = t.lookup_list().unwrap().unwrap();
        assert_eq!(
            (lookups.len(), lookups.is_wide()),
            (2, version >= 0x0001_0002)
        );
        let l0 = lookups.lookup(0).unwrap().unwrap();
        assert_eq!(l0.lookup_type(), 1);
        assert_eq!(l0.flag(), lookup_flag::IGNORE_MARKS);
        assert_eq!(l0.mark_filtering_set(), None);
        assert_eq!(l0.num_subtables(), 1);
        assert_eq!(&l0.subtable_data(0).unwrap()[..2], &[0, 1]);
        let l1 = lookups.lookup(1).unwrap().unwrap();
        assert_eq!(l1.lookup_type(), 4);
        assert_eq!(l1.flag() & lookup_flag::MARK_ATTACHMENT_TYPE_MASK, 0x0300);
        assert_eq!(l1.mark_filtering_set(), Some(7));
        assert_eq!(l1.num_subtables(), 2);
        assert_eq!(l1.subtable_data(1).unwrap(), &[0, 2]);
        assert_eq!(l1.subtable_data(2), None);
        assert_eq!(lookups.lookup(2).unwrap_err(), ReadError::OutOfBounds);
    }
}

#[test]
fn version_1_2_prefers_the_32_bit_offsets() {
    // Version 1.2 with both: the 16-bit lists say one thing, the 32-bit
    // ones another.
    let narrow = sample_layout(0x0001_0000);
    let mut d = Buf::new()
        .u32(0x0001_0002)
        .u16(0)
        .u16(0)
        .u16(0)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(0)
        .done();
    let lists = script_list(vec![(b"cyrl", script(vec![], vec![]))]);
    d = link(d, vec![(4, 2, lists.clone())]);
    let at = d.len();
    d.extend_from_slice(&narrow);
    // 32-bit script list: the one inside the 1.0 table.
    let inner = u16::from_be_bytes([narrow[4], narrow[5]]) as u32;
    d[14..18].copy_from_slice(&(at as u32 + inner).to_be_bytes());
    let t = LayoutTable::new(&d).unwrap();
    let s = t.script_list().unwrap().unwrap();
    assert_eq!(s.tag(0), Some(Tag::new(b"DFLT")));
    // Null 32-bit offsets fall back to the 16-bit ones.
    d[14..18].copy_from_slice(&[0; 4]);
    let t = LayoutTable::new(&d).unwrap();
    assert_eq!(
        t.script_list().unwrap().unwrap().tag(0),
        Some(Tag::new(b"cyrl"))
    );
    assert!(!t.has_wide_lookup_list());
}

#[test]
fn layout_header_errors() {
    assert_eq!(
        LayoutTable::new(&[0, 2, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap_err(),
        ReadError::UnknownVersion(0x0002_0000)
    );
    assert!(matches!(
        LayoutTable::new(&[0, 1, 0, 1, 0, 0, 0, 0, 0, 0]),
        Err(ReadError::Truncated { needed: 14, .. })
    ));
    assert!(matches!(
        LayoutTable::new(&[0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        Err(ReadError::Truncated { needed: 26, .. })
    ));
    assert!(matches!(
        Lookup::new(&[0, 1, 0, 0x10, 0, 1, 0, 8]),
        Err(ReadError::Truncated { .. })
    ));
    assert!(matches!(
        LookupList::new(&[0, 2, 0, 4], false),
        Err(ReadError::Truncated { .. })
    ));
    assert!(matches!(
        ScriptList::new(&[0, 1, b'D', b'F', b'L', b'T']),
        Err(ReadError::Truncated { .. })
    ));
}
