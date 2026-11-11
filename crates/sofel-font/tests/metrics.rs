//! Font-wide and vertical metrics (ADR 0016), on fonts built with the
//! ADR 0007 builders.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use std::cell::Cell;

use build::*;
use sofel_font::read::GlyphId;
use sofel_font::{Font, FontExtents, GlyphExtents, HbScale, MetricsTag};

fn font_with(replace: &[(&'static [u8; 4], Vec<u8>)], remove: &[&[u8; 4]]) -> Vec<u8> {
    let mut tables: Vec<_> = sample_tables()
        .into_iter()
        .filter(|(t, _)| !replace.iter().any(|(r, _)| r == t) && !remove.contains(t))
        .collect();
    tables.extend(replace.iter().cloned());
    tables.sort_by_key(|(t, _)| **t);
    sfnt(&tables)
}

fn head_upem(upem: u16) -> Vec<u8> {
    head(HeadSpec {
        units_per_em: upem,
        ..Default::default()
    })
}

fn ext(ascender: i32, descender: i32, line_gap: i32) -> FontExtents {
    FontExtents {
        ascender,
        descender,
        line_gap,
    }
}

fn no_extents(_: GlyphId) -> Option<GlyphExtents> {
    None
}

// ------------------------------------------------------------ h_extents

#[test]
fn h_extents_from_hhea_without_os2() {
    let data = font_with(&[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.default_instance().h_extents(), ext(800, -200, 90));
}

#[test]
fn h_extents_follow_use_typo_metrics() {
    let typo = os2(Os2Spec {
        fs_selection: 0x0080,
        ..Default::default()
    });
    let data = font_with(&[(b"OS/2", typo)], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().default_instance().h_extents(),
        ext(750, -250, 100)
    );

    let no_typo = os2(Os2Spec {
        fs_selection: 0x0040,
        ..Default::default()
    });
    let data = font_with(&[(b"OS/2", no_typo)], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().default_instance().h_extents(),
        ext(800, -200, 90)
    );
}

#[test]
fn h_extents_ignore_os2_without_data() {
    // OS2::has_data: weight class, width class and the character indices
    // all zero.
    let mut empty = os2(Os2Spec {
        fs_selection: 0x0080,
        weight_class: 0,
        first_char: 0,
        last_char: 0,
        ..Default::default()
    });
    empty[6..8].copy_from_slice(&[0, 0]); // usWidthClass
    let data = font_with(&[(b"OS/2", empty)], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().default_instance().h_extents(),
        ext(800, -200, 90)
    );
}

#[test]
fn h_extents_fall_back_without_hhea() {
    let data = font_with(&[(b"head", head_upem(2048))], &[b"hhea"]);
    let f = Font::new(&data, 0).unwrap();
    // 2048 * 0.8 truncated, then minus 1 em.
    assert_eq!(f.default_instance().h_extents(), ext(1638, -410, 0));
    // With OS/2 USE_TYPO_METRICS, hhea is not needed.
    let typo = os2(Os2Spec {
        fs_selection: 0x0080,
        ..Default::default()
    });
    let data = font_with(&[(b"OS/2", typo)], &[b"hhea"]);
    assert_eq!(
        Font::new(&data, 0).unwrap().default_instance().h_extents(),
        ext(750, -250, 100)
    );
}

#[test]
fn h_extents_fix_the_signs() {
    let mut flipped = hhea(20);
    flipped[4..6].copy_from_slice(&(-800i16).to_be_bytes());
    flipped[6..8].copy_from_slice(&200i16.to_be_bytes());
    flipped[8..10].copy_from_slice(&(-90i16).to_be_bytes());
    let data = font_with(&[(b"hhea", flipped)], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().default_instance().h_extents(),
        ext(800, -200, -90)
    );
}

// ------------------------------------------------- v_extents, v_advance

#[test]
fn v_extents_from_vhea_or_fallback() {
    let data = font_with(&[(b"vhea", hhea(20))], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().default_instance().v_extents(),
        ext(800, -200, 90)
    );
    let data = font_with(&[], &[]);
    assert_eq!(
        Font::new(&data, 0).unwrap().default_instance().v_extents(),
        ext(500, -500, 0)
    );
}

fn vertical_tables() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let long: Vec<(u16, i16)> = (0..3).map(|g| (1000 + g as u16, 50 + g as i16)).collect();
    vec![(b"vhea", hhea(3)), (b"vmtx", hmtx(&long, &[60; 25]))]
}

#[test]
fn v_advance_from_vmtx() {
    let data = font_with(&vertical_tables(), &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.default_instance().v_advance(GlyphId(0)), -1000);
    assert_eq!(f.default_instance().v_advance(GlyphId(2)), -1002);
    assert_eq!(f.default_instance().v_advance(GlyphId(10)), -1002);
    assert_eq!(f.default_instance().v_advance(GlyphId(28)), 0);
}

#[test]
fn v_advance_without_vmtx_is_the_font_height() {
    let data = font_with(&[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.default_instance().v_advance(GlyphId(1)), -1000);
    let data = font_with(&[(b"head", head_upem(2048))], &[b"hhea"]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.default_instance().v_advance(GlyphId(1)), -2048);
}

// ------------------------------------------------------------- v_origin

/// Glyphs 0..=27 as in `sample_tables`, with glyf data: 1 a simple glyph
/// with xMin 100 and yMax 700, 2 a simple glyph with yMax 300, 3 a
/// composite of 1 and 2 with USE_MY_METRICS on 2, 4 the same without
/// the flag, and the rest empty.
fn glyf_tables() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    use Pt::On;
    let a = simple_glyph(&[&[On(100, 0), On(100, 700), On(500, 0)]], false, &[]);
    let b = simple_glyph(&[&[On(0, -100), On(0, 300), On(200, 0)]], false, &[]);
    let mut c_comp = comp(2, Args::Xy(0, 0));
    c_comp.flags |= USE_MY_METRICS;
    let c = composite_glyph(
        &[comp(1, Args::Xy(0, 0)), c_comp],
        [0, -100, 500, 900],
        false,
    );
    let d = composite_glyph(
        &[comp(1, Args::Xy(0, 0)), comp(2, Args::Xy(0, 0))],
        [0, -100, 500, 900],
        false,
    );
    let mut glyphs = vec![Vec::new(), a, b, c, d];
    glyphs.resize(28, Vec::new());
    let (glyf, loca) = glyf_loca(&glyphs, false);
    vec![(b"glyf", glyf), (b"loca", loca)]
}

#[test]
fn v_origin_from_vorg() {
    let mut tables = vertical_tables();
    tables.extend(glyf_tables());
    tables.push((b"VORG", vorg(1, 880, &[(1, 900)])));
    let data = font_with(&tables, &[]);
    let f = Font::new(&data, 0).unwrap();
    // x is half the horizontal advance (501 / 2).
    assert_eq!(
        f.default_instance().v_origin(GlyphId(1), no_extents),
        (250, 900)
    );
    assert_eq!(
        f.default_instance().v_origin(GlyphId(2), no_extents),
        (251, 880)
    );
}

#[test]
fn v_origin_from_the_top_phantom_point() {
    let mut tables = vertical_tables();
    tables.extend(glyf_tables());
    let data = font_with(&tables, &[]);
    let f = Font::new(&data, 0).unwrap();
    // yMax + tsb.
    assert_eq!(
        f.default_instance().v_origin(GlyphId(1), no_extents).1,
        700 + 51
    );
    assert_eq!(
        f.default_instance().v_origin(GlyphId(2), no_extents).1,
        300 + 52
    );
    // A composite with USE_MY_METRICS takes its component's phantoms.
    assert_eq!(
        f.default_instance().v_origin(GlyphId(3), no_extents).1,
        300 + 52
    );
    // Without the flag, its own header and bearing.
    assert_eq!(
        f.default_instance().v_origin(GlyphId(4), no_extents).1,
        900 + 60
    );
    // An empty glyph has an all-zero header.
    assert_eq!(f.default_instance().v_origin(GlyphId(0), no_extents).1, 50);
    // Past the glyphs.
    assert_eq!(f.default_instance().v_origin(GlyphId(40), no_extents).1, 0);
}

#[test]
fn v_origin_from_the_box_without_vmtx() {
    let data = font_with(&glyf_tables(), &[]);
    let f = Font::new(&data, 0).unwrap();
    // Font height 1000; the box's top plus half of (height + box height).
    let e = |_| {
        Some(GlyphExtents {
            x_bearing: 0,
            y_bearing: 700,
            width: 400,
            height: -701,
        })
    };
    assert_eq!(
        f.default_instance().v_origin(GlyphId(1), e),
        (250, 700 + ((1000 - 701) >> 1))
    );
    // Without a box, the ascender.
    assert_eq!(
        f.default_instance().v_origin(GlyphId(1), no_extents),
        (250, 800)
    );
    // With the glyf boxes.
    assert_eq!(
        f.default_instance()
            .v_origin(GlyphId(1), |g| f.default_instance().glyf_extents(g)),
        (250, 700 + ((1000 - 700) >> 1))
    );
}

#[test]
fn v_origin_asks_for_boxes_only_as_the_last_resort() {
    let calls = Cell::new(0);
    let counting = |_| {
        calls.set(calls.get() + 1);
        None
    };
    let mut tables = vertical_tables();
    tables.extend(glyf_tables());
    for extra in [vec![(b"VORG", vorg(1, 0, &[]))], vec![]] {
        let mut t = tables.clone();
        t.extend(extra);
        let data = font_with(&t, &[]);
        Font::new(&data, 0)
            .unwrap()
            .default_instance()
            .v_origin(GlyphId(1), counting);
        assert_eq!(calls.get(), 0);
    }
    let data = font_with(&glyf_tables(), &[]);
    Font::new(&data, 0)
        .unwrap()
        .default_instance()
        .v_origin(GlyphId(1), counting);
    assert_eq!(calls.get(), 1);
}

// --------------------------------------------------------- glyf_extents

#[test]
fn glyf_extents_use_the_side_bearing() {
    let data = font_with(&glyf_tables(), &[]);
    let f = Font::new(&data, 0).unwrap();
    // hmtx gives glyph 1 the bearing 1, not its xMin 100.
    assert_eq!(
        f.default_instance().glyf_extents(GlyphId(1)),
        Some(GlyphExtents {
            x_bearing: 1,
            y_bearing: 700,
            width: 400,
            height: -700,
        })
    );
    assert_eq!(
        f.default_instance().glyf_extents(GlyphId(0)),
        Some(GlyphExtents::default())
    );
    assert_eq!(f.default_instance().glyf_extents(GlyphId(28)), None);
    let data = font_with(&[], &[]);
    assert_eq!(
        Font::new(&data, 0)
            .unwrap()
            .default_instance()
            .glyf_extents(GlyphId(1)),
        None
    );
}

// --------------------------------------------------------------- fs_type

#[test]
fn fs_type_like_freetype() {
    let data = font_with(&[(b"OS/2", os2(Os2Spec::default()))], &[]);
    assert_eq!(Font::new(&data, 0).unwrap().fs_type(), 8);
    let data = font_with(&[], &[]);
    assert_eq!(Font::new(&data, 0).unwrap().fs_type(), 0);
    let marked = os2(Os2Spec {
        version: 0xFFFF,
        ..Default::default()
    });
    let data = font_with(&[(b"OS/2", marked)], &[]);
    assert_eq!(Font::new(&data, 0).unwrap().fs_type(), 0);
    let mut short = os2(Os2Spec::default());
    short.truncate(80);
    let data = font_with(&[(b"OS/2", short)], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.fs_type(), 0);
    assert!(f.os2().is_none());
}

// ------------------------------------------------------ metrics_position

#[test]
fn metrics_positions() {
    let data = font_with(
        &[
            (b"OS/2", os2(Os2Spec::default())),
            (b"post", post_header(0x0003_0000).done()),
        ],
        &[],
    );
    let f = Font::new(&data, 0).unwrap();
    let p = |t| f.default_instance().metrics_position(t);
    assert_eq!(p(MetricsTag::HorizontalAscender), Some(800));
    assert_eq!(p(MetricsTag::HorizontalClippingAscent), Some(900));
    assert_eq!(p(MetricsTag::HorizontalClippingDescent), Some(300));
    assert_eq!(p(MetricsTag::XHeight), Some(500));
    assert_eq!(p(MetricsTag::CapHeight), Some(700));
    assert_eq!(p(MetricsTag::HorizontalCaretRise), Some(1));
    assert_eq!(p(MetricsTag::HorizontalCaretOffset), Some(5));
    assert_eq!(p(MetricsTag::SuperscriptEmYOffset), Some(350));
    assert_eq!(p(MetricsTag::StrikeoutSize), Some(50));
    assert_eq!(p(MetricsTag::UnderlineSize), Some(50));
    assert_eq!(p(MetricsTag::UnderlineOffset), Some(-75));
    assert_eq!(p(MetricsTag::VerticalAscender), None);
    assert_eq!(p(MetricsTag::VerticalCaretRise), None);
}

#[test]
fn metrics_positions_need_their_tables() {
    let v1 = os2(Os2Spec {
        version: 1,
        ..Default::default()
    });
    // post 2.5 does not pass HarfBuzz's check.
    let post25 = post_header(0x0002_5000).u16(0).done();
    let data = font_with(&[(b"OS/2", v1), (b"post", post25)], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(
        f.default_instance().metrics_position(MetricsTag::XHeight),
        None
    );
    assert_eq!(
        f.default_instance()
            .metrics_position(MetricsTag::UnderlineSize),
        None
    );
    assert_eq!(
        f.default_instance()
            .metrics_position(MetricsTag::StrikeoutOffset),
        Some(260)
    );
    // OS/2 version 2 with both heights 0 has no x-height.
    let zero = os2(Os2Spec {
        x_height: 0,
        cap_height: 0,
        ..Default::default()
    });
    let data = font_with(&[(b"OS/2", zero)], &[]);
    assert_eq!(
        Font::new(&data, 0)
            .unwrap()
            .default_instance()
            .metrics_position(MetricsTag::XHeight),
        None
    );
}

#[test]
fn metrics_position_fallbacks() {
    let data = font_with(&glyf_tables(), &[]);
    let f = Font::new(&data, 0).unwrap();
    let q = |t| {
        f.default_instance()
            .metrics_position_with_fallback(t, |g| f.default_instance().glyf_extents(g))
    };
    assert_eq!(q(MetricsTag::HorizontalAscender), 800);
    assert_eq!(q(MetricsTag::HorizontalClippingAscent), 800);
    assert_eq!(q(MetricsTag::HorizontalClippingDescent), -200);
    assert_eq!(q(MetricsTag::VerticalAscender), 500);
    assert_eq!(q(MetricsTag::VerticalCaretRise), 1);
    assert_eq!(q(MetricsTag::VerticalCaretRun), 0);
    // No 'x' in the cmap: half an em. 'O' maps to glyph 15 (empty box).
    assert_eq!(q(MetricsTag::XHeight), 500);
    assert_eq!(q(MetricsTag::CapHeight), 0);
    assert_eq!(q(MetricsTag::StrikeoutSize), 1000 / 18);
    assert_eq!(q(MetricsTag::StrikeoutOffset), 400);
    assert_eq!(q(MetricsTag::UnderlineSize), 1000 / 18);
    assert_eq!(q(MetricsTag::UnderlineOffset), -1000 / 18);
    assert_eq!(q(MetricsTag::SubscriptEmXSize), 833);
    assert_eq!(q(MetricsTag::SubscriptEmYOffset), 200);
    assert_eq!(q(MetricsTag::SuperscriptEmXOffset), 0);

    // A zero underline thickness falls back too.
    let mut zero = post_header(0x0003_0000).done();
    zero[10..12].copy_from_slice(&[0, 0]);
    let data = font_with(&[(b"post", zero)], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(
        f.default_instance()
            .metrics_position(MetricsTag::UnderlineSize),
        Some(0)
    );
    assert_eq!(
        f.default_instance()
            .metrics_position_with_fallback(MetricsTag::UnderlineSize, |_| None),
        55
    );
}

#[test]
fn metrics_tags_round_trip() {
    for t in MetricsTag::ALL {
        assert_eq!(MetricsTag::from_tag(t.tag()), Some(t));
    }
    assert_eq!(MetricsTag::XHeight.tag().0, *b"xhgt");
    assert_eq!(
        MetricsTag::from_tag(sofel_font::read::Tag::new(b"none")),
        None
    );
}

// ---------------------------------------------------------------- scale

#[test]
fn scale_multipliers_like_harfbuzz() {
    let s = HbScale::new(768, -1000, 1000);
    assert_eq!((s.x_scale(), s.y_scale()), (768, -1000));
    // em_mult: x_mult = 50331 (768 * 65536 / 1000 in f32, truncated).
    assert_eq!(s.em_scale_x(500), 384);
    assert_eq!(s.em_scale_x(1000), 768);
    assert_eq!(s.em_scale_y(3), -3);
    assert_eq!(s.em_scale_y(-3), 3);
    // em_multf: roundf(v * x_multf).
    assert_eq!(s.em_scalef_x(750.0), 576);
    assert_eq!(s.em_scalef_y(750.0), -750);
    let unit = HbScale::new(1000, 1000, 1000);
    for v in [-32768, -1, 0, 1, 999, 32767] {
        assert_eq!(unit.em_scale_x(v), v);
        assert_eq!(unit.em_scalef_y(v as f32), v);
    }
}

#[test]
fn scaled_glyph_extents_round_outward() {
    let s = HbScale::new(768, 768, 1000);
    let e = GlyphExtents {
        x_bearing: 30,
        y_bearing: 700,
        width: 400,
        height: -700,
    };
    assert_eq!(
        s.scale_glyph_extents(e),
        GlyphExtents {
            x_bearing: 23,
            y_bearing: 537,
            width: 308,
            height: -537,
        }
    );
    let unit = HbScale::new(2048, 2048, 2048);
    assert_eq!(unit.scale_glyph_extents(e), e);
}

#[test]
fn scaled_queries() {
    let mut tables = vertical_tables();
    tables.extend(glyf_tables());
    let data = font_with(&tables, &[]);
    let f = Font::new(&data, 0).unwrap();
    let s = HbScale::for_font(&f, 2000, 500);
    assert_eq!(s.h_extents(&f), ext(400, -100, 45));
    assert_eq!(s.v_extents(&f), ext(1600, -400, 180));
    assert_eq!(s.v_advance(&f, GlyphId(0)), -500);
    assert_eq!(s.h_advance(&f, GlyphId(1)), 1002);
    assert_eq!(s.v_origin(&f, GlyphId(1), no_extents), (501, 376));
    assert_eq!(
        s.glyf_extents(&f, GlyphId(1)),
        Some(GlyphExtents {
            x_bearing: 2,
            y_bearing: 350,
            width: 800,
            height: -350,
        })
    );
    // The fallback is computed at the scale, not scaled from font units.
    let data = font_with(&[], &[b"hhea"]);
    let f = Font::new(&data, 0).unwrap();
    let s = HbScale::for_font(&f, 768, 768);
    assert_eq!(s.h_extents(&f), ext(614, -154, 0));
    assert_eq!(s.v_extents(&f), ext(384, -384, 0));
    assert_eq!(
        s.metrics_position_with_fallback(&f, MetricsTag::CapHeight, |_| None),
        512
    );
}
