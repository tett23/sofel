//! `FT_Vector_Length` by CORDIC.
//!
//! Ported from FreeType 2.15.0 (commit 37515cf56ba3349656049edbed89ffc6f6466f2d),
//! `src/base/fttrigon.c` (`ft_trig_downscale`, `ft_trig_prenorm`,
//! `ft_trig_pseudo_polarize`, `FT_Vector_Length`) and `src/base/ftcalc.c`
//! (`FT_MSB`). Copyright (C) 2001-2026 by David Turner, Robert Wilhelm,
//! and Werner Lemberg. Used under the FreeType License (FTL); see
//! THIRD_PARTY_NOTICES.md. Computed in i64 like FreeType's `long` on LP64.

/// The CORDIC shrink factor 0.858785336480436 * 2^32.
const TRIG_SCALE: u64 = 0xDBD9_5B16;
/// The highest bit in overflow-safe vector components.
const TRIG_SAFE_MSB: u32 = 29;
const ARCTAN_TABLE_LEN: u32 = 22;

/// `ft_trig_downscale`.
fn downscale(val: i64) -> i64 {
    let magnitude = val.unsigned_abs();
    let scaled = magnitude.wrapping_mul(TRIG_SCALE).wrapping_add(0x4000_0000) >> 32;
    let scaled = i64::try_from(scaled).unwrap_or(i64::MAX);
    if val < 0 {
        scaled.wrapping_neg()
    } else {
        scaled
    }
}

/// `ft_trig_prenorm`: scales the vector so that its largest component
/// has its top bit at bit 29. Returns the scaled vector and the shift
/// (positive: shifted left).
fn prenorm(x: i64, y: i64) -> (i64, i64, i32) {
    let z = u32::try_from(x.unsigned_abs() | y.unsigned_abs()).unwrap_or(u32::MAX);
    // FT_MSB: the index of the highest set bit (z is never 0 here).
    let msb = 31u32.saturating_sub(z.leading_zeros());
    if msb <= TRIG_SAFE_MSB {
        let s = TRIG_SAFE_MSB.saturating_sub(msb);
        (
            x.wrapping_shl(s),
            y.wrapping_shl(s),
            i32::try_from(s).unwrap_or(0),
        )
    } else {
        let s = msb.saturating_sub(TRIG_SAFE_MSB);
        (
            x >> s,
            y >> s,
            i32::try_from(s).map_or(0, i32::wrapping_neg),
        )
    }
}

/// `ft_trig_pseudo_polarize`, keeping only the length (the angle is not
/// needed here).
fn pseudo_polarize(x: i64, y: i64) -> i64 {
    // Get the vector into the [-PI/4, PI/4] sector.
    let (mut x, mut y) = if y > x {
        if y > x.wrapping_neg() {
            (y, x.wrapping_neg())
        } else {
            (x.wrapping_neg(), y.wrapping_neg())
        }
    } else if y < x.wrapping_neg() {
        (y.wrapping_neg(), x)
    } else {
        (x, y)
    };
    // Pseudorotations, with right shifts.
    let mut b: i64 = 1;
    for i in 1..=ARCTAN_TABLE_LEN {
        let dx = y.wrapping_add(b) >> i;
        let dy = x.wrapping_add(b) >> i;
        if y > 0 {
            x = x.wrapping_add(dx);
            y = y.wrapping_sub(dy);
        } else {
            x = x.wrapping_sub(dx);
            y = y.wrapping_add(dy);
        }
        b = b.wrapping_shl(1);
    }
    x
}

/// `FT_Vector_Length` (and `FT_Hypot`): the length of `(x, y)`.
pub fn vector_length(x: i32, y: i32) -> i32 {
    let saturate = |v: i64| i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
    if x == 0 {
        return saturate(i64::from(y).abs());
    }
    if y == 0 {
        return saturate(i64::from(x).abs());
    }
    let (vx, vy, shift) = prenorm(i64::from(x), i64::from(y));
    let len = downscale(pseudo_polarize(vx, vy));
    if shift > 0 {
        let s = shift.unsigned_abs();
        saturate(len.wrapping_add(1i64.wrapping_shl(s.saturating_sub(1))) >> s)
    } else {
        // (FT_UInt32)v.x << -shift
        let low = u32::try_from(len & 0xFFFF_FFFF).unwrap_or(0);
        saturate(i64::from(low.wrapping_shl(shift.unsigned_abs())))
    }
}
