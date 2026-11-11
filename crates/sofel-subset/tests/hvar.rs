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

fn two_item_data_store() -> Vec<u8> {
    let mut store = Vec::new();
    store.extend(1u16.to_be_bytes());
    store.extend(16u32.to_be_bytes());
    store.extend(2u16.to_be_bytes());
    store.extend(26u32.to_be_bytes());
    store.extend(37u32.to_be_bytes());
    store.extend(1u16.to_be_bytes());
    store.extend(1u16.to_be_bytes());
    store.extend([0, 0, 0x40, 0, 0x40, 0]);
    for values in [[0, 1, 2], [10, 20, 30]] {
        store.extend(3u16.to_be_bytes());
        store.extend(0u16.to_be_bytes());
        store.extend(1u16.to_be_bytes());
        store.extend(0u16.to_be_bytes());
        store.extend(values);
    }
    store
}

fn font_with_two_item_data(tag: &'static [u8; 4]) -> Vec<u8> {
    let header = if tag == b"VVAR" { 24u32 } else { 20u32 };
    let advance = [0, 1, 0, 3, 6, 1, 4];
    let side = [0, 1, 0, 3, 0, 5, 2];
    let mut variation = Vec::new();
    variation.extend(0x0001_0000u32.to_be_bytes());
    variation.extend((if tag == b"VVAR" { 38u32 } else { 34u32 }).to_be_bytes());
    variation.extend(header.to_be_bytes());
    variation.extend((if tag == b"VVAR" { 31u32 } else { 27u32 }).to_be_bytes());
    for _ in 0..if tag == b"VVAR" { 2 } else { 1 } {
        variation.extend(0u32.to_be_bytes());
    }
    variation.extend(advance);
    variation.extend(side);
    variation.extend(two_item_data_store());
    let mut tables = sample_tables();
    tables.push((tag, variation));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn adr0022_hvar_multiple_item_data_preserves_advance_and_side_deltas() {
    let data = font_with_two_item_data(b"HVAR");
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let hvar = Hvar::new(output.table(Tag::new(b"HVAR")).unwrap()).unwrap();
    let store = hvar.store().unwrap();
    let advance = hvar.advance_map().unwrap().unwrap();
    let side = hvar.lsb_map().unwrap().unwrap();
    assert_eq!(store.data_count(), 2);
    for (gid, expected) in [(0, 30), (1, 10)] {
        let index = advance.map(gid);
        assert_eq!(
            store.delta(
                (index >> 16) as u16,
                u16::try_from(index & 0xffff).unwrap(),
                0
            ),
            Some(expected)
        );
    }
    let index = side.map(0);
    assert_eq!(
        store.delta(
            (index >> 16) as u16,
            u16::try_from(index & 0xffff).unwrap(),
            0
        ),
        Some(0)
    );
}

#[test]
fn adr0022_vvar_multiple_item_data_preserves_advance_and_side_deltas() {
    let data = font_with_two_item_data(b"VVAR");
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let vvar = Vvar::new(output.table(Tag::new(b"VVAR")).unwrap()).unwrap();
    let store = vvar.store().unwrap();
    let advance = vvar.advance_map().unwrap().unwrap();
    let side = vvar.tsb_map().unwrap().unwrap();
    assert_eq!(store.data_count(), 2);
    let index = advance.map(1);
    assert_eq!(
        store.delta(
            (index >> 16) as u16,
            u16::try_from(index & 0xffff).unwrap(),
            0
        ),
        Some(10)
    );
    let index = side.map(1);
    assert_eq!(
        store.delta(
            (index >> 16) as u16,
            u16::try_from(index & 0xffff).unwrap(),
            0
        ),
        Some(2)
    );
}

#[test]
fn adr0022_hvar_multiple_item_data_with_retain_gids() {
    let data = font_with_two_item_data(b"HVAR");
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Glyphs, |glyphs| {
            glyphs.add(2);
        })
        .with_flags(Flags::RETAIN_GIDS);
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let hvar = Hvar::new(output.table(Tag::new(b"HVAR")).unwrap()).unwrap();
    let store = hvar.store().unwrap();
    let advance = hvar.advance_map().unwrap().unwrap();
    let index = advance.map(2);
    assert_eq!(
        store.delta(
            (index >> 16) as u16,
            u16::try_from(index & 0xffff).unwrap(),
            0
        ),
        Some(10)
    );
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

fn font_with_explicit_maps(tag: &'static [u8; 4], side_identity: bool) -> Vec<u8> {
    let mut variation = Vec::new();
    let header = if tag == b"VVAR" { 24u32 } else { 20u32 };
    let advance = [0, 1, 0, 3, 2, 0, 1];
    let side = if side_identity {
        [0, 1, 0, 3, 0, 1, 2]
    } else {
        [0, 1, 0, 3, 1, 2, 0]
    };
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
    let data = font_with_explicit_maps(b"HVAR", false);
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
    let data = font_with_explicit_maps(b"VVAR", false);
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
fn adr0022_vvar_shared_advance_and_vertical_origin_map_is_serialized_once() {
    let mut data = font_with_explicit_maps(b"VVAR", false);
    let directory = sofel_font::read::sfnt::Sfnt::new(&data).unwrap();
    let table = directory.record(Tag::new(b"VVAR")).unwrap().offset as usize;
    let advance_offset = data[table + 8..table + 12].to_vec();
    data[table + 20..table + 24].copy_from_slice(&advance_offset);
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| glyphs.add(2));
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let vvar = output.table(Tag::new(b"VVAR")).unwrap();
    assert_ne!(&vvar[8..12], &[0; 4]);
    assert_eq!(&vvar[8..12], &vvar[20..24]);
    let vvar = Vvar::new(vvar).unwrap();
    let advance = vvar.advance_map().unwrap().unwrap();
    let origin = vvar.vorg_map().unwrap().unwrap();
    assert_eq!((advance.map(0), advance.map(1)), (origin.map(0), origin.map(1)));
}

#[test]
fn adr0022_hvar_explicit_side_map_is_kept_when_identity() {
    let data = font_with_explicit_maps(b"HVAR", true);
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(1);
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let hvar = Hvar::new(output.table(Tag::new(b"HVAR")).unwrap()).unwrap();
    let side = hvar.lsb_map().unwrap().unwrap();
    assert_eq!((side.map(0), side.map(1), side.map(2)), (0, 1, 2));
}

#[test]
fn adr0022_hvar_explicit_maps_with_retain_gids() {
    let data = font_with_explicit_maps(b"HVAR", false);
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Glyphs, |glyphs| {
            glyphs.add(2);
        })
        .with_flags(Flags::RETAIN_GIDS);
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let hvar = Hvar::new(output.table(Tag::new(b"HVAR")).unwrap()).unwrap();
    let advance = hvar.advance_map().unwrap().unwrap();
    let side = hvar.lsb_map().unwrap().unwrap();
    assert_eq!((advance.map(0), advance.map(1), advance.map(2)), (2, 0, 1));
    assert_eq!((side.map(0), side.map(1), side.map(2)), (1, 0, 0));
}

#[test]
fn adr0022_explicit_metric_maps_trim_repeated_tail_across_gid_holes() {
    for tag in [b"HVAR", b"VVAR"] {
        let data = font_with_explicit_maps(tag, false);
        let font = Font::new(&data, 0).unwrap();
        let selection = Input::new().map_set(SetKind::Glyphs, |glyphs| {
            glyphs.add(2);
            glyphs.add(5);
        });
        for (input, count) in [
            (selection.clone(), 2),
            (selection.with_flags(Flags::RETAIN_GIDS), 3),
        ] {
            let output = subset(&font, &input).unwrap();
            let font = Font::new(&output, 0).unwrap();
            let bytes = font.table(Tag::new(tag)).unwrap();
            let map = if tag == b"HVAR" {
                Hvar::new(bytes).unwrap().advance_map()
            } else {
                Vvar::new(bytes).unwrap().advance_map()
            }
            .unwrap()
            .unwrap();
            assert_eq!(map.len(), count);
            assert_eq!(map.map(5), map.map(count - 1));
        }
    }
}

#[test]
fn adr0022_empty_optional_metric_map_has_no_output_offset() {
    for tag in [b"HVAR", b"VVAR"] {
        let mut data = font_with_explicit_maps(tag, false);
        let directory = sofel_font::read::sfnt::Sfnt::new(&data).unwrap();
        let table = directory.record(Tag::new(tag)).unwrap().offset as usize;
        let first_side = if tag == b"HVAR" { 27 } else { 31 };
        data[table + first_side + 2..table + first_side + 4].fill(0);
        let font = Font::new(&data, 0).unwrap();
        let input = Input::new().map_set(SetKind::Glyphs, |g| g.add(2));
        let output = subset(&font, &input).unwrap();
        let result = Font::new(&output, 0).unwrap();
        let bytes = result.table(Tag::new(tag)).unwrap();
        assert_eq!(&bytes[12..16], &[0; 4]);
    }
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
