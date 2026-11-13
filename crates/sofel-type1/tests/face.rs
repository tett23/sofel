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

#[test]
fn adr0065_i1_type1_attached_afm_changes_pair_kerning() -> Result<(), Box<dyn std::error::Error>> {
    let pfa = b"%!PS-AdobeFont-1.0\n/Encoding 256 array\ndup 65/A put\nreadonly def\ncurrentfile eexec\nb8588da5424438407bb191bb43917fe221abd99deb2b4445bd5dc6bc878aed52c60104d4fb73902409cd1f6e6f4d242ad4e3209df6508dce5468e50a62bb81ca1078b9f3529710710c06cdde6e7146c338ce\ncleartomark\n";
    let source = Type1Source::parse_pfa(pfa, 4096)?;
    let mut face = Type1Face::new(&source)?;
    assert_eq!(face.kerning(1, 1), (0, 0));
    let afm =
        b"StartFontMetrics 4.1\nStartKernPairs 1\nKPX A A -80\nEndKernPairs\nEndFontMetrics\n";
    face.attach_afm(afm)?;
    assert_eq!(face.kerning(1, 1), (-80, 0));
    assert_eq!(face.kerning(0, 1), (0, 0));
    Ok(())
}

#[test]
fn adr0065_i1_type1_attached_pfm_changes_pair_kerning() -> Result<(), Box<dyn std::error::Error>> {
    let pfa = b"%!PS-AdobeFont-1.0\n/Encoding 256 array\ndup 65/A put\nreadonly def\ncurrentfile eexec\nb8588da5424438407bb191bb43917fe221abd99deb2b4445bd5dc6bc878aed52c60104d4fb73902409cd1f6e6f4d242ad4e3209df6508dce5468e50a62bb81ca1078b9f3529710710c06cdde6e7146c338ce\ncleartomark\n";
    let source = Type1Source::parse_pfa(pfa, 4096)?;
    let mut face = Type1Face::new(&source)?;
    let mut pfm = vec![0u8; 143];
    pfm.get_mut(1).ok_or("version")?.clone_from(&1);
    pfm.get_mut(2..6)
        .ok_or("file size")?
        .copy_from_slice(&143u32.to_le_bytes());
    pfm.get_mut(119..121)
        .ok_or("extension size")?
        .copy_from_slice(&18u16.to_le_bytes());
    pfm.get_mut(133..137)
        .ok_or("pair offset")?
        .copy_from_slice(&137u32.to_le_bytes());
    pfm.get_mut(137..139)
        .ok_or("pair count")?
        .copy_from_slice(&1u16.to_le_bytes());
    pfm.get_mut(139..143)
        .ok_or("pair")?
        .copy_from_slice(&[b'A', b'A', 0xec, 0xff]);
    face.attach_pfm(&pfm)?;
    assert_eq!(face.kerning(1, 1), (-20, 0));
    assert_eq!(face.kerning(0, 1), (0, 0));
    Ok(())
}
