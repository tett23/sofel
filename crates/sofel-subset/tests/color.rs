//! COLR, CPAL, CBLC, CBDT and sbix on hand-made fonts (ADR 0022, stage
//! 9): the COLR closure, the renumbered layers and palette entries, the
//! version 1 paints, the bitmap strikes and the variation store left to
//! stage 10.

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
use sofel_font::read::cblc::Cblc;
use sofel_font::read::colr::Colr;
use sofel_font::read::cpal::Cpal;
use sofel_font::read::sbix::Sbix;
use sofel_font::read::sfnt::Sfnt;
use sofel_subset::{Input, SetKind, SubsetError, subset};

/// Glyph id of a capital letter of the sample font ('A' is 1).
fn g(c: char) -> u16 {
    c as u16 - 0x40
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

fn num_glyphs(out: &[u8]) -> u32 {
    Font::new(out, 0).unwrap().num_glyphs()
}

/// Four colors per palette, two palettes sharing nothing.
fn cpal_table(v1: bool) -> Vec<u8> {
    let records: Vec<[u8; 4]> = (0..8u8).map(|i| [i, i, i, 0xFF]).collect();
    cpal(
        4,
        &[0, 4],
        &records,
        v1.then(|| {
            (
                Some(vec![1, 2]),
                Some(vec![300, 301]),
                Some(vec![400, 401, 402, 403]),
            )
        }),
    )
}

/// 'A' is drawn with glyphs 20 (palette entry 3) and 21 (entry 1), 'B'
/// with glyph 22 (entry 2).
fn colr0() -> Vec<u8> {
    colr_v0(
        &[(g('A'), 0, 2), (g('B'), 2, 1)],
        &[(20, 3), (21, 1), (22, 2)],
    )
}

#[test]
fn colr_v0_closure_and_renumbering() {
    let data = font_with(vec![(b"COLR", colr0()), (b"CPAL", cpal_table(false))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    // .notdef, A and its two layer glyphs.
    assert_eq!(num_glyphs(&out), 4);
    let colr = out_table(&out, b"COLR").unwrap();
    let c = Colr::new(&colr).unwrap();
    assert_eq!(c.version(), 0);
    assert_eq!(c.num_base_glyphs(), 1);
    let base = c.base_glyph(0).unwrap();
    assert_eq!((base.glyph, base.first_layer, base.num_layers), (1, 0, 2));
    // Entries 1 and 3 are kept and become 0 and 1.
    assert_eq!(c.layer(0), Some((2, 1)));
    assert_eq!(c.layer(1), Some((3, 0)));
    let cpal = out_table(&out, b"CPAL").unwrap();
    let p = Cpal::new(&cpal).unwrap();
    assert_eq!(p.num_palette_entries(), 2);
    assert_eq!(p.num_palettes(), 2);
    assert_eq!(p.color(0, 0), Some([1, 1, 1, 0xFF]));
    assert_eq!(p.color(0, 1), Some([3, 3, 3, 0xFF]));
    assert_eq!(p.color(1, 0), Some([5, 5, 5, 0xFF]));
    assert_eq!(p.color(1, 1), Some([7, 7, 7, 0xFF]));
}

#[test]
fn colr_without_kept_color_glyphs_is_dropped() {
    let data = font_with(vec![(b"COLR", colr0()), (b"CPAL", cpal_table(false))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("C")).unwrap();
    assert_eq!(num_glyphs(&out), 2);
    assert!(out_table(&out, b"COLR").is_none());
    assert!(out_table(&out, b"CPAL").is_none());
}

#[test]
fn palettes_with_the_same_kept_colors_share_records() {
    // Entry 1 is the same color in both palettes.
    let mut records: Vec<[u8; 4]> = (0..8u8).map(|i| [i, i, i, 0xFF]).collect();
    records[5] = records[1];
    let cpal_t = cpal(4, &[0, 4], &records, None);
    let colr = colr_v0(&[(g('A'), 0, 1)], &[(20, 1)]);
    let data = font_with(vec![(b"COLR", colr), (b"CPAL", cpal_t)]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let cpal = out_table(&out, b"CPAL").unwrap();
    let p = Cpal::new(&cpal).unwrap();
    assert_eq!(p.num_palette_entries(), 1);
    assert_eq!(p.num_color_records(), 1);
    assert_eq!(p.first_color_record(0), Some(0));
    assert_eq!(p.first_color_record(1), Some(0));
}

#[test]
fn cpal_v1_labels_follow_the_kept_entries() {
    let data = font_with(vec![(b"COLR", colr0()), (b"CPAL", cpal_table(true))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    let cpal = out_table(&out, b"CPAL").unwrap();
    let p = Cpal::new(&cpal).unwrap();
    assert_eq!(p.version(), 1);
    assert_eq!(p.palette_flags(1), 2);
    assert_eq!(p.palette_name_id(0), 300);
    assert_eq!(p.color_name_id(0), 401);
    assert_eq!(p.color_name_id(1), 403);
}

/// PaintSolid of palette entry `p`.
fn solid(p: u16) -> Vec<u8> {
    Buf::new().u8(2).u16(p).u16(0x4000).done()
}

/// A COLRv1 table: 'A' paints glyph 23 with entry 2 (PaintGlyph over
/// PaintSolid), 'B' is a PaintColrLayers of layers 1 and 2 (glyph 24
/// with entry 3, then PaintColrGlyph of 'A'), 'C' has a version 0 record
/// only (glyph 25, entry 1).
fn colr1() -> Vec<u8> {
    // Paint 0: PaintGlyph (6 bytes) to paint 1 right after it.
    let paint_glyph = |gid: u16| Buf::new().u8(10).u24(6).u16(gid).done();
    let paints = vec![
        paint_glyph(23),                      // 0
        solid(2),                             // 1
        Buf::new().u8(1).u8(2).u32(1).done(), // 2: layers 1, 2
        paint_glyph(24),                      // 3
        solid(3),                             // 4
        Buf::new().u8(11).u16(g('A')).done(), // 5: PaintColrGlyph
        paint_glyph(26),                      // 6: unused layer 0
        solid(0),                             // 7
    ];
    colr_v1(
        &[(g('C'), 0, 1)],
        &[(25, 1)],
        &paints,
        &[(g('A'), 0), (g('B'), 2)],
        &[6, 3, 5],
        &[(
            g('A'),
            g('B'),
            Buf::new().u8(1).u16(0).u16(0).u16(100).u16(100).done(),
        )],
    )
}

#[test]
fn colr_v1_closure_and_paints() {
    let data = font_with(vec![(b"COLR", colr1()), (b"CPAL", cpal_table(false))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    // .notdef, A and the glyph its PaintGlyph fills.
    assert_eq!(num_glyphs(&out), 3);
    let colr = out_table(&out, b"COLR").unwrap();
    let c = Colr::new(&colr).unwrap();
    assert_eq!(c.version(), 1);
    assert_eq!(c.num_base_glyph_paints(), 1);
    let (gid, paint) = c.base_glyph_paint_record(0).unwrap();
    assert_eq!(gid, 1);
    // PaintGlyph of the new glyph 2 over PaintSolid of entry 2, now 0.
    assert_eq!(colr[paint], 10);
    assert_eq!(u16::from_be_bytes([colr[paint + 4], colr[paint + 5]]), 2);
    let solid_at =
        paint + u32::from_be_bytes([0, colr[paint + 1], colr[paint + 2], colr[paint + 3]]) as usize;
    assert_eq!(&colr[solid_at..solid_at + 5], &[2, 0, 0, 0x40, 0]);
    assert_eq!(c.num_layer_paints(), 0, "no layer is used");
    assert_eq!(c.num_clips(), 1);
    assert_eq!(c.clip_record(0).map(|r| (r.0, r.1)), Some((1, 1)));
    let cpal = out_table(&out, b"CPAL").unwrap();
    assert_eq!(Cpal::new(&cpal).unwrap().num_palette_entries(), 1);
}

#[test]
fn colr_v1_layers_are_renumbered() {
    let data = font_with(vec![(b"COLR", colr1()), (b"CPAL", cpal_table(false))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("B")).unwrap();
    // B, the layer glyph 24, and A with its glyph 23 through
    // PaintColrGlyph.
    assert_eq!(num_glyphs(&out), 5);
    let colr = out_table(&out, b"COLR").unwrap();
    let c = Colr::new(&colr).unwrap();
    assert_eq!(c.num_base_glyph_paints(), 2);
    assert_eq!(c.num_layer_paints(), 2);
    let (_, paint) = c.base_glyph_paint_record(1).unwrap();
    // PaintColrLayers now starts at layer 0.
    assert_eq!(&colr[paint..paint + 6], &[1, 2, 0, 0, 0, 0]);
}

#[test]
fn colr_v1_without_v1_glyphs_becomes_version_0() {
    let data = font_with(vec![(b"COLR", colr1()), (b"CPAL", cpal_table(false))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("C")).unwrap();
    let colr = out_table(&out, b"COLR").unwrap();
    assert_eq!(colr.len(), 14 + 6 + 4);
    let c = Colr::new(&colr).unwrap();
    assert_eq!(c.version(), 0);
    assert_eq!(c.layer(0), Some((2, 0)));
}

#[test]
fn colr_variation_store_is_stage_10() {
    let mut colr = colr1();
    let len = colr.len() as u32;
    colr[30..34].copy_from_slice(&len.to_be_bytes());
    colr.extend_from_slice(&item_variation_store(1, &[], &[]));
    let data = font_with(vec![(b"COLR", colr), (b"CPAL", cpal_table(false))]);
    let font = Font::new(&data, 0).unwrap();
    let static_paints = subset(&font, &chars("AB")).unwrap();
    let table = out_table(&static_paints, b"COLR").unwrap();
    assert_eq!(&table[26..34], &[0; 8]);
    let v0 = subset(&font, &chars("C")).unwrap();
    let table = out_table(&v0, b"COLR").unwrap();
    assert_eq!(&table[..2], &[0, 0]);
    let without_store = font_with(vec![(b"COLR", colr1()), (b"CPAL", cpal_table(false))]);
    let without_store = Font::new(&without_store, 0).unwrap();
    assert_eq!(static_paints, subset(&without_store, &chars("AB")).unwrap());
    assert_eq!(v0, subset(&without_store, &chars("C")).unwrap());
    // Dropping COLR leaves CPAL without palette entries to keep.
    let input = chars("A").map_set(SetKind::DropTables, |s| s.add(Tag::new(b"COLR").to_u32()));
    let out = subset(&font, &input).unwrap();
    assert!(out_table(&out, b"CPAL").is_none());
}

#[test]
fn retained_variable_paint_or_clip_needs_the_colr_store() {
    let variable = Buf::new().u8(3).u16(0).u16(0x4000).u32(0).done();
    let clip = Buf::new()
        .u8(2)
        .u16(0)
        .u16(0)
        .u16(100)
        .u16(100)
        .u32(1)
        .done();
    let mut colr = colr_v1(
        &[],
        &[],
        &[solid(0), variable],
        &[(g('A'), 0), (g('B'), 1), (g('C'), 0)],
        &[],
        &[(g('C'), g('C'), clip)],
    );
    let offset = colr.len() as u32;
    colr[30..34].copy_from_slice(&offset.to_be_bytes());
    colr.extend_from_slice(&item_variation_store(1, &[], &[]));
    let data = font_with(vec![(b"COLR", colr), (b"CPAL", cpal_table(false))]);
    let font = Font::new(&data, 0).unwrap();
    assert!(subset(&font, &chars("A")).is_ok());
    for selected in ["B", "C"] {
        assert!(
            matches!(subset(&font, &chars(selected)), Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"COLR"))
        );
    }
}

/// CBDT with 3 image blocks for 'A', 'B' and 'C', and CBLC with one strike
/// whose format `index_format` subtable covers them, plus a strike for
/// 'Z' only.
fn bitmap_font(index_format: u16) -> Vec<u8> {
    let cbdt = Buf::new()
        .u16(3)
        .u16(0)
        .bytes(b"aaaa")
        .bytes(b"bbbbbb")
        .bytes(b"cc")
        .bytes(b"zzz")
        .done();
    let abc = SizeSpec {
        start_glyph: g('A'),
        end_glyph: g('C'),
        subtables: vec![(
            g('A'),
            g('C'),
            index_offsets(index_format, 17, 4, &[0, 4, 10, 12]),
        )],
        ..SizeSpec::default()
    };
    let z = SizeSpec {
        ppem_x: 32,
        ppem_y: 32,
        start_glyph: g('Z'),
        end_glyph: g('Z'),
        subtables: vec![(g('Z'), g('Z'), index_offsets(index_format, 17, 16, &[0, 3]))],
        ..SizeSpec::default()
    };
    font_with(vec![(b"CBDT", cbdt), (b"CBLC", cblc(3, &[abc, z]))])
}

#[test]
fn cblc_and_cbdt_keep_the_strikes_of_kept_glyphs() {
    for format in [1, 3] {
        let data = bitmap_font(format);
        let font = Font::new(&data, 0).unwrap();
        let out = subset(&font, &chars("AC")).unwrap();
        let cbdt = out_table(&out, b"CBDT").unwrap();
        assert_eq!(cbdt, b"\x00\x03\x00\x00aaaacc");
        let cblc = out_table(&out, b"CBLC").unwrap();
        let c = Cblc::new(&cblc).unwrap();
        assert_eq!(c.num_sizes(), 1, "the strike of Z is dropped");
        let size = c.size(0).unwrap();
        assert_eq!((size.start_glyph, size.end_glyph), (1, 2));
        let rec = c.index_record(&size, 0).unwrap();
        for (gid, data) in [(1u32, &b"aaaa"[..]), (2, b"cc")] {
            let loc = c.locate(&rec, gid).unwrap();
            let at = loc.offset as usize;
            assert_eq!(&cbdt[at..at + loc.length as usize], data);
        }
    }
}

#[test]
fn cblc_without_kept_bitmaps_keeps_empty_tables() {
    let data = bitmap_font(1);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("M")).unwrap();
    assert_eq!(out_table(&out, b"CBDT").unwrap(), b"\x00\x03\x00\x00");
    let cblc = out_table(&out, b"CBLC").unwrap();
    assert_eq!(cblc, b"\x00\x03\x00\x00\x00\x00\x00\x00");
}

#[test]
fn sbix_keeps_the_strikes_of_kept_glyphs() {
    let glyph = |c: u8| Some((1i16, 2i16, *b"png ", vec![c; 3]));
    let mut glyphs: Vec<_> = (0..28).map(|_| None).collect();
    glyphs[g('A') as usize] = glyph(b'a');
    glyphs[g('B') as usize] = glyph(b'b');
    let mut only_b: Vec<_> = (0..28).map(|_| None).collect();
    only_b[g('B') as usize] = glyph(b'B');
    let table = sbix(1, &[(16, 72, glyphs), (32, 72, only_b)]);
    let data = font_with(vec![(b"sbix", table)]);
    let font = Font::new(&data, 0).unwrap();
    let input = chars("A").map_set(SetKind::DropTables, |s| {
        s.remove(Tag::new(b"sbix").to_u32())
    });
    let out = subset(&font, &input).unwrap();
    let sbix_out = out_table(&out, b"sbix").unwrap();
    let s = Sbix::new(&sbix_out).unwrap();
    assert_eq!(s.num_strikes(), 1);
    let strike = s.strike(0).unwrap();
    assert_eq!(strike.ppem(), 16);
    assert!(strike.glyph(0).is_none());
    let a = strike.glyph(1).unwrap();
    assert_eq!(a.data, b"aaa");
}

#[test]
fn colr_paints_nested_too_deep_are_cut() {
    // 70 PaintTranslate, each to the next, then PaintSolid: deeper than
    // HarfBuzz's sanitizer accepts, which neuters the innermost offset:
    // the glyph's paint fails and COLR is dropped.
    let mut paints: Vec<Vec<u8>> = (0..70)
        .map(|_| Buf::new().u8(14).u24(8).u16(0).u16(0).done())
        .collect();
    paints.push(solid(0));
    let colr = colr_v1(&[], &[], &paints, &[(g('A'), 0)], &[], &[]);
    let data = font_with(vec![(b"COLR", colr), (b"CPAL", cpal_table(false))]);
    let font = Font::new(&data, 0).unwrap();
    let out = subset(&font, &chars("A")).unwrap();
    assert!(out_table(&out, b"COLR").is_none());
}
