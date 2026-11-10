//! Preprocessing (ADR 0022 stage 11): a plan made with [`Prepared`] gives
//! the same mappings and the same bytes as one made without it, through
//! every way the plan collects characters and on glyf and CFF fonts; and
//! the CFF charstring accessors.

#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::{Font, HbCff1};
use sofel_subset::{
    Flags, Input, Plan, Prepared, RangeSet, SetKind, SubsetError, cff_charstring_data,
    cff_charstrings_index,
};

/// A glyf font with the sample cmap (A to Z are glyphs 1 to 26, U+1F600
/// glyph 27); glyph 2 is a composite of glyph 1.
fn glyf_font() -> Vec<u8> {
    let a = simple_glyph(
        &[&[Pt::On(0, 0), Pt::On(100, 0), Pt::On(100, 100)]],
        false,
        &[1, 2],
    );
    let c = composite_glyph(&[comp(1, Args::Xy(10, 10))], [10, 10, 110, 110], false);
    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    for g in 0..28 {
        loca.extend_from_slice(&((glyf.len() / 2) as u16).to_be_bytes());
        let bytes = match g {
            0 | 1 | 5 => a.clone(),
            2 => c.clone(),
            _ => Vec::new(),
        };
        glyf.extend_from_slice(&bytes);
        if glyf.len() % 2 == 1 {
            glyf.push(0);
        }
    }
    loca.extend_from_slice(&((glyf.len() / 2) as u16).to_be_bytes());
    let mut t = sample_tables();
    t.push((b"glyf", glyf));
    t.push((b"loca", loca));
    t.sort_by_key(|(t, _)| **t);
    sfnt(&t)
}

fn endchar() -> Vec<u8> {
    charstring(&[Cs::Op(14)])
}

/// A name-keyed CFF font: glyph 1 calls a local and a global
/// subroutine after hints, glyph 2 a local subroutine, glyph 3 is an
/// `seac` of glyphs 1 and 2.
fn cff_font() -> Vec<u8> {
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
        Cs::I(0 - 107),
        Cs::Op(10),
        Cs::I(1 - 107),
        Cs::Op(29),
        Cs::Op(14),
    ]);
    let g2 = charstring(&[Cs::I(1 - 107), Cs::Op(10), Cs::Op(14)]);
    let seac = charstring(&[Cs::I(0), Cs::I(0), Cs::I(65), Cs::I(194), Cs::Op(14)]);
    let mut spec = Cff1Spec::new(vec![endchar(), g1, g2, seac, endchar()]);
    spec.lsubrs = vec![
        charstring(&[Cs::I(5), Cs::I(5), Cs::Op(5), Cs::Op(11)]),
        charstring(&[Cs::I(1), Cs::Op(21), Cs::Op(11)]),
    ];
    spec.gsubrs = vec![
        vec![11],
        charstring(&[Cs::I(3), Cs::I(3), Cs::Op(5), Cs::Op(11)]),
    ];
    let mut charset = vec![0u8];
    for sid in [
        sofel_font::read::cff::STANDARD_ENCODING[65],
        sofel_font::read::cff::STANDARD_ENCODING[194],
        200,
        201,
    ] {
        charset.extend(sid.to_be_bytes());
    }
    spec.charset = Some(charset);
    otto_font(5, vec![(b"CFF ", cff1(&spec))])
}

fn set(values: &[u32]) -> RangeSet {
    let mut s = RangeSet::new();
    values.iter().for_each(|&v| s.add(v));
    s
}

fn input(unicodes: RangeSet, glyphs: RangeSet, flags: Flags) -> Input {
    Input::new()
        .with_set(SetKind::Unicodes, unicodes)
        .with_set(SetKind::Glyphs, glyphs)
        .with_flags(flags)
}

/// The inputs that take each way of collecting characters: few
/// characters and no glyphs (the direct lookup), glyphs with few
/// characters (the reverse map of the prepared value), many characters
/// or glyphs (the walk over the cmap), and the flags that change the
/// glyph closure or the CFF programs.
fn inputs() -> Vec<Input> {
    let few = set(&[0x41, 0x42, 0x45]);
    let all = RangeSet::all();
    let mut out = Vec::new();
    for flags in [
        Flags::DEFAULT,
        Flags::DEFAULT | Flags::NO_HINTING,
        Flags::DEFAULT | Flags::RETAIN_GIDS,
        Flags::DEFAULT | Flags::DESUBROUTINIZE,
        Flags::DEFAULT | Flags::NOTDEF_OUTLINE,
    ] {
        out.push(input(few.clone(), RangeSet::new(), flags));
        out.push(input(RangeSet::new(), set(&[2, 3]), flags));
        out.push(input(few.clone(), set(&[3, 27, 400]), flags));
        out.push(input(all.clone(), RangeSet::new(), flags));
        out.push(input(RangeSet::new(), all.clone(), flags));
        out.push(input(
            (0x41..0x5B)
                .chain([0x1F600])
                .fold(RangeSet::new(), |mut s, c| {
                    s.add(c);
                    s
                }),
            set(&[4]),
            flags,
        ));
        out.push(input(set(&[0x1F600]), set(&[1]), flags));
    }
    out
}

/// Every input gives the same plan and bytes with and without `prepared`.
fn assert_same(data: &[u8]) {
    let font = Font::new(data, 0).unwrap();
    let prepared = Prepared::new(&font);
    for input in inputs() {
        let plain = Plan::new(&font, &input);
        let fast = Plan::with_prepared(&font, &prepared, &input);
        match (plain, fast) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.new_to_old_glyphs(), b.new_to_old_glyphs(), "{input:?}");
                assert_eq!(a.num_output_glyphs(), b.num_output_glyphs());
                assert_eq!(
                    a.unicode_to_old_glyphs().collect::<Vec<_>>(),
                    b.unicode_to_old_glyphs().collect::<Vec<_>>(),
                    "{input:?}"
                );
                assert_eq!(a.execute(&font), b.execute(&font), "{input:?}");
            }
            (a, b) => assert_eq!(a.err(), b.err(), "{input:?}"),
        }
    }
}

#[test]
fn prepared_plans_equal_plain_plans_on_glyf() {
    assert_same(&glyf_font());
}

#[test]
fn prepared_plans_equal_plain_plans_on_cff() {
    let data = cff_font();
    assert_same(&data);
    // The seac components come in with and without the prepared value.
    let font = Font::new(&data, 0).unwrap();
    let prepared = Prepared::new(&font);
    // The prepared value holds the parsed programs and the seac.
    let debug = format!("{prepared:?}");
    assert!(debug.contains("has_seac: true, cff: true"), "{debug}");
    let i = input(RangeSet::new(), set(&[3]), Flags::DEFAULT);
    let plan = Plan::with_prepared(&font, &prepared, &i).unwrap();
    assert_eq!(
        plan.glyph_set().iter().collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}

#[test]
fn prepared_plans_keep_the_subroutines_the_glyphs_call() {
    let data = cff_font();
    let font = Font::new(&data, 0).unwrap();
    let prepared = Prepared::new(&font);
    for flags in [Flags::DEFAULT, Flags::DEFAULT | Flags::NO_HINTING] {
        let i = input(RangeSet::new(), set(&[2]), flags);
        let out = Plan::with_prepared(&font, &prepared, &i)
            .unwrap()
            .execute(&font)
            .unwrap();
        let o = Font::new(&out, 0).unwrap();
        let acc = HbCff1::new(&o).unwrap();
        // Glyph 2 calls only local subroutine 1.
        assert_eq!(acc.local_subrs(0).len(), 1);
        assert_eq!(acc.global_subrs().len(), 0);
    }
}

#[test]
fn prepared_from_another_font_is_refused() {
    let a = glyf_font();
    let b = cff_font();
    let fa = Font::new(&a, 0).unwrap();
    let fb = Font::new(&b, 0).unwrap();
    let prepared = Prepared::new(&fb);
    let i = input(set(&[0x41]), RangeSet::new(), Flags::DEFAULT);
    assert_eq!(
        Plan::with_prepared(&fa, &prepared, &i).err(),
        Some(SubsetError::InvalidInput)
    );
}

#[test]
fn prepared_is_shared_between_threads() {
    fn shared<T: Send + Sync + Clone + 'static>() {}
    shared::<Prepared>();
    let data = glyf_font();
    let font = Font::new(&data, 0).unwrap();
    let prepared = Prepared::new(&font);
    let i = input(set(&[0x42]), RangeSet::new(), Flags::DEFAULT);
    let expected = Plan::new(&font, &i).unwrap().execute(&font).unwrap();
    let check = || {
        let font = Font::new(&data, 0).unwrap();
        let out = Plan::with_prepared(&font, &prepared, &i)
            .unwrap()
            .execute(&font)
            .unwrap();
        assert_eq!(out, expected);
    };
    // wasm32-wasip1 has no threads.
    #[cfg(not(target_os = "wasi"))]
    std::thread::scope(|s| {
        for _ in 0..2 {
            s.spawn(check);
        }
    });
    #[cfg(target_os = "wasi")]
    check();
}

#[test]
fn prepared_without_cmap_or_cff_still_plans() {
    // A font without a CFF table and with a cmap that maps nothing.
    let data = otto_font(3, Vec::new());
    let font = Font::new(&data, 0).unwrap();
    let prepared = Prepared::new(&font);
    let i = input(set(&[0x41]), set(&[1]), Flags::DEFAULT);
    let a = Plan::new(&font, &i).unwrap();
    let b = Plan::with_prepared(&font, &prepared, &i).unwrap();
    assert_eq!(a.new_to_old_glyphs(), b.new_to_old_glyphs());
}

#[test]
fn charstring_accessors() {
    let data = cff_font();
    let font = Font::new(&data, 0).unwrap();
    let index = cff_charstrings_index(&font).unwrap();
    // The INDEX: a count of 5, then offsets and data, ending the table's
    // CharStrings.
    assert_eq!(u16::from_be_bytes([index[0], index[1]]), 5);
    let acc = HbCff1::new(&font).unwrap();
    for g in 0..5 {
        assert_eq!(cff_charstring_data(&font, g), acc.char_string(g));
        let d = cff_charstring_data(&font, g).unwrap();
        // Each charstring lies within the INDEX bytes.
        let start = d.as_ptr() as usize - index.as_ptr() as usize;
        assert!(start + d.len() <= index.len());
    }
    assert_eq!(cff_charstring_data(&font, 5), None);
    // No CFF table.
    let g = glyf_font();
    let gf = Font::new(&g, 0).unwrap();
    assert_eq!(cff_charstrings_index(&gf), None);
    assert_eq!(cff_charstring_data(&gf, 0), None);
}
