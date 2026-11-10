//! The floating-point helpers of `sofel_geom::float` (ADR 0026): HarfBuzz's
//! `_hb_roundf` (halves up, also below zero; not C's `roundf`), C's
//! `floor`, `floorf`, `ceilf` and `truncf`, and HarfBuzz's conversions.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_geom::float::{
    ceilf, f32_to_i64, floor, floorf, hb_clamp_to_i32, hb_roundf, hb_roundf32, i32_to_f32,
    i64_to_f32, truncf,
};

/// Same value and same sign of zero.
fn same(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || (a.to_bits() == b.to_bits())
}

#[test]
fn halves_round_up_also_below_zero() {
    // (v, HarfBuzz's roundf); C's roundf differs at the negative halves.
    let cases: [(f64, f64); 12] = [
        (-2.5, -2.0),
        (-1.5, -1.0),
        // floor(-0.5 + 0.5) = floor(+0.0) = +0.0.
        (-0.5, 0.0),
        (0.5, 1.0),
        (1.5, 2.0),
        (2.5, 3.0),
        (-494.5, -494.0),
        (3.0, 3.0),
        (-3.0, -3.0),
        (0.0, 0.0),
        (-1.25, -1.0),
        (1.75, 2.0),
    ];
    for (v, r) in cases {
        assert!(same(hb_roundf(v), r), "hb_roundf({v}) = {}", hb_roundf(v));
        assert!(
            same(hb_roundf32(v as f32), r),
            "hb_roundf32({v}) = {}",
            hb_roundf32(v as f32)
        );
    }
    // The negative halves are where C's roundf (away from zero) differs.
    assert_ne!(hb_roundf(-2.5), (-2.5f64).round());
    assert_ne!(hb_roundf32(-0.5), f64::from((-0.5f32).round()));
}

#[test]
fn signed_zero_follows_floor() {
    // floor(-0.0 + 0.5) = floor(0.5) = +0.0; floor(-0.25 + 0.5) = +0.0.
    for v in [-0.0, 0.0, -0.25, -0.5, 0.25] {
        assert!(same(hb_roundf(v), 0.0), "hb_roundf({v})");
        assert!(same(hb_roundf32(v as f32), 0.0), "hb_roundf32({v})");
    }
    // A negative value that rounds to zero is +0.0 too: -0.5 + 0.5 = +0.0.
    assert!(hb_roundf(-0.5).is_sign_positive());
    assert!(same(floor(-0.0), -0.0));
    assert!(same(floor(-0.25), -1.0));
}

#[test]
fn just_below_a_half() {
    // The largest f32 below 0.5 stays below it in f64, as HarfBuzz widens
    // before adding: 0.49999997 + 0.5 < 1.
    let v = 0.499_999_97_f32;
    assert!(v < 0.5);
    assert!(same(hb_roundf32(v), 0.0));
    // The largest f64 below 0.5: x + 0.5 rounds to 1.0 in f64, so floor
    // gives 1 — as HarfBuzz's double version does.
    let d = f64::from_bits(0.5f64.to_bits() - 1);
    assert!(same(hb_roundf(d), 1.0));
    assert!(same(hb_roundf(-d), 0.0));
}

#[test]
fn large_values_are_integers() {
    let f = 8_388_608.0_f32; // 2^23
    for v in [f, f + 1.0, -f, -(f + 1.0), 16_777_217.0, f32::MAX, f32::MIN] {
        assert!(same(hb_roundf32(v), f64::from(v)), "hb_roundf32({v})");
    }
    // 2^23 - 0.5 is exact in f32 and rounds up.
    assert!(same(hb_roundf32(8_388_607.5), 8_388_608.0));
    assert!(same(hb_roundf32(-8_388_607.5), -8_388_607.0));
    let d = 4_503_599_627_370_496.0_f64; // 2^52
    for v in [d, d + 2.0, -d, -(d + 2.0), 9.0e18, f64::MAX, f64::MIN] {
        assert!(same(hb_roundf(v), v), "hb_roundf({v})");
    }
    // Odd integers above 2^52: v + 0.5 is a tie in f64 and goes to the
    // even neighbour, as in HarfBuzz.
    assert!(same(hb_roundf(d + 1.0), d + 2.0));
    assert!(same(hb_roundf(-(d + 1.0)), -d));
    // 2^52 - 0.5 + 0.5 = 2^52 exactly.
    assert!(same(hb_roundf(d - 0.5), d));
    assert!(same(hb_roundf(-(d - 0.5)), -(d - 1.0)));
}

#[test]
fn nan_and_infinity() {
    assert!(hb_roundf(f64::NAN).is_nan());
    assert!(hb_roundf32(f32::NAN).is_nan());
    assert_eq!(hb_roundf(f64::INFINITY), f64::INFINITY);
    assert_eq!(hb_roundf(f64::NEG_INFINITY), f64::NEG_INFINITY);
    assert_eq!(hb_roundf32(f32::INFINITY), f64::INFINITY);
    assert_eq!(hb_roundf32(f32::NEG_INFINITY), f64::NEG_INFINITY);
}

#[test]
fn c_floor_ceil_trunc() {
    let cases: [(f32, f32, f32, f32); 9] = [
        // v, floorf, ceilf, truncf
        (0.5, 0.0, 1.0, 0.0),
        (-0.5, -1.0, -0.0, -0.0),
        (1.5, 1.0, 2.0, 1.0),
        (-2.5, -3.0, -2.0, -2.0),
        (0.499_999_97, 0.0, 1.0, 0.0),
        (-1.25, -2.0, -1.0, -1.0),
        (8_388_609.0, 8_388_609.0, 8_388_609.0, 8_388_609.0),
        (-3.0, -3.0, -3.0, -3.0),
        (f32::INFINITY, f32::INFINITY, f32::INFINITY, f32::INFINITY),
    ];
    for (v, f, c, t) in cases {
        assert_eq!(floorf(v).to_bits(), f.to_bits(), "floorf({v})");
        assert_eq!(ceilf(v).to_bits(), c.to_bits(), "ceilf({v})");
        assert_eq!(truncf(v).to_bits(), t.to_bits(), "truncf({v})");
    }
    assert!(floorf(f32::NAN).is_nan());
    assert!(floor(f64::NAN).is_nan());
    assert_eq!(floor(-2.5), -3.0);
    assert_eq!(floor(2.5), 2.0);
}

#[test]
fn agrees_with_std_on_random_values() {
    let mut s: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    for _ in 0..200_000 {
        let d = f64::from_bits(next());
        let f = f32::from_bits(next() as u32);
        assert!(same(floor(d), d.floor()), "floor({d:e})");
        assert!(same(hb_roundf(d), (d + 0.5).floor()), "hb_roundf({d:e})");
        let w = f64::from(f);
        assert!(
            same(hb_roundf32(f), (w + 0.5).floor()),
            "hb_roundf32({f:e})"
        );
        if f.is_nan() {
            assert!(floorf(f).is_nan() && ceilf(f).is_nan() && truncf(f).is_nan());
            continue;
        }
        assert_eq!(floorf(f).to_bits(), f.floor().to_bits(), "floorf({f:e})");
        assert_eq!(ceilf(f).to_bits(), f.ceil().to_bits(), "ceilf({f:e})");
        assert_eq!(truncf(f).to_bits(), f.trunc().to_bits(), "truncf({f:e})");
    }
}

#[test]
fn conversions_saturate() {
    assert_eq!(hb_clamp_to_i32(3.9), 3);
    assert_eq!(hb_clamp_to_i32(-3.9), -3);
    assert_eq!(hb_clamp_to_i32(1e20), i32::MAX);
    assert_eq!(hb_clamp_to_i32(-1e20), i32::MIN);
    assert_eq!(hb_clamp_to_i32(f64::NAN), 0);
    assert_eq!(f32_to_i64(50_331.65), 50_331);
    assert_eq!(f32_to_i64(-65_536.0), -65_536);
    assert_eq!(f32_to_i64(f32::MAX), i64::MAX);
    assert_eq!(f32_to_i64(f32::NAN), 0);
    assert_eq!(i64_to_f32(16_777_217), 16_777_216.0);
    assert_eq!(i32_to_f32(-16_777_219), -16_777_220.0);
}
