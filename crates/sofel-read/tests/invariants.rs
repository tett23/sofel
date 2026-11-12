//! The validator invariants of ADR 0007, checked on many mutated inputs:
//! without an `Error`, views can be built and in-range reads are `Some`.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

mod support;
use support::build::*;

use sofel_read::cmap::Cmap;
use sofel_read::head::Head;
use sofel_read::hhea::Hhea;
use sofel_read::hmtx::Hmtx;
use sofel_read::maxp::Maxp;
use sofel_read::sfnt::Sfnt;
use sofel_read::{
    CountWidth, Diagnostic, GlyphId, Severity, cmap, glyf, head, hhea, hmtx, maxp, validate,
};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn mutate(rng: &mut XorShift, base: &[u8]) -> Vec<u8> {
    let mut d = base.to_vec();
    match rng.below(4) {
        0 => {
            let n = rng.below(d.len() + 1);
            d.truncate(n);
        }
        1 => {
            for _ in 0..=rng.below(4) {
                if !d.is_empty() {
                    let i = rng.below(d.len());
                    d[i] = rng.next() as u8;
                }
            }
        }
        2 => {
            for _ in 0..rng.below(16) {
                d.push(rng.next() as u8);
            }
        }
        _ => {
            if d.len() >= 2 {
                let i = rng.below(d.len() - 1);
                d[i] = 0xFF;
                d[i + 1] = 0xFF;
            }
        }
    }
    d
}

fn has_error(f: impl FnOnce(&mut dyn FnMut(Diagnostic))) -> bool {
    let mut error = false;
    f(&mut |d| error |= d.severity == Severity::Error);
    error
}

const ROUNDS: usize = 1500;

fn cmap_bases() -> Vec<Vec<u8>> {
    let mut is32 = [0u8; 8192];
    is32[0xD800 >> 3] |= 0x80;
    let mut keys = [0u16; 256];
    keys[0x81] = 1;
    vec![
        cmap(&[(
            3,
            1,
            cmap4(&[
                seg(0x41, 0x5A, 3),
                seg_ids(0x61, 0x63, 2, vec![4, 0, 6]),
                seg(0xFFFF, 0xFFFF, 1),
            ]),
        )]),
        cmap(&[(
            3,
            10,
            cmap12_13(12, &[(0x41, 0x43, 1), (0x1F600, 0x1F601, 9)]),
        )]),
        cmap(&[(3, 10, cmap12_13(13, &[(0x41, 0x43, 1)]))]),
        cmap(&[(1, 0, cmap0(&[7u8; 256]))]),
        cmap(&[(
            3,
            2,
            cmap2(&keys, &[(0x20, 0, vec![3, 4]), (0x40, 5, vec![10, 11])]),
        )]),
        cmap(&[(1, 0, cmap6(0x20, &[1, 2, 3]))]),
        cmap(&[(3, 10, cmap8(&is32, &[(0x41, 0x42, 3)]))]),
        cmap(&[(3, 10, cmap10(0x1_0000, &[1, 2]))]),
        cmap(&[(
            0,
            5,
            cmap14_15(
                false,
                &[Uvs {
                    selector: 0xFE00,
                    defaults: Some(vec![(0x4E00, 3)]),
                    mappings: Some(vec![(0x5000, 4)]),
                }],
            ),
        )]),
    ]
}

#[test]
fn cmap_invariants() {
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    for base in cmap_bases() {
        for _ in 0..ROUNDS {
            let d = mutate(&mut rng, &base);
            let error = has_error(|r| cmap::validate(&d, Some(30), r));
            let Ok(c) = Cmap::new(&d) else {
                assert!(error, "Cmap::new failed without an Error: {d:02x?}");
                continue;
            };
            for rec in c.records() {
                match rec.subtable() {
                    Err(_) => assert!(error, "subtable failed without an Error: {d:02x?}"),
                    Ok(s) => {
                        for (cp, gid) in s.mappings().take(5000) {
                            assert_eq!(s.glyph(cp), Some(gid));
                        }
                        for cp in [
                            0u32,
                            0x41,
                            0x61,
                            0x8140,
                            0xFFFF,
                            0x1F600,
                            0x10_FFFF,
                            u32::MAX,
                        ] {
                            let _ = s.glyph(cp);
                            let _ = s.variant(cp, 0xFE00);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn metrics_invariants() {
    let mut rng = XorShift(0x0123_4567_89AB_CDEF);
    let base_head = head(HeadSpec::default());
    let base_hhea = hhea(3);
    let base_maxp = maxp10(5);
    let base_hmtx = hmtx(&[(1, 2), (3, 4), (5, 6)], &[7, 8]);
    for _ in 0..ROUNDS {
        let d = mutate(&mut rng, &base_head);
        if !has_error(|r| head::validate(&d, r)) {
            Head::new(&d).unwrap();
        }
        let d = mutate(&mut rng, &base_hhea);
        if !has_error(|r| hhea::validate(&d, CountWidth::U16, r)) {
            Hhea::new(&d, CountWidth::U16).unwrap();
        }
        let d = mutate(&mut rng, &base_maxp);
        if !has_error(|r| maxp::validate(&d, CountWidth::U16, r)) {
            Maxp::new(&d, CountWidth::U16).unwrap();
        }
        let d = mutate(&mut rng, &base_hmtx);
        let nlm = rng.below(5) as u32;
        let glyphs = rng.below(7) as u32;
        if !has_error(|r| hmtx::validate(&d, nlm, glyphs, r)) {
            let h = Hmtx::new(&d, nlm, glyphs).unwrap();
            for g in 0..glyphs {
                assert!(h.advance(GlyphId(g)).is_some(), "{d:02x?} {nlm} {glyphs}");
                assert!(
                    h.left_side_bearing(GlyphId(g)).is_some(),
                    "{d:02x?} {nlm} {glyphs}"
                );
            }
        }
    }
}

#[test]
fn whole_font_never_panics() {
    let mut rng = XorShift(0xDEAD_BEEF_CAFE_F00D);
    let base = sample_font();
    for _ in 0..ROUNDS {
        let d = mutate(&mut rng, &base);
        if let Ok(s) = Sfnt::new(&d) {
            validate::sfnt(&s, &mut |_| {});
        }
    }
}

#[test]
fn glyf_invariants() {
    use sofel_read::glyf::Glyph;
    use sofel_read::loca::Loca;
    let square = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::Off(0, 700),
            Pt::On(500, 700),
            Pt::On(500, 0),
        ]],
        false,
        &[],
    );
    let comp = composite_glyph(
        &[
            comp(1, Args::Xy(-3, 400)),
            Comp {
                glyph: 1,
                args: Args::Points(1, 2),
                xf: Xf::TwoByTwo(1, 2, 3, 4),
                flags: 0,
            },
        ],
        [0; 4],
        false,
    );
    let glyphs = vec![square.clone(), comp];
    let (glyf_base, loca) = glyf_loca(&glyphs, true);
    let lv = Loca::new(&loca, 1).unwrap();
    let mut rng = XorShift(0x5151_5151_5151_5151);
    for _ in 0..ROUNDS * 4 {
        let g = mutate(&mut rng, &glyf_base);
        let error = has_error(|r| glyf::validate(&g, &lv, 2, false, glyf::TAG, r));
        if error {
            continue;
        }
        for i in 0..2 {
            let (s, e) = (
                lv.offset(i).unwrap() as usize,
                lv.offset(i + 1).unwrap() as usize,
            );
            let Some(bytes) = g.get(s..e.min(g.len())) else {
                continue;
            };
            if e > g.len() || bytes.is_empty() {
                continue;
            }
            let glyph = Glyph::new(bytes).unwrap();
            if glyph.number_of_contours() > 0 {
                let simple = glyph.simple().unwrap();
                assert_eq!(simple.points(false).count() as u32, simple.num_points());
            } else if glyph.number_of_contours() < 0 {
                assert!(glyph.components(false).all(|c| c.is_ok()));
            }
        }
    }
}

#[test]
fn metrics_table_invariants() {
    use sofel_read::name::Name;
    use sofel_read::os2::Os2;
    use sofel_read::post::Post;
    use sofel_read::vorg::Vorg;
    use sofel_read::{name, os2, post, vorg};
    let mut rng = XorShift(0x0BAD_C0DE_1234_5678);
    let base_os2 = os2(Os2Spec {
        version: 5,
        ..Default::default()
    });
    let ps = utf16be("Sofel-Regular");
    let records: Vec<NameRec<'_>> = vec![(1, 0, 0, 6, b"Sofel"), (3, 1, 0x409, 6, &ps)];
    let base_names = [name(0, &records, &[]), name(1, &records, &[b"en"])];
    let base_post = post2(&[0, 258, 259, 3], &["alpha", "beta"]);
    let base_vorg = [
        vorg(1, 880, &[(1, 1), (3, 2)]),
        vorg(2, 880, &[(1, 1), (3, 2)]),
    ];
    for _ in 0..ROUNDS {
        let d = mutate(&mut rng, &base_os2);
        if !has_error(|r| os2::validate(&d, r)) {
            Os2::new(&d).unwrap();
        }
        for base in &base_names {
            let d = mutate(&mut rng, base);
            if !has_error(|r| name::validate(&d, r)) {
                let n = Name::new(&d).unwrap();
                for r in n.records() {
                    assert!(n.string(&r).is_some(), "{d:02x?}");
                }
                for i in 0..usize::from(n.lang_tag_count()) {
                    assert!(n.lang_tag_string(&n.lang_tag(i).unwrap()).is_some());
                }
            }
        }
        let d = mutate(&mut rng, &base_post);
        if !has_error(|r| post::validate(&d, r)) {
            let p = Post::new(&d).unwrap();
            let stored = p.names().count();
            for g in 0..u32::from(p.num_glyphs().unwrap_or(0)) {
                if let Some(i) = p.glyph_name_index(GlyphId(g)) {
                    assert!(i < 258 || usize::from(i - 258) < stored, "{d:02x?}");
                }
            }
        }
        for base in &base_vorg {
            let d = mutate(&mut rng, base);
            if !has_error(|r| vorg::validate(&d, Some(5), r)) {
                let v = Vorg::new(&d).unwrap();
                for i in 0..v.num_records() {
                    let (g, y) = v.record(i).unwrap();
                    assert_eq!(v.lookup(g), Some(y), "{d:02x?}");
                }
            }
        }
    }
}

/// Follows every offset of a BASE table; false when any table it points
/// to cannot be read.
fn base_walks(data: &[u8]) -> bool {
    use sofel_read::base::{Base, BaseCoord, MinMax};
    fn coord(c: Result<Option<BaseCoord<'_>>, sofel_read::ReadError>) -> bool {
        match c {
            Err(_) => false,
            Ok(None) => true,
            Ok(Some(c)) => c.device().is_ok(),
        }
    }
    fn min_max(m: Result<Option<MinMax<'_>>, sofel_read::ReadError>) -> bool {
        match m {
            Err(_) => false,
            Ok(None) => true,
            Ok(Some(m)) => {
                coord(m.min_coord())
                    && coord(m.max_coord())
                    && (0..m.num_features())
                        .all(|i| coord(m.feature_min_coord(i)) && coord(m.feature_max_coord(i)))
            }
        }
    }
    let Ok(b) = Base::new(data) else { return false };
    [b.horiz_axis(), b.vert_axis()].into_iter().all(|a| {
        let Ok(a) = a else { return false };
        let Some(a) = a else { return true };
        if a.base_tag_list().is_err() {
            return false;
        }
        let Ok(list) = a.base_script_list() else {
            return false;
        };
        let Some(list) = list else { return true };
        (0..list.len()).all(|i| match list.script(i) {
            Err(_) => false,
            Ok(None) => true,
            Ok(Some(s)) => {
                let values = match s.base_values() {
                    Err(_) => false,
                    Ok(None) => true,
                    Ok(Some(v)) => (0..v.len()).all(|i| coord(v.coord(i))),
                };
                values
                    && min_max(s.default_min_max())
                    && (0..s.num_lang_sys()).all(|i| min_max(s.lang_sys_min_max(i)))
            }
        })
    })
}

#[test]
fn base_table_invariants() {
    use sofel_read::base;
    let mut rng = XorShift(0x5EED_BA5E_0017_0017);
    let axis = AxisSpec {
        tags: Some(vec![*b"ideo", *b"romn"]),
        scripts: Some(vec![(
            *b"latn",
            Some(ScriptSpec {
                values: Some((
                    1,
                    vec![
                        Some(Coord::F3(-120, Some(device(9, 17, 2, &[0, 0, 0])))),
                        Some(Coord::F4(0, 5, 1)),
                    ],
                )),
                default_min_max: Some(MinMaxSpec {
                    min: Some(Coord::F2(-200, 1, 2)),
                    max: Some(Coord::F1(900)),
                    features: vec![(*b"kern", Some(Coord::F1(-1)), None)],
                }),
                lang_sys: vec![(
                    *b"ENG ",
                    Some(MinMaxSpec {
                        min: Some(Coord::F1(-100)),
                        ..Default::default()
                    }),
                )],
            }),
        )]),
    };
    let tables = [
        base(Some(&axis), Some(&axis)),
        base11(Some(&axis), None, Some(&[0, 1, 0, 0, 0, 0, 0, 0])),
    ];
    for _ in 0..ROUNDS {
        for t in &tables {
            let d = mutate(&mut rng, t);
            if !has_error(|r| base::validate(&d, r)) {
                assert!(base_walks(&d), "{d:02x?}");
            }
        }
    }
}

// ---- GDEF, GSUB and GPOS (ADR 0020) ----

use sofel_read::layout::{
    ChainedSequenceContext, CoverageArray, LayoutTable, Lookup, SequenceContext,
};
use sofel_read::{ReadError, gdef, gpos, gsub};

fn ok<T>(r: Result<Option<T>, ReadError>, f: impl FnOnce(T) -> bool) -> bool {
    match r {
        Err(_) => false,
        Ok(None) => true,
        Ok(Some(t)) => f(t),
    }
}

fn coverages_walk(a: &CoverageArray<'_>) -> bool {
    (0..a.len()).all(|i| a.get(i).is_ok())
}

fn context_walks(c: &SequenceContext<'_>) -> bool {
    match c {
        SequenceContext::Glyphs { rule_sets, .. } | SequenceContext::Classes { rule_sets, .. } => {
            (0..rule_sets.len())
                .all(|i| ok(rule_sets.get(i), |s| (0..s.len()).all(|j| s.get(j).is_ok())))
        }
        SequenceContext::Coverages { coverages, .. } => coverages_walk(coverages),
    }
}

fn chain_walks(c: &ChainedSequenceContext<'_>) -> bool {
    match c {
        ChainedSequenceContext::Glyphs { rule_sets, .. }
        | ChainedSequenceContext::Classes { rule_sets, .. } => (0..rule_sets.len())
            .all(|i| ok(rule_sets.get(i), |s| (0..s.len()).all(|j| s.get(j).is_ok()))),
        ChainedSequenceContext::Coverages {
            backtrack,
            input,
            lookahead,
            ..
        } => coverages_walk(backtrack) && coverages_walk(input) && coverages_walk(lookahead),
    }
}

/// The header, lists and lookups walk; `sub` walks each subtable.
fn layout_walks(data: &[u8], extension: u16, sub: impl Fn(&Lookup<'_>, usize) -> bool) -> bool {
    let Ok(t) = LayoutTable::new(data) else {
        return false;
    };
    let scripts = ok(t.script_list(), |l| {
        (0..l.len()).all(|i| {
            ok(l.script(i), |s| {
                s.default_lang_sys().is_ok() && (0..s.num_lang_sys()).all(|j| s.lang_sys(j).is_ok())
            })
        })
    });
    let features = ok(t.feature_list(), |l| {
        (0..l.len()).all(|i| l.feature(i).is_ok())
    });
    let lookups = ok(t.lookup_list(), |l| {
        (0..l.len()).all(|i| {
            ok(l.lookup(i), |lk| {
                lk.extension_types_agree(extension)
                    && (0..lk.num_subtables())
                        .all(|j| {
                            // Null offsets have no target for the view to walk.
                            let null_extension = lk.lookup_type() == extension && matches!(
                                lk.subtable_data(j),
                                Some([0, 1, 0, ty, 0, 0, 0, 0, ..]) if (1..=8).contains(ty) && u16::from(*ty) != extension
                            );
                            lk.subtable_offset(j) == Some(0) || null_extension || sub(&lk, j)
                        })
            })
        })
    });
    scripts && features && lookups
}

fn gsub_walks(data: &[u8]) -> bool {
    use gsub::SubstSubtable as S;
    layout_walks(data, gsub::EXTENSION, |lk, j| match gsub::subtable(lk, j) {
        Err(_) => false,
        Ok(S::SingleDelta { .. } | S::SingleList { .. }) => true,
        Ok(
            S::Multiple { sequences: a, .. }
            | S::Alternate {
                alternate_sets: a, ..
            },
        ) => (0..a.len()).all(|i| a.get(i).is_ok()),
        Ok(S::Ligature { ligature_sets, .. }) => (0..ligature_sets.len()).all(|i| {
            ok(ligature_sets.get(i), |s| {
                (0..s.len()).all(|k| s.get(k).is_ok())
            })
        }),
        Ok(S::Context(c)) => context_walks(&c),
        Ok(S::ChainContext(c)) => chain_walks(&c),
        Ok(S::ReverseChainSingle {
            backtrack,
            lookahead,
            ..
        }) => coverages_walk(&backtrack) && coverages_walk(&lookahead),
    })
}

fn anchor_ok(a: Result<Option<gpos::Anchor<'_>>, ReadError>) -> bool {
    a.is_ok()
}

fn matrix_walks(m: &gpos::AnchorMatrix<'_>) -> bool {
    (0..m.rows()).all(|r| (0..m.cols()).all(|c| anchor_ok(m.get(r, c))))
}

fn marks_walk(m: &gpos::MarkArray<'_>) -> bool {
    (0..m.len()).all(|i| m.get(i).is_some_and(|(_, a)| anchor_ok(a)))
}

fn gpos_walks(data: &[u8]) -> bool {
    use gpos::PosSubtable as P;
    layout_walks(data, gpos::EXTENSION, |lk, j| match gpos::subtable(lk, j) {
        Err(_) => false,
        Ok(P::Single { .. } | P::SingleArray { .. } | P::PairClasses { .. }) => true,
        Ok(P::PairGlyphs { pair_sets, .. }) => {
            (0..pair_sets.len()).all(|i| pair_sets.get(i).is_ok())
        }
        Ok(P::Cursive { entry_exits, .. }) => (0..entry_exits.len()).all(|i| {
            entry_exits
                .get(i)
                .is_some_and(|(a, b)| anchor_ok(a) && anchor_ok(b))
        }),
        Ok(
            P::MarkBase {
                marks, bases: m, ..
            }
            | P::MarkMark {
                marks, mark2s: m, ..
            },
        ) => marks_walk(&marks) && matrix_walks(&m),
        Ok(P::MarkLig {
            marks, ligatures, ..
        }) => {
            marks_walk(&marks)
                && (0..ligatures.len()).all(|i| ok(ligatures.get(i), |m| matrix_walks(&m)))
        }
        Ok(P::Context(c)) => context_walks(&c),
        Ok(P::ChainContext(c)) => chain_walks(&c),
    })
}

fn gdef_walks(data: &[u8]) -> bool {
    let Ok(t) = gdef::Gdef::new(data) else {
        return false;
    };
    t.glyph_class_def().is_ok()
        && t.mark_attach_class_def().is_ok()
        && ok(t.attach_list(), |a| {
            a.coverage().is_ok() && (0..a.len()).all(|i| a.points(i).is_ok())
        })
        && ok(t.lig_caret_list(), |l| {
            l.coverage().is_ok()
                && (0..l.len())
                    .all(|i| ok(l.lig_glyph(i), |g| (0..g.len()).all(|k| g.caret(k).is_ok())))
        })
        && ok(t.mark_glyph_sets(), |s| {
            (0..s.len()).all(|i| s.coverage(i).is_ok())
        })
}

fn layout_with(lookups: Vec<Vec<u8>>) -> Vec<u8> {
    simple_layout(lookups)
}

fn sub_with(format: u16, coverage: Vec<u8>, kids: Vec<Vec<u8>>, rest: &[u8]) -> Vec<u8> {
    let n = kids.len();
    let mut b = zeros(Buf::new().u16(format), 2).u16(n as u16);
    b = zeros(b, 2 * n).bytes(rest);
    let mut links = vec![(2, 2, coverage)];
    links.extend(kids.into_iter().enumerate().map(|(i, k)| (6 + 2 * i, 2, k)));
    link(b.done(), links)
}

fn gsub_base() -> Vec<u8> {
    let cov = || coverage1(&[1, 2]);
    let single = link(Buf::new().u16(1).u16(0).u16(3).done(), vec![(2, 2, cov())]);
    let list = link(
        Buf::new().u16(2).u16(0).u16(2).u16(5).u16(6).done(),
        vec![(2, 2, cov())],
    );
    let seq = Buf::new().u16(2).u16(7).u16(8).done();
    let multiple = sub_with(1, cov(), vec![seq.clone(), seq.clone()], &[]);
    let lig = Buf::new().u16(9).u16(2).u16(2).done();
    let set = link(Buf::new().u16(1).u16(0).done(), vec![(2, 2, lig)]);
    let ligature = sub_with(1, cov(), vec![set.clone(), set], &[]);
    let rule = Buf::new().u16(2).u16(1).u16(2).u16(1).u16(0).done();
    let rules = link(Buf::new().u16(1).u16(0).done(), vec![(2, 2, rule)]);
    let context = sub_with(1, cov(), vec![rules.clone(), rules], &[]);
    let chain3 = link(
        Buf::new()
            .u16(3)
            .u16(1)
            .u16(0)
            .u16(1)
            .u16(0)
            .u16(0)
            .u16(0)
            .done(),
        vec![(4, 2, cov()), (8, 2, cov())],
    );
    let reverse = link(
        Buf::new()
            .u16(1)
            .u16(0)
            .u16(0)
            .u16(1)
            .u16(0)
            .u16(2)
            .u16(4)
            .u16(5)
            .done(),
        vec![(2, 2, cov()), (8, 2, cov())],
    );
    layout_with(vec![
        lookup(1, 0, vec![single, list], None),
        lookup(2, 0x0010, vec![multiple.clone()], Some(0)),
        lookup(3, 0, vec![multiple], None),
        lookup(4, 0, vec![ligature], None),
        lookup(5, 0, vec![context], None),
        lookup(7, 0, vec![extension(6, chain3)], None),
        lookup(8, 0, vec![reverse], None),
    ])
}

fn gpos_base() -> Vec<u8> {
    let cov = || coverage1(&[1, 2]);
    let anchor = || {
        Buf::new()
            .u16(3)
            .i16(1)
            .i16(2)
            .u16(10)
            .u16(0)
            .bytes(&device(9, 9, 1, &[0x4000]))
            .done()
    };
    let single = link(
        Buf::new()
            .u16(1)
            .u16(0)
            .u16(0x0011)
            .i16(5)
            .u16(8)
            .bytes(&device(9, 9, 1, &[0x4000]))
            .done(),
        vec![(2, 2, cov())],
    );
    let set = Buf::new().u16(1).u16(2).i16(-3).done();
    let pair = link(
        Buf::new()
            .u16(1)
            .u16(0)
            .u16(0x0004)
            .u16(0)
            .u16(2)
            .u16(0)
            .u16(0)
            .done(),
        vec![(2, 2, cov()), (10, 2, set.clone()), (12, 2, set)],
    );
    let pair2 = link(
        Buf::new()
            .u16(2)
            .u16(0)
            .u16(0x0004)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(1)
            .u16(2)
            .i16(1)
            .i16(2)
            .done(),
        vec![(2, 2, cov()), (8, 2, class_def1(1, &[0, 1]))],
    );
    let cursive = link(
        Buf::new()
            .u16(1)
            .u16(0)
            .u16(2)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .done(),
        vec![(2, 2, cov()), (6, 2, anchor()), (12, 2, anchor())],
    );
    let marks = link(
        Buf::new().u16(2).u16(0).u16(0).u16(1).u16(0).done(),
        vec![(4, 2, anchor()), (8, 2, anchor())],
    );
    let matrix = link(
        Buf::new().u16(2).u16(0).u16(0).done(),
        vec![(2, 2, anchor()), (4, 2, anchor())],
    );
    let mark_base = link(
        Buf::new().u16(1).u16(0).u16(0).u16(1).u16(0).u16(0).done(),
        vec![
            (2, 2, cov()),
            (4, 2, cov()),
            (8, 2, marks.clone()),
            (10, 2, matrix.clone()),
        ],
    );
    let ligs = link(
        Buf::new().u16(2).u16(0).u16(0).done(),
        vec![(2, 2, matrix.clone()), (4, 2, matrix.clone())],
    );
    let mark_lig = link(
        Buf::new().u16(1).u16(0).u16(0).u16(1).u16(0).u16(0).done(),
        vec![
            (2, 2, cov()),
            (4, 2, cov()),
            (8, 2, marks.clone()),
            (10, 2, ligs),
        ],
    );
    let mark_mark = link(
        Buf::new().u16(1).u16(0).u16(0).u16(1).u16(0).u16(0).done(),
        vec![(2, 2, cov()), (4, 2, cov()), (8, 2, marks), (10, 2, matrix)],
    );
    let context3 = link(
        Buf::new().u16(3).u16(1).u16(1).u16(0).u16(0).u16(0).done(),
        vec![(6, 2, cov())],
    );
    layout_with(vec![
        lookup(1, 0, vec![single], None),
        lookup(2, 0, vec![pair, pair2], None),
        lookup(3, 0, vec![cursive], None),
        lookup(4, 0, vec![mark_base], None),
        lookup(5, 0, vec![mark_lig], None),
        lookup(6, 0, vec![mark_mark], None),
        lookup(9, 0, vec![extension(7, context3)], None),
    ])
}

fn gdef_base() -> Vec<u8> {
    let attach = link(
        Buf::new().u16(0).u16(1).u16(0).done(),
        vec![
            (0, 2, coverage1(&[7])),
            (4, 2, Buf::new().u16(1).u16(3).done()),
        ],
    );
    let carets = link(
        Buf::new().u16(2).u16(0).u16(0).done(),
        vec![
            (2, 2, Buf::new().u16(1).i16(4).done()),
            (
                4,
                2,
                link(
                    Buf::new().u16(3).i16(1).u16(0).done(),
                    vec![(4, 2, device(9, 9, 2, &[0x1000]))],
                ),
            ),
        ],
    );
    let lig = link(
        Buf::new().u16(0).u16(1).u16(0).done(),
        vec![(0, 2, coverage1(&[8])), (4, 2, carets)],
    );
    let sets = link(
        Buf::new().u16(1).u16(1).u32(0).done(),
        vec![(4, 4, coverage2(&[(3, 4, 0)]))],
    );
    link(
        Buf::new()
            .u32(0x0001_0003)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u16(0)
            .u32(0)
            .done(),
        vec![
            (4, 2, class_def2(&[(1, 9, 1)])),
            (6, 2, attach),
            (8, 2, lig),
            (10, 2, class_def1(3, &[1, 2])),
            (12, 2, sets),
        ],
    )
}

#[test]
fn null_extension_target_has_no_subtable_to_walk() {
    // GPOS fuzz regression 7ea7de07: extension format 1, type 4, null target.
    let mut table = [
        0, 1, 0, 1, 0, 0, 0, 2, 0, 6, 0, 0, 0, 9, 0, 0, 0, 1, 0, 4, 0, 0, 0, 0, 4, 0, 0, 17, 0, 0,
        1, 1,
    ];
    assert!(!has_error(|r| gpos::validate(&table, Some(1986), r)));
    assert!(gpos_walks(&table));
    table[23] = 64;
    assert!(has_error(|r| gpos::validate(&table, Some(1986), r)));
    assert!(!gpos_walks(&table));
}

#[test]
fn gsub_null_extension_allows_reverse_chain_single() {
    let seed = include_bytes!("../../../fuzz/corpus/gsub/null-extension-target");
    assert!(!has_error(|r| gsub::validate(&seed[2..], Some(0x2a3d), r)));
    assert!(gsub_walks(&seed[2..]));
    for offset in [0, 255] {
        let ext = vec![0, 1, 0, 8, 0, 0, 0, offset];
        let table = layout_with(vec![lookup(7, 0, vec![ext], None)]);
        assert_eq!(has_error(|r| gsub::validate(&table, None, r)), offset != 0);
        assert_eq!(gsub_walks(&table), offset == 0);
    }
}

#[test]
fn gsub_fuzz_regression_lookup_offset_not_silent() {
    // CI fuzz (gsub), run 43139595946, 43-byte artifact with a mark-set prefix.
    let input = [
        42, 61, 0, 1, 0, 0, 0, 6, 0, 0, 0, 4, 0, 14, 0, 0, 0, 4, 0, 0, 0, 7, 0, 1, 0, 5, 0, 0, 0,
        0, 0, 2, 0, 0, 0, 0, 0, 78, 0, 0, 0, 0, 0,
    ];
    let table = &input[2..];
    let mut diagnostics = Vec::new();
    gsub::validate(table, Some(0x2a3d), &mut |d| diagnostics.push(d));
    assert!(diagnostics.iter().any(|d| matches!(
        d.kind,
        sofel_read::Kind::ExtensionTypeMismatch {
            expected: 0,
            found: 5
        }
    )));
    assert!(has_error(|r| gsub::validate(table, Some(0x2a3d), r)) || gsub_walks(table));
}

#[test]
fn layout_table_invariants() {
    let mut rng = XorShift(0x5EED_0020_0020_0020);
    let (gsub_t, gpos_t, gdef_t) = (gsub_base(), gpos_base(), gdef_base());
    // The bases are valid and walk.
    assert!(!has_error(|r| gsub::validate(&gsub_t, Some(1), r)));
    assert!(!has_error(|r| gpos::validate(&gpos_t, Some(1), r)));
    assert!(!has_error(|r| gdef::validate(&gdef_t, r)));
    assert!(gsub_walks(&gsub_t) && gpos_walks(&gpos_t) && gdef_walks(&gdef_t));
    for _ in 0..ROUNDS * 4 {
        let d = mutate(&mut rng, &gsub_t);
        if !has_error(|r| gsub::validate(&d, Some(1), r)) {
            assert!(gsub_walks(&d), "{d:02x?}");
        }
        let d = mutate(&mut rng, &gpos_t);
        if !has_error(|r| gpos::validate(&d, Some(1), r)) {
            assert!(gpos_walks(&d), "{d:02x?}");
        }
        let d = mutate(&mut rng, &gdef_t);
        if !has_error(|r| gdef::validate(&d, r)) {
            assert!(gdef_walks(&d), "{d:02x?}");
        }
    }
}

#[test]
fn gpos_mixed_extension_offsets_report_or_walk() {
    let table = [
        0, 1, 0, 1, 0, 0, 0, 2, 0, 6, 0, 0, 0, 9, 0, 1, 0, 3, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1, 1,
    ];
    assert!(has_error(|report| gpos::validate(&table, Some(0x07c2), report)) || gpos_walks(&table));
}
