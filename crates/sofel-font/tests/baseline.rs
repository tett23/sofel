//! Baselines from BASE and their synthesized fallbacks, and the BASE
//! MinMax font extents (ADR 0017), on fonts built with the ADR 0007
//! builders. The values follow HarfBuzz 15.1.0's
//! `hb_ot_layout_get_baseline`, `hb_ot_layout_get_baseline_with_fallback`
//! and `hb_ot_layout_get_font_extents`.

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

use build::*;
use sofel_font::read::{GlyphId, Tag};
use sofel_font::{Direction, Font, FontExtents, GlyphExtents, HbScale, baseline};

const H: Direction = Direction::Horizontal;
const V: Direction = Direction::Vertical;

fn t(s: &[u8; 4]) -> Tag {
    Tag::new(s)
}

/// `sample_tables` with BASE, a cmap mapping `chars` (code point,
/// glyph) and the given tables replaced or removed.
fn font_with(
    base_table: Option<Vec<u8>>,
    chars: &[(u32, u32)],
    replace: &[(&'static [u8; 4], Vec<u8>)],
    remove: &[&[u8; 4]],
) -> Vec<u8> {
    let mut groups: Vec<(u32, u32, u32)> = chars.iter().map(|&(c, g)| (c, c, g)).collect();
    groups.sort();
    let mut tables: Vec<_> = sample_tables()
        .into_iter()
        .filter(|(t, _)| *t != b"cmap")
        .filter(|(t, _)| !replace.iter().any(|(r, _)| r == t) && !remove.contains(t))
        .collect();
    tables.push((b"cmap", cmap(&[(3, 10, cmap12_13(12, &groups))])));
    if let Some(b) = base_table {
        tables.push((b"BASE", b));
    }
    tables.extend(replace.iter().cloned());
    tables.sort_by_key(|(t, _)| **t);
    sfnt(&tables)
}

fn boxes(map: &[(u32, GlyphExtents)]) -> impl Fn(GlyphId) -> Option<GlyphExtents> + '_ {
    move |g| map.iter().find(|(x, _)| *x == g.0).map(|(_, e)| *e)
}

fn no_boxes(_: GlyphId) -> Option<GlyphExtents> {
    None
}

fn bx(y_bearing: i32, height: i32) -> GlyphExtents {
    GlyphExtents {
        x_bearing: 0,
        y_bearing,
        width: 100,
        height,
    }
}

fn ext(ascender: i32, descender: i32, line_gap: i32) -> FontExtents {
    FontExtents {
        ascender,
        descender,
        line_gap,
    }
}

// --------------------------------------------------------------- lookup

#[test]
fn baseline_tags_are_harfbuzz_values() {
    let tags: Vec<_> = baseline::ALL.iter().map(|t| t.0).collect();
    assert_eq!(
        tags,
        vec![
            *b"romn", *b"hang", *b"icfb", *b"icft", *b"Icfc", *b"ideo", *b"idtp", *b"Idce",
            *b"math"
        ]
    );
    assert_eq!(baseline::IDEO_EMBOX_CENTRAL, t(b"Idce"));
}

fn lookup_font() -> Vec<u8> {
    let h = AxisSpec {
        tags: Some(vec![*b"hang", *b"ideo", *b"idtp", *b"romn"]),
        scripts: Some(vec![
            (
                *b"DFLT",
                Some(ScriptSpec {
                    values: Some((
                        3,
                        vec![None, Some(Coord::F1(-110)), None, Some(Coord::F1(0))],
                    )),
                    ..Default::default()
                }),
            ),
            (
                *b"deva",
                Some(ScriptSpec {
                    values: Some((
                        3,
                        vec![
                            Some(Coord::F2(600, 3, 2)),
                            Some(Coord::F3(-120, Some(device(8, 20, 1, &[0xFFFF, 0xFFFF])))),
                            Some(Coord::F4(880, 70_000, 1)),
                        ],
                    )),
                    ..Default::default()
                }),
            ),
            (
                *b"hani",
                Some(ScriptSpec {
                    default_min_max: Some(MinMaxSpec::default()),
                    ..Default::default()
                }),
            ),
            (*b"kana", None),
        ]),
    };
    let v = simple_axis(b"DFLT", &[(*b"ideo", -500), (*b"idtp", 500)]);
    font_with(Some(base(Some(&h), Some(&v))), &[], &[], &[])
}

#[test]
fn baseline_lookup_by_script_with_dflt_fallback() {
    let data = lookup_font();
    let f = Font::new(&data, 0).unwrap();
    // deva: formats 2, 3 and 4 use the coordinate only.
    assert_eq!(f.baseline(baseline::HANGING, H, t(b"deva")), Some(600));
    assert_eq!(
        f.baseline(baseline::IDEO_EMBOX_BOTTOM_OR_LEFT, H, t(b"deva")),
        Some(-120)
    );
    assert_eq!(
        f.baseline(baseline::IDEO_EMBOX_TOP_OR_RIGHT, H, t(b"deva")),
        Some(880)
    );
    // Past deva's three coordinates.
    assert_eq!(f.baseline(baseline::ROMAN, H, t(b"deva")), None);
    // latn is not there: DFLT.
    assert_eq!(
        f.baseline(baseline::IDEO_EMBOX_BOTTOM_OR_LEFT, H, t(b"latn")),
        Some(-110)
    );
    assert_eq!(f.baseline(baseline::ROMAN, H, t(b"latn")), Some(0));
    // A null coordinate, and a tag the axis does not have.
    assert_eq!(f.baseline(baseline::HANGING, H, t(b"latn")), None);
    assert_eq!(f.baseline(baseline::MATH, H, t(b"latn")), None);
    // hani has no BaseValues: no baseline, and no DFLT either.
    assert_eq!(f.baseline(baseline::ROMAN, H, t(b"hani")), None);
    // kana's record has a null BaseScript: likewise.
    assert_eq!(f.baseline(baseline::ROMAN, H, t(b"kana")), None);
    // The vertical axis.
    assert_eq!(
        f.baseline(baseline::IDEO_EMBOX_TOP_OR_RIGHT, V, t(b"hani")),
        Some(500)
    );
    assert_eq!(f.baseline(baseline::HANGING, V, t(b"deva")), None);
}

#[test]
fn baseline_scales_like_harfbuzz() {
    let data = lookup_font();
    let f = Font::new(&data, 0).unwrap();
    // Horizontal coordinates scale with y, vertical ones with x
    // (em_scale: (v * mult + 32768) >> 16).
    let s = HbScale::for_font(&f, 768, -1536);
    assert_eq!(
        s.baseline(&f, baseline::HANGING, H, t(b"deva")),
        Some(s.em_scale_y(600))
    );
    assert_eq!(s.baseline(&f, baseline::HANGING, H, t(b"deva")), Some(-922));
    assert_eq!(
        s.baseline(&f, baseline::IDEO_EMBOX_TOP_OR_RIGHT, V, t(b"hani")),
        Some(384)
    );
}

#[test]
fn baseline_ignores_a_base_harfbuzz_rejects() {
    // An offset past the table makes HarfBuzz's sanitizer reject the
    // whole table.
    let mut bad = base(
        Some(&simple_axis(b"DFLT", &[(*b"romn", 7)])),
        Some(&AxisSpec::default()),
    );
    let good = font_with(Some(bad.clone()), &[], &[], &[]);
    assert_eq!(
        Font::new(&good, 0)
            .unwrap()
            .baseline(baseline::ROMAN, H, t(b"latn")),
        Some(7)
    );
    bad[6..8].copy_from_slice(&500u16.to_be_bytes());
    let data = font_with(Some(bad), &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.baseline(baseline::ROMAN, H, t(b"latn")), None);
    // An unknown BaseCoord format anywhere, likewise.
    let axis = AxisSpec {
        tags: Some(vec![*b"ideo", *b"romn"]),
        scripts: Some(vec![(
            *b"DFLT",
            Some(ScriptSpec {
                values: Some((
                    0,
                    vec![Some(Coord::Raw(vec![0, 5, 0, 0])), Some(Coord::F1(7))],
                )),
                ..Default::default()
            }),
        )]),
    };
    let data = font_with(Some(base(Some(&axis), None)), &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.baseline(baseline::ROMAN, H, t(b"latn")), None);
    // A Device table that does not fit, likewise; one with an unknown
    // delta format is accepted.
    for (dev, found) in [
        (device(8, 40, 3, &[]), None),
        (device(8, 40, 7, &[]), Some(7)),
    ] {
        let axis = AxisSpec {
            tags: Some(vec![*b"romn"]),
            scripts: Some(vec![(
                *b"DFLT",
                Some(ScriptSpec {
                    values: Some((0, vec![Some(Coord::F3(7, Some(dev)))])),
                    ..Default::default()
                }),
            )]),
        };
        let data = font_with(Some(base(Some(&axis), None)), &[], &[], &[]);
        let f = Font::new(&data, 0).unwrap();
        assert_eq!(f.baseline(baseline::ROMAN, H, t(b"latn")), found);
    }
    // Version 2.
    let mut v2 = base(Some(&simple_axis(b"DFLT", &[(*b"romn", 7)])), None);
    v2[0..2].copy_from_slice(&2u16.to_be_bytes());
    let data = font_with(Some(v2), &[], &[], &[]);
    assert_eq!(
        Font::new(&data, 0)
            .unwrap()
            .baseline(baseline::ROMAN, H, t(b"latn")),
        None
    );
}

#[test]
fn baseline_checks_the_item_variation_store_of_version_1_1() {
    let axis = simple_axis(b"DFLT", &[(*b"romn", 7)]);
    // format 1, no regions, one VarData with 1 item, no words, 1 region
    // index: 1 byte per row.
    let store = Buf::new()
        .u16(1)
        .u32(0)
        .u16(1)
        .u32(12)
        .u16(1)
        .u16(0)
        .u16(1)
        .u16(0)
        .u8(5)
        .done();
    let data = font_with(Some(base11(Some(&axis), None, Some(&store))), &[], &[], &[]);
    assert_eq!(
        Font::new(&data, 0)
            .unwrap()
            .baseline(baseline::ROMAN, H, t(b"latn")),
        Some(7)
    );
    // The row does not fit.
    let short = &store[..store.len() - 1];
    let data = font_with(Some(base11(Some(&axis), None, Some(short))), &[], &[], &[]);
    assert_eq!(
        Font::new(&data, 0)
            .unwrap()
            .baseline(baseline::ROMAN, H, t(b"latn")),
        None
    );
    // Format 2.
    let mut f2 = store.clone();
    f2[1] = 2;
    let data = font_with(Some(base11(Some(&axis), None, Some(&f2))), &[], &[], &[]);
    assert_eq!(
        Font::new(&data, 0)
            .unwrap()
            .baseline(baseline::ROMAN, H, t(b"latn")),
        None
    );
    // A null store offset.
    let data = font_with(Some(base11(Some(&axis), None, None)), &[], &[], &[]);
    assert_eq!(
        Font::new(&data, 0)
            .unwrap()
            .baseline(baseline::ROMAN, H, t(b"latn")),
        Some(7)
    );
}

// ------------------------------------------------------------ fallbacks

#[test]
fn fallback_roman_and_unknown_tags_are_zero() {
    let data = font_with(None, &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.baseline(baseline::ROMAN, H, t(b"latn")), None);
    assert_eq!(
        f.baseline_with_fallback(baseline::ROMAN, H, t(b"latn"), no_boxes),
        0
    );
    assert_eq!(
        f.baseline_with_fallback(t(b"xxxx"), V, t(b"latn"), no_boxes),
        0
    );
}

#[test]
fn fallback_math_from_the_minus_sign_box() {
    let chars = [(0x2212, 5), (u32::from('-'), 6), (u32::from('x'), 7)];
    let data = font_with(None, &chars, &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    let map = [(5, bx(300, -61)), (6, bx(280, -40)), (7, bx(480, -480))];
    // y_bearing + height / 2, the division toward zero.
    assert_eq!(
        f.baseline_with_fallback(baseline::MATH, H, t(b"latn"), boxes(&map)),
        270
    );
    // Without a box for U+2212, HarfBuzz does not try '-': x-height / 2
    // (no OS/2 and no box for 'x': upem / 2).
    let only_hyphen = [(6, bx(280, -40))];
    assert_eq!(
        f.baseline_with_fallback(baseline::MATH, H, t(b"latn"), boxes(&only_hyphen)),
        250
    );
    // Vertical: x-height / 2 (the top of 'x').
    assert_eq!(
        f.baseline_with_fallback(baseline::MATH, V, t(b"latn"), boxes(&map)),
        240
    );

    // '-' when there is no U+2212.
    let data = font_with(None, &chars[1..], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(
        f.baseline_with_fallback(baseline::MATH, H, t(b"latn"), boxes(&map)),
        260
    );
}

#[test]
fn fallback_math_from_the_x_height() {
    // OS/2 version 1 has no sxHeight: the box of 'x', else upem / 2.
    let old = os2(Os2Spec {
        version: 1,
        ..Default::default()
    });
    let data = font_with(None, &[(u32::from('x'), 7)], &[(b"OS/2", old)], &[]);
    let f = Font::new(&data, 0).unwrap();
    let map = [(7, bx(481, -481))];
    assert_eq!(
        f.baseline_with_fallback(baseline::MATH, H, t(b"latn"), boxes(&map)),
        240
    );
    assert_eq!(
        f.baseline_with_fallback(baseline::MATH, H, t(b"latn"), no_boxes),
        250
    );
    let s = HbScale::for_font(&f, 768, -1536);
    assert_eq!(
        s.baseline_with_fallback(&f, baseline::MATH, H, t(b"latn"), no_boxes),
        -384
    );
}

#[test]
fn fallback_embox_edges_from_each_other_or_the_font_extents() {
    let data = font_with(None, &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    // hhea 800 / -200; no vhea: 500 / -500.
    let wf = |tag, dir| f.baseline_with_fallback(tag, dir, t(b"hani"), no_boxes);
    assert_eq!(wf(baseline::IDEO_EMBOX_TOP_OR_RIGHT, H), 800);
    assert_eq!(wf(baseline::IDEO_EMBOX_BOTTOM_OR_LEFT, H), -200);
    assert_eq!(wf(baseline::IDEO_EMBOX_TOP_OR_RIGHT, V), 500);
    assert_eq!(wf(baseline::IDEO_EMBOX_BOTTOM_OR_LEFT, V), -500);
    // 1/10 of the em box inside.
    assert_eq!(wf(baseline::IDEO_FACE_TOP_OR_RIGHT, H), 700);
    assert_eq!(wf(baseline::IDEO_FACE_BOTTOM_OR_LEFT, H), -100);
    assert_eq!(wf(baseline::IDEO_EMBOX_CENTRAL, H), 300);
    assert_eq!(wf(baseline::IDEO_FACE_CENTRAL, H), 300);
    assert_eq!(wf(baseline::IDEO_EMBOX_CENTRAL, V), 0);

    // One edge in BASE: the other is one em away.
    let h = simple_axis(b"DFLT", &[(*b"ideo", -121)]);
    let v = simple_axis(b"DFLT", &[(*b"idtp", 501)]);
    let data = font_with(Some(base(Some(&h), Some(&v))), &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    let wf = |tag, dir| f.baseline_with_fallback(tag, dir, t(b"hani"), no_boxes);
    assert_eq!(wf(baseline::IDEO_EMBOX_TOP_OR_RIGHT, H), 879);
    assert_eq!(wf(baseline::IDEO_EMBOX_BOTTOM_OR_LEFT, H), -121);
    assert_eq!(wf(baseline::IDEO_EMBOX_BOTTOM_OR_LEFT, V), -499);
    // (879 - 121) / 2 = 379; the face is 100 inside.
    assert_eq!(wf(baseline::IDEO_EMBOX_CENTRAL, H), 379);
    assert_eq!(wf(baseline::IDEO_FACE_TOP_OR_RIGHT, H), 779);
    assert_eq!(wf(baseline::IDEO_FACE_BOTTOM_OR_LEFT, H), -21);
    assert_eq!(wf(baseline::IDEO_FACE_CENTRAL, H), 379);
    // (501 - 499) / 2 = 1.
    assert_eq!(wf(baseline::IDEO_EMBOX_CENTRAL, V), 1);

    // An odd negative sum is halved toward zero: (-1 + -2) / 2 = -1.
    let h = simple_axis(b"DFLT", &[(*b"ideo", -2), (*b"idtp", -1)]);
    let data = font_with(Some(base(Some(&h), None)), &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(
        f.baseline_with_fallback(baseline::IDEO_EMBOX_CENTRAL, H, t(b"hani"), no_boxes),
        -1
    );
    // Both edges in BASE: the face uses them.
    assert_eq!(
        f.baseline_with_fallback(baseline::IDEO_FACE_TOP_OR_RIGHT, H, t(b"hani"), no_boxes),
        -1
    );
}

#[test]
fn fallback_central_comes_from_base_when_there() {
    let h = simple_axis(b"DFLT", &[(*b"Idce", 333)]);
    let data = font_with(Some(base(Some(&h), None)), &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(
        f.baseline_with_fallback(baseline::IDEO_EMBOX_CENTRAL, H, t(b"hani"), no_boxes),
        333
    );
}

#[test]
fn fallback_hanging_from_the_script_character() {
    let chars = [(0x0915, 3), (0x0995, 4)];
    let data = font_with(None, &chars, &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    let map = [(3, bx(640, -640)), (4, bx(650, -650))];
    let hang = |script: &[u8; 4], dir| {
        f.baseline_with_fallback(baseline::HANGING, dir, t(script), boxes(&map))
    };
    // Old and new tags of a script, as hb_ot_tag_to_script maps them.
    assert_eq!(hang(b"deva", H), 640);
    assert_eq!(hang(b"dev2", H), 640);
    assert_eq!(hang(b"dev3", H), 640);
    assert_eq!(hang(b"beng", H), 650);
    assert_eq!(hang(b"bng2", H), 650);
    // A script of the table whose character the font lacks, a script
    // outside the table, and vertical text: 0.6 em.
    assert_eq!(hang(b"gujr", H), 600);
    assert_eq!(hang(b"latn", H), 600);
    assert_eq!(hang(b"DEVA", H), 600);
    assert_eq!(hang(b"deva", V), 600);
    // No box for the character.
    assert_eq!(
        f.baseline_with_fallback(baseline::HANGING, H, t(b"deva"), no_boxes),
        600
    );
    let s = HbScale::for_font(&f, 768, -1536);
    assert_eq!(
        s.baseline_with_fallback(&f, baseline::HANGING, H, t(b"latn"), no_boxes),
        -921
    );
    assert_eq!(
        s.baseline_with_fallback(&f, baseline::HANGING, V, t(b"latn"), no_boxes),
        460
    );
}

#[test]
fn fallback_scales_the_em_box() {
    let h = simple_axis(b"DFLT", &[(*b"ideo", -120)]);
    let data = font_with(Some(base(Some(&h), None)), &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    let s = HbScale::for_font(&f, 2000, 2000);
    // -240 + y_scale.
    assert_eq!(
        s.baseline_with_fallback(
            &f,
            baseline::IDEO_EMBOX_TOP_OR_RIGHT,
            H,
            t(b"hani"),
            no_boxes
        ),
        1760
    );
}

// --------------------------------------------------------------- MinMax

fn min_max_font() -> Vec<u8> {
    let h = AxisSpec {
        tags: Some(vec![*b"romn"]),
        scripts: Some(vec![
            (
                *b"DFLT",
                Some(ScriptSpec {
                    values: Some((0, vec![Some(Coord::F1(0))])),
                    ..Default::default()
                }),
            ),
            (
                *b"cyrl",
                Some(ScriptSpec {
                    lang_sys: vec![(
                        *b"SRB ",
                        Some(MinMaxSpec {
                            min: Some(Coord::F1(-1)),
                            max: Some(Coord::F1(1)),
                            features: vec![],
                        }),
                    )],
                    ..Default::default()
                }),
            ),
            (
                *b"latn",
                Some(ScriptSpec {
                    default_min_max: Some(MinMaxSpec {
                        min: Some(Coord::F1(-300)),
                        max: Some(Coord::F1(1100)),
                        features: vec![(*b"kern", Some(Coord::F1(-9)), Some(Coord::F1(9)))],
                    }),
                    lang_sys: vec![
                        (
                            *b"ENG ",
                            Some(MinMaxSpec {
                                min: None,
                                max: Some(Coord::F3(1200, None)),
                                features: vec![],
                            }),
                        ),
                        (*b"TRK ", None),
                    ],
                    ..Default::default()
                }),
            ),
        ]),
    };
    font_with(Some(base(Some(&h), None)), &[], &[], &[])
}

#[test]
fn layout_font_extents_from_min_max() {
    let data = min_max_font();
    let f = Font::new(&data, 0).unwrap();
    let get = |script: &[u8; 4], lang: &[u8; 4]| f.layout_font_extents(H, t(script), t(lang));
    // The default MinMax; FeatMinMax is not used.
    assert_eq!(get(b"latn", b"dflt"), (ext(1100, -300, 0), true));
    // A language's MinMax; a null coordinate is 0.
    assert_eq!(get(b"latn", b"ENG "), (ext(1200, 0, 0), true));
    // A language record with a null MinMax: both 0.
    assert_eq!(get(b"latn", b"TRK "), (ext(0, 0, 0), true));
    // Language systems but no default MinMax: an unknown language gets
    // the null default.
    assert_eq!(get(b"cyrl", b"SRB "), (ext(1, -1, 0), true));
    assert_eq!(get(b"cyrl", b"dflt"), (ext(0, 0, 0), true));
    // DFLT has no MinMax at all; an unknown script falls to DFLT: the
    // font's extents for the direction.
    assert_eq!(get(b"DFLT", b"dflt"), (ext(800, -200, 90), false));
    assert_eq!(get(b"grek", b"dflt"), (ext(800, -200, 90), false));
    // No vertical axis.
    assert_eq!(
        f.layout_font_extents(V, t(b"latn"), t(b"dflt")),
        (ext(500, -500, 0), false)
    );

    let s = HbScale::for_font(&f, 768, -1536);
    assert_eq!(
        s.layout_font_extents(&f, H, t(b"latn"), t(b"dflt")),
        (ext(-1690, 461, 0), true)
    );
}

#[test]
fn layout_font_extents_without_base() {
    let data = font_with(None, &[], &[], &[]);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(
        f.layout_font_extents(H, t(b"latn"), t(b"dflt")),
        (f.h_extents(), false)
    );
    assert_eq!(
        f.layout_font_extents(V, t(b"latn"), t(b"dflt")),
        (f.v_extents(), false)
    );
}
