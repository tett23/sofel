//! The GDEF, GSUB and GPOS validators (ADR 0020, ADR 0007): each problem
//! with its kind, severity and offset.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

mod support;
use support::build::*;

use sofel_read::layout::LayoutTable;
use sofel_read::sfnt::Sfnt;
use sofel_read::{Diagnostic, Kind, Severity, Tag, gdef, gpos, gsub, validate};

fn gsub_diags(d: &[u8], mark_sets: Option<u16>) -> Vec<Diagnostic> {
    let mut v = Vec::new();
    gsub::validate(d, mark_sets, &mut |x| v.push(x));
    v
}

fn gpos_diags(d: &[u8]) -> Vec<Diagnostic> {
    let mut v = Vec::new();
    gpos::validate(d, None, &mut |x| v.push(x));
    v
}

fn gdef_diags(d: &[u8]) -> Vec<Diagnostic> {
    let mut v = Vec::new();
    gdef::validate(d, &mut |x| v.push(x));
    v
}

/// Asserts that `diags` has exactly one diagnostic of `kind`, at `offset`
/// with `severity`.
#[track_caller]
fn has(diags: &[Diagnostic], offset: usize, kind: Kind, severity: Severity) {
    let found: Vec<&Diagnostic> = diags.iter().filter(|d| d.kind == kind).collect();
    assert_eq!(found.len(), 1, "{kind:?} in {diags:#?}");
    assert_eq!(found[0].offset as usize, offset, "{kind:?} in {diags:#?}");
    assert_eq!(found[0].severity, severity);
    assert_eq!(kind.severity(), severity);
}

#[track_caller]
fn clean(diags: &[Diagnostic]) {
    assert!(diags.is_empty(), "{diags:#?}");
}

/// The position of the lookup `i` and of its first subtable.
fn positions(d: &[u8], i: usize) -> (usize, usize) {
    let t = LayoutTable::new(d).unwrap();
    let list = t.lookup_list_offset() as usize;
    let l = list + u16::from_be_bytes([d[list + 2 + 2 * i], d[list + 3 + 2 * i]]) as usize;
    let s = l + u16::from_be_bytes([d[l + 6], d[l + 7]]) as usize;
    (l, s)
}

fn single1(coverage: Vec<u8>) -> Vec<u8> {
    link(
        Buf::new().u16(1).u16(0).u16(1).done(),
        vec![(2, 2, coverage)],
    )
}

fn single2(coverage: Vec<u8>, substitutes: &[u16]) -> Vec<u8> {
    let mut b = Buf::new().u16(2).u16(0).u16(substitutes.len() as u16);
    for s in substitutes {
        b = b.u16(*s);
    }
    link(b.done(), vec![(2, 2, coverage)])
}

#[test]
fn a_valid_table_is_clean() {
    let d = simple_layout(vec![lookup(
        1,
        0,
        vec![single2(coverage1(&[1, 2]), &[3, 4])],
        None,
    )]);
    clean(&gsub_diags(&d, Some(0)));
    for version in [0x0001_0001, 0x0001_0002] {
        let d = gsubgpos(
            version,
            script_list(vec![(b"DFLT", script(lang_sys(0xFFFF, &[0]), vec![]))]),
            feature_list(&[(b"test", &[0])]),
            lookup_list(
                vec![lookup(1, 0, vec![single1(coverage1(&[1]))], None)],
                version >= 0x0001_0002,
            ),
        );
        clean(&gsub_diags(&d, None));
        clean(&gpos_diags(&d));
    }
}

#[test]
fn header_problems() {
    let e = Severity::Error;
    has(
        &gsub_diags(&[0, 1, 0, 0, 0, 0], None),
        0,
        Kind::Truncated {
            needed: 10,
            available: 6,
        },
        e,
    );
    has(
        &gsub_diags(&[0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], None),
        0,
        Kind::Truncated {
            needed: 26,
            available: 14,
        },
        e,
    );
    has(
        &gsub_diags(&[0, 3, 0, 0, 0, 0, 0, 0, 0, 0], None),
        0,
        Kind::UnknownVersion { found: 0x0003_0000 },
        e,
    );
    let d = Buf::new().u32(0x0001_0000).u16(0x7000).u16(0).u16(0).done();
    has(
        &gsub_diags(&d, None),
        4,
        Kind::OffsetOutOfBounds {
            offset: 0x7000,
            length: 0,
            available: 10,
        },
        e,
    );
    let d = Buf::new()
        .u32(0x0001_0001)
        .u16(0)
        .u16(0)
        .u16(0)
        .u32(0x100)
        .done();
    has(
        &gsub_diags(&d, None),
        10,
        Kind::OffsetOutOfBounds {
            offset: 0x100,
            length: 0,
            available: 14,
        },
        e,
    );
}

#[test]
fn list_problems() {
    let e = Severity::Error;
    // Scripts out of tag order, feature indices past the features,
    // lookup indices past the lookups.
    let scripts = script_list(vec![
        (b"latn", script(lang_sys(5, &[0, 3]), vec![])),
        (
            b"DFLT",
            script(
                vec![],
                vec![(b"TRK ", vec![]), (b"DEU ", lang_sys(0xFFFF, &[]))],
            ),
        ),
    ]);
    let d = gsubgpos(
        0x0001_0000,
        scripts,
        feature_list(&[(b"test", &[0, 4])]),
        lookup_list(
            vec![lookup(1, 0, vec![single1(coverage1(&[1]))], None)],
            false,
        ),
    );
    let diags = gsub_diags(&d, None);
    // ScriptList at 10: count, two records.
    let list = 10;
    let latn = list + u16::from_be_bytes([d[list + 6], d[list + 7]]) as usize;
    let ls = latn + u16::from_be_bytes([d[latn], d[latn + 1]]) as usize;
    has(
        &diags,
        ls + 2,
        Kind::FeatureIndexOutOfRange { index: 5, count: 1 },
        e,
    );
    let dflt = list + u16::from_be_bytes([d[list + 12], d[list + 13]]) as usize;
    let tags: Vec<&Diagnostic> = diags
        .iter()
        .filter(|x| matches!(x.kind, Kind::TagsNotSorted { .. }))
        .collect();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].offset as usize, 10 + 2 + 6);
    assert_eq!(tags[1].offset as usize, dflt + 4 + 6);
    assert_eq!(tags[0].severity, e);
    let index = diags
        .iter()
        .find(|x| x.kind == Kind::FeatureIndexOutOfRange { index: 3, count: 1 })
        .unwrap();
    assert_eq!(index.offset as usize, ls + 6 + 2);
    let features = u16::from_be_bytes([d[6], d[7]]) as usize;
    let feature = features + u16::from_be_bytes([d[features + 6], d[features + 7]]) as usize;
    has(
        &diags,
        feature + 4 + 2,
        Kind::LookupIndexOutOfRange { index: 4, count: 1 },
        e,
    );
}

#[test]
fn lookup_problems() {
    let e = Severity::Error;
    // An unknown type, reserved flags, a mark filtering set past GDEF's.
    let d = simple_layout(vec![
        lookup(9, 0, vec![single1(coverage1(&[1]))], None),
        lookup(1, 0x0020 | 0x0010, vec![single1(coverage1(&[1]))], Some(3)),
    ]);
    let diags = gsub_diags(&d, Some(2));
    let (l0, _) = positions(&d, 0);
    let (l1, _) = positions(&d, 1);
    has(&diags, l0, Kind::UnknownLookupType { found: 9 }, e);
    has(
        &diags,
        l1 + 2,
        Kind::ReservedBitsSet { found: 0x0030 },
        Severity::Warning,
    );
    has(
        &diags,
        l1 + 8,
        Kind::MarkSetOutOfRange { index: 3, count: 2 },
        e,
    );
    assert!(
        gsub_diags(&d, None)
            .iter()
            .all(|x| !matches!(x.kind, Kind::MarkSetOutOfRange { .. }))
    );
    assert!(
        gsub_diags(&d, Some(4))
            .iter()
            .all(|x| !matches!(x.kind, Kind::MarkSetOutOfRange { .. }))
    );

    // Subtable offsets past the data and unknown formats.
    let mut d = simple_layout(vec![lookup(1, 0, vec![Buf::new().u16(9).done()], None)]);
    let (l, s) = positions(&d, 0);
    has(
        &gsub_diags(&d, None),
        l + 6,
        Kind::UnknownFormat { found: 9 },
        e,
    );
    let len = d.len();
    d[l + 6..l + 8].copy_from_slice(&((len - l) as u16).to_be_bytes());
    has(
        &gsub_diags(&d, None),
        l + 6,
        Kind::OffsetOutOfBounds {
            offset: len as u32,
            length: 0,
            available: len as u32,
        },
        e,
    );
    let _ = s;
}

#[test]
fn extension_problems() {
    let e = Severity::Error;
    let d = simple_layout(vec![lookup(
        7,
        0,
        vec![
            extension(1, single1(coverage1(&[1]))),
            extension(
                2,
                link(
                    Buf::new().u16(1).u16(0).u16(0).done(),
                    vec![(2, 2, coverage1(&[]))],
                ),
            ),
            extension(7, extension(1, single1(coverage1(&[1])))),
            Buf::new().u16(2).u16(1).u32(0).done(),
        ],
        None,
    )]);
    let diags = gsub_diags(&d, None);
    let (l, _) = positions(&d, 0);
    let sub = |i: usize| l + u16::from_be_bytes([d[l + 6 + 2 * i], d[l + 7 + 2 * i]]) as usize;
    has(
        &diags,
        sub(1) + 2,
        Kind::ExtensionTypeMismatch {
            expected: 1,
            found: 2,
        },
        e,
    );
    has(&diags, sub(2) + 2, Kind::NestedExtension, e);
    has(&diags, sub(3), Kind::UnknownFormat { found: 2 }, e);
}

#[test]
fn coverage_problems() {
    let e = Severity::Error;
    let cases: Vec<(Vec<u8>, usize, Kind, Severity)> = vec![
        (
            coverage1(&[5, 2]),
            6,
            Kind::CoverageNotSorted { index: 1 },
            e,
        ),
        (
            coverage1(&[2, 2]),
            6,
            Kind::CoverageDuplicate { index: 1, glyph: 2 },
            Severity::Warning,
        ),
        (
            coverage2(&[(1, 3, 0), (3, 5, 3)]),
            10,
            Kind::CoverageNotSorted { index: 1 },
            e,
        ),
        (
            coverage2(&[(4, 3, 0)]),
            4,
            Kind::StartAfterEnd {
                index: 0,
                start: 4,
                end: 3,
            },
            e,
        ),
        (
            coverage2(&[(1, 3, 0), (5, 5, 2)]),
            14,
            Kind::CoverageIndexMismatch {
                index: 1,
                expected: 3,
                found: 2,
            },
            e,
        ),
        (
            coverage3(&[0x1_0001, 0x1_0000]),
            8,
            Kind::CoverageNotSorted { index: 1 },
            e,
        ),
        (
            coverage4(&[(0x1_0000, 0x1_0001, 1)]),
            11,
            Kind::CoverageIndexMismatch {
                index: 0,
                expected: 0,
                found: 1,
            },
            e,
        ),
        (vec![0, 9], 0, Kind::UnknownFormat { found: 9 }, e),
    ];
    for (coverage, at, kind, severity) in cases {
        let d = simple_layout(vec![lookup(1, 0, vec![single1(coverage)], None)]);
        let (_, s) = positions(&d, 0);
        // An unknown format is reported at the offset field.
        let base = if matches!(kind, Kind::UnknownFormat { .. }) {
            s + 2
        } else {
            s + 6
        };
        has(&gsub_diags(&d, None), base + at, kind, severity);
    }
}

#[test]
fn count_and_context_problems() {
    let e = Severity::Error;
    let d = simple_layout(vec![lookup(
        1,
        0,
        vec![single2(coverage1(&[1, 2]), &[3])],
        None,
    )]);
    let (_, s) = positions(&d, 0);
    has(
        &gsub_diags(&d, None),
        s + 4,
        Kind::CoverageCountMismatch {
            coverage: 2,
            count: 1,
        },
        e,
    );

    // Context format 2: overlapping classes, a rule with a lookup record
    // past the input and a lookup index past the lookups.
    let rule = Buf::new().u16(2).u16(1).u16(1).u16(2).u16(5).done();
    let set = link(Buf::new().u16(1).u16(0).done(), vec![(2, 2, rule)]);
    let ctx = link(
        Buf::new().u16(2).u16(0).u16(0).u16(1).u16(0).done(),
        vec![
            (2, 2, coverage1(&[1])),
            (4, 2, class_def2(&[(1, 5, 1), (5, 6, 2)])),
            (8, 2, set),
        ],
    );
    let d = simple_layout(vec![lookup(5, 0, vec![ctx], None)]);
    let (_, s) = positions(&d, 0);
    let diags = gsub_diags(&d, None);
    let cd = s + u16::from_be_bytes([d[s + 4], d[s + 5]]) as usize;
    has(&diags, cd + 4 + 6, Kind::ClassRangesOverlap { index: 1 }, e);
    let set = s + u16::from_be_bytes([d[s + 8], d[s + 9]]) as usize;
    let rule = set + u16::from_be_bytes([d[set + 2], d[set + 3]]) as usize;
    has(
        &diags,
        rule + 6,
        Kind::SequenceIndexOutOfRange { index: 2, count: 2 },
        e,
    );
    has(
        &diags,
        rule + 8,
        Kind::LookupIndexOutOfRange { index: 5, count: 1 },
        e,
    );

    // No input glyphs: context format 3, chained format 3, a ligature.
    let d = simple_layout(vec![lookup(
        5,
        0,
        vec![Buf::new().u16(3).u16(0).u16(0).done()],
        None,
    )]);
    let (_, s) = positions(&d, 0);
    has(&gsub_diags(&d, None), s + 2, Kind::EmptyInputSequence, e);
    let chain = Buf::new().u16(3).u16(1).u16(0).u16(0).u16(0).u16(0).done();
    let d = simple_layout(vec![lookup(6, 0, vec![chain], None)]);
    let (_, s) = positions(&d, 0);
    has(&gsub_diags(&d, None), s + 6, Kind::EmptyInputSequence, e);
    let lig = Buf::new().u16(9).u16(0).done();
    let set = link(Buf::new().u16(1).u16(0).done(), vec![(2, 2, lig)]);
    let sub = link(
        Buf::new().u16(1).u16(0).u16(1).u16(0).done(),
        vec![(2, 2, coverage1(&[1])), (6, 2, set)],
    );
    let d = simple_layout(vec![lookup(4, 0, vec![sub], None)]);
    let diags = gsub_diags(&d, None);
    assert!(
        diags.iter().any(|x| x.kind == Kind::EmptyInputSequence),
        "{diags:#?}"
    );
}

#[test]
fn work_is_bounded() {
    // 2000 ligature sets that all point to one set of 2000 offsets to
    // one ligature: four million ligatures to follow in 8 KB.
    let n = 2000usize;
    let lig = Buf::new().u16(9).u16(1).done();
    let mut set = Buf::new().u16(n as u16);
    let set_len = 2 + 2 * n;
    for _ in 0..n {
        set = set.u16(set_len as u16);
    }
    let set = set.bytes(&lig).done();
    let header = 6 + 2 * n;
    let mut sub = Buf::new().u16(1).u16(0).u16(n as u16);
    for _ in 0..n {
        sub = sub.u16(header as u16);
    }
    let sub = link(
        sub.bytes(&set).done(),
        vec![(2, 2, coverage2(&[(0, n as u16 - 1, 0)]))],
    );
    let d = simple_layout(vec![lookup(4, 0, vec![sub], None)]);
    let diags = gsub_diags(&d, None);
    has(&diags, 0, Kind::WorkLimitExceeded, Severity::Error);
}

#[test]
fn gpos_problems() {
    let e = Severity::Error;
    // Reserved ValueFormat bits; a PairSet out of order; an unknown
    // anchor format; a Device table that does not fit.
    let set = Buf::new().u16(2).u16(9).i16(1).u16(8).i16(2).done();
    let pair = link(
        Buf::new()
            .u16(1)
            .u16(0)
            .u16(0x1004)
            .u16(0)
            .u16(1)
            .u16(0)
            .done(),
        vec![(2, 2, coverage1(&[1])), (10, 2, set)],
    );
    let anchor = Buf::new().u16(7).u16(0).u16(0).done();
    let cursive = link(
        Buf::new().u16(1).u16(0).u16(1).u16(0).u16(0).done(),
        vec![(2, 2, coverage1(&[1])), (6, 2, anchor)],
    );
    let single = Buf::new().u16(1).u16(0).u16(0x0010).u16(0x7000).done();
    let single = link(single, vec![(2, 2, coverage1(&[1]))]);
    let d = simple_layout(vec![
        lookup(2, 0, vec![pair], None),
        lookup(3, 0, vec![cursive], None),
        lookup(1, 0, vec![single], None),
    ]);
    let diags = gpos_diags(&d);
    let (_, p) = positions(&d, 0);
    has(
        &diags,
        p + 4,
        Kind::ReservedBitsSet { found: 0x1004 },
        Severity::Warning,
    );
    let set_at = p + u16::from_be_bytes([d[p + 10], d[p + 11]]) as usize;
    has(
        &diags,
        set_at + 2 + 4,
        Kind::GlyphsNotSorted { index: 1 },
        e,
    );
    let (_, c) = positions(&d, 1);
    has(&diags, c + 6, Kind::UnknownFormat { found: 7 }, e);
    let (_, s) = positions(&d, 2);
    let len = d.len() as u32;
    has(
        &diags,
        s + 6,
        Kind::OffsetOutOfBounds {
            offset: (s + 0x7000) as u32,
            length: 0,
            available: len,
        },
        e,
    );
}

#[test]
fn gdef_problems() {
    let e = Severity::Error;
    has(
        &gdef_diags(&[0, 1, 0, 0, 0, 0]),
        0,
        Kind::Truncated {
            needed: 12,
            available: 6,
        },
        e,
    );
    has(
        &gdef_diags(&[0, 1, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0]),
        0,
        Kind::Truncated {
            needed: 18,
            available: 12,
        },
        e,
    );
    has(
        &gdef_diags(&[0, 2, 0, 0]),
        0,
        Kind::UnknownVersion { found: 0x0002_0000 },
        e,
    );

    let attach = link(
        Buf::new().u16(0).u16(2).u16(0).u16(0).done(),
        vec![(0, 2, coverage1(&[7]))],
    );
    let carets = link(
        Buf::new().u16(1).u16(0).done(),
        vec![(2, 2, Buf::new().u16(5).u16(0).done())],
    );
    let lig = link(
        Buf::new().u16(0).u16(1).u16(0).done(),
        vec![(0, 2, coverage1(&[8])), (4, 2, carets)],
    );
    let d = link(
        Buf::new()
            .u32(0x0001_0003)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u32(0x7000)
            .done(),
        vec![
            (4, 2, class_def2(&[(1, 5, 1), (3, 9, 2)])),
            (6, 2, attach),
            (8, 2, lig),
            (12, 2, Buf::new().u16(2).u16(0).done()),
        ],
    );
    let diags = gdef_diags(&d);
    let at = |field: usize| u16::from_be_bytes([d[field], d[field + 1]]) as usize;
    has(&diags, at(4) + 10, Kind::ClassRangesOverlap { index: 1 }, e);
    has(
        &diags,
        at(6) + 2,
        Kind::CoverageCountMismatch {
            coverage: 1,
            count: 2,
        },
        e,
    );
    let lg = at(8) + at(at(8) + 4);
    has(&diags, lg + 2, Kind::UnknownFormat { found: 5 }, e);
    has(&diags, 12, Kind::UnknownFormat { found: 2 }, e);
    has(
        &diags,
        14,
        Kind::OffsetOutOfBounds {
            offset: 0x7000,
            length: 0,
            available: d.len() as u32,
        },
        e,
    );
}

#[test]
fn sfnt_checks_mark_sets_against_gdef() {
    let gdef_table = link(
        Buf::new()
            .u32(0x0001_0002)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .done(),
        vec![(
            12,
            2,
            link(
                Buf::new().u16(1).u16(1).u32(0).done(),
                vec![(4, 4, coverage1(&[3]))],
            ),
        )],
    );
    let gsub_table = simple_layout(vec![lookup(
        1,
        0x0010,
        vec![single1(coverage1(&[1]))],
        Some(1),
    )]);
    let (l, _) = positions(&gsub_table, 0);
    for (with_gdef, count) in [(true, 1u32), (false, 0)] {
        let mut tables = vec![(b"GSUB", gsub_table.clone())];
        if with_gdef {
            tables.push((b"GDEF", gdef_table.clone()));
        }
        tables.sort_by_key(|t| *t.0);
        let file = sfnt(&tables);
        let font = Sfnt::new(&file).unwrap();
        let mut diags = Vec::new();
        validate::sfnt(&font, &mut |x| diags.push(x));
        let d: Vec<&Diagnostic> = diags
            .iter()
            .filter(|x| x.table == Tag::new(b"GSUB"))
            .collect();
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].offset as usize, l + 8);
        assert_eq!(d[0].kind, Kind::MarkSetOutOfRange { index: 1, count });
    }
}

#[test]
fn kind_ids_are_stable() {
    let ids = [
        (Kind::CoverageNotSorted { index: 0 }, "coverage_not_sorted"),
        (
            Kind::CoverageDuplicate { index: 0, glyph: 0 },
            "coverage_duplicate",
        ),
        (
            Kind::CoverageIndexMismatch {
                index: 0,
                expected: 0,
                found: 0,
            },
            "coverage_index_mismatch",
        ),
        (
            Kind::ClassRangesOverlap { index: 0 },
            "class_ranges_overlap",
        ),
        (
            Kind::CoverageCountMismatch {
                coverage: 0,
                count: 0,
            },
            "coverage_count_mismatch",
        ),
        (
            Kind::LookupIndexOutOfRange { index: 0, count: 0 },
            "lookup_index_out_of_range",
        ),
        (
            Kind::FeatureIndexOutOfRange { index: 0, count: 0 },
            "feature_index_out_of_range",
        ),
        (
            Kind::SequenceIndexOutOfRange { index: 0, count: 0 },
            "sequence_index_out_of_range",
        ),
        (
            Kind::MarkSetOutOfRange { index: 0, count: 0 },
            "mark_set_out_of_range",
        ),
        (Kind::UnknownLookupType { found: 0 }, "unknown_lookup_type"),
        (
            Kind::ExtensionTypeMismatch {
                expected: 0,
                found: 0,
            },
            "extension_type_mismatch",
        ),
        (Kind::NestedExtension, "nested_extension"),
        (Kind::EmptyInputSequence, "empty_input_sequence"),
        (Kind::ReservedBitsSet { found: 0 }, "reserved_bits_set"),
        (Kind::WorkLimitExceeded, "work_limit_exceeded"),
    ];
    for (k, id) in ids {
        assert_eq!(k.id(), id);
        assert!(!k.to_string().is_empty());
        let mut n = 0;
        k.details(&mut |_, _| n += 1);
        let expected = match k {
            Kind::NestedExtension | Kind::EmptyInputSequence | Kind::WorkLimitExceeded => 0,
            Kind::CoverageNotSorted { .. }
            | Kind::ClassRangesOverlap { .. }
            | Kind::UnknownLookupType { .. }
            | Kind::ReservedBitsSet { .. } => 1,
            Kind::CoverageIndexMismatch { .. } => 3,
            _ => 2,
        };
        assert_eq!(n, expected, "{id}");
    }
}
