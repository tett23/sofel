//! Variation arithmetic and location identity (ADR 0027, ADR 0030).

#![allow(clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{Buf, name, sample_tables, sample_tables_with_glyf, sfnt, utf16be};
use sofel_font::Font;
use sofel_font::read::{Fixed, GlyphId};
use sofel_font::var::{Ft, FtLocationError, HbF32, Location, StoreKind, VarError, Variation};

#[test]
fn adr0027_zero_normalized_coordinates_disable_ft_gvar_blending() {
    let gvar = vec![
        0, 1, 0, 0, 0, 1, 0, 0, // version and one axis
        0, 0, 0, 24, 0, 1, 0, 0, // shared tuple offset and one glyph
        0, 0, 0, 24, 0, 0, 0, 8, // glyph data offset and 16-byte glyph
        0, 1, 0, 10, // one tuple and serialized data at 10
        0, 6, 0x80, 0, 0, 0, // embedded zero peak
        3, 10, 20, 30, 40, 0x83, // x deltas and zero y deltas
    ];
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_axis()));
    tables.push((b"gvar", gvar));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_16_16(&font, &[Fixed(0)]);
    let mut points = [sofel_font::var::gvar::Point::default(); 4];
    assert_eq!(
        font.instance(&location).apply_gvar_ft(
            0,
            &mut points,
            &mut sofel_font::var::gvar::FtScratch::default()
        ),
        Ok(false)
    );
    assert_eq!(points, [sofel_font::var::gvar::Point::default(); 4]);
}

fn fvar_one_axis() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    fvar
}

#[test]
fn adr0027_duplicate_axis_tags_keep_hb_and_ft_design_coordinates_separate() {
    let axis = fvar_one_axis().get(16..36).unwrap().to_vec();
    let mut fvar = fvar_one_axis();
    fvar.get_mut(8..10)
        .unwrap()
        .copy_from_slice(&3u16.to_be_bytes());
    fvar.get_mut(14..16)
        .unwrap()
        .copy_from_slice(&16u16.to_be_bytes());
    fvar.extend_from_slice(&axis);
    fvar.extend_from_slice(&axis);
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::new(
        &font,
        None,
        &[Variation {
            tag: sofel_font::read::Tag::new(b"wght"),
            value: 0.75,
        }],
    );
    assert_eq!(location.coords_design(), &[0.75, 0.75, 0.75]);
    assert_eq!(
        location.ft_design_coords(),
        &[Fixed(49152), Fixed(0), Fixed(0)]
    );
}

fn font_with_hvar(first_advance: u16, include_hmtx: bool) -> Vec<u8> {
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(16384)
        .i16(16384)
        .u16(2)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(21)
        .i16(-31)
        .done();
    let hvar = Buf::new()
        .u32(0x0001_0000)
        .u32(20)
        .u32(0)
        .u32(0)
        .u32(0)
        .bytes(&store)
        .done();
    let mut tables = sample_tables_with_glyf();
    if !include_hmtx {
        tables.retain(|(tag, _)| **tag != *b"hmtx");
    }
    if let Some(data) = tables
        .iter_mut()
        .find(|(tag, _)| **tag == *b"hmtx")
        .and_then(|(_, data)| data.get_mut(..2))
    {
        data.copy_from_slice(&first_advance.to_be_bytes());
    }
    tables.push((b"fvar", fvar_one_axis()));
    tables.push((b"HVAR", hvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn font_with_vvar() -> Vec<u8> {
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(16384)
        .i16(16384)
        .u16(1)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(21)
        .done();
    let vvar = Buf::new()
        .u32(0x0001_0000)
        .u32(24)
        .u32(0)
        .u32(0)
        .u32(0)
        .u32(56)
        .bytes(&store)
        .u8(0)
        .u8(0)
        .u16(1)
        .u8(0)
        .done();
    let mut tables = sample_tables_with_glyf();
    tables.push((b"vhea", build::hhea(1)));
    tables.push((b"vmtx", build::hmtx(&[(500, 0)], &[])));
    tables.push((b"VORG", Buf::new().u16(1).u16(0).i16(700).u16(0).done()));
    tables.push((b"fvar", fvar_one_axis()));
    tables.push((b"VVAR", vvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn font_with_metric_mvar(default_region: bool, tag: &[u8; 4], delta: i16) -> Vec<u8> {
    let peak = if default_region { 0 } else { 16384 };
    let store = Buf::new()
        .u16(1)
        .u32(12)
        .u16(1)
        .u32(22)
        .u16(1)
        .u16(1)
        .i16(0)
        .i16(peak)
        .i16(peak)
        .u16(1)
        .u16(1)
        .u16(1)
        .u16(0)
        .i16(delta)
        .done();
    let mvar = Buf::new()
        .u32(0x0001_0000)
        .u16(0)
        .u16(8)
        .u16(1)
        .u16(20)
        .bytes(tag)
        .u16(0)
        .u16(0)
        .bytes(&store)
        .done();
    let mut tables = sample_tables_with_glyf();
    tables.push((b"vhea", build::hhea(1)));
    tables.push((
        b"post",
        Buf::new()
            .u32(0x0003_0000)
            .i32(0)
            .i16(-100)
            .i16(20)
            .bytes(&[0; 20])
            .done(),
    ));
    tables.push((b"fvar", fvar_one_axis()));
    tables.push((b"MVAR", mvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn i03_freetype_mvar_does_not_create_metrics_without_outline() {
    let bytes = font_with_metric_mvar(false, b"hasc", 21);
    let mut tables = sample_tables();
    let mvar_font = Font::new(&bytes, 0).unwrap();
    let mvar = mvar_font
        .table(sofel_font::read::Tag::new(b"MVAR"))
        .unwrap();
    tables.push((b"MVAR", mvar.to_vec()));
    tables.push((b"fvar", fvar_one_axis()));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_2_14(&font, &[8192]);
    assert_eq!(font.instance(&location).ft_face_metrics().ascender, 0);
}

#[test]
fn i03_hvar_advance_uses_implicit_map_and_harfbuzz_roundf() {
    let bytes = font_with_hvar(500, true);
    let font = Font::new(&bytes, 0).unwrap();
    let default = font.default_instance();
    assert_eq!(default.advance(GlyphId(0)), 500);
    assert_eq!(default.advance(GlyphId(1)), 501);
    let half = Location::from_design(&font, &[0.5]);
    let instance = font.instance(&half);
    assert_eq!(
        instance.item_delta::<HbF32>(StoreKind::Hvar, 0, 0),
        Ok(10.5)
    );
    assert_eq!(instance.advance(GlyphId(0)), 511);
    assert_eq!(instance.advance(GlyphId(1)), 486);
    let max = Location::from_design(&font, &[1.0]);
    let instance = font.instance(&max);
    assert_eq!(
        instance.item_delta::<HbF32>(StoreKind::Hvar, 0, 0),
        Ok(21.0)
    );
    assert_eq!(instance.advance(GlyphId(0)), 521);
    assert_eq!(instance.advance(GlyphId(1)), 470);
}

#[test]
fn i03_hvar_advance_retains_unsigned_range_above_u16() {
    let bytes = font_with_hvar(u16::MAX, true);
    let font = Font::new(&bytes, 0).unwrap();
    let max = Location::from_design(&font, &[1.0]);
    let value: u32 = font.instance(&max).advance(GlyphId(0));
    assert_eq!(value, 65_556);
}

#[test]
fn adr0027_i07_freetype_phantom_metrics_use_hvar_and_vmtx() {
    let bytes = font_with_hvar(500, true);
    let font = Font::new(&bytes, 0).unwrap();
    let max = Location::from_design(&font, &[1.0]);
    assert_eq!(font.default_instance().ft_h_advance(GlyphId(0)), Some(500));
    assert_eq!(font.instance(&max).ft_h_advance(GlyphId(0)), Some(521));

    let bytes = font_with_vvar();
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        font.default_instance().ft_top_side_bearing(GlyphId(0), 123),
        Some(0)
    );
}

#[test]
fn i03_hvar_does_not_adjust_missing_horizontal_metrics() {
    let bytes = font_with_hvar(500, false);
    let font = Font::new(&bytes, 0).unwrap();
    let max = Location::from_design(&font, &[1.0]);
    assert_eq!(font.instance(&max).advance(GlyphId(0)), 500);
}

#[test]
fn i03_vvar_advance_uses_implicit_map_and_negative_harfbuzz_result() {
    let bytes = font_with_vvar();
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(font.default_instance().v_advance(GlyphId(0)), -500);
    let half = Location::from_design(&font, &[0.5]);
    assert_eq!(font.instance(&half).v_advance(GlyphId(0)), -511);
    let max = Location::from_design(&font, &[1.0]);
    assert_eq!(font.instance(&max).v_advance(GlyphId(0)), -521);
    assert_eq!(font.default_instance().ft_v_advance(GlyphId(0)), Some(500));
    assert_eq!(font.instance(&max).ft_v_advance(GlyphId(0)), Some(521));
}

#[test]
fn i03_vorg_uses_vvar_origin_map_before_scaling() {
    let bytes = font_with_vvar();
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        font.default_instance().v_origin(GlyphId(0), |_| None),
        (250, 700)
    );
    let half = Location::from_design(&font, &[0.5]);
    assert_eq!(
        font.instance(&half).v_origin(GlyphId(0), |_| None),
        (250, 711)
    );
}

#[test]
fn i03_mvar_adds_float_delta_before_metric_scaling() {
    let bytes = font_with_metric_mvar(false, b"hasc", 21);
    let font = Font::new(&bytes, 0).unwrap();
    let tag = sofel_font::MetricsTag::HorizontalAscender;
    assert_eq!(font.default_instance().metrics_position(tag), Some(800));
    let half = Location::from_design(&font, &[0.5]);
    assert_eq!(font.instance(&half).metrics_position(tag), Some(811));
    let max = Location::from_design(&font, &[1.0]);
    assert_eq!(font.instance(&max).metrics_position(tag), Some(821));
    assert_eq!(font.instance(&max).h_extents().ascender, 821);
    assert_eq!(
        font.instance(&max)
            .metrics_position_with_fallback(tag, |_| None),
        821
    );
}

#[test]
fn i03_mvar_static_default_region_still_applies() {
    let bytes = font_with_metric_mvar(true, b"hasc", 21);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        font.default_instance()
            .metrics_position(sofel_font::MetricsTag::HorizontalAscender),
        Some(821)
    );
}

#[test]
fn i03_mvar_vertical_extents_follow_the_horizontal_scale() {
    let bytes = font_with_metric_mvar(false, b"vasc", 21);
    let font = Font::new(&bytes, 0).unwrap();
    let half = Location::from_design(&font, &[0.5]);
    let tag = sofel_font::MetricsTag::VerticalAscender;
    assert_eq!(font.instance(&half).metrics_position(tag), Some(811));
    assert_eq!(font.instance(&half).v_extents().ascender, 811);
}

#[test]
fn i03_mvar_zero_underline_size_uses_the_fallback() {
    let bytes = font_with_metric_mvar(false, b"unds", -20);
    let font = Font::new(&bytes, 0).unwrap();
    let max = Location::from_design(&font, &[1.0]);
    let instance = font.instance(&max);
    let tag = sofel_font::MetricsTag::UnderlineSize;
    assert_eq!(instance.metrics_position(tag), Some(0));
    assert_eq!(instance.metrics_position_with_fallback(tag, |_| None), 55);
}

#[test]
fn i03_freetype_mvar_adjusts_derived_face_height() {
    let bytes = font_with_metric_mvar(false, b"hasc", 21);
    let font = Font::new(&bytes, 0).unwrap();
    let original = font.default_instance().ft_face_metrics();
    let max = Location::from_design(&font, &[1.0]);
    let varied = font.instance(&max).ft_face_metrics();
    assert_eq!(
        i32::from(varied.ascender),
        i32::from(original.ascender) + 21
    );
    assert_eq!(i32::from(varied.height), i32::from(original.height) + 21);
}

#[test]
fn i03_freetype_mvar_recomputes_underline_center() {
    let bytes = font_with_metric_mvar(false, b"unds", -20);
    let font = Font::new(&bytes, 0).unwrap();
    let max = Location::from_design(&font, &[1.0]);
    let varied = font.instance(&max).ft_face_metrics();
    assert_eq!(varied.underline_thickness, 0);
    assert_eq!(varied.underline_position, -100);
}

#[test]
fn i03_default_instance_keeps_existing_metric_values() {
    let bytes = sfnt(&sample_tables_with_glyf());
    let font = Font::new(&bytes, 0).unwrap();
    let instance = font.default_instance();
    assert_eq!(
        instance.advance(GlyphId(0)),
        font.default_instance().advance(GlyphId(0))
    );
    assert_eq!(instance.h_extents(), font.default_instance().h_extents());
    assert_eq!(
        instance.v_advance(GlyphId(0)),
        font.default_instance().v_advance(GlyphId(0))
    );
    assert_eq!(
        instance.ft_left_side_bearing(GlyphId(0)),
        font.default_instance().ft_left_side_bearing(GlyphId(0))
    );
}

fn fvar_one_named() -> Vec<u8> {
    let mut fvar = fvar_one_axis();
    *fvar.get_mut(13).unwrap() = 1;
    fvar.extend_from_slice(&[1, 1, 0, 0]);
    fvar.extend_from_slice(&0x0000_c000i32.to_be_bytes());
    fvar
}

fn fvar_one_named_with_psid() -> Vec<u8> {
    let mut fvar = fvar_one_named();
    *fvar.get_mut(15).unwrap() = 10;
    fvar.extend_from_slice(&258u16.to_be_bytes());
    fvar
}

fn font_with_mvar() -> Vec<u8> {
    sfnt(&mvar_tables(0))
}

fn mvar_tables(peak: u8) -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let store = [
        0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 22, 0, 1, 0, 1, 0, 0, 0, peak, 0, 16, 0, 1, 0, 1, 0, 1,
        0, 0, 0, 7,
    ];
    let mut mvar = vec![0, 1, 0, 0, 0, 0, 0, 8, 0, 0, 0, 12];
    mvar.extend_from_slice(&store);
    let mut tables = sample_tables();
    tables.push((b"MVAR", mvar));
    tables.push((b"fvar", fvar_one_axis()));
    tables.sort_by_key(|(tag, _)| **tag);
    tables
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
fn i03_source_identity_uses_target_regions_for_distinct_collection_faces() {
    let bytes = build::ttc(false, &[mvar_tables(8), mvar_tables(16)]);
    let first = Font::new(&bytes, 0).unwrap();
    let second = Font::new(&bytes, 1).unwrap();
    let location = Location::from_normalized_2_14(&first, &[4]);
    assert_eq!(
        first
            .instance(&location)
            .item_delta::<HbF32>(StoreKind::Mvar, 0, 0),
        Ok(3.5)
    );
    assert_eq!(
        second
            .instance(&location)
            .item_delta::<HbF32>(StoreKind::Mvar, 0, 0),
        Ok(1.75)
    );
    assert_eq!(
        second
            .instance(&location)
            .item_delta::<Ft>(StoreKind::Mvar, 0, 0),
        Ok(2)
    );
}

#[test]
fn i03_location_borrow_outlives_font_handle() {
    let bytes = font_with_mvar();
    let location = {
        let font = Font::new(&bytes, 0).unwrap();
        Location::from_normalized_2_14(&font, &[4])
    };
    let reopened = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        reopened.instance(&location).location().coords_normalized(),
        &[4]
    );
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
    let named_instance = font.instance(&named);
    assert_eq!(named_instance.coords_design(), &[0.75]);
    assert_eq!(
        named_instance.coords_normalized(),
        named.coords_normalized()
    );
    assert_eq!(named_instance.ft_coords(), named.ft_coords());
    assert_eq!(named_instance.ft_design_coords(), named.ft_design_coords());
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
    let first_missing = u16::try_from(font.ft_mm_var().unwrap().named_styles().count()).unwrap();
    let missing = Location::named_instance(&font, first_missing);
    assert_eq!(missing.coords_design(), &[0.0]);
    assert_eq!(missing.ft_error(), Some(FtLocationError::InvalidArgument));
}

#[test]
fn axis_and_named_instance_queries_use_fvar_records() {
    let mut tables = sample_tables();
    tables.push((b"fvar", fvar_one_named()));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let axes: Vec<_> = font.var_axes().collect();
    assert_eq!(axes.len(), 1);
    assert_eq!(axes.first().map(|a| a.index), Some(0));
    assert_eq!(
        axes.first().map(|a| a.tag),
        Some(sofel_font::read::Tag::new(b"wght"))
    );
    assert_eq!(
        axes.first().map(|a| (a.min, a.default, a.max)),
        Some((0.0, 0.0, 1.0))
    );
    assert_eq!(axes.first().map(|a| a.name_id), Some(256));
    assert_eq!(axes.first().map(|a| a.hidden), Some(false));
    assert_eq!(
        font.find_var_axis(sofel_font::read::Tag::new(b"wght")),
        axes.first().copied()
    );
    assert_eq!(
        font.find_var_axis(sofel_font::read::Tag::new(b"xxxx")),
        None
    );
    let instances: Vec<_> = font.named_instances().collect();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances.first().map(|n| n.subfamily_name_id), Some(257));
    assert_eq!(instances.first().map(|n| n.postscript_name_id), Some(None));
    assert_eq!(
        instances
            .first()
            .map(|n| n.design_coords().collect::<Vec<_>>()),
        Some(vec![0.75])
    );
}

#[test]
fn variable_queries_on_static_font_are_empty() {
    let bytes = sfnt(&sample_tables());
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(font.var_axes().count(), 0);
    assert_eq!(font.named_instances().count(), 0);
    assert_eq!(
        font.find_var_axis(sofel_font::read::Tag::new(b"wght")),
        None
    );
    assert!(font.ft_mm_var().is_none());
    assert_eq!(font.ft_default_named_instance(), None);
}

#[test]
fn freetype_mm_var_synthesizes_missing_default_named_style() {
    let subfamily = utf16be("Regular");
    let postscript = utf16be("Demo-Regular");
    let names = name(
        0,
        &[(3, 1, 0x409, 2, &subfamily), (3, 1, 0x409, 6, &postscript)],
        &[],
    );
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_named()));
    tables.push((b"name", names));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let mm = font.ft_mm_var().unwrap();
    assert_eq!(mm.axes().count(), 1);
    assert_eq!(mm.named_styles().count(), 2);
    let original = mm.named_styles().next().unwrap();
    assert_eq!((original.strid, original.psid), (257, 0xFFFF));
    assert_eq!(
        original.design_coords().collect::<Vec<_>>(),
        vec![Fixed(0xC000)]
    );
    let synthesized = mm.named_styles().nth(1).unwrap();
    assert_eq!((synthesized.strid, synthesized.psid), (2, 6));
    assert_eq!(
        synthesized.design_coords().collect::<Vec<_>>(),
        vec![Fixed(0)]
    );
    assert_eq!(mm.default_named_instance(), Some(2));
    assert_eq!(font.ft_default_named_instance(), Some(2));
    let default = Location::named_instance(&font, 1);
    assert_eq!(default.coords_design(), &[0.0]);
    assert_eq!(default.ft_error(), None);
    assert!(!default.ft_default());
}

#[test]
fn adr0036_i01_missing_default_counts_tail_without_names() {
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_named()));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(font.ft_mm_var().unwrap().named_styles().count(), 2);

    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_axis()));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(font.ft_mm_var().unwrap().named_styles().count(), 1);
}

#[test]
fn adr0036_i02_missing_names_leave_zero_tail() {
    let mut fvar = fvar_one_named();
    fvar.get_mut(24..28)
        .unwrap()
        .copy_from_slice(&0x4000i32.to_be_bytes());
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let tail = font.ft_mm_var().unwrap().named_styles().nth(1).unwrap();
    assert_eq!((tail.strid, tail.psid), (0, 0));
    assert_eq!(tail.design_coords().collect::<Vec<_>>(), vec![Fixed(0)]);
    let location = Location::named_instance(&font, 1);
    assert_eq!(location.ft_error(), None);
    assert_eq!(location.ft_design_coords(), &[Fixed(0)]);
    assert_eq!(location.coords_design(), &[0.25]);
    let weight = Variation::parse("wght=0.5").unwrap();
    let overridden = Location::new(&font, Some(1), &[weight]);
    assert_eq!(overridden.ft_design_coords(), &[Fixed(32768)]);
    assert_eq!(overridden.coords_design(), &[0.5]);
}

#[test]
fn adr0036_i02_zero_length_and_unsupported_platform_leave_zero_tail() {
    for names in [
        vec![(3, 1, 0x409, 2, &[][..]), (3, 1, 0x409, 6, &[][..])],
        vec![
            (0, 1, 0x409, 2, &b"Regular"[..]),
            (0, 1, 0x409, 6, &b"Demo"[..]),
        ],
    ] {
        let mut tables = sample_tables_with_glyf();
        tables.push((b"fvar", fvar_one_named()));
        tables.push((b"name", name(0, &names, &[])));
        tables.sort_by_key(|(tag, _)| **tag);
        let bytes = sfnt(&tables);
        let font = Font::new(&bytes, 0).unwrap();
        let tail = font.ft_mm_var().unwrap().named_styles().nth(1).unwrap();
        assert_eq!((tail.strid, tail.psid), (0, 0));
        assert_eq!(tail.design_coords().collect::<Vec<_>>(), vec![Fixed(0)]);
    }
}

#[test]
fn adr0036_i03_default_named_instance_uses_one_based_tail() {
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_named()));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(font.ft_default_named_instance(), Some(2));

    let mut fvar = fvar_one_named();
    fvar.get_mut(40..44)
        .unwrap()
        .copy_from_slice(&0i32.to_be_bytes());
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(font.ft_default_named_instance(), Some(1));
}

#[test]
fn adr0036_i02_named_default_uses_name_ids() {
    let subfamily = utf16be("Regular");
    let typographic = utf16be("Text");
    let postscript = utf16be("Demo-Regular");
    let names = name(
        0,
        &[
            (3, 1, 0x409, 2, &subfamily),
            (3, 1, 0x409, 6, &postscript),
            (3, 1, 0x409, 17, &typographic),
        ],
        &[],
    );
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_named()));
    tables.push((b"name", names));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let tail = font.ft_mm_var().unwrap().named_styles().nth(1).unwrap();
    assert_eq!((tail.strid, tail.psid), (17, 6));
    assert_eq!(tail.design_coords().collect::<Vec<_>>(), vec![Fixed(0)]);
}

#[test]
fn freetype_mm_var_uses_existing_default_named_style() {
    let mut fvar = fvar_one_named();
    fvar.get_mut(40..44)
        .unwrap()
        .copy_from_slice(&0i32.to_be_bytes());
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let mm = font.ft_mm_var().unwrap();
    assert_eq!(mm.named_styles().count(), 1);
    assert_eq!(mm.default_named_instance(), Some(1));
}

#[test]
fn freetype_mm_var_requires_exact_fvar_version() {
    let mut fvar = fvar_one_axis();
    *fvar.get_mut(3).unwrap() = 1;
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(font.var_axes().count(), 1);
    assert!(font.ft_mm_var().is_none());
}

#[test]
fn freetype_mm_var_rejects_zero_axes_and_nonstandard_instance_size() {
    let mut zero_axes = fvar_one_axis();
    zero_axes
        .get_mut(8..10)
        .unwrap()
        .copy_from_slice(&0u16.to_be_bytes());
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", zero_axes));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    assert!(font.ft_mm_var().is_none());

    let mut nonstandard = fvar_one_named();
    nonstandard
        .get_mut(14..16)
        .unwrap()
        .copy_from_slice(&9u16.to_be_bytes());
    nonstandard.push(0);
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", nonstandard));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(font.named_instances().count(), 1);
    assert!(font.ft_mm_var().is_none());
}

#[test]
fn freetype_mm_var_collapses_invalid_axis_range_while_harfbuzz_orders_it() {
    let mut fvar = fvar_one_axis();
    for (at, value) in [(20, 0x10000i32), (24, 0x8000), (28, 0)] {
        fvar.get_mut(at..at + 4)
            .unwrap()
            .copy_from_slice(&value.to_be_bytes());
    }
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let hb = font.var_axes().next().unwrap();
    assert_eq!((hb.min, hb.default, hb.max), (0.5, 0.5, 0.5));
    let ft = font.ft_mm_var().unwrap().axes().next().unwrap();
    assert_eq!(
        (ft.min, ft.default, ft.max),
        (Fixed(0x8000), Fixed(0x8000), Fixed(0x8000))
    );
    assert_eq!(ft.flags, 0);
}

#[test]
fn postscript_name_uses_named_psid_or_variation_axis_values() {
    let family = utf16be("Demo");
    let subfamily = utf16be("Semibold");
    let base_ps = utf16be("Demo-Regular");
    let named_ps = utf16be("Demo-Semibold");
    let names = name(
        0,
        &[
            (3, 1, 0x409, 6, &base_ps),
            (3, 1, 0x409, 25, &family),
            (3, 1, 0x409, 257, &subfamily),
            (3, 1, 0x409, 258, &named_ps),
        ],
        &[],
    );
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar_one_named_with_psid()));
    tables.push((b"name", names));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let ps = |location: &Location<'_>| {
        let mut out = [0u8; 128];
        let len = font.instance(location).postscript_name(&mut out).unwrap();
        String::from_utf8(out.get(..len).unwrap().to_vec()).unwrap()
    };
    assert_eq!(ps(Location::default_ref()), "Demo-Regular");
    assert_eq!(ps(&Location::named_instance(&font, 0)), "Demo-Semibold");
    assert_eq!(ps(&Location::from_design(&font, &[0.5])), "Demo_.5wght");
}

#[test]
fn postscript_name_hashes_long_variation_name_as_freetype() {
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 8, 0, 20, 0, 0, 0, 36];
    for index in 0..8u16 {
        let tag = format!("AX{index:02}");
        fvar.extend_from_slice(tag.as_bytes());
        for value in [0i32, 0, 65536] {
            fvar.extend_from_slice(&value.to_be_bytes());
        }
        fvar.extend_from_slice(&0u16.to_be_bytes());
        fvar.extend_from_slice(&(256 + index).to_be_bytes());
    }
    let prefix = utf16be(&"ABCDEFGHIJ".repeat(9));
    let subfamily = utf16be("Regular");
    let postscript = utf16be("Test-Regular");
    let names = name(
        0,
        &[
            (3, 1, 0x409, 2, &subfamily),
            (3, 1, 0x409, 6, &postscript),
            (3, 1, 0x409, 25, &prefix),
        ],
        &[],
    );
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.push((b"name", names));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[1.0; 8]);
    let mut out = [0u8; 256];
    let len = font.instance(&location).postscript_name(&mut out).unwrap();
    assert_eq!(
        out.get(..len),
        Some(b"ABCDEFGHIJABCDEFGHIJABCDEFGHIJABCDEFGHIJABCDEFGHIJABCDEFGHIJABCDEFGHIJABCDEFGHIJABCDEFGHIJ-2601C9791E88535C01098898186549F1...".as_slice())
    );
}

#[test]
fn postscript_name_formats_fixed_fraction_as_freetype() {
    let mut fvar = fvar_one_axis();
    fvar.get_mut(28..32)
        .unwrap()
        .copy_from_slice(&21845i32.to_be_bytes());
    let prefix = utf16be("Demo");
    let mut tables = sample_tables_with_glyf();
    tables.push((b"fvar", fvar));
    tables.push((b"name", name(0, &[(3, 1, 0x409, 25, &prefix)], &[])));
    tables.sort_by_key(|(tag, _)| **tag);
    let bytes = sfnt(&tables);
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_normalized_16_16(&font, &[Fixed(65536)]);
    let mut out = [0u8; 128];
    let len = font.instance(&location).postscript_name(&mut out).unwrap();
    assert_eq!(out.get(..len), Some(b"Demo_.33333wght".as_slice()));
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
