use sofel_type1::face::Type1Face;
use sofel_type1::source::Type1Source;

#[test]
fn adr0065_i1_type1_face_maps_character_to_glyph() -> Result<(), Box<dyn std::error::Error>> {
    let pfa = b"%!PS-AdobeFont-1.0\n/Encoding 256 array\ndup 65/A put\nreadonly def\ncurrentfile eexec\nb8588da5424438407bb191bb43917fe221abd99deb2b4445bd5dc6bc878aed52c60104d4fb73902409cd1f6e6f4d242ad4e3209df6508dce5468e50a62bb81ca1078b9f3529710710c06cdde6e7146c338ce\ncleartomark\n";
    let source = Type1Source::parse_pfa(pfa, 4096)?;
    let face = Type1Face::new(&source)?;
    assert_eq!(face.num_glyphs(), 2);
    assert_eq!(face.nominal_glyph(b'A'), Some(1));
    assert_eq!(face.nominal_glyph(b'B'), None);
    assert_eq!(face.glyph_name(1), Some(&b"A"[..]));
    assert_eq!(face.glyph_program(1), Some(vec![0x8c, 0x0e]));
    Ok(())
}
