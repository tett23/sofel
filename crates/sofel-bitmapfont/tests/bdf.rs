//! sofel ADR 0065 I1 and I2 against FreeType 2.15.0 BDF behavior.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use sofel_bitmapfont::BdfFont;

const ORDER: &str = include_str!("data/order.bdf");

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

#[test]
fn adr0065_i1_bdf_line_endings_preserve_bitmaps_and_offsets() {
    let reference = BdfFont::parse(ORDER.as_bytes()).unwrap();
    for ending in ["\r", "\r\n", "\n", "\n\r"] {
        let source = ORDER.replace('\n', ending);
        let face = BdfFont::parse(source.as_bytes()).unwrap();
        for id in 0..3 {
            assert_eq!(face.glyph(id), reference.glyph(id));
        }
        let broken = source.replacen(
            &format!("{ending}FF{ending}"),
            &format!("{ending}FG{ending}"),
            1,
        );
        assert_eq!(
            BdfFont::parse_with_offset(broken.as_bytes()).unwrap_err(),
            broken.find("FG").unwrap()
        );
    }
}

#[test]
fn adr0065_i1_bdf_alternative_encoding_selects_the_same_glyph() {
    let reference = BdfFont::parse(ORDER.as_bytes()).unwrap();
    for value in ["-1 65", "-2 65"] {
        let source = ORDER.replace("ENCODING 65", &format!("ENCODING {value}"));
        let font = BdfFont::parse(source.as_bytes()).unwrap();
        assert_eq!(font.nominal_glyph(65), reference.nominal_glyph(65));
        assert_eq!(font.glyph(1), reference.glyph(1));
    }
    let source = ORDER.replace("ENCODING 65", "ENCODING 1114112");
    let font = BdfFont::parse(source.as_bytes()).unwrap();
    assert_eq!(font.nominal_glyph(1114112), None);
}

#[test]
fn adr0065_i1_bdf_strike_uses_size_properties() {
    for (properties, expected) in [
        ("POINT_SIZE 80", (510, 510)),
        ("PIXEL_SIZE 9", (576, 576)),
        ("RESOLUTION_X 96\nRESOLUTION_Y 75", (682, 533)),
        (
            "PIXEL_SIZE -9\nRESOLUTION_X 96\nRESOLUTION_Y 72",
            (768, 576),
        ),
    ] {
        let count = 3 + properties.lines().count();
        let source = ORDER.replace(
            "STARTPROPERTIES 3",
            &format!("STARTPROPERTIES {count}\n{properties}"),
        );
        assert_eq!(
            BdfFont::parse(source.as_bytes()).unwrap().ppem_26_6(),
            expected
        );
    }
}
