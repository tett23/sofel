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

use sofel_read::glyf::{Component, Glyph, GlyphPoint};
use sofel_read::loca::Loca;
use sofel_read::sfnt::Sfnt;
use sofel_read::{Diagnostic, GlyphId, ReadError, Severity, Tag, glyf, loca, validate};

fn collect(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    f(&mut |d| out.push(d));
    out
}

fn ids(ds: &[Diagnostic]) -> Vec<(&'static str, Severity, u32)> {
    ds.iter()
        .map(|d| (d.kind.id(), d.severity, d.offset))
        .collect()
}

#[test]
fn loca_formats() {
    let short = Buf::new().u16(0).u16(5).u16(9).done();
    let l = Loca::new(&short, 0).unwrap();
    assert_eq!(l.num_locations(), 3);
    assert_eq!(
        (l.offset(0), l.offset(1), l.offset(2), l.offset(3)),
        (Some(0), Some(10), Some(18), None)
    );
    let long = Buf::new().u32(0).u32(7).u32(0x1_0000_0).done();
    let l = Loca::new(&long, 1).unwrap();
    assert_eq!(l.num_locations(), 3);
    assert_eq!(l.offset(2), Some(0x10_0000));
    // A trailing partial entry is not counted.
    assert_eq!(Loca::new(&long[..11], 1).unwrap().num_locations(), 2);
    assert!(matches!(
        Loca::new(&short, 2),
        Err(ReadError::UnknownFormat(2))
    ));
}

fn square() -> Vec<u8> {
    simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 700),
            Pt::On(500, 700),
            Pt::On(500, 0),
        ]],
        false,
        &[],
    )
}

fn points(g: &Glyph<'_>, extended: bool) -> Vec<GlyphPoint> {
    g.simple().unwrap().points(extended).collect()
}

#[test]
fn simple_glyph_decoding() {
    let contour: &[Pt] = &[
        Pt::On(10, -20),      // short +x, short -y
        Pt::Off(10, 300),     // same x, long y
        Pt::Cubic(-990, 300), // long -x, same y
        Pt::On(-1000, 299),
    ];
    let data = simple_glyph(&[contour, &[Pt::On(5, 5)]], true, &[0xB0, 0x01]);
    let g = Glyph::new(&data).unwrap();
    assert_eq!(g.number_of_contours(), 2);
    assert_eq!(
        (g.x_min(), g.y_min(), g.x_max(), g.y_max()),
        (-1000, -20, 10, 300)
    );
    let s = g.simple().unwrap();
    assert_eq!(s.num_points(), 5);
    assert_eq!(s.end_points().collect::<Vec<_>>(), vec![3, 4]);
    assert_eq!(s.instructions(), &[0xB0, 0x01]);
    assert!(s.overlap());
    let p = points(&g, false);
    let xy: Vec<(i32, i32)> = p.iter().map(|p| (p.x, p.y)).collect();
    assert_eq!(
        xy,
        vec![(10, -20), (10, 300), (-990, 300), (-1000, 299), (5, 5)]
    );
    let on: Vec<bool> = p.iter().map(|p| p.on_curve).collect();
    assert_eq!(on, vec![true, false, false, true, true]);
    // The cubic flag counts only in extended (GLYF) fonts.
    assert!(p.iter().all(|p| !p.cubic));
    let p = points(&g, true);
    assert_eq!(
        p.iter().map(|p| p.cubic).collect::<Vec<_>>(),
        vec![false, false, true, false, false]
    );
}

#[test]
fn repeated_flags() {
    let contour: Vec<Pt> = (0..40).map(|i| Pt::On(i * 3, 0)).collect();
    let data = simple_glyph(&[&contour], false, &[]);
    let g = Glyph::new(&data).unwrap();
    let xs: Vec<i32> = points(&g, false).iter().map(|p| p.x).collect();
    assert_eq!(xs, (0..40).map(|i| i * 3).collect::<Vec<_>>());
}

#[test]
fn broken_simple_glyphs() {
    let data = square();
    assert!(matches!(
        Glyph::new(&data[..9]),
        Err(ReadError::Truncated { .. })
    ));
    // Coordinates cut short.
    let g = Glyph::new(&data[..data.len() - 1]).unwrap();
    assert!(g.simple().is_err());
    // End points not increasing.
    let two = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(1, 1)], &[Pt::On(2, 2)]],
        false,
        &[],
    );
    let mut bad = two.clone();
    bad[12..14].copy_from_slice(&1u16.to_be_bytes()); // second end point = first
    assert!(matches!(
        Glyph::new(&bad).unwrap().simple(),
        Err(ReadError::InvalidValue)
    ));
    // A repeat count past the last point.
    let mut bad = data.clone();
    let flags_at = 10 + 2 + 2; // header, one end point, instruction length
    bad[flags_at] |= REPEAT;
    bad.insert(flags_at + 1, 200);
    assert!(matches!(
        Glyph::new(&bad).unwrap().simple(),
        Err(ReadError::InvalidValue)
    ));
    // 0xFFF contours or more.
    let mut bad = data.clone();
    bad[0..2].copy_from_slice(&0x0FFFu16.to_be_bytes());
    assert!(Glyph::new(&bad).unwrap().simple().is_err());
}

fn components(data: &[u8], extended: bool) -> Vec<Component> {
    Glyph::new(data)
        .unwrap()
        .components(extended)
        .map(|c| c.unwrap())
        .collect()
}

#[test]
fn composite_records() {
    let comps = [
        comp(1, Args::Xy(-5, 7)),
        comp(2, Args::Xy(300, -400)),
        Comp {
            glyph: 3,
            args: Args::Points(1, 2),
            xf: Xf::Scale(0x2000),
            flags: SCALED_COMPONENT_OFFSET,
        },
        Comp {
            glyph: 4,
            args: Args::Points(300, 2),
            xf: Xf::XyScale(0x4000, -0x4000),
            flags: 0,
        },
        Comp {
            glyph: 5,
            args: Args::Xy(1, 1),
            xf: Xf::TwoByTwo(1, 2, 3, 4),
            flags: USE_MY_METRICS,
        },
    ];
    let data = composite_glyph(&comps, [0, 0, 10, 10], false);
    let g = Glyph::new(&data).unwrap();
    assert_eq!(g.number_of_contours(), -1);
    let c = components(&data, false);
    assert_eq!(c.len(), 5);
    assert_eq!(
        (c[0].glyph, c[0].arg1, c[0].arg2, c[0].args_are_xy()),
        (GlyphId(1), -5, 7, true)
    );
    assert_eq!((c[1].arg1, c[1].arg2), (300, -400));
    assert_eq!((c[2].arg1, c[2].arg2, c[2].args_are_xy()), (1, 2, false));
    assert_eq!((c[2].xx, c[2].yx, c[2].xy, c[2].yy), (0x2000, 0, 0, 0x2000));
    assert!(c[2].has_scale() && c[2].flags & SCALED_COMPONENT_OFFSET != 0);
    assert_eq!((c[3].arg1, c[3].arg2), (300, 2));
    assert_eq!((c[3].xx, c[3].yy), (0x4000, -0x4000));
    assert_eq!((c[4].xx, c[4].yx, c[4].xy, c[4].yy), (1, 2, 3, 4));
    assert!(!c[0].has_scale());
    assert_eq!((c[0].xx, c[0].yy), (0x4000, 0x4000));
}

#[test]
fn wide_component_ids_need_extended_fonts() {
    let data = composite_glyph(&[comp(0x1_2345, Args::Xy(0, 0))], [0; 4], true);
    assert_eq!(components(&data, true)[0].glyph, GlyphId(0x1_2345));
    // Without GLYF the flag is ignored and a uint16 is read.
    assert_eq!(components(&data, false)[0].glyph, GlyphId(0x0123));
}

#[test]
fn truncated_components() {
    let data = composite_glyph(
        &[comp(1, Args::Xy(0, 0)), comp(2, Args::Xy(300, 300))],
        [0; 4],
        false,
    );
    let g = Glyph::new(&data[..data.len() - 1]).unwrap();
    let all: Vec<_> = g.components(false).collect();
    assert!(all[0].is_ok());
    assert!(all.last().unwrap().is_err());
}

#[test]
fn valid_glyph_fonts_report_nothing() {
    for (long, extended) in [(false, false), (true, false), (false, true), (true, true)] {
        let font = glyph_font(
            &[
                Vec::new(),
                square(),
                composite_glyph(&[comp(1, Args::Xy(10, 0))], [10, 0, 510, 700], extended),
            ],
            long,
            extended,
        );
        let s = Sfnt::new(&font).unwrap();
        let ds = collect(|r| validate::sfnt(&s, r));
        assert!(ds.is_empty(), "{long} {extended}: {ds:?}");
    }
}

fn glyf_diagnostics(
    glyphs: &[Vec<u8>],
    patch: impl Fn(&mut Vec<u8>, &mut Vec<u8>),
) -> Vec<(&'static str, Severity, u32)> {
    let (mut g, mut l) = glyf_loca(glyphs, true);
    patch(&mut g, &mut l);
    let mut out = collect(|r| loca::validate(&l, 1, glyphs.len() as u32, g.len(), loca::TAG, r));
    if let Ok(lv) = Loca::new(&l, 1) {
        out.extend(collect(|r| {
            glyf::validate(&g, &lv, glyphs.len() as u32, false, glyf::TAG, r)
        }));
    }
    ids(&out)
}

#[test]
fn loca_diagnostics() {
    let glyphs = [square(), square()];
    let d = glyf_diagnostics(&glyphs, |_, l| {
        l.truncate(8);
    });
    assert!(d.contains(&("truncated", Severity::Error, 8)), "{d:?}");
    // The last entry (4) is below the one before it (the length of a square).
    let d = glyf_diagnostics(&glyphs, |_, l| {
        l[8..12].copy_from_slice(&4u32.to_be_bytes())
    });
    assert!(
        d.contains(&("loca_not_sorted", Severity::Error, 8)),
        "{d:?}"
    );
    let d = glyf_diagnostics(&glyphs, |_, l| {
        l[8..12].copy_from_slice(&0xFFFFu32.to_be_bytes())
    });
    assert!(
        d.contains(&("offset_out_of_bounds", Severity::Error, 8)),
        "{d:?}"
    );
    assert_eq!(
        ids(&collect(|r| loca::validate(&[0; 4], 3, 1, 0, loca::TAG, r))),
        vec![("invalid_index_to_loc_format", Severity::Error, 0)]
    );
}

#[test]
fn glyph_diagnostics() {
    let two = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(1, 1)], &[Pt::On(2, 2)]],
        false,
        &[],
    );
    let d = glyf_diagnostics(&[two], |g, _| {
        g[12..14].copy_from_slice(&1u16.to_be_bytes())
    });
    assert_eq!(d, vec![("end_points_not_sorted", Severity::Error, 12)]);

    let d = glyf_diagnostics(&[square()], |g, _| {
        g[14] |= REPEAT;
        g.insert(15, 200);
    });
    assert_eq!(d, vec![("flag_repeat_overrun", Severity::Error, 14)]);

    let d = glyf_diagnostics(&[square()], |g, _| {
        g[0..2].copy_from_slice(&0x0FFFu16.to_be_bytes())
    });
    assert_eq!(d, vec![("too_many_contours", Severity::Error, 0)]);

    let d = glyf_diagnostics(&[square(), square()], |g, l| {
        g.truncate(g.len() - 1);
        let n = g.len() as u32;
        l[8..12].copy_from_slice(&n.to_be_bytes());
    });
    assert!(
        d.iter()
            .any(|(id, s, _)| *id == "truncated" && *s == Severity::Error),
        "{d:?}"
    );

    let c = composite_glyph(
        &[comp(0, Args::Xy(0, 0)), comp(9, Args::Xy(0, 0))],
        [0; 4],
        false,
    );
    let d = glyf_diagnostics(&[c], |_, _| {});
    assert_eq!(
        d,
        vec![("component_glyph_out_of_range", Severity::Error, 16)]
    );
}

#[test]
fn extended_tables_are_labelled() {
    let font = glyph_font(&[square()], false, true);
    let mut bad = font.clone();
    // Find LOCA and break its second entry.
    let s = Sfnt::new(&font).unwrap();
    let r = s.record(Tag::new(b"LOCA")).unwrap();
    let at = r.offset as usize + 2;
    bad[at..at + 2].copy_from_slice(&0xFFFFu16.to_be_bytes());
    let s = Sfnt::new(&bad).unwrap();
    let ds = collect(|r| validate::sfnt(&s, r));
    assert!(
        ds.iter()
            .any(|d| d.table == Tag::new(b"LOCA") && d.severity == Severity::Error),
        "{ds:?}"
    );
}
