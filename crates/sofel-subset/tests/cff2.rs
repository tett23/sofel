//! CFF2 with every axis pinned on hand-made fonts (ADR 0022 stage 7):
//! the blends evaluated in the charstrings and the Private DICT, the
//! rounding of the absolute positions, the BCD reals of the Private DICT,
//! and the conversion to CFF with `DOWNGRADE_CFF2` (respecialized
//! charstrings, widths, ROS, charset and FDSelect).

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
use sofel_font::read::cff::Cff1;
use sofel_font::read::cff::hb::{DictKind, interpret_dict};
use sofel_font::read::sfnt::Sfnt;
use sofel_font::{Font, HbCff1, HbCff2};
use sofel_subset::{Flags, Input, Plan, SetKind, SubsetError, subset};

fn fixed(v: f64) -> [u8; 4] {
    ((v * 65536.0) as i32).to_be_bytes()
}

/// An fvar with one axis wght 100–400–900.
fn fvar() -> Vec<u8> {
    let mut d = vec![0, 1, 0, 0];
    d.extend(16u16.to_be_bytes());
    d.extend(2u16.to_be_bytes());
    d.extend(1u16.to_be_bytes());
    d.extend(20u16.to_be_bytes());
    d.extend(0u16.to_be_bytes());
    d.extend(10u16.to_be_bytes());
    d.extend(*b"wght");
    d.extend(fixed(100.0));
    d.extend(fixed(400.0));
    d.extend(fixed(900.0));
    d.extend(0u16.to_be_bytes());
    d.extend(256u16.to_be_bytes());
    d
}

fn f(v: f64) -> Cs {
    Cs::F((v * 65536.0) as i32)
}

/// Glyph 1: a blended move and a line at fractional positions; glyph 2:
/// no blend, a horizontal move and a vertical line; glyph 3: a blend and
/// flex1.
fn charstrings() -> Vec<Vec<u8>> {
    let g1 = charstring(&[
        f(10.25),
        f(20.75),
        Cs::I(5),
        Cs::I(-5),
        Cs::I(3),
        Cs::I(-3),
        Cs::I(2),
        Cs::Op(16),
        Cs::Op(21),
        f(1.5),
        f(2.5),
        Cs::Op(5),
    ]);
    let g2 = charstring(&[f(1.5), Cs::I(0), Cs::Op(21), Cs::I(0), Cs::I(10), Cs::Op(5)]);
    let mut g3 = vec![
        f(2.5),
        Cs::I(1),
        Cs::I(1),
        Cs::I(1),
        Cs::Op(16),
        Cs::I(0),
        Cs::Op(21),
    ];
    g3.extend([10, 0, 10, 0, 10, 0, 10, 0, 10, 0, 10].map(Cs::I));
    g3.push(Cs::Op(0x0C25));
    vec![Vec::new(), g1, g2, charstring(&g3)]
}

/// The Private DICT: BlueValues blended, BlueScale and ExpansionFactor
/// as reals, StdHW.
fn private() -> Vec<u8> {
    let mut p = Vec::new();
    for v in [-10, 0, 1, 1, 1, 1, 2] {
        p.extend(d_int(v));
    }
    p.extend(d_op(23));
    p.extend(d_op(6));
    p.extend(d_real(".039625"));
    p.extend(d_op(0x0C09));
    p.extend(d_real(".06"));
    p.extend(d_op(0x0C12));
    p.extend(d_int(50));
    p.extend(d_op(10));
    p
}

fn font_with_subrs(gsubrs: Vec<Vec<u8>>) -> Vec<u8> {
    font_with_programs(gsubrs, Vec::new(), charstrings())
}

fn font_with_programs(
    gsubrs: Vec<Vec<u8>>,
    lsubrs: Vec<Vec<u8>>,
    programs: Vec<Vec<u8>>,
) -> Vec<u8> {
    let mut top = Vec::new();
    for r in [".001", "0", "0", ".001", "0", "0"] {
        top.extend(d_real(r));
    }
    top.extend(d_op(0x0C07));
    let vstore = item_variation_store(
        1,
        &[vec![(0, 16384, 16384)], vec![(-16384, -16384, 0)]],
        &[vec![0, 1]],
    );
    let t = cff2(&Cff2Spec {
        top,
        gsubrs,
        charstrings: programs,
        fds: vec![(private(), lsubrs)],
        vstore: Some(vstore),
        ..Cff2Spec::default()
    });
    otto_font(4, vec![(b"CFF2", t), (b"fvar", fvar())])
}

fn font() -> Vec<u8> {
    font_with_subrs(Vec::new())
}

fn glyphs(ids: &[u32]) -> Input {
    Input::new().map_set(SetKind::Glyphs, |s| {
        for &g in ids {
            s.add(g);
        }
    })
}

fn pinned(input: Input) -> (Vec<u8>, Font<'static>) {
    let bytes: &'static [u8] = Box::leak(font().into_boxed_slice());
    let f = Font::new(bytes, 0).unwrap();
    let input = input.with_all_axes_pinned_to_default(&f).unwrap();
    (subset(&f, &input).unwrap(), f)
}

fn table(out: &[u8], tag: &[u8; 4]) -> Option<Vec<u8>> {
    Sfnt::new(out)
        .unwrap()
        .table(Tag::new(tag))
        .map(<[u8]>::to_vec)
}

/// The Private DICT bytes of the first Font DICT of a CFF2 table.
fn cff2_private(out: &[u8]) -> Vec<u8> {
    let o = Font::new(out, 0).unwrap();
    let c = HbCff2::new(&o).unwrap();
    let fd = interpret_dict(c.fd_array().get(0).unwrap(), DictKind::Cff2Font).unwrap();
    let at = fd.private_offset as usize;
    c.data()[at..at + fd.private_size as usize].to_vec()
}

#[test]
fn cff2_without_subroutines_keeps_variations() {
    let bytes = font();
    let f = Font::new(&bytes, 0).unwrap();
    let out = subset(&f, &glyphs(&[1])).unwrap();
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert!(cff2.var_store().is_some());
    assert_eq!(cff2.num_glyphs(), 2);
    assert!(cff2.char_string(1).unwrap().contains(&16));
    let drop = glyphs(&[1]).map_set(SetKind::DropTables, |s| {
        s.add(Tag::new(b"CFF2").to_u32());
        s.add(Tag::new(b"fvar").to_u32());
    });
    let out = subset(&f, &drop).unwrap();
    assert!(table(&out, b"CFF2").is_none());
}

#[test]
fn cff2_unused_subroutines_are_removed_for_the_selected_glyphs() {
    let mut programs = charstrings();
    // Glyph 2 alone calls global subroutine 0 (bias 107).
    programs[2] = charstring(&[Cs::I(-107), Cs::Op(29)]);
    let bytes = font_with_programs(vec![vec![11]], Vec::new(), programs);
    let font = Font::new(&bytes, 0).unwrap();
    let without_subrs = self::font();
    let reference = subset(&Font::new(&without_subrs, 0).unwrap(), &glyphs(&[1])).unwrap();
    assert!(Plan::new(&font, &glyphs(&[1])).is_ok());
    assert_eq!(subset(&font, &glyphs(&[1])).unwrap(), reference);
    assert!(Plan::new(&font, &glyphs(&[2])).is_ok());
    let used = subset(&font, &glyphs(&[2])).unwrap();
    let result = Font::new(&used, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.global_subrs().len(), 1);
    assert_eq!(cff2.global_subrs().get(0), Some([11].as_slice()));
    assert_eq!(
        cff2.char_string(1),
        Some(charstring(&[Cs::I(-107), Cs::Op(29)]).as_slice())
    );
}

#[test]
fn cff2_unused_local_subroutines_are_removed_for_the_selected_glyphs() {
    let mut programs = charstrings();
    programs[2] = charstring(&[Cs::I(-107), Cs::Op(10)]);
    let bytes = font_with_programs(Vec::new(), vec![vec![11]], programs);
    let font = Font::new(&bytes, 0).unwrap();
    let without_subrs = self::font();
    let reference = subset(&Font::new(&without_subrs, 0).unwrap(), &glyphs(&[1])).unwrap();
    assert!(Plan::new(&font, &glyphs(&[1])).is_ok());
    assert_eq!(subset(&font, &glyphs(&[1])).unwrap(), reference);
    assert!(Plan::new(&font, &glyphs(&[2])).is_ok());
    let used = subset(&font, &glyphs(&[2])).unwrap();
    let result = Font::new(&used, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.global_subrs().len(), 0);
    assert_eq!(cff2.private(0).0.len(), 1);
    assert_eq!(cff2.private(0).0.get(0), Some([11].as_slice()));
    assert_eq!(
        cff2.char_string(1),
        Some(charstring(&[Cs::I(-107), Cs::Op(10)]).as_slice())
    );
}

#[test]
fn cff2_called_subroutines_collect_nested_calls_and_renumber() {
    let mut programs = charstrings();
    // Old global 2 calls local 2; old local 2 calls global 1.
    programs[2] = charstring(&[Cs::I(-105), Cs::Op(29)]);
    let gsubrs = vec![
        vec![11],
        vec![11],
        charstring(&[Cs::I(-105), Cs::Op(10), Cs::Op(11)]),
    ];
    let lsubrs = vec![
        vec![11],
        vec![11],
        charstring(&[Cs::I(-106), Cs::Op(29), Cs::Op(11)]),
    ];
    let bytes = font_with_programs(gsubrs, lsubrs, programs);
    let out = subset(&Font::new(&bytes, 0).unwrap(), &glyphs(&[2])).unwrap();
    // HarfBuzz 15.1.0: hb-dump --subset --font-file=INPUT
    // --output-file=OUTPUT --gids=2 (the generated INPUT is this test's bytes).
    assert_eq!(out, include_bytes!("data/cff2-called-subrs-hb15.otf"));
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.global_subrs().len(), 2);
    assert_eq!(cff2.private(0).0.len(), 1);
    assert_eq!(
        cff2.char_string(1),
        Some(charstring(&[Cs::I(-106), Cs::Op(29)]).as_slice())
    );
    assert_eq!(
        cff2.global_subrs().get(1),
        Some(charstring(&[Cs::I(-107), Cs::Op(10), Cs::Op(11)]).as_slice())
    );
    assert_eq!(
        cff2.private(0).0.get(0),
        Some(charstring(&[Cs::I(-107), Cs::Op(29), Cs::Op(11)]).as_slice())
    );
}

#[test]
fn cff2_called_subroutines_recompute_bias_after_shrinking_index() {
    let mut programs = charstrings();
    programs[2] = charstring(&[Cs::I(109), Cs::Op(29)]);
    let mut gsubrs = vec![vec![11]; 1241];
    gsubrs[1240] = charstring(&[Cs::I(50), Cs::Op(22), Cs::Op(11)]);
    let bytes = font_with_programs(gsubrs, Vec::new(), programs);
    let out = subset(&Font::new(&bytes, 0).unwrap(), &glyphs(&[2])).unwrap();
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.global_subrs().len(), 1);
    assert_eq!(
        cff2.global_subrs().get(0),
        Some(charstring(&[Cs::I(50), Cs::Op(22), Cs::Op(11)]).as_slice())
    );
    assert_eq!(
        cff2.char_string(1),
        Some(charstring(&[Cs::I(-107), Cs::Op(29)]).as_slice())
    );
}

#[test]
fn cff2_no_hinting_removes_calls_to_empty_subroutines() {
    for (call, subroutine) in [(29, vec![11]), (10, vec![11]), (29, vec![]), (10, vec![])] {
        let mut programs = charstrings();
        programs[2] = charstring(&[Cs::Op(19), Cs::I(-107), Cs::Op(call)]);
        let (globals, locals) = if call == 29 {
            (vec![subroutine], vec![])
        } else {
            (vec![], vec![subroutine])
        };
        let bytes = font_with_programs(globals, locals, programs);
        let font = Font::new(&bytes, 0).unwrap();
        assert!(Plan::new(&font, &glyphs(&[2])).is_ok());
        let out = subset(&font, &glyphs(&[2]).with_flags(Flags::NO_HINTING)).unwrap();
        if call == 29 {
            assert_eq!(out, include_bytes!("data/cff2-no-hint-empty-subr-hb15.otf"));
        }
        let result = Font::new(&out, 0).unwrap();
        let cff2 = HbCff2::new(&result).unwrap();
        assert_eq!(cff2.char_string(1), Some([].as_slice()));
        assert!(cff2.global_subrs().is_empty());
        assert!(cff2.private(0).0.is_empty());
    }
}

#[test]
fn cff2_no_hinting_removes_stem_args_supplied_by_caller() {
    let mut programs = charstrings();
    programs[2] = charstring(&[Cs::I(0), Cs::I(10), Cs::I(-107), Cs::Op(29)]);
    let global = charstring(&[
        Cs::Op(1),
        Cs::I(0),
        Cs::I(0),
        Cs::Op(21),
        Cs::I(5),
        Cs::I(5),
        Cs::Op(5),
        Cs::Op(11),
    ]);
    let bytes = font_with_programs(vec![global], Vec::new(), programs);
    let font = Font::new(&bytes, 0).unwrap();
    let out = subset(&font, &glyphs(&[2]).with_flags(Flags::NO_HINTING)).unwrap();
    assert_eq!(
        out,
        include_bytes!("data/cff2-no-hint-caller-stems-hb15.otf")
    );
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.char_string(1), Some([32, 29].as_slice()));
    assert_eq!(
        cff2.global_subrs().get(0),
        Some([139, 139, 21, 144, 144, 5].as_slice())
    );
}

#[test]
fn cff2_hintmask_bytes_that_look_like_calls_are_not_calls() {
    let mut programs = charstrings();
    programs[2] = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(1),
        Cs::Op(19),
        Cs::Raw(vec![29]),
        Cs::Op(20),
        Cs::Raw(vec![10]),
        Cs::I(-107),
        Cs::Op(29),
    ]);
    let bytes = font_with_programs(vec![vec![11]], vec![vec![11]], programs);
    let font = Font::new(&bytes, 0).unwrap();
    assert!(Plan::new(&font, &glyphs(&[2])).is_ok());
    let out = subset(&font, &glyphs(&[2])).unwrap();
    // HarfBuzz 15.1.0: hb-dump --subset --font-file=INPUT --output-file=OUTPUT --gids=2.
    assert_eq!(out, include_bytes!("data/cff2-hintmask-hb15.otf"));
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.global_subrs().len(), 1);
    assert_eq!(cff2.private(0).0.len(), 0);
    assert!(
        cff2.char_string(1)
            .unwrap()
            .windows(2)
            .any(|w| w == [19, 29])
    );
    assert!(
        cff2.char_string(1)
            .unwrap()
            .windows(2)
            .any(|w| w == [20, 10])
    );
}

#[test]
fn cff2_mask_length_counts_stems_added_in_a_subroutine() {
    let mut programs = charstrings();
    programs[2] = charstring(&[
        Cs::I(-107),
        Cs::Op(29),
        Cs::Op(19),
        Cs::Raw(vec![10]),
        Cs::I(-107),
        Cs::Op(10),
    ]);
    let global = charstring(&[Cs::I(0), Cs::I(10), Cs::Op(1), Cs::Op(11)]);
    let bytes = font_with_programs(vec![global], vec![vec![11]], programs);
    let font = Font::new(&bytes, 0).unwrap();
    assert!(Plan::new(&font, &glyphs(&[2])).is_ok());
    let out = subset(&font, &glyphs(&[2])).unwrap();
    // HarfBuzz 15.1.0, with INPUT built above and --gids=2.
    assert_eq!(out, include_bytes!("data/cff2-subr-stem-hb15.otf"));
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.global_subrs().len(), 1);
    assert_eq!(cff2.private(0).0.len(), 1);
    assert!(
        cff2.char_string(1)
            .unwrap()
            .windows(2)
            .any(|w| w == [19, 10])
    );
}

#[test]
fn cff2_shared_subroutine_with_conflicting_mask_lengths_retains_original_bytes() {
    let mut programs = charstrings();
    programs[1] = charstring(&[Cs::I(0), Cs::I(10), Cs::Op(1), Cs::I(-107), Cs::Op(29)]);
    programs[2] = charstring(&[Cs::I(-107), Cs::Op(29)]);
    let bytes = font_with_programs(vec![vec![19, 139, 11]], Vec::new(), programs);
    let font = Font::new(&bytes, 0).unwrap();
    let out = subset(&font, &glyphs(&[1, 2])).unwrap();
    assert_eq!(out, include_bytes!("data/cff2-context-mask-hb15.otf"));
    assert!(matches!(
        subset(&font, &glyphs(&[1, 2]).with_flags(Flags::NO_HINTING)),
        Err(SubsetError::Unsupported { tag, .. }) if tag == Tag::new(b"CFF2")
    ));
}

#[test]
fn cff2_no_hinting_removes_used_glyph_and_subroutine_stems() {
    let mut programs = charstrings();
    programs[2] = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(1),
        Cs::I(-107),
        Cs::Op(29),
        Cs::I(10),
        Cs::I(0),
        Cs::Op(5),
    ]);
    let global = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(3),
        Cs::I(0),
        Cs::I(0),
        Cs::Op(21),
        Cs::I(5),
        Cs::I(5),
        Cs::Op(5),
        Cs::Op(11),
    ]);
    let bytes = font_with_programs(vec![global], Vec::new(), programs);
    let font = Font::new(&bytes, 0).unwrap();
    let input = glyphs(&[2]).with_flags(Flags::NO_HINTING);
    assert!(Plan::new(&font, &input).is_ok());
    let out = subset(&font, &input).unwrap();
    // HarfBuzz 15.1.0: hb-dump --subset --gids=2 --no-hinting.
    assert_eq!(out, include_bytes!("data/cff2-no-hint-stems-hb15.otf"));
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.global_subrs().len(), 1);
    assert!(!cff2.char_string(1).unwrap().contains(&1));
    assert!(!cff2.global_subrs().get(0).unwrap().contains(&3));
}

#[test]
fn cff2_no_hinting_removes_masks_and_preserves_used_paths() {
    let mut programs = charstrings();
    programs[2] = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(1),
        Cs::Op(19),
        Cs::Raw(vec![29]),
        Cs::I(-107),
        Cs::Op(29),
        Cs::Op(19),
        Cs::Raw(vec![29]),
        Cs::I(10),
        Cs::I(0),
        Cs::Op(5),
    ]);
    let global = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(3),
        Cs::Op(20),
        Cs::Raw(vec![10]),
        Cs::I(0),
        Cs::I(0),
        Cs::Op(21),
        Cs::Op(20),
        Cs::Raw(vec![10]),
        Cs::I(5),
        Cs::I(5),
        Cs::Op(5),
        Cs::Op(11),
    ]);
    let bytes = font_with_programs(vec![global], Vec::new(), programs);
    let font = Font::new(&bytes, 0).unwrap();
    let input = glyphs(&[2]).with_flags(Flags::NO_HINTING);
    assert!(Plan::new(&font, &input).is_ok());
    let out = subset(&font, &input).unwrap();
    // The mask and stem commands disappear; the path has the same golden output.
    assert_eq!(out, include_bytes!("data/cff2-no-hint-stems-hb15.otf"));
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.global_subrs().len(), 1);
    assert!(!cff2.char_string(1).unwrap().contains(&19));
    assert!(!cff2.global_subrs().get(0).unwrap().contains(&20));
}

#[test]
fn cff2_no_hinting_keeps_only_used_local_subroutine_without_hints() {
    let mut programs = charstrings();
    programs[2] = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(1),
        Cs::I(-106),
        Cs::Op(10),
        Cs::I(10),
        Cs::I(0),
        Cs::Op(5),
    ]);
    let unused = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(1),
        Cs::Op(19),
        Cs::Raw(vec![29]),
        Cs::Op(11),
    ]);
    let used = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(3),
        Cs::I(0),
        Cs::I(0),
        Cs::Op(21),
        Cs::I(5),
        Cs::I(5),
        Cs::Op(5),
        Cs::Op(11),
    ]);
    let bytes = font_with_programs(Vec::new(), vec![unused, used], programs);
    let font = Font::new(&bytes, 0).unwrap();
    let input = glyphs(&[2]).with_flags(Flags::NO_HINTING);
    let out = subset(&font, &input).unwrap();
    // HarfBuzz 15.1.0: hb-dump --subset --gids=2 --no-hinting.
    assert_eq!(out, include_bytes!("data/cff2-no-hint-local-hb15.otf"));
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.private(0).0.len(), 1);
    assert_eq!(
        cff2.char_string(1).unwrap().get(0..2),
        Some([32, 10].as_slice())
    );
}

#[test]
fn cff2_keep_variations_with_desubroutinize() {
    let bytes = font();
    let f = Font::new(&bytes, 0).unwrap();
    let input = glyphs(&[1]).with_flags(Flags::DESUBROUTINIZE);
    let out = subset(&f, &input).unwrap();
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert!(cff2.var_store().is_some());
    assert!(cff2.global_subrs().is_empty());
    assert_eq!(cff2.num_glyphs(), 2);
    assert!(cff2.char_string(1).unwrap().contains(&16));
    assert_eq!(cff2_private(&out), private());
}

#[test]
fn cff2_without_variation_store_can_be_desubroutinized() {
    let bytes = otto_font(
        2,
        vec![(
            b"CFF2",
            cff2(&Cff2Spec {
                charstrings: vec![Vec::new(), charstring(&[Cs::I(1), Cs::I(2), Cs::Op(21)])],
                fds: vec![(Vec::new(), Vec::new())],
                ..Cff2Spec::default()
            }),
        )],
    );
    let font = Font::new(&bytes, 0).unwrap();
    let out = subset(&font, &glyphs(&[1]).with_flags(Flags::DESUBROUTINIZE)).unwrap();
    let result = Font::new(&out, 0).unwrap();
    let cff2 = HbCff2::new(&result).unwrap();
    assert_eq!(cff2.var_store(), None);
    assert_eq!(cff2.char_string(1).unwrap(), &[140, 141, 21]);
}

#[test]
fn pinned_blends_are_evaluated_and_positions_rounded() {
    let (out, _) = pinned(glyphs(&[1, 2, 3]));
    assert!(table(&out, b"fvar").is_none());
    let o = Font::new(&out, 0).unwrap();
    let c = HbCff2::new(&o).unwrap();
    // No subroutines and no Variation Store are left.
    assert!(c.global_subrs().is_empty());
    assert_eq!(c.var_store(), None);
    // Glyph 1 rounds the absolute positions (10.25, 20.75) and
    // (11.75, 23.25): 10 21 rmoveto 2 2 rlineto.
    assert_eq!(c.char_string(1).unwrap(), &[149, 160, 21, 141, 141, 5]);
    // Glyph 2 has no blend: its numbers stay as they are.
    assert_eq!(
        c.char_string(2).unwrap(),
        &[255, 0, 1, 0x80, 0, 139, 21, 139, 149, 5]
    );
    // Glyph 3 has flex1: its blend is rounded on its own, half away from
    // zero.
    let mut g3 = vec![142, 139, 21];
    g3.extend([149, 139, 149, 139, 149, 139, 149, 139, 149, 139, 149]);
    g3.extend([12, 37]);
    assert_eq!(c.char_string(3).unwrap(), &g3[..]);
    // The Top DICT keeps FontMatrix once and drops vstore.
    let top = &c.data()[5..5 + u16::from_be_bytes([c.data()[3], c.data()[4]]) as usize];
    assert_eq!(interpret_dict(top, DictKind::Cff2Top).unwrap().vstore, 0);
    let mut matrix = Vec::new();
    for r in [".001", "0", "0", ".001", "0", "0"] {
        matrix.extend(d_real(r));
    }
    matrix.extend([12, 7]);
    let found = top
        .windows(matrix.len())
        .filter(|w| *w == &matrix[..])
        .count();
    assert_eq!(found, 1);
    // The Private DICT: BlueValues evaluated, the reals written as
    // fontTools writes them, no vsindex.
    let mut p = vec![129, 139, 6];
    p.extend([30, 0x39, 0x62, 0x5C, 0x6F, 12, 9]);
    p.extend([30, 0x6C, 0x2F, 12, 18]);
    p.extend([189, 10]);
    assert_eq!(cff2_private(&out), p);
}

#[test]
fn pinned_metrics_come_from_the_outlines() {
    let (out, _) = pinned(glyphs(&[1, 2]));
    let head = table(&out, b"head").unwrap();
    let i16_at = |d: &[u8], at: usize| i16::from_be_bytes([d[at], d[at + 1]]);
    // Glyph 1 spans x 10..12 and y 21..23; glyph 2 y 0..10, and its box
    // without width counts as x 0..0.
    assert_eq!([36, 38, 40, 42].map(|a| i16_at(&head, a)), [0, 0, 12, 23]);
    let hmtx = table(&out, b"hmtx").unwrap();
    // The left side bearings: 0 for the empty .notdef (hmtx), then the
    // boxes' x_bearing (0 for glyph 2, whose box has no width).
    assert_eq!(i16_at(&hmtx, 2), 0);
    assert_eq!(i16_at(&hmtx, 6), 10);
    assert_eq!(i16_at(&hmtx, 10), 0);
}

#[test]
fn hints_are_dropped() {
    let (out, _) = pinned(glyphs(&[1]).with_flags(Flags::NO_HINTING));
    assert!(cff2_private(&out).is_empty());
}

#[test]
fn downgrade_writes_a_cid_keyed_cff() {
    let (out, _) = pinned(glyphs(&[1, 2, 3]).with_flags(Flags::DOWNGRADE_CFF2));
    assert!(table(&out, b"CFF2").is_none());
    let cff = table(&out, b"CFF ").unwrap();
    let mut errors = Vec::new();
    sofel_font::read::cff::validate(&cff, None, &mut |d| {
        if d.severity == sofel_font::read::Severity::Error {
            errors.push(format!("{:?}", d.kind));
        }
    });
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(&Sfnt::new(&out).unwrap().version().to_be_bytes(), b"OTTO");
    let c = Cff1::new(&cff).unwrap();
    assert_eq!(c.names().get(0).unwrap(), b"CFF1Font");
    assert_eq!(c.strings().get(0).unwrap(), b"Adobe");
    assert_eq!(c.strings().get(1).unwrap(), b"Identity");
    let o = Font::new(&out, 0).unwrap();
    let h = HbCff1::new(&o).unwrap();
    assert!(h.is_cid());
    // Widths 500 to 503: defaultWidthX and nominalWidthX are 500; the
    // others are written first. The charstrings end with endchar.
    assert_eq!(h.char_string(0).unwrap(), &[14]);
    assert_eq!(
        h.char_string(1).unwrap(),
        &[140, 149, 160, 21, 141, 141, 5, 14]
    );
    // rmoveto 1.5 0 and rlineto 0 10 become hmoveto and vlineto.
    assert_eq!(
        h.char_string(2).unwrap(),
        &[141, 255, 0, 1, 0x80, 0, 22, 149, 7, 14]
    );
    // The identity charset (format 2, CIDs 1 to 3) and FontBBox from head.
    let top = interpret_dict(c.top_dicts().get(0).unwrap(), DictKind::Cff1Top).unwrap();
    let cs = top.charset as usize;
    assert_eq!(&cff[cs..cs + 5], &[2, 0, 1, 0, 2]);
    let fs = top.fd_select as usize;
    assert_eq!(&cff[fs..fs + 8], &[3, 0, 1, 0, 0, 0, 0, 4]);
}
