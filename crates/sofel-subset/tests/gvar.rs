//! Glyph variation table subsetting (ADR 0022 stage 10).

#![allow(clippy::all, clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{sample_tables, sfnt};
use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_subset::{Flags, Input, SetKind, subset};

fn font_with_gvar(large: bool) -> Vec<u8> {
    font_with_shared_offset(large, if large { 137 } else { 136 })
}

fn font_with_shared_offset(large: bool, shared_offset: u32) -> Vec<u8> {
    // Long source offsets allow an odd-length record. The output is short
    // and pads that record to an even length.
    let mut gvar = vec![0, 1, 0, 0, 0, 1, 0, 1];
    gvar.extend(shared_offset.to_be_bytes());
    if large {
        gvar.extend([0, 0, 28]);
    } else {
        gvar.extend(28u16.to_be_bytes());
    }
    gvar.extend(1u16.to_be_bytes());
    gvar.extend((if large { 139u32 } else { 138u32 }).to_be_bytes());
    for i in 0..=28 {
        let offset: u32 = if i <= 1 {
            0
        } else if i == 2 {
            5
        } else {
            9
        };
        gvar.extend(offset.to_be_bytes());
    }
    gvar.extend(0x4000u16.to_be_bytes());
    gvar.extend([0, 0, 0, 4, 0xAA]);
    gvar.extend([0, 0, 0, 4]);
    let mut tables = sample_tables();
    tables.push((if large { b"GVAR" } else { b"gvar" }, gvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn adr0022_gvar_keeps_mapped_glyphs_and_pads_short_offsets() {
    let data = font_with_gvar(false);
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(1);
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let actual = output.table(Tag::new(b"gvar")).unwrap();
    let mut expected = vec![0, 1, 0, 0, 0, 1, 0, 1];
    expected.extend(28u32.to_be_bytes());
    expected.extend(3u16.to_be_bytes());
    expected.extend(0u16.to_be_bytes());
    expected.extend(30u32.to_be_bytes());
    for offset in [0u16, 0, 3, 5] {
        expected.extend(offset.to_be_bytes());
    }
    expected.extend(0x4000u16.to_be_bytes());
    expected.extend([0, 0, 0, 4, 0xAA, 0]);
    expected.extend([0, 0, 0, 4]);
    assert_eq!(actual, expected);
}

#[test]
fn adr0022_gvar_retain_gids_preserves_empty_slots() {
    let data = font_with_gvar(false);
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Glyphs, |glyphs| {
            glyphs.add(2);
        })
        .with_flags(Flags::RETAIN_GIDS);
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let gvar = output.table(Tag::new(b"gvar")).unwrap();
    assert_eq!(&gvar[20..28], &[0, 0, 0, 0, 0, 0, 0, 2]);
    assert_eq!(&gvar[28..30], &[0x40, 0]);
    assert_eq!(&gvar[30..], &[0, 0, 0, 4]);
}

#[test]
fn adr0022_gvar_iftb_uses_long_offsets_without_padding() {
    let data = font_with_gvar(false);
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Glyphs, |glyphs| {
            glyphs.add(1);
            glyphs.add(2);
        })
        .with_flags(Flags::IFTB_REQUIREMENTS);
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let gvar = output.table(Tag::new(b"gvar")).unwrap();
    assert_eq!(&gvar[14..20], &[0, 1, 0, 0, 0, 38]);
    assert_eq!(
        &gvar[20..36],
        &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 9]
    );
    assert_eq!(&gvar[36..38], &[0x40, 0]);
    assert_eq!(&gvar[38..], &[0, 0, 0, 4, 0xAA, 0, 0, 0, 4]);
}

#[test]
fn adr0022_gvar_supports_extended_glyph_count_header() {
    let data = font_with_gvar(true);
    let font = Font::new(&data, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |glyphs| {
        glyphs.add(1);
        glyphs.add(2);
    });
    let output = subset(&font, &input).unwrap();
    let output = Font::new(&output, 0).unwrap();
    let gvar = output.table(Tag::new(b"GVAR")).unwrap();
    assert_eq!(&gvar[12..15], &[0, 0, 3]);
    assert_eq!(&gvar[15..21], &[0, 0, 0, 0, 0, 31]);
    assert_eq!(&gvar[21..29], &[0, 0, 0, 0, 0, 3, 0, 5]);
    assert_eq!(&gvar[29..31], &[0x40, 0]);
    assert_eq!(&gvar[31..], &[0, 0, 0, 4, 0xAA, 0, 0, 0, 0, 4]);
}

#[test]
fn adr0022_gvar_zero_shared_tuple_offset() {
    for large in [false, true] {
        let bytes = font_with_shared_offset(large, 0);
        let font = Font::new(&bytes, 0).unwrap();
        let input = Input::new().map_set(SetKind::Glyphs, |set| {
            set.add(1);
            set.add(2);
        });
        let output = subset(&font, &input).unwrap();
        let font = Font::new(&output, 0).unwrap();
        let tag = Tag::new(if large { b"GVAR" } else { b"gvar" });
        let gvar = font.table(tag).unwrap();
        let header = if large { 21 } else { 20 };
        let records = u32::try_from(header + 8).unwrap();
        assert_eq!(&gvar[6..8], &1u16.to_be_bytes());
        assert_eq!(&gvar[8..12], &records.to_be_bytes());
        assert_eq!(&gvar[header - 4..header], &records.to_be_bytes());
        assert_eq!(&gvar[header + 8..], &[0, 0, 0, 4, 0xaa, 0, 0, 0, 0, 4]);
    }
}
