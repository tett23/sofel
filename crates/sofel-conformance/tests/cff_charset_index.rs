//! The charset index on the test fonts (ADR 0025): for every CFF face,
//! the CID and the glyph names through a `CharsetIndex` equal those
//! without it, for every glyph. The FreeType comparison of the CIDs
//! (`:cid` in `cff_info.rs`) is unchanged.

#![allow(clippy::unwrap_used, clippy::panic)]

use sofel_conformance::{fonts, paths};
use sofel_font::read::GlyphId;
use sofel_font::{Cff, Font, FontError};

#[test]
fn indexed_charset_queries_equal_the_plain_ones() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let (mut faces, mut glyphs, mut failed) = (0usize, 0usize, Vec::new());
    for path in fonts::harfbuzz_fonts(&testdata) {
        let id = fonts::id(&path, &testdata);
        let bytes = std::fs::read(&path).unwrap();
        for face in 0.. {
            let font = match Font::new(&bytes, face) {
                Ok(f) => f,
                Err(FontError::IndexOutOfRange) => break,
                Err(_) if face == 0 => break,
                Err(_) => continue,
            };
            let Ok(cff) = Cff::new(&font) else {
                continue;
            };
            let Some(charset) = cff.charset() else {
                continue;
            };
            let mut starts = vec![0u32; charset.num_ranges()];
            let index = charset.index_into(&mut starts).unwrap();
            faces = faces.saturating_add(1);
            for g in (0..cff.num_glyphs()).map(GlyphId) {
                glyphs = glyphs.saturating_add(1);
                let same = cff.cid_with(&index, g) == cff.cid(g)
                    && cff.charset_name_with(&index, g) == cff.charset_name(g)
                    && cff.ft_glyph_name_with(&index, g) == cff.ft_glyph_name(g)
                    && index.sid(g.0) == charset.sid(g.0);
                if !same {
                    failed.push(format!("{id}#{face} glyph {}", g.0));
                }
            }
        }
    }
    eprintln!(
        "{faces} CFF faces, {glyphs} glyphs, {} differ",
        failed.len()
    );
    assert!(faces > 0, "no CFF faces in the test fonts");
    assert!(failed.is_empty(), "{failed:#?}");
}
