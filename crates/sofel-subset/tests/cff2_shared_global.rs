//! Pinned HarfBuzz 15.1.0 CFF2 shared-global subroutine behavior (root ADR 0102).

#![allow(clippy::all, clippy::unwrap_used, clippy::indexing_slicing)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;
#[path = "support/fixture_input.rs"]
mod fixture_input;

use build::{Cff2Spec, Cs, cff2, charstring, item_variation_store, otto_font};
use sofel_font::Font;
use sofel_font::read::{GlyphId, Severity, Tag};
use sofel_outline::{Outlines, Scale, Scratch};
use sofel_subset::{Flags, Input, SetKind, SubsetError, subset};

fn input_font(retain_fd1_local0: bool) -> Vec<u8> {
    // FD 0 retains only local 1; FD 1 retains local 0 and local 1.
    // A call to global 0 reaches local 1 from both Font DICTs.
    let local_0 = charstring(&[
        Cs::I(10),
        Cs::I(10),
        Cs::Op(21),
        Cs::I(10),
        Cs::I(-10),
        Cs::Op(5),
        Cs::Op(11),
    ]);
    let local_1 = charstring(&[
        Cs::I(0),
        Cs::I(10),
        Cs::Op(21),
        Cs::I(5),
        Cs::I(0),
        Cs::Op(5),
        Cs::Op(11),
    ]);
    let global = charstring(&[Cs::I(-106), Cs::Op(10), Cs::Op(11)]);
    let glyph_1 = charstring(&[Cs::I(-107), Cs::Op(29)]);
    let glyph_2 = if retain_fd1_local0 {
        charstring(&[Cs::I(-107), Cs::Op(10), Cs::I(-107), Cs::Op(29)])
    } else {
        charstring(&[Cs::I(-107), Cs::Op(29)])
    };
    let spec = Cff2Spec {
        gsubrs: vec![global],
        charstrings: vec![vec![], glyph_1, glyph_2],
        fds: vec![
            (vec![], vec![vec![11], local_1.clone()]),
            (vec![], vec![local_0, local_1]),
        ],
        fd_select: vec![0, 0, 0, 1],
        ..Cff2Spec::default()
    };
    otto_font(3, vec![(b"CFF2", cff2(&spec))])
}

fn glyphs(ids: &[u32]) -> Input {
    Input::new().map_set(SetKind::Glyphs, |set| {
        for &id in ids {
            set.add(id);
        }
    })
}

fn contour_starts(font: &Font<'_>, glyph: u32) -> Vec<(i32, i32)> {
    let outlines = Outlines::new(font.default_instance());
    let mut scratch = Scratch::default();
    outlines
        .load(GlyphId(glyph), Scale::Unscaled, &mut scratch)
        .unwrap();
    let starts = core::iter::once(0).chain(
        scratch
            .contour_ends()
            .iter()
            .copied()
            .map(|end| end.saturating_add(1)),
    );
    starts
        .take(scratch.contour_ends().len())
        .map(|at| {
            let point = scratch.points()[at];
            (point.x, point.y)
        })
        .collect()
}

fn cff2_errors(font: &Font<'_>) -> Vec<String> {
    let mut errors = Vec::new();
    sofel_font::read::cff::validate2(
        font.table(Tag::new(b"CFF2")).unwrap(),
        Some(font.num_glyphs()),
        None,
        &mut |d| {
            if d.severity == Severity::Error {
                errors.push(format!("{:?}", d.kind));
            }
        },
    );
    errors
}

#[test]
fn adr0102_i3_i4_shared_global_local_remapping() {
    let bytes = input_font(true);
    fixture_input::export("cff2-shared-global-hb15.otf", &bytes).unwrap();
    let source = Font::new(&bytes, 0).unwrap();
    assert_eq!(contour_starts(&source, 2).get(1), Some(&(20, 10)));
    let reference = Font::new(include_bytes!("data/cff2-shared-global-hb15.otf"), 0).unwrap();
    assert_eq!(contour_starts(&reference, 2).get(1), Some(&(30, 10)));
    assert!(
        cff2_errors(&reference).is_empty(),
        "{:?}",
        cff2_errors(&reference)
    );
    let output = subset(&source, &glyphs(&[1, 2])).unwrap();
    assert_eq!(output, include_bytes!("data/cff2-shared-global-hb15.otf"));
    let result = Font::new(&output, 0).unwrap();
    assert_eq!(contour_starts(&result, 2).get(1), Some(&(30, 10)));
    assert!(cff2_errors(&result).is_empty());
}

#[test]
fn adr0102_i5_invalid_reference_output_is_rejected() {
    let bytes = input_font(true);
    fixture_input::export("cff2-shared-global-hb15.otf", &bytes).unwrap();
    let reference = include_bytes!("data/cff2-shared-global-gid2-hb15.otf");
    let invalid = Font::new(reference, 0).unwrap();
    assert!(!cff2_errors(&invalid).is_empty());
    let source = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        subset(&source, &glyphs(&[2])),
        Err(SubsetError::Malformed {
            tag: Tag::new(b"CFF2")
        })
    );
}

#[test]
fn adr0102_i3_equal_local_remapping_preserves_outline() {
    let bytes = input_font(false);
    fixture_input::export("cff2-shared-global-equal-hb15.otf", &bytes).unwrap();
    let source = Font::new(&bytes, 0).unwrap();
    let output = subset(&source, &glyphs(&[1, 2])).unwrap();
    assert_eq!(
        output,
        include_bytes!("data/cff2-shared-global-equal-hb15.otf")
    );
    let result = Font::new(&output, 0).unwrap();
    assert_eq!(contour_starts(&source, 2), contour_starts(&result, 2));
    assert!(cff2_errors(&result).is_empty());
}

fn fvar() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0];
    for value in [16u16, 2, 1, 20, 0, 10] {
        fvar.extend(value.to_be_bytes());
    }
    fvar.extend(*b"wght");
    for value in [100i32, 400, 900] {
        fvar.extend(value.checked_mul(65536).unwrap().to_be_bytes());
    }
    fvar.extend([0, 0, 1, 0]);
    fvar
}

fn store_font(case: &str) -> Vec<u8> {
    let store = item_variation_store(
        1,
        &[vec![(0, 16384, 16384)], vec![(-16384, -16384, 0)]],
        &[vec![0], vec![1]],
    );
    let blend = |index| {
        charstring(&[
            Cs::I(index),
            Cs::Op(15),
            Cs::I(10),
            Cs::I(5),
            Cs::I(1),
            Cs::Op(16),
            Cs::I(0),
            Cs::Op(21),
        ])
    };
    let charstrings = match case {
        "none" => vec![vec![], charstring(&[Cs::I(0), Cs::I(0), Cs::Op(21)])],
        "partial" => vec![vec![], blend(0)],
        "all" => vec![vec![], blend(0), blend(1)],
        "unused-vsindex" => vec![
            vec![],
            charstring(&[Cs::I(0), Cs::Op(15), Cs::I(0), Cs::I(0), Cs::Op(21)]),
        ],
        _ => unreachable!(),
    };
    let count = u16::try_from(charstrings.len()).unwrap();
    let spec = Cff2Spec {
        charstrings,
        fds: vec![(vec![], vec![])],
        vstore: Some(store),
        ..Cff2Spec::default()
    };
    otto_font(count, vec![(b"CFF2", cff2(&spec)), (b"fvar", fvar())])
}

#[test]
fn adr0102_a1_export_reference_inputs() {
    for (name, bytes) in [
        ("cff2-shared-global-hb15.otf", input_font(true)),
        ("cff2-shared-global-equal-hb15.otf", input_font(false)),
        ("cff2-varstore-none-hb15.otf", store_font("none")),
        ("cff2-varstore-partial-hb15.otf", store_font("partial")),
        ("cff2-varstore-all-hb15.otf", store_font("all")),
        (
            "cff2-varstore-unused-vsindex-hb15.otf",
            store_font("unused-vsindex"),
        ),
    ] {
        assert!(!bytes.is_empty());
        fixture_input::export(name, &bytes).unwrap();
    }
}

#[test]
fn adr0102_i1_unpinned_variation_store() {
    for (case, reference) in [
        (
            "none",
            include_bytes!("data/cff2-varstore-none-hb15.otf").as_slice(),
        ),
        (
            "partial",
            include_bytes!("data/cff2-varstore-partial-hb15.otf").as_slice(),
        ),
        (
            "all",
            include_bytes!("data/cff2-varstore-all-hb15.otf").as_slice(),
        ),
    ] {
        let bytes = store_font(case);
        fixture_input::export(&format!("cff2-varstore-{case}-hb15.otf"), &bytes).unwrap();
        let source = Font::new(&bytes, 0).unwrap();
        let retained = if case == "all" { &[1, 2][..] } else { &[1][..] };
        let output = subset(&source, &glyphs(retained)).unwrap();
        assert_eq!(output, reference, "{case}");
    }
}

#[test]
fn adr0102_i2_vsindex_desubroutinize() {
    for (case, retained) in [("unused-vsindex", &[1][..]), ("partial", &[1][..])] {
        let bytes = store_font(case);
        fixture_input::export(&format!("cff2-varstore-{case}-hb15.otf"), &bytes).unwrap();
        let source = Font::new(&bytes, 0).unwrap();
        for (desubr, reference) in match case {
            "unused-vsindex" => [
                (
                    false,
                    include_bytes!("data/cff2-varstore-unused-vsindex-hb15.otf").as_slice(),
                ),
                (
                    true,
                    include_bytes!("data/cff2-varstore-unused-vsindex-desubr-hb15.otf").as_slice(),
                ),
            ],
            _ => [
                (
                    false,
                    include_bytes!("data/cff2-varstore-partial-hb15.otf").as_slice(),
                ),
                (
                    true,
                    include_bytes!("data/cff2-varstore-partial-desubr-hb15.otf").as_slice(),
                ),
            ],
        } {
            let flags = if desubr {
                Flags::DESUBROUTINIZE
            } else {
                Flags::DEFAULT
            };
            let output = subset(&source, &glyphs(retained).with_flags(flags)).unwrap();
            assert_eq!(output, reference, "{case} desubr={desubr}");
        }
    }
}
