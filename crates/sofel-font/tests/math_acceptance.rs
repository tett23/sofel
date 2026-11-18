//! ADR 0091: MATH tables are accepted exactly when HarfBuzz 15.1.0's
//! sanitizer accepts them (the rows of the ADR's invariant table; the
//! HarfBuzz values were measured with `hb_ot_math_has_data` and
//! `hb_ot_math_is_glyph_extended_shape`, and sofel-conformance compares
//! the same tables with `hb-dump --math`).

#![allow(
    clippy::all,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;
#[path = "support/math_acceptance_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/layout_query_fixture.rs"]
mod layout_query_fixture;

use fixture::*;
use sofel_font::read::GlyphId;
use sofel_font::read::math::Corner;
use sofel_font::{Direction, Font, HbScale};

/// `sample_tables` has 28 glyphs; the probes add the first id past them
/// and 0xFFFF.
const PROBES: [u32; 30] = {
    let mut p = [0u32; 30];
    let mut i = 0;
    while i < 29 {
        p[i] = i as u32;
        i += 1;
    }
    p[29] = 0xFFFF;
    p
};

/// `has_math` and the glyphs among [`PROBES`] that are extended shapes.
fn observe(math: Option<Vec<u8>>) -> (bool, Vec<u32>) {
    let data = font(math);
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 0, 0);
    let extended = PROBES
        .iter()
        .copied()
        .filter(|g| query.math_is_extended_shape(GlyphId(*g)))
        .collect();
    (query.has_math(), extended)
}

#[test]
fn adr0091_i1_no_math_table() {
    assert_eq!(observe(None), (false, vec![]));
}

#[test]
fn adr0091_i1_short_header_and_major_two_are_rejected() {
    assert_eq!(observe(Some(nine_bytes())), (false, vec![]));
    assert_eq!(observe(Some(major_two())), (false, vec![]));
}

#[test]
fn adr0091_i1_null_offsets_are_accepted() {
    assert_eq!(observe(Some(empty())), (true, vec![]));
}

#[test]
fn adr0091_i2_extended_shape_coverage_formats() {
    assert_eq!(observe(Some(extended(&coverage1(&[3])))), (true, vec![3]));
    assert_eq!(
        observe(Some(extended(&coverage2(&[(3, 3)])))),
        (true, vec![3])
    );
    assert_eq!(
        observe(Some(extended(&coverage2(&[(3, 5), (28, 28)])))),
        (true, vec![3, 4, 5, 28])
    );
    assert_eq!(observe(Some(extended(&coverage1(&[])))), (true, vec![]));
    // HarfBuzz accepts an unknown Coverage format as an empty set.
    assert_eq!(observe(Some(extended(&coverage_unknown()))), (true, vec![]));
}

#[test]
fn adr0091_i2_glyph_ids_past_the_font_are_not_extended() {
    let (_, extended) = observe(Some(extended(&coverage1(&[3]))));
    assert!(!extended.contains(&28) && !extended.contains(&0xFFFF));
}

#[test]
fn adr0091_i1_out_of_range_math_variants_rejects_the_table() {
    assert_eq!(observe(Some(out_of_range(8))), (false, vec![]));
}

#[test]
fn adr0091_i1_out_of_range_subtables_reject_the_table() {
    assert_eq!(observe(Some(out_of_range(4))), (false, vec![]));
    assert_eq!(observe(Some(out_of_range(6))), (false, vec![]));
    assert_eq!(
        observe(Some(extended(&coverage_truncated()))),
        (false, vec![])
    );
}

#[test]
fn adr0091_i1_operation_budget_rejects_shared_offsets() {
    assert!(observe(Some(shared_constructions(10, 200))).0);
    assert!(!observe(Some(shared_constructions(200, 200))).0);
}

#[test]
fn adr0091_i1_every_math_query_shares_the_acceptance() {
    // The ADR 0062 fixture with its MathVariants offset out of range.
    let mut math = layout_query_fixture::sample();
    math[8..10].copy_from_slice(&0xFFF0u16.to_be_bytes());
    let data = font(Some(math));
    let font = Font::new(&data, 0).unwrap();
    let query = font
        .default_instance()
        .layout_queries(HbScale::unscaled(&font), 12, 16);
    assert!(!query.has_math());
    assert_eq!(query.math_constant(4), 0);
    assert_eq!(query.math_italics_correction(GlyphId(3)), 0);
    assert_eq!(query.math_top_accent_attachment(GlyphId(5)), 252);
    assert!(!query.math_is_extended_shape(GlyphId(7)));
    assert!(query.math_kernings(GlyphId(3), Corner::TopRight).is_empty());
    assert_eq!(query.math_min_connector_overlap(Direction::Vertical), 0);
    assert!(
        query
            .math_variants(GlyphId(10), Direction::Vertical)
            .is_empty()
    );
    assert!(
        query
            .math_assembly(GlyphId(10), Direction::Vertical)
            .parts
            .is_empty()
    );
}
