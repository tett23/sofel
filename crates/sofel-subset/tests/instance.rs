//! Variation subsetting on hand-made fonts (ADR 0022): axis functions,
//! full pinning, and dropping an unused BASE variation store.

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::sfnt::Sfnt;
use sofel_subset::{AxisRange, Input, Plan, SetKind, SubsetError, subset};

fn fixed(v: f64) -> [u8; 4] {
    ((v * 65536.0) as i32).to_be_bytes()
}

/// An fvar with axes `(tag, min, default, max)` and one instance.
fn fvar(axes: &[(&[u8; 4], f64, f64, f64)]) -> Vec<u8> {
    let n = axes.len() as u16;
    let mut d = vec![0, 1, 0, 0];
    d.extend(16u16.to_be_bytes());
    d.extend(2u16.to_be_bytes());
    d.extend(n.to_be_bytes());
    d.extend(20u16.to_be_bytes());
    d.extend(1u16.to_be_bytes());
    d.extend((n * 4 + 6).to_be_bytes());
    for (i, (tag, min, def, max)) in axes.iter().enumerate() {
        d.extend(*tag);
        d.extend(fixed(*min));
        d.extend(fixed(*def));
        d.extend(fixed(*max));
        d.extend(0u16.to_be_bytes());
        d.extend((256 + i as u16).to_be_bytes());
    }
    d.extend(300u16.to_be_bytes());
    d.extend(0u16.to_be_bytes());
    for (_, _, def, _) in axes {
        d.extend(fixed(*def));
    }
    d.extend(301u16.to_be_bytes());
    d
}

/// An avar of `version` mapping each axis with `maps`.
fn avar(version: u16, maps: &[&[(f64, f64)]]) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend(version.to_be_bytes());
    d.extend([0, 0, 0, 0]);
    d.extend((maps.len() as u16).to_be_bytes());
    for m in maps {
        d.extend((m.len() as u16).to_be_bytes());
        for (a, b) in *m {
            d.extend(((a * 16384.0) as i16).to_be_bytes());
            d.extend(((b * 16384.0) as i16).to_be_bytes());
        }
    }
    if version == 2 {
        d.extend([0; 8]);
    }
    d
}

/// A STAT 1.1 with design axes wght and wdth and format 1 axis values
/// `(axis, name, value)`.
fn stat(values: &[(u16, u16, f64)]) -> Vec<u8> {
    let mut d = vec![0, 1, 0, 1];
    d.extend(8u16.to_be_bytes());
    d.extend(2u16.to_be_bytes());
    d.extend(20u32.to_be_bytes());
    d.extend((values.len() as u16).to_be_bytes());
    d.extend(36u32.to_be_bytes());
    d.extend(2u16.to_be_bytes());
    for (tag, name) in [(b"wght", 256u16), (b"wdth", 257)] {
        d.extend(tag);
        d.extend(name.to_be_bytes());
        d.extend(0u16.to_be_bytes());
    }
    let first = values.len() * 2;
    for i in 0..values.len() {
        d.extend(((first + i * 12) as u16).to_be_bytes());
    }
    for (axis, name, value) in values {
        d.extend(1u16.to_be_bytes());
        d.extend(axis.to_be_bytes());
        d.extend(0u16.to_be_bytes());
        d.extend(name.to_be_bytes());
        d.extend(fixed(*value));
    }
    d
}

const AXES: [(&[u8; 4], f64, f64, f64); 2] = [
    (b"wght", 100.0, 400.0, 900.0),
    (b"wdth", 75.0, 100.0, 100.0),
];

fn font_with(extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    let mut t = sample_tables();
    t.extend(extra);
    t.sort_by_key(|(t, _)| **t);
    sfnt(&t)
}

fn point(v: f32) -> AxisRange {
    AxisRange {
        min: v,
        default: v,
        max: v,
    }
}

fn tables(out: &[u8]) -> Vec<Tag> {
    Sfnt::new(out).unwrap().records().map(|r| r.tag).collect()
}

#[test]
fn axis_functions() {
    let data = font_with(vec![(b"fvar", fvar(&AXES))]);
    let font = Font::new(&data, 0).unwrap();
    let wght = Tag::new(b"wght");
    let wdth = Tag::new(b"wdth");
    let i = Input::new();
    let all = i.with_all_axes_pinned_to_default(&font).unwrap();
    assert_eq!(all.axis_range(wght), Some(point(400.0)));
    assert_eq!(all.axis_range(wdth), Some(point(100.0)));
    assert_eq!(
        i.with_axis_pinned_to_default(wdth, &font)
            .unwrap()
            .axes()
            .len(),
        1
    );
    // pin_axis_location clamps to the axis range.
    let p = i.with_axis_pinned(wght, 1000.0, &font).unwrap();
    assert_eq!(p.axis_range(wght), Some(point(900.0)));
    // set_axis_range: missing values are the axis's, values are clamped.
    let r = i
        .with_axis_range(wght, Some(50.0), None, Some(950.0), &font)
        .unwrap();
    assert_eq!(
        r.axis_range(wght),
        Some(AxisRange {
            min: 100.0,
            default: 900.0,
            max: 900.0
        })
    );
    let nan = i
        .with_axis_range(wght, Some(f32::NAN), Some(500.0), None, &font)
        .unwrap();
    assert_eq!(
        nan.axis_range(wght),
        Some(AxisRange {
            min: 100.0,
            default: 400.0,
            max: 500.0
        })
    );
    // Failures leave the input as it was: a missing axis, min > max.
    let opsz = Tag::new(b"opsz");
    assert!(i.with_axis_pinned_to_default(opsz, &font).is_none());
    assert!(i.with_axis_pinned(opsz, 12.0, &font).is_none());
    assert!(i.with_axis_range(opsz, None, None, None, &font).is_none());
    assert!(
        i.with_axis_range(wght, Some(600.0), Some(500.0), None, &font)
            .is_none()
    );
    assert!(i.axes().is_empty());
}

#[test]
fn axis_functions_fail_without_fvar() {
    let data = sample_font();
    let font = Font::new(&data, 0).unwrap();
    let wght = Tag::new(b"wght");
    let i = Input::new();
    assert!(i.with_all_axes_pinned_to_default(&font).is_none());
    assert!(i.with_axis_pinned_to_default(wght, &font).is_none());
    assert!(i.with_axis_pinned(wght, 400.0, &font).is_none());
    assert!(i.with_axis_range(wght, None, None, None, &font).is_none());
    // A broken fvar (axisSize 24) is no fvar, as for HarfBuzz.
    let mut bad = fvar(&AXES);
    bad[11] = 24;
    let data = font_with(vec![(b"fvar", bad)]);
    let font = Font::new(&data, 0).unwrap();
    assert!(i.with_all_axes_pinned_to_default(&font).is_none());
}

fn unicodes() -> Input {
    Input::new().map_set(SetKind::Unicodes, |s| s.add(0x41))
}

#[test]
fn only_pinning_every_axis_to_its_default_is_planned() {
    let data = font_with(vec![(b"fvar", fvar(&AXES))]);
    let font = Font::new(&data, 0).unwrap();
    let wght = Tag::new(b"wght");
    let pinned = unicodes().with_all_axes_pinned_to_default(&font).unwrap();
    assert!(Plan::new(&font, &pinned).is_ok());
    let unsupported = |i: &Input| {
        matches!(
            Plan::new(&font, i),
            Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"fvar")
        )
    };
    // Another location, a range, or one axis of two.
    assert!(unsupported(
        &unicodes().with_axis_pinned(wght, 500.0, &font).unwrap()
    ));
    assert!(unsupported(
        &unicodes()
            .with_axis_range(wght, Some(300.0), Some(500.0), None, &font)
            .unwrap()
    ));
    assert!(unsupported(
        &unicodes().with_axis_pinned_to_default(wght, &font).unwrap()
    ));
    // A range equal to fvar's is no setting at all (normalize_axes_location).
    let same = unicodes()
        .with_axis_range(wght, None, None, None, &font)
        .unwrap();
    assert!(Plan::new(&font, &same).is_ok());
}

#[test]
fn avar_that_moves_the_default_or_version_2_is_unsupported() {
    let moved = avar(1, &[&[(-1.0, -1.0), (0.0, 0.25), (1.0, 1.0)], &[]]);
    let identity = avar(1, &[&[(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)], &[]]);
    let v2 = avar(2, &[&[], &[]]);
    for (avar, ok) in [(identity, true), (moved, false), (v2, false)] {
        let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"avar", avar)]);
        let font = Font::new(&data, 0).unwrap();
        let i = unicodes().with_all_axes_pinned_to_default(&font).unwrap();
        let r = Plan::new(&font, &i);
        assert_eq!(r.is_ok(), ok, "{r:?}");
    }
}

#[test]
fn pinning_drops_the_variation_tables_and_filters_stat() {
    let st = stat(&[(0, 258, 400.0), (0, 259, 700.0), (1, 260, 100.0)]);
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"avar", avar(1, &[&[], &[]])),
        (b"HVAR", vec![0, 1, 0, 0]),
        (b"MVAR", vec![0, 1, 0, 0]),
        (b"STAT", st.clone()),
    ]);
    let font = Font::new(&data, 0).unwrap();
    // Unpinned, the variation tables wait for stage 10 (HVAR), and fvar,
    // avar, MVAR and STAT are copied.
    let drop_hvar = unicodes().map_set(SetKind::DropTables, |s| s.add(Tag::new(b"HVAR").to_u32()));
    let out = subset(&font, &drop_hvar).unwrap();
    let t = tables(&out);
    for tag in [b"fvar", b"avar", b"MVAR", b"STAT"] {
        assert!(t.contains(&Tag::new(tag)), "{tag:?}");
    }
    assert_eq!(
        Sfnt::new(&out).unwrap().table(Tag::new(b"STAT")),
        Some(st.as_slice())
    );
    // Pinned: fvar, avar, HVAR and MVAR are dropped; STAT keeps the axis
    // values at the pinned location.
    let pinned = unicodes().with_all_axes_pinned_to_default(&font).unwrap();
    let out = subset(&font, &pinned).unwrap();
    let t = tables(&out);
    for tag in [b"fvar", b"avar", b"HVAR", b"MVAR"] {
        assert!(!t.contains(&Tag::new(tag)), "{tag:?}");
    }
    let s = Sfnt::new(&out).unwrap();
    let stat = sofel_font::read::stat::Stat::new(s.table(Tag::new(b"STAT")).unwrap()).unwrap();
    let names: Vec<u16> = (0..stat.axis_value_count())
        .map(|i| stat.axis_value(i).unwrap().0.name_id().unwrap())
        .collect();
    assert_eq!(names, vec![258, 260]);
    assert_eq!(stat.design_axis_count(), 2);
}

#[test]
fn pinning_removes_the_base_store_even_with_no_remaining_axes() {
    // BASE 1.1, no horizontal or vertical Axis, and an empty two-axis store.
    let base = vec![
        0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 1, 0, 0, 0, 8, 0, 0, 0, 2, 0, 0,
    ];
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", base)]);
    let font = Font::new(&data, 0).unwrap();
    let pinned = unicodes().with_all_axes_pinned_to_default(&font).unwrap();
    let out = subset(&font, &pinned).unwrap();
    let sfnt = Sfnt::new(&out).unwrap();
    assert_eq!(
        sfnt.table(Tag::new(b"BASE")).unwrap(),
        &[0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn pinning_preserves_the_base_coordinate_and_removes_its_variation_index() {
    let words = [
        1u16, 1, 12, 0, 0, 54, // BASE 1.1, horizontal Axis and store
        4, 10, 1, 0x726f, 0x6d6e, // Axis, BaseTagList: romn
        1, 0x4446, 0x4c54, 8, // BaseScriptList: DFLT
        6, 0, 0, // BaseScript
        0, 1, 6, // BaseValues with one BaseCoord
        3, 123, 6, 0, 0, 0x8000, // BaseCoord and VariationIndex (0, 0)
        1, 0, 12, 1, 0, 28, // ItemVariationStore
        2, 1, 0, 0x4000, 0x4000, 0, 0, 0, // one region, two axes
        1, 1, 1, 0, 21, // one item
    ];
    let base = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", base)]);
    let font = Font::new(&data, 0).unwrap();
    let pinned = unicodes().with_all_axes_pinned_to_default(&font).unwrap();
    let out = subset(&font, &pinned).unwrap();
    let sfnt = Sfnt::new(&out).unwrap();
    // hb-subset 15.1.0, --unicodes=41 --variations=*=drop.
    let expected = [
        1u16, 1, 12, 0, 0, 0, // version, axes, null variation store
        30, 4, 1, 0x4446, 0x4c54, 8, // Axis and BaseScriptList
        6, 0, 0, 0, 1, 6, // BaseScript and BaseValues
        3, 123, 0, // BaseCoord, null VariationIndex
        1, 0x726f, 0x6d6e, // BaseTagList
    ];
    let expected: Vec<_> = expected.into_iter().flat_map(u16::to_be_bytes).collect();
    assert_eq!(sfnt.table(Tag::new(b"BASE")).unwrap(), expected);
}

#[test]
fn unpinned_base_store_without_variation_index_is_removed() {
    let words = [
        1u16, 1, 12, 0, 0, 54, 4, 10, 1, 0x726f, 0x6d6e, 1, 0x4446, 0x4c54, 8, 6, 0, 0, 0, 1, 6, 3,
        123, 0, 0, 0, 0x8000, 1, 0, 12, 1, 0, 28, 2, 1, 0, 0x4000, 0x4000, 0, 0, 0, 1, 1, 1, 0, 21,
    ];
    let base = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", base)]);
    let font = Font::new(&data, 0).unwrap();
    let output = subset(&font, &unicodes()).unwrap();
    let sfnt = Sfnt::new(&output).unwrap();
    let result = sfnt.table(Tag::new(b"BASE")).unwrap();
    assert_eq!(result.len(), 48);
    assert_eq!(&result[8..12], &[0, 0, 0, 0]);
    assert_eq!(&result[36..42], &[0, 3, 0, 123, 0, 0]);
}

#[test]
fn unpinned_base_store_with_variation_index_is_unsupported() {
    let words = [
        1u16, 1, 12, 0, 0, 54, 4, 10, 1, 0x726f, 0x6d6e, 1, 0x4446, 0x4c54, 8, 6, 0, 0, 0, 1, 6, 3,
        123, 6, 0, 0, 0x8000, 1, 0, 12, 1, 0, 28, 2, 1, 0, 0x4000, 0x4000, 0, 0, 0, 1, 1, 1, 0, 21,
    ];
    let base: Vec<u8> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", base.clone())]);
    let font = Font::new(&data, 0).unwrap();
    assert!(matches!(
        subset(&font, &unicodes()),
        Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"BASE")
    ));

    let mut min_max = base.clone();
    min_max[31] = 0;
    min_max[33] = 6;
    min_max[37] = 6;
    min_max[39] = 0;
    min_max[41] = 0;
    let view = sofel_font::read::base::Base::new(&min_max).unwrap();
    let min = view
        .horiz_axis()
        .unwrap()
        .unwrap()
        .base_script_list()
        .unwrap()
        .unwrap()
        .script(0)
        .unwrap()
        .unwrap()
        .default_min_max()
        .unwrap()
        .unwrap()
        .min_coord()
        .unwrap()
        .unwrap();
    assert_eq!(
        min.device().unwrap().unwrap().variation_index(),
        Some((0, 0))
    );
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", min_max)]);
    let font = Font::new(&data, 0).unwrap();
    assert!(matches!(
        subset(&font, &unicodes()),
        Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"BASE")
    ));

    let mut vertical = base;
    vertical[5] = 0;
    vertical[7] = 12;
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", vertical)]);
    let font = Font::new(&data, 0).unwrap();
    assert!(matches!(
        subset(&font, &unicodes()),
        Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"BASE")
    ));
}
