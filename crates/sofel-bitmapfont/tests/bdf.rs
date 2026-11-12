//! sofel ADR 0065 I1 and I2 against FreeType 2.15.0 BDF behavior.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use sofel_bitmapfont::BdfFont;

const ORDER: &str = "STARTFONT 2.1\nFONT -misc-test-medium-r-normal--8-80-72-72-c-80-iso10646-1\nSIZE 8 72 72\nFONTBOUNDINGBOX 8 8 0 0\nSTARTPROPERTIES 3\nFONT_ASCENT 8\nFONT_DESCENT 0\nDEFAULT_CHAR 66\nENDPROPERTIES\nCHARS 2\nSTARTCHAR B\nENCODING 66\nSWIDTH 1000 0\nDWIDTH 8 0\nBBX 8 8 0 0\nBITMAP\nFF\n00\n00\n00\n00\n00\n00\n00\nENDCHAR\nSTARTCHAR A\nENCODING 65\nSWIDTH 1000 0\nDWIDTH 8 0\nBBX 8 8 0 0\nBITMAP\n18\n24\n42\n7E\n42\n42\n42\n00\nENDCHAR\nENDFONT\n";

#[test]
fn adr0065_i1_bdf_sorts_encoding_and_uses_default_char() {
    let font = BdfFont::parse(ORDER.as_bytes()).unwrap();
    assert_eq!(font.num_glyphs(), 3);
    assert_eq!(font.ppem_26_6(), (512, 512));
    assert_eq!(font.nominal_glyph(65), Some(1));
    assert_eq!(font.nominal_glyph(66), Some(2));
    assert_eq!(font.glyph(0).unwrap().bitmap[0], 0xff);
    assert_eq!(font.glyph(1).unwrap().bitmap[0], 0x18);
    assert_eq!(font.glyph(2).unwrap().bitmap[0], 0xff);
}

#[test]
fn adr0065_i2_bdf_rejects_missing_rows_and_count_overflow() {
    let truncated = ORDER.replace("42\n42\n42\n00\nENDCHAR", "42\n42\n42\nENDCHAR");
    assert!(BdfFont::parse(truncated.as_bytes()).is_err());
    let count = ORDER.replace("CHARS 2", "CHARS 2147483648");
    assert!(BdfFont::parse(count.as_bytes()).is_err());
    let bad_hex = ORDER.replacen("\nFF\n", "\nFG\n", 1);
    assert!(BdfFont::parse(bad_hex.as_bytes()).is_err());
    let missing_end = ORDER.replace("ENDFONT\n", "");
    assert!(BdfFont::parse(missing_end.as_bytes()).is_err());
    let missing_box = ORDER.replace("FONTBOUNDINGBOX 8 8 0 0\n", "");
    assert!(BdfFont::parse(missing_box.as_bytes()).is_err());
    let missing_properties_end = ORDER.replace("ENDPROPERTIES\n", "");
    assert!(BdfFont::parse(missing_properties_end.as_bytes()).is_err());
}
