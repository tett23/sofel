//! FreeType's integer arithmetic as the CFF driver uses it on 64-bit
//! targets, where `FT_Long` and `FT_Fixed` are 64 bits wide
//! (`src/base/ftcalc.c`, `include/freetype/internal/ftcalc.h`).
//!
//! Ported from FreeType 2.15.0. Copyright (C) 1996-2026 by David Turner,
//! Robert Wilhelm, and Werner Lemberg. Used under the FreeType License
//! (FTL); see THIRD_PARTY_NOTICES.md.

/// `FT_MulFix` on `FT_Long`: `(a * b + 0x8000 + sign) >> 16` with 64-bit
/// wrapping, as `FT_MulFix_64` computes it.
pub fn mul_fix(a: i64, b: i64) -> i64 {
    let ab = a.wrapping_mul(b);
    ab.wrapping_add(0x8000_i64.wrapping_add(ab >> 63)) >> 16
}

/// `FT_DivFix` on `FT_Long`: the magnitudes as unsigned 64-bit numbers,
/// `((a << 16) + b / 2) / b`, `0x7FFFFFFF` for `b == 0`.
pub fn div_fix(a: i64, b: i64) -> i64 {
    let negative = (a < 0) != (b < 0);
    let (ua, ub) = (a.unsigned_abs(), b.unsigned_abs());
    let q = (ua << 16)
        .wrapping_add(ub >> 1)
        .checked_div(ub)
        .unwrap_or(0x7FFF_FFFF);
    let q = q.cast_signed();
    if negative { q.wrapping_neg() } else { q }
}

/// `FT_MulDiv` on `FT_Long`: `(a * b + c / 2) / c` on the magnitudes,
/// `0x7FFFFFFF` for `c == 0`.
pub fn mul_div(a: i64, b: i64, c: i64) -> i64 {
    let negative = ((a < 0) != (b < 0)) != (c < 0);
    let (ua, ub, uc) = (a.unsigned_abs(), b.unsigned_abs(), c.unsigned_abs());
    let d = ua
        .wrapping_mul(ub)
        .wrapping_add(uc >> 1)
        .checked_div(uc)
        .unwrap_or(0x7FFF_FFFF);
    let d = d.cast_signed();
    if negative { d.wrapping_neg() } else { d }
}

/// `FT_RoundFix`: `(a + 0x8000 - (a < 0)) & ~0xFFFF`, wrapping.
pub fn round_fix(a: i64) -> i64 {
    a.wrapping_add(0x8000_i64.wrapping_sub(i64::from(a < 0))) & !0xFFFF
}

/// `FT_SqrtFixed` with 64-bit intermediates: the square root of a 16.16
/// number, rounded.
pub fn sqrt_fixed(v: u32) -> u32 {
    if v == 0 {
        return 0;
    }
    let r = (u64::from(v) << 16).wrapping_sub(1);
    let msb = 31u32.saturating_sub(v.leading_zeros());
    let mut q: u32 = 1u32 << ((17u32.saturating_add(msb)) >> 1);
    loop {
        let t = q;
        let div = low32(r.checked_div(u64::from(t)).unwrap_or(0));
        q = t.wrapping_add(div).wrapping_add(1) >> 1;
        if q == t {
            return q;
        }
    }
}

/// The low 32 bits of a 64-bit FreeType value, as C's conversion to
/// `FT_Int32` gives them.
pub fn to_i32(v: i64) -> i32 {
    low32(v.cast_unsigned()).cast_signed()
}

/// The low 32 bits of `v`.
pub fn low32(v: u64) -> u32 {
    u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0)
}
