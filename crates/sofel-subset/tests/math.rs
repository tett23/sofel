//! MATH on hand-made fonts (ADR 0022, stage 8): the glyph closure over
//! the size variants and assemblies, and the subset table read back with
//! the `sofel-read` view.

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
use sofel_font::read::math::{Corner, Math};
use sofel_font::read::sfnt::Sfnt;
use sofel_font::read::{GlyphId, Tag};
use sofel_subset::{Input, SetKind, SubsetError, subset};

/// Glyph id of a capital letter of the sample font ('A' is 1).
fn g(c: char) -> u16 {
    c as u16 - 0x40
}

/// Appends a MathValueRecord of `v` without a device.
fn value(b: Buf, v: i16) -> Buf {
    b.i16(v).u16(0)
}

/// A MathGlyphConstruction with `variants` and an assembly of `parts`
/// (none when empty).
fn construction(variants: &[u16], parts: &[u16]) -> Vec<u8> {
    let mut b = Buf::new().u16(0).u16(variants.len() as u16);
    for v in variants {
        b = b.u16(*v).u16(1000);
    }
    let assembly = if parts.is_empty() {
        Vec::new()
    } else {
        let mut a = Buf::new().i16(7).u16(0).u16(parts.len() as u16);
        for p in parts {
            a = a.u16(*p).u16(10).u16(20).u16(300).u16(0);
        }
        a.done()
    };
    link(b.done(), vec![(0, 2, assembly)])
}

/// The MATH table of the tests:
/// - constants with a hinting device on `mathLeading` and a
///   VariationIndex device on `axisHeight`;
/// - italics corrections of A and C, a top accent of C, extended shapes
///   A to C and a top right kern of A;
/// - vertical constructions of A (variant X, part Y), B (variant Z) and
///   X (variant V), and a horizontal one of C (variant W).
fn math_table() -> Vec<u8> {
    let mut k = Buf::new().i16(80).i16(60).u16(1500).u16(2000);
    for i in 0..51 {
        k = value(k, i as i16);
    }
    let k = k.i16(55).done();
    let k = link(
        k,
        vec![
            (10, 2, Buf::new().u16(9).u16(9).u16(1).u16(0x4000).done()),
            (14, 2, Buf::new().u16(0).u16(3).u16(0x8000).done()),
        ],
    );
    let italics = link(
        value(value(Buf::new().u16(0).u16(2), 30), -40).done(),
        vec![(0, 2, coverage1(&[g('A'), g('C')]))],
    );
    let accent = link(
        value(Buf::new().u16(0).u16(1), 250).done(),
        vec![(0, 2, coverage1(&[g('C')]))],
    );
    let kern = value(value(value(Buf::new().u16(1), 100), -10), -20).done();
    let kern_info = link(
        Buf::new().u16(0).u16(1).u16(0).u16(0).u16(0).u16(0).done(),
        vec![(0, 2, coverage1(&[g('A')])), (4, 2, kern)],
    );
    let info = link(
        Buf::new().u16(0).u16(0).u16(0).u16(0).done(),
        vec![
            (0, 2, italics),
            (2, 2, accent),
            (4, 2, coverage2(&[(g('A'), g('C'), 0)])),
            (6, 2, kern_info),
        ],
    );
    let variants = link(
        Buf::new()
            .u16(50)
            .u16(0)
            .u16(0)
            .u16(3)
            .u16(1)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .done(),
        vec![
            (2, 2, coverage1(&[g('A'), g('B'), g('X')])),
            (4, 2, coverage1(&[g('C')])),
            (10, 2, construction(&[g('X')], &[g('Y')])),
            (12, 2, construction(&[g('Z')], &[])),
            (14, 2, construction(&[g('V')], &[])),
            (16, 2, construction(&[g('W')], &[])),
        ],
    );
    link(
        Buf::new().u32(0x0001_0000).u16(0).u16(0).u16(0).done(),
        vec![(4, 2, k), (6, 2, info), (8, 2, variants)],
    )
}

fn font_with(math: Vec<u8>) -> Vec<u8> {
    let mut t = sample_tables();
    t.push((b"MATH", math));
    t.sort_by_key(|(t, _)| **t);
    sfnt(&t)
}

fn chars(s: &str) -> Input {
    Input::new().map_set(SetKind::Unicodes, |set| {
        s.chars().for_each(|c| set.add(c as u32));
    })
}

fn num_glyphs(out: &[u8]) -> u32 {
    Font::new(out, 0).unwrap().num_glyphs()
}

fn out_math(out: &[u8]) -> Vec<u8> {
    Sfnt::new(out)
        .unwrap()
        .table(Tag::new(b"MATH"))
        .unwrap()
        .to_vec()
}

#[test]
fn closure_adds_variants_and_parts_once() {
    let data = font_with(math_table());
    let font = Font::new(&data, 0).unwrap();
    // A brings X and Y; X's own variant V is not added (one pass).
    let out = subset(&font, &chars("A")).unwrap();
    assert_eq!(num_glyphs(&out), 4);
    // B brings Z; C brings W.
    let out = subset(&font, &chars("BC")).unwrap();
    assert_eq!(num_glyphs(&out), 5);
    // X itself brings V.
    let out = subset(&font, &chars("X")).unwrap();
    assert_eq!(num_glyphs(&out), 3);
    // Without MATH there is no closure.
    let input = chars("A").map_set(SetKind::DropTables, |s| {
        s.add(Tag::new(b"MATH").to_u32());
    });
    let out = subset(&font, &input).unwrap();
    assert_eq!(num_glyphs(&out), 2);
    assert!(Sfnt::new(&out).unwrap().table(Tag::new(b"MATH")).is_none());
}

#[test]
fn subset_table() {
    let data = font_with(math_table());
    let font = Font::new(&data, 0).unwrap();
    // Glyphs 0, A, X, Y become 0, 1, 2, 3.
    let out = subset(&font, &chars("A")).unwrap();
    let table = out_math(&out);
    let m = Math::new(&table).unwrap();

    let k = m.constants().unwrap().unwrap();
    assert_eq!(k.script_percent_scale_down(), 80);
    assert_eq!(k.radical_degree_bottom_raise_percent(), 55);
    assert_eq!(k.value(50).unwrap().value(), 50);
    let leading = k.value(0).unwrap().device().unwrap().unwrap();
    assert_eq!((leading.start_size(), leading.delta_pixels(9)), (9, 1));
    // The VariationIndex device is dropped (no GDEF variation store).
    assert_eq!(k.value(1).unwrap().device_offset(), 0);

    let gi = m.glyph_info().unwrap().unwrap();
    let it = gi.italics_correction().unwrap().unwrap();
    assert_eq!(it.len(), 1);
    assert_eq!(it.get(GlyphId(1)).unwrap().value(), 30);
    // C is gone: the table stays, empty, with an empty coverage.
    let ta = gi.top_accent_attachment().unwrap().unwrap();
    assert_eq!(ta.len(), 0);
    assert_eq!(ta.coverage().unwrap().unwrap().num_records(), 0);
    let ext = gi.extended_shape_coverage().unwrap().unwrap();
    assert_eq!(ext.get(GlyphId(1)), Some(0));
    assert_eq!(ext.num_records(), 1);
    let ki = gi.kern_info().unwrap().unwrap();
    assert_eq!(ki.len(), 1);
    let kern = ki.kern(0, Corner::TopRight).unwrap().unwrap();
    assert_eq!(kern.kern_value(1).unwrap().value(), -20);
    assert!(ki.kern(0, Corner::TopLeft).unwrap().is_none());

    // Only the constructions of glyphs mapped by cmap are kept: X's is
    // dropped although X is kept (HarfBuzz filters by
    // `_glyphset_cmaped`).
    let v = m.variants().unwrap().unwrap();
    assert_eq!(v.min_connector_overlap(), 50);
    assert_eq!((v.vert_glyph_count(), v.horiz_glyph_count()), (1, 0));
    assert!(v.horiz_coverage().unwrap().is_none());
    assert_eq!(v.vert_coverage().unwrap().unwrap().get(GlyphId(1)), Some(0));
    let c = v.vert_construction(0).unwrap().unwrap();
    assert_eq!(c.variant(0), Some((GlyphId(2), 1000)));
    let a = c.glyph_assembly().unwrap().unwrap();
    assert_eq!(a.italics_correction().value(), 7);
    assert_eq!(a.part(0).unwrap().glyph, GlyphId(3));
    assert_eq!(a.part(0).unwrap().full_advance, 300);
}

#[test]
fn horizontal_constructions_follow_vertical_ones() {
    let data = font_with(math_table());
    let font = Font::new(&data, 0).unwrap();
    // Glyphs 0, B, C, W, Z become 0 to 4.
    let out = subset(&font, &chars("BC")).unwrap();
    let table = out_math(&out);
    let v = Math::new(&table).unwrap().variants().unwrap().unwrap();
    assert_eq!((v.vert_glyph_count(), v.horiz_glyph_count()), (1, 1));
    let vert = v.vert_construction(0).unwrap().unwrap();
    assert_eq!(vert.variant(0), Some((GlyphId(4), 1000)));
    assert!(vert.glyph_assembly().unwrap().is_none());
    let horiz = v.horiz_construction(0).unwrap().unwrap();
    assert_eq!(horiz.variant(0), Some((GlyphId(3), 1000)));
    assert_eq!(
        v.horiz_coverage().unwrap().unwrap().get(GlyphId(2)),
        Some(0)
    );
}

#[test]
fn unknown_versions_fail() {
    let mut math = math_table();
    math[1] = 2;
    let data = font_with(math);
    let font = Font::new(&data, 0).unwrap();
    assert_eq!(
        subset(&font, &chars("A")),
        Err(SubsetError::Malformed {
            tag: Tag::new(b"MATH")
        })
    );
}

#[test]
fn constructions_of_glyphs_past_the_font_fail() {
    // A's variant 40 is past the 28 glyphs: the closure drops it, and the
    // variant record cannot be mapped (HarfBuzz fails the subset too).
    let mut math = math_table();
    let at = math
        .windows(4)
        .position(|w| w == [0, 1, 0, g('X') as u8])
        .unwrap();
    math[at + 3] = 40;
    let data = font_with(math);
    let font = Font::new(&data, 0).unwrap();
    assert_eq!(
        subset(&font, &chars("A")),
        Err(SubsetError::Failed {
            tag: Tag::new(b"MATH")
        })
    );
}
