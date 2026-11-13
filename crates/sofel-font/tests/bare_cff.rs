use sofel_font::{BareCffFace, Cff};

const TWO_GLYPHS: &[u8] = &[
    1, 0, 4, 4, // Header
    0, 1, 1, 1, 2, b'A', // Name INDEX
    0, 1, 1, 1, 3, 160, 17, // Top DICT INDEX
    0, 0, // String INDEX
    0, 0, // Global Subrs INDEX
    0, 2, 1, 1, 2, 3, 14, 14, // CharStrings INDEX
];

const WIDTH_500: &[u8] = &[
    1, 0, 4, 4, 0, 1, 1, 1, 2, b'A', 0, 1, 1, 1, 3, 160, 17, 0, 0, 0, 0, 0, 1, 1, 1, 6, 248, 136,
    139, 22, 14,
];

// One .notdef glyph. The Top DICT's CharStrings offset is 21.
const BARE_CFF: &[u8] = &[
    1, 0, 4, 4, // Header
    0, 1, 1, 1, 2, b'A', // Name INDEX
    0, 1, 1, 1, 3, 160, 17, // Top DICT INDEX
    0, 0, // String INDEX
    0, 0, // Global Subrs INDEX
    0, 1, 1, 1, 2, 14, // CharStrings INDEX
];

// Two names select different Top DICTs; the second has UniqueID 42.
const TWO_FACES: &[u8] = &[
    1, 0, 4, 4, // Header
    0, 2, 1, 1, 2, 3, b'A', b'B', // Name INDEX
    0, 2, 1, 1, 3, 7, 167, 17, 167, 17, 181, 13, // Top DICT INDEX
    0, 0, // String INDEX
    0, 0, // Global Subrs INDEX
    0, 1, 1, 1, 2, 14, // CharStrings INDEX
];

#[test]
fn adr0065_i1_bare_cff_opens_face_zero() {
    assert_eq!(
        Cff::from_bare(BARE_CFF, 0).map(|face| face.num_glyphs()),
        Ok(1)
    );
    assert_eq!(
        Cff::from_bare(BARE_CFF, 0).map(|face| face.ft_glyph_name(sofel_font::read::GlyphId(0))),
        Ok(Some(&b".notdef"[..]))
    );
}

#[test]
fn adr0065_i2_bare_cff_rejects_invalid_index_and_truncation() {
    assert!(Cff::from_bare(BARE_CFF, 1).is_err());
    assert!(Cff::from_bare(BARE_CFF.get(..BARE_CFF.len() - 1).unwrap_or_default(), 0).is_err());
}

#[test]
fn adr0065_i1_bare_cff_selects_last_face() {
    assert_eq!(
        Cff::from_bare(TWO_FACES, 0).map(|face| face.top_dict().unique_id),
        Ok(0)
    );
    assert_eq!(
        Cff::from_bare(TWO_FACES, 1).map(|face| face.top_dict().unique_id),
        Ok(42)
    );
    assert!(Cff::from_bare(TWO_FACES, 2).is_err());
}

#[test]
fn adr0065_i1_bare_cff_character_mapping_matches_postscript_names() {
    let face = BareCffFace::new(TWO_GLYPHS, 0);
    assert_eq!(face.as_ref().map(BareCffFace::num_glyphs), Ok(2));
    assert_eq!(
        face.as_ref().ok().and_then(|face| face.nominal_glyph(32)),
        Some(sofel_font::read::GlyphId(1))
    );
    assert_eq!(
        face.as_ref().ok().and_then(|face| face.encoded_glyph(32)),
        Some(sofel_font::read::GlyphId(1))
    );
    assert_eq!(
        face.as_ref()
            .ok()
            .and_then(|face| face.glyph_name(sofel_font::read::GlyphId(1))),
        Some(&b"space"[..])
    );
}

#[test]
fn adr0065_i1_bare_cff_reads_charstring_advance() {
    let face = BareCffFace::new(WIDTH_500, 0);
    assert_eq!(
        face.as_ref()
            .ok()
            .and_then(|face| face.h_advance(sofel_font::read::GlyphId(0))),
        Some(500)
    );
}
