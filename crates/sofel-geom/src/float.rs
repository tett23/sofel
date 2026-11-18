//! The floating-point operations sofel needs to give HarfBuzz's values
//! bit for bit (ADR 0026): C's `floor`, `floorf`, `ceilf` and `truncf`,
//! HarfBuzz's `roundf` ([`hb_roundf`], [`hb_roundf32`]) and the
//! conversions HarfBuzz uses between floats and integers.
//!
//! Everything is written with `f32` and `f64` arithmetic only: no `std`,
//! and no fused multiply-add, which would change results.
//!
//! HarfBuzz's `roundf` is not C's. `src/hb-algs.hh` of HarfBuzz 15.1.0
//! replaces it with a macro:
//!
//! ```c
//! static inline double _hb_roundf (double x) { return floor (x + .5); }
//! static inline double _hb_roundf (float x)  { return floor ((double) x + .5); }
//! #define roundf(x) _hb_roundf(x)
//! ```
//!
//! so halves round toward positive infinity also below zero (−2.5 gives
//! −2, where C's `roundf` gives −3), and the result is a `double`.
//! Ported from HarfBuzz 15.1.0 (Copyright © 2017 Google, Inc.; "Old
//! MIT" license, see THIRD_PARTY_NOTICES.md).

/// 2^23: from here on every `f32` is an integer.
const F32_INTEGRAL: f32 = 8_388_608.0;
/// 2^52: from here on every `f64` is an integer.
const F64_INTEGRAL: f64 = 4_503_599_627_370_496.0;

/// `v` truncated toward zero, when `|v| < 2^23` (so that it fits `i32`
/// exactly); `None` otherwise or for NaN.
fn trunc_small(v: f32) -> Option<i32> {
    if v.abs() < F32_INTEGRAL {
        // |v| < 2^23 was checked: the truncated value fits i32 exactly.
        #[allow(clippy::cast_possible_truncation)]
        Some(v as i32)
    } else {
        None
    }
}

/// C's `truncf`: `v` rounded toward zero, keeping the sign of zero.
pub fn truncf(v: f32) -> f32 {
    trunc_small(v).map_or(v, |i| {
        let t = i32_to_f32(i);
        if t == 0.0 { 0.0f32.copysign(v) } else { t }
    })
}

/// C's `floorf`: the largest integer not above `v`.
pub fn floorf(v: f32) -> f32 {
    let t = truncf(v);
    if t > v { t - 1.0 } else { t }
}

/// C's `ceilf`: the smallest integer not below `v`.
pub fn ceilf(v: f32) -> f32 {
    let t = truncf(v);
    if t < v { t + 1.0 } else { t }
}

/// C's `floor`: the largest integer not above `v`, keeping the sign of
/// zero.
pub fn floor(v: f64) -> f64 {
    if v.is_nan() || v.abs() >= F64_INTEGRAL {
        return v;
    }
    // |v| < 2^52 was checked: the truncated value fits i64 exactly.
    #[allow(clippy::cast_possible_truncation)]
    let i = v as i64;
    #[allow(clippy::cast_precision_loss)]
    let t = i as f64;
    if t > v {
        t - 1.0
    } else if t == 0.0 {
        0.0f64.copysign(v)
    } else {
        t
    }
}

/// HarfBuzz's `roundf` of a `double` (`_hb_roundf (double)` of
/// `hb-algs.hh`): `floor (v + 0.5)`.
///
/// Not C's `roundf`: halves round toward positive infinity also below
/// zero (−2.5 gives −2, −0.5 gives +0), and there is no conversion to
/// `float`. Use it wherever sofel copies a HarfBuzz computation that
/// calls `roundf`; for C's rounding use another function.
pub fn hb_roundf(v: f64) -> f64 {
    floor(v + 0.5)
}

/// HarfBuzz's `roundf` of a `float` (`_hb_roundf (float)` of
/// `hb-algs.hh`): `floor ((double) v + 0.5)`, a `double`.
///
/// `v` is widened before adding 0.5, so 0.49999997 gives 0 (C's
/// `roundf` does too, but `floorf (v + 0.5f)` would give 1). Halves
/// round toward positive infinity also below zero, unlike C's `roundf`;
/// see [`hb_roundf`].
pub fn hb_roundf32(v: f32) -> f64 {
    hb_roundf(f64::from(v))
}

/// HarfBuzz's `hb_clamp_to<int> (double)`: clamps to the `i32` range and
/// truncates toward zero. NaN gives 0.
pub fn hb_clamp_to_i32(v: f64) -> i32 {
    if v.is_nan() {
        return 0;
    }
    let c = v.clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    // Clamped to the i32 range above; `as` truncates toward zero.
    #[allow(clippy::cast_possible_truncation)]
    let i = c as i32;
    i
}

/// `(int64_t) v` in C, truncating toward zero; saturating out of range
/// (where C is undefined) and 0 for NaN.
pub fn f32_to_i64(v: f32) -> i64 {
    if v.is_nan() {
        return 0;
    }
    let d = f64::from(v);
    if d.abs() >= F64_INTEGRAL {
        // Already an integer; saturate at the i64 range.
        return if d < 0.0 { i64::MIN } else { i64::MAX };
    }
    // |d| < 2^52 was checked: the truncated value fits i64 exactly.
    #[allow(clippy::cast_possible_truncation)]
    let i = d as i64;
    i
}

/// `(float) v` for an `int64_t`, rounding to nearest as C does.
pub fn i64_to_f32(v: i64) -> f32 {
    // Conversion to the nearest representable value is what C does.
    #[allow(clippy::cast_precision_loss)]
    let f = v as f32;
    f
}

/// `(float) v` for an `int`, rounding to nearest as C does.
pub fn i32_to_f32(v: i32) -> f32 {
    // Conversion to the nearest representable value is what C does.
    #[allow(clippy::cast_precision_loss)]
    let f = v as f32;
    f
}

/// The integer square root of `n` and the remainder `n - root²`.
// Bit-by-bit square root: `root + bit` and `rem - (root + bit)` stay
// within `u64` (`root + bit ≤ 2·√n < 2^33`, and the subtraction is done
// only when `rem ≥ root + bit`).
#[allow(clippy::arithmetic_side_effects)]
fn isqrt(n: u64) -> (u64, u64) {
    let mut bit = 1u64 << 62;
    while bit > n {
        bit >>= 2;
    }
    let (mut root, mut rem) = (0u64, n);
    while bit != 0 {
        if rem >= root + bit {
            rem -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    (root, rem)
}

/// C's `sqrtf`: the square root correctly rounded to nearest, ties to
/// even, as IEEE 754 `squareRoot` (ADR 0095). NaN for a NaN or a value
/// below zero; `-0.0` for `-0.0`.
///
/// The value is computed with integers only, so it is the same on every
/// target and equal to every platform's `sqrtf`.
// The exponent fits `i32` and the shifts stay below 64 bits: the
// mantissa has at most 25 bits and is shifted to at most 62; the root
// has 31 or 32 bits, so `n` is between 7 and 8 and the biased exponent of
// the result is between 52 and 191 (√ of 2^-149 .. 2^128).
#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
pub fn sqrtf(x: f32) -> f32 {
    if x.is_nan() || x == 0.0 || x == f32::INFINITY {
        return x;
    }
    if x < 0.0 {
        return f32::NAN;
    }
    let bits = x.to_bits();
    let biased = ((bits >> 23) & 0xff) as i32;
    let frac = u64::from(bits & 0x7f_ffff);
    // x = m · 2^e with an integer m.
    let (m, e) = if biased == 0 {
        (frac, -149)
    } else {
        (frac | 0x80_0000, biased - 150)
    };
    // Make the exponent even, then shift m left by an even amount so that
    // its root has at least 31 bits.
    let (m, e) = if e & 1 == 0 { (m, e) } else { (m << 1, e - 1) };
    let shift = (m.leading_zeros() - 2) & !1;
    let (root, rem) = isqrt(m << shift);
    let e = (e - shift as i32) / 2;
    // Round the root to 24 bits; a nonzero remainder lies past the half.
    let n = 64 - root.leading_zeros() - 24;
    let keep = root >> n;
    let dropped = root & ((1 << n) - 1);
    let half = 1u64 << (n - 1);
    let up = dropped > half || (dropped == half && (rem != 0 || keep & 1 == 1));
    let keep = keep + u64::from(up);
    let (keep, n) = if keep >> 24 == 0 {
        (keep, n)
    } else {
        (keep >> 1, n + 1)
    };
    let exponent = (e + n as i32 + 23 + 127) as u32;
    f32::from_bits((exponent << 23) | (keep as u32 & 0x7f_ffff))
}

/// A C `float` constant written as a `double` literal, as musl's tables
/// are: the decimal is rounded to `double` first, then to `float`.
#[allow(clippy::cast_possible_truncation)]
const fn c_float(v: f64) -> f32 {
    v as f32
}

/// `atan(0.5)`, `atan(1)`, `atan(1.5)` and `atan(∞)`, high parts.
const ATAN_HI: [f32; 4] = [
    c_float(4.6364760399e-01),
    c_float(7.8539812565e-01),
    c_float(9.8279368877e-01),
    c_float(1.5707962513e+00),
];
/// The low parts of [`ATAN_HI`].
const ATAN_LO: [f32; 4] = [
    c_float(5.0121582440e-09),
    c_float(3.7748947079e-08),
    c_float(3.4473217170e-08),
    c_float(7.5497894159e-08),
];
/// The odd polynomial of `atanf`.
const AT: [f32; 5] = [
    c_float(3.3333328366e-01),
    c_float(-1.9999158382e-01),
    c_float(1.4253635705e-01),
    c_float(-1.0648017377e-01),
    c_float(6.1687607318e-02),
];
/// π rounded to `float`, and the rest.
const PI: f32 = c_float(3.1415927410e+00);
const PI_LO: f32 = c_float(-8.7422776573e-08);

/// musl's `atanf`.
fn atanf(x: f32) -> f32 {
    let ix = x.to_bits() & 0x7fff_ffff;
    let negative = x.is_sign_negative();
    if ix >= 0x4c80_0000 {
        // |x| ≥ 2^26 (musl adds 2^-120 to raise inexact; the sum rounds
        // to the high part).
        if x.is_nan() {
            return x;
        }
        let z = ATAN_HI[3] + f32::from_bits(7 << 23);
        return if negative { -z } else { z };
    }
    let (x, id) = if ix < 0x3ee0_0000 {
        // |x| < 0.4375
        if ix < 0x3980_0000 {
            // |x| < 2^-12
            return x;
        }
        (x, None)
    } else {
        let x = f32::from_bits(ix);
        if ix < 0x3f98_0000 {
            if ix < 0x3f30_0000 {
                ((2.0 * x - 1.0) / (2.0 + x), Some((ATAN_HI[0], ATAN_LO[0])))
            } else {
                ((x - 1.0) / (x + 1.0), Some((ATAN_HI[1], ATAN_LO[1])))
            }
        } else if ix < 0x401c_0000 {
            ((x - 1.5) / (1.0 + 1.5 * x), Some((ATAN_HI[2], ATAN_LO[2])))
        } else {
            (-1.0 / x, Some((ATAN_HI[3], ATAN_LO[3])))
        }
    };
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * AT[4]));
    let s2 = w * (AT[1] + w * AT[3]);
    match id {
        None => x - x * (s1 + s2),
        Some((hi, lo)) => {
            let z = hi - ((x * (s1 + s2) - lo) - x);
            if negative { -z } else { z }
        }
    }
}

/// C's `atan2f(y, x)`: the angle of the point (x, y) from the positive x
/// axis, in −π ..= π (ADR 0095).
///
/// Ported from musl 1.2.6 (commit bd54dc3e8049b9c7db092905e743cdebbb8864f3),
/// `src/math/atan2f.c` and `src/math/atanf.c`, which come from FreeBSD's
/// msun. Copyright © 2005-2020 Rich Felker, et al. (MIT); Copyright (C)
/// 1993 by Sun Microsystems, Inc. Used under their licenses; see
/// THIRD_PARTY_NOTICES.md.
///
/// Every operation is an IEEE 754 `f32` addition, subtraction,
/// multiplication or division (no fused multiply-add, no platform
/// `libm`), so the results are the same on every target. They are within
/// 2 ulp of the exact angle; they need not equal a platform's `atan2f`.
pub fn atan2f(y: f32, x: f32) -> f32 {
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    if x.to_bits() == 0x3f80_0000 {
        return atanf(y);
    }
    let (ix, iy) = (x.to_bits() & 0x7fff_ffff, y.to_bits() & 0x7fff_ffff);
    // 2·sign(x) + sign(y)
    let (neg_y, neg_x) = (y.is_sign_negative(), x.is_sign_negative());
    let half_pi = if neg_y { -PI / 2.0 } else { PI / 2.0 };
    if iy == 0 {
        return match (neg_x, neg_y) {
            (false, _) => y,
            (true, false) => PI,
            (true, true) => -PI,
        };
    }
    if ix == 0 {
        return half_pi;
    }
    if ix == 0x7f80_0000 {
        return match (iy == 0x7f80_0000, neg_x, neg_y) {
            (true, false, false) => PI / 4.0,
            (true, false, true) => -PI / 4.0,
            (true, true, false) => 3.0 * PI / 4.0,
            (true, true, true) => -3.0 * PI / 4.0,
            (false, false, false) => 0.0,
            (false, false, true) => -0.0,
            (false, true, false) => PI,
            (false, true, true) => -PI,
        };
    }
    // |y/x| > 2^26 (the sums stay below 2^32: both are at most 0x7f800000).
    if ix.saturating_add(26 << 23) < iy || iy == 0x7f80_0000 {
        return half_pi;
    }
    let z = if neg_x && iy.saturating_add(26 << 23) < ix {
        // |y/x| < 2^-26 and x < 0
        0.0
    } else {
        atanf((y / x).abs())
    };
    match (neg_x, neg_y) {
        (false, false) => z,
        (false, true) => -z,
        (true, false) => PI - (z - PI_LO),
        (true, true) => (z - PI_LO) - PI,
    }
}
