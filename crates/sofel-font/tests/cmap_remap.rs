//! The cmap remapping of symbol and Macintosh subtables (ADR 0016,
//! deferred from ADR 0010).

#![allow(
    clippy::all,
    clippy::cast_possible_wrap,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::Font;
use sofel_font::read::GlyphId;

fn font(records: &[(u16, u16, Vec<u8>)], os2_table: Option<Vec<u8>>) -> Vec<u8> {
    let mut tables: Vec<_> = sample_tables()
        .into_iter()
        .filter(|(t, _)| *t != b"cmap")
        .collect();
    tables.push((b"cmap", cmap(records)));
    if let Some(o) = os2_table {
        tables.push((b"OS/2", o));
    }
    tables.sort_by_key(|(t, _)| **t);
    sfnt(&tables)
}

fn page(fs_selection: u16) -> Vec<u8> {
    os2(Os2Spec {
        version: 0,
        fs_selection,
        ..Default::default()
    })
}

fn g(v: u32) -> Option<GlyphId> {
    Some(GlyphId(v))
}

#[test]
fn symbol_fonts_map_the_first_256_to_the_pua() {
    let sym = cmap4(&[
        seg(0x41, 0x41, delta(2, 0x41)),
        seg(0xF041, 0xF042, delta(5, 0xF041)),
        seg(0xFFFF, 0xFFFF, 1),
    ]);
    let data = font(&[(3, 0, sym)], None);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.nominal_glyph(0x41), g(2)); // the subtable first
    assert_eq!(f.nominal_glyph(0x42), g(6)); // then U+F042
    assert_eq!(f.nominal_glyph(0xF041), g(5));
    assert_eq!(f.nominal_glyph(0x142), None); // only U+0000..U+00FF
}

#[test]
fn symbol_fonts_of_arabic_font_pages() {
    // U+0645 is U+F17E in the simplified and U+F2C2 in the traditional map.
    let sym = cmap4(&[
        seg(0xF020, 0xF020, delta(9, 0xF020)),
        seg(0xF17E, 0xF17E, delta(7, 0xF17E)),
        seg(0xF2C2, 0xF2C2, delta(8, 0xF2C2)),
        seg(0xFFFF, 0xFFFF, 1),
    ]);
    let simp = font(&[(3, 0, sym.clone())], Some(page(0xB200)));
    assert_eq!(Font::new(&simp, 0).unwrap().nominal_glyph(0x645), g(7));
    let trad = font(&[(3, 0, sym.clone())], Some(page(0xB300)));
    assert_eq!(Font::new(&trad, 0).unwrap().nominal_glyph(0x645), g(8));
    // Other pages: no remapping; no page (or OS/2 version 1+): U+F0xx.
    let hebrew = font(&[(3, 0, sym.clone())], Some(page(0xB100)));
    assert_eq!(Font::new(&hebrew, 0).unwrap().nominal_glyph(0x20), None);
    let v4 = font(
        &[(3, 0, sym)],
        Some(os2(Os2Spec {
            fs_selection: 0xB200,
            ..Default::default()
        })),
    );
    assert_eq!(Font::new(&v4, 0).unwrap().nominal_glyph(0x20), g(9));
}

#[test]
fn macroman_subtables_convert_unicode() {
    // MacRoman 0x80 is U+00C4, 0xCA is U+00A0.
    let mut ids = [0u8; 256];
    ids[0x41] = 3;
    ids[0x80] = 4;
    ids[0xCA] = 5;
    let data = font(&[(1, 0, cmap0(&ids))], None);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.nominal_glyph(0x41), g(3));
    assert_eq!(f.nominal_glyph(0xC4), g(4));
    assert_eq!(f.nominal_glyph(0x80), None);
    assert_eq!(f.nominal_glyph(0xA0), g(5));
    // HarfBuzz compares the low 16 bits of the code point.
    assert_eq!(f.nominal_glyph(0x100A0), g(5));
    // Other Macintosh encodings: ASCII only.
    let data = font(&[(1, 1, cmap0(&ids))], None);
    let f = Font::new(&data, 0).unwrap();
    assert_eq!(f.nominal_glyph(0x41), g(3));
    assert_eq!(f.nominal_glyph(0xC4), None);
    assert_eq!(f.nominal_glyph(0x80), None);
}

/// The idDelta mapping `start` to `glyph`, modulo 65536.
fn delta(glyph: u16, start: u16) -> i16 {
    glyph.wrapping_sub(start) as i16
}
