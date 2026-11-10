//! The few floating-point operations HarfBuzz's scaling needs, written
//! with `f32` and `f64` arithmetic only (no `std`, no fused
//! multiply-add), so that results match HarfBuzz bit for bit (ADR 0016).

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

/// C's `truncf`.
pub(crate) fn truncf(v: f32) -> f32 {
    trunc_small(v).map_or(v, |i| {
        let t = i as f32;
        // Keep the sign of zero, as truncf does.
        if t == 0.0 { 0.0f32.copysign(v) } else { t }
    })
}

/// C's `floorf`.
pub(crate) fn floorf(v: f32) -> f32 {
    let t = truncf(v);
    if t > v { t - 1.0 } else { t }
}

/// C's `ceilf`.
pub(crate) fn ceilf(v: f32) -> f32 {
    let t = truncf(v);
    if t < v { t + 1.0 } else { t }
}

/// HarfBuzz's `roundf` (`_hb_roundf` in `hb-algs.hh`):
/// `floor((double) v + 0.5)`, so halfway cases round up, also below
/// zero.
pub(crate) fn roundf(v: f32) -> f64 {
    floor(f64::from(v) + 0.5)
}

/// C's `floor`.
pub(crate) fn floor(v: f64) -> f64 {
    if v.is_nan() || v.abs() >= F64_INTEGRAL {
        return v;
    }
    // |v| < 2^52 was checked: the truncated value fits i64 exactly.
    #[allow(clippy::cast_possible_truncation)]
    let i = v as i64;
    #[allow(clippy::cast_precision_loss)]
    let t = i as f64;
    if t > v { t - 1.0 } else { t }
}

/// HarfBuzz's `hb_clamp_to<int> (double)`: clamps to the `i32` range
/// and truncates toward zero. NaN gives 0.
pub(crate) fn f64_to_i32(v: f64) -> i32 {
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
/// and 0 for NaN.
pub(crate) fn f32_to_i64(v: f32) -> i64 {
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

/// `(float) v` for an `i64`, rounding to nearest as C does.
pub(crate) fn i64_to_f32(v: i64) -> f32 {
    // Conversion to the nearest representable value is what C does.
    #[allow(clippy::cast_precision_loss)]
    let f = v as f32;
    f
}

/// `(float) v` for an `i32`, rounding to nearest as C does.
pub(crate) fn i32_to_f32(v: i32) -> f32 {
    // Conversion to the nearest representable value is what C does.
    #[allow(clippy::cast_precision_loss)]
    let f = v as f32;
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_matches_c() {
        let cases: [(f32, f32, f32, f32); 9] = [
            // v, roundf, floorf, ceilf
            (0.5, 1.0, 0.0, 1.0),
            (-0.5, 0.0, -1.0, -0.0),
            (1.5, 2.0, 1.0, 2.0),
            (2.5, 3.0, 2.0, 3.0),
            (-2.5, -2.0, -3.0, -2.0),
            (0.49999997, 0.0, 0.0, 1.0),
            (-1.25, -1.0, -2.0, -1.0),
            (8_388_609.0, 8_388_609.0, 8_388_609.0, 8_388_609.0),
            (-3.0, -3.0, -3.0, -3.0),
        ];
        for (v, r, f, c) in cases {
            assert_eq!(roundf(v), f64::from(r), "roundf({v})");
            assert_eq!(floorf(v), f, "floorf({v})");
            assert_eq!(ceilf(v), c, "ceilf({v})");
        }
        assert!(ceilf(-0.5).is_sign_negative());
        assert!(roundf(f32::NAN).is_nan());
        assert_eq!(roundf(f32::INFINITY), f64::INFINITY);
        assert_eq!(roundf(-494.5), -494.0);
    }

    #[test]
    fn conversions_saturate() {
        assert_eq!(f64_to_i32(3.9), 3);
        assert_eq!(f64_to_i32(-3.9), -3);
        assert_eq!(f64_to_i32(1e20), i32::MAX);
        assert_eq!(f64_to_i32(-1e20), i32::MIN);
        assert_eq!(f64_to_i32(f64::NAN), 0);
        assert_eq!(f32_to_i64(50_331.65), 50_331);
        assert_eq!(f32_to_i64(-65_536.0), -65_536);
        assert_eq!(f32_to_i64(f32::MAX), i64::MAX);
    }
}
