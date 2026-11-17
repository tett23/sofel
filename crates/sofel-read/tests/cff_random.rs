//! The interval check of CFF glyphs that run `random` (ADR 0082): the
//! three diagnostics, the order of the branches, the execution limit and
//! the determinism of the reports.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

mod support;

use sofel_read::cff::dict::op;
use sofel_read::{CffRandomReason, Diagnostic, Kind, Severity};
use support::build::*;

use Cs::{F, I, Op};

const RANDOM: u16 = 0x0C17;
const MUL: u16 = 0x0C18;
const DIV: u16 = 0x0C0C;
const SUB: u16 = 0x0C0B;
const SQRT: u16 = 0x0C1A;
const DROP: u16 = 0x0C12;
const IFELSE: u16 = 0x0C16;
const NOT: u16 = 0x0C05;
const EQ: u16 = 0x0C0F;
const INDEX: u16 = 0x0C1D;
const ROLL: u16 = 0x0C1E;
const PUT: u16 = 0x0C14;
const GET: u16 = 0x0C15;
const CALLSUBR: u16 = 10;
const RETURN: u16 = 11;
const ENDCHAR: u16 = 14;
const RMOVETO: u16 = 21;
const HALF: i32 = 0x8000;

/// A CFF table with `glyph` as glyph 1, `subrs` local subroutines that
/// only return, and `initialRandomSeed` `seed` (`None` for the default).
fn table(glyph: &[Cs], subrs: usize, seed: Option<i32>) -> Vec<u8> {
    table_with(glyph, vec![charstring(&[Op(RETURN)]); subrs], seed)
}

fn table_with(glyph: &[Cs], subrs: Vec<Vec<u8>>, seed: Option<i32>) -> Vec<u8> {
    let mut spec = Cff1Spec::new(vec![charstring(&[Op(ENDCHAR)]), charstring(glyph)]);
    spec.lsubrs = subrs;
    if let Some(s) = seed {
        spec.private = Some(dict(&[(&[s], op::INITIAL_RANDOM_SEED)]));
    }
    cff1(&spec)
}

fn validate(t: &[u8]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    sofel_read::cff::validate(t, None, &mut |d| out.push(d));
    out
}

/// The diagnostics of the interval check.
fn random_kinds(t: &[u8]) -> Vec<Kind> {
    validate(t)
        .into_iter()
        .map(|d| d.kind)
        .filter(|k| k.id().starts_with("cff_random_"))
        .collect()
}

fn reason(t: &[u8]) -> Option<CffRandomReason> {
    random_kinds(t).into_iter().find_map(|k| match k {
        Kind::CffRandomUnchecked { reason, .. } => Some(reason),
        _ => None,
    })
}

#[test]
fn adr0082_i5_glyph_without_random_is_not_checked() {
    // A glyph without random keeps only the seed path's diagnostics.
    let t = table(&[I(5), Op(CALLSUBR), Op(ENDCHAR)], 0, None);
    let d = validate(&t);
    assert_eq!(
        d.iter().map(|d| d.kind.id()).collect::<Vec<_>>(),
        vec!["subroutine_out_of_range"]
    );
}

#[test]
fn adr0082_i5_random_once_and_dropped_has_no_diagnostics() {
    let t = table(&[Op(RANDOM), Op(DROP), Op(ENDCHAR)], 0, None);
    assert_eq!(validate(&t), vec![]);
}

#[test]
fn adr0082_i4_random_coordinates_have_no_diagnostics() {
    let t = table(
        &[
            Op(RANDOM),
            I(1000),
            Op(MUL),
            Op(RANDOM),
            I(1000),
            Op(MUL),
            Op(RMOVETO),
            Op(ENDCHAR),
        ],
        0,
        None,
    );
    assert_eq!(validate(&t), vec![]);
}

/// `0 1 random 0.5 ifelse not callsubr`: number 1 (index 108) when the
/// value is at most 0.5, number 0 (index 107) otherwise. With 108 local
/// subroutines only the first branch leaves the INDEX.
fn one_sided_call() -> Vec<Cs> {
    vec![
        I(0),
        I(1),
        Op(RANDOM),
        F(HALF),
        Op(IFELSE),
        Op(NOT),
        Op(CALLSUBR),
        Op(ENDCHAR),
    ]
}

#[test]
fn adr0082_i7_one_sided_out_of_range_call_whatever_the_seed() {
    // Seed 1 draws 2/65536 (the out-of-range branch), seed 0xFFFF draws
    // 1.0 (the valid branch); both report the error.
    for seed in [1, 0xFFFF, 0, -1, i32::MAX] {
        let t = table(&one_sided_call(), 108, Some(seed));
        let d: Vec<Diagnostic> = validate(&t)
            .into_iter()
            .filter(|d| d.kind.id().starts_with("cff_random_"))
            .collect();
        assert_eq!(d.len(), 1, "seed {seed}: {d:?}");
        assert_eq!(
            d[0].kind,
            Kind::CffRandomSubroutineOutOfRange {
                glyph: 1,
                position: 13
            },
            "seed {seed}"
        );
        assert_eq!(d[0].severity, Severity::Error);
    }
    // With 109 subroutines both branches are valid.
    assert_eq!(
        random_kinds(&table(&one_sided_call(), 109, Some(1))),
        vec![]
    );
}

#[test]
fn adr0082_i7_seed_path_reports_as_before() {
    // The seed path of seed 1 takes the out-of-range branch and reports it
    // as it did before the interval check; seed 0xFFFF does not.
    let ids = |seed| {
        validate(&table(&one_sided_call(), 108, Some(seed)))
            .iter()
            .map(|d| d.kind.id())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids(1),
        vec![
            "subroutine_out_of_range",
            "cff_random_subroutine_out_of_range"
        ]
    );
    assert_eq!(ids(0xFFFF), vec!["cff_random_subroutine_out_of_range"]);
}

#[test]
fn adr0082_i4_divisor_range_with_zero() {
    // 1 / (random - 0.5): the divisor range (-0.5, 0.5] contains 0.
    let t = table(
        &[
            I(1),
            Op(RANDOM),
            F(HALF),
            Op(SUB),
            Op(DIV),
            Op(DROP),
            Op(ENDCHAR),
        ],
        0,
        None,
    );
    assert_eq!(reason(&t), Some(CffRandomReason::DivisionByZero));
}

#[test]
fn adr0082_i4_divisor_open_at_zero_gives_an_infinite_end() {
    // 1 / (0, 1] is [1, inf): no diagnostic until the end is used.
    let reciprocal = [I(1), Op(RANDOM), Op(DIV)];
    let mut dropped = reciprocal.to_vec();
    dropped.extend([Op(DROP), Op(ENDCHAR)]);
    assert_eq!(random_kinds(&table(&dropped, 0, None)), vec![]);
    let mut used = reciprocal.to_vec();
    used.extend([I(2), Op(MUL), Op(DROP), Op(ENDCHAR)]);
    assert_eq!(
        random_kinds(&table(&used, 0, None)),
        vec![Kind::CffRandomUnchecked {
            glyph: 1,
            position: 6,
            reason: CffRandomReason::InfiniteBound
        }]
    );
}

#[test]
fn adr0082_i4_infinite_number_is_out_of_range() {
    // An infinite subroutine number is outside every INDEX.
    let t = table(
        &[I(1), Op(RANDOM), Op(DIV), Op(CALLSUBR), Op(ENDCHAR)],
        200,
        None,
    );
    assert!(matches!(
        random_kinds(&t)[..],
        [Kind::CffRandomSubroutineOutOfRange { .. }]
    ));
}

#[test]
fn adr0082_i4_negative_sqrt() {
    let t = table(
        &[
            Op(RANDOM),
            F(HALF),
            Op(SUB),
            Op(SQRT),
            Op(DROP),
            Op(ENDCHAR),
        ],
        0,
        None,
    );
    assert_eq!(reason(&t), Some(CffRandomReason::NegativeSqrt));
    // sqrt of (0, 1] is fine.
    let t = table(&[Op(RANDOM), Op(SQRT), Op(DROP), Op(ENDCHAR)], 0, None);
    assert_eq!(random_kinds(&t), vec![]);
}

#[test]
fn adr0082_i4_index_and_roll_positions() {
    // index with (0, 4] on a stack of 3.
    let t = table(
        &[
            I(1),
            I(2),
            I(3),
            Op(RANDOM),
            I(4),
            Op(MUL),
            Op(INDEX),
            Op(ENDCHAR),
        ],
        0,
        None,
    );
    assert_eq!(reason(&t), Some(CffRandomReason::StackPosition));
    // roll with a count of (-1, 0].
    let t = table(
        &[
            I(1),
            I(2),
            Op(RANDOM),
            I(1),
            Op(SUB),
            I(1),
            Op(ROLL),
            Op(ENDCHAR),
        ],
        0,
        None,
    );
    assert_eq!(reason(&t), Some(CffRandomReason::StackPosition));
}

#[test]
fn adr0082_i4_random_operand_rejected() {
    // random callsubr: numbers 0 and 1 are inside 110 subroutines, but
    // the 16.16 number cannot be taken by callsubr.
    let t = table(&[Op(RANDOM), Op(CALLSUBR), Op(ENDCHAR)], 110, None);
    let d: Vec<Diagnostic> = validate(&t)
        .into_iter()
        .filter(|d| d.kind.id().starts_with("cff_random_"))
        .collect();
    assert_eq!(
        d.iter().map(|d| d.kind).collect::<Vec<_>>(),
        vec![Kind::CffRandomOperandRejected {
            glyph: 1,
            position: 2,
            operator: 10
        }]
    );
    assert_eq!(d[0].severity, Severity::Warning);
    // So do put and get with 16.16 positions.
    for glyph in [
        vec![I(7), Op(RANDOM), Op(PUT), Op(ENDCHAR)],
        vec![Op(RANDOM), Op(GET), Op(DROP), Op(ENDCHAR)],
    ] {
        assert!(matches!(
            random_kinds(&table(&glyph, 0, None))[..],
            [Kind::CffRandomOperandRejected { .. }]
        ));
    }
}

#[test]
fn adr0082_i4_comparison_branches_are_integers() {
    // eq gives integers: random 0.5 eq callsubr calls numbers 0 and 1,
    // both inside 110 subroutines, so nothing is reported.
    let t = table(
        &[Op(RANDOM), F(HALF), Op(EQ), Op(CALLSUBR), Op(ENDCHAR)],
        110,
        None,
    );
    assert_eq!(random_kinds(&t), vec![]);
    // With 108 the branch of 1 leaves the INDEX.
    let t = table(
        &[Op(RANDOM), F(HALF), Op(EQ), Op(CALLSUBR), Op(ENDCHAR)],
        108,
        None,
    );
    assert!(matches!(
        random_kinds(&t)[..],
        [Kind::CffRandomSubroutineOutOfRange { .. }]
    ));
}

/// `random drop`, `pairs` times `0 drop`, then `tail`.
fn long_glyph(pairs: usize, tail: &[Cs]) -> Vec<Cs> {
    let mut g = vec![Op(RANDOM), Op(DROP)];
    for _ in 0..pairs {
        g.extend([I(0), Op(DROP)]);
    }
    g.extend(tail.iter().cloned());
    g
}

#[test]
fn adr0082_i4_execution_limit() {
    // 2 + 2 * 49998 + 2 = 100000 numbers and operators.
    let t = table(&long_glyph(49_998, &[I(0), Op(ENDCHAR)]), 0, None);
    assert_eq!(random_kinds(&t), vec![]);
    // 2 + 2 * 49999 + 1 = 100001.
    let t = table(&long_glyph(49_999, &[Op(ENDCHAR)]), 0, None);
    assert_eq!(reason(&t), Some(CffRandomReason::Limit));
}

#[test]
fn adr0082_i4_limit_counts_every_branch() {
    // 17 ifelses of two random values double the branches each time:
    // 2^17 paths exceed the limit.
    let mut g = Vec::new();
    for _ in 0..17 {
        g.extend([I(1), I(2), Op(RANDOM), Op(RANDOM), Op(IFELSE), Op(DROP)]);
    }
    g.push(Op(ENDCHAR));
    assert_eq!(reason(&table(&g, 0, None)), Some(CffRandomReason::Limit));
    // 3 of them stay well below it.
    let mut g = Vec::new();
    for _ in 0..3 {
        g.extend([I(1), I(2), Op(RANDOM), Op(RANDOM), Op(IFELSE), Op(DROP)]);
    }
    g.push(Op(ENDCHAR));
    assert_eq!(random_kinds(&table(&g, 0, None)), vec![]);
}

#[test]
fn adr0082_i4_only_the_first_finding() {
    // A negative sqrt range, then a divisor range with 0: one report.
    let t = table(
        &[
            Op(RANDOM),
            F(HALF),
            Op(SUB),
            Op(SQRT),
            Op(DROP),
            I(1),
            Op(RANDOM),
            F(HALF),
            Op(SUB),
            Op(DIV),
            Op(DROP),
            Op(ENDCHAR),
        ],
        0,
        None,
    );
    assert_eq!(
        random_kinds(&t),
        vec![Kind::CffRandomUnchecked {
            glyph: 1,
            position: 9,
            reason: CffRandomReason::NegativeSqrt
        }]
    );
}

/// 107 subroutines that return, then subroutine 107 (number 0) that
/// takes the square root of random - 0.5, then `extra` more.
fn sqrt_subrs(extra: usize) -> Vec<Vec<u8>> {
    let mut subrs = vec![charstring(&[Op(RETURN)]); 107];
    subrs.push(charstring(&[
        Op(RANDOM),
        F(HALF),
        Op(SUB),
        Op(SQRT),
        Op(DROP),
        Op(RETURN),
    ]));
    subrs.extend(vec![charstring(&[Op(RETURN)]); extra]);
    subrs
}

#[test]
fn adr0082_i4_ifelse_takes_the_first_argument_first() {
    // The first argument's branch calls number 1 (out of range with 108
    // subroutines); the second calls number 0, whose sqrt range is
    // negative. The first argument's branch is reported.
    let t = table_with(&one_sided_call(), sqrt_subrs(0), None);
    assert_eq!(
        random_kinds(&t),
        vec![Kind::CffRandomSubroutineOutOfRange {
            glyph: 1,
            position: 13
        }]
    );
    // With 109 subroutines the first branch is valid and the second's
    // finding, inside the subroutine, is reported at its position there.
    let t = table_with(&one_sided_call(), sqrt_subrs(1), None);
    let d: Vec<Diagnostic> = validate(&t)
        .into_iter()
        .filter(|d| d.kind.id().starts_with("cff_random_"))
        .collect();
    assert_eq!(
        d.iter().map(|d| d.kind).collect::<Vec<_>>(),
        vec![Kind::CffRandomUnchecked {
            glyph: 1,
            position: 9,
            reason: CffRandomReason::NegativeSqrt
        }]
    );
}

#[test]
fn adr0082_i4_reports_are_deterministic() {
    let tables = [
        table(&one_sided_call(), 108, Some(1)),
        table(&long_glyph(49_999, &[Op(ENDCHAR)]), 0, None),
        table(&[Op(RANDOM), Op(CALLSUBR), Op(ENDCHAR)], 110, None),
    ];
    for t in &tables {
        let first = validate(t);
        for _ in 0..3 {
            assert_eq!(validate(t), first);
        }
    }
}

#[test]
fn adr0082_cff2_is_not_checked() {
    // CFF2 has no random: the operator is reserved there.
    let t = cff2(&Cff2Spec {
        charstrings: vec![Vec::new(), charstring(&[I(1), Op(RANDOM), Op(CALLSUBR)])],
        fds: vec![(Vec::new(), Vec::new())],
        ..Default::default()
    });
    let mut out = Vec::new();
    sofel_read::cff::validate2(&t, None, None, &mut |d| out.push(d));
    assert!(out.iter().all(|d| !d.kind.id().starts_with("cff_random_")));
}

#[test]
fn adr0082_kind_details_and_messages() {
    let k = Kind::CffRandomUnchecked {
        glyph: 3,
        position: 9,
        reason: CffRandomReason::Limit,
    };
    let mut details = Vec::new();
    k.details(&mut |name, v| details.push((name, v)));
    assert_eq!(
        details,
        vec![
            ("glyph", sofel_read::DetailValue::Uint(3)),
            ("position", sofel_read::DetailValue::Uint(9)),
            ("reason", sofel_read::DetailValue::Uint(4)),
        ]
    );
    assert_eq!(k.id(), "cff_random_unchecked");
    assert!(k.to_string().starts_with("glyph 3:"));
    let codes: Vec<u32> = [
        CffRandomReason::DivisionByZero,
        CffRandomReason::NegativeSqrt,
        CffRandomReason::StackPosition,
        CffRandomReason::InfiniteBound,
        CffRandomReason::Limit,
    ]
    .iter()
    .map(|r| r.code())
    .collect();
    assert_eq!(codes, vec![0, 1, 2, 3, 4]);
}
