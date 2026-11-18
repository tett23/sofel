//! sofel ADR 0096 and ADR 0098: the allocation-free validators of the
//! Type 1 family, called through `src/noalloc/`.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

#[path = "support/type1_triangle.rs"]
mod type1_fixture;

use sofel_type1::afm::Afm;
use sofel_type1::cid::CidFace;
use sofel_type1::face::Type1Face;
use sofel_type1::noalloc::{self, Problem};
use sofel_type1::pfm::Pfm;
use sofel_type1::source::Type1Source;
use sofel_type1::type42::Type42Source;

const LIMIT: usize = 1 << 20;

fn problems(validate: impl Fn(&[u8], &mut dyn FnMut(Problem)), data: &[u8]) -> Vec<Problem> {
    let mut found = Vec::new();
    validate(data, &mut |problem| found.push(problem));
    found
}

fn pfa(data: &[u8], report: &mut dyn FnMut(Problem)) {
    noalloc::type1::validate_pfa(data, LIMIT, report);
}
fn pfb(data: &[u8], report: &mut dyn FnMut(Problem)) {
    noalloc::type1::validate_pfb(data, LIMIT, report);
}
fn cid(data: &[u8], report: &mut dyn FnMut(Problem)) {
    noalloc::cid::validate(data, LIMIT, report);
}
fn t42(data: &[u8], report: &mut dyn FnMut(Problem)) {
    noalloc::type42::validate(data, LIMIT, report);
}
fn afm(data: &[u8], report: &mut dyn FnMut(Problem)) {
    noalloc::afm::validate(data, report);
}
fn pfm(data: &[u8], report: &mut dyn FnMut(Problem)) {
    noalloc::pfm::validate(data, report);
}

fn parse_pfa(data: &[u8]) -> bool {
    Type1Source::parse_pfa(data, LIMIT).is_ok_and(|s| Type1Face::new(&s).is_ok())
}
fn parse_pfb(data: &[u8]) -> bool {
    Type1Source::parse_pfb(data, LIMIT).is_ok_and(|s| Type1Face::new(&s).is_ok())
}
fn parse_cid(data: &[u8]) -> bool {
    CidFace::parse(data, LIMIT).is_ok()
}
fn parse_t42(data: &[u8]) -> bool {
    Type42Source::parse(data, LIMIT).is_ok()
}
fn parse_afm(data: &[u8]) -> bool {
    Afm::parse(data).is_ok()
}
fn parse_pfm(data: &[u8]) -> bool {
    Pfm::parse(data).is_ok()
}

fn cid_sample() -> Vec<u8> {
    let mut data = b"%!PS-Adobe-3.0 Resource-CIDFont\n\
/CIDFontName /TestCID def\n/CIDFontType 0 def\n\
/Registry (Adobe) def\n/Ordering (Identity) def\n/Supplement 0 def\n\
/CIDCount 1 def\n/FDBytes 1 def\n/GDBytes 2 def\n/CIDMapOffset 0 def\n\
/FDArray 1 array\n%ADOBeginFontDict\n\
dup 0 10 dict begin\n/FontName /TestCID def\n/FontType 1 def\n\
/FontMatrix [0.001 0 0 0.001 0 0] def\n\
/SubrCount 1 def\n/SDBytes 1 def\n/SubrMapOffset 6 def\n\
/Private 1 dict begin /lenIV -1 def end\n\
end put\n%ADOEndFontDict\ndef\n(Hex) 14 StartData\n"
        .to_vec();
    data.extend_from_slice(b"000008 00000d 0d0e 8bf8880d0e 0b>");
    data
}

fn type42_sample() -> Vec<u8> {
    let mut sfnt = b"\x00\x01\x00\x00\x00\x01\x00\x10\x00\x00\x00\x00".to_vec();
    sfnt.extend_from_slice(b"head\x00\x00\x00\x00\x00\x00\x00\x1c\x00\x00\x00\x02ab");
    let mut out = b"%!PS-TrueTypeFont-1.0\n/FontType 42 def\n/Encoding StandardEncoding def\n\
/CharStrings 2 dict dup begin\n/.notdef 0 def\n/A 1 def\nend readonly def\n/sfnts [\n"
        .to_vec();
    for chunk in [&sfnt[..20], &sfnt[20..]] {
        out.push(b'<');
        for byte in chunk {
            out.extend_from_slice(format!("{byte:02X}").as_bytes());
        }
        out.extend_from_slice(b">\n");
    }
    out.extend_from_slice(b"] def\n");
    out
}

const AFM: &[u8] = b"StartFontMetrics 4.1\nFontName Example\nStartKernData\nStartKernPairs 3\nKPX A V -80\nKPY V A 12\nKP T o -30 4\nEndKernPairs\nEndKernData\nEndFontMetrics\n";

fn pfm_sample() -> Vec<u8> {
    let mut data = vec![0u8; 143];
    data[1] = 1;
    data[2..6].copy_from_slice(&143u32.to_le_bytes());
    data[117..119].copy_from_slice(&18u16.to_le_bytes());
    data[131..135].copy_from_slice(&135u32.to_le_bytes());
    data[135..137].copy_from_slice(&1u16.to_le_bytes());
    data[137..141].copy_from_slice(&[b'A', b'V', 0xb0, 0xff]);
    data
}

type Case = (
    &'static str,
    Vec<u8>,
    fn(&[u8], &mut dyn FnMut(Problem)),
    fn(&[u8]) -> bool,
);

fn cases() -> Vec<Case> {
    let (pfa_data, pfb_data) = type1_fixture::sample();
    vec![
        ("PFA", pfa_data, pfa, parse_pfa),
        ("PFB", pfb_data, pfb, parse_pfb),
        ("CID", cid_sample(), cid, parse_cid),
        ("Type 42", type42_sample(), t42, parse_t42),
        ("AFM", AFM.to_vec(), afm, parse_afm),
        ("PFM", pfm_sample(), pfm, parse_pfm),
    ]
}

/// The copies of I3: every prefix, and every byte replaced by a few values.
fn damaged(data: &[u8]) -> impl Iterator<Item = Vec<u8>> + '_ {
    let prefixes = (0..data.len()).map(|end| data[..end].to_vec());
    let replaced = (0..data.len()).flat_map(move |at| {
        [0u8, b' ', b'0', b'/', 0xff].into_iter().map(move |value| {
            let mut copy = data.to_vec();
            copy[at] = value;
            copy
        })
    });
    prefixes.chain(replaced)
}

#[test]
fn adr0096_i1_valid_fonts_have_no_problems() {
    for (name, data, validate, parse) in cases() {
        assert!(parse(&data), "{name}: the sample does not parse");
        let found = problems(validate, &data);
        assert!(found.is_empty(), "{name}: {found:?}");
    }
}

#[test]
fn adr0096_i3_no_problem_means_the_parser_accepts() {
    for (name, data, validate, parse) in cases() {
        for copy in damaged(&data) {
            if problems(validate, &copy).is_empty() {
                assert!(
                    parse(&copy),
                    "{name}: accepted {:?}",
                    String::from_utf8_lossy(&copy)
                );
            } else {
                assert!(!parse(&copy), "{name}: rejected a parsable copy");
            }
        }
    }
}

#[test]
fn adr0096_i2_boundary_inputs_are_reported() {
    let (pfa_data, pfb_data) = type1_fixture::sample();
    for (name, _, validate, _) in cases() {
        for input in [&b""[..], b"%"] {
            assert!(!problems(validate, input).is_empty(), "{name}: {input:?}");
        }
    }
    // A PFB cut inside its second (binary) section.
    let header = u32::from_le_bytes(pfb_data[2..6].try_into().unwrap()) as usize;
    let inside = 6 + header + 6 + 3;
    let found = problems(pfb, &pfb_data[..inside]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].offset, inside);
    // A PFA cut inside its eexec part.
    let eexec = pfa_data.windows(5).position(|w| w == b"eexec").unwrap();
    assert!(!problems(pfa, &pfa_data[..eexec + 40]).is_empty());
}

#[test]
fn adr0096_i3_pfa_hex_and_pfb_section_offsets_match_the_parser() {
    let bad_hex = b"%!PS-AdobeFont-1.0\ncurrentfile eexec\n 000Z\ncleartomark\n";
    let found = problems(pfa, bad_hex);
    assert_eq!(
        found[0].offset,
        bad_hex.iter().position(|b| *b == b'Z').unwrap()
    );
    let short = b"\x80\x01\x08\x00\x00\x00ab";
    assert_eq!(problems(pfb, short)[0].offset, short.len());
}

#[test]
fn adr0096_i3_private_dictionary_error_has_file_offset() {
    let (_, mut data) = type1_fixture::sample();
    let header_len = u32::from_le_bytes(data[2..6].try_into().unwrap()) as usize;
    let binary = 6 + header_len + 6;
    let private = Type1Source::parse_pfb(&data, data.len()).unwrap();
    let len_iv = private
        .private()
        .windows(6)
        .position(|w| w == b"/lenIV")
        .unwrap();
    let digit = binary + 4 + len_iv + b"/lenIV ".len();
    let mut plain = private.private().to_vec();
    plain[digit - binary - 4] = b'!';
    let mut state = 55665u16;
    for (slot, byte) in data[binary..].iter_mut().zip(b"ABCD".iter().chain(&plain)) {
        *slot = *byte ^ (state >> 8) as u8;
        state = (u16::from(*slot).wrapping_add(state))
            .wrapping_mul(52845)
            .wrapping_add(22719);
    }
    let found = problems(pfb, &data);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].offset, digit);
}

/// A charstring with `count` operands before `hsbw`.
fn operands(count: usize) -> Vec<u8> {
    let mut program = vec![139; count];
    program.push(13);
    program
}

#[test]
fn adr0098_i3_metrics_operand_stack_limit() {
    let mut two = operands(0);
    two.splice(0..0, [139, 239]);
    let metrics = noalloc::metrics::parse_metrics(&two).unwrap();
    assert_eq!((metrics.left, metrics.advance_x), (0, 100));
    // 24 operands fit the stack; `hsbw` then sees the wrong count.
    assert!(noalloc::metrics::parse_metrics(&operands(24)).is_err());
    let mut fits = vec![139; 22];
    fits.extend_from_slice(&[12, 0, 139, 239, 13]);
    let mut full = vec![139; 24];
    full.extend_from_slice(&[12, 0, 139, 239, 13]);
    assert!(noalloc::metrics::parse_metrics(&fits).is_ok());
    assert!(noalloc::metrics::parse_metrics(&full).is_ok());
    let mut over = vec![139; 25];
    over.extend_from_slice(&[12, 0, 139, 239, 13]);
    assert!(noalloc::metrics::parse_metrics(&over).is_err());
    assert_eq!(
        sofel_type1::metrics::parse_metrics(&two),
        noalloc::metrics::parse_metrics(&two)
    );
}

#[test]
fn adr0098_i3_validators_are_reexported() {
    let (pfa_data, _) = type1_fixture::sample();
    let mut count = 0;
    sofel_type1::source::validate_pfa(&pfa_data, LIMIT, &mut |_| count += 1);
    sofel_type1::cid::validate(&cid_sample(), LIMIT, &mut |_| count += 1);
    sofel_type1::type42::validate(&type42_sample(), LIMIT, &mut |_| count += 1);
    sofel_type1::afm::validate(AFM, &mut |_| count += 1);
    sofel_type1::pfm::validate(&pfm_sample(), &mut |_| count += 1);
    assert_eq!(count, 0);
}
