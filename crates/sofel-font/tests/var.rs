//! Variation arithmetic and location identity (ADR 0027, ADR 0030).

#![allow(clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{sample_tables, sfnt};
use sofel_font::Font;
use sofel_font::read::Fixed;
use sofel_font::var::{Ft, FtLocationError, HbF32, Location, StoreKind, VarError, Variation};

fn fvar_one_axis() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    fvar
}

fn fvar_one_named() -> Vec<u8> {
    let mut fvar = fvar_one_axis();
    *fvar.get_mut(13).unwrap() = 1;
    fvar.extend_from_slice(&[1, 1, 0, 0]);
    fvar.extend_from_slice(&0x0000_c000i32.to_be_bytes());
    fvar
}

fn font_with_mvar() -> Vec<u8> {
    // A valid region whose peak is zero evaluates to one in HarfBuzz.
    let store = [
        0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 22, 0, 1, 0, 1, 0, 0, 0, 0, 0, 16, 0, 1, 0, 1, 0, 1, 0,
        0, 0, 7,
    ];
    let mut mvar = vec![0, 1, 0, 0, 0, 0, 0, 8, 0, 0, 0, 12];
    mvar.extend_from_slice(&store);
    let mut tables = sample_tables();
    tables.push((b"MVAR", mvar));
    tables.push((b"fvar", fvar_one_axis()));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn font_with_avar() -> Vec<u8> {
    let mut tables = sample_tables();
    tables.push((b"fvar", fvar_one_axis()));
    // -1 -> -1, 0 -> 0, 1/2 -> 1/4, 1 -> 1.
    let mut avar = vec![0, 1, 0, 0, 0, 0, 0, 1, 0, 4];
    for (from, to) in [(-16384i16, -16384i16), (0, 0), (8192, 4096), (16384, 16384)] {
        avar.extend_from_slice(&from.to_be_bytes());
        avar.extend_from_slice(&to.to_be_bytes());
    }
    tables.push((b"avar", avar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn font_with_avar2() -> Vec<u8> {
    let mut tables = sample_tables();
    tables.push((b"fvar", fvar_one_axis()));
    // Version 2, one empty segment map, no index map, store at byte 18.
    let mut avar = vec![0, 2, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18];
    avar.extend_from_slice(&[
        0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 22, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 0,
        0x20, 0,
    ]);
    tables.push((b"avar", avar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn font_with_avar2_map() -> Vec<u8> {
    let mut tables = sample_tables();
    tables.push((b"fvar", fvar_one_axis()));
    // Axis 0 maps to item 1; item 0 has a zero delta.
    let mut avar = vec![0, 2, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 18, 0, 0, 0, 23];
    avar.extend_from_slice(&[0, 0, 0, 1, 1]);
    avar.extend_from_slice(&[
        0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 22, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 0, 1, 0, 0,
        0, 0, 0x20, 0,
    ]);
    tables.push((b"avar", avar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn i02_cached_and_copied_font_use_equal_deltas() {
    let mut bytes = font_with_mvar();
    let mvar = bytes
        .windows(4)
        .position(|window| window == b"MVAR")
        .unwrap();
    let table_offset =
        u32::from_be_bytes(bytes.get(mvar + 8..mvar + 12).unwrap().try_into().unwrap()) as usize;
    bytes
        .get_mut(table_offset + 12 + 18..table_offset + 12 + 20)
        .unwrap()
        .copy_from_slice(&8i16.to_be_bytes());
    let original = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_2_14(&original, &[4]);
    assert_eq!(location.ft_design_coords(), &[Fixed(16)]);
    let copy = bytes.clone();
    let other = Font::new(&copy, 0).unwrap();
    let cached = original.instance(&location);
    let live = other.instance(&location);
    assert_eq!(cached.item_delta::<HbF32>(StoreKind::Mvar, 0, 0), Ok(3.5));
    assert_eq!(
        cached.item_delta::<HbF32>(StoreKind::Mvar, 0, 0),
        live.item_delta::<HbF32>(StoreKind::Mvar, 0, 0)
    );
    assert_eq!(
        cached.item_delta::<Ft>(StoreKind::Mvar, 0, 0),
        live.item_delta::<Ft>(StoreKind::Mvar, 0, 0)
    );
}

#[test]
fn i05_invalid_ft_normalized_location_precedes_store_lookup() {
    let bytes = font_with_mvar();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_2_14(&font, &[0x4001]);
    let instance = font.instance(&location);
    assert_eq!(location.ft_error(), Some(FtLocationError::InvalidArgument));
    assert_eq!(
        instance.item_delta::<Ft>(StoreKind::Hvar, 0, 0),
        Err(VarError::FtLocation(FtLocationError::InvalidArgument))
    );
    assert_eq!(instance.item_delta::<HbF32>(StoreKind::Hvar, 0, 0), Ok(0.0));
}

#[test]
fn i04_normalized_16_16_rounds_hb_coordinate() {
    let bytes = font_with_mvar();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_16_16(&font, &[Fixed(2)]);
    assert_eq!(location.coords_normalized(), &[1]);
    assert_eq!(location.ft_coords(), &[Fixed(2)]);
    assert_eq!(location.ft_design_coords(), &[Fixed(2)]);
    let invalid = Location::from_normalized_16_16(&font, &[Fixed(0x10001)]);
    assert_eq!(invalid.ft_error(), Some(FtLocationError::InvalidArgument));
}

#[test]
fn i04_design_coordinate_normalizes_in_both_precisions() {
    let bytes = font_with_mvar();
    let font = Font::new(&bytes, 0).unwrap();
    let middle = Location::from_design(&font, &[0.5]);
    assert_eq!(middle.coords_normalized(), &[8192]);
    assert_eq!(middle.coords_design(), &[0.5]);
    assert_eq!(middle.ft_coords(), &[Fixed(32768)]);
    assert_eq!(middle.ft_design_coords(), &[Fixed(32768)]);
    let outside = Location::from_design(&font, &[2.0]);
    assert_eq!(outside.coords_normalized(), &[16384]);
    assert_eq!(outside.ft_coords(), &[Fixed(65536)]);
}

#[test]
fn i04_design_avar1_maps_both_precisions() {
    let bytes = font_with_avar();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[0.5]);
    assert_eq!(location.coords_normalized(), &[4096]);
    assert_eq!(location.ft_coords(), &[Fixed(16384)]);
    assert_eq!(location.ft_design_coords(), &[Fixed(32768)]);
}

#[test]
fn i04_named_location_then_later_variation_wins() {
    let mut tables = sample_tables();
    tables.push((b"fvar", fvar_one_named()));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let named = Location::named_instance(&font, 0);
    assert_eq!(named.coords_design(), &[0.75]);
    assert_eq!(named.named_instance_index(), Some(0));
    assert!(!named.ft_default());
    let variations = [
        Variation {
            tag: sofel_font::read::Tag::new(b"wght"),
            value: 0.5,
        },
        Variation {
            tag: sofel_font::read::Tag::new(b"wght"),
            value: 0.25,
        },
    ];
    let overridden = Location::new(&font, Some(0), &variations);
    assert_eq!(overridden.coords_design(), &[0.25]);
    assert_eq!(overridden.ft_design_coords(), &[Fixed(16384)]);
    assert_eq!(overridden.named_instance_index(), Some(0));
    let missing = Location::named_instance(&font, 1);
    assert_eq!(missing.coords_design(), &[0.0]);
    assert_eq!(missing.ft_error(), Some(FtLocationError::InvalidArgument));
}

#[test]
fn i04_variation_parse_and_format_css_and_hb_forms() {
    let expected = Variation {
        tag: sofel_font::read::Tag::new(b"wght"),
        value: 500.0,
    };
    assert_eq!(Variation::parse("wght=500"), Some(expected));
    assert_eq!(Variation::parse("'wght' 500"), Some(expected));
    assert_eq!(expected.format(), "wght=500");
    assert_eq!(Variation::parse("wght"), None);
    assert_eq!(Variation::parse("'wgt' 500"), None);
    assert_eq!(
        Variation {
            value: 0.00001,
            ..expected
        }
        .format(),
        "wght=1e-05"
    );
    assert_eq!(
        Variation {
            value: 1_000_000.0,
            ..expected
        }
        .format(),
        "wght=1e+06"
    );
    assert_eq!(
        Variation {
            value: 1.234567,
            ..expected
        }
        .format(),
        "wght=1.23457"
    );
}

#[test]
fn i04_avar2_uses_pre_delta_coordinates() {
    let bytes = font_with_avar2();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[0.0]);
    assert_eq!(location.coords_normalized(), &[8192]);
    assert_eq!(location.ft_coords(), &[Fixed(32768)]);
}

#[test]
fn i04_avar2_uses_index_map_item() {
    let bytes = font_with_avar2_map();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[0.0]);
    assert_eq!(location.coords_normalized(), &[8192]);
    assert_eq!(location.ft_coords(), &[Fixed(32768)]);
}

#[test]
fn i04_explicit_zero_enables_ft_region_evaluation() {
    let bytes = font_with_mvar();
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_2_14(&font, &[0]);
    let instance = font.instance(&location);
    assert_eq!(location.coords_normalized(), &[0]);
    assert_eq!(instance.item_delta::<Ft>(StoreKind::Mvar, 0, 0), Ok(7));
}

#[test]
fn i04_static_default_keeps_hb_mvar_but_ft_unsupplied_is_zero() {
    let bytes = font_with_mvar();
    let font = Font::new(&bytes, 0).unwrap();
    let instance = font.default_instance();
    assert_eq!(instance.item_delta::<HbF32>(StoreKind::Mvar, 0, 0), Ok(7.0));
    assert_eq!(instance.item_delta::<Ft>(StoreKind::Mvar, 0, 0), Ok(0));
}
