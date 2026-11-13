use sofel_type1::source::Type1Source;

#[test]
fn adr0065_i1_pfb_source_decrypts_private_dictionary()
-> Result<(), sofel_type1::source::SourceError> {
    let mut font = Vec::new();
    let clear = b"%!PS-AdobeFont-1.0\ncurrentfile eexec\n";
    font.extend_from_slice(&[0x80, 1]);
    font.extend_from_slice(
        &u32::try_from(clear.len())
            .map_err(|_| sofel_type1::source::SourceError)?
            .to_le_bytes(),
    );
    font.extend_from_slice(clear);
    font.extend_from_slice(&[0x80, 2, 13, 0, 0, 0]);
    font.extend_from_slice(&[
        0xb8, 0x58, 0x8d, 0xa5, 0x42, 0x6e, 0x54, 0x79, 0x75, 0xa1, 0x11, 0x80, 0xdc,
    ]);
    font.extend_from_slice(&[0x80, 1, 0, 0, 0, 0, 0x80, 3]);
    let source = Type1Source::parse_pfb(&font, 4096)?;
    assert_eq!(source.cleartext(), clear);
    assert_eq!(source.private(), b"/FontName");
    Ok(())
}

#[test]
fn adr0065_i2_pfb_source_rejects_missing_eexec() {
    let font = b"\x80\x01\x05\0\0\0hello\x80\x02\x04\0\0\0abcd\x80\x03";
    assert!(Type1Source::parse_pfb(font, 100).is_err());
}

#[test]
fn adr0065_i1_pfa_hex_eexec_matches_pfb() -> Result<(), sofel_type1::source::SourceError> {
    let data = b"%!PS-AdobeFont-1.0\ncurrentfile eexec\nb8588da5 426e547975a11180dc\ncleartomark\n";
    let source = Type1Source::parse_pfa(data, 4096)?;
    assert_eq!(source.private(), b"/FontName");
    assert!(source.cleartext().ends_with(b"eexec\n"));
    Ok(())
}

#[test]
fn adr0065_i2_pfa_rejects_short_or_invalid_hex() {
    assert!(Type1Source::parse_pfa(b"%!PS-AdobeFont-1.0\ncurrentfile eexec\nb85\n", 100).is_err());
    assert!(
        Type1Source::parse_pfa(
            b"%!PS-AdobeFont-1.0\ncurrentfile eexec\nb8588da5426e5z\n",
            100
        )
        .is_err()
    );
}
