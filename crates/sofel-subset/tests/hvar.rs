//! HVAR/VVAR subsetting without instancing (ADR 0022 stage 10).

#![allow(clippy::all, clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{sample_tables, sfnt};
use sofel_font::Font;
use sofel_font::read::{
    Tag,
    hvar::{Hvar, Vvar},
};
use sofel_subset::{Flags, Input, SetKind, subset};

fn item_store() -> Vec<u8> {
    let mut store = Vec::new();
    store.extend(1u16.to_be_bytes());
    store.extend(12u32.to_be_bytes());
    store.extend(1u16.to_be_bytes());
    store.extend(22u32.to_be_bytes());
    store.extend(1u16.to_be_bytes());
    store.extend(1u16.to_be_bytes());
    store.extend([0, 0, 0x40, 0, 0x40, 0]);
    store.extend(3u16.to_be_bytes());
    store.extend(0u16.to_be_bytes());
    store.extend(1u16.to_be_bytes());
    store.extend(0u16.to_be_bytes());
    store.extend([0, 1, 2]);
    store
}

fn font_with_hvar(tag: &'static [u8; 4]) -> Vec<u8> {
    let mut variation = Vec::new();
    variation.extend(0x0001_0000u32.to_be_bytes());
    variation.extend((if tag == b"VVAR" { 24u32 } else { 20u32 }).to_be_bytes());
    for _ in 0..if tag == b"VVAR" { 4 } else { 3 } {
        variation.extend(0u32.to_be_bytes());
    }
    variation.extend(item_store());
    let mut tables = sample_tables();
    tables.push((tag, variation));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn font_with_explicit_maps(tag: &'static [u8; 4]) -> Vec<u8> {
    let mut variation = Vec::new();
    let header = if tag == b"VVAR" { 24u32 } else { 20u32 };
    let advance = [0, 1, 0, 3, 2, 0, 1];
    let side = [0, 1, 0, 3, 1, 2, 0];
    variation.extend(0x0001_0000u32.to_be_bytes());
    variation.extend((if tag == b"VVAR" { 38u32 } else { 34u32 }).to_be_bytes());
    variation.extend(header.to_be_bytes());
    variation.extend((if tag == b"VVAR" { 31u32 } else { 27u32 }).to_be_bytes());
    for _ in 0..if tag == b"VVAR" { 2 } else { 1 } {
        variation.extend(0u32.to_be_bytes());
    }
    variation.extend(advance);
    variation.extend(side);
    variation.extend(item_store());
    let mut tables = sample_tables();
    tables.push((tag, variation));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn adr0022_hvar_explicit_advance_and_side_maps_follow_retained_glyphs() {
    let data = font_with_explicit_maps(b"HVAR");
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let hvar = Hvar::new(output.table(Tag::new(b"HVAR")).unwrap()).unwrap();
    let advance = hvar.advance_map().unwrap().unwrap();
    let side = hvar.lsb_map().unwrap().unwrap();
    let store = hvar.store().unwrap();
    let data = store.item_data(0).unwrap();
    assert_eq!(
        data.delta(u16::try_from(advance.map(0)).unwrap(), 0),
        Some(2)
    );
    assert_eq!(
        data.delta(u16::try_from(advance.map(1)).unwrap(), 0),
        Some(1)
    );
    assert_eq!(data.delta(u16::try_from(side.map(1)).unwrap(), 0), Some(0));
}

#[test]
fn adr0022_vvar_explicit_advance_and_side_maps_follow_retained_glyphs() {
    let data = font_with_explicit_maps(b"VVAR");
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let vvar = Vvar::new(output.table(Tag::new(b"VVAR")).unwrap()).unwrap();
    let advance = vvar.advance_map().unwrap().unwrap();
    let side = vvar.tsb_map().unwrap().unwrap();
    let store = vvar.store().unwrap();
    let data = store.item_data(0).unwrap();
    assert_eq!(
        data.delta(u16::try_from(advance.map(1)).unwrap(), 0),
        Some(1)
    );
    assert_eq!(data.delta(u16::try_from(side.map(1)).unwrap(), 0), Some(0));
}

#[test]
fn adr0022_hvar_implicit_advance_map_follows_new_glyph_ids() {
    let data = font_with_hvar(b"HVAR");
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let hvar = output.table(Tag::new(b"HVAR")).unwrap();
    assert_eq!(
        &hvar[..20],
        &[0, 1, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    let store = &hvar[20..];
    assert_eq!(&store[..12], &[0, 1, 0, 0, 0, 22, 0, 1, 0, 0, 0, 12]);
    assert_eq!(&store[12..22], &[0, 2, 0, 0, 0, 1, 0, 0, 0, 2]);
}

#[test]
fn adr0022_vvar_implicit_advance_map_follows_new_glyph_ids() {
    let data = font_with_hvar(b"VVAR");
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let vvar = output.table(Tag::new(b"VVAR")).unwrap();
    assert_eq!(
        &vvar[..24],
        &[
            0, 1, 0, 0, 0, 0, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );
    assert_eq!(&vvar[36..46], &[0, 2, 0, 0, 0, 1, 0, 0, 0, 2]);
}

#[test]
fn adr0022_hvar_retain_gids_writes_advance_map_for_holes() {
    let data = font_with_hvar(b"HVAR");
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Glyphs, |glyphs| {
            glyphs.add(2);
        })
        .with_flags(Flags::RETAIN_GIDS);
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let hvar = Hvar::new(output.table(Tag::new(b"HVAR")).unwrap()).unwrap();
    let map = hvar.advance_map().unwrap().unwrap();
    assert_eq!((map.map(0), map.map(1), map.map(2)), (0, 0, 1));
}
