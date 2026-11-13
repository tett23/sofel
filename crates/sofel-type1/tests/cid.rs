//! CID-keyed Type 1 samples compared with FreeType 2.15.0 (37515cf56ba3).

use sofel_type1::cid::CidFace;

fn sample() -> Vec<u8> {
    let mut data = b"%!PS-Adobe-3.0 Resource-CIDFont\n\
/CIDFontName /TestCID def\n/CIDFontType 0 def\n\
/Registry (Adobe) def\n/Ordering (Identity) def\n/Supplement 0 def\n\
/CIDCount 1 def\n/FDBytes 1 def\n/GDBytes 2 def\n/CIDMapOffset 0 def\n\
/FDArray 1 array\n%ADOBeginFontDict\n\
dup 0 10 dict begin\n/FontName /TestCID def\n/FontType 1 def\n\
/FontMatrix [0.001 0 0 0.001 0 0] def\n\
/Private 1 dict begin /lenIV -1 def end\n\
end put\n%ADOEndFontDict\ndef\n(Binary) 11 StartData\n"
        .to_vec();
    data.extend_from_slice(&[0, 0, 6, 0, 0, 11, 139, 248, 136, 13, 14]);
    data
}

fn two_dicts() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let one = sample();
    let marker = b"%ADOBeginFontDict\n";
    let begin = one
        .windows(marker.len())
        .position(|w| w == marker)
        .ok_or("begin")?;
    let end_marker = b"%ADOEndFontDict\n";
    let end = one
        .windows(end_marker.len())
        .position(|w| w == end_marker)
        .ok_or("end")?
        .checked_add(end_marker.len())
        .ok_or("overflow")?;
    let dict = one.get(begin..end).ok_or("dict bounds")?;
    let header_end = one
        .windows(b"(Binary)".len())
        .position(|w| w == b"(Binary)")
        .ok_or("header end")?;
    let header = String::from_utf8(one.get(..header_end).ok_or("header bounds")?.to_vec())?
        .replace("/CIDCount 1", "/CIDCount 2")
        .replace("/FDArray 1 array", "/FDArray 2 array");
    let dict = String::from_utf8(dict.to_vec())?;
    let header = header.replace(&dict, &(dict.clone() + &dict.replace("dup 0", "dup 1")));
    let mut out = header.into_bytes();
    out.extend_from_slice(b"(Binary) 19 StartData\n");
    out.extend_from_slice(&[
        0, 0, 9, 1, 0, 14, 0, 0, 19, 139, 248, 136, 13, 14, 139, 248, 236, 13, 14,
    ]);
    Ok(out)
}

#[test]
fn adr0065_i1_opens_cid_type1_and_reads_advance() -> Result<(), Box<dyn std::error::Error>> {
    let face = CidFace::parse(&sample(), 1024)?;
    assert_eq!(face.num_glyphs(), 1);
    assert_eq!(
        face.glyph_program(0).ok_or("glyph")?,
        [139, 248, 136, 13, 14]
    );
    assert_eq!(face.glyph_metrics(0).ok_or("metrics")?.advance_x, 500);
    Ok(())
}

#[test]
fn adr0065_i2_rejects_truncated_cid_map() {
    let mut data = sample();
    data.truncate(data.len() - 9);
    assert!(CidFace::parse(&data, 1024).is_err());
    let mut data = sample();
    data.truncate(data.len() - 8);
    assert!(CidFace::parse(&data, 1024).is_err());
}

#[test]
fn adr0065_i2_rejects_invalid_fd_or_offset() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = sample();
    let map = data
        .windows(11)
        .position(|w| w == [0, 0, 6, 0, 0, 11, 139, 248, 136, 13, 14])
        .ok_or("map")?;
    *data.get_mut(map).ok_or("map bounds")? = 1;
    let face = CidFace::parse(&data, 1024)?;
    assert!(face.glyph_program(0).is_none());
    *data.get_mut(map).ok_or("map bounds")? = 0;
    *data
        .get_mut(map.checked_add(4).ok_or("overflow")?)
        .ok_or("map bounds")? = 5;
    let face = CidFace::parse(&data, 1024)?;
    assert!(face.glyph_program(0).is_none());
    Ok(())
}

#[test]
fn adr0065_i1_selects_font_dict_per_cid() -> Result<(), Box<dyn std::error::Error>> {
    let face = CidFace::parse(&two_dicts()?, 1024)?;
    assert_eq!(face.num_glyphs(), 2);
    assert_eq!(face.glyph_metrics(0).ok_or("metrics")?.advance_x, 500);
    assert_eq!(face.glyph_metrics(1).ok_or("metrics")?.advance_x, 600);
    Ok(())
}

#[test]
fn adr0065_i1_opens_hex_cid_data() -> Result<(), Box<dyn std::error::Error>> {
    let source = sample();
    let marker = b"StartData\n";
    let binary = source
        .windows(marker.len())
        .position(|w| w == marker)
        .ok_or("data marker")?
        .checked_add(marker.len())
        .ok_or("offset overflow")?;
    let mut data = String::from_utf8(source.get(..binary).ok_or("header")?.to_vec())?
        .replace("(Binary) 11", "(Hex) 22")
        .into_bytes();
    for byte in source.get(binary..).ok_or("binary")? {
        data.extend_from_slice(format!("{byte:02x}").as_bytes());
    }
    data.push(b'>');
    let face = CidFace::parse(&data, 1024)?;
    assert_eq!(face.glyph_metrics(0).ok_or("metrics")?.advance_x, 500);
    Ok(())
}

#[test]
fn adr0065_i1_uses_font_matrix_for_units_per_em() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = sample();
    let old = b"[0.001 0 0 0.001 0 0]";
    let at = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("matrix")?;
    source
        .get_mut(at..at.checked_add(old.len()).ok_or("overflow")?)
        .ok_or("bounds")?
        .copy_from_slice(b"[0.002 0 0 0.002 0 0]");
    let face = CidFace::parse(&source, 1024)?;
    assert_eq!(face.units_per_em(), 500);
    Ok(())
}
