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
