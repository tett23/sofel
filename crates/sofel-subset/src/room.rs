//! The room of HarfBuzz's serialization buffer: `hb_subset_plan_execute_or_fail`
//! subsets every table into one buffer (`buf`), which
//! `_hb_subset_table` grows to an estimate of the table's size
//! (`_hb_subset_estimate_table_size`) and `_hb_subset_table_try` doubles
//! while the serializer runs out of room, up to 256 times the source
//! table's length. A table that needs more fails the subset. sofel's
//! serializer grows as needed, so this module decides from the most
//! bytes the table's serializer held ([`sofel_write::Serializer::peak_len`])
//! whether HarfBuzz would have fit them, and keeps the buffer's capacity
//! from table to table as HarfBuzz does.

use sofel_font::read::Tag;

use crate::input::Flags;

/// The buffer's capacity before the first table (`buf.alloc (8192 - 16)`
/// of an empty `hb_vector_t`).
pub(crate) fn initial() -> u32 {
    alloc(0, 8192 - 16)
}

/// `hb_vector_t::alloc (size)` (not exact): the new capacity.
fn alloc(allocated: u32, size: u32) -> u32 {
    let mut n = allocated;
    while size > n {
        n = n.saturating_add(n >> 1).saturating_add(8);
    }
    n.min(0x7FFF_FFFF)
}

/// `_hb_subset_estimate_table_size`.
pub(crate) fn estimate(
    flags: Flags,
    src_glyphs: u32,
    dst_glyphs: u32,
    table_len: u32,
    tag: Tag,
) -> u32 {
    let same_size = matches!(&tag.0, b"GSUB" | b"GPOS" | b"GDEF" | b"name");
    let extra = match &tag.0 {
        b"CFF " if flags.has(Flags::RETAIN_GIDS) => src_glyphs.wrapping_mul(16),
        b"CFF2" if flags.has(Flags::RETAIN_GIDS) => src_glyphs.wrapping_mul(4),
        _ => 0,
    };
    let bulk = 8192u32.wrapping_add(extra);
    if src_glyphs == 0 || same_size {
        return bulk.wrapping_add(table_len);
    }
    let scaled = f64::from(table_len) * sqrt(f64::from(dst_glyphs) / f64::from(src_glyphs));
    bulk.wrapping_add(to_unsigned(scaled))
}

/// `(unsigned) v` for `0 <= v < 2^32`, truncating as C does.
fn to_unsigned(v: f64) -> u32 {
    // The callers' values are in [0, 2^32): `as` truncates toward zero.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let u = v as u32;
    u
}

/// The capacity after a table, and whether the table fit: the buffer
/// grows to `estimate`, then doubles (`allocated * 2 + 16`) while `peak`
/// bytes do not fit, failing once that would exceed 256 times the source
/// table's length (`_hb_subset_table` and `_hb_subset_table_try`).
pub(crate) fn fit(allocated: u32, estimate: u32, peak: usize, src_len: u32) -> (u32, bool) {
    let limit = src_len.wrapping_mul(256);
    let mut cap = alloc(allocated, estimate);
    loop {
        if peak <= usize::try_from(cap).unwrap_or(usize::MAX) {
            return (cap, true);
        }
        let size = cap.wrapping_mul(2).wrapping_add(16);
        if size > limit || size > 0x7FFF_FFFF {
            return (cap, false);
        }
        cap = size;
    }
}

/// The square root of a non-negative finite `v`, correctly rounded as
/// C's `sqrt` (IEEE 754); `core` has none without `std`.
pub(crate) fn sqrt(v: f64) -> f64 {
    if v <= 0.0 || !v.is_finite() {
        return v.max(0.0);
    }
    let bits = v.to_bits();
    let biased = (bits >> 52) & 0x7FF;
    let frac = bits & ((1u64 << 52).wrapping_sub(1));
    // v = mant * 2^e, with a subnormal's exponent fixed at -1074.
    let (mant, e) = if biased == 0 {
        // Normalized, so that the root below has 53 bits or more.
        let s = frac.leading_zeros().wrapping_sub(11);
        (frac << s, (-1074i64).wrapping_sub(i64::from(s)))
    } else {
        (
            frac | (1u64 << 52),
            i64::try_from(biased).unwrap_or(0).wrapping_sub(1075),
        )
    };
    // An even exponent; then sqrt(mant * 2^64) has more than 53 bits.
    let (mant, e) = if e & 1 != 0 {
        (mant << 1, e.wrapping_sub(1))
    } else {
        (mant, e)
    };
    let m = u128::from(mant) << 64;
    let (r, rem) = isqrt(m);
    let sticky = rem != 0;
    // Round r to 53 significant bits, to nearest, ties to even.
    let bits_r = 128u32.wrapping_sub(r.leading_zeros());
    let shift = bits_r.saturating_sub(53);
    let keep = r >> shift;
    let dropped = r & ((1u128 << shift).wrapping_sub(1));
    let half = if shift == 0 {
        0
    } else {
        1u128 << shift.wrapping_sub(1)
    };
    let up = shift != 0 && (dropped > half || (dropped == half && (sticky || keep & 1 == 1)));
    let keep = keep.wrapping_add(u128::from(up));
    let (keep, shift) = if keep >> 53 != 0 {
        (keep >> 1, shift.wrapping_add(1))
    } else {
        (keep, shift)
    };
    // sqrt(v) = keep * 2^(shift + e / 2 - 32), keep in [2^52, 2^53).
    let exp = i64::from(shift)
        .wrapping_add(e / 2)
        .wrapping_sub(32)
        .wrapping_add(52)
        .wrapping_add(1023);
    let exp_bits = u64::try_from(exp).unwrap_or(0) & 0x7FF;
    let low = u64::try_from(keep & ((1u128 << 52).wrapping_sub(1))).unwrap_or(0);
    f64::from_bits((exp_bits << 52) | low)
}

/// The integer square root of `n` and the remainder `n - r * r`.
fn isqrt(n: u128) -> (u128, u128) {
    // Digit by digit, two bits at a time.
    let mut rem = n;
    let mut root = 0u128;
    let mut bit = 1u128 << 126;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        let t = root.wrapping_add(bit);
        if rem >= t {
            rem = rem.wrapping_sub(t);
            root = (root >> 1).wrapping_add(bit);
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    (root, rem)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::float_cmp,
        clippy::arithmetic_side_effects,
        clippy::cast_precision_loss
    )]
    extern crate std;
    use super::*;

    #[test]
    fn sqrt_is_correctly_rounded() {
        assert_eq!(sqrt(0.0), 0.0);
        assert_eq!(sqrt(1.0), 1.0);
        assert_eq!(sqrt(4.0), 2.0);
        assert_eq!(sqrt(0.25), 0.5);
        // Values of std's sqrt (IEEE 754, correctly rounded).
        assert_eq!(sqrt(2.0).to_bits(), 0x3FF6_A09E_667F_3BCD);
        assert_eq!(sqrt(0.5).to_bits(), 0x3FE6_A09E_667F_3BCD);
        assert_eq!(sqrt(3.0).to_bits(), 0x3FFB_B67A_E858_4CAA);
        // Squares of odd 26-bit numbers are exact.
        for k in [1u64, 3, 12345, 67_108_863] {
            let x = k as f64;
            assert_eq!(sqrt(x * x), x);
        }
        // A sweep against std's sqrt, which is correctly rounded.
        let mut v = 1.0e-300f64;
        while v < 1.0e300 {
            for x in [
                v,
                f64::from_bits(v.to_bits() + 1),
                f64::from_bits(v.to_bits() + 7),
            ] {
                assert_eq!(sqrt(x).to_bits(), x.sqrt().to_bits(), "{x}");
            }
            v *= 1.37;
        }
        assert_eq!(
            sqrt(f64::from_bits(1)).to_bits(),
            f64::from_bits(1).sqrt().to_bits()
        );
    }

    #[test]
    fn the_capacity_grows_as_hb_vector_t() {
        // 0 -> 8 -> 20 -> 38 -> ... first value >= 8176.
        let mut n = 0u32;
        while n < 8176 {
            n += (n >> 1) + 8;
        }
        assert_eq!(initial(), n);
        assert_eq!(alloc(n, 100), n);
    }

    #[test]
    fn a_table_fits_until_256_times_its_source() {
        let cap = initial();
        // Fits at once.
        assert_eq!(fit(cap, 100, 50, 10), (cap, true));
        // Doubles once: allowed when 2 * cap + 16 <= 256 * len.
        let doubled = cap * 2 + 16;
        assert_eq!(
            fit(cap, 100, usize::try_from(cap).unwrap() + 1, 1000),
            (doubled, true)
        );
        // The doubling would exceed the limit: fails.
        assert!(!fit(cap, 100, usize::try_from(cap).unwrap() + 1, 10).1);
    }

    #[test]
    fn the_estimate_is_hb_subset_estimate_table_size() {
        let tag = Tag::new(b"cmap");
        // 8192 + 1000 * sqrt(1/4).
        assert_eq!(estimate(Flags::DEFAULT, 400, 100, 1000, tag), 8192 + 500);
        assert_eq!(
            estimate(Flags::DEFAULT, 400, 100, 1000, Tag::new(b"GSUB")),
            9192
        );
        assert_eq!(estimate(Flags::DEFAULT, 0, 0, 1000, tag), 9192);
        assert_eq!(
            estimate(Flags::RETAIN_GIDS, 400, 400, 1000, Tag::new(b"CFF ")),
            8192 + 400 * 16 + 1000
        );
        assert_eq!(
            estimate(Flags::RETAIN_GIDS, 400, 400, 1000, Tag::new(b"CFF2")),
            8192 + 400 * 4 + 1000
        );
        // Truncated: 3 * sqrt(1/3) = 1.732...
        assert_eq!(estimate(Flags::DEFAULT, 3, 1, 3, tag), 8193);
    }
}
