//! Packed monochrome rasterization (sofel ADR 0064).

use super::PixelBox;

// ft_glyphslot_preset_bitmap rounds ties toward the outside. If an
// interval collapses, keep the pixel containing the larger part of it.
#[allow(
    clippy::arithmetic_side_effects,
    reason = "shifting by six leaves room for rounding and one extra pixel"
)]
fn mono_axis(start: i32, end: i32) -> (i32, i32) {
    let mut lower = (start >> 6) + (((start & 63) + 31) >> 6);
    let mut upper = (end >> 6) + (((end & 63) + 32) >> 6);
    if lower == upper {
        let remainder = (((start & 63) + 31) & 63) - 31 + (((end & 63) + 32) & 63) - 32;
        if remainder < 0 {
            lower -= 1;
        } else {
            upper += 1;
        }
    }
    (lower, upper)
}

/// Grid-fit the 26.6 control box for FreeType's monochrome rasterizer.
pub fn mono_pixel_box(x_min: i32, y_min: i32, x_max: i32, y_max: i32) -> PixelBox {
    let (left, right) = mono_axis(x_min, x_max);
    let (bottom, top) = mono_axis(y_min, y_max);
    PixelBox {
        left,
        top,
        width: u32::try_from(right.saturating_sub(left)).unwrap_or(0),
        rows: u32::try_from(top.saturating_sub(bottom)).unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::mono_axis;

    #[test]
    fn adr0064_i2_mono_bounds_include_centres_and_preserve_collapsed_axes() {
        for (start, end, expected) in [
            (0, 0, (0, 1)),
            (-1, -1, (-1, 0)),
            (1, 1, (0, 1)),
            (31, 31, (0, 1)),
            (32, 32, (0, 1)),
            (33, 33, (0, 1)),
            (-32, 32, (-1, 1)),
            (32, 96, (0, 2)),
            (33, 95, (1, 2)),
            (0, 448, (0, 7)),
            (0, 512, (0, 8)),
            (0, 576, (0, 9)),
        ] {
            assert_eq!(mono_axis(start, end), expected, "{start}..{end}");
        }
    }
}
