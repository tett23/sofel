//! GSUB, GPOS and GDEF on hand-made fonts (ADR 0022, stage 3): the
//! layout closure, the feature filter and HarfBuzz's Coverage and
//! ClassDef format choices.

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
use sofel_subset::{Flags, Input, SetKind, SubsetError, subset};

/// Glyph id of a capital letter of the sample font ('A' is 1).
fn g(c: char) -> u16 {
    c as u16 - 0x40
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([d[at], d[at + 1]])
}

/// The bytes at the 16-bit offset stored at `at` of `d`.
fn follow(d: &[u8], at: usize) -> &[u8] {
    &d[u16_at(d, at) as usize..]
}

/// A GSUB or GPOS table with one script (`DFLT`, default language
/// system using every feature), `features` as `(tag, lookup indices)`
/// and `lookups` as `(type, subtable)` with one subtable each.
fn layout_table(features: &[(&[u8; 4], &[u16])], lookups: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut langsys = Buf::new().u16(0).u16(0xFFFF).u16(features.len() as u16);
    for i in 0..features.len() {
        langsys = langsys.u16(i as u16);
    }
    let script = link(
        Buf::new().u16(0).u16(0).done(),
        vec![(0, 2, langsys.done())],
    );
    let script_list = link(
        Buf::new().u16(1).tag(b"DFLT").u16(0).done(),
        vec![(6, 2, script)],
    );
    let mut fl = Buf::new().u16(features.len() as u16);
    for (tag, _) in features {
        fl = fl.tag(tag).u16(0);
    }
    let feature_list = link(
        fl.done(),
        features
            .iter()
            .enumerate()
            .map(|(i, (_, ls))| {
                let mut f = Buf::new().u16(0).u16(ls.len() as u16);
                for l in *ls {
                    f = f.u16(*l);
                }
                (2 + 6 * i + 4, 2, f.done())
            })
            .collect(),
    );
    let mut ll = Buf::new().u16(lookups.len() as u16);
    for _ in lookups {
        ll = ll.u16(0);
    }
    let lookup_list = link(
        ll.done(),
        lookups
            .iter()
            .enumerate()
            .map(|(i, (ty, st))| {
                let l = link(
                    Buf::new().u16(*ty).u16(0).u16(1).u16(0).done(),
                    vec![(6, 2, st.clone())],
                );
                (2 + 2 * i, 2, l)
            })
            .collect(),
    );
    link(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
        vec![
            (4, 2, script_list),
            (6, 2, feature_list),
            (8, 2, lookup_list),
        ],
    )
}

/// A LigatureSubst of `first` + `second` to `lig`.
fn ligature(first: u16, second: u16, lig: u16) -> Vec<u8> {
    let ligature = Buf::new().u16(lig).u16(2).u16(second).done();
    let set = link(Buf::new().u16(1).u16(0).done(), vec![(2, 2, ligature)]);
    link(
        Buf::new().u16(1).u16(0).u16(1).u16(0).done(),
        vec![(2, 2, coverage1(&[first])), (6, 2, set)],
    )
}

/// A SingleSubst format 1 of `glyphs` by `delta`.
fn single(glyphs: &[u16], delta: u16) -> Vec<u8> {
    link(
        Buf::new().u16(1).u16(0).u16(delta).done(),
        vec![(2, 2, coverage1(glyphs))],
    )
}

fn font_with(extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    let mut t = sample_tables();
    t.extend(extra);
    t.sort_by_key(|(t, _)| **t);
    sfnt(&t)
}

fn chars(s: &str) -> Input {
    Input::new().map_set(SetKind::Unicodes, |set| {
        s.chars().for_each(|c| set.add(c as u32));
    })
}

fn out_table(out: &[u8], tag: &[u8; 4]) -> Option<Vec<u8>> {
    Sfnt::new(out)
        .unwrap()
        .table(Tag::new(tag))
        .map(|t| t.to_vec())
}

fn num_glyphs(out: &[u8]) -> u16 {
    Font::new(out, 0).unwrap().num_glyphs() as u16
}

#[test]
fn ligature_closure() {
    let gsub = layout_table(&[(b"liga", &[0])], &[(4, ligature(g('A'), g('B'), g('Z')))]);
    let data = font_with(vec![(b"GSUB", gsub)]);
    let font = Font::new(&data, 0).unwrap();
    // A and B make Z.
    let out = subset(&font, &chars("AB")).unwrap();
    assert_eq!(num_glyphs(&out), 4);
    // A alone does not.
    let out = subset(&font, &chars("A")).unwrap();
    assert_eq!(num_glyphs(&out), 2);
    // Without the layout closure Z is not added, and the ligature, whose
    // result is gone, is dropped from the lookup.
    let input = chars("AB").with_flags(Flags::NO_LAYOUT_CLOSURE);
    let out = subset(&font, &input).unwrap();
    assert_eq!(num_glyphs(&out), 3);
    let gsub = out_table(&out, b"GSUB").unwrap();
    let lookup = follow(follow(&gsub, 8), 2);
    assert_eq!(u16_at(lookup, 4), 0, "no subtable is left");
}

#[test]
fn feature_filter() {
    // 'smcp' is not among the default features: its lookup and the
    // glyphs it would add are dropped.
    let gsub = layout_table(
        &[(b"liga", &[0]), (b"smcp", &[1])],
        &[
            (4, ligature(g('A'), g('B'), g('Z'))),
            (1, single(&[g('A')], g('Y') - g('A'))),
        ],
    );
    let data = font_with(vec![(b"GSUB", gsub)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("AB")).unwrap();
    assert_eq!(num_glyphs(&out), 4);
    let gsub = out_table(&out, b"GSUB").unwrap();
    assert_eq!(u16_at(follow(&gsub, 6), 0), 1, "one feature");
    assert_eq!(u16_at(follow(&gsub, 8), 0), 1, "one lookup");
    // Keeping every feature keeps both.
    let input = chars("AB").map_set(SetKind::LayoutFeatures, |s| {
        s.clear();
        s.invert();
    });
    let out = subset(&font, &input).unwrap();
    assert_eq!(num_glyphs(&out), 5);
    let gsub = out_table(&out, b"GSUB").unwrap();
    assert_eq!(u16_at(follow(&gsub, 8), 0), 2);
}

/// The coverage of the first subtable of the first lookup.
fn first_coverage(table: &[u8]) -> Vec<u8> {
    let lookup = follow(follow(table, 8), 2);
    let st = follow(lookup, 6);
    let cov = follow(st, 2);
    let len = match u16_at(cov, 0) {
        1 => 4 + 2 * u16_at(cov, 2) as usize,
        _ => 4 + 6 * u16_at(cov, 2) as usize,
    };
    cov[..len].to_vec()
}

#[test]
fn coverage_formats() {
    // A..E become F..J; five consecutive glyphs are one range, which is
    // smaller than five glyph ids (count > 3 * ranges): format 2.
    let gsub = layout_table(&[(b"liga", &[0])], &[(1, single(&[1, 2, 3, 4, 5], 5))]);
    let data = font_with(vec![(b"GSUB", gsub)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("ABCDE")).unwrap();
    let gsub = out_table(&out, b"GSUB").unwrap();
    assert_eq!(first_coverage(&gsub), vec![0, 2, 0, 1, 0, 1, 0, 5, 0, 0]);
    // A, C and E are renumbered 1, 2 and 3: one range of three glyphs,
    // and three ids are not more than three times one range: format 1.
    let out = subset(&font, &chars("ACE")).unwrap();
    let gsub = out_table(&out, b"GSUB").unwrap();
    assert_eq!(first_coverage(&gsub), vec![0, 1, 0, 3, 0, 1, 0, 2, 0, 3]);
    // The substitution keeps its constant delta (F is new glyph 4).
    let lookup = follow(follow(&gsub, 8), 2);
    let st = follow(lookup, 6);
    assert_eq!((u16_at(st, 0), u16_at(st, 4)), (1, 3));
}

fn gdef_with_classes(class_def: Vec<u8>) -> Vec<u8> {
    link(
        Buf::new()
            .u32(0x0001_0000)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .done(),
        vec![(4, 2, class_def)],
    )
}

fn gdef_with_unused_store(class_def: Vec<u8>) -> Vec<u8> {
    let store = Buf::new().u16(1).u32(8).u16(0).u16(0).u16(0).done();
    link(
        Buf::new()
            .u32(0x0001_0003)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u32(0)
            .done(),
        vec![(4, 2, class_def), (14, 4, store)],
    )
}

#[test]
fn unused_gdef_variation_store_is_removed() {
    let class_def = class_def2(&[(1, 5, 1)]);
    let data = font_with(vec![(b"GDEF", gdef_with_unused_store(class_def))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("ABCDE")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    assert_eq!(
        u32::from_be_bytes(gdef[0..4].try_into().unwrap()),
        0x0001_0000
    );
    assert_eq!(glyph_class_def(&out), vec![0, 2, 0, 1, 0, 1, 0, 5, 0, 1]);
}

#[test]
fn referenced_gdef_variation_store_remains_unsupported() {
    let varidx = Buf::new().u16(0).u16(0).u16(0x8000).done();
    let caret = link(Buf::new().u16(3).i16(0).u16(0).done(), vec![(4, 2, varidx)]);
    let glyph = link(Buf::new().u16(1).u16(0).done(), vec![(2, 2, caret)]);
    let carets = link(
        Buf::new().u16(0).u16(1).u16(0).done(),
        vec![(0, 2, coverage1(&[1])), (4, 2, glyph)],
    );
    let store = Buf::new().u16(1).u32(8).u16(0).u16(0).u16(0).done();
    let gdef = link(
        Buf::new()
            .u32(0x0001_0003)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u32(0)
            .done(),
        vec![
            (4, 2, class_def2(&[(1, 5, 1)])),
            (8, 2, carets),
            (14, 4, store),
        ],
    );
    let data = font_with(vec![(b"GDEF", gdef)]);
    let font = Font::new(&data, 0).unwrap();
    assert!(
        matches!(subset(&font, &chars("A")), Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"GDEF"))
    );
}

#[test]
fn gdef_store_with_gpos_without_variation_index_is_subset() {
    let gdef = gdef_with_unused_store(class_def2(&[(1, 5, 1)]));
    let gpos = layout_table(&[], &[]);
    let data = font_with(vec![(b"GDEF", gdef), (b"GPOS", gpos)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    assert!(out_table(&out, b"GDEF").is_some());
    let copied_gpos = chars("A").map_set(SetKind::NoSubsetTables, |set| {
        set.add(Tag::new(b"GPOS").to_u32());
    });
    let out = subset(&font, &copied_gpos).unwrap();
    assert_eq!(out_table(&out, b"GPOS"), Some(layout_table(&[], &[])));
}

#[test]
fn gdef_store_with_math_without_variation_index_is_subset() {
    let gdef = gdef_with_unused_store(class_def2(&[(1, 5, 1)]));
    let math = Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done();
    let data = font_with(vec![(b"GDEF", gdef), (b"MATH", math)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    assert!(out_table(&out, b"GDEF").is_some());
}

#[test]
fn gdef_store_with_gpos_variation_index_remains_unsupported() {
    let gdef = gdef_with_unused_store(class_def2(&[(1, 5, 1)]));
    let varidx = Buf::new().u16(0).u16(0).u16(0x8000).done();
    let single = link(
        Buf::new().u16(1).u16(0).u16(0x0040).u16(0).done(),
        vec![(2, 2, coverage1(&[1])), (6, 2, varidx)],
    );
    let gpos = layout_table(&[(b"kern", &[0])], &[(1, single)]);
    let data = font_with(vec![(b"GDEF", gdef), (b"GPOS", gpos)]);
    let font = Font::new(&data, 0).unwrap();
    assert!(
        matches!(subset(&font, &chars("A")), Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"GDEF"))
    );
    let copied_gpos_without_gdef = chars("A")
        .map_set(SetKind::NoSubsetTables, |set| {
            set.add(Tag::new(b"GPOS").to_u32())
        })
        .map_set(SetKind::DropTables, |set| {
            set.add(Tag::new(b"GDEF").to_u32())
        });
    let out = subset(&font, &copied_gpos_without_gdef).unwrap();
    assert!(out_table(&out, b"GDEF").is_none());
    assert_eq!(out_table(&out, b"GPOS"), out_table(&data, b"GPOS"));
}

fn singlepos_with_variation_index(format: u16) -> Vec<u8> {
    let device = |inner| Buf::new().u16(0).u16(inner).u16(0x8000).done();
    let single = if format == 1 {
        link(
            Buf::new().u16(1).u16(0).u16(0x0040).u16(0).done(),
            vec![(2, 2, coverage1(&[g('B'), g('C')])), (6, 2, device(2))],
        )
    } else {
        link(
            Buf::new()
                .u16(2)
                .u16(0)
                .u16(0x0040)
                .u16(2)
                .u16(0)
                .u16(0)
                .done(),
            vec![
                (2, 2, coverage1(&[g('A'), g('B')])),
                (8, 2, device(1)),
                (10, 2, device(2)),
            ],
        )
    };
    layout_table(&[(b"kern", &[0])], &[(1, single)])
}

#[test]
fn gpos_single1_variation_index_retained_after_glyph_filter_is_remapped() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", singlepos_with_variation_index(1)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("BC")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 1);
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
}

#[test]
fn gpos_single2_variation_index_uses_only_kept_value_record() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", singlepos_with_variation_index(2)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("B")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 1);
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
}

#[test]
fn gpos_single_variation_index_is_removed_by_no_hinting() {
    for format in [1, 2] {
        let data = font_with(vec![
            (b"GDEF", gdef_with_referenced_store(&[2])),
            (b"GPOS", singlepos_with_variation_index(format)),
        ]);
        let font = Font::new(&data, 0).unwrap();
        let input = chars("B").with_flags(Flags::NO_HINTING);
        let out = subset(&font, &input).unwrap();
        let gpos = out_table(&out, b"GPOS").unwrap();
        assert!(!gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
    }
}

#[test]
fn copied_gdef_uses_single_index_remap_like_harfbuzz() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", singlepos_with_variation_index(1)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = chars("BC").map_set(SetKind::NoSubsetTables, |set| {
        set.add(Tag::new(b"GDEF").to_u32());
    });
    let out = subset(&font, &input).unwrap();
    assert_eq!(out_table(&out, b"GDEF"), out_table(&data, b"GDEF"));
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
}

#[test]
fn gpos_single_variation_index_combines_with_gdef_caret() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[0])),
        (b"GPOS", singlepos_with_variation_index(1)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("ABC")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 2);
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 1, 0x80, 0]));
}

#[test]
fn gpos_single_variation_index_drops_with_lookup_or_glyph() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", singlepos_with_variation_index(1)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    assert!(out_table(&subset(&font, &chars("D")).unwrap(), b"GDEF").is_none());
    let input = chars("BC").map_set(SetKind::LayoutFeatures, |set| {
        set.clear();
        set.add(Tag::new(b"liga").to_u32());
    });
    assert!(out_table(&subset(&font, &input).unwrap(), b"GDEF").is_none());
}

#[test]
fn gpos_cursive_variation_index_is_remapped() {
    let device = Buf::new().u16(0).u16(2).u16(0x8000).done();
    let anchor = link(
        Buf::new().u16(3).i16(0).i16(0).u16(0).u16(0).done(),
        vec![(6, 2, device)],
    );
    let cursive = link(
        Buf::new().u16(1).u16(0).u16(1).u16(0).u16(0).done(),
        vec![(2, 2, coverage1(&[g('B')])), (6, 2, anchor)],
    );
    let gpos = layout_table(&[(b"curs", &[0])], &[(3, cursive)]);
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", gpos),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("B")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 1);
}

fn anchor3_varidx(inner: u16) -> Vec<u8> {
    let device = Buf::new().u16(0).u16(inner).u16(0x8000).done();
    link(
        Buf::new().u16(3).i16(10).i16(20).u16(0).u16(0).done(),
        vec![(6, 2, device)],
    )
}

fn anchor3_both_varidx(x: u16, y: u16) -> Vec<u8> {
    link(
        Buf::new().u16(3).i16(10).i16(20).u16(0).u16(0).done(),
        vec![
            (6, 2, Buf::new().u16(0).u16(x).u16(0x8000).done()),
            (8, 2, Buf::new().u16(0).u16(y).u16(0x8000).done()),
        ],
    )
}

fn gpos_anchor_lookup(kind: u16) -> Vec<u8> {
    let subtable = if kind == 3 {
        link(
            Buf::new()
                .u16(1)
                .u16(0)
                .u16(2)
                .u16(0)
                .u16(0)
                .u16(0)
                .u16(0)
                .done(),
            vec![
                (2, 2, coverage1(&[g('A'), g('B')])),
                (6, 2, anchor3_varidx(1)),
                (10, 2, anchor3_varidx(2)),
            ],
        )
    } else {
        let marks = link(
            Buf::new().u16(2).u16(0).u16(0).u16(0).u16(0).done(),
            vec![(4, 2, anchor3_varidx(0)), (8, 2, anchor3_varidx(1))],
        );
        let bases = if kind == 5 {
            let attach = |inner| {
                link(
                    Buf::new().u16(1).u16(0).done(),
                    vec![(2, 2, anchor3_varidx(inner))],
                )
            };
            link(
                Buf::new().u16(2).u16(0).u16(0).done(),
                vec![(2, 2, attach(0)), (4, 2, attach(2))],
            )
        } else {
            link(
                Buf::new().u16(2).u16(0).u16(0).done(),
                vec![(2, 2, anchor3_varidx(0)), (4, 2, anchor3_varidx(2))],
            )
        };
        link(
            Buf::new().u16(1).u16(0).u16(0).u16(1).u16(0).u16(0).done(),
            vec![
                (2, 2, coverage1(&[g('A'), g('B')])),
                (4, 2, coverage1(&[g('C'), g('D')])),
                (8, 2, marks),
                (10, 2, bases),
            ],
        )
    };
    let feature = if kind == 3 { b"curs" } else { b"mark" };
    layout_table(&[(feature, &[0])], &[(kind, subtable)])
}

fn mark_base_two_classes() -> Vec<u8> {
    let marks = link(
        Buf::new().u16(2).u16(0).u16(0).u16(1).u16(0).done(),
        vec![(4, 2, anchor3_varidx(0)), (8, 2, anchor3_varidx(1))],
    );
    let bases = link(
        Buf::new().u16(2).u16(0).u16(0).u16(0).u16(0).done(),
        vec![(2, 2, anchor3_varidx(0)), (8, 2, anchor3_varidx(2))],
    );
    let subtable = link(
        Buf::new().u16(1).u16(0).u16(0).u16(2).u16(0).u16(0).done(),
        vec![
            (2, 2, coverage1(&[g('A'), g('B')])),
            (4, 2, coverage1(&[g('C'), g('D')])),
            (8, 2, marks),
            (10, 2, bases),
        ],
    );
    layout_table(&[(b"mark", &[0])], &[(4, subtable)])
}

fn assert_anchor_subset(kind: u16, expected_items: u16) {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[0])),
        (b"GPOS", gpos_anchor_lookup(kind)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("BD")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, expected_items);
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
}

#[test]
fn gpos_cursive_anchor_variation_index_follows_kept_glyph() {
    assert_anchor_subset(3, 1);
}

#[test]
fn gpos_cursive_anchor_variation_index_remaps_both_devices() {
    let cursive = link(
        Buf::new().u16(1).u16(0).u16(1).u16(0).u16(0).done(),
        vec![
            (2, 2, coverage1(&[g('B')])),
            (6, 2, anchor3_both_varidx(2, 1)),
        ],
    );
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[0])),
        (b"GPOS", layout_table(&[(b"curs", &[0])], &[(3, cursive)])),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("B")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 2);
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 1, 0x80, 0]));
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
}

#[test]
fn gpos_mark_base_anchor_variation_index_follows_kept_rows() {
    assert_anchor_subset(4, 2);
}

#[test]
fn gpos_mark_lig_anchor_variation_index_follows_kept_rows() {
    assert_anchor_subset(5, 2);
}

#[test]
fn gpos_mark_mark_anchor_variation_index_follows_kept_rows() {
    assert_anchor_subset(6, 2);
}

#[test]
fn gpos_mark_base_anchor_variation_index_follows_kept_class_column() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[0])),
        (b"GPOS", mark_base_two_classes()),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("BD")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 2);
}

#[test]
fn gpos_anchor_variation_index_survives_no_hinting() {
    for kind in [3, 4, 5, 6] {
        let data = font_with(vec![
            (b"GDEF", gdef_with_referenced_store(&[0])),
            (b"GPOS", gpos_anchor_lookup(kind)),
        ]);
        let font = Font::new(&data, 0).unwrap();
        let out = subset(&font, &chars("BD").with_flags(Flags::NO_HINTING)).unwrap();
        let gpos = out_table(&out, b"GPOS").unwrap();
        assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
    }
}

#[test]
fn copied_gdef_uses_anchor_index_remap_like_harfbuzz() {
    for kind in [3, 4, 5, 6] {
        let data = font_with(vec![
            (b"GDEF", gdef_with_referenced_store(&[0])),
            (b"GPOS", gpos_anchor_lookup(kind)),
        ]);
        let font = Font::new(&data, 0).unwrap();
        let input = chars("BD").map_set(SetKind::NoSubsetTables, |set| {
            set.add(Tag::new(b"GDEF").to_u32());
        });
        let out = subset(&font, &input).unwrap();
        assert_eq!(out_table(&out, b"GDEF"), out_table(&data, b"GDEF"));
        let gpos = out_table(&out, b"GPOS").unwrap();
        assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
    }
}

#[test]
fn gpos_anchor_variation_index_drops_with_lookup_or_glyph() {
    for kind in [3, 4, 5, 6] {
        let data = font_with(vec![
            (b"GDEF", gdef_with_referenced_store(&[0])),
            (b"GPOS", gpos_anchor_lookup(kind)),
        ]);
        let font = Font::new(&data, 0).unwrap();
        let filtered_lookup = chars("BD").map_set(SetKind::LayoutFeatures, |set| {
            set.clear();
            set.add(Tag::new(b"liga").to_u32());
        });
        assert!(out_table(&subset(&font, &filtered_lookup).unwrap(), b"GDEF").is_none());
        assert!(out_table(&subset(&font, &chars("E")).unwrap(), b"GDEF").is_none());
    }
}

#[test]
fn gpos_pair_variation_index_retained_after_layout_filter_is_remapped() {
    // GDEF's caret on A is dropped. PairPos on B+C keeps row 2.
    let gdef = gdef_with_referenced_store(&[2]);
    let gpos = pairpos_with_variation_index(2);
    let data = font_with(vec![(b"GDEF", gdef), (b"GPOS", gpos)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("BC")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 1);
    let gpos = out_table(&out, b"GPOS").unwrap();
    let lookup = follow(follow(&gpos, 8), 2);
    let pair = follow(lookup, 6);
    let pair_set = follow(pair, 10);
    let device = follow(pair_set, 4);
    assert_eq!(&device[..6], &[0, 0, 0, 0, 0x80, 0]);
}

fn pairpos_with_variation_index(inner: u16) -> Vec<u8> {
    let varidx = Buf::new().u16(0).u16(inner).u16(0x8000).done();
    let pair_set = link(
        Buf::new().u16(1).u16(g('C')).u16(0).done(),
        vec![(4, 2, varidx)],
    );
    let pair = link(
        Buf::new()
            .u16(1)
            .u16(0)
            .u16(0x0040)
            .u16(0)
            .u16(1)
            .u16(0)
            .done(),
        vec![(2, 2, coverage1(&[g('B')])), (10, 2, pair_set)],
    );
    layout_table(&[(b"kern", &[0])], &[(2, pair)])
}

fn pairpos2_with_variation_index(inner: u16) -> Vec<u8> {
    let varidx = Buf::new().u16(0).u16(inner).u16(0x8000).done();
    let pair = link(
        Buf::new()
            .u16(2)
            .u16(0)
            .u16(0x0040)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(2)
            .u16(2)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .done(),
        vec![
            (2, 2, coverage1(&[g('B')])),
            (8, 2, class_def2(&[(g('B'), g('B'), 1)])),
            (10, 2, class_def2(&[(g('C'), g('C'), 1)])),
            (22, 2, varidx),
        ],
    );
    layout_table(&[(b"kern", &[0])], &[(2, pair)])
}

#[test]
fn gpos_pair2_variation_index_follows_kept_classes() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", pairpos2_with_variation_index(2)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("BC")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 1);
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
}

#[test]
fn gpos_pair2_variation_index_survives_no_hinting() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", pairpos2_with_variation_index(2)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = chars("BC").with_flags(Flags::NO_HINTING);
    let out = subset(&font, &input).unwrap();
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
}

#[test]
fn gpos_pair2_variation_index_drops_with_class_two() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", pairpos2_with_variation_index(2)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("B")).unwrap();
    assert!(out_table(&out, b"GDEF").is_none());
}

#[test]
fn copied_gdef_uses_pair_index_remap_like_harfbuzz() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", pairpos_with_variation_index(2)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = chars("BC").map_set(SetKind::NoSubsetTables, |set| {
        set.add(Tag::new(b"GDEF").to_u32());
    });
    let out = subset(&font, &input).unwrap();
    assert_eq!(out_table(&out, b"GDEF"), out_table(&data, b"GDEF"));
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert!(gpos.windows(6).any(|w| w == [0, 0, 0, 0, 0x80, 0]));
}

#[test]
fn gdef_store_with_empty_mark_sets_zeros_removed_offset() {
    let mut gdef = gdef_with_referenced_store(&[2]);
    let mark_sets_offset = u16::try_from(gdef.len()).unwrap();
    gdef[12..14].copy_from_slice(&mark_sets_offset.to_be_bytes());
    gdef.extend_from_slice(&[0, 1, 0, 0]);
    let data = font_with(vec![(b"GDEF", gdef)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    assert_eq!(u16_at(&gdef, 12), 0);
}

#[test]
fn gpos_pair_variation_index_dropped_with_second_glyph() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", pairpos_with_variation_index(2)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("B")).unwrap();
    assert!(out_table(&out, b"GDEF").is_none());
}

#[test]
fn gpos_pair_variation_index_dropped_with_feature() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[2])),
        (b"GPOS", pairpos_with_variation_index(2)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let input = chars("BC").map_set(SetKind::LayoutFeatures, |set| {
        set.clear();
        set.add(Tag::new(b"liga").to_u32());
    });
    let out = subset(&font, &input).unwrap();
    assert!(out_table(&out, b"GDEF").is_none());
}

#[test]
fn gpos_pair_variation_index_combines_with_gdef_caret() {
    let data = font_with(vec![
        (b"GDEF", gdef_with_referenced_store(&[0])),
        (b"GPOS", pairpos_with_variation_index(2)),
    ]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("ABC")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 2);
    let gpos = out_table(&out, b"GPOS").unwrap();
    let pair = follow(follow(follow(&gpos, 8), 2), 6);
    let device = follow(follow(pair, 10), 4);
    assert_eq!(&device[..6], &[0, 0, 0, 1, 0x80, 0]);
}

#[test]
fn gdef_store_with_math_constant_variation_index_is_remapped() {
    let gdef = gdef_with_referenced_store(&[0]);
    let varidx = Buf::new().u16(0).u16(2).u16(0x8000).done();
    let constants = link(vec![0; 214], vec![(10, 2, varidx)]);
    let math = link(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
        vec![(4, 2, constants)],
    );
    let data = font_with(vec![(b"GDEF", gdef), (b"MATH", math)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 2);
    let math = out_table(&out, b"MATH").unwrap();
    let device = follow(follow(&math, 4), 10);
    assert_eq!(&device[..6], &[0, 0, 0, 1, 0x80, 0]);
    // The MATH writer still remaps its device indices when GDEF is copied.
    let copy_gdef = chars("A").map_set(SetKind::NoSubsetTables, |set| {
        set.add(Tag::new(b"GDEF").to_u32());
    });
    let out = subset(&font, &copy_gdef).unwrap();
    assert_eq!(out_table(&out, b"GDEF"), out_table(&data, b"GDEF"));
    let math = out_table(&out, b"MATH").unwrap();
    assert_eq!(&follow(follow(&math, 4), 10)[..6], &[0, 0, 0, 1, 0x80, 0]);
    let copied_math_without_gdef = chars("A")
        .map_set(SetKind::NoSubsetTables, |set| {
            set.add(Tag::new(b"MATH").to_u32())
        })
        .map_set(SetKind::DropTables, |set| {
            set.add(Tag::new(b"GDEF").to_u32())
        });
    let out = subset(&font, &copied_math_without_gdef).unwrap();
    assert!(out_table(&out, b"GDEF").is_none());
    assert_eq!(out_table(&out, b"MATH"), out_table(&data, b"MATH"));
}

#[test]
fn copied_math_with_an_unchanged_variation_index_keeps_its_gdef_row() {
    let input = chars("A").map_set(SetKind::NoSubsetTables, |set| {
        set.add(Tag::new(b"MATH").to_u32());
    });
    for inner in [1, 2] {
        let device = Buf::new().u16(0).u16(inner).u16(0x8000).done();
        let constants = link(vec![0; 214], vec![(10, 2, device)]);
        let math = link(
            Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
            vec![(4, 2, constants)],
        );
        let data = font_with(vec![
            (b"GDEF", gdef_with_referenced_store(&[0])),
            (b"MATH", math),
        ]);
        let font = Font::new(&data, 0).unwrap();
        let result = subset(&font, &input);
        if inner == 2 {
            assert!(
                matches!(result, Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"GDEF"))
            );
            continue;
        }
        let out = result.unwrap();
        assert_eq!(out_table(&out, b"MATH"), out_table(&data, b"MATH"));
        let gdef = out_table(&out, b"GDEF").unwrap();
        let gdef = sofel_font::read::gdef::Gdef::new(&gdef).unwrap();
        assert_eq!(
            gdef.variation_store()
                .unwrap()
                .unwrap()
                .item_data(0)
                .unwrap()
                .item_count,
            2
        );
    }
}

#[test]
fn adr0022_negative_gpos_advance_is_not_a_variation_index() {
    let gdef = gdef_with_unused_store(class_def2(&[(1, 5, 1)]));
    let position = link(
        Buf::new().u16(1).u16(0).u16(4).i16(i16::MIN).done(),
        vec![(2, 2, coverage1(&[g('A')]))],
    );
    let gpos = layout_table(&[(b"kern", &[0])], &[(1, position)]);
    let data = font_with(vec![(b"GDEF", gdef), (b"GPOS", gpos)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let gpos = out_table(&out, b"GPOS").unwrap();
    let lookup_list = follow(&gpos, 8);
    let lookup = follow(lookup_list, 2);
    let single = follow(lookup, 6);
    assert_eq!(u16_at(single, 6), 0x8000);
    assert_eq!(
        &out_table(&out, b"GDEF").unwrap()[..4],
        &0x10000u32.to_be_bytes()
    );
}

#[test]
fn adr0022_gsub_axis_range_can_survive_gdef_store_subsetting() {
    let gdef = gdef_with_unused_store(class_def2(&[(1, 5, 1)]));
    let mut gsub = gsubgpos(
        0x0001_0001,
        script_list(vec![(b"DFLT", script(lang_sys(0xffff, &[0]), vec![]))]),
        feature_list(&[(b"liga", &[0])]),
        lookup_list(
            vec![
                lookup(1, 0, vec![single(&[1], 1)], None),
                lookup(1, 0, vec![single(&[1], 2)], None),
            ],
            false,
        ),
    );
    let fv_at = u32::try_from(gsub.len()).unwrap();
    gsub[10..14].copy_from_slice(&fv_at.to_be_bytes());
    let condition = Buf::new().u16(1).u16(0).i16(0).i16(0x4000).done();
    let conditions = link(Buf::new().u16(1).u32(0).done(), vec![(2, 4, condition)]);
    let replacement = Buf::new().u16(0).u16(1).u16(1).done();
    let substitutions = link(
        Buf::new().u32(0x10000).u16(1).u16(0).u32(0).done(),
        vec![(8, 4, replacement)],
    );
    let fv = link(
        Buf::new().u32(0x10000).u32(1).u32(0).u32(0).done(),
        vec![(8, 4, conditions), (12, 4, substitutions)],
    );
    gsub.extend(fv);
    let data = font_with(vec![(b"GDEF", gdef), (b"GSUB", gsub)]);
    let font = Font::new(&data, 0).unwrap();
    let copied = chars("ABC").map_set(SetKind::NoSubsetTables, |set| {
        set.add(Tag::new(b"GSUB").to_u32());
    });
    let copied_out = subset(&font, &copied).unwrap();
    assert_eq!(out_table(&copied_out, b"GSUB"), out_table(&data, b"GSUB"));
    assert_eq!(
        &out_table(&copied_out, b"GDEF").unwrap()[..4],
        &0x10000u32.to_be_bytes()
    );
    let out = subset(&font, &chars("ABC")).unwrap();
    let gsub = out_table(&out, b"GSUB").unwrap();
    let offset = u32::from_be_bytes(gsub[10..14].try_into().unwrap()) as usize;
    let view =
        sofel_font::read::feature_variations::FeatureVariations::new(&gsub[offset..]).unwrap();
    let record = view.record(0).unwrap();
    let set = record.conditions().unwrap().unwrap();
    assert_eq!(set.condition(0).unwrap().unwrap().format(), 1);
    assert_eq!(record.substitutions().unwrap().unwrap().len(), 1);
    let gdef = out_table(&out, b"GDEF").unwrap();
    assert_eq!(&gdef[..4], &0x10000u32.to_be_bytes());
}

#[test]
fn gdef_store_with_invalid_math_constant_index_is_rejected() {
    let gdef = gdef_with_referenced_store(&[0]);
    let varidx = Buf::new().u16(0).u16(3).u16(0x8000).done();
    let constants = link(vec![0; 214], vec![(10, 2, varidx)]);
    let math = link(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
        vec![(4, 2, constants)],
    );
    let data = font_with(vec![(b"GDEF", gdef), (b"MATH", math)]);
    let font = Font::new(&data, 0).unwrap();
    assert!(matches!(
        subset(&font, &chars("A")),
        Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"GDEF")
    ));
}

#[test]
fn gdef_store_with_math_glyph_info_variation_index_follows_kept_glyphs() {
    let gdef = gdef_with_referenced_store(&[0]);
    let first_varidx = Buf::new().u16(0).u16(1).u16(0x8000).done();
    let second_varidx = Buf::new().u16(0).u16(2).u16(0x8000).done();
    let value_table = link(
        Buf::new()
            .u16(0)
            .u16(2)
            .i16(10)
            .u16(0)
            .i16(20)
            .u16(0)
            .done(),
        vec![
            (0, 2, coverage1(&[1, 2])),
            (6, 2, first_varidx),
            (10, 2, second_varidx),
        ],
    );
    let glyph_info = link(vec![0; 8], vec![(0, 2, value_table)]);
    let math = link(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
        vec![(6, 2, glyph_info)],
    );
    let data = font_with(vec![(b"GDEF", gdef), (b"MATH", math)]);
    let font = Font::new(&data, 0).unwrap();
    // The copied correction for glyph B would point beyond the two retained rows.
    let copy_math = chars("A").map_set(SetKind::NoSubsetTables, |set| {
        set.add(Tag::new(b"MATH").to_u32());
    });
    assert!(matches!(
        subset(&font, &copy_math),
        Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"GDEF")
    ));
    for (text, retained_index, item_count) in [("A", 1, 2), ("B", 0, 1)] {
        let out = subset(&font, &chars(text)).unwrap();
        let gdef = out_table(&out, b"GDEF").unwrap();
        let store = sofel_font::read::gdef::Gdef::new(&gdef)
            .unwrap()
            .variation_store()
            .unwrap()
            .unwrap();
        assert_eq!(store.item_data(0).unwrap().item_count, item_count);
        let math = out_table(&out, b"MATH").unwrap();
        let info = follow(&math, 6);
        let values = follow(info, 0);
        assert_eq!(u16_at(values, 2), 1);
        let device = follow(values, 6);
        assert_eq!(&device[..6], &[0, 0, 0, retained_index, 0x80, 0]);
    }
}

#[test]
fn gdef_store_with_math_top_accent_variation_index_is_remapped() {
    let gdef = gdef_with_referenced_store(&[0]);
    let varidx = Buf::new().u16(0).u16(2).u16(0x8000).done();
    let value_table = link(
        Buf::new().u16(0).u16(1).i16(20).u16(0).done(),
        vec![(0, 2, coverage1(&[2])), (6, 2, varidx)],
    );
    let glyph_info = link(vec![0; 8], vec![(2, 2, value_table)]);
    let math = link(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
        vec![(6, 2, glyph_info)],
    );
    let data = font_with(vec![(b"GDEF", gdef), (b"MATH", math)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("B")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let store = sofel_font::read::gdef::Gdef::new(&gdef)
        .unwrap()
        .variation_store()
        .unwrap()
        .unwrap();
    assert_eq!(store.item_data(0).unwrap().item_count, 1);
    let math = out_table(&out, b"MATH").unwrap();
    let device = follow(follow(follow(&math, 6), 2), 6);
    assert_eq!(&device[..6], &[0, 0, 0, 0, 0x80, 0]);
}

#[test]
fn gdef_store_with_math_kern_variation_index_is_remapped() {
    let gdef = gdef_with_referenced_store(&[0]);
    let varidx = Buf::new().u16(0).u16(2).u16(0x8000).done();
    let kern = link(
        Buf::new().u16(0).i16(10).u16(0).done(),
        vec![(4, 2, varidx)],
    );
    let dropped_varidx = Buf::new().u16(0).u16(1).u16(0x8000).done();
    let second_kern = link(
        Buf::new().u16(0).i16(20).u16(0).done(),
        vec![(4, 2, dropped_varidx)],
    );
    let kern_info = link(
        Buf::new()
            .u16(0)
            .u16(2)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .done(),
        vec![
            (0, 2, coverage1(&[1, 2])),
            (4, 2, kern),
            (12, 2, second_kern),
        ],
    );
    let glyph_info = link(vec![0; 8], vec![(6, 2, kern_info)]);
    let math = link(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
        vec![(6, 2, glyph_info)],
    );
    let data = font_with(vec![(b"GDEF", gdef), (b"MATH", math)]);
    let font = Font::new(&data, 0).unwrap();
    for (text, retained_index, item_count) in [("A", 1, 2), ("B", 0, 1)] {
        let out = subset(&font, &chars(text)).unwrap();
        let gdef = out_table(&out, b"GDEF").unwrap();
        let store = sofel_font::read::gdef::Gdef::new(&gdef)
            .unwrap()
            .variation_store()
            .unwrap()
            .unwrap();
        assert_eq!(store.item_data(0).unwrap().item_count, item_count);
        let math = out_table(&out, b"MATH").unwrap();
        let device = follow(follow(follow(follow(&math, 6), 6), 4), 4);
        assert_eq!(&device[..6], &[0, 0, 0, retained_index, 0x80, 0]);
    }
}

#[test]
fn gdef_store_with_math_assembly_variation_index_is_remapped() {
    let gdef = gdef_with_referenced_store(&[0]);
    let varidx = Buf::new().u16(0).u16(2).u16(0x8000).done();
    let assembly = link(
        Buf::new().i16(10).u16(0).u16(0).done(),
        vec![(2, 2, varidx)],
    );
    let construction = link(Buf::new().u16(0).u16(0).done(), vec![(0, 2, assembly)]);
    let second_varidx = Buf::new().u16(0).u16(1).u16(0x8000).done();
    let second_assembly = link(
        Buf::new().i16(20).u16(0).u16(0).done(),
        vec![(2, 2, second_varidx)],
    );
    let second_construction = link(
        Buf::new().u16(0).u16(0).done(),
        vec![(0, 2, second_assembly)],
    );
    let variants = link(
        Buf::new()
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(1)
            .u16(1)
            .u16(0)
            .u16(0)
            .done(),
        vec![
            (2, 2, coverage1(&[1])),
            (4, 2, coverage1(&[2])),
            (10, 2, construction),
            (12, 2, second_construction),
        ],
    );
    let math = link(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
        vec![(8, 2, variants)],
    );
    let data = font_with(vec![(b"GDEF", gdef), (b"MATH", math)]);
    let font = Font::new(&data, 0).unwrap();
    for (text, retained_index, item_count) in [("A", 1, 2), ("B", 0, 1)] {
        let out = subset(&font, &chars(text)).unwrap();
        let gdef = out_table(&out, b"GDEF").unwrap();
        let store = sofel_font::read::gdef::Gdef::new(&gdef)
            .unwrap()
            .variation_store()
            .unwrap()
            .unwrap();
        assert_eq!(store.item_data(0).unwrap().item_count, item_count);
        let math = out_table(&out, b"MATH").unwrap();
        let device = follow(follow(follow(follow(&math, 8), 10), 0), 2);
        assert_eq!(&device[..6], &[0, 0, 0, retained_index, 0x80, 0]);
    }
}

#[test]
fn gdef_store_with_gsub_feature_variations_remains_unsupported() {
    let gdef = gdef_with_unused_store(class_def2(&[(1, 5, 1)]));
    let gsub = Buf::new()
        .u32(0x0001_0001)
        .u16(0)
        .u16(0)
        .u16(0)
        .u32(14)
        .done();
    let data = font_with(vec![(b"GDEF", gdef), (b"GSUB", gsub)]);
    let font = Font::new(&data, 0).unwrap();
    assert!(
        matches!(subset(&font, &chars("A")), Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"GDEF"))
    );
}

fn gdef_with_referenced_store(indices: &[u16]) -> Vec<u8> {
    gdef_with_referenced_store_at(0, indices)
}

fn gdef_with_referenced_store_at(outer: u16, indices: &[u16]) -> Vec<u8> {
    let mut glyph = Buf::new().u16(u16::try_from(indices.len()).unwrap());
    for _ in indices {
        glyph = glyph.u16(0);
    }
    let glyph = link(
        glyph.done(),
        indices
            .iter()
            .enumerate()
            .map(|(i, &inner)| {
                let varidx = Buf::new().u16(outer).u16(inner).u16(0x8000).done();
                let caret = link(
                    Buf::new().u16(3).i16(10).u16(0).done(),
                    vec![(4, 2, varidx)],
                );
                (2 + 2 * i, 2, caret)
            })
            .collect(),
    );
    let carets = link(
        Buf::new().u16(0).u16(1).u16(0).done(),
        vec![(0, 2, coverage1(&[1])), (4, 2, glyph)],
    );
    let regions = Buf::new()
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(0x4000)
        .i16(0x4000)
        .done();
    let items = Buf::new()
        .u16(3)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(5)
        .i16(7)
        .i16(9)
        .done();
    let store = if outer == 0 {
        link(
            Buf::new().u16(1).u32(0).u16(1).u32(0).done(),
            vec![(2, 4, regions), (8, 4, items)],
        )
    } else {
        let other = Buf::new()
            .u16(3)
            .u16(1)
            .u16(1)
            .u16(0)
            .i16(11)
            .i16(13)
            .i16(15)
            .done();
        link(
            Buf::new().u16(1).u32(0).u16(2).u32(0).u32(0).done(),
            vec![(2, 4, regions), (8, 4, items), (12, 4, other)],
        )
    };
    link(
        Buf::new()
            .u32(0x0001_0003)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u32(0)
            .done(),
        vec![
            (4, 2, class_def2(&[(1, 1, 1)])),
            (8, 2, carets),
            (14, 4, store),
        ],
    )
}

#[test]
fn referenced_gdef_store_renumbers_retained_caret_index() {
    let data = font_with(vec![(b"GDEF", gdef_with_referenced_store(&[2]))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let actual = gdef.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        actual,
        "00010003002e00000012000000000000003600060001000c000100010001000100040003000a0006000000008000000100010001000100010000001500010000000c00010000000100000900010001000040004000"
    );
}

#[test]
fn gdef_store_keeps_two_referenced_rows_in_index_order() {
    let data = font_with(vec![(b"GDEF", gdef_with_referenced_store(&[2, 0]))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let actual = gdef.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        actual,
        "00010003003c00000012000000000000004400060001000c0001000100010002001200060003000a00060000000080000003000a0006000000018000000100010001000100010000001600010000000c0002000000010000050900010001000040004000"
    );
}

#[test]
fn gdef_store_drops_when_no_referenced_caret_is_retained() {
    let data = font_with(vec![(b"GDEF", gdef_with_referenced_store(&[2]))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("B")).unwrap();
    assert!(out_table(&out, b"GDEF").is_none());
}

#[test]
fn gdef_store_remaps_second_outer_subtable_to_zero() {
    let data = font_with(vec![(b"GDEF", gdef_with_referenced_store_at(1, &[2]))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let gdef = out_table(&out, b"GDEF").unwrap();
    let actual = gdef.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        actual,
        "00010003002e00000012000000000000003600060001000c000100010001000100040003000a0006000000008000000100010001000100010000001500010000000c00010000000100000f00010001000040004000"
    );
}

#[test]
fn gdef_store_with_unknown_caret_device_remains_unsupported() {
    let mut gdef = gdef_with_referenced_store(&[2]);
    let at = gdef
        .windows(6)
        .position(|bytes| bytes == [0, 0, 0, 2, 0x80, 0])
        .unwrap();
    gdef.get_mut(at + 4..at + 6)
        .unwrap()
        .copy_from_slice(&4u16.to_be_bytes());
    let data = font_with(vec![(b"GDEF", gdef)]);
    let font = Font::new(&data, 0).unwrap();
    assert!(
        matches!(subset(&font, &chars("A")), Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"GDEF"))
    );
}

fn glyph_class_def(out: &[u8]) -> Vec<u8> {
    let gdef = out_table(out, b"GDEF").unwrap();
    let cd = follow(&gdef, 4);
    let len = match u16_at(cd, 0) {
        1 => 6 + 2 * u16_at(cd, 4) as usize,
        _ => 4 + 6 * u16_at(cd, 2) as usize,
    };
    cd[..len].to_vec()
}

#[test]
fn class_def_formats() {
    // One range of one class: format 2 (10 bytes) beats format 1 (16).
    let data = font_with(vec![(b"GDEF", gdef_with_classes(class_def2(&[(1, 5, 1)])))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("ABCDE")).unwrap();
    assert_eq!(glyph_class_def(&out), vec![0, 2, 0, 1, 0, 1, 0, 5, 0, 1]);
    // Two glyphs: both formats take 10 bytes and format 1 wins the tie.
    let out = subset(&font, &chars("AC")).unwrap();
    assert_eq!(glyph_class_def(&out), vec![0, 1, 0, 1, 0, 2, 0, 1, 0, 1]);
    // Alternating classes: format 1 (12 bytes) beats three ranges (22).
    let data = font_with(vec![(
        b"GDEF",
        gdef_with_classes(class_def1(1, &[1, 2, 1])),
    )]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("ABC")).unwrap();
    assert_eq!(
        glyph_class_def(&out),
        vec![0, 1, 0, 1, 0, 3, 0, 1, 0, 2, 0, 1]
    );
    // No classified glyph is kept: GDEF has nothing left and is dropped.
    let out = subset(&font, &chars("X")).unwrap();
    assert!(out_table(&out, b"GDEF").is_none());
}

#[test]
fn gpos_kept_glyphs_only() {
    // A SinglePos on A and B; only A is kept.
    let st = link(
        Buf::new().u16(1).u16(0).u16(0x0004).i16(-30).done(),
        vec![(2, 2, coverage1(&[g('A'), g('B')]))],
    );
    let gpos = layout_table(&[(b"kern", &[0])], &[(1, st)]);
    let data = font_with(vec![(b"GPOS", gpos)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert_eq!(first_coverage(&gpos), vec![0, 1, 0, 1, 0, 1]);
    // Nothing positioned is kept: the lookup is kept without subtables
    // and the feature that used it is pruned.
    let out = subset(&font, &chars("Z")).unwrap();
    let gpos = out_table(&out, b"GPOS").unwrap();
    assert_eq!(u16_at(follow(&gpos, 6), 0), 0);
    assert_eq!(u16_at(follow(&gpos, 8), 0), 0);
}

#[test]
fn adr0022_wide_coverage_retains_single_substitutions() {
    for coverage in [
        vec![0, 3, 0, 0, 2, 0, 0, 1, 0, 0, 2],
        vec![0, 4, 0, 0, 1, 0, 0, 1, 0, 0, 2, 0, 0, 0],
    ] {
        let subtable = link(
            Buf::new().u16(1).u16(0).u16(4).done(),
            vec![(2, 2, coverage)],
        );
        let data = font_with(vec![(
            b"GSUB",
            layout_table(&[(b"liga", &[0])], &[(1, subtable)]),
        )]);
        let out = subset(&Font::new(&data, 0).unwrap(), &chars("AB")).unwrap();
        assert_eq!(num_glyphs(&out), 5);
        assert_eq!(
            first_coverage(&out_table(&out, b"GSUB").unwrap()),
            coverage1(&[1, 2])
        );
    }
}

#[test]
fn adr0022_wide_class_definitions_keep_glyph_classes() {
    for classes in [
        vec![0, 3, 0, 0, 1, 0, 0, 2, 0, 0, 1, 0, 0, 1],
        vec![0, 4, 0, 0, 1, 0, 0, 1, 0, 0, 2, 0, 1],
    ] {
        let data = font_with(vec![(b"GDEF", gdef_with_classes(classes))]);
        let out = subset(&Font::new(&data, 0).unwrap(), &chars("AB")).unwrap();
        assert_eq!(glyph_class_def(&out), vec![0, 1, 0, 1, 0, 2, 0, 1, 0, 1]);
    }
}

#[test]
fn adr0022_wide_single_substitution_retains_output_glyphs() {
    for mut st in [
        vec![0, 3, 0, 0, 0, 9, 0, 0, 4],
        vec![0, 4, 0, 0, 0, 15, 0, 0, 2, 0, 0, 5, 0, 0, 6],
    ] {
        st.extend(coverage1(&[1, 2]));
        let data = font_with(vec![(
            b"GSUB",
            layout_table(&[(b"liga", &[0])], &[(1, st)]),
        )]);
        let out = subset(&Font::new(&data, 0).unwrap(), &chars("AB")).unwrap();
        assert_eq!(num_glyphs(&out), 5);
        assert_eq!(
            first_coverage(&out_table(&out, b"GSUB").unwrap()),
            coverage1(&[1, 2])
        );
    }
}

#[test]
fn adr0022_wide_single_position_values_survive_subsetting() {
    for (mut st, expected) in [
        (
            vec![0, 3, 0, 0, 0, 10, 0, 4, 255, 226],
            vec![0, 1, 0, 8, 0, 4, 255, 226],
        ),
        (
            vec![0, 4, 0, 0, 0, 15, 0, 4, 0, 0, 2, 255, 226, 0, 25],
            vec![0, 2, 0, 12, 0, 4, 0, 2, 255, 226, 0, 25],
        ),
    ] {
        st.extend(coverage1(&[1, 2]));
        let data = font_with(vec![(
            b"GPOS",
            layout_table(&[(b"kern", &[0])], &[(1, st)]),
        )]);
        let out = subset(&Font::new(&data, 0).unwrap(), &chars("AB")).unwrap();
        let table = out_table(&out, b"GPOS").unwrap();
        let list = follow(&table, 8);
        assert_eq!(u16_at(list, 0), 1);
        assert!(follow(follow(list, 2), 6).starts_with(&expected));
    }
}

#[test]
fn adr0022_wide_source_keeps_layout_for_low_glyphs() {
    let data = font_with(vec![
        (b"MAXP", vec![0, 0, 80, 0, 1, 0, 1]),
        (
            b"GSUB",
            layout_table(&[(b"liga", &[0])], &[(1, single(&[1, 2], 4))]),
        ),
    ]);
    let font = Font::new(&data, 0).unwrap();
    assert_eq!(font.num_glyphs(), 65537);
    let out = subset(&font, &chars("AB")).unwrap();
    assert_eq!(num_glyphs(&out), 5);
    assert_eq!(
        first_coverage(&out_table(&out, b"GSUB").unwrap()),
        coverage1(&[1, 2])
    );
}

#[test]
fn adr0022_wide_source_substitution_maps_high_input_to_compact_ids() {
    // Coverage contains glyph 65536; the 24-bit delta wraps to glyph 1.
    let st = vec![0, 3, 0, 0, 0, 9, 255, 0, 1, 0, 3, 0, 0, 1, 1, 0, 0];
    let data = font_with(vec![
        (b"MAXP", vec![0, 0, 80, 0, 1, 0, 1]),
        (b"GSUB", layout_table(&[(b"liga", &[0])], &[(1, st)])),
    ]);
    let input = Input::new().map_set(SetKind::Glyphs, |set| {
        set.add(65536);
    });
    let out = subset(&Font::new(&data, 0).unwrap(), &input).unwrap();
    assert_eq!(num_glyphs(&out), 3);
    let gsub = out_table(&out, b"GSUB").unwrap();
    assert_eq!(first_coverage(&gsub), coverage1(&[2]));
    let subtable = follow(follow(follow(&gsub, 8), 2), 6);
    assert_eq!(u16_at(subtable, 4), 65535);
}

#[test]
fn adr0022_retained_high_gid_keeps_single_substitution() {
    let st = vec![0, 3, 0, 0, 0, 9, 0, 255, 255, 0, 1, 0, 1, 0, 1];
    let mut tables = sample_tables();
    tables
        .iter_mut()
        .find(|(tag, _)| *tag == b"hmtx")
        .unwrap()
        .1
        .resize(2 * 65537 + 2 * 20, 0);
    let classes = vec![0, 3, 1, 0, 0, 0, 0, 1, 0, 0, 2];
    tables.extend([
        (b"GDEF", gdef_with_classes(classes.clone())),
        (b"MAXP", vec![0, 0, 80, 0, 1, 0, 1]),
        (b"GSUB", layout_table(&[(b"liga", &[0])], &[(1, st)])),
    ]);
    tables.sort_by_key(|(tag, _)| **tag);
    let data = sfnt(&tables);
    let input = chars("A").with_flags(Flags::RETAIN_GIDS);
    let out = subset(&Font::new(&data, 0).unwrap(), &input).unwrap();
    assert_eq!(Font::new(&out, 0).unwrap().num_glyphs(), 65537);
    let gdef = out_table(&out, b"GDEF").unwrap();
    assert!(follow(&gdef, 4).starts_with(&classes));
    let gsub = out_table(&out, b"GSUB").unwrap();
    let st = follow(follow(follow(&gsub, 8), 2), 6);
    assert!(st.starts_with(&[0, 3, 0, 0, 0, 9, 0, 255, 255]));
}
