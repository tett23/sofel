//! The MATH table (ADR 0022): the view, the validator and its ADR 0007
//! invariants, on a table built by hand.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

mod support;

use sofel_read::math::{self, Corner, GlyphPart, Math, MathValue};
use sofel_read::sfnt::Sfnt;
use sofel_read::{Diagnostic, GlyphId, ReadError, Severity, validate};
use support::build::*;

fn collect(data: &[u8], glyphs: Option<u32>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    math::validate(data, glyphs, &mut |d| out.push(d));
    out
}

fn kinds(data: &[u8], glyphs: Option<u32>) -> Vec<(&'static str, Severity, u32)> {
    collect(data, glyphs)
        .iter()
        .map(|d| (d.kind.id(), d.severity, d.offset))
        .collect()
}

/// Absolute positions of the parts of [`sample`].
const CONSTANTS: usize = 10;
const DEVICE: usize = 224;
const GLYPH_INFO: usize = 232;
const ITALICS: usize = 240;
const ITALICS_COV: usize = 252;
const ACCENT: usize = 260;
const ACCENT_COV: usize = 268;
const EXTENDED: usize = 274;
const KERN_INFO: usize = 284;
const KERN_COV: usize = 296;
const KERN: usize = 302;
const VARIANTS: usize = 316;
const VERT_COV: usize = 330;
const HORIZ_COV: usize = 336;
const CONSTRUCTION0: usize = 342;
const ASSEMBLY: usize = 354;
const CONSTRUCTION1: usize = 380;
const END: usize = 388;

fn off(to: usize, from: usize) -> u16 {
    (to - from) as u16
}

/// A MATH table with every subtable: italics corrections for glyphs 3
/// and 5, a top accent for 5, extended shapes 7 and 8, a top right kern
/// for 3, a vertical construction for 10 (variants 12 and 13, parts 14
/// and 15) and a horizontal one for 11 (variant 16).
fn sample() -> Vec<u8> {
    let mut b = Buf::new()
        .u32(0x0001_0000)
        .u16(off(CONSTANTS, 0))
        .u16(off(GLYPH_INFO, 0))
        .u16(off(VARIANTS, 0));
    // MathConstants.
    b = b.i16(80).i16(60).u16(1500).u16(2000);
    for i in 0..51u16 {
        let device = if i == 0 { off(DEVICE, CONSTANTS) } else { 0 };
        b = b.i16(i.cast_signed() * 10).u16(device);
    }
    b = b.i16(55);
    assert_eq!(b.len(), DEVICE);
    b = b.u16(12).u16(12).u16(1).u16(0x4000);
    assert_eq!(b.len(), GLYPH_INFO);
    b = b
        .u16(off(ITALICS, GLYPH_INFO))
        .u16(off(ACCENT, GLYPH_INFO))
        .u16(off(EXTENDED, GLYPH_INFO))
        .u16(off(KERN_INFO, GLYPH_INFO));
    b = b
        .u16(off(ITALICS_COV, ITALICS))
        .u16(2)
        .i16(30)
        .u16(0)
        .i16(-40)
        .u16(0);
    b = b.u16(1).u16(2).u16(3).u16(5);
    assert_eq!(b.len(), ACCENT);
    b = b.u16(off(ACCENT_COV, ACCENT)).u16(1).i16(250).u16(0);
    b = b.u16(1).u16(1).u16(5);
    assert_eq!(b.len(), EXTENDED);
    b = b.u16(2).u16(1).u16(7).u16(8).u16(0);
    assert_eq!(b.len(), KERN_INFO);
    b = b
        .u16(off(KERN_COV, KERN_INFO))
        .u16(1)
        .u16(off(KERN, KERN_INFO))
        .u16(0)
        .u16(0)
        .u16(0);
    b = b.u16(1).u16(1).u16(3);
    assert_eq!(b.len(), KERN);
    b = b.u16(1).i16(100).u16(0).i16(-10).u16(0).i16(-20).u16(0);
    assert_eq!(b.len(), VARIANTS);
    b = b
        .u16(50)
        .u16(off(VERT_COV, VARIANTS))
        .u16(off(HORIZ_COV, VARIANTS))
        .u16(1)
        .u16(1)
        .u16(off(CONSTRUCTION0, VARIANTS))
        .u16(off(CONSTRUCTION1, VARIANTS));
    b = b.u16(1).u16(1).u16(10);
    b = b.u16(1).u16(1).u16(11);
    assert_eq!(b.len(), CONSTRUCTION0);
    b = b
        .u16(off(ASSEMBLY, CONSTRUCTION0))
        .u16(2)
        .u16(12)
        .u16(1000)
        .u16(13)
        .u16(1500);
    assert_eq!(b.len(), ASSEMBLY);
    b = b.i16(5).u16(0).u16(2);
    b = b.u16(14).u16(0).u16(100).u16(500).u16(0);
    b = b.u16(15).u16(100).u16(0).u16(300).u16(1);
    assert_eq!(b.len(), CONSTRUCTION1);
    b = b.u16(0).u16(1).u16(16).u16(800);
    assert_eq!(b.len(), END);
    b.done()
}

fn value(v: Option<MathValue<'_>>) -> (i16, u16) {
    let v = v.unwrap();
    (v.value(), v.device_offset())
}

#[test]
fn header_and_constants() {
    let d = sample();
    let m = Math::new(&d).unwrap();
    assert_eq!(m.version(), 0x0001_0000);
    assert_eq!(
        (
            m.constants_offset(),
            m.glyph_info_offset(),
            m.variants_offset()
        ),
        (10, 232, 316)
    );
    let c = m.constants().unwrap().unwrap();
    assert_eq!(c.script_percent_scale_down(), 80);
    assert_eq!(c.script_script_percent_scale_down(), 60);
    assert_eq!(c.delimited_sub_formula_min_height(), 1500);
    assert_eq!(c.display_operator_min_height(), 2000);
    assert_eq!(c.radical_degree_bottom_raise_percent(), 55);
    assert_eq!(value(c.value(0)), (0, 214));
    assert_eq!(value(c.value(50)), (500, 0));
    assert!(c.value(51).is_none());
    let dev = c.value(0).unwrap().device().unwrap().unwrap();
    assert_eq!(
        (dev.start_size(), dev.end_size(), dev.delta_format()),
        (12, 12, 1)
    );
    assert_eq!(dev.delta_pixels(12), 1);
    assert!(c.value(1).unwrap().device().unwrap().is_none());
}

#[test]
fn glyph_info() {
    let d = sample();
    let gi = Math::new(&d).unwrap().glyph_info().unwrap().unwrap();
    let it = gi.italics_correction().unwrap().unwrap();
    assert_eq!(it.len(), 2);
    assert_eq!(value(it.get(GlyphId(3))), (30, 0));
    assert_eq!(value(it.get(GlyphId(5))), (-40, 0));
    assert!(it.get(GlyphId(4)).is_none());
    assert!(it.value(2).is_none());
    let ta = gi.top_accent_attachment().unwrap().unwrap();
    assert_eq!(value(ta.get(GlyphId(5))), (250, 0));
    let ext = gi.extended_shape_coverage().unwrap().unwrap();
    assert_eq!(ext.get(GlyphId(8)), Some(1));
    assert_eq!(ext.get(GlyphId(9)), None);
    let ki = gi.kern_info().unwrap().unwrap();
    assert_eq!(ki.len(), 1);
    assert_eq!(ki.coverage().unwrap().unwrap().get(GlyphId(3)), Some(0));
    let k = ki.kern(0, Corner::TopRight).unwrap().unwrap();
    assert_eq!(k.height_count(), 1);
    assert_eq!(value(k.correction_height(0)), (100, 0));
    assert!(k.correction_height(1).is_none());
    assert_eq!(value(k.kern_value(0)), (-10, 0));
    assert_eq!(value(k.kern_value(1)), (-20, 0));
    assert!(k.kern_value(2).is_none());
    assert!(ki.kern(0, Corner::BottomLeft).unwrap().is_none());
    assert_eq!(
        ki.kern(1, Corner::TopRight).unwrap_err(),
        ReadError::OutOfBounds
    );
}

#[test]
fn variants() {
    let d = sample();
    let v = Math::new(&d).unwrap().variants().unwrap().unwrap();
    assert_eq!(v.min_connector_overlap(), 50);
    assert_eq!((v.vert_glyph_count(), v.horiz_glyph_count()), (1, 1));
    assert_eq!(
        v.vert_coverage().unwrap().unwrap().get(GlyphId(10)),
        Some(0)
    );
    assert_eq!(
        v.horiz_coverage().unwrap().unwrap().get(GlyphId(11)),
        Some(0)
    );
    let c = v.vert_construction(0).unwrap().unwrap();
    assert_eq!(c.num_variants(), 2);
    assert_eq!(c.variant(1), Some((GlyphId(13), 1500)));
    assert_eq!(c.variant(2), None);
    let a = c.glyph_assembly().unwrap().unwrap();
    assert_eq!(a.italics_correction().value(), 5);
    assert_eq!(a.num_parts(), 2);
    assert_eq!(
        a.part(1),
        Some(GlyphPart {
            glyph: GlyphId(15),
            start_connector_length: 100,
            end_connector_length: 0,
            full_advance: 300,
            flags: math::PART_EXTENDER,
        })
    );
    let h = v.horiz_construction(0).unwrap().unwrap();
    assert!(h.glyph_assembly().unwrap().is_none());
    assert_eq!(h.variant(0), Some((GlyphId(16), 800)));
    assert_eq!(v.vert_construction(1).unwrap_err(), ReadError::OutOfBounds);
    assert_eq!(v.horiz_construction(1).unwrap_err(), ReadError::OutOfBounds);
}

#[test]
fn bad_headers() {
    let mut d = sample();
    d[1] = 2;
    assert_eq!(
        Math::new(&d).unwrap_err(),
        ReadError::UnknownVersion(0x0002_0000)
    );
    assert_eq!(
        kinds(&d, None),
        vec![("unknown_version", Severity::Error, 0)]
    );
    let d = sample();
    assert!(matches!(
        Math::new(&d[..9]),
        Err(ReadError::Truncated { .. })
    ));
    assert_eq!(
        kinds(&d[..9], None),
        vec![("truncated", Severity::Error, 0)]
    );
}

#[test]
fn valid_table_has_no_diagnostics() {
    assert_eq!(kinds(&sample(), Some(17)), vec![]);
    assert_eq!(kinds(&sample(), None), vec![]);
}

#[test]
fn glyphs_out_of_range_are_warnings() {
    // Glyphs 15 (a part) and 16 (a variant) are not below 15.
    assert_eq!(
        kinds(&sample(), Some(15)),
        vec![
            (
                "glyph_id_out_of_range",
                Severity::Warning,
                (ASSEMBLY + 16) as u32
            ),
            (
                "glyph_id_out_of_range",
                Severity::Warning,
                (CONSTRUCTION1 + 4) as u32
            ),
        ]
    );
}

#[test]
fn reserved_part_flags_are_warnings() {
    let mut d = sample();
    d[ASSEMBLY + 25] = 3;
    assert_eq!(
        kinds(&d, None),
        vec![(
            "reserved_bits_set",
            Severity::Warning,
            (ASSEMBLY + 24) as u32
        )]
    );
}

#[test]
fn offsets_out_of_bounds() {
    // The variants offset points past the end.
    let mut d = sample();
    d[8..10].copy_from_slice(&u16be(END as u16));
    assert_eq!(
        kinds(&d, None),
        vec![("offset_out_of_bounds", Severity::Error, 8)]
    );
    assert!(Math::new(&d).unwrap().variants().is_err());
    // The device of the second constant points past the end.
    let mut d = sample();
    d[CONSTANTS + 14..CONSTANTS + 16].copy_from_slice(&u16be(400));
    assert_eq!(
        kinds(&d, None),
        vec![(
            "offset_out_of_bounds",
            Severity::Error,
            (CONSTANTS + 14) as u32
        )]
    );
    // The MathConstants table does not fit.
    let mut d = sample();
    d[4..6].copy_from_slice(&u16be(300));
    assert_eq!(
        kinds(&d, None),
        vec![("offset_out_of_bounds", Severity::Error, 4)]
    );
    assert!(matches!(
        Math::new(&d).unwrap().constants(),
        Err(ReadError::Truncated { .. })
    ));
}

#[test]
fn coverage_count_mismatches() {
    // The italics coverage lists one glyph for two records.
    let mut d = sample();
    d[ITALICS_COV + 3] = 1;
    assert_eq!(
        kinds(&d, None),
        vec![(
            "coverage_count_mismatch",
            Severity::Error,
            (ITALICS + 2) as u32
        )]
    );
    // vertGlyphCount of 2 with one covered glyph (the horizontal
    // construction is then read as the second vertical one).
    let mut d = sample();
    d[VARIANTS + 7] = 2;
    d[VARIANTS + 9] = 0;
    assert_eq!(
        kinds(&d, None),
        vec![
            (
                "coverage_count_mismatch",
                Severity::Error,
                (VARIANTS + 6) as u32
            ),
            (
                "coverage_count_mismatch",
                Severity::Error,
                (VARIANTS + 8) as u32
            ),
        ]
    );
}

#[test]
fn coverage_order() {
    let mut d = sample();
    // The italics coverage [5, 3].
    d[ITALICS_COV + 5] = 5;
    d[ITALICS_COV + 7] = 3;
    assert_eq!(
        kinds(&d, None),
        vec![(
            "coverage_not_sorted",
            Severity::Error,
            (ITALICS_COV + 6) as u32
        )]
    );
}

#[test]
fn whole_font_validation_includes_math() {
    let mut tables = sample_tables();
    let mut bad = sample();
    bad[ASSEMBLY + 25] = 2;
    tables.push((b"MATH", bad));
    tables.sort_by_key(|t| *t.0);
    let file = sfnt(&tables);
    let font = Sfnt::new(&file).unwrap();
    let mut out = Vec::new();
    validate::sfnt(&font, &mut |d| out.push(d));
    assert!(
        out.iter()
            .any(|d| d.table == math::TAG && d.kind.id() == "reserved_bits_set"),
        "{out:?}"
    );
}

// ---- ADR 0007 invariants on mutated tables ----

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
    match rng.below(3) {
        0 => {
            let n = rng.below(d.len() + 1);
            d.truncate(n);
        }
        1 => {
            for _ in 0..=rng.below(4) {
                let i = rng.below(d.len());
                d[i] = rng.next() as u8;
            }
        }
        _ => {
            // Small changes to the offsets and counts (even positions).
            let i = rng.below(d.len() / 2) * 2;
            let v = u16::from_be_bytes([d[i], d[i + 1]]).wrapping_add(rng.below(5) as u16);
            d[i..i + 2].copy_from_slice(&v.to_be_bytes());
        }
    }
    d
}

fn ok<T>(r: Result<Option<T>, ReadError>, f: impl FnOnce(T) -> bool) -> bool {
    match r {
        Err(_) => false,
        Ok(None) => true,
        Ok(Some(t)) => f(t),
    }
}

fn device(v: Option<MathValue<'_>>) -> bool {
    v.is_some_and(|v| v.device().is_ok())
}

/// Every read of the table; false when an offset cannot be followed or
/// an in-range read is `None`.
fn walks(d: &[u8]) -> bool {
    let Ok(m) = Math::new(d) else { return false };
    let constants = ok(m.constants(), |c| {
        (0..math::NUM_CONSTANT_VALUES).all(|i| device(c.value(i)))
    });
    let info = ok(m.glyph_info(), |gi| {
        let table = |t| {
            ok(t, |t: math::MathValueTable<'_>| {
                let n = ok(t.coverage(), |c| {
                    c.ranges()
                        .flat_map(|(a, z)| a.0..=z.0)
                        .all(|g| t.get(GlyphId(g)).is_some())
                });
                n && (0..t.len()).all(|i| device(t.value(i)))
            })
        };
        table(gi.italics_correction())
            && table(gi.top_accent_attachment())
            && gi.extended_shape_coverage().is_ok()
            && ok(gi.kern_info(), |k| {
                k.coverage().is_ok()
                    && (0..k.len()).all(|i| {
                        [
                            Corner::TopRight,
                            Corner::TopLeft,
                            Corner::BottomRight,
                            Corner::BottomLeft,
                        ]
                        .into_iter()
                        .all(|c| {
                            ok(k.kern(i, c), |k| {
                                (0..k.height_count()).all(|j| device(k.correction_height(j)))
                                    && (0..=k.height_count()).all(|j| device(k.kern_value(j)))
                            })
                        })
                    })
            })
    });
    let variants = ok(m.variants(), |v| {
        let construction = |c| {
            ok(c, |c: math::MathGlyphConstruction<'_>| {
                (0..c.num_variants()).all(|i| c.variant(i).is_some())
                    && ok(c.glyph_assembly(), |a| {
                        a.italics_correction().device().is_ok()
                            && (0..a.num_parts()).all(|i| a.part(i).is_some())
                    })
            })
        };
        v.vert_coverage().is_ok()
            && v.horiz_coverage().is_ok()
            && (0..v.vert_glyph_count()).all(|i| construction(v.vert_construction(i)))
            && (0..v.horiz_glyph_count()).all(|i| construction(v.horiz_construction(i)))
    });
    constants && info && variants
}

#[test]
fn invariants() {
    let base = sample();
    assert!(walks(&base));
    let mut rng = XorShift(0x5EED_0022_0008_0008);
    for _ in 0..20000 {
        let d = mutate(&mut rng, &base);
        let error = collect(&d, Some(17))
            .iter()
            .any(|x| x.severity == Severity::Error);
        if !error {
            assert!(walks(&d), "{d:02x?}");
        }
    }
}
