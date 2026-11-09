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
