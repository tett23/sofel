//! ADR 0062 I1: layout queries carry the font location, scale and ppem.

#![allow(clippy::all, clippy::indexing_slicing, clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;
#[path = "support/layout_query_fixture.rs"]
mod layout_query_fixture;

use build::*;
use sofel_font::read::math::Corner;
use sofel_font::read::{GlyphId, Tag};
use sofel_font::var::{Location, Variation};
use sofel_font::{Direction, Font, HbScale};

fn font_with_math(math: Vec<u8>) -> Vec<u8> {
    let mut tables = sample_tables();
    tables.push((b"MATH", math));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

fn math_with_device() -> Vec<u8> {
    let mut math = Buf::new().u32(0x0001_0000).u16(10).u16(0).u16(0);
    math = math.i16(80).i16(60).u16(1500).u16(2000);
    for index in 0..51 {
        math = math.i16(if index == 0 { 100 } else { 0 });
        math = math.u16(if index == 0 { 214 } else { 0 });
    }
    math = math.i16(55);
    // 2-bit Device deltas: +1 at 12 ppem, -1 at 13 ppem.
    math.u16(12).u16(13).u16(1).u16(0x7000).done()
}

#[test]
fn i1_device_query_math_constant_uses_y_ppem() {
    let data = font_with_math(math_with_device());
    let font = Font::new(&data, 0).unwrap();
    let instance = font.default_instance();
    for (y_ppem, expected) in [(0, 100), (11, 100), (12, 183), (13, 24), (14, 100)] {
        let query = instance.layout_queries(HbScale::unscaled(&font), 40, y_ppem);
        assert!(query.has_math());
        assert_eq!(query.math_constant(4), expected, "y_ppem={y_ppem}");
    }
}

#[test]
fn i1_device_query_math_horizontal_constant_uses_x_ppem() {
    let mut math = math_with_device();
    // `spaceAfterScript` is MATH constant 17, value record 13.
    math[70..74].copy_from_slice(&[0, 100, 0, 214]);
    let data = font_with_math(math);
    let font = Font::new(&data, 0).unwrap();
    let instance = font.default_instance();
    let x_query = instance.layout_queries(HbScale::unscaled(&font), 12, 0);
    let y_query = instance.layout_queries(HbScale::unscaled(&font), 0, 12);
    assert_eq!(x_query.math_constant(17), 183);
    assert_eq!(y_query.math_constant(17), 100);
}

#[test]
fn i1_device_query_missing_math_returns_harfbuzz_defaults() {
    let data = sfnt(&sample_tables());
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 12, 16);
    assert!(!query.has_math());
    assert_eq!(query.math_constant(4), 0);
    assert_eq!(query.math_italics_correction(GlyphId(3)), 0);
    assert_eq!(query.math_top_accent_attachment(GlyphId(3)), 251);
    assert!(!query.math_is_extended_shape(GlyphId(3)));
    assert_eq!(query.math_kerning(GlyphId(3), Corner::TopRight, 100), 0);
    assert!(query.math_kernings(GlyphId(3), Corner::TopRight).is_empty());
    assert!(
        query
            .math_variants(GlyphId(3), Direction::Vertical)
            .is_empty()
    );
    assert!(
        query
            .math_assembly(GlyphId(3), Direction::Vertical)
            .parts
            .is_empty()
    );
    assert_eq!(query.math_min_connector_overlap(Direction::Vertical), 0);
    assert!(
        query
            .ligature_carets(GlyphId(3), Direction::Horizontal)
            .is_empty()
    );
}

#[test]
fn i1_device_query_math_glyphs_kern_and_assembly() {
    let data = font_with_math(layout_query_fixture::sample());
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 12, 16);
    assert_eq!(query.math_italics_correction(GlyphId(3)), 30);
    assert_eq!(query.math_italics_correction(GlyphId(5)), -40);
    assert_eq!(query.math_top_accent_attachment(GlyphId(5)), 250);
    assert!(query.math_is_extended_shape(GlyphId(7)));
    assert!(!query.math_is_extended_shape(GlyphId(6)));
    let kernings = query.math_kernings(GlyphId(3), Corner::TopRight);
    assert_eq!(kernings.len(), 2);
    assert_eq!(
        (kernings[0].max_correction_height, kernings[0].kern_value),
        (100, -10)
    );
    assert_eq!(
        (kernings[1].max_correction_height, kernings[1].kern_value),
        (i32::MAX, -20)
    );
    assert_eq!(query.math_kerning(GlyphId(3), Corner::TopRight, 99), -10);
    assert_eq!(query.math_kerning(GlyphId(3), Corner::TopRight, 100), -20);
    assert_eq!(query.math_min_connector_overlap(Direction::Vertical), 50);
    let variants = query.math_variants(GlyphId(10), Direction::Vertical);
    assert_eq!(
        variants
            .iter()
            .map(|v| (v.glyph.0, v.advance))
            .collect::<Vec<_>>(),
        [(12, 1000), (13, 1500)]
    );
    let assembly = query.math_assembly(GlyphId(10), Direction::Vertical);
    assert_eq!(assembly.italics_correction, 5);
    assert_eq!(assembly.parts.len(), 2);
    assert_eq!(
        (assembly.parts[0].glyph.0, assembly.parts[0].full_advance),
        (14, 500)
    );
    assert_eq!(
        (assembly.parts[1].glyph.0, assembly.parts[1].flags),
        (15, 1)
    );
}

#[test]
fn i1_device_query_base_uses_directional_ppem() {
    let axis = AxisSpec {
        tags: Some(vec![*b"romn"]),
        scripts: Some(vec![(
            *b"DFLT",
            Some(ScriptSpec {
                values: Some((
                    0,
                    vec![Some(Coord::F3(100, Some(device(12, 13, 1, &[0x7000]))))],
                )),
                default_min_max: Some(MinMaxSpec {
                    min: Some(Coord::F3(-100, Some(device(12, 13, 1, &[0x7000])))),
                    max: Some(Coord::F3(800, Some(device(12, 13, 1, &[0x7000])))),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        )]),
    };
    let mut tables = sample_tables();
    tables.push((b"BASE", base(Some(&axis), Some(&axis))));
    tables.sort_by_key(|(tag, _)| **tag);
    let data = sfnt(&tables);
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 13, 12);
    assert_eq!(
        query.baseline(Tag::new(b"romn"), Direction::Horizontal, Tag::new(b"DFLT")),
        Some(183)
    );
    assert_eq!(
        query.baseline(Tag::new(b"romn"), Direction::Vertical, Tag::new(b"DFLT")),
        Some(24)
    );
    assert_eq!(
        query.baseline_with_fallback(
            Tag::new(b"romn"),
            Direction::Horizontal,
            Tag::new(b"DFLT"),
            |_| None
        ),
        183
    );
    let (horizontal, found) =
        query.layout_font_extents(Direction::Horizontal, Tag::new(b"DFLT"), Tag::new(b"dflt"));
    assert!(found);
    assert_eq!((horizontal.ascender, horizontal.descender), (883, -17));
    let (vertical, found) =
        query.layout_font_extents(Direction::Vertical, Tag::new(b"DFLT"), Tag::new(b"dflt"));
    assert!(found);
    assert_eq!((vertical.ascender, vertical.descender), (724, -176));
}

#[test]
fn i1_device_query_ligature_carets_use_x_or_y_ppem() {
    let mut tables = sample_tables();
    tables.push((b"GDEF", layout_query_fixture::gdef_with_carets()));
    tables.sort_by_key(|(tag, _)| **tag);
    let data = sfnt(&tables);
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 12, 13);
    assert_eq!(
        query.ligature_carets(GlyphId(3), Direction::Horizontal),
        [100, 283]
    );
    assert_eq!(
        query.ligature_carets(GlyphId(3), Direction::Vertical),
        [100, 124]
    );
    assert!(
        query
            .ligature_carets(GlyphId(4), Direction::Horizontal)
            .is_empty()
    );
}

#[test]
fn i1_device_query_math_variation_index_uses_location_without_ppem() {
    let data = layout_query_fixture::variable_math_font();
    let font = Font::new(&data, 0).unwrap();
    let location = Location::new(
        &font,
        None,
        &[Variation {
            tag: Tag::new(b"wght"),
            value: 1.0,
        }],
    );
    let query = font
        .instance(&location)
        .layout_queries(HbScale::unscaled(&font), 0, 0);
    assert_eq!(query.math_constant(4), 21);
    assert_eq!(query.style_value(Tag::new(b"wght")), 1.0);
}

#[test]
fn i1_device_query_style_values_use_same_font_location() {
    let data = sfnt(&sample_tables());
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 0, 0);
    assert_eq!(query.style_value(Tag::new(b"wght")), 700.0);
    assert_eq!(query.style_value(Tag::new(b"wdth")), 100.0);
    assert_eq!(query.style_value(Tag::new(b"xxxx")), 0.0);
}

#[test]
fn i1_device_query_style_optical_size_reads_gpos_size_feature() {
    let gpos = Buf::new()
        .u32(0x0001_0000)
        .u16(0)
        .u16(10)
        .u16(0)
        .u16(1)
        .bytes(b"size")
        .u16(8)
        .u16(4)
        .u16(0)
        .u16(180)
        .u16(0)
        .u16(0)
        .u16(0)
        .u16(0)
        .done();
    let mut tables = sample_tables();
    tables.push((b"GPOS", gpos));
    tables.sort_by_key(|(tag, _)| **tag);
    let data = sfnt(&tables);
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 0, 0);
    assert_eq!(query.style_value(Tag::new(b"opsz")), 18.0);
    let point_size_query =
        font.default_instance()
            .layout_queries(HbScale::unscaled(&font).with_ptem(23.5), 0, 0);
    assert_eq!(point_size_query.style_value(Tag::new(b"opsz")), 23.5);
}

#[test]
fn i1_device_query_style_value_reads_stat_when_no_fvar_axis() {
    let stat = Buf::new()
        .u32(0x0001_0001)
        .u16(8)
        .u16(1)
        .u32(20)
        .u16(1)
        .u32(28)
        .u16(0)
        .bytes(b"wdth")
        .u16(0)
        .u16(0)
        .u16(2)
        .u16(1)
        .u16(0)
        .u16(0)
        .u16(0)
        .u32(120 * 65536)
        .done();
    let mut tables = sample_tables();
    tables.push((b"STAT", stat));
    tables.sort_by_key(|(tag, _)| **tag);
    let data = sfnt(&tables);
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 0, 0);
    assert_eq!(query.style_value(Tag::new(b"wdth")), 120.0);
}
