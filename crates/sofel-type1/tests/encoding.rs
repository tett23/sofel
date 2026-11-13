use sofel_type1::encoding::Encoding;

#[test]
fn adr0065_i1_type1_explicit_and_standard_encoding()
-> Result<(), sofel_type1::encoding::EncodingError> {
    let explicit = b"%!PS-AdobeFont-1.0\n/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\ndup 65/A put\ndup 86 /V put\nreadonly def\ncurrentfile eexec\n";
    let encoding = Encoding::parse(explicit)?;
    assert_eq!(encoding.name(65), Some(&b"A"[..]));
    assert_eq!(encoding.name(86), Some(&b"V"[..]));
    assert_eq!(encoding.name(66), None);
    let standard = Encoding::parse(b"/Encoding StandardEncoding def\n")?;
    assert_eq!(standard.name(65), Some(&b"A"[..]));
    assert_eq!(standard.name(32), Some(&b"space"[..]));
    Ok(())
}

#[test]
fn adr0065_i2_type1_encoding_rejects_out_of_range_code() {
    assert!(Encoding::parse(b"/Encoding 256 array\ndup 256/A put\nreadonly def\n").is_err());
}

#[test]
fn adr0065_i1_type1_mm_postscript_helper_precedes_encoding()
-> Result<(), sofel_type1::encoding::EncodingError> {
    let data =
        b"/Encoding currentfont /Encoding get put definefont\n/Encoding StandardEncoding def\n";
    let encoding = Encoding::parse(data)?;
    assert_eq!(encoding.name(b'A'), Some(&b"A"[..]));
    Ok(())
}
