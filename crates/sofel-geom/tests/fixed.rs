//! The arithmetic of ADR 0009 against FreeType's C formulas written
//! directly with i128 (FreeType 2.15.0, src/base/ftcalc.c and
//! include/freetype/internal/ftcalc.h). Results are compared where they
//! fit in i32; sofel saturates beyond that.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_geom::{
    F2Dot14, F26Dot6, Fixed, Matrix, Point, div_fix, mul_div, mul_div_no_round, mul_fix, transform,
};

fn sat(v: i128) -> i32 {
    v.clamp(i32::MIN as i128, i32::MAX as i128) as i32
}

/// FT_MulFix_64: ab += 0x8000 + (ab >> 63); ab >> 16.
fn ref_mul_fix(a: i32, b: i32) -> i32 {
    let ab = a as i128 * b as i128;
    let sign = if ab < 0 { -1 } else { 0 };
    sat((ab + 0x8000 + sign) >> 16)
}

/// FT_MulDiv and FT_MulDiv_No_Round on magnitudes, sign moved.
fn ref_mul_div(a: i32, b: i32, c: i32, round: bool) -> i32 {
    let s = (a < 0) ^ (b < 0) ^ (c < 0);
    let (a, b, c) = (
        a.unsigned_abs() as i128,
        b.unsigned_abs() as i128,
        c.unsigned_abs() as i128,
    );
    let d = if c > 0 {
        (a * b + if round { c >> 1 } else { 0 }) / c
    } else {
        0x7FFF_FFFF
    };
    sat(if s { -d } else { d })
}

/// FT_DivFix: ((a << 16) + (b >> 1)) / b on magnitudes; 0x7FFFFFFF for b == 0.
fn ref_div_fix(a: i32, b: i32) -> i32 {
    let s = (a < 0) ^ (b < 0);
    let (a, b) = (a.unsigned_abs() as i128, b.unsigned_abs() as i128);
    let q = if b > 0 {
        ((a << 16) + (b >> 1)) / b
    } else {
        0x7FFF_FFFF
    };
    sat(if s { -q } else { q })
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Values spread over magnitudes, not only near i32 extremes.
    fn i32(&mut self) -> i32 {
        let v = self.next();
        let bits = (v % 32) as u32;
        let m = (self.next() as u32) >> (31 - bits.min(31));
        if v & (1 << 40) != 0 {
            -(m as i32)
        } else {
            m as i32
        }
    }
}

const EDGES: [i32; 14] = [
    0,
    1,
    -1,
    2,
    -2,
    0x7FFF,
    0x8000,
    -0x8000,
    0x8001,
    0x1_0000,
    -0x1_0000,
    i32::MAX,
    i32::MIN,
    i32::MIN + 1,
];

#[test]
fn mul_fix_matches_freetype() {
    for &a in &EDGES {
        for &b in &EDGES {
            assert_eq!(mul_fix(a, b), ref_mul_fix(a, b), "{a} {b}");
        }
    }
    let mut r = XorShift(1);
    for _ in 0..200_000 {
        let (a, b) = (r.i32(), r.i32());
        assert_eq!(mul_fix(a, b), ref_mul_fix(a, b), "{a} {b}");
    }
    // Half rounds away from zero.
    assert_eq!(mul_fix(1, 0x8000), 1);
    assert_eq!(mul_fix(-1, 0x8000), -1);
}

#[test]
fn div_fix_matches_freetype() {
    for &a in &EDGES {
        for &b in &EDGES {
            assert_eq!(div_fix(a, b), ref_div_fix(a, b), "{a} {b}");
        }
    }
    let mut r = XorShift(2);
    for _ in 0..200_000 {
        let (a, b) = (r.i32(), r.i32());
        assert_eq!(div_fix(a, b), ref_div_fix(a, b), "{a} {b}");
    }
    assert_eq!(div_fix(1, 0), 0x7FFF_FFFF);
    assert_eq!(div_fix(-1, 0), -0x7FFF_FFFF);
}

#[test]
fn mul_div_matches_freetype() {
    for &a in &EDGES {
        for &b in &EDGES {
            for &c in &EDGES {
                assert_eq!(mul_div(a, b, c), ref_mul_div(a, b, c, true), "{a} {b} {c}");
                assert_eq!(
                    mul_div_no_round(a, b, c),
                    ref_mul_div(a, b, c, false),
                    "{a} {b} {c}"
                );
            }
        }
    }
    let mut r = XorShift(3);
    for _ in 0..200_000 {
        let (a, b, c) = (r.i32(), r.i32(), r.i32());
        assert_eq!(mul_div(a, b, c), ref_mul_div(a, b, c, true), "{a} {b} {c}");
        assert_eq!(
            mul_div_no_round(a, b, c),
            ref_mul_div(a, b, c, false),
            "{a} {b} {c}"
        );
    }
}

#[test]
fn conversions() {
    assert_eq!(Fixed::from(F2Dot14(0x4000)), Fixed(0x1_0000));
    assert_eq!(Fixed::from(F2Dot14(-0x2000)), Fixed(-0x8000));
    assert_eq!(F26Dot6::from_int(3), F26Dot6(192));
    assert_eq!(Fixed::ONE, Fixed(0x1_0000));
}

#[test]
fn transforms() {
    let p = Point { x: 100, y: -50 };
    assert_eq!(transform(p, &Matrix::IDENTITY), p);
    let scale = Matrix {
        xx: Fixed(0x2_0000),
        xy: Fixed(0),
        yx: Fixed(0),
        yy: Fixed(0x8000),
    };
    assert_eq!(transform(p, &scale), Point { x: 200, y: -25 });
    // x' = x*xx + y*xy, y' = x*yx + y*yy (FT_Vector_Transform)
    let shear = Matrix {
        xx: Fixed(0x1_0000),
        xy: Fixed(0x1_0000),
        yx: Fixed(0),
        yy: Fixed(0x1_0000),
    };
    assert_eq!(transform(p, &shear), Point { x: 50, y: -50 });
    let swap = Matrix {
        xx: Fixed(0),
        xy: Fixed(0x1_0000),
        yx: Fixed(0x1_0000),
        yy: Fixed(0),
    };
    assert_eq!(transform(p, &swap), Point { x: -50, y: 100 });
    // Each product is rounded on its own.
    let half = Matrix {
        xx: Fixed(0x8000),
        xy: Fixed(0x8000),
        yx: Fixed(0),
        yy: Fixed(0),
    };
    assert_eq!(transform(Point { x: 1, y: 1 }, &half), Point { x: 2, y: 0 });
}
