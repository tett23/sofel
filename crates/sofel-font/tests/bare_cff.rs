use sofel_font::Cff;

// One .notdef glyph. The Top DICT's CharStrings offset is 21.
const BARE_CFF: &[u8] = &[
    1, 0, 4, 4, // Header
    0, 1, 1, 1, 2, b'A', // Name INDEX
    0, 1, 1, 1, 3, 160, 17, // Top DICT INDEX
    0, 0, // String INDEX
    0, 0, // Global Subrs INDEX
    0, 1, 1, 1, 2, 14, // CharStrings INDEX
];

// Two names select different Top DICTs and CharStrings INDEXes.
const TWO_FACES: &[u8] = &[
    1, 0, 4, 4, // Header
    0, 2, 1, 1, 2, 3, b'A', b'B', // Name INDEX
    0, 2, 1, 1, 3, 5, 165, 17, 171, 17, // Top DICT INDEX
    0, 0, // String INDEX
    0, 0, // Global Subrs INDEX
    0, 1, 1, 1, 2, 14, // first CharStrings INDEX
    0, 2, 1, 1, 2, 3, 14, 14, // second CharStrings INDEX
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
        Cff::from_bare(TWO_FACES, 0).map(|face| face.num_glyphs()),
        Ok(1)
    );
    assert_eq!(
        Cff::from_bare(TWO_FACES, 1).map(|face| face.num_glyphs()),
        Ok(2)
    );
    assert!(Cff::from_bare(TWO_FACES, 2).is_err());
}
