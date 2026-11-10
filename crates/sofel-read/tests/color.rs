//! CPAL, COLR, CBLC/EBLC/bloc, CBDT/EBDT/bdat, sbix and SVG (ADR 0023),
//! on tables built with the ADR 0007 builders.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

mod support;

use sofel_read::cbdt::{self, Cbdt, GlyphMetrics};
use sofel_read::cblc::{self, Cblc};
use sofel_read::colr::{self, Colr};
use sofel_read::cpal::{self, Cpal};
use sofel_read::sbix::{self, Sbix};
use sofel_read::svg::{self, Svg};
use sofel_read::{Diagnostic, ReadError, Tag};
use support::build::*;

fn ids(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> Vec<(&'static str, u32)> {
    let mut out = Vec::new();
    f(&mut |d: Diagnostic| out.push((d.kind.id(), d.offset)));
    out
}

const RED: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [255, 0, 0, 128];

#[test]
fn cpal_version_0() {
    let d = cpal(2, &[0, 1], &[RED, GREEN, BLUE], None);
    let c = Cpal::new(&d).unwrap();
    assert_eq!(
        (c.version(), c.num_palettes(), c.num_palette_entries()),
        (0, 2, 2)
    );
    assert_eq!(c.color(0, 0), Some(RED));
    assert_eq!(c.color(1, 1), Some(BLUE));
    assert_eq!(c.color(0, 2), None);
    assert_eq!(c.color(2, 0), None);
    assert_eq!(c.palette_len(1), 2);
    assert_eq!(c.palette_flags(0), 0);
    assert_eq!(c.palette_name_id(0), cpal::NO_NAME);
    assert_eq!(c.color_name_id(1), cpal::NO_NAME);
    assert!(ids(|r| cpal::validate(&d, r)).is_empty());
}

#[test]
fn cpal_version_1() {
    let d = cpal(
        1,
        &[0, 1, 2],
        &[RED, GREEN, BLUE],
        Some((Some(vec![1, 2, 0]), Some(vec![256, 257, 258]), None)),
    );
    let c = Cpal::new(&d).unwrap();
    assert_eq!(c.version(), 1);
    assert_eq!(
        (c.palette_flags(0), c.palette_flags(1), c.palette_flags(3)),
        (
            cpal::USABLE_WITH_LIGHT_BACKGROUND,
            cpal::USABLE_WITH_DARK_BACKGROUND,
            0
        )
    );
    assert_eq!(
        (c.palette_name_id(2), c.palette_name_id(3)),
        (258, cpal::NO_NAME)
    );
    assert_eq!(c.color_name_id(0), cpal::NO_NAME);
    let d = cpal(1, &[0], &[RED], Some((None, None, Some(vec![300]))));
    let c = Cpal::new(&d).unwrap();
    assert_eq!(
        (c.color_name_id(0), c.color_name_id(1)),
        (300, cpal::NO_NAME)
    );
}

#[test]
fn cpal_out_of_range() {
    // Palette 1 starts at record 2 and needs records 2 and 3.
    let d = cpal(2, &[0, 2], &[RED, GREEN, BLUE], None);
    let c = Cpal::new(&d).unwrap();
    assert_eq!(c.palette_len(1), 1);
    assert_eq!(c.color(1, 1), None);
    assert_eq!(
        ids(|r| cpal::validate(&d, r)),
        [("color_index_out_of_range", 14)]
    );
    // Truncated color records.
    let mut d = cpal(1, &[0], &[RED, GREEN], None);
    d.truncate(d.len() - 2);
    assert_eq!(Cpal::new(&d).unwrap_err(), ReadError::OutOfBounds);
    assert_eq!(
        ids(|r| cpal::validate(&d, r)),
        [("offset_out_of_bounds", 0)]
    );
    assert_eq!(ids(|r| cpal::validate(&d[..6], r)), [("truncated", 0)]);
    // A version 1 label array out of bounds.
    let mut d = cpal(1, &[0], &[RED], Some((None, Some(vec![1]), None)));
    d.truncate(d.len() - 1);
    assert!(Cpal::new(&d).is_err());
}

#[test]
fn colr_v0_records() {
    let d = colr_v0(
        &[(2, 0, 2), (5, 2, 1), (7, 3, 4)],
        &[(10, 0), (11, 0xFFFF), (12, 1), (13, 2)],
    );
    let c = Colr::new(&d).unwrap();
    assert_eq!(c.version(), 0);
    assert_eq!(
        c.glyph_layers(2).collect::<Vec<_>>(),
        [(10, 0), (11, 0xFFFF)]
    );
    assert_eq!(c.glyph_layers(5).collect::<Vec<_>>(), [(12, 1)]);
    // Cut at the end of the layer records, as HarfBuzz does.
    assert_eq!(c.glyph_layers(7).collect::<Vec<_>>(), [(13, 2)]);
    assert_eq!(c.glyph_layers(3).count(), 0);
    assert_eq!(c.find_base_glyph(5).unwrap().first_layer, 2);
    assert_eq!(c.base_glyph_paint(2), None);
    assert_eq!(c.clip_box(2), None);
    assert_eq!(
        ids(|r| colr::validate(&d, Some(20), Some(2), r)),
        [
            ("layer_index_out_of_range", 26),
            ("color_index_out_of_range", 44)
        ]
    );
    assert_eq!(
        ids(|r| colr::validate(&d, Some(12), None, r)),
        [
            ("layer_index_out_of_range", 26),
            ("glyph_id_out_of_range", 40),
            ("glyph_id_out_of_range", 44)
        ]
    );
}

#[test]
fn colr_v0_unsorted_and_broken() {
    let d = colr_v0(&[(5, 0, 1), (5, 0, 1), (2, 0, 1)], &[(1, 0)]);
    assert_eq!(
        ids(|r| colr::validate(&d, None, None, r)),
        [("duplicate_glyph", 20), ("glyphs_not_sorted", 26)]
    );
    let mut d = colr_v0(&[(5, 0, 1)], &[(1, 0)]);
    d.truncate(d.len() - 1);
    assert_eq!(Colr::new(&d).unwrap_err(), ReadError::OutOfBounds);
    assert_eq!(
        ids(|r| colr::validate(&d, None, None, r)),
        [("offset_out_of_bounds", 0)]
    );
    assert_eq!(
        ids(|r| colr::validate(&d[..10], None, None, r)),
        [("truncated", 0)]
    );
}

fn clip1(x0: i16, y0: i16, x1: i16, y1: i16) -> Vec<u8> {
    Buf::new().u8(1).i16(x0).i16(y0).i16(x1).i16(y1).done()
}

#[test]
fn colr_v1_lists_and_clips() {
    let solid = Buf::new().u8(2).u16(0).u16(0x4000).done();
    let clip2 = Buf::new().u8(2).i16(-1).i16(-2).i16(3).i16(4).u32(9).done();
    let d = colr_v1(
        &[],
        &[],
        &[solid.clone(), solid],
        &[(3, 0), (9, 1)],
        &[1, 0],
        &[
            (3, 5, clip1(0, -100, 500, 800)),
            (9, 9, clip2),
            (12, 12, vec![3, 0, 0]),
        ],
    );
    let c = Colr::new(&d).unwrap();
    assert_eq!(c.version(), 1);
    let p3 = c.base_glyph_paint(3).unwrap();
    assert_eq!(&d[p3..p3 + 5], &[2, 0, 0, 0x40, 0]);
    assert!(c.base_glyph_paint(4).is_none());
    assert_eq!(c.num_layer_paints(), 2);
    assert_eq!(c.layer_paint(1), Some(p3));
    let b = c.clip_box(4).unwrap();
    assert_eq!(
        (
            b.format,
            b.x_min,
            b.y_min,
            b.x_max,
            b.y_max,
            b.var_index_base
        ),
        (1, 0, -100, 500, 800, None)
    );
    assert_eq!(c.clip_box(9).unwrap().var_index_base, Some(9));
    assert_eq!(c.clip_box(6), None);
    assert_eq!(c.clip_box(12), None);
    let diags = ids(|r| colr::validate(&d, Some(20), None, r));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].0, "unknown_format");
}

#[test]
fn colr_v1_list_out_of_bounds() {
    let mut d = colr_v1(&[], &[], &[vec![2, 0, 0, 0x40, 0]], &[(3, 0)], &[], &[]);
    // BaseGlyphList count far past the table.
    d[34..38].copy_from_slice(&1000u32.to_be_bytes());
    assert_eq!(Colr::new(&d).unwrap_err(), ReadError::OutOfBounds);
}

fn eblc_with(index: Vec<u8>, first: u16, last: u16) -> Vec<u8> {
    cblc(
        2,
        &[SizeSpec {
            start_glyph: first,
            end_glyph: last,
            subtables: vec![(first, last, index)],
            ..SizeSpec::default()
        }],
    )
}

#[test]
fn location_index_formats() {
    // Format 1: glyphs 3..=5, glyph 4 empty.
    let d = eblc_with(index_offsets(1, 6, 100, &[0, 20, 20, 44]), 3, 5);
    let t = Cblc::new(&d).unwrap();
    assert_eq!(t.num_sizes(), 1);
    let s = t.size(0).unwrap();
    assert_eq!(
        (
            s.ppem_x,
            s.ppem_y,
            s.bit_depth,
            s.hori.ascender,
            s.hori.descender
        ),
        (16, 16, 1, 12, -4)
    );
    let r = t.index_record(&s, 0).unwrap();
    let l = t.locate(&r, 3).unwrap();
    assert_eq!(
        (l.image_format, l.offset, l.length, l.metrics),
        (6, 100, 20, None)
    );
    assert_eq!(t.locate(&r, 4), None);
    assert_eq!(t.locate(&r, 5).unwrap().offset, 120);
    assert_eq!(t.locate(&r, 6), None);
    assert!(ids(|rep| cblc::validate(cblc::TAG_EBLC, &d, rep)).is_empty());

    // Format 3.
    let d = eblc_with(index_offsets(3, 1, 4, &[0, 10, 30]), 7, 8);
    let t = Cblc::new(&d).unwrap();
    let r = t.index_record(&t.size(0).unwrap(), 0).unwrap();
    assert_eq!(
        t.locate(&r, 8).map(|l| (l.offset, l.length)),
        Some((14, 20))
    );

    // Format 2: constant size and big metrics.
    let metrics = [8, 6, 1, 7, 7, 0xFD, 1, 9];
    let f2 = Buf::new()
        .u16(2)
        .u16(5)
        .u32(40)
        .u32(6)
        .bytes(&metrics)
        .done();
    let d = eblc_with(f2, 10, 12);
    let t = Cblc::new(&d).unwrap();
    let r = t.index_record(&t.size(0).unwrap(), 0).unwrap();
    let l = t.locate(&r, 12).unwrap();
    assert_eq!(
        (l.image_format, l.offset, l.length, l.metrics),
        (5, 52, 6, Some(metrics))
    );
    let m = GlyphMetrics::big(&metrics).unwrap();
    assert_eq!(
        (
            m.height,
            m.width,
            m.hori_bearing_y,
            m.vert_bearing_x,
            m.vert_advance
        ),
        (8, 6, 7, -3, 9)
    );

    // Format 4: sparse pairs.
    let f4 = Buf::new()
        .u16(4)
        .u16(1)
        .u32(0)
        .u32(2)
        .u16(20)
        .u16(0)
        .u16(25)
        .u16(9)
        .u16(0)
        .u16(15)
        .done();
    let d = eblc_with(f4, 20, 30);
    let t = Cblc::new(&d).unwrap();
    let r = t.index_record(&t.size(0).unwrap(), 0).unwrap();
    assert_eq!(t.locate(&r, 25).map(|l| (l.offset, l.length)), Some((9, 6)));
    assert_eq!(t.locate(&r, 21), None);

    // Format 5: sparse ids with constant size and metrics.
    let f5 = Buf::new()
        .u16(5)
        .u16(5)
        .u32(100)
        .u32(4)
        .bytes(&metrics)
        .u32(2)
        .u16(40)
        .u16(44)
        .done();
    let d = eblc_with(f5, 40, 44);
    let t = Cblc::new(&d).unwrap();
    let r = t.index_record(&t.size(0).unwrap(), 0).unwrap();
    assert_eq!(
        t.locate(&r, 44).map(|l| (l.offset, l.length)),
        Some((104, 4))
    );
    assert_eq!(t.locate(&r, 42), None);
    assert!(ids(|rep| cblc::validate(cblc::TAG_CBLC, &d, rep)).is_empty());
}

#[test]
fn location_tables_broken() {
    // Offsets that go down.
    let d = eblc_with(index_offsets(1, 1, 0, &[0, 20, 10]), 0, 1);
    assert_eq!(
        ids(|r| cblc::validate(cblc::TAG_EBLC, &d, r)),
        [("offsets_not_sorted", 80)]
    );
    // Unknown index format.
    let d = eblc_with(Buf::new().u16(9).u16(1).u32(0).done(), 0, 0);
    assert_eq!(
        ids(|r| cblc::validate(cblc::TAG_BLOC, &d, r)),
        [("unknown_format", 64)]
    );
    // Unknown version, reversed range.
    let mut d = eblc_with(index_offsets(1, 1, 0, &[0, 4]), 5, 4);
    d[1] = 7;
    let got = ids(|r| cblc::validate(cblc::TAG_EBLC, &d, r));
    assert_eq!(got[0], ("unknown_version", 0));
    assert!(got.contains(&("start_after_end", 8)));
    // Index array out of bounds.
    let mut d = eblc_with(index_offsets(1, 1, 0, &[0, 4]), 0, 0);
    d[8..12].copy_from_slice(&1000u32.to_be_bytes());
    assert_eq!(
        ids(|r| cblc::validate(cblc::TAG_EBLC, &d, r)),
        [("offset_out_of_bounds", 8)]
    );
    // Truncated BitmapSize records.
    assert_eq!(
        ids(|r| cblc::validate(cblc::TAG_EBLC, &d[..30], r)),
        [("truncated", 0)]
    );
}

#[test]
fn data_table_images() {
    // Glyph 0: format 17 with a 29-byte PNG header; glyph 1: format 17
    // whose data length runs past the image.
    let png = png_header(20, 30);
    let mut data = Buf::new().u16(3).u16(0);
    let g0 = Buf::new()
        .bytes(&[30, 20, 1, 25, 22])
        .u32(png.len() as u32)
        .bytes(&png)
        .done();
    let g1 = Buf::new()
        .bytes(&[30, 20, 1, 25, 22])
        .u32(1000)
        .bytes(&png)
        .done();
    let (o0, o1) = (4u32, 4 + g0.len() as u32);
    data = data.bytes(&g0).bytes(&g1);
    let data = data.done();
    let loc = eblc_with(
        index_offsets(1, 17, 0, &[o0, o1, o1 + g1.len() as u32]),
        0,
        1,
    );
    let c = Cbdt::new(&data).unwrap();
    assert_eq!(c.major_version(), 3);
    assert_eq!(&c.image(o0, 5).unwrap(), &[30, 20, 1, 25, 22]);
    assert_eq!(c.image(o1, 1000), None);
    let t = Cblc::new(&loc).unwrap();
    assert_eq!(
        ids(|r| cbdt::validate_images(cbdt::TAG_CBDT, &data, &t, r)),
        [("truncated", o1)]
    );
    assert!(ids(|r| cbdt::validate(cbdt::TAG_CBDT, &data, r)).is_empty());
    assert_eq!(
        ids(|r| cbdt::validate(cbdt::TAG_EBDT, &[0, 9, 0, 0], r)),
        [("unknown_version", 0)]
    );
    assert_eq!(
        ids(|r| cbdt::validate(cbdt::TAG_BDAT, &[0, 2], r)),
        [("truncated", 0)]
    );
    // An unknown image format and an image past the table.
    let loc = eblc_with(index_offsets(1, 4, 0, &[4, 9]), 0, 0);
    let t = Cblc::new(&loc).unwrap();
    assert_eq!(
        ids(|r| cbdt::validate_images(cbdt::TAG_EBDT, &data, &t, r)),
        [("unknown_format", 4)]
    );
    let loc = eblc_with(index_offsets(1, 1, 0, &[4, 5000]), 0, 0);
    let t = Cblc::new(&loc).unwrap();
    assert_eq!(
        ids(|r| cbdt::validate_images(cbdt::TAG_EBDT, &data, &t, r)),
        [("offset_out_of_bounds", 4)]
    );
    for format in [1u16, 2, 5, 6, 7, 8, 9, 18, 19] {
        let loc = eblc_with(index_offsets(1, format, 0, &[4, 6]), 0, 0);
        let t = Cblc::new(&loc).unwrap();
        let got = ids(|r| cbdt::validate_images(cbdt::TAG_EBDT, &data, &t, r));
        // Two bytes are too short for every format with metrics or a
        // PNG length; format 5 has neither.
        if format == 5 {
            assert!(got.is_empty(), "{format}");
        } else {
            assert_eq!(got, [("truncated", 4)], "{format}");
        }
    }
}

#[test]
fn sbix_strikes_and_glyphs() {
    let png = png_header(3, 4);
    let d = sbix(
        1,
        &[
            (
                20,
                72,
                vec![
                    None,
                    Some((1, -2, *b"png ", png.clone())),
                    Some((0, 0, *b"dupe", vec![0, 1])),
                ],
            ),
            (
                40,
                144,
                vec![
                    None,
                    Some((0, 0, *b"flip", vec![0, 1])),
                    Some((0, 0, *b"jpg ", vec![1, 2, 3])),
                ],
            ),
        ],
    );
    let s = Sbix::new(&d).unwrap();
    assert_eq!((s.version(), s.flags(), s.num_strikes()), (1, 1, 2));
    let st = s.strike(0).unwrap();
    assert_eq!((st.ppem(), st.ppi()), (20, 72));
    assert_eq!(st.glyph(0), None);
    let g = st.glyph(1).unwrap();
    assert_eq!(
        (g.origin_x, g.origin_y, g.graphic_type, g.data),
        (1, -2, Tag::new(b"png "), &png[..])
    );
    assert_eq!(st.glyph(2).unwrap().data, &[0, 1]);
    assert_eq!(
        s.strike(1).unwrap().glyph(2).unwrap().graphic_type,
        Tag::new(b"jpg ")
    );
    assert!(ids(|r| sbix::validate(&d, Some(3), r)).is_empty());
    // A dupe past the glyphs, bad flags, a strike past the table.
    let d = sbix(2, &[(20, 72, vec![Some((0, 0, *b"dupe", vec![0, 9]))])]);
    assert_eq!(
        ids(|r| sbix::validate(&d, Some(1), r)),
        [("reserved_bits_set", 2), ("glyph_id_out_of_range", 16)]
    );
    let mut d = sbix(1, &[(20, 72, vec![None])]);
    d[8..12].copy_from_slice(&500u32.to_be_bytes());
    assert_eq!(
        ids(|r| sbix::validate(&d, Some(1), r)),
        [("offset_out_of_bounds", 8)]
    );
    assert_eq!(
        ids(|r| sbix::validate(&d[..6], None, r)),
        [("truncated", 0)]
    );
}

#[test]
fn svg_documents() {
    let d = svg_table(&[(1, 3, b"<svg a/>".to_vec()), (5, 5, b"<svg b/>".to_vec())]);
    let s = Svg::new(&d).unwrap();
    assert_eq!(s.num_documents(), 2);
    assert_eq!(s.find(2), Some(0));
    assert_eq!(s.find(5), Some(1));
    assert_eq!(s.find(4), None);
    assert_eq!(s.document(1), Some(&b"<svg b/>"[..]));
    let r = s.record(0).unwrap();
    assert_eq!((r.start_glyph, r.end_glyph, r.length), (1, 3, 8));
    assert!(ids(|rep| svg::validate(&d, rep)).is_empty());
    // A document cut by the end of the table, and unsorted entries.
    let mut d = svg_table(&[(5, 5, b"<svg/>".to_vec()), (1, 3, b"<svg/>".to_vec())]);
    d.truncate(d.len() - 2);
    let s = Svg::new(&d).unwrap();
    assert_eq!(s.document(1), Some(&b"<svg"[..]));
    assert_eq!(
        ids(|rep| svg::validate(&d, rep)),
        [("records_not_sorted", 24), ("offset_out_of_bounds", 24)]
    );
    assert_eq!(ids(|rep| svg::validate(&d[..8], rep)), [("truncated", 0)]);
}
