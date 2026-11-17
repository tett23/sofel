//! FreeType's fixed-point helpers on its 64-bit `FT_Long`, as the
//! automatic hinter uses them.
//!
//! Ported from FreeType 2.15.0's `src/base/ftcalc.c` (`FT_MulFix`,
//! `FT_DivFix`, `FT_MulDiv` with `FT_INT64`, `ft_corner_is_flat`) and
//! `include/freetype/internal/ftcalc.h` (`FT_HYPOT`, `FT_PIX_ROUND`).
//! Copyright (C) 1996-2026 by David Turner, Robert Wilhelm, and Werner
//! Lemberg. Used under the FreeType License (FTL); see
//! THIRD_PARTY_NOTICES.md.

/// `FT_MulFix`: `(a * b + 0x8000 + sign) >> 16` on 64 bits.
pub(super) fn mul_fix(a: i64, b: i64) -> i64 {
    let ab = a.wrapping_mul(b);
    ab.wrapping_add(0x8000).wrapping_add(ab >> 63) >> 16
}

fn split(v: i64, negative: &mut bool) -> u64 {
    if v < 0 {
        *negative = !*negative;
    }
    v.unsigned_abs()
}

fn join(negative: bool, magnitude: u64) -> i64 {
    let v = magnitude.cast_signed();
    if negative { v.wrapping_neg() } else { v }
}

/// `FT_DivFix`: `(a << 16) / b`, rounded on the magnitudes.
pub(super) fn div_fix(a: i64, b: i64) -> i64 {
    let mut negative = false;
    let a = split(a, &mut negative);
    let b = split(b, &mut negative);
    let q = (a << 16)
        .wrapping_add(b >> 1)
        .checked_div(b)
        .unwrap_or(0x7FFF_FFFF);
    join(negative, q)
}

/// `FT_MulDiv`: `a * b / c`, rounded on the magnitudes.
pub(super) fn mul_div(a: i64, b: i64, c: i64) -> i64 {
    let mut negative = false;
    let a = split(a, &mut negative);
    let b = split(b, &mut negative);
    let c = split(c, &mut negative);
    let d = a
        .wrapping_mul(b)
        .wrapping_add(c >> 1)
        .checked_div(c)
        .unwrap_or(0x7FFF_FFFF);
    join(negative, d)
}

/// `FT_PIX_ROUND`.
pub(super) fn pix_round(x: i64) -> i64 {
    x.wrapping_add(32) & !63
}

/// `FT_PIX_FLOOR`.
pub(super) fn pix_floor(x: i64) -> i64 {
    x & !63
}

/// `FT_HYPOT`.
fn hypot(x: i64, y: i64) -> i64 {
    let (x, y) = (x.wrapping_abs(), y.wrapping_abs());
    if x > y {
        x.wrapping_add((3 * y) >> 3)
    } else {
        y.wrapping_add((3 * x) >> 3)
    }
}

/// `ft_corner_is_flat`.
pub(super) fn corner_is_flat(in_x: i64, in_y: i64, out_x: i64, out_y: i64) -> bool {
    let ax = in_x.wrapping_add(out_x);
    let ay = in_y.wrapping_add(out_y);
    let d_in = hypot(in_x, in_y);
    let d_out = hypot(out_x, out_y);
    let d_hypot = hypot(ax, ay);
    d_in.wrapping_add(d_out).wrapping_sub(d_hypot) < (d_hypot >> 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0064_i1_fixed_point_follows_freetype_64_bit_long() {
        assert_eq!(mul_fix(0x18000, 3), 5);
        assert_eq!(mul_fix(-0x18000, 3), -5);
        assert_eq!(div_fix(1, 0), 0x7FFF_FFFF);
        assert_eq!(div_fix(-1, 0), -0x7FFF_FFFF);
        assert_eq!(div_fix(64, 1 << 16), 64);
        assert_eq!(mul_div(5, -3, 2), -8);
        assert_eq!(pix_round(-33), -64);
        assert!(corner_is_flat(100, 0, 100, 1));
        assert!(!corner_is_flat(100, 0, 0, 100));
    }
}
