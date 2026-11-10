//! The `fvar`, `avar` and `STAT` views and validators (ADR 0022).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_read::avar::Avar;
use sofel_read::fvar::Fvar;
use sofel_read::stat::{AxisValue, Stat};
use sofel_read::{Diagnostic, F2Dot14, Fixed, Kind, ReadError, Severity, Tag, avar, fvar, stat};

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}

fn fixed(v: f64) -> [u8; 4] {
    ((v * 65536.0) as i32).to_be_bytes()
}

/// An fvar with axes `(tag, min, default, max)` and instances
/// `(subfamily, coordinates, postscript)`.
fn fvar_table(
    axes: &[(&[u8; 4], f64, f64, f64)],
    instances: &[(u16, &[f64], Option<u16>)],
) -> Vec<u8> {
    let n = axes.len() as u16;
    let ps = instances.iter().any(|i| i.2.is_some());
    let size = n * 4 + 4 + if ps { 2 } else { 0 };
    let mut d = Vec::new();
    d.extend([0, 1, 0, 0]);
    d.extend(be16(16));
    d.extend(be16(2));
    d.extend(be16(n));
    d.extend(be16(20));
    d.extend(be16(instances.len() as u16));
    d.extend(be16(size));
    for (i, (tag, min, def, max)) in axes.iter().enumerate() {
        d.extend(*tag);
        d.extend(fixed(*min));
        d.extend(fixed(*def));
        d.extend(fixed(*max));
        d.extend(be16(0));
        d.extend(be16(256 + i as u16));
    }
    for (sub, coords, p) in instances {
        d.extend(be16(*sub));
        d.extend(be16(0));
        for c in *coords {
            d.extend(fixed(*c));
        }
        if ps {
            d.extend(be16(p.unwrap_or(0xFFFF)));
        }
    }
    d
}

fn diagnostics(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    f(&mut |d| out.push(d));
    out
}

#[test]
fn fvar_axes_and_instances() {
    let d = fvar_table(
        &[
            (b"wght", 100.0, 400.0, 900.0),
            (b"wdth", 75.0, 100.0, 100.0),
        ],
        &[(17, &[700.0, 100.0], Some(300)), (18, &[400.0, 75.0], None)],
    );
    let f = Fvar::new(&d).unwrap();
    assert_eq!(f.axis_count(), 2);
    let w = f.axis(0).unwrap();
    assert_eq!(w.tag, Tag::new(b"wght"));
    assert_eq!(w.default, Fixed(400 << 16));
    assert_eq!(w.name_id, 256);
    assert_eq!(f.find_axis(Tag::new(b"wdth")).unwrap().0, 1);
    assert!(f.find_axis(Tag::new(b"opsz")).is_none());
    assert_eq!(f.instance_count(), 2);
    let i = f.instance(0).unwrap();
    assert_eq!(i.subfamily_name_id, 17);
    assert_eq!(i.coordinate(0), Some(Fixed(700 << 16)));
    assert_eq!(i.postscript_name_id, Some(300));
    assert_eq!(f.instance(1).unwrap().postscript_name_id, Some(0xFFFF));
    assert!(f.instance(2).is_none());
    assert!(diagnostics(|r| fvar::validate(&d, r)).is_empty());
}

#[test]
fn fvar_orders_reversed_ranges() {
    let d = fvar_table(&[(b"wght", 500.0, 400.0, 300.0)], &[]);
    let a = Fvar::new(&d).unwrap().axis(0).unwrap();
    assert_eq!(
        a.ordered(),
        (Fixed(400 << 16), Fixed(400 << 16), Fixed(400 << 16))
    );
}

#[test]
fn fvar_rejects_what_harfbuzz_rejects() {
    let good = fvar_table(&[(b"wght", 100.0, 400.0, 900.0)], &[(17, &[700.0], None)]);
    let mut v2 = good.clone();
    v2[1] = 2;
    assert!(matches!(
        Fvar::new(&v2),
        Err(ReadError::UnknownVersion(0x0002_0000))
    ));
    let mut axis_size = good.clone();
    axis_size[11] = 24;
    assert_eq!(Fvar::new(&axis_size).err(), Some(ReadError::InvalidValue));
    let k = diagnostics(|r| fvar::validate(&axis_size, r));
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].offset, 10);
    let mut instance_size = good.clone();
    instance_size[15] = 6;
    assert_eq!(
        Fvar::new(&instance_size).err(),
        Some(ReadError::InvalidValue)
    );
    assert_eq!(
        diagnostics(|r| fvar::validate(&instance_size, r))[0].offset,
        14
    );
    let short = &good[..good.len() - 1];
    assert!(matches!(Fvar::new(short), Err(ReadError::Truncated { .. })));
    let k = diagnostics(|r| fvar::validate(short, r));
    assert!(matches!(k[0].kind, Kind::Truncated { .. }));
    assert_eq!(k[0].severity, Severity::Error);
}

fn avar_table(version: u16, maps: &[&[(f64, f64)]]) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend(be16(version));
    d.extend(be16(0));
    d.extend(be16(0));
    d.extend(be16(maps.len() as u16));
    for m in maps {
        d.extend(be16(m.len() as u16));
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

#[test]
fn avar_segment_maps() {
    let d = avar_table(
        1,
        &[&[(-1.0, -1.0), (0.0, 0.0), (0.5, 0.25), (1.0, 1.0)], &[]],
    );
    let a = Avar::new(&d).unwrap();
    assert_eq!(a.major_version(), 1);
    assert_eq!(a.axis_count(), 2);
    let m = a.segment_map(0).unwrap();
    assert_eq!(m.len(), 4);
    assert_eq!(m.pair(2), Some((F2Dot14(8192), F2Dot14(4096))));
    assert!(a.segment_map(1).unwrap().is_empty());
    assert!(a.segment_map(2).is_none());
    assert_eq!(a.v2_offsets(), None);
    assert!(diagnostics(|r| avar::validate(&d, Some(2), r)).is_empty());
    let k = diagnostics(|r| avar::validate(&d, Some(3), r));
    assert!(matches!(
        k[0].kind,
        Kind::AxisCountMismatch {
            found: 2,
            expected: 3
        }
    ));
}

#[test]
fn avar_version_2_and_errors() {
    let d = avar_table(2, &[&[(0.0, 0.0)]]);
    let a = Avar::new(&d).unwrap();
    assert_eq!(a.v2_offsets(), Some((0, 0)));
    assert!(matches!(
        Avar::new(&d[..d.len() - 1]),
        Err(ReadError::Truncated { .. })
    ));
    let mut v3 = d.clone();
    v3[1] = 3;
    assert!(matches!(Avar::new(&v3), Err(ReadError::UnknownVersion(_))));
    assert!(matches!(
        diagnostics(|r| avar::validate(&v3, None, r))[0].kind,
        Kind::UnknownVersion { found: 0x0003_0000 }
    ));
    let one = avar_table(1, &[&[(0.0, 0.0)]]);
    assert!(matches!(
        diagnostics(|r| avar::validate(&one[..one.len() - 2], None, r))[0].kind,
        Kind::Truncated { .. }
    ));
}

/// A STAT 1.1 with design axes and axis value tables (raw bytes).
fn stat_table(axes: &[(&[u8; 4], u16)], values: &[Vec<u8>]) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend([0, 1, 0, 1]);
    d.extend(be16(8));
    d.extend(be16(axes.len() as u16));
    d.extend(20u32.to_be_bytes());
    d.extend(be16(values.len() as u16));
    let values_at = 20 + axes.len() * 8;
    d.extend((values_at as u32).to_be_bytes());
    d.extend(be16(2));
    for (i, (tag, name)) in axes.iter().enumerate() {
        d.extend(*tag);
        d.extend(be16(*name));
        d.extend(be16(i as u16));
    }
    let mut at = values.len() * 2;
    let mut tables: Vec<u8> = Vec::new();
    for v in values {
        d.extend(be16(at as u16));
        at += v.len();
        tables.extend(v.iter());
    }
    d.extend(tables);
    d
}

fn format1(axis: u16, name: u16, value: f64) -> Vec<u8> {
    let mut v = vec![0, 1];
    v.extend(be16(axis));
    v.extend(be16(0));
    v.extend(be16(name));
    v.extend(fixed(value));
    v
}

#[test]
fn stat_axes_and_values() {
    let mut f4 = vec![0, 4];
    f4.extend(be16(2));
    f4.extend(be16(0));
    f4.extend(be16(260));
    f4.extend(be16(0));
    f4.extend(fixed(700.0));
    f4.extend(be16(1));
    f4.extend(fixed(100.0));
    let d = stat_table(
        &[(b"wght", 256), (b"wdth", 257)],
        &[format1(0, 258, 400.0), f4, vec![0, 9]],
    );
    let s = Stat::new(&d).unwrap();
    assert_eq!(s.design_axis_count(), 2);
    assert_eq!(s.design_axis(1).unwrap().tag, Tag::new(b"wdth"));
    assert_eq!(s.axis_value_count(), 3);
    let (v, bytes) = s.axis_value(0).unwrap();
    assert!(matches!(
        v,
        AxisValue::Format1 {
            axis_index: 0,
            name_id: 258,
            ..
        }
    ));
    assert_eq!(bytes.len(), 12);
    let (v, bytes) = s.axis_value(1).unwrap();
    assert_eq!(v.name_id(), Some(260));
    assert_eq!(bytes.len(), 20);
    assert_eq!(
        v.axis_values().collect::<Vec<_>>(),
        vec![(0, Fixed(700 << 16)), (1, Fixed(100 << 16))]
    );
    let (v, _) = s.axis_value(2).unwrap();
    assert_eq!(v.format(), 9);
    assert_eq!(v.name_id(), None);
    assert_eq!(s.elided_fallback_name_id(), 2);
    assert!(diagnostics(|r| stat::validate(&d, r)).is_empty());
}

#[test]
fn stat_rejects_what_harfbuzz_rejects() {
    let d = stat_table(&[(b"wght", 256)], &[format1(0, 258, 400.0)]);
    let mut v10 = d.clone();
    v10[3] = 0;
    assert!(matches!(
        Stat::new(&v10),
        Err(ReadError::UnknownVersion(0x0001_0000))
    ));
    // A value table cut short is skipped, and reported.
    let cut = &d[..d.len() - 2];
    let s = Stat::new(cut).unwrap();
    assert!(s.axis_value(0).is_none());
    let k = diagnostics(|r| stat::validate(cut, r));
    assert!(matches!(k[0].kind, Kind::OffsetOutOfBounds { .. }));
    assert!(matches!(
        Stat::new(&d[..19]),
        Err(ReadError::Truncated { .. })
    ));
}
