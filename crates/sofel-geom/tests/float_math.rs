//! `sofel_geom::float::sqrtf` and `atan2f` (ADR 0095): the square root
//! correctly rounded for every `f32`, and `atan2f` giving the same bits
//! natively and on wasm32.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::unwrap_used
)]

use sofel_geom::float::{atan2f, sqrtf};

/// Same bits, or both NaN.
fn same(a: f32, b: f32) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

/// Checks `sqrtf` against `f32::sqrt` for the bit patterns from `start`
/// in steps of `step`.
#[cfg(not(target_family = "wasm"))]
fn check_sqrt(start: u32, step: u32) {
    let mut bits = u64::from(start);
    while bits <= u64::from(u32::MAX) {
        let x = f32::from_bits(bits as u32);
        let (got, want) = (sqrtf(x), x.sqrt());
        assert!(
            same(got, want),
            "sqrtf({x:e} = {bits:#010x}) = {got:e}, want {want:e}"
        );
        bits += u64::from(step);
    }
}

#[test]
fn adr0095_i1_sqrtf_special_values() {
    for (x, want) in [
        (0.0f32, 0.0f32),
        (-0.0, -0.0),
        (f32::INFINITY, f32::INFINITY),
        (1.0, 1.0),
        (4.0, 2.0),
        (0.25, 0.5),
        (2.0, core::f32::consts::SQRT_2),
        (f32::from_bits(1), f32::from_bits(0x1a35_04f3)),
        (f32::MAX, f32::from_bits(0x5f7f_ffff)),
    ] {
        assert!(same(sqrtf(x), want), "sqrtf({x:e}) = {:e}", sqrtf(x));
    }
    for x in [-1.0f32, -f32::INFINITY, f32::NAN, -f32::from_bits(1)] {
        assert!(sqrtf(x).is_nan(), "sqrtf({x:e}) = {:e}", sqrtf(x));
    }
}

/// Every 9973rd bit pattern (a prime stride covers every exponent and
/// mantissa residue), in every build.
#[cfg(not(target_family = "wasm"))]
#[test]
fn adr0095_i1_sqrtf_strided_f32() {
    check_sqrt(0, 9973);
}

/// All 2^32 bit patterns, in 16 parts on as many threads. Run it in a
/// release build: `cargo test --release -p sofel-geom --test float_math
/// -- --ignored adr0095_i1_sqrtf_all_f32` (CI does).
#[cfg(not(target_family = "wasm"))]
#[test]
#[ignore = "4.3e9 values; run with --release --ignored"]
fn adr0095_i1_sqrtf_all_f32() {
    const PARTS: u32 = 16;
    let handles: Vec<_> = (0..PARTS)
        .map(|part| std::thread::spawn(move || check_sqrt(part, PARTS)))
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

/// C's `atan2f` special cases (C17 F.10.1.4).
#[test]
fn adr0095_i2_atan2f_special_values() {
    use core::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    let inf = f32::INFINITY;
    for (y, x, want) in [
        (0.0f32, 1.0f32, 0.0f32),
        (-0.0, 1.0, -0.0),
        (0.0, -1.0, PI),
        (-0.0, -1.0, -PI),
        (0.0, 0.0, 0.0),
        (-0.0, 0.0, -0.0),
        (0.0, -0.0, PI),
        (-0.0, -0.0, -PI),
        (1.0, 0.0, FRAC_PI_2),
        (-1.0, 0.0, -FRAC_PI_2),
        (1.0, -0.0, FRAC_PI_2),
        (inf, inf, FRAC_PI_4),
        (-inf, inf, -FRAC_PI_4),
        (inf, -inf, 3.0 * PI / 4.0),
        (-inf, -inf, -3.0 * PI / 4.0),
        (1.0, inf, 0.0),
        (-1.0, inf, -0.0),
        (1.0, -inf, PI),
        (-1.0, -inf, -PI),
        // musl: x = 1 goes through atanf, whose atan(∞) is atanhi[3].
        (inf, 1.0, f32::from_bits(0x3fc9_0fda)),
        (-inf, 1.0, -f32::from_bits(0x3fc9_0fda)),
        (1.0, 1.0, FRAC_PI_4),
        (1.0, -1.0, f32::from_bits(0x4016_cbe4)),
    ] {
        let got = atan2f(y, x);
        assert!(
            same(got, want),
            "atan2f({y:e}, {x:e}) = {got:e}, want {want:e}"
        );
    }
    for (y, x) in [(f32::NAN, 1.0f32), (1.0, f32::NAN), (f32::NAN, f32::NAN)] {
        assert!(atan2f(y, x).is_nan());
    }
}

/// Within 2 ulp of the correctly rounded value (from f64), on values
/// around every octant.
#[cfg(not(target_family = "wasm"))]
#[test]
fn adr0095_i2_atan2f_is_accurate() {
    let mut rng = XorShift(0x9e37_79b9_7f4a_7c15);
    for _ in 0..1_000_000 {
        let (y, x) = rng.pair();
        let got = atan2f(y, x);
        let want = (f64::from(y)).atan2(f64::from(x)) as f32;
        let ulps = (got.to_bits() as i64 - want.to_bits() as i64).abs();
        assert!(ulps <= 2, "atan2f({y:e}, {x:e}) = {got:e}, want {want:e}");
    }
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Two finite values with mixed signs and magnitudes 2^-30 .. 2^30.
    fn pair(&mut self) -> (f32, f32) {
        let r = self.next();
        let v = |bits: u64| {
            let m = (bits & 0xff_ffff) as f32 / (1u64 << 24) as f32 + 0.5;
            let e = ((bits >> 24) & 0x3f) as i32 - 30;
            let s = if bits & (1 << 30) == 0 { 1.0 } else { -1.0 };
            s * m * f32::from_bits(((e + 127) as u32) << 23)
        };
        (v(r), v(r >> 32))
    }
}

/// The FNV-1a hash of `atan2f` over the special values, the axes, and a
/// million pairs from a fixed seed. CI runs it on x86-64, aarch64 and
/// wasm32-wasip1 against the same constant (ADR 0095 I2).
#[test]
fn adr0095_i2_atan2f_is_deterministic() {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |v: f32| {
        for b in v.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    let specials = [
        0.0f32,
        -0.0,
        1.0,
        -1.0,
        f32::INFINITY,
        -f32::INFINITY,
        f32::NAN,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::MAX,
        -f32::MAX,
        f32::from_bits(0x4c80_0000), // 2^26
        f32::from_bits(0x3280_0000), // 2^-26
    ];
    for y in specials {
        for x in specials {
            let v = atan2f(y, x);
            // The bits of a NaN are not specified; count NaN as one value.
            eat(if v.is_nan() { f32::NAN } else { v });
        }
    }
    let mut rng = XorShift(0x2545_f491_4f6c_dd1d);
    for _ in 0..1_000_000 {
        let (y, x) = rng.pair();
        eat(atan2f(y, x));
        eat(atan2f(y, 0.0));
        eat(atan2f(0.0, x));
    }
    assert_eq!(h, ATAN2F_HASH, "hash {h:#018x}");
}

const ATAN2F_HASH: u64 = 0xba53_c2f0_d8b8_f4cd;
