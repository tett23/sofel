//! FreeType 2.15.0 `cf2_computeDarkening`, in 16.16 character space.
//!
//! Copyright 2007-2014 Adobe Systems Incorporated and Copyright (C)
//! 1996-2026 David Turner, Robert Wilhelm, and Werner Lemberg.
//! Used under the FreeType License and Adobe patent license.

use super::ft;

const ONE: i32 = 0x1_0000;

fn fixed(value: i32) -> i32 {
    value.wrapping_mul(ONE)
}

fn div(a: i32, b: i32) -> i32 {
    ft::to_i32(ft::div_fix(i64::from(a), i64::from(b)))
}

fn mul(a: i32, b: i32) -> i32 {
    ft::to_i32(ft::mul_fix(i64::from(a), i64::from(b)))
}

fn lerp(stem: i32, ppem: i32, low: [i32; 2], high: [i32; 2]) -> Option<i32> {
    let dx = high[0].wrapping_sub(low[0]);
    if dx == 0 {
        return None;
    }
    let x = stem.wrapping_sub(div(fixed(low[0]), ppem));
    let y = ft::to_i32(ft::mul_div(
        i64::from(x),
        i64::from(high[1].wrapping_sub(low[1])),
        i64::from(dx),
    ));
    Some(y.wrapping_add(div(fixed(low[1]), ppem)))
}

/// The half-width darkening amount in 16.16 character-space units.
#[allow(clippy::arithmetic_side_effects)] // The pinned C implementation uses truncating division.
pub(super) fn amount(upem: u32, ppem: u16, stem_width: i32, parameters: [i32; 8]) -> i32 {
    let units = if upem == 0 { 1000 } else { upem };
    let denominator = i32::try_from(units).unwrap_or(i32::MAX);
    let em_ratio = fixed(1000).wrapping_div(denominator);
    if em_ratio < 655 {
        return 0;
    }
    let ppem = fixed(i32::from(ppem).max(4));
    let stem = mul(stem_width, em_ratio);
    let points: [[i32; 2]; 4] = [
        [parameters[0], parameters[1]],
        [parameters[2], parameters[3]],
        [parameters[4], parameters[5]],
        [parameters[6], parameters[7]],
    ];
    let msb = |value: i32| 31u32.saturating_sub(value.cast_unsigned().leading_zeros());
    let log = msb(stem).saturating_add(msb(ppem));
    let scaled = if log >= 46 {
        fixed(points[3][0])
    } else {
        mul(stem, ppem)
    };
    let value = if scaled < fixed(points[0][0]) {
        div(fixed(points[0][1]), ppem)
    } else if scaled < fixed(points[1][0]) {
        lerp(stem, ppem, points[0], points[1])
            .or_else(|| lerp(stem, ppem, points[1], points[2]))
            .or_else(|| lerp(stem, ppem, points[2], points[3]))
            .unwrap_or_else(|| div(fixed(points[3][1]), ppem))
    } else if scaled < fixed(points[2][0]) {
        lerp(stem, ppem, points[1], points[2])
            .or_else(|| lerp(stem, ppem, points[2], points[3]))
            .unwrap_or_else(|| div(fixed(points[3][1]), ppem))
    } else if scaled < fixed(points[3][0]) {
        lerp(stem, ppem, points[2], points[3]).unwrap_or_else(|| div(fixed(points[3][1]), ppem))
    } else {
        div(fixed(points[3][1]), ppem)
    };
    div(value, em_ratio.wrapping_mul(2))
}

#[cfg(test)]
mod tests {
    use super::amount;

    #[test]
    fn adr0086_i2_equal_knot_curve_at_ten_ppem() {
        let curve = [0, 500, 0, 500, 0, 500, 0, 500];
        assert_eq!(amount(1000, 10, 0, curve), 25 << 16);
        assert_eq!(amount(1000, 10, 75 << 16, curve), 25 << 16);
    }
}
