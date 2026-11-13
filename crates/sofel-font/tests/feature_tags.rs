//! sofel ADR 0075 I1: table-local FeatureList pagination.
#![allow(clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{
    feature_list, gsubgpos, lang_sys, lookup_list, sample_tables, script, script_list, sfnt,
};
use sofel_font::read::Tag;
use sofel_font::{Font, LayoutTable};

fn layout(features: &[&[u8; 4]]) -> Vec<u8> {
    let indices: Vec<u16> = (0..features.len())
        .map(|i| u16::try_from(i).unwrap())
        .collect();
    let records: Vec<(&[u8; 4], &[u16])> = features.iter().map(|tag| (*tag, &[][..])).collect();
    gsubgpos(
        0x0001_0000,
        script_list(vec![(b"DFLT", script(lang_sys(0xffff, &indices), vec![]))]),
        feature_list(&records),
        lookup_list(vec![], false),
    )
}

fn font(gsub: Option<Vec<u8>>, gpos: Option<Vec<u8>>) -> Vec<u8> {
    let mut tables = sample_tables();
    if let Some(data) = gsub {
        tables.push((b"GSUB", data));
    }
    if let Some(data) = gpos {
        tables.push((b"GPOS", data));
    }
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn inv1_feature_tags_preserves_table_order_duplicates_and_page() {
    let bytes = font(
        Some(layout(&[b"kern", b"kern", b"vert"])),
        Some(layout(&[b"mark"])),
    );
    let face = Font::new(&bytes, 0).unwrap();
    let mut out = [Tag::new(b"zzzz"); 2];
    assert_eq!(face.feature_tags(LayoutTable::Gsub, 0, &mut out), (2, 3));
    assert_eq!(out, [Tag::new(b"kern"), Tag::new(b"kern")]);
    assert_eq!(face.feature_tags(LayoutTable::Gsub, 2, &mut out), (1, 3));
    assert_eq!(out, [Tag::new(b"vert"), Tag::new(b"kern")]);
    assert_eq!(face.feature_tags(LayoutTable::Gpos, 0, &mut out), (1, 1));
    assert_eq!(out[0], Tag::new(b"mark"));
}

#[test]
fn inv1_feature_tags_empty_and_discarded_tables() {
    let mut bad = layout(&[b"kern"]);
    bad.truncate(8);
    let bytes = font(Some(bad), Some(layout(&[b"mark"])));
    let face = Font::new(&bytes, 0).unwrap();
    let mut out = [Tag::new(b"zzzz"); 1];
    assert_eq!(face.feature_tags(LayoutTable::Gsub, 0, &mut out), (0, 0));
    assert_eq!(out[0], Tag::new(b"zzzz"));
    assert_eq!(face.feature_tags(LayoutTable::Gpos, 0, &mut out), (1, 1));
    assert_eq!(out[0], Tag::new(b"mark"));
    assert_eq!(
        face.feature_tags(LayoutTable::Gpos, u32::MAX, &mut out),
        (0, 1)
    );
    assert_eq!(out[0], Tag::new(b"mark"));
    assert_eq!(face.feature_tags(LayoutTable::Gpos, 0, &mut []), (0, 1));
    let absent = font(None, None);
    assert_eq!(
        Font::new(&absent, 0)
            .unwrap()
            .feature_tags(LayoutTable::Gsub, 0, &mut out),
        (0, 0)
    );
}
