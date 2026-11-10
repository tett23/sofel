//! GSUB lookup types 1 to 8 in every format, the 24-bit ones included,
//! the shared sequence context formats, and extensions (ADR 0020).

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

use sofel_read::gsub::{self, SubstSubtable};
use sofel_read::layout::{ChainedSequenceContext, Lookup, SequenceContext, SequenceLookupRecord};
use sofel_read::{GlyphId, ReadError};

fn g(v: u32) -> GlyphId {
    GlyphId(v)
}

/// Subtable 0 of a lookup of `ty` holding `sub`.
fn first(ty: u16, sub: Vec<u8>) -> Vec<u8> {
    lookup(ty, 0, vec![sub], None)
}

fn read(lookup_bytes: &[u8]) -> SubstSubtable<'_> {
    gsub::subtable(&Lookup::new(lookup_bytes).unwrap(), 0).unwrap()
}

/// Glyph arrays: `(16-bit or 24-bit glyphs)` with a 16-bit count.
fn glyphs(wide: bool, v: &[u32]) -> Vec<u8> {
    let mut b = Buf::new().u16(v.len() as u16);
    for x in v {
        b = if wide { b.u24(*x) } else { b.u16(*x as u16) };
    }
    b.done()
}

/// `format`, a coverage offset, a count and offsets to `kids`; 16-bit
/// offsets and count when not `wide`, else 32-bit coverage and 24-bit
/// count and offsets.
fn offsets_subtable(format: u16, wide: bool, coverage: Vec<u8>, kids: Vec<Vec<u8>>) -> Vec<u8> {
    let (cw, w) = if wide { (4, 3) } else { (2, 2) };
    let mut b = zeros(Buf::new().u16(format), cw);
    b = if wide {
        b.u24(kids.len() as u32)
    } else {
        b.u16(kids.len() as u16)
    };
    b = zeros(b, w * kids.len());
    let mut links = vec![(2, cw, coverage)];
    links.extend(
        kids.into_iter()
            .enumerate()
            .map(|(i, k)| (2 + cw + w + w * i, w, k)),
    );
    link(b.done(), links)
}

#[test]
fn single_formats() {
    let l = first(
        1,
        link(
            Buf::new().u16(1).u16(0).u16(0xFFFE).done(),
            vec![(2, 2, coverage1(&[3, 5]))],
        ),
    );
    let SubstSubtable::SingleDelta {
        coverage,
        delta,
        mask,
    } = read(&l)
    else {
        panic!()
    };
    assert_eq!((delta, mask), (0xFFFE, 0xFFFF));
    assert_eq!(coverage.get(g(5)), Some(1));
    // The substitute is (g + delta) & mask: 3 + 0xFFFE = 1 modulo 0x10000.
    assert_eq!((3 + delta) & mask, 1);

    let l = first(
        1,
        link(
            Buf::new().u16(3).u32(0).u24(0xFF_FFFF).done(),
            vec![(2, 4, coverage3(&[0x1_0000]))],
        ),
    );
    let SubstSubtable::SingleDelta {
        coverage,
        delta,
        mask,
    } = read(&l)
    else {
        panic!()
    };
    assert_eq!((delta, mask), (0xFF_FFFF, 0xFF_FFFF));
    assert_eq!(coverage.format(), 3);
    assert_eq!((0x1_0000 + delta) & mask, 0xFFFF);

    let l = first(
        1,
        link(
            Buf::new().u16(2).u16(0).u16(2).u16(7).u16(8).done(),
            vec![(2, 2, coverage1(&[3, 5]))],
        ),
    );
    let SubstSubtable::SingleList {
        coverage,
        substitutes,
    } = read(&l)
    else {
        panic!()
    };
    assert_eq!(coverage.get(g(3)), Some(0));
    assert_eq!(substitutes.iter().collect::<Vec<_>>(), vec![7, 8]);

    let l = first(
        1,
        link(
            Buf::new()
                .u16(4)
                .u32(0)
                .u24(2)
                .u24(0x2_0000)
                .u24(0x2_0001)
                .done(),
            vec![(2, 4, coverage4(&[(0x1_0000, 0x1_0001, 0)]))],
        ),
    );
    let SubstSubtable::SingleList {
        coverage,
        substitutes,
    } = read(&l)
    else {
        panic!()
    };
    assert_eq!(coverage.get(g(0x1_0001)), Some(1));
    assert_eq!(substitutes.width(), 3);
    assert_eq!(substitutes.get(1), Some(0x2_0001));
    assert_eq!(substitutes.get(2), None);
}

#[test]
fn multiple_and_alternate_formats() {
    for (format, wide) in [(1, false), (2, true)] {
        for ty in [2u16, 3] {
            let sub = offsets_subtable(
                format,
                wide,
                coverage1(&[4, 6]),
                vec![glyphs(wide, &[10, 11, 0x1234]), glyphs(wide, &[])],
            );
            let l = first(ty, sub);
            let arrays = match (ty, read(&l)) {
                (
                    2,
                    SubstSubtable::Multiple {
                        sequences,
                        coverage,
                    },
                ) => {
                    assert_eq!(coverage.get(g(6)), Some(1));
                    sequences
                }
                (3, SubstSubtable::Alternate { alternate_sets, .. }) => alternate_sets,
                (_, other) => panic!("{other:?}"),
            };
            assert_eq!(arrays.len(), 2);
            let a = arrays.get(0).unwrap().unwrap();
            assert_eq!(a.width(), if wide { 3 } else { 2 });
            assert_eq!(a.iter().collect::<Vec<_>>(), vec![10, 11, 0x1234]);
            assert!(arrays.get(1).unwrap().unwrap().is_empty());
            assert_eq!(arrays.get(2).unwrap_err(), ReadError::OutOfBounds);
        }
    }
}

fn ligature(wide: bool, glyph: u32, components: &[u32]) -> Vec<u8> {
    let mut b = if wide {
        Buf::new().u24(glyph)
    } else {
        Buf::new().u16(glyph as u16)
    };
    b = b.u16(components.len() as u16 + 1);
    for c in components {
        b = if wide { b.u24(*c) } else { b.u16(*c as u16) };
    }
    b.done()
}

fn ligature_set(wide: bool, ligatures: Vec<Vec<u8>>) -> Vec<u8> {
    let w = if wide { 3 } else { 2 };
    let b = zeros(Buf::new().u16(ligatures.len() as u16), w * ligatures.len());
    let links = ligatures
        .into_iter()
        .enumerate()
        .map(|(i, l)| (2 + w * i, w, l))
        .collect();
    link(b.done(), links)
}

#[test]
fn ligature_formats() {
    for (format, wide) in [(1, false), (2, true)] {
        let set = ligature_set(
            wide,
            vec![ligature(wide, 0x99, &[2, 3]), ligature(wide, 0x1_0098, &[])],
        );
        let l = first(
            4,
            offsets_subtable(format, wide, coverage1(&[1]), vec![set]),
        );
        let SubstSubtable::Ligature {
            coverage,
            ligature_sets,
        } = read(&l)
        else {
            panic!()
        };
        assert_eq!(coverage.get(g(1)), Some(0));
        let set = ligature_sets.get(0).unwrap().unwrap();
        assert_eq!(set.len(), 2);
        let a = set.get(0).unwrap().unwrap();
        assert_eq!(a.glyph, g(0x99));
        assert_eq!(a.components.iter().collect::<Vec<_>>(), vec![2, 3]);
        let b = set.get(1).unwrap().unwrap();
        assert_eq!(b.glyph, g(if wide { 0x1_0098 } else { 0x98 }));
        assert!(b.components.is_empty());
    }
}

fn records(v: &[(u16, u16)]) -> Vec<SequenceLookupRecord> {
    v.iter()
        .map(|&(sequence_index, lookup_index)| SequenceLookupRecord {
            sequence_index,
            lookup_index,
        })
        .collect()
}

/// A SequenceRule with values of `width` bytes.
fn rule(width: usize, input: &[u32], lookups: &[(u16, u16)]) -> Vec<u8> {
    let mut b = Buf::new()
        .u16(input.len() as u16 + 1)
        .u16(lookups.len() as u16);
    for x in input {
        b = if width == 3 {
            b.u24(*x)
        } else {
            b.u16(*x as u16)
        };
    }
    for (s, l) in lookups {
        b = b.u16(*s).u16(*l);
    }
    b.done()
}

/// A rule set with a 16-bit count and offsets of `width` bytes.
fn rule_set(width: usize, rules: Vec<Vec<u8>>) -> Vec<u8> {
    let b = zeros(Buf::new().u16(rules.len() as u16), width * rules.len());
    let links = rules
        .into_iter()
        .enumerate()
        .map(|(i, r)| (2 + width * i, width, r))
        .collect();
    link(b.done(), links)
}

#[test]
fn sequence_context_formats_1_and_4() {
    // Format 1: 16-bit everything. Format 4: 32-bit coverage, 24-bit
    // count and rule set offsets, 16-bit rule offsets, 24-bit glyphs.
    for (format, value_w) in [(1u16, 2usize), (4, 3)] {
        let wide = format == 4;
        let set = rule_set(2, vec![rule(value_w, &[0x1_0002, 3], &[(1, 4)])]);
        let sub = offsets_subtable(format, wide, coverage1(&[1]), vec![set]);
        let l = first(5, sub);
        let SubstSubtable::Context(SequenceContext::Glyphs {
            coverage,
            rule_sets,
        }) = read(&l)
        else {
            panic!()
        };
        assert_eq!(coverage.get(g(1)), Some(0));
        let r = rule_sets.get(0).unwrap().unwrap().get(0).unwrap().unwrap();
        let expected = if wide { 0x1_0002 } else { 0x0002 };
        assert_eq!(r.input.iter().collect::<Vec<_>>(), vec![expected, 3]);
        assert_eq!(r.lookups.iter().collect::<Vec<_>>(), records(&[(1, 4)]));
    }
}

#[test]
fn sequence_context_formats_2_and_5() {
    // Format 5: 32-bit coverage and ClassDef, 24-bit count and offsets,
    // rules with 16-bit classes behind 24-bit offsets.
    for format in [2u16, 5] {
        let wide = format == 5;
        let (cw, w) = if wide { (4, 3) } else { (2, 2) };
        let set = rule_set(w, vec![rule(2, &[2], &[(0, 1)])]);
        let mut b = zeros(Buf::new().u16(format), 2 * cw);
        b = if wide { b.u24(2) } else { b.u16(2) };
        b = zeros(b, 2 * w);
        let sub = link(
            b.done(),
            vec![
                (2, cw, coverage1(&[1, 2])),
                (2 + cw, cw, class_def2(&[(1, 1, 1), (2, 2, 2)])),
                (2 + 2 * cw + w, w, vec![]),
                (2 + 2 * cw + 2 * w, w, set),
            ],
        );
        let l = first(5, sub);
        let SubstSubtable::Context(SequenceContext::Classes {
            coverage,
            class_def,
            rule_sets,
        }) = read(&l)
        else {
            panic!()
        };
        assert_eq!(coverage.num_records(), 2);
        assert_eq!(class_def.get(g(2)), 2);
        assert_eq!(rule_sets.len(), 2);
        assert!(rule_sets.get(0).unwrap().is_none());
        let r = rule_sets.get(1).unwrap().unwrap().get(0).unwrap().unwrap();
        assert_eq!(r.input.iter().collect::<Vec<_>>(), vec![2]);
        assert_eq!(r.lookups.get(0).unwrap().lookup_index, 1);
    }
}

#[test]
fn sequence_context_formats_3_and_6() {
    for format in [3u16, 6] {
        let w = if format == 6 { 3 } else { 2 };
        let b = zeros(Buf::new().u16(format).u16(2).u16(1), 2 * w)
            .u16(1)
            .u16(0);
        let sub = link(
            b.done(),
            vec![(6, w, coverage1(&[1])), (6 + w, w, coverage1(&[2]))],
        );
        let l = first(5, sub);
        let SubstSubtable::Context(SequenceContext::Coverages { coverages, lookups }) = read(&l)
        else {
            panic!()
        };
        assert_eq!((coverages.len(), coverages.width()), (2, w));
        assert_eq!(coverages.get(1).unwrap().unwrap().get(g(2)), Some(0));
        assert_eq!(lookups.iter().collect::<Vec<_>>(), records(&[(1, 0)]));
    }
    // No input glyphs: HarfBuzz rejects the subtable.
    let l = first(5, Buf::new().u16(3).u16(0).u16(0).done());
    assert_eq!(
        gsub::subtable(&Lookup::new(&l).unwrap(), 0).unwrap_err(),
        ReadError::InvalidValue
    );
}

/// A ChainedSequenceRule with values of `width` bytes.
fn chain_rule(
    width: usize,
    bt: &[u32],
    input: &[u32],
    la: &[u32],
    lookups: &[(u16, u16)],
) -> Vec<u8> {
    let put = |mut b: Buf, v: &[u32]| {
        for x in v {
            b = if width == 3 {
                b.u24(*x)
            } else {
                b.u16(*x as u16)
            };
        }
        b
    };
    let mut b = put(Buf::new().u16(bt.len() as u16), bt);
    b = put(b.u16(input.len() as u16 + 1), input);
    b = put(b.u16(la.len() as u16), la);
    b = b.u16(lookups.len() as u16);
    for (s, l) in lookups {
        b = b.u16(*s).u16(*l);
    }
    b.done()
}

#[test]
fn chained_context_formats_1_and_4() {
    // Format 4: 24-bit rule set offsets and count, 24-bit rule offsets
    // and glyphs.
    for format in [1u16, 4] {
        let wide = format == 4;
        let w = if wide { 3 } else { 2 };
        let set = rule_set(
            w,
            vec![chain_rule(w, &[0x1_0007], &[8], &[9, 10], &[(0, 2)])],
        );
        let l = first(
            6,
            offsets_subtable(format, wide, coverage1(&[1]), vec![set]),
        );
        let SubstSubtable::ChainContext(ChainedSequenceContext::Glyphs { rule_sets, .. }) =
            read(&l)
        else {
            panic!()
        };
        let r = rule_sets.get(0).unwrap().unwrap().get(0).unwrap().unwrap();
        let bt = if wide { 0x1_0007 } else { 7 };
        assert_eq!(r.backtrack.iter().collect::<Vec<_>>(), vec![bt]);
        assert_eq!(r.input.iter().collect::<Vec<_>>(), vec![8]);
        assert_eq!(r.lookahead.iter().collect::<Vec<_>>(), vec![9, 10]);
        assert_eq!(r.lookups.iter().collect::<Vec<_>>(), records(&[(0, 2)]));
    }
}

#[test]
fn chained_context_formats_2_and_5() {
    // Format 5: 32-bit coverage and ClassDefs, a 16-bit count of 24-bit
    // rule set offsets, 24-bit rule offsets, 16-bit classes.
    for format in [2u16, 5] {
        let wide = format == 5;
        let (cw, w) = if wide { (4, 3) } else { (2, 2) };
        let set = rule_set(w, vec![chain_rule(2, &[1], &[], &[2], &[(0, 0)])]);
        let b = zeros(zeros(Buf::new().u16(format), 4 * cw).u16(1), w);
        let sub = link(
            b.done(),
            vec![
                (2, cw, coverage1(&[1])),
                (2 + cw, cw, class_def2(&[(5, 5, 1)])),
                (2 + 2 * cw, cw, class_def1(1, &[1])),
                (2 + 3 * cw, cw, vec![]),
                (4 + 4 * cw, w, set),
            ],
        );
        let l = first(6, sub);
        let SubstSubtable::ChainContext(ChainedSequenceContext::Classes {
            backtrack_class_def,
            input_class_def,
            lookahead_class_def,
            rule_sets,
            ..
        }) = read(&l)
        else {
            panic!()
        };
        assert_eq!(backtrack_class_def.get(g(5)), 1);
        assert_eq!(input_class_def.get(g(1)), 1);
        // A null ClassDef offset reads as all class 0.
        assert_eq!(lookahead_class_def.format(), 0);
        let r = rule_sets.get(0).unwrap().unwrap().get(0).unwrap().unwrap();
        assert_eq!(r.backtrack.iter().collect::<Vec<_>>(), vec![1]);
        assert!(r.input.is_empty());
        assert_eq!(r.lookahead.iter().collect::<Vec<_>>(), vec![2]);
    }
}

#[test]
fn chained_context_format_3() {
    let b = Buf::new()
        .u16(3)
        .u16(1)
        .u16(0)
        .u16(1)
        .u16(0)
        .u16(0)
        .u16(1)
        .u16(0)
        .u16(3);
    let sub = link(
        b.done(),
        vec![(4, 2, coverage1(&[1])), (8, 2, coverage1(&[2]))],
    );
    let l = first(6, sub);
    let SubstSubtable::ChainContext(ChainedSequenceContext::Coverages {
        backtrack,
        input,
        lookahead,
        lookups,
    }) = read(&l)
    else {
        panic!()
    };
    assert_eq!((backtrack.len(), input.len(), lookahead.len()), (1, 1, 0));
    assert_eq!(input.get(0).unwrap().unwrap().get(g(2)), Some(0));
    assert_eq!(lookups.iter().collect::<Vec<_>>(), records(&[(0, 3)]));
    let l = first(6, Buf::new().u16(3).u16(0).u16(0).u16(0).u16(0).done());
    assert_eq!(
        gsub::subtable(&Lookup::new(&l).unwrap(), 0).unwrap_err(),
        ReadError::InvalidValue
    );
}

#[test]
fn reverse_chain_single_formats() {
    for (format, wide) in [(1u16, false), (2, true)] {
        let (cw, w) = if wide { (4, 3) } else { (2, 2) };
        let mut b = zeros(Buf::new().u16(format), cw).u16(1);
        b = zeros(b, w).u16(2);
        b = zeros(b, 2 * w);
        b = if wide {
            b.u24(1).u24(0x1_0001)
        } else {
            b.u16(1).u16(0x31)
        };
        let la = 2 + cw + 2 + w + 2;
        let sub = link(
            b.done(),
            vec![
                (2, cw, coverage1(&[1])),
                (2 + cw + 2, w, coverage1(&[2])),
                (la, w, coverage1(&[3])),
                (la + w, w, coverage1(&[4])),
            ],
        );
        let l = first(8, sub);
        let SubstSubtable::ReverseChainSingle {
            backtrack,
            lookahead,
            substitutes,
            ..
        } = read(&l)
        else {
            panic!()
        };
        assert_eq!((backtrack.len(), lookahead.len()), (1, 2));
        assert_eq!(lookahead.get(1).unwrap().unwrap().get(g(4)), Some(0));
        let s = if wide { 0x1_0001 } else { 0x31 };
        assert_eq!(substitutes.iter().collect::<Vec<_>>(), vec![s]);
        assert_eq!(gsub::effective_type(&Lookup::new(&l).unwrap()), 8);
    }
}

fn single1() -> Vec<u8> {
    link(
        Buf::new().u16(1).u16(0).u16(1).done(),
        vec![(2, 2, coverage1(&[1]))],
    )
}

#[test]
fn extensions_are_followed() {
    let l = lookup(
        7,
        0,
        vec![extension(1, single1()), extension(1, single1())],
        None,
    );
    let lk = Lookup::new(&l).unwrap();
    assert!(matches!(
        gsub::subtable(&lk, 1).unwrap(),
        SubstSubtable::SingleDelta { delta: 1, .. }
    ));
    assert_eq!(lk.lookup_type(), 7);
    assert_eq!(gsub::effective_type(&lk), 1);
    assert!(lk.extension_types_agree(7));

    let reverse = lookup(7, 0, vec![extension(8, vec![0, 9])], None);
    assert_eq!(gsub::effective_type(&Lookup::new(&reverse).unwrap()), 8);

    // An extension pointing to an extension is an error.
    let nested = lookup(7, 0, vec![extension(7, extension(1, single1()))], None);
    let lk = Lookup::new(&nested).unwrap();
    assert_eq!(gsub::subtable(&lk, 0).unwrap_err(), ReadError::InvalidValue);
    assert_eq!(gsub::effective_type(&lk), 0);

    // Subtables of different types: HarfBuzz drops the lookup.
    let mixed = lookup(
        7,
        0,
        vec![extension(1, single1()), extension(2, vec![0, 1])],
        None,
    );
    assert!(!Lookup::new(&mixed).unwrap().extension_types_agree(7));

    // Unknown extension formats and lookup types.
    let odd = lookup(7, 0, vec![Buf::new().u16(2).u16(1).u32(8).done()], None);
    let lk = Lookup::new(&odd).unwrap();
    assert_eq!(
        gsub::subtable(&lk, 0).unwrap_err(),
        ReadError::UnknownFormat(2)
    );
    assert_eq!(gsub::effective_type(&lk), 0);
    let unknown = lookup(9, 0, vec![single1()], None);
    assert_eq!(
        gsub::subtable(&Lookup::new(&unknown).unwrap(), 0).unwrap_err(),
        ReadError::InvalidValue
    );
    // A null subtable offset.
    let null = lookup(1, 0, vec![vec![]], None);
    assert_eq!(
        gsub::subtable(&Lookup::new(&null).unwrap(), 0).unwrap_err(),
        ReadError::OutOfBounds
    );
}

#[test]
fn bad_coverage_reads_as_empty() {
    // A coverage offset past the data, as HarfBuzz's neutered offset.
    let l = first(1, Buf::new().u16(1).u16(0x7000).u16(1).done());
    let SubstSubtable::SingleDelta { coverage, .. } = read(&l) else {
        panic!()
    };
    assert_eq!(coverage.format(), 0);
    assert_eq!(
        gsub::subtable(
            &Lookup::new(&first(1, Buf::new().u16(7).done())).unwrap(),
            0
        )
        .unwrap_err(),
        ReadError::UnknownFormat(7)
    );
    assert!(matches!(
        gsub::subtable(&Lookup::new(&first(1, vec![0, 1, 0, 6])).unwrap(), 0),
        Err(ReadError::Truncated { .. })
    ));
}
