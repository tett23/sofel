#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_type1::type42::Type42Source;

fn wrapper(chunks: &[&[u8]]) -> Vec<u8> {
    let mut out = b"%!PS-TrueTypeFont-1.0\n/FontType 42 def\n/Encoding StandardEncoding def\n/CharStrings 1 dict dup begin\n/.notdef 0 def\nend readonly def\n/sfnts [\n".to_vec();
    for chunk in chunks {
        out.extend_from_slice(b"<");
        for byte in *chunk {
            out.extend_from_slice(format!("{byte:02X}").as_bytes());
        }
        out.extend_from_slice(b">\n");
    }
    out.extend_from_slice(b"] def\n");
    out
}

#[test]
fn adr0065_i1_type42_joins_split_sfnts() {
    let mut sfnt = b"\x00\x01\x00\x00\x00\x01\x00\x10\x00\x00\x00\x00".to_vec();
    sfnt.extend_from_slice(b"head\x00\x00\x00\x00\x00\x00\x00\x1c\x00\x00\x00\x02");
    sfnt.extend_from_slice(b"ab");
    let data = wrapper(&[&sfnt[..28], &sfnt[28..]]);
    let source = Type42Source::parse(&data, 64).unwrap();
    assert_eq!(source.sfnt(), sfnt);
}

#[test]
fn adr0065_i2_type42_rejects_bounds_and_missing_data() {
    let good = wrapper(&[b"\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"]);
    assert!(Type42Source::parse(&good, 11).is_err());
    assert!(Type42Source::parse(&good[..good.len() - 6], 64).is_err());
    let bad_hex = b"%!PS-TrueTypeFont\n/FontType 42 def\n/sfnts [<GG>] def";
    assert!(Type42Source::parse(bad_hex, 64).is_err());
    let wrong_type = b"%!PS-TrueTypeFont\n/FontType 1 def\n/sfnts [<000100000000000000000000>] def";
    assert!(Type42Source::parse(wrong_type, 64).is_err());
}

#[test]
fn adr0065_i1_type42_reads_binary_sfnts() {
    let mut data = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding StandardEncoding def\n/CharStrings 1 dict dup begin /.notdef 0 def end readonly def\n/sfnts [ 12 RD ".to_vec();
    data.extend_from_slice(b"\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00");
    data.extend_from_slice(b" ] def");
    let source = Type42Source::parse(&data, 12).unwrap();
    assert_eq!(
        source.sfnt(),
        b"\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
    );
    let mut short = data.clone();
    let length = short.windows(2).position(|part| part == b"12").unwrap();
    short[length..length + 2].copy_from_slice(b"20");
    assert!(Type42Source::parse(&short, 20).is_err());
}

#[test]
fn adr0065_i1_type42_skips_postscript_comments() {
    let data = b"%!PS-TrueTypeFont\n% /sfnts [<bad>]\n/FontType 42 def\n/Encoding StandardEncoding def\n/CharStrings 1 dict dup begin /.notdef 0 def end readonly def\n/sfnts [ % a comment\n<000100000000000000000000> ] def";
    for ending in ["\n", "\r", "\r\n"] {
        let bytes = std::str::from_utf8(data).unwrap().replace('\n', ending);
        assert_eq!(
            Type42Source::parse(bytes.as_bytes(), 12)
                .unwrap()
                .sfnt()
                .len(),
            12
        );
    }
}

#[test]
fn adr0065_i1_type42_uses_encoding_and_charstrings_order() {
    let data = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding 256 array\ndup 65 /B put\ndup 66 /A put\nreadonly def\n/CharStrings 3 dict dup begin\n/A 2 def\n/.notdef 0 def\n/B 1 def\nend readonly def\n/sfnts [<000100000000000000000000>] def";
    let source = Type42Source::parse(data, 12).unwrap();
    assert_eq!(source.num_glyphs(), 3);
    assert_eq!(source.glyph_name(0), Some(&b".notdef"[..]));
    assert_eq!(source.glyph_name(1), Some(&b"A"[..]));
    assert_eq!(source.embedded_glyph(1), Some(2));
    assert_eq!(source.embedded_glyph(2), Some(1));
    assert_eq!(source.unicode_glyph('A' as u32), Some(1));
    assert_eq!(source.unicode_glyph('B' as u32), Some(2));
    assert_eq!(source.encoded_glyph(b'A'), Some(2));
    assert_eq!(source.encoded_glyph(b'B'), Some(1));
}

#[test]
fn adr0065_i1_type42_unicode_uses_adobe_glyph_names() {
    let data = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding StandardEncoding def\n/CharStrings 3 dict dup begin\n/.notdef 0 def\n/Aacute 2 def\n/A.swash 1 def\nend readonly def\n/sfnts [<000100000000000000000000>] def";
    let source = Type42Source::parse(data, 12).unwrap();
    assert_eq!(source.unicode_glyph(0x00c1), Some(1));
    assert_eq!(source.unicode_glyph('A' as u32), Some(2));
}

#[test]
fn adr0065_i1_type42_accepts_dictionary_and_direct_encoding() {
    let data = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding [/.notdef /A /B] def\n/CharStrings << /B 1 /.notdef 0 /A 2 >> def\n/sfnts [<000100000000000000000000>] def";
    let source = Type42Source::parse(data, 12).unwrap();
    assert_eq!(source.num_glyphs(), 3);
    assert_eq!(source.glyph_name(0), Some(&b".notdef"[..]));
    assert_eq!(source.glyph_name(1), Some(&b"B"[..]));
    assert_eq!(source.glyph_name(2), Some(&b"A"[..]));
    assert_eq!(source.encoded_glyph(1), Some(2));
    assert_eq!(source.encoded_glyph(2), Some(1));
}

#[test]
fn adr0065_i1_type42_named_encodings_match_glyph_names() {
    let expert = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding ExpertEncoding def\n/CharStrings 2 dict dup begin /.notdef 0 def /exclamsmall 1 def end readonly def\n/sfnts [<000100000000000000000000>] def";
    let source = Type42Source::parse(expert, 12).unwrap();
    assert_eq!(source.encoded_glyph(33), Some(1));
    let latin1 = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding ISOLatin1Encoding def\n/CharStrings 2 dict dup begin /.notdef 0 def /Aacute 1 def end readonly def\n/sfnts [<000100000000000000000000>] def";
    let source = Type42Source::parse(latin1, 12).unwrap();
    assert_eq!(source.encoded_glyph(0xc1), Some(1));
}

#[test]
fn adr0065_i1_type42_accepts_literal_charstring_names() {
    let data = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding StandardEncoding def\n/CharStrings 2 dict dup begin\n(.notdef) cvn 0 def\n(A) cvn 1 def\nend readonly def\n/sfnts [<000100000000000000000000>] def";
    let source = Type42Source::parse(data, 12).unwrap();
    assert_eq!(source.glyph_name(1), Some(&b"A"[..]));
    assert_eq!(source.unicode_glyph('A' as u32), Some(1));
}

#[test]
fn adr0065_i1_type42_accepts_short_explicit_encoding() {
    let data = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding 3 array\ndup 1 /A put\ndup 2 /B put\nreadonly def\n/CharStrings 3 dict dup begin /.notdef 0 def /A 1 def /B 2 def end readonly def\n/sfnts [<000100000000000000000000>] def";
    let source = Type42Source::parse(data, 12).unwrap();
    assert_eq!(source.encoded_glyph(1), Some(1));
    assert_eq!(source.encoded_glyph(2), Some(2));
    assert_eq!(source.encoded_glyph(3), None);
}

#[test]
fn adr0065_i1_type42_reads_multiple_encoding_assignments_on_one_line() {
    let data = b"%!PS-TrueTypeFont\n/FontType 42 def\n/Encoding 256 array dup 65 /B put dup 66 /A put readonly def\n/CharStrings 3 dict dup begin /.notdef 0 def /A 1 def /B 2 def end readonly def\n/sfnts [<000100000000000000000000>] def";
    let source = Type42Source::parse(data, 12).unwrap();
    assert_eq!(source.encoded_glyph(65), Some(2));
    assert_eq!(source.encoded_glyph(66), Some(1));
}
