//! Common Adobe PostScript path scale for CFF, Type 1, and CID outlines.

use sofel_geom::Fixed;

/// FreeType `cf2_getScaleAndHintFlag` in `psaux/psft.c`.
pub(super) fn path_scale(scale: Fixed) -> i32 {
    scale.0.wrapping_add(32) / 64
}

/// FreeType's 16.16 Adobe path callback converts to 26.6 by shifting ten bits.
pub(super) fn coordinate(value: i32, scale: Fixed) -> i32 {
    let product = i64::from(value).saturating_mul(i64::from(path_scale(scale)));
    sofel_font::read::cff::ft::to_i32(product >> 10)
}

/// Keep Type 1 fractional charstring coordinates until the Adobe path callback.
pub(super) fn coordinate_fixed(value: i64, scale: Fixed) -> i32 {
    let scaled = sofel_font::read::cff::ft::mul_fix(value, i64::from(path_scale(scale)));
    sofel_font::read::cff::ft::to_i32(scaled >> 10)
}

pub(super) fn advance(value: i32, scale: Fixed) -> i32 {
    let scaled = sofel_geom::mul_fix(value, scale.0);
    scaled.wrapping_add(32) & !63
}
