//! MATH tables at the acceptance boundary of HarfBuzz's sanitizer
//! (ADR 0091), shared by the sofel-font tests, the HarfBuzz comparison
//! of sofel-conformance and the C ABI fixture.

#![allow(
    dead_code,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use super::build::*;

/// The MATH header: version `major`.0 and the three subtable offsets.
pub fn header(major: u16, constants: u16, glyph_info: u16, variants: u16) -> Buf {
    Buf::new()
        .u16(major)
        .u16(0)
        .u16(constants)
        .u16(glyph_info)
        .u16(variants)
}

/// Version 1.0, every offset null.
pub fn empty() -> Vec<u8> {
    header(1, 0, 0, 0).done()
}

/// The first nine bytes of [`empty`].
pub fn nine_bytes() -> Vec<u8> {
    let mut t = empty();
    t.truncate(9);
    t
}

/// [`empty`] with major version 2.
pub fn major_two() -> Vec<u8> {
    header(2, 0, 0, 0).done()
}

/// Coverage format 1 over `glyphs`.
pub fn coverage1(glyphs: &[u16]) -> Vec<u8> {
    glyphs
        .iter()
        .fold(Buf::new().u16(1).u16(glyphs.len() as u16), |b, g| b.u16(*g))
        .done()
}

/// Coverage format 2 over `(first, last)` ranges.
pub fn coverage2(ranges: &[(u16, u16)]) -> Vec<u8> {
    let mut index = 0u16;
    ranges
        .iter()
        .fold(Buf::new().u16(2).u16(ranges.len() as u16), |b, (f, l)| {
            let b = b.u16(*f).u16(*l).u16(index);
            index += l - f + 1;
            b
        })
        .done()
}

/// A Coverage of a format HarfBuzz does not know (accepted, empty).
pub fn coverage_unknown() -> Vec<u8> {
    Buf::new().u16(7).u16(1).u16(3).done()
}

/// Format 1 claiming 200 glyphs, more than the table holds.
pub fn coverage_truncated() -> Vec<u8> {
    Buf::new().u16(1).u16(200).u16(3).done()
}

/// A MATH table whose MathGlyphInfo has only an ExtendedShapeCoverage
/// (`coverage`), and an empty MathVariants after it.
pub fn extended(coverage: &[u8]) -> Vec<u8> {
    // Header 10, MathGlyphInfo 8, coverage, MathVariants 10.
    let variants = 18 + coverage.len() as u16;
    let mut t = header(1, 0, 10, variants)
        .u16(0)
        .u16(0)
        .u16(8)
        .u16(0)
        .bytes(coverage)
        .done();
    t.extend_from_slice(&[0, 50, 0, 0, 0, 0, 0, 0, 0, 0]);
    t
}

/// [`extended`] of glyph 3 with the header offset at `field` (4, 6 or 8)
/// pointing past the end of the table.
pub fn out_of_range(field: usize) -> Vec<u8> {
    let mut t = extended(&coverage1(&[3]));
    t[field..field + 2].copy_from_slice(&0xFFF0u16.to_be_bytes());
    t
}

/// A MathVariants whose `count` vertical constructions all share one
/// MathGlyphConstruction with `variants` variant records: HarfBuzz checks
/// the shared records once per offset against its operation budget of
/// 64 per byte of the table.
pub fn shared_constructions(count: u16, variants: u16) -> Vec<u8> {
    let variants_at = 10u16;
    let coverage_at = 10 + 2 * count; // relative to MathVariants
    let coverage = coverage2(&[(1, count)]);
    let construction_at = coverage_at + coverage.len() as u16;
    let mut b = header(1, 0, 0, variants_at)
        .u16(0)
        .u16(coverage_at)
        .u16(0)
        .u16(count)
        .u16(0);
    for _ in 0..count {
        b = b.u16(construction_at);
    }
    b = b.bytes(&coverage).u16(0).u16(variants);
    for i in 0..variants {
        b = b.u16(i).u16(100);
    }
    b.done()
}

/// A font of `sample_tables` with `math` as its MATH table, or none.
pub fn font(math: Option<Vec<u8>>) -> Vec<u8> {
    let mut tables = sample_tables();
    if let Some(math) = math {
        tables.push((b"MATH", math));
    }
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}
