//! `sinf`, `cosf` and `tanf` that give the same bits on every target
//! (ADR 0023).
//!
//! Ported from musl 1.2.6 (commit bd54dc3e8049b9c7db092905e743cdebbb8864f3),
//! `src/math/sincosf.c`, `src/math/tanf.c`, `src/math/__sindf.c`,
//! `src/math/__cosdf.c`, `src/math/__tandf.c` and `src/math/__rem_pio2f.c`,
//! which come from FreeBSD's msun. Copyright © 2005-2020 Rich Felker, et
//! al. (MIT); Copyright (C) 1993, 2004 by Sun Microsystems, Inc. Used
//! under their licenses; see THIRD_PARTY_NOTICES.md.
//!
//! Every operation is an IEEE 754 `f64` addition, multiplication or
//! division (no fused multiply-add, no platform `libm`), so the results
//! are the same on x86-64, aarch64 and wasm32. For |x| ≥ 2^28·π/2,
//! where musl calls `__rem_pio2_large`, the remainder is computed here
//! with 128-bit integers from the same bits of 2/π (musl's `ipio2`
//! table); both give the remainder to more bits than the `f32` result
//! needs.

/// `__sindf` coefficients: |sin(x)/x - s(x)| < 2^-37.5.
const S1: f64 = f64::from_bits(0xbfc5_5555_54cb_ac77);
const S2: f64 = f64::from_bits(0x3f81_1110_896e_fbb2);
const S3: f64 = f64::from_bits(0xbf2a_00f9_e2ca_e774);
const S4: f64 = f64::from_bits(0x3ec6_cd87_8c3b_46a7);

/// `__cosdf` coefficients: |cos(x) - c(x)| < 2^-34.1.
const C0: f64 = f64::from_bits(0xbfdf_ffff_fd0c_5e81);
const C1: f64 = f64::from_bits(0x3fa5_5553_e105_3a42);
const C2: f64 = f64::from_bits(0xbf56_c087_e80f_1e27);
const C3: f64 = f64::from_bits(0x3ef9_9342_e0ee_5069);

/// `__tandf` coefficients: |tan(x)/x - t(x)| < 2^-25.5.
const T: [f64; 6] = [
    f64::from_bits(0x3fd5_554d_3418_c99f),
    f64::from_bits(0x3fc1_12fd_3899_9f72),
    f64::from_bits(0x3fab_54c9_1d86_5afe),
    f64::from_bits(0x3f99_1df3_908c_33ce),
    f64::from_bits(0x3f68_5dad_fcec_f44e),
    f64::from_bits(0x3f83_62b9_bf97_1bcd),
];

/// Small multiples of π/2 rounded to double precision.
const PIO2_1X: f64 = core::f64::consts::FRAC_PI_2;
const PIO2_2X: f64 = 2.0 * core::f64::consts::FRAC_PI_2;
const PIO2_3X: f64 = 3.0 * core::f64::consts::FRAC_PI_2;
const PIO2_4X: f64 = 4.0 * core::f64::consts::FRAC_PI_2;

/// `toint = 1.5 / DBL_EPSILON`, for rounding to an integer.
const TOINT: f64 = 1.5 / f64::EPSILON;
/// `pio4 = 0x1.921fb6p-1`.
const PIO4: f64 = f64::from_bits(0x3fe9_21fb_6000_0000);
/// 53 bits of 2/π.
const INVPIO2: f64 = f64::from_bits(0x3fe4_5f30_6dc9_c883);
/// The first 25 bits of π/2.
const PIO2_1: f64 = f64::from_bits(0x3ff9_21fb_5000_0000);
/// π/2 - `PIO2_1`.
const PIO2_1T: f64 = f64::from_bits(0x3e51_10b4_611a_6263);

/// The first 240 bits of 2/π after the binary point, 24 per entry
/// (musl's `ipio2`).
const IPIO2: [u32; 10] = [
    0xA2_F983, 0x6E_4E44, 0x15_29FC, 0x27_57D1, 0xF5_34DD, 0xC0_DB62, 0x95_993C, 0x43_9041,
    0xFE_5163, 0xAB_DEBB,
];

/// `__sindf`.
fn k_sin(x: f64) -> f32 {
    let z = x * x;
    let w = z * z;
    let r = S3 + z * S4;
    let s = z * x;
    to_f32((x + s * (S1 + z * S2)) + s * w * r)
}

/// `__cosdf`.
fn k_cos(x: f64) -> f32 {
    let z = x * x;
    let w = z * z;
    let r = C2 + z * C3;
    to_f32(((1.0 + z * C0) + w * C1) + (w * z) * r)
}

/// `__tandf`.
fn k_tan(x: f64, odd: bool) -> f32 {
    let [t0, t1, t2, t3, t4, t5] = T;
    let z = x * x;
    let r = t4 + z * t5;
    let t = t2 + z * t3;
    let w = z * z;
    let s = z * x;
    let u = t0 + z * t1;
    let r = (x + s * u) + (s * w) * (t + w * r);
    to_f32(if odd { -1.0 / r } else { r })
}

/// `(float) v`, rounding to nearest.
fn to_f32(v: f64) -> f32 {
    // Rounding to the nearest f32 is the C conversion being ported.
    #[allow(clippy::cast_possible_truncation)]
    let f = v as f32;
    f
}

/// 64 bits of 2/π starting at bit `start` after the binary point
/// (bit 1 is the first), zero past the table.
fn bits_of_2_over_pi(start: u32) -> u64 {
    let entry = |i: u32| {
        usize::try_from(i)
            .ok()
            .and_then(|i| IPIO2.get(i))
            .copied()
            .unwrap_or(0)
    };
    // Entry k holds bits 24k+1..=24k+24.
    let first = start.saturating_sub(1) / 24;
    let acc = (first..first.saturating_add(4)).fold(0u128, |a, k| (a << 24) | u128::from(entry(k)));
    // `acc` holds 96 bits starting at bit 24·first+1; drop those before
    // `start` and keep 64.
    let skip = start
        .saturating_sub(1)
        .saturating_sub(first.saturating_mul(24));
    let shifted = acc << (32u32.saturating_add(skip));
    u64::try_from(shifted >> 64).unwrap_or(0)
}

/// `__rem_pio2f` for |x| ≥ 2^28·π/2 (finite): x mod π/2 in [-π/4, π/4]
/// and the quotient's low bits.
fn rem_pio2_large(x: f32) -> (i32, f64) {
    let bits = x.to_bits();
    let negative = bits >> 31 != 0;
    let ix = bits & 0x7fff_ffff;
    // x = m · 2^e with m the 24-bit significand; e ≥ 5 here.
    let m = u128::from((ix & 0x007f_ffff) | 0x0080_0000);
    let e = (ix >> 23).saturating_sub(127 + 23);
    // x·2/π mod 4 needs the bits of 2/π from position e-1 on: the ones
    // before multiply m by a multiple of 4.
    let start = e.saturating_sub(1);
    let hi = u128::from(bits_of_2_over_pi(start));
    let lo = u128::from(bits_of_2_over_pi(start.saturating_add(64)));
    // r / 2^126 = x·2/π mod 4, to 126 fraction bits.
    let r = ((m.wrapping_mul(hi) & u128::from(u64::MAX)) << 64).wrapping_add(m.wrapping_mul(lo));
    let quotient = r >> 126;
    // The fraction times 2^128, read as two's complement: in [-1/2, 1/2),
    // with the quotient rounded up when it is negative.
    let frac = r << 2;
    let n = quotient.wrapping_add(frac >> 127);
    let signed = i128::from_ne_bytes(frac.to_ne_bytes());
    // 2^-128 · π/2.
    const SCALE: f64 =
        core::f64::consts::FRAC_PI_2 / 340_282_366_920_938_463_463_374_607_431_768_211_456.0;
    // The conversion rounds to nearest, as C's does.
    #[allow(clippy::cast_precision_loss)]
    let y = signed as f64 * SCALE;
    let n = i32::try_from(n & 3).unwrap_or(0);
    if negative {
        (n.wrapping_neg(), -y)
    } else {
        (n, y)
    }
}

/// `__rem_pio2f`: x mod π/2 in about [-π/4, π/4] and the quotient (only
/// its low two bits are used).
fn rem_pio2(x: f32) -> (i32, f64) {
    let ix = x.to_bits() & 0x7fff_ffff;
    if ix < 0x4dc9_0fdb {
        // |x| ~< 2^28·(π/2), medium size.
        let xd = f64::from(x);
        let mut fnum = xd * INVPIO2 + TOINT - TOINT;
        // |fnum| < 2^28, so it fits i32 exactly.
        #[allow(clippy::cast_possible_truncation)]
        let mut n = fnum as i32;
        let mut y = xd - fnum * PIO2_1 - fnum * PIO2_1T;
        if y < -PIO4 {
            n = n.wrapping_sub(1);
            fnum -= 1.0;
            y = xd - fnum * PIO2_1 - fnum * PIO2_1T;
        } else if y > PIO4 {
            n = n.wrapping_add(1);
            fnum += 1.0;
            y = xd - fnum * PIO2_1 - fnum * PIO2_1T;
        }
        return (n, y);
    }
    if ix >= 0x7f80_0000 {
        return (0, f64::NAN);
    }
    rem_pio2_large(x)
}

/// The sine and cosine of `x` (radians), as musl's `sincosf`.
pub fn sin_cos(x: f32) -> (f32, f32) {
    let bits = x.to_bits();
    let negative = bits >> 31 != 0;
    let ix = bits & 0x7fff_ffff;
    let xd = f64::from(x);
    if ix <= 0x3f49_0fda {
        // |x| ~<= π/4.
        if ix < 0x3980_0000 {
            // |x| < 2^-12.
            return (x, 1.0);
        }
        return (k_sin(xd), k_cos(xd));
    }
    if ix <= 0x407b_53d1 {
        // |x| ~<= 5π/4.
        if ix <= 0x4016_cbe3 {
            // |x| ~<= 3π/4.
            return if negative {
                (-k_cos(xd + PIO2_1X), k_sin(xd + PIO2_1X))
            } else {
                (k_cos(PIO2_1X - xd), k_sin(PIO2_1X - xd))
            };
        }
        let y = if negative { xd + PIO2_2X } else { xd - PIO2_2X };
        return (-k_sin(y), -k_cos(y));
    }
    if ix <= 0x40e2_31d5 {
        // |x| ~<= 9π/4.
        if ix <= 0x40af_eddf {
            // |x| ~<= 7π/4.
            return if negative {
                (k_cos(xd + PIO2_3X), -k_sin(xd + PIO2_3X))
            } else {
                (-k_cos(xd - PIO2_3X), k_sin(xd - PIO2_3X))
            };
        }
        let y = if negative { xd + PIO2_4X } else { xd - PIO2_4X };
        return (k_sin(y), k_cos(y));
    }
    if ix >= 0x7f80_0000 {
        // sin(Inf or NaN) is NaN.
        let nan = f32::NAN;
        return (nan, nan);
    }
    let (n, y) = rem_pio2(x);
    let (s, c) = (k_sin(y), k_cos(y));
    match n & 3 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// The sine of `x` (radians): the first value of [`sin_cos`].
pub fn sin(x: f32) -> f32 {
    sin_cos(x).0
}

/// The cosine of `x` (radians): the second value of [`sin_cos`].
pub fn cos(x: f32) -> f32 {
    sin_cos(x).1
}

/// The tangent of `x` (radians), as musl's `tanf`.
pub fn tan(x: f32) -> f32 {
    let bits = x.to_bits();
    let negative = bits >> 31 != 0;
    let ix = bits & 0x7fff_ffff;
    let xd = f64::from(x);
    if ix <= 0x3f49_0fda {
        // |x| ~<= π/4.
        if ix < 0x3980_0000 {
            return x;
        }
        return k_tan(xd, false);
    }
    if ix <= 0x407b_53d1 {
        // |x| ~<= 5π/4.
        return if ix <= 0x4016_cbe3 {
            k_tan(if negative { xd + PIO2_1X } else { xd - PIO2_1X }, true)
        } else {
            k_tan(if negative { xd + PIO2_2X } else { xd - PIO2_2X }, false)
        };
    }
    if ix <= 0x40e2_31d5 {
        // |x| ~<= 9π/4.
        return if ix <= 0x40af_eddf {
            k_tan(if negative { xd + PIO2_3X } else { xd - PIO2_3X }, true)
        } else {
            k_tan(if negative { xd + PIO2_4X } else { xd - PIO2_4X }, false)
        };
    }
    if ix >= 0x7f80_0000 {
        return f32::NAN;
    }
    let (n, y) = rem_pio2(x);
    k_tan(y, n & 1 != 0)
}
