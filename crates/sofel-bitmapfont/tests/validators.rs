//! sofel ADR 0096 and ADR 0098: the allocation-free validators of BDF,
//! PCF, FNT and FON, called through `src/noalloc/`.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_bitmapfont::BdfFont;
use sofel_bitmapfont::noalloc::{self, Problem};
use sofel_bitmapfont::pcf::PcfFont;
use sofel_bitmapfont::winfnt::WinFont;

const BDF: &[u8] = include_bytes!("data/mono.bdf");
const BDF_GRAY: &[u8] = include_bytes!("data/gray4.bdf");
const PCF: &[u8] = include_bytes!("data/mono.pcf");
const PCF_SWAP: &[u8] = include_bytes!("data/mono_swap.pcf");
const FNT: &[u8] = include_bytes!("data/mono.fnt");
const FNT3: &[u8] = include_bytes!("data/mono3.fnt");
const FON: &[u8] = include_bytes!("data/two.fon");
const FON_PE: &[u8] = include_bytes!("data/mono-pe.fon");

fn problems(validate: fn(&[u8], &mut dyn FnMut(Problem)), data: &[u8]) -> Vec<Problem> {
    let mut found = Vec::new();
    validate(data, &mut |problem| found.push(problem));
    found
}

fn parse_bdf(data: &[u8]) -> bool {
    BdfFont::parse(data).is_ok()
}
fn parse_pcf(data: &[u8]) -> bool {
    PcfFont::parse(data).is_ok()
}
fn parse_win(data: &[u8]) -> bool {
    WinFont::face_count(data)
        .is_ok_and(|count| (0..count).all(|face| WinFont::parse(data, face).is_ok()))
}

type Case = (
    &'static [u8],
    fn(&[u8], &mut dyn FnMut(Problem)),
    fn(&[u8]) -> bool,
);

const CASES: [Case; 8] = [
    (BDF, noalloc::bdf::validate, parse_bdf),
    (BDF_GRAY, noalloc::bdf::validate, parse_bdf),
    (PCF, noalloc::pcf::validate, parse_pcf),
    (PCF_SWAP, noalloc::pcf::validate, parse_pcf),
    (FNT, noalloc::winfnt::validate, parse_win),
    (FNT3, noalloc::winfnt::validate, parse_win),
    (FON, noalloc::winfnt::validate, parse_win),
    (FON_PE, noalloc::winfnt::validate, parse_win),
];

fn damaged(data: &[u8]) -> impl Iterator<Item = Vec<u8>> + '_ {
    let prefixes = (0..data.len()).map(|end| data[..end].to_vec());
    let replaced = (0..data.len()).flat_map(move |at| {
        [0u8, b' ', b'0', 0x7f, 0xff].into_iter().map(move |value| {
            let mut copy = data.to_vec();
            copy[at] = value;
            copy
        })
    });
    prefixes.chain(replaced)
}

#[test]
fn adr0096_i1_valid_fonts_have_no_problems() {
    for (index, (data, validate, parse)) in CASES.into_iter().enumerate() {
        assert!(parse(data), "case {index} does not parse");
        let found = problems(validate, data);
        assert!(found.is_empty(), "case {index}: {found:?}");
    }
}

#[test]
fn adr0096_i3_no_problem_means_the_parser_accepts() {
    for (index, (data, validate, parse)) in CASES.into_iter().enumerate() {
        for copy in damaged(data) {
            let found = problems(validate, &copy);
            assert_eq!(found.is_empty(), parse(&copy), "case {index}: {found:?}");
        }
    }
}

#[test]
fn adr0096_i2_boundary_inputs_are_reported() {
    for (index, (_, validate, _)) in CASES.into_iter().enumerate() {
        for input in [&b""[..], b"S"] {
            assert!(!problems(validate, input).is_empty(), "case {index}");
        }
    }
}

#[test]
fn adr0096_i3_offsets_match_the_parsers() {
    let end = BDF.windows(7).position(|w| w == b"BITMAP\n").unwrap() + 7;
    let found = problems(noalloc::bdf::validate, &BDF[..end]);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].offset,
        BdfFont::parse_with_offset(&BDF[..end]).unwrap_err()
    );
    let mut fon = FON.to_vec();
    let second = usize::from(u16::from_le_bytes([fon[0x96], fon[0x97]]));
    fon[second + 88..second + 90].fill(0);
    let first = usize::from(u16::from_le_bytes([fon[0x8a], fon[0x8b]]));
    fon[first + 88..first + 90].fill(0);
    let found = problems(noalloc::winfnt::validate, &fon);
    let places: Vec<_> = found.iter().map(|p| (p.face, p.offset)).collect();
    assert_eq!(places, [(Some(0), first + 88), (Some(1), second + 88)]);
    let mut pcf = PCF.to_vec();
    pcf.truncate(20);
    let found = problems(noalloc::pcf::validate, &pcf);
    assert_eq!(
        found[0].offset,
        PcfFont::parse_positioned(&pcf).err().unwrap().1
    );
}

#[test]
fn adr0098_i3_validators_are_reexported() {
    let mut count = 0;
    BdfFont::validate(BDF, &mut |_| count += 1);
    PcfFont::validate(PCF, &mut |_| count += 1);
    WinFont::validate(FON, &mut |_| count += 1);
    assert_eq!(count, 0);
}
