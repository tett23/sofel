//! CFF subsetting on hand-made fonts (ADR 0022 stage 2): the number
//! encoding of desubroutinized charstrings, the subroutine renumbering and
//! bias, the `seac` closure, and the charset and FDSelect formats.

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::read::Tag;
use sofel_font::read::cff::hb::{DictKind, HbDict, interpret_dict};
use sofel_font::read::cff::{Cff1, STANDARD_ENCODING};
use sofel_font::read::sfnt::Sfnt;
use sofel_font::{Font, HbCff1};
use sofel_subset::{Flags, Input, Plan, SetKind, subset};

fn otf(num_glyphs: u16, cff: Vec<u8>) -> Vec<u8> {
    otto_font(num_glyphs, vec![(b"CFF ", cff)])
}

fn glyphs(ids: &[u32]) -> Input {
    Input::new().map_set(SetKind::Glyphs, |s| {
        for &g in ids {
            s.add(g);
        }
    })
}

fn run(font: &[u8], input: &Input) -> Vec<u8> {
    subset(&Font::new(font, 0).unwrap(), input).unwrap()
}

fn cff_of(out: &[u8]) -> Vec<u8> {
    Sfnt::new(out)
        .unwrap()
        .table(Tag::new(b"CFF "))
        .unwrap()
        .to_vec()
}

/// The output's CFF table must be valid on its own (ADR 0022).
fn assert_valid(cff: &[u8]) {
    let mut errors = Vec::new();
    sofel_font::read::cff::validate(cff, None, &mut |d| {
        if d.severity == sofel_font::read::Severity::Error {
            errors.push(format!("{:?}", d.kind));
        }
    });
    assert!(errors.is_empty(), "{errors:?}");
}

fn top(cff: &[u8]) -> HbDict {
    let c = Cff1::new(cff).unwrap();
    interpret_dict(c.top_dicts().get(0).unwrap(), DictKind::Cff1Top).unwrap()
}

fn endchar() -> Vec<u8> {
    charstring(&[Cs::Op(14)])
}

#[test]
fn desubroutinized_numbers_take_the_shortest_form() {
    let raw = |b: &[u8]| Cs::Raw(b.to_vec());
    let g1 = charstring(&[
        raw(&[28, 0, 5]),
        Cs::F(5 << 16),
        Cs::Op(21),
        Cs::F(0x0001_8000),
        raw(&[28, 0x01, 0x2C]),
        raw(&[28, 0xFE, 0xD4]),
        raw(&[28, 0x07, 0xD0]),
        raw(&[28, 0, 107]),
        raw(&[28, 0xFF, 0x95]),
        raw(&[28, 0, 108]),
        raw(&[28, 0xFF, 0x94]),
        raw(&[28, 0x04, 0x6B]),
        raw(&[28, 0xFB, 0x95]),
        Cs::I(1132),
        Cs::I(-1132),
        Cs::F(0x4E20_8000),
        Cs::I(1),
        Cs::Op(5),
        Cs::Op(14),
    ]);
    let font = otf(2, cff1(&Cff1Spec::new(vec![endchar(), g1.clone()])));
    let input = glyphs(&[1]);

    // Without desubroutinizing, the tokens are copied as they are.
    let out = cff_of(&run(&font, &input));
    assert_valid(&out);
    let bytes = run(&font, &input);
    let o = Font::new(&bytes, 0).unwrap();
    assert_eq!(HbCff1::new(&o).unwrap().char_string(1).unwrap(), &g1[..]);

    let input = input.with_flags(Flags::DESUBROUTINIZE);
    let bytes = run(&font, &input);
    assert_valid(&cff_of(&bytes));
    let o = Font::new(&bytes, 0).unwrap();
    let expected: Vec<u8> = vec![
        144, 144, 21, // 5 and 5.0 as one byte
        255, 0, 1, 0x80, 0, // 1.5 stays 16.16
        247, 192, 251, 192, // ±300 in two bytes
        28, 0x07, 0xD0, // 2000 in three
        246, 32, // ±107 in one byte
        247, 0, 251, 0, // ±108 in two
        250, 255, 254, 255, // ±1131 in two
        28, 0x04, 0x6C, 28, 0xFB, 0x94, // ±1132 in three
        255, 0x4E, 0x20, 0x80, 0, // 20000.5
        140, 5, 14,
    ];
    assert_eq!(
        HbCff1::new(&o).unwrap().char_string(1).unwrap(),
        &expected[..]
    );
}

/// A name-keyed font whose glyph 1 calls local subroutine `l` and global
/// subroutine `g`, and whose glyph 2 calls local subroutine 5.
fn subr_font(num_local: usize) -> Vec<u8> {
    let bias = |n: usize| -> i32 {
        if n < 1240 {
            107
        } else if n < 33900 {
            1131
        } else {
            32768
        }
    };
    let lb = bias(num_local);
    let g1 = charstring(&[
        Cs::I(num_local as i32 - 1 - lb),
        Cs::Op(10),
        Cs::I(2 - 107),
        Cs::Op(29),
        Cs::Op(14),
    ]);
    let g2 = charstring(&[Cs::I(5 - lb), Cs::Op(10), Cs::Op(14)]);
    let mut spec = Cff1Spec::new(vec![endchar(), g1, g2]);
    spec.lsubrs = vec![vec![11]; num_local];
    spec.gsubrs = vec![vec![11], vec![11], vec![11]];
    otf(3, cff1(&spec))
}

#[test]
fn kept_subroutines_are_renumbered_with_a_new_bias() {
    let font = subr_font(1240);
    let bytes = run(&font, &glyphs(&[1]));
    assert_valid(&cff_of(&bytes));
    let o = Font::new(&bytes, 0).unwrap();
    let acc = HbCff1::new(&o).unwrap();
    // One local and one global subroutine are left: number 0, bias 107.
    assert_eq!(acc.local_subrs(0).len(), 1);
    assert_eq!(acc.global_subrs().len(), 1);
    assert_eq!(acc.char_string(1).unwrap(), &[32, 10, 32, 29, 14][..]);

    // Both glyphs: subroutines 5 and 1239 become 0 and 1.
    let bytes = run(&font, &glyphs(&[1, 2]));
    let o = Font::new(&bytes, 0).unwrap();
    let acc = HbCff1::new(&o).unwrap();
    assert_eq!(acc.local_subrs(0).len(), 2);
    assert_eq!(acc.char_string(1).unwrap(), &[33, 10, 32, 29, 14][..]);
    assert_eq!(acc.char_string(2).unwrap(), &[32, 10, 14][..]);
}

#[test]
fn bias_follows_the_count_of_kept_subroutines() {
    // Glyph 1 calls all 1240 local subroutines: they stay, with bias 1131.
    let n = 1240;
    let mut items = Vec::new();
    for i in 0..n {
        items.push(Cs::I(i - 1131));
        items.push(Cs::Op(10));
    }
    items.push(Cs::Op(14));
    let mut spec = Cff1Spec::new(vec![endchar(), charstring(&items), endchar()]);
    spec.lsubrs = vec![vec![11]; n as usize];
    let font = otf(3, cff1(&spec));
    let bytes = run(&font, &glyphs(&[1]));
    assert_valid(&cff_of(&bytes));
    let o = Font::new(&bytes, 0).unwrap();
    let acc = HbCff1::new(&o).unwrap();
    assert_eq!(acc.local_subrs(0).len(), 1240);
    assert_eq!(acc.char_string(1).unwrap(), &charstring(&items)[..]);
    // Glyph 2 calls none: no Subrs INDEX at all.
    let bytes = run(&font, &glyphs(&[2]));
    let o = Font::new(&bytes, 0).unwrap();
    assert_eq!(HbCff1::new(&o).unwrap().local_subrs(0).len(), 0);
}

#[test]
fn hints_are_dropped_with_the_width_kept() {
    // width hstem hintmask rmoveto endchar
    let g1 = charstring(&[
        Cs::I(500),
        Cs::I(10),
        Cs::I(20),
        Cs::Op(1),
        Cs::Op(19),
        Cs::Raw(vec![0x80]),
        Cs::I(1),
        Cs::I(2),
        Cs::Op(21),
        Cs::Op(14),
    ]);
    let font = otf(2, cff1(&Cff1Spec::new(vec![endchar(), g1])));
    let bytes = run(&font, &glyphs(&[1]).with_flags(Flags::NO_HINTING));
    assert_valid(&cff_of(&bytes));
    let o = Font::new(&bytes, 0).unwrap();
    let mut expected = charstring(&[Cs::I(500), Cs::I(1), Cs::I(2), Cs::Op(21)]);
    expected.push(14);
    assert_eq!(
        HbCff1::new(&o).unwrap().char_string(1).unwrap(),
        &expected[..]
    );
}

#[test]
fn retained_gids_get_endchar_charstrings() {
    let g = charstring(&[Cs::I(1), Cs::I(2), Cs::Op(21), Cs::Op(14)]);
    let font = otf(
        4,
        cff1(&Cff1Spec::new(vec![endchar(), g.clone(), g.clone(), g])),
    );
    let bytes = run(&font, &glyphs(&[3]).with_flags(Flags::RETAIN_GIDS));
    assert_valid(&cff_of(&bytes));
    let o = Font::new(&bytes, 0).unwrap();
    let acc = HbCff1::new(&o).unwrap();
    assert_eq!(acc.num_glyphs(), 4);
    assert_eq!(acc.char_string(1).unwrap(), &[14][..]);
    assert_eq!(acc.char_string(2).unwrap(), &[14][..]);
    assert_eq!(acc.char_string(3).unwrap().len(), 4);
}

/// A name-keyed font: glyph 1 is "A", glyph 2 "acute", glyph 3 an
/// `seac` of the two.
fn seac_spec() -> Cff1Spec {
    let a = STANDARD_ENCODING[65];
    let acute = STANDARD_ENCODING[194];
    let seac = charstring(&[Cs::I(0), Cs::I(0), Cs::I(65), Cs::I(194), Cs::Op(14)]);
    let mut s = Cff1Spec::new(vec![endchar(), endchar(), endchar(), seac]);
    let mut charset = vec![0u8];
    for sid in [a, acute, 200] {
        charset.extend(sid.to_be_bytes());
    }
    s.charset = Some(charset);
    s
}

#[test]
fn seac_components_join_the_closure() {
    let font = otf(4, cff1(&seac_spec()));
    let f = Font::new(&font, 0).unwrap();
    let plan = Plan::new(&f, &glyphs(&[3])).unwrap();
    assert_eq!(
        plan.glyph_set().iter().collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_valid(&cff_of(&run(&font, &glyphs(&[3]))));

    // CID-keyed fonts have no seac.
    let mut spec = seac_spec();
    spec.private = None;
    spec.strings = vec![b"Adobe".to_vec(), b"Identity".to_vec()];
    spec.cid = Some(CidSpec {
        ros: (391, 392, 0),
        fds: vec![(Vec::new(), Vec::new(), Vec::new())],
        fd_select: vec![3, 0, 1, 0, 0, 0, 0, 4],
    });
    let font = otf(4, cff1(&spec));
    let f = Font::new(&font, 0).unwrap();
    let plan = Plan::new(&f, &glyphs(&[3])).unwrap();
    assert_eq!(plan.glyph_set().iter().collect::<Vec<_>>(), vec![0, 3]);
}

/// A CID-keyed font with 400 glyphs (CIDs equal to the glyph ids), two
/// Font DICTs: glyph 0 and glyphs 200 and up in the second, glyphs 1 to
/// 199 in the first.
fn cid_font() -> Vec<u8> {
    let mut s = Cff1Spec::new(vec![endchar(); 400]);
    s.private = None;
    s.strings = vec![b"Adobe".to_vec(), b"Identity".to_vec()];
    s.charset = Some(vec![2, 0, 1, 0x01, 0x8E]);
    s.cid = Some(CidSpec {
        ros: (391, 392, 0),
        // Non-empty Private DICTs (defaultWidthX): an empty one would be
        // written with offset 0, as HarfBuzz writes it.
        fds: vec![
            (Vec::new(), dict(&[(&[0], 20)]), Vec::new()),
            (Vec::new(), dict(&[(&[1], 20)]), Vec::new()),
        ],
        fd_select: vec![3, 0, 3, 0, 0, 1, 0, 1, 0, 0, 200, 1, 1, 0x90],
    });
    otf(400, cff1(&s))
}

fn charset_format(cff: &[u8]) -> u8 {
    cff[top(cff).charset as usize]
}

fn fd_select(cff: &[u8]) -> Vec<u8> {
    let at = top(cff).fd_select as usize;
    let n = u16::from_be_bytes([cff[at + 1], cff[at + 2]]) as usize;
    cff[at..at + 3 + 3 * n + 2].to_vec()
}

fn fd_count(cff: &[u8]) -> u32 {
    let at = top(cff).fd_array as usize;
    u32::from(u16::from_be_bytes([cff[at], cff[at + 1]]))
}

#[test]
fn charset_format_is_the_smallest() {
    let font = cid_font();
    // One run of 10 CIDs: format 1 (3 bytes) beats format 2 (4) and 0 (20).
    let out = cff_of(&run(&font, &glyphs(&(1..=10).collect::<Vec<_>>())));
    assert_valid(&out);
    assert_eq!(charset_format(&out), 1);
    // A run of 300: format 1 needs two ranges (6 bytes), format 2 one (4).
    let out = cff_of(&run(&font, &glyphs(&(1..=300).collect::<Vec<_>>())));
    assert_valid(&out);
    assert_eq!(charset_format(&out), 2);
    // CIDs 5 and 10: format 0 (4 bytes) beats two ranges.
    let out = cff_of(&run(&font, &glyphs(&[5, 10])));
    assert_valid(&out);
    assert_eq!(charset_format(&out), 0);
    // CFF_IDENTITY_CHARSET: the CIDs become the new glyph ids, one run.
    let bytes = run(
        &font,
        &glyphs(&[5, 10]).with_flags(Flags::CFF_IDENTITY_CHARSET),
    );
    let o = Font::new(&bytes, 0).unwrap();
    let acc = HbCff1::new(&o).unwrap();
    assert_eq!((acc.sid(1), acc.sid(2)), (1, 2));
    // Otherwise the CIDs are kept.
    let bytes = run(&font, &glyphs(&[5, 10]));
    let o = Font::new(&bytes, 0).unwrap();
    let acc = HbCff1::new(&o).unwrap();
    assert_eq!((acc.sid(1), acc.sid(2)), (5, 10));
}

#[test]
fn fdselect_keeps_the_used_font_dicts() {
    let font = cid_font();
    // Glyphs 0 and 250 use only the second Font DICT: it becomes the only
    // one, and FDSelect format 3 has one range.
    let out = cff_of(&run(&font, &glyphs(&[250])));
    assert_valid(&out);
    assert_eq!(fd_count(&out), 1);
    assert_eq!(fd_select(&out), vec![3, 0, 1, 0, 0, 0, 0, 2]);
    // Both used: the Font DICTs stay as they are.
    let out = cff_of(&run(&font, &glyphs(&[1, 250])));
    assert_eq!(fd_count(&out), 2);
    assert_eq!(
        fd_select(&out),
        vec![3, 0, 3, 0, 0, 1, 0, 1, 0, 0, 2, 1, 0, 3]
    );
    // RETAIN_GIDS: the glyphs not kept keep their Font DICT too.
    let out = cff_of(&run(&font, &glyphs(&[250]).with_flags(Flags::RETAIN_GIDS)));
    assert_valid(&out);
    assert_eq!(fd_count(&out), 2);
    assert_eq!(
        fd_select(&out),
        vec![3, 0, 3, 0, 0, 1, 0, 1, 0, 0, 200, 1, 0, 251]
    );
}

#[test]
fn iftb_requirements_use_four_byte_charstring_offsets() {
    let font = otf(2, cff1(&Cff1Spec::new(vec![endchar(), endchar()])));
    let out = cff_of(&run(
        &font,
        &glyphs(&[1]).with_flags(Flags::IFTB_REQUIREMENTS),
    ));
    assert_valid(&out);
    let at = top(&out).char_strings as usize;
    assert_eq!(out[at + 2], 4);
    let out = cff_of(&run(&font, &glyphs(&[1])));
    let at = top(&out).char_strings as usize;
    assert_eq!(out[at + 2], 1);
}

#[test]
fn vorg_keeps_the_records_of_kept_glyphs() {
    let cff = cff1(&Cff1Spec::new(vec![endchar(); 4]));
    let font = otto_font(
        4,
        vec![
            (b"CFF ", cff),
            (b"VORG", vorg(1, 880, &[(1, 800), (2, 810), (3, 820)])),
        ],
    );
    let out = run(&font, &glyphs(&[1, 3]));
    let s = Sfnt::new(&out).unwrap();
    assert_eq!(
        s.table(Tag::new(b"VORG")).unwrap(),
        &vorg(1, 880, &[(1, 800), (2, 820)])[..]
    );
}
