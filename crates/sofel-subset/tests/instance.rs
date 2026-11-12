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
fn partial_instancing_with_unrebased_variation_data_remains_unsupported() {
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"HVAR", vec![0, 1, 0, 0])]);
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
fn i1_subset_instance_one_default_axis_keeps_the_other_axis() {
    let data = font_with(vec![(b"fvar", fvar(&AXES))]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned_to_default(Tag::new(b"wdth"), &font)
        .unwrap();
    let result = subset(&font, &input);
    assert!(result.is_ok(), "{result:?}");
    let output = result.unwrap();
    let sfnt = Sfnt::new(&output).unwrap();
    let fvar = sfnt.table(Tag::new(b"fvar")).unwrap();
    assert_eq!(u16::from_be_bytes([fvar[8], fvar[9]]), 1);
    assert_eq!(&fvar[16..20], b"wght");
    assert_eq!(u16::from_be_bytes([fvar[12], fvar[13]]), 1);
    assert_eq!(&fvar[40..44], &fixed(400.0));
}

#[test]
fn i1_subset_instance_restricts_remaining_fvar_axis_range() {
    let data = font_with(vec![(b"fvar", fvar(&AXES))]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_range(Tag::new(b"wght"), Some(300.0), Some(500.0), None, &font)
        .unwrap()
        .with_axis_pinned_to_default(Tag::new(b"wdth"), &font)
        .unwrap();
    let output = subset(&font, &input).unwrap();
    let sfnt = Sfnt::new(&output).unwrap();
    let fvar = sfnt.table(Tag::new(b"fvar")).unwrap();
    assert_eq!(&fvar[20..24], &fixed(300.0));
    assert_eq!(&fvar[24..28], &fixed(400.0));
    assert_eq!(&fvar[28..32], &fixed(500.0));
}

#[test]
fn i1_subset_instance_endpoint_pin_removes_outside_named_instance() {
    let data = font_with(vec![(b"fvar", fvar(&AXES))]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned(Tag::new(b"wght"), 900.0, &font)
        .unwrap();
    let output = subset(&font, &input).unwrap();
    let sfnt = Sfnt::new(&output).unwrap();
    let fvar = sfnt.table(Tag::new(b"fvar")).unwrap();
    assert_eq!(u16::from_be_bytes([fvar[8], fvar[9]]), 1);
    assert_eq!(&fvar[16..20], b"wdth");
    assert_eq!(u16::from_be_bytes([fvar[12], fvar[13]]), 0);
}

#[test]
fn i1_subset_instance_endpoint_pin_filters_stat_without_variation_data() {
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (
            b"STAT",
            stat(&[
                (0, 258, 100.0),
                (0, 259, 400.0),
                (0, 260, 900.0),
                (1, 261, 75.0),
            ]),
        ),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned(Tag::new(b"wght"), 900.0, &font)
        .unwrap();
    let output = subset(&font, &input).unwrap();
    // hb-dump --subset from HarfBuzz 3df40d9, --instance=wght=900.
    assert_eq!(
        output,
        include_bytes!("data/adr0061-stat-endpoint-hb15.ttf")
    );
    let sfnt = Sfnt::new(&output).unwrap();
    let stat = sfnt.table(Tag::new(b"STAT")).unwrap();
    assert_eq!(u16::from_be_bytes([stat[12], stat[13]]), 2);
}

#[test]
fn i1_subset_instance_range_filters_stat_without_variation_data() {
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (
            b"STAT",
            stat(&[
                (0, 258, 100.0),
                (0, 259, 400.0),
                (0, 260, 900.0),
                (1, 261, 75.0),
            ]),
        ),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_range(
            Tag::new(b"wght"),
            Some(300.0),
            Some(500.0),
            Some(400.0),
            &font,
        )
        .unwrap();
    let output = subset(&font, &input).unwrap();
    // hb-dump --subset from HarfBuzz 3df40d9, --instance=wght=300:400:500.
    assert_eq!(output, include_bytes!("data/adr0061-stat-range-hb15.ttf"));
    let sfnt = Sfnt::new(&output).unwrap();
    let stat = sfnt.table(Tag::new(b"STAT")).unwrap();
    assert_eq!(u16::from_be_bytes([stat[12], stat[13]]), 2);
}

#[test]
fn i1_subset_instance_all_axes_pinned_filters_stat_without_variation_data() {
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (
            b"STAT",
            stat(&[
                (0, 258, 100.0),
                (0, 259, 400.0),
                (0, 260, 900.0),
                (1, 261, 75.0),
            ]),
        ),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned(Tag::new(b"wght"), 900.0, &font)
        .unwrap()
        .with_axis_pinned_to_default(Tag::new(b"wdth"), &font)
        .unwrap();
    let output = subset(&font, &input).unwrap();
    // hb-dump --subset from HarfBuzz 3df40d9, --instance=wght=900,wdth=drop.
    assert_eq!(output, include_bytes!("data/adr0061-stat-all-hb15.ttf"));
    let sfnt = Sfnt::new(&output).unwrap();
    let stat = sfnt.table(Tag::new(b"STAT")).unwrap();
    assert_eq!(u16::from_be_bytes([stat[12], stat[13]]), 1);
}

#[test]
fn i1_subset_instance_default_pin_keeps_avar_and_filters_stat() {
    let map = [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)];
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"avar", avar(1, &[&map, &map])),
        (
            b"STAT",
            stat(&[
                (0, 258, 100.0),
                (0, 259, 400.0),
                (0, 260, 900.0),
                (1, 261, 75.0),
            ]),
        ),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned_to_default(Tag::new(b"wdth"), &font)
        .unwrap();
    let output = subset(&font, &input).unwrap();
    // hb-dump --subset from HarfBuzz 3df40d9, --instance=wdth=drop.
    assert_eq!(output, include_bytes!("data/adr0061-stat-avar-hb15.ttf"));
    let sfnt = Sfnt::new(&output).unwrap();
    let stat = sfnt.table(Tag::new(b"STAT")).unwrap();
    assert_eq!(u16::from_be_bytes([stat[12], stat[13]]), 3);
}

#[test]
fn i1_subset_instance_nondefault_pin_with_identity_avar() {
    let identity = [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)];
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"avar", avar(1, &[&identity, &identity])),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned(Tag::new(b"wght"), 900.0, &font)
        .unwrap();
    let output = subset(&font, &input).unwrap();
    // HarfBuzz 3df40d9, hb-dump --subset --unicodes=41 --instance=wght=900.
    assert_eq!(output, include_bytes!("data/adr0061-avar-pin-hb15.ttf"));
}

#[test]
fn i1_subset_instance_range_with_identity_avar() {
    let identity = [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)];
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"avar", avar(1, &[&identity, &identity])),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_range(
            Tag::new(b"wght"),
            Some(300.0),
            Some(500.0),
            Some(400.0),
            &font,
        )
        .unwrap();
    let output = subset(&font, &input).unwrap();
    // HarfBuzz 3df40d9, hb-dump --subset --unicodes=41 --instance=wght=300:400:500.
    assert_eq!(output, include_bytes!("data/adr0061-avar-range-hb15.ttf"));
}

#[test]
fn i1_subset_instance_one_sided_range_with_identity_avar() {
    let identity = [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)];
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"avar", avar(1, &[&identity, &identity])),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_range(
            Tag::new(b"wght"),
            Some(400.0),
            Some(500.0),
            Some(400.0),
            &font,
        )
        .unwrap();
    let output = subset(&font, &input).unwrap();
    // HarfBuzz 3df40d9, hb-dump --subset --unicodes=41 --instance=wght=400:400:500.
    assert_eq!(
        output,
        include_bytes!("data/adr0061-avar-one-sided-hb15.ttf")
    );
}

#[test]
fn i1_subset_instance_nonidentity_avar_rejects_nondefault_pin() {
    let shifted = [(-1.0, -1.0), (0.0, 0.0), (0.5, 0.75), (1.0, 1.0)];
    let identity = [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)];
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"avar", avar(1, &[&shifted, &identity])),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned(Tag::new(b"wght"), 900.0, &font)
        .unwrap();
    assert!(matches!(
        subset(&font, &input),
        Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"fvar")
    ));
}

#[test]
fn i1_subset_instance_keeps_unmodified_avar_map_of_remaining_axis() {
    let wght_map = [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)];
    let wdth_map = [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)];
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"avar", avar(1, &[&wght_map, &wdth_map])),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned_to_default(Tag::new(b"wdth"), &font)
        .unwrap();
    let output = subset(&font, &input).unwrap();
    let sfnt = Sfnt::new(&output).unwrap();
    let avar_bytes = sfnt.table(Tag::new(b"avar")).unwrap();
    assert_eq!(u16::from_be_bytes([avar_bytes[6], avar_bytes[7]]), 1);
    assert_eq!(&avar_bytes[8..], &avar(1, &[&wght_map])[8..]);
}

#[test]
fn i1_subset_instance_drops_names_of_removed_axes() {
    let names = name(
        0,
        &[
            (3, 1, 0x409, 256, b"W"),
            (3, 1, 0x409, 257, b"D"),
            (3, 1, 0x409, 300, b"I"),
            (3, 1, 0x409, 301, b"P"),
        ],
        &[],
    );
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"name", names)]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes()
        .with_axis_pinned_to_default(Tag::new(b"wdth"), &font)
        .unwrap();
    let output = subset(&font, &input).unwrap();
    let sfnt = Sfnt::new(&output).unwrap();
    let names = sfnt.table(Tag::new(b"name")).unwrap();
    let count = usize::from(u16::from_be_bytes([names[2], names[3]]));
    let ids: Vec<u16> = (0..count)
        .map(|i| {
            let at = 12 + i * 12;
            u16::from_be_bytes([names[at], names[at + 1]])
        })
        .collect();
    assert_eq!(ids, vec![256, 300, 301]);
}

#[test]
fn i2_unknown_axis_preserves_previous_axis_range_and_output() {
    let data = font_with(vec![(b"fvar", fvar(&AXES))]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes().with_all_axes_pinned_to_default(&font).unwrap();
    let before = input.axis_range(Tag::new(b"wght"));
    let output_before = subset(&font, &input).unwrap();
    assert!(
        input
            .with_axis_pinned_to_default(Tag::new(b"opsz"), &font)
            .is_none()
    );
    assert_eq!(input.axis_range(Tag::new(b"wght")), before);
    assert_eq!(input.axes().len(), 2);
    assert_eq!(subset(&font, &input).unwrap(), output_before);
}

#[test]
fn full_pin_accepts_identity_avar_versions_and_rejects_a_moved_v1_default() {
    let moved = avar(1, &[&[(-1.0, -1.0), (0.0, 0.25), (1.0, 1.0)], &[]]);
    let identity = avar(1, &[&[(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0)], &[]]);
    let v2 = avar(2, &[&[], &[]]);
    for (avar, ok) in [(identity, true), (moved, false), (v2, true)] {
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
    for (weight, baseline) in [(650.0, 134u16), (900.0, 144)] {
        let input = pinned
            .with_axis_pinned(Tag::new(b"wght"), weight, &font)
            .unwrap();
        let out = subset(&font, &input).unwrap();
        let mut varied = expected.clone();
        varied[38..40].copy_from_slice(&baseline.to_be_bytes());
        assert_eq!(
            Sfnt::new(&out).unwrap().table(Tag::new(b"BASE")).unwrap(),
            varied
        );
    }
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

fn base_with_variation_index() -> Vec<u8> {
    let words = [
        1u16, 1, 12, 0, 0, 54, 4, 10, 1, 0x726f, 0x6d6e, 1, 0x4446, 0x4c54, 8, 6, 0, 0, 0, 1, 6, 3,
        123, 6, 0, 0, 0x8000, 1, 0, 12, 1, 0, 28, 2, 1, 0, 0x4000, 0x4000, 0, 0, 0, 1, 1, 1, 0, 21,
    ];
    words.into_iter().flat_map(u16::to_be_bytes).collect()
}

fn base_with_feature_variation_index() -> Vec<u8> {
    let varidx = Buf::new().u16(0).u16(1).u16(0x8000).done();
    let coord = link(
        Buf::new().u16(3).i16(123).u16(0).done(),
        vec![(4, 2, varidx)],
    );
    let min_max = link(
        Buf::new()
            .u16(0)
            .u16(0)
            .u16(1)
            .tag(b"kern")
            .u16(0)
            .u16(0)
            .done(),
        vec![(10, 2, coord)],
    );
    let script = link(
        Buf::new().u16(0).u16(0).u16(0).done(),
        vec![(2, 2, min_max)],
    );
    let scripts = link(
        Buf::new().u16(1).tag(b"DFLT").u16(0).done(),
        vec![(6, 2, script)],
    );
    let tags = Buf::new().u16(1).tag(b"romn").done();
    let axis = link(
        Buf::new().u16(0).u16(0).done(),
        vec![(0, 2, tags), (2, 2, scripts)],
    );
    let mut store = base_with_variation_index()[54..].to_vec();
    store[29] = 2;
    store.extend(34u16.to_be_bytes());
    link(
        Buf::new().u32(0x0001_0001).u16(0).u16(0).u32(0).done(),
        vec![(4, 2, axis), (8, 4, store)],
    )
}

fn base_with_language_variation_index() -> Vec<u8> {
    let varidx = Buf::new().u16(0).u16(0).u16(0x8000).done();
    let coord = link(
        Buf::new().u16(3).i16(123).u16(0).done(),
        vec![(4, 2, varidx)],
    );
    let min_max = link(Buf::new().u16(0).u16(0).u16(0).done(), vec![(0, 2, coord)]);
    let script = link(
        Buf::new().u16(0).u16(0).u16(1).tag(b"ENG ").u16(0).done(),
        vec![(10, 2, min_max)],
    );
    let scripts = link(
        Buf::new().u16(1).tag(b"DFLT").u16(0).done(),
        vec![(6, 2, script)],
    );
    let tags = Buf::new().u16(1).tag(b"romn").done();
    let axis = link(
        Buf::new().u16(0).u16(0).done(),
        vec![(0, 2, tags), (2, 2, scripts)],
    );
    link(
        Buf::new().u32(0x0001_0001).u16(0).u16(0).u32(0).done(),
        vec![
            (4, 2, axis),
            (8, 4, base_with_variation_index()[54..].to_vec()),
        ],
    )
}

#[test]
fn unpinned_base_store_with_variation_index_is_remapped() {
    let base = base_with_variation_index();
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", base.clone())]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &unicodes()).unwrap();
    let sfnt = Sfnt::new(&out).unwrap();
    let result = sofel_font::read::base::Base::new(sfnt.table(Tag::new(b"BASE")).unwrap()).unwrap();
    let script = result
        .horiz_axis()
        .unwrap()
        .unwrap()
        .base_script_list()
        .unwrap()
        .unwrap()
        .script(0)
        .unwrap()
        .unwrap();
    let coord = script
        .base_values()
        .unwrap()
        .unwrap()
        .coord(0)
        .unwrap()
        .unwrap();
    assert_eq!(
        coord.device().unwrap().unwrap().variation_index(),
        Some((0, 0))
    );
    assert_eq!(
        result
            .variation_store()
            .unwrap()
            .unwrap()
            .item_data(0)
            .unwrap()
            .delta(0, 0),
        Some(21)
    );

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
    let out = subset(&font, &unicodes()).unwrap();
    let sfnt = Sfnt::new(&out).unwrap();
    let result = sofel_font::read::base::Base::new(sfnt.table(Tag::new(b"BASE")).unwrap()).unwrap();
    let script = result
        .horiz_axis()
        .unwrap()
        .unwrap()
        .base_script_list()
        .unwrap()
        .unwrap()
        .script(0)
        .unwrap()
        .unwrap();
    let coord = script
        .default_min_max()
        .unwrap()
        .unwrap()
        .min_coord()
        .unwrap()
        .unwrap();
    assert_eq!(
        coord.device().unwrap().unwrap().variation_index(),
        Some((0, 0))
    );
    assert_eq!(
        result
            .variation_store()
            .unwrap()
            .unwrap()
            .item_data(0)
            .unwrap()
            .delta(0, 0),
        Some(21)
    );

    let mut vertical = base;
    vertical[5] = 0;
    vertical[7] = 12;
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", vertical)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &unicodes()).unwrap();
    let sfnt = Sfnt::new(&out).unwrap();
    let result = sofel_font::read::base::Base::new(sfnt.table(Tag::new(b"BASE")).unwrap()).unwrap();
    let script = result
        .vert_axis()
        .unwrap()
        .unwrap()
        .base_script_list()
        .unwrap()
        .unwrap()
        .script(0)
        .unwrap()
        .unwrap();
    let coord = script
        .base_values()
        .unwrap()
        .unwrap()
        .coord(0)
        .unwrap()
        .unwrap();
    assert_eq!(
        coord.device().unwrap().unwrap().variation_index(),
        Some((0, 0))
    );
    assert_eq!(
        result
            .variation_store()
            .unwrap()
            .unwrap()
            .item_data(0)
            .unwrap()
            .delta(0, 0),
        Some(21)
    );
}

#[test]
fn unpinned_base_store_compacts_second_item_to_first() {
    let mut base = base_with_variation_index();
    base[51] = 1; // BaseCoord's VariationIndex: old inner 1.
    base[83] = 2; // ItemVariationData has two rows.
    base.extend(34u16.to_be_bytes());
    let data = font_with(vec![(b"fvar", fvar(&AXES)), (b"BASE", base)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &unicodes()).unwrap();
    let sfnt = Sfnt::new(&out).unwrap();
    let base = sofel_font::read::base::Base::new(sfnt.table(Tag::new(b"BASE")).unwrap()).unwrap();
    let coord = base
        .horiz_axis()
        .unwrap()
        .unwrap()
        .base_script_list()
        .unwrap()
        .unwrap()
        .script(0)
        .unwrap()
        .unwrap()
        .base_values()
        .unwrap()
        .unwrap()
        .coord(0)
        .unwrap()
        .unwrap();
    assert_eq!(
        coord.device().unwrap().unwrap().variation_index(),
        Some((0, 0))
    );
    assert_eq!(
        base.variation_store()
            .unwrap()
            .unwrap()
            .item_data(0)
            .unwrap()
            .delta(0, 0),
        Some(34)
    );
}

#[test]
fn unpinned_base_store_with_filtered_script_has_no_retained_index() {
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"BASE", base_with_variation_index()),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = unicodes().map_set(SetKind::LayoutScripts, |set| set.clear());
    let out = subset(&font, &input).unwrap();
    assert!(Sfnt::new(&out).unwrap().table(Tag::new(b"BASE")).is_some());
}

#[test]
fn unpinned_base_store_feature_filter_removes_unreferenced_row() {
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"BASE", base_with_feature_variation_index()),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &unicodes()).unwrap();
    let base = sofel_font::read::base::Base::new(
        Sfnt::new(&out).unwrap().table(Tag::new(b"BASE")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        base.variation_store()
            .unwrap()
            .unwrap()
            .item_data(0)
            .unwrap()
            .delta(0, 0),
        Some(34)
    );
    let filtered = unicodes().map_set(SetKind::LayoutFeatures, |set| set.clear());
    let out = subset(&font, &filtered).unwrap();
    let base = sofel_font::read::base::Base::new(
        Sfnt::new(&out).unwrap().table(Tag::new(b"BASE")).unwrap(),
    )
    .unwrap();
    assert!(base.variation_store().unwrap().is_none());
}

#[test]
fn unpinned_base_store_collects_language_min_max() {
    let data = font_with(vec![
        (b"fvar", fvar(&AXES)),
        (b"BASE", base_with_language_variation_index()),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &unicodes()).unwrap();
    let base = sofel_font::read::base::Base::new(
        Sfnt::new(&out).unwrap().table(Tag::new(b"BASE")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        base.variation_store()
            .unwrap()
            .unwrap()
            .item_data(0)
            .unwrap()
            .delta(0, 0),
        Some(21)
    );
}
