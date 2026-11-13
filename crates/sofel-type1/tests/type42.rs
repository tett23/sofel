#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_type1::type42::Type42Source;

fn wrapper(chunks: &[&[u8]]) -> Vec<u8> {
    let mut out = b"%!PS-TrueTypeFont-1.0\n/FontType 42 def\n/sfnts [\n".to_vec();
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
    let mut data = b"%!PS-TrueTypeFont\n/FontType 42 def\n/sfnts [ 12 RD ".to_vec();
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
    let data = b"%!PS-TrueTypeFont\n% /sfnts [<bad>]\n/FontType 42 def\n/sfnts [ % a comment\n<000100000000000000000000> ] def";
    assert_eq!(Type42Source::parse(data, 12).unwrap().sfnt().len(), 12);
}
