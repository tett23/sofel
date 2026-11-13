use sofel_type1::pfb::{Pfb, SectionKind, decode_pfb};

#[test]
fn adr0065_i1_pfb_reassembles_ascii_and_binary_sections() -> Result<(), sofel_type1::pfb::PfbError>
{
    let font = b"\x80\x01\x05\0\0\0hello\x80\x02\x03\0\0\0\x01\x02\x03\x80\x01\x01\0\0\0!\x80\x03";
    assert_eq!(decode_pfb(font, 9)?, b"hello\x01\x02\x03!");
    let sections = Pfb::parse(font, 9)?;
    assert_eq!(sections.sections().len(), 3);
    assert_eq!(
        sections.sections().first().map(|section| section.kind),
        Some(SectionKind::Ascii)
    );
    assert_eq!(
        sections.sections().get(1).map(|section| section.kind),
        Some(SectionKind::Binary)
    );
    Ok(())
}

#[test]
fn adr0065_i2_pfb_rejects_truncation_bad_sections_and_output_limit() {
    let font = b"\x80\x01\x05\0\0\0hello\x80\x03";
    assert!(decode_pfb(font, 4).is_err());
    assert!(decode_pfb(b"\x80\x01\x05\0\0\0hell", 100).is_err());
    assert!(decode_pfb(b"\x80\x02\x01\0\0\0x\x80\x03", 100).is_err());
    assert!(decode_pfb(b"\x80\x01\xff\xff\xff\xff\x80\x03", 100).is_err());
    assert!(decode_pfb(b"\x80\x01\x01\0\0\0x\x80\x03junk", 100).is_err());
}
