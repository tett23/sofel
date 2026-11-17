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

/// Move the two edges of a single unlocked horizontal stem together so the
/// nearer edge reaches a pixel boundary (FreeType pshalgo.c:418-430).
pub(super) fn single_hstem_shift(position: i64, width: i64, scale: Fixed) -> Option<i64> {
    if width <= 0 {
        return None;
    }
    let upper = position.checked_add(width)?;
    let factor = i64::from(path_scale(scale));
    let lower_ds = sofel_font::read::cff::ft::mul_fix(position, factor);
    let upper_ds = sofel_font::read::cff::ft::mul_fix(upper, factor);
    if upper_ds <= lower_ds {
        return None;
    }
    let lower_frac = lower_ds.rem_euclid(65536);
    let upper_frac = upper_ds.rem_euclid(65536);
    let move_up = (if lower_frac == 0 {
        0
    } else {
        65536_i64.saturating_sub(lower_frac)
    })
    .min(if upper_frac == 0 {
        0
    } else {
        65536_i64.saturating_sub(upper_frac)
    });
    let move_down = lower_frac.saturating_neg().max(upper_frac.saturating_neg());
    Some(if move_down.saturating_neg() < move_up {
        move_down
    } else {
        move_up
    })
}

pub(super) fn coordinate_fixed_shifted(value: i64, scale: Fixed, shift: i64) -> i32 {
    let scaled = sofel_font::read::cff::ft::mul_fix(value, i64::from(path_scale(scale)));
    sofel_font::read::cff::ft::to_i32(scaled.saturating_add(shift) >> 10)
}

pub(super) fn advance(value: i32, scale: Fixed) -> i32 {
    let scaled = sofel_geom::mul_fix(value, scale.0);
    scaled.wrapping_add(32) & !63
}
