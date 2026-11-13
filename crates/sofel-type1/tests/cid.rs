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

fn sample_with_subr() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let source = sample();
    let marker = b"(Binary) 11 StartData\n";
    let at = source
        .windows(marker.len())
        .position(|w| w == marker)
        .ok_or("data marker")?;
    let mut header = String::from_utf8(source.get(..at).ok_or("header")?.to_vec())?;
    header = header.replace(
        "/Private 1 dict",
        "/SubrCount 1 def\n/SDBytes 1 def\n/SubrMapOffset 13 def\n/Private 1 dict",
    );
    let mut data = header.into_bytes();
    data.extend_from_slice(b"(Binary) 29 StartData\n");
    data.extend_from_slice(&[
        0, 0, 6, 0, 0, 13, 139, 248, 136, 13, 139, 10, 14, 15, 29, 239, 239, 21, 239, 139, 5, 139,
        239, 5, 39, 39, 5, 9, 11,
    ]);
    Ok(data)
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
    assert_eq!(
        CidFace::parse(&data, 1024)
            .err()
            .map(|error| error.offset()),
        Some(data.len())
    );
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
fn adr0065_i2_reports_invalid_hex_byte_position() -> Result<(), Box<dyn std::error::Error>> {
    let original = sample();
    let at = original
        .windows(b"StartData\n".len())
        .position(|w| w == b"StartData\n")
        .ok_or("marker")?;
    let mut source = original
        .get(..at + b"StartData\n".len())
        .ok_or("header")?
        .to_vec();
    let mode = source
        .windows(b"(Binary)".len())
        .position(|w| w == b"(Binary)")
        .ok_or("mode")?;
    source.splice(mode..mode + b"(Binary)".len(), b"(Hex)".iter().copied());
    source.push(b'x');
    assert_eq!(
        CidFace::parse(&source, 1024)
            .err()
            .map(|error| error.offset()),
        Some(source.len() - 1)
    );
    Ok(())
}

#[test]
fn adr0065_i2_reports_invalid_header_value_position() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = sample();
    let old = b"/GDBytes 2";
    let at = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("field")?;
    *source.get_mut(at + old.len() - 1).ok_or("value")? = b'x';
    let bad_value = at + old.len() - 1;
    assert_eq!(
        CidFace::parse(&source, 1024)
            .err()
            .map(|error| error.offset()),
        Some(bad_value)
    );
    Ok(())
}

#[test]
fn adr0065_i2_reports_invalid_font_dict_field_position() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = sample_with_subr()?;
    let old = b"/SubrCount 1";
    let at = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("field")?;
    let bad_value = at + old.len() - 1;
    *source.get_mut(bad_value).ok_or("value")? = b'x';
    assert_eq!(
        CidFace::parse(&source, 1024)
            .err()
            .map(|error| error.offset()),
        Some(bad_value)
    );
    Ok(())
}

#[test]
fn adr0065_i2_reports_invalid_matrix_field_position() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = sample();
    let old = b"/FontMatrix [0.001 0 0 0.001 0 0]";
    let at = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("matrix")?;
    let value = at + b"/FontMatrix [".len();
    *source.get_mut(value).ok_or("value")? = b'x';
    assert_eq!(
        CidFace::parse(&source, 1024)
            .err()
            .map(|error| error.offset()),
        Some(at)
    );
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

#[test]
fn adr0065_i1_skips_comment_startdata_marker() -> Result<(), Box<dyn std::error::Error>> {
    let source = sample();
    let marker = b"/CIDFontName";
    let at = source
        .windows(marker.len())
        .position(|w| w == marker)
        .ok_or("name")?;
    let mut decorated = source.get(..at).ok_or("prefix")?.to_vec();
    decorated.extend_from_slice(b"% fake StartData marker\n");
    decorated.extend_from_slice(source.get(at..).ok_or("suffix")?);
    let face = CidFace::parse(&decorated, 1024)?;
    assert_eq!(face.glyph_metrics(0).ok_or("metrics")?.advance_x, 500);
    Ok(())
}

#[test]
fn adr0065_i1_reads_cid_font_dict_subroutine() -> Result<(), Box<dyn std::error::Error>> {
    let face = CidFace::parse(&sample_with_subr()?, 1024)?;
    assert_eq!(
        face.glyph_program(0).ok_or("glyph")?,
        [139, 248, 136, 13, 139, 10, 14]
    );
    assert_eq!(
        face.subroutine_program(0, 0).ok_or("subroutine")?,
        [239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 11]
    );
    assert!(face.subroutine_program(0, 1).is_none());
    Ok(())
}

#[test]
fn adr0065_i2_rejects_truncated_subroutine_map() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = sample_with_subr()?;
    let old = b"/SubrMapOffset 13";
    let at = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("offset")?;
    source
        .get_mut(at..at + old.len())
        .ok_or("bounds")?
        .copy_from_slice(b"/SubrMapOffset 99");
    assert_eq!(
        CidFace::parse(&source, 1024)
            .err()
            .map(|error| error.offset()),
        Some(source.len())
    );
    Ok(())
}

#[test]
fn adr0065_i1_applies_general_font_matrix_to_advance() -> Result<(), Box<dyn std::error::Error>> {
    let original = sample();
    let mut source = original.clone();
    let old = b"[0.001 0 0 0.001 0 0]";
    let at = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("matrix")?;
    source.splice(at..at + old.len(), b"[0.002 0 0 0.001 0 0]".iter().copied());
    let face = CidFace::parse(&source, 1024)?;
    assert_eq!(face.units_per_em(), 1000);
    assert_eq!(face.glyph_metrics(0).ok_or("metrics")?.advance_x, 1000);

    let mut source = original;
    let at = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("matrix")?;
    source.splice(
        at..at + old.len(),
        b"[0.001 0 0 0.001 0.01 0.02]".iter().copied(),
    );
    let face = CidFace::parse(&source, 1024)?;
    assert_eq!(face.glyph_metrics(0).ok_or("metrics")?.advance_x, 510);
    Ok(())
}

#[test]
fn adr0065_i1_zero_yy_matrix_matches_freetype() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = sample();
    let old = b"[0.001 0 0 0.001 0 0]";
    let at = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("matrix")?;
    source.splice(
        at..at + old.len(),
        b"[0 0.001 -0.001 0 0 0]".iter().copied(),
    );
    let face = CidFace::parse(&source, 1024)?;
    assert_eq!(face.units_per_em(), 1000);
    assert_eq!(face.glyph_metrics(0).ok_or("metrics")?.advance_x, 0);
    Ok(())
}

#[test]
fn adr0065_i1_last_font_dict_sets_face_units_per_em() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = two_dicts()?;
    let old = b"[0.001 0 0 0.001 0 0]";
    let first = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("first")?;
    let second = source
        .get(first + old.len()..)
        .ok_or("rest")?
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("second")?
        + first
        + old.len();
    source
        .get_mut(second..second + old.len())
        .ok_or("matrix")?
        .copy_from_slice(b"[0.002 0 0 0.002 0 0]");
    let face = CidFace::parse(&source, 1024)?;
    assert_eq!(face.units_per_em(), 500);
    assert_eq!(face.glyph_metrics(0).ok_or("first metrics")?.advance_x, 500);
    assert_eq!(
        face.glyph_metrics(1).ok_or("second metrics")?.advance_x,
        600
    );
    Ok(())
}

#[test]
fn adr0065_i1_default_matrix_keeps_previous_face_units() -> Result<(), Box<dyn std::error::Error>> {
    let mut source = two_dicts()?;
    let old = b"[0.001 0 0 0.001 0 0]";
    let first = source
        .windows(old.len())
        .position(|w| w == old)
        .ok_or("first")?;
    source
        .get_mut(first..first + old.len())
        .ok_or("matrix")?
        .copy_from_slice(b"[0.002 0 0 0.002 0 0]");
    let face = CidFace::parse(&source, 1024)?;
    assert_eq!(face.units_per_em(), 500);
    Ok(())
}
