//! The input, the plan and the stage gating on hand-made fonts (ADR 0022).

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use std::collections::BTreeMap;

use build::*;
use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_subset::{
    DEFAULT_DROP_TABLES, DEFAULT_LAYOUT_FEATURES, DEFAULT_NO_SUBSET_TABLES, Flags, Input, Plan,
    RangeSet, SetKind, SubsetError, subset,
};

fn tags(s: &RangeSet) -> Vec<Tag> {
    s.iter().map(Tag::from_u32).collect()
}

#[test]
fn input_defaults() {
    let i = Input::new();
    assert_eq!(i.flags(), Flags::DEFAULT);
    assert_eq!(
        i.set(SetKind::NameIds).iter().collect::<Vec<_>>(),
        (0..=6).collect::<Vec<_>>()
    );
    assert_eq!(
        i.set(SetKind::NameLanguages).iter().collect::<Vec<_>>(),
        vec![0x409]
    );
    let mut drop: Vec<Tag> = DEFAULT_DROP_TABLES.iter().map(|t| Tag::new(t)).collect();
    drop.sort();
    assert_eq!(tags(i.set(SetKind::DropTables)), drop);
    let mut copy: Vec<Tag> = DEFAULT_NO_SUBSET_TABLES
        .iter()
        .map(|t| Tag::new(t))
        .collect();
    copy.sort();
    assert_eq!(tags(i.set(SetKind::NoSubsetTables)), copy);
    let mut feats: Vec<Tag> = DEFAULT_LAYOUT_FEATURES
        .iter()
        .map(|t| Tag::new(t))
        .collect();
    feats.sort();
    feats.dedup();
    assert_eq!(tags(i.set(SetKind::LayoutFeatures)), feats);
    assert!(i.set(SetKind::LayoutScripts).is_inverted());
    assert!(i.set(SetKind::Glyphs).is_empty() && i.set(SetKind::Unicodes).is_empty());

    let k = Input::new().keep_everything();
    for kind in [
        SetKind::Unicodes,
        SetKind::Glyphs,
        SetKind::NameIds,
        SetKind::NameLanguages,
        SetKind::LayoutFeatures,
        SetKind::LayoutScripts,
    ] {
        assert_eq!(k.set(kind), &RangeSet::all());
    }
    assert!(k.set(SetKind::DropTables).is_empty());
    assert_eq!(k.flags(), Flags(0x40 | 0x80 | 0x8 | 0x100 | 0x20));
}

fn with_glyf() -> Vec<u8> {
    let a = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(100, 0), Pt::On(100, 100)]],
        false,
        &[1, 2],
    );
    let c = composite_glyph(&[comp(1, Args::Xy(10, 10))], [10, 10, 110, 110], false);
    let glyphs: Vec<Vec<u8>> = (0..28)
        .map(|g| match g {
            0 | 1 => a.clone(),
            2 => c.clone(),
            _ => Vec::new(),
        })
        .collect();
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for g in &glyphs {
        loca.extend_from_slice(&((glyf.len() / 2) as u16).to_be_bytes());
        glyf.extend_from_slice(g);
        if glyf.len() % 2 == 1 {
            glyf.push(0);
        }
    }
    loca.extend_from_slice(&((glyf.len() / 2) as u16).to_be_bytes());
    let mut t = sample_tables();
    t.push((b"glyf", glyf));
    t.push((b"loca", loca));
    t.sort_by_key(|(t, _)| **t);
    sfnt(&t)
}

#[test]
fn plan_mappings() {
    let data = with_glyf();
    let font = Font::new(&data, 0).unwrap();
    // 'B' maps to the composite glyph 2, which pulls in glyph 1.
    let input = Input::new().map_set(SetKind::Unicodes, |s| s.add(0x42));
    let plan = Plan::new(&font, &input).unwrap();
    assert_eq!(plan.new_to_old_glyphs(), &[(0, 0), (1, 1), (2, 2)]);
    assert_eq!(
        plan.unicode_to_old_glyphs().collect::<Vec<_>>(),
        vec![(0x42, 2)]
    );
    // Retained glyph ids.
    let input = Input::new()
        .map_set(SetKind::Unicodes, |s| s.add(0x45))
        .with_flags(Flags::RETAIN_GIDS);
    let plan = Plan::new(&font, &input).unwrap();
    assert_eq!(plan.new_to_old_glyphs(), &[(0, 0), (5, 5)]);
    assert_eq!(plan.num_output_glyphs(), 6);
    let plan = Plan::new(
        &font,
        &input.with_flags(Flags::RETAIN_GIDS | Flags::RETAIN_NUM_GLYPHS),
    )
    .unwrap();
    assert_eq!(plan.num_output_glyphs(), 28);
    // Glyphs beyond the count are dropped.
    let input = Input::new().map_set(SetKind::Glyphs, |s| {
        s.add(3);
        s.add(100);
    });
    let plan = Plan::new(&font, &input).unwrap();
    assert_eq!(plan.new_to_old_glyphs(), &[(0, 0), (1, 3)]);
    // The two ways of collecting characters agree: requesting glyph 3
    // also keeps its character.
    assert_eq!(
        plan.unicode_to_old_glyphs().collect::<Vec<_>>(),
        vec![(0x43, 3)]
    );
    // A requested mapping.
    let map = BTreeMap::from([(3u32, 7u32)]);
    let input = Input::new()
        .map_set(SetKind::Glyphs, |s| {
            s.add(3);
            s.add(4);
        })
        .with_glyph_mapping(map);
    let plan = Plan::new(&font, &input).unwrap();
    assert_eq!(plan.new_to_old_glyphs(), &[(0, 0), (7, 3), (8, 4)]);
    assert_eq!(plan.num_output_glyphs(), 9);
}

#[test]
fn invalid_inputs() {
    let data = with_glyf();
    let font = Font::new(&data, 0).unwrap();
    let g = |s: &mut RangeSet| {
        s.add(3);
        s.add(4);
    };
    let dup = Input::new()
        .map_set(SetKind::Glyphs, g)
        .with_glyph_mapping(BTreeMap::from([(3, 5), (4, 5)]));
    assert_eq!(
        Plan::new(&font, &dup).err(),
        Some(SubsetError::InvalidInput)
    );
    let zero = Input::new()
        .map_set(SetKind::Glyphs, g)
        .with_glyph_mapping(BTreeMap::from([(3, 0)]));
    assert_eq!(
        Plan::new(&font, &zero).err(),
        Some(SubsetError::InvalidInput)
    );
    let retain = Input::new()
        .map_set(SetKind::Glyphs, g)
        .with_glyph_mapping(BTreeMap::from([(3, 5)]))
        .with_flags(Flags::RETAIN_GIDS);
    assert_eq!(
        Plan::new(&font, &retain).err(),
        Some(SubsetError::InvalidInput)
    );
}

#[test]
fn malformed_varc_is_rejected_unless_dropped() {
    let mut t = sample_tables();
    t.push((b"VARC", vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 0]));
    t.sort_by_key(|(t, _)| **t);
    let data = sfnt(&t);
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Unicodes, |s| s.add(0x41));
    assert!(matches!(
        subset(&font, &input),
        Err(SubsetError::Malformed { tag }) if tag == Tag::new(b"VARC")
    ));
    let dropped = input.map_set(SetKind::DropTables, |s| s.add(Tag::new(b"VARC").to_u32()));
    let out = subset(&font, &dropped).unwrap();
    assert!(
        sofel_font::read::sfnt::Sfnt::new(&out)
            .unwrap()
            .table(Tag::new(b"VARC"))
            .is_none()
    );
}

#[test]
fn deterministic() {
    let data = with_glyf();
    let font = Font::new(&data, 0).unwrap();
    let a = Input::new().map_set(SetKind::Unicodes, |s| {
        s.add(0x42);
        s.add(0x41);
        s.add(0x1F600);
    });
    let b = Input::new().map_set(SetKind::Unicodes, |s| {
        s.add(0x1F600);
        s.add(0x41);
        s.add(0x42);
    });
    let x = subset(&font, &a).unwrap();
    assert_eq!(x, subset(&font, &a).unwrap());
    assert_eq!(x, subset(&font, &b).unwrap());
    let f = Font::new(&x, 0).unwrap();
    assert_eq!(f.num_glyphs(), 4);
    assert_eq!(f.nominal_glyph(0x42).map(|g| g.0), Some(2));
}

#[test]
fn adr0022_name_overrides_replace_insert_and_remove_records() {
    let mut tables = sample_tables_with_glyf();
    tables.retain(|(tag, _)| **tag != *b"name");
    tables.push((
        b"name",
        name(
            0,
            &[(3, 1, 0x409, 1, b"old"), (3, 1, 0x409, 2, b"gone")],
            &[],
        ),
    ));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let input = Input::new()
        .with_name_override((3, 1, 0x409, 1), "字😀".as_bytes().to_vec())
        .with_name_override((3, 1, 0x409, 2), Vec::new())
        .with_name_override((1, 0, 0, 55), b"Mac".to_vec());
    let result = subset(&font, &input).unwrap();
    let output = Font::new(&result, 0).unwrap();
    let names =
        sofel_font::read::name::Name::new(output.table(Tag::new(b"name")).unwrap()).unwrap();
    let rows: Vec<_> = names
        .records()
        .map(|r| (r.name_id, names.string(&r).unwrap().to_vec()))
        .collect();
    assert_eq!(rows, [(55, b"Mac".to_vec()), (1, utf16be("字😀"))]);
}
