//! Fixed-point numbers and FreeType's rounding arithmetic.
//!
//! Ported from FreeType 2.15.0 (commit 37515cf56ba3349656049edbed89ffc6f6466f2d),
//! `src/base/ftcalc.c` and `include/freetype/internal/ftcalc.h`
//! (`FT_MulFix`, `FT_DivFix`, `FT_MulDiv`, `FT_MulDiv_No_Round`) and
//! `src/base/ftoutln.c` (`FT_Vector_Transform`). Copyright (C) 1996-2026 by
//! David Turner, Robert Wilhelm, and Werner Lemberg. Used under the
//! FreeType License (FTL); see THIRD_PARTY_NOTICES.md.

/// A 26.6 fixed-point number (1/64 of a pixel).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct F26Dot6(pub i32);

impl F26Dot6 {
    pub const fn from_int(v: i32) -> F26Dot6 {
        F26Dot6(v.saturating_mul(64))
    }
}

/// A 16.16 fixed-point number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Fixed(pub i32);

impl Fixed {
    pub const ONE: Fixed = Fixed(0x1_0000);
}

/// A 2.14 fixed-point number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct F2Dot14(pub i16);

/// As FreeType does: the 2.14 value times 4.
impl From<F2Dot14> for Fixed {
    fn from(v: F2Dot14) -> Fixed {
        Fixed(i32::from(v.0).saturating_mul(4))
    }
}

/// A point. The unit is set by whoever makes it: 26.6 for scaled
/// outlines, font units for unscaled ones.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A 2x2 matrix of 16.16 numbers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Matrix {
    pub xx: Fixed,
    pub xy: Fixed,
    pub yx: Fixed,
    pub yy: Fixed,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        xx: Fixed::ONE,
        xy: Fixed(0),
        yx: Fixed(0),
        yy: Fixed::ONE,
    };
}

fn saturate(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
}

/// The sign of a product or quotient of `a`, `b` (and `c`), and their
/// magnitudes, as `FT_MOVE_SIGN` gives them.
fn negative(values: &[i32]) -> bool {
    values.iter().filter(|v| **v < 0).count() % 2 == 1
}

fn signed(negative: bool, magnitude: u64) -> i32 {
    let m = i64::try_from(magnitude).unwrap_or(i64::MAX);
    saturate(if negative { m.wrapping_neg() } else { m })
}

/// `FT_MulFix`: `(a * b) / 0x10000`, with halves rounded away from zero.
#[inline]
pub fn mul_fix(a: i32, b: i32) -> i32 {
    let ab = i64::from(a).wrapping_mul(i64::from(b));
    let ab = ab.wrapping_add(0x8000).wrapping_add(ab >> 63);
    saturate(ab >> 16)
}

/// `FT_DivFix`: `(a * 0x10000) / b`, rounded on the magnitudes; for
/// `b == 0`, `0x7FFFFFFF` with the sign of `a`.
#[inline]
pub fn div_fix(a: i32, b: i32) -> i32 {
    let (ua, ub) = (u64::from(a.unsigned_abs()), u64::from(b.unsigned_abs()));
    let q = (ua << 16)
        .wrapping_add(ub >> 1)
        .checked_div(ub)
        .unwrap_or(0x7FFF_FFFF);
    signed(negative(&[a, b]), q)
}

fn mul_div_impl(a: i32, b: i32, c: i32, round: bool) -> i32 {
    let (ua, ub, uc) = (
        u64::from(a.unsigned_abs()),
        u64::from(b.unsigned_abs()),
        u64::from(c.unsigned_abs()),
    );
    let half = if round { uc >> 1 } else { 0 };
    let d = ua
        .wrapping_mul(ub)
        .wrapping_add(half)
        .checked_div(uc)
        .unwrap_or(0x7FFF_FFFF);
    signed(negative(&[a, b, c]), d)
}

/// `FT_MulDiv`: `(a * b) / c`, rounded on the magnitudes.
#[inline]
pub fn mul_div(a: i32, b: i32, c: i32) -> i32 {
    mul_div_impl(a, b, c, true)
}

/// `FT_MulDiv_No_Round`: `(a * b) / c`, truncated on the magnitudes.
#[inline]
pub fn mul_div_no_round(a: i32, b: i32, c: i32) -> i32 {
    mul_div_impl(a, b, c, false)
}

/// `FT_Vector_Transform`: `x' = x*xx + y*xy`, `y' = x*yx + y*yy`, each
/// product rounded by [`mul_fix`].
#[inline]
pub fn transform(p: Point, m: &Matrix) -> Point {
    Point {
        x: mul_fix(p.x, m.xx.0).saturating_add(mul_fix(p.y, m.xy.0)),
        y: mul_fix(p.x, m.yx.0).saturating_add(mul_fix(p.y, m.yy.0)),
    }
}
