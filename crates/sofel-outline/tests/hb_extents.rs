//! HarfBuzz's glyph boxes for CFF and CFF2 (ADR 0021, "HarfBuzzの箱").
//! The expected values follow HarfBuzz 15.1.0; `sofel-conformance`
//! checks the same fonts against `hb-dump`.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_font::HbCff2;
use sofel_font::read::GlyphId;
use sofel_font::var::Location;
use sofel_font::{Font, GlyphExtents};
use sofel_outline::{HbExtents, cff2_extents_at};

pub const RMOVETO: u16 = 21;
pub const RLINETO: u16 = 5;
pub const RRCURVETO: u16 = 8;
pub const ENDCHAR: u16 = 14;
pub const CALLSUBR: u16 = 10;
pub const RETURN: u16 = 11;

fn ext(x_bearing: i32, y_bearing: i32, width: i32, height: i32) -> GlyphExtents {
    GlyphExtents {
        x_bearing,
        y_bearing,
        width,
        height,
    }
}

/// A CFF font whose glyphs are `charstrings`, with local subroutines.
fn cff_font(charstrings: Vec<Vec<u8>>, lsubrs: Vec<Vec<u8>>) -> Vec<u8> {
    let n = charstrings.len() as u16;
    let mut s = Cff1Spec::new(charstrings);
    s.lsubrs = lsubrs;
    otto_font(n, vec![(b"CFF ", cff1(&s))])
}

fn boxes(data: &[u8]) -> Vec<Option<GlyphExtents>> {
    let font = Font::new(data, 0).unwrap();
    let e = HbExtents::new(&font);
    (0..font.num_glyphs())
        .map(|g| e.glyph_extents(GlyphId(g)))
        .collect()
}

fn one(items: &[Cs]) -> Option<GlyphExtents> {
    boxes(&cff_font(vec![charstring(items)], vec![]))[0]
}

#[test]
fn control_points_are_in_the_box() {
    use Cs::*;
    // From (10, 20) a curve whose control points reach x 110 and y 220.
    let e = one(&[
        I(10),
        I(20),
        Op(RMOVETO),
        I(100),
        I(200),
        I(0),
        I(0),
        I(-50),
        I(-150),
        Op(RRCURVETO),
        Op(ENDCHAR),
    ]);
    assert_eq!(e, Some(ext(10, 220, 100, -200)));
}

#[test]
fn move_only_paths_are_not_in_the_box() {
    use Cs::*;
    let e = one(&[
        I(-500),
        I(-500),
        Op(RMOVETO),
        I(510),
        I(520),
        Op(RMOVETO),
        I(10),
        I(10),
        Op(RLINETO),
        Op(ENDCHAR),
    ]);
    assert_eq!(e, Some(ext(10, 30, 10, -10)));
    // Only moves: an empty box, both axes 0.
    let e = one(&[I(5), I(5), Op(RMOVETO), Op(ENDCHAR)]);
    assert_eq!(e, Some(ext(0, 0, 0, 0)));
}

#[test]
fn an_empty_axis_is_zero() {
    use Cs::*;
    // A horizontal line: no height.
    let e = one(&[
        I(10),
        I(20),
        Op(RMOVETO),
        I(30),
        I(0),
        Op(RLINETO),
        Op(ENDCHAR),
    ]);
    assert_eq!(e, Some(ext(10, 0, 30, 0)));
}

#[test]
fn edges_round_half_up_on_the_double() {
    use Cs::*;
    // x 300 + 0x7FFF/65536 rounds to 300 (through float it would be
    // 300.5 and then 301); y -0.5 rounds up to 0.
    let e = one(&[
        F((300 << 16) | 0x7FFF),
        F(-0x8000),
        Op(RMOVETO),
        I(10),
        I(10),
        Op(RLINETO),
        Op(ENDCHAR),
    ]);
    assert_eq!(e, Some(ext(300, 10, 10, -10)));
}

#[test]
fn seac_merges_base_and_moved_accent() {
    use Cs::*;
    // The ISOAdobe charset: glyph 34 is "A" (code 65), glyph 1 is
    // "space"... use "A" (65) as base and "grave" (code 193, SID 124) as
    // accent: glyph 124.
    let mut cs = vec![charstring(&[Op(ENDCHAR)]); 125];
    cs[34] = charstring(&[
        I(0),
        I(0),
        Op(RMOVETO),
        I(100),
        I(100),
        Op(RLINETO),
        Op(ENDCHAR),
    ]);
    cs[124] = charstring(&[
        I(0),
        I(0),
        Op(RMOVETO),
        I(10),
        I(10),
        Op(RLINETO),
        Op(ENDCHAR),
    ]);
    cs[1] = charstring(&[I(50), I(200), I(65), I(193), Op(ENDCHAR)]);
    // A seac component that is itself a seac fails.
    cs[2] = charstring(&[I(0), I(0), I(65), I(66), Op(ENDCHAR)]);
    cs[35] = charstring(&[I(0), I(0), I(65), I(193), Op(ENDCHAR)]);
    // A code without a glyph fails.
    cs[3] = charstring(&[I(0), I(0), I(65), I(0), Op(ENDCHAR)]);
    let b = boxes(&cff_font(cs, vec![]));
    assert_eq!(b[1], Some(ext(0, 210, 100, -210)));
    assert_eq!(b[2], None);
    assert_eq!(b[3], None);
}

#[test]
fn subroutine_nesting_ten_and_eleven() {
    use Cs::*;
    // Subroutine k calls k + 1; the last draws. Local bias 107.
    let chain = |depth: usize| {
        let mut subrs: Vec<Vec<u8>> = (0..depth - 1)
            .map(|k| charstring(&[I(k as i32 + 1 - 107), Op(CALLSUBR), Op(RETURN)]))
            .collect();
        subrs.push(charstring(&[I(1), I(1), Op(RLINETO), Op(RETURN)]));
        let g = charstring(&[I(0), I(0), Op(RMOVETO), I(-107), Op(CALLSUBR), Op(ENDCHAR)]);
        boxes(&cff_font(vec![g], subrs))[0]
    };
    assert_eq!(chain(10), Some(ext(0, 1, 1, -1)));
    assert_eq!(chain(11), None);
}

#[test]
fn operator_limit() {
    use Cs::*;
    // Each `I(0) I(0) rlineto` is three operators: n of them plus the
    // move (3) and endchar (1).
    let glyph = |n: usize| {
        let mut items = vec![I(0), I(0), Op(RMOVETO)];
        for _ in 0..n {
            items.extend([I(0), I(0), Op(RLINETO)]);
        }
        items.push(Op(ENDCHAR));
        charstring(&items)
    };
    // 3 + 3 * 66665 + 1 = 199999 operators pass; one more line fails.
    let b = boxes(&cff_font(vec![glyph(66665), glyph(66666)], vec![]));
    assert_eq!(b[0], Some(ext(0, 0, 0, 0)));
    assert_eq!(b[1], None);
}

#[test]
fn a_charstring_without_endchar_fails_in_cff() {
    use Cs::*;
    assert_eq!(
        one(&[I(0), I(0), Op(RMOVETO), I(5), I(5), Op(RLINETO)]),
        None
    );
    // Arithmetic operators are unknown: they clear the stack.
    assert_eq!(
        one(&[I(1), I(2), Op(0x0C0A), I(5), I(5), Op(RLINETO), Op(ENDCHAR)]),
        Some(ext(0, 5, 5, -5))
    );
}

fn cff2_font(spec: &Cff2Spec, extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    let mut tables = vec![(b"CFF2", cff2(spec))];
    tables.extend(extra);
    otto_font(spec.charstrings.len() as u16, tables)
}

#[test]
fn cff2_blend_keeps_the_default() {
    use Cs::*;
    // Two regions: blend takes 1 default and 2 deltas per value.
    let vs = item_variation_store(1, &[vec![(0, 0, 0)], vec![(-1, 1, 1)]], &[vec![0, 1]]);
    let g = charstring(&[
        I(0),
        I(0),
        Op(RMOVETO),
        I(100),
        I(50),
        I(500),
        I(500),
        I(-70),
        I(70),
        I(2),
        Op(16),
        Op(RLINETO),
    ]);
    let spec = Cff2Spec {
        charstrings: vec![g],
        fds: vec![(Vec::new(), Vec::new())],
        vstore: Some(vs),
        ..Default::default()
    };
    let b = boxes(&cff2_font(&spec, vec![]));
    assert_eq!(b[0], Some(ext(0, 50, 100, -50)));
    // Too few operands for the blend fail.
    let g = charstring(&[I(0), I(0), I(5), I(2), Op(16), Op(RLINETO)]);
    let spec = Cff2Spec {
        charstrings: vec![g],
        fds: vec![(Vec::new(), Vec::new())],
        vstore: Some(item_variation_store(1, &[vec![(0, 0, 0)]], &[vec![0]])),
        ..Default::default()
    };
    assert_eq!(boxes(&cff2_font(&spec, vec![]))[0], None);
}

#[test]
fn cff2_ends_without_endchar_and_falls_back_to_cff() {
    use Cs::*;
    let g = charstring(&[I(0), I(0), Op(RMOVETO), I(7), I(9), Op(RLINETO)]);
    let spec = Cff2Spec {
        charstrings: vec![g.clone()],
        fds: vec![(Vec::new(), Vec::new())],
        ..Default::default()
    };
    assert_eq!(boxes(&cff2_font(&spec, vec![]))[0], Some(ext(0, 9, 7, -9)));
    // A CFF2 table HarfBuzz rejects (no FDArray count match: an FDSelect
    // that is broken) leaves the CFF table.
    let bad = Cff2Spec {
        charstrings: vec![g],
        fds: vec![(Vec::new(), Vec::new()), (Vec::new(), Vec::new())],
        fd_select: vec![3, 0, 1, 0, 0, 0, 0, 9],
        ..Default::default()
    };
    let cff = cff1(&Cff1Spec::new(vec![charstring(&[
        I(0),
        I(0),
        Op(RMOVETO),
        I(3),
        I(4),
        Op(RLINETO),
        Op(ENDCHAR),
    ])]));
    let b = boxes(&cff2_font(&bad, vec![(b"CFF ", cff)]));
    assert_eq!(b[0], Some(ext(0, 4, 3, -4)));
}

#[test]
fn adr0027_i04_cff2_blend_changes_harfbuzz_box() {
    use Cs::*;
    let store = item_variation_store(1, &[vec![(0, 0x4000, 0x4000)]], &[vec![0]]);
    let glyph = charstring(&[
        I(0),
        I(0),
        Op(RMOVETO),
        I(100),
        I(50),
        I(1),
        Op(16),
        I(100),
        Op(RLINETO),
    ]);
    let spec = Cff2Spec {
        charstrings: vec![glyph],
        fds: vec![(Vec::new(), Vec::new())],
        vstore: Some(store),
        ..Default::default()
    };
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    let data = cff2_font(&spec, vec![(b"fvar", fvar)]);
    let font = Font::new(&data, 0).unwrap();
    let cff = HbCff2::new(&font).unwrap();
    let default = Location::from_design(&font, &[0.0]);
    let max = Location::from_design(&font, &[1.0]);
    assert_eq!(
        cff2_extents_at(&cff, 0, &font.instance(&default)),
        Some(ext(0, 100, 100, -100))
    );
    assert_eq!(
        cff2_extents_at(&cff, 0, &font.instance(&max)),
        Some(ext(0, 100, 150, -100))
    );
}
