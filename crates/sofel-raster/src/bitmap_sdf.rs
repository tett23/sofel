//! FreeType's bitmap SDF from ftbsdf.c. Fixed-point values use 16.16
//! arithmetic. The Sobel kernel operates on 8-bit coverage, the radius
//! is at most 32, and the map has a checked size and a two-pixel-or-wider
//! border around the source. Both sweeps visit only in-bounds neighbors.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing
)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0064_i2_bitmap_sdf_preserves_subpixel_edge_and_propagates_distance() {
        let mut map = [Distance::FAR; 49];
        map[24] = Distance::new(ONE / 4, 0);
        transform(&mut map, 7, 7);
        assert_eq!(map[24].length, ONE / 4);
        assert_eq!(map[23].length, ONE + ONE / 4);
        assert_eq!(map[25].length, ONE - ONE / 4);
        assert_eq!(
            map[8].length,
            sofel_geom::vector_length(2 * ONE + ONE / 4, 2 * ONE)
        );
        assert_eq!(
            map[40].length,
            sofel_geom::vector_length(-2 * ONE + ONE / 4, -2 * ONE)
        );
    }

    #[test]
    fn adr0064_i2_bitmap_sdf_encodes_the_sign_and_spread_limits() {
        assert_eq!(encode(400 * ONE, 0, 8), 0);
        assert_eq!(encode(400 * ONE, 255, 8), 255);
        assert_eq!(encode(0, 0, 8), 128);
        assert_eq!(encode(ONE / 2, 126, 8), 120);
        assert_eq!(encode(ONE / 2, 127, 8), 136);
    }
}

const ONE: i32 = 1 << 16;

#[derive(Clone, Copy)]
struct Distance {
    x: i32,
    y: i32,
    length: i32,
}

impl Distance {
    const FAR: Self = Self {
        x: 200 * ONE,
        y: 200 * ONE,
        length: 400 * ONE,
    };

    fn new(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            length: sofel_geom::vector_length(x, y),
        }
    }

    fn compare(&mut self, neighbor: Self, dx: i32, dy: i32) {
        if self.length <= ONE / 2 || neighbor.length - ONE >= self.length {
            return;
        }
        let candidate = Self::new(neighbor.x + dx * ONE, neighbor.y + dy * ONE);
        if candidate.length < self.length {
            *self = candidate;
        }
    }
}

// FreeType first_pass and second_pass visit the same row neighborhood
// in the same order, looking above on the first pass and below on the second.
fn scan_row(map: &mut [Distance], width: usize, row: usize, neighbor_row: usize, dy: i32) {
    for x in 1..width - 1 {
        let index = row * width + x;
        let above = neighbor_row * width + x;
        for (other, dx, dy) in [
            (above - 1, -1, dy),
            (above, 0, dy),
            (above + 1, 1, dy),
            (index - 1, -1, 0),
        ] {
            let neighbor = map[other];
            map[index].compare(neighbor, dx, dy);
        }
    }
    for x in (0..width - 1).rev() {
        let index = row * width + x;
        let right = map[index + 1];
        map[index].compare(right, 1, 0);
    }
}

fn transform(map: &mut [Distance], width: usize, rows: usize) {
    if width == 0 || rows == 0 {
        return;
    }
    for row in 1..rows {
        scan_row(map, width, row, row - 1, -1);
    }
    for row in (0..rows - 1).rev() {
        scan_row(map, width, row, row + 1, 1);
    }
}

pub(super) fn encode(distance: i32, alpha: u8, spread: u8) -> u8 {
    let limit = i32::from(spread) * ONE;
    let magnitude = sofel_geom::div_fix(distance.clamp(0, limit), limit) >> 9;
    if alpha < 127 {
        (128 - magnitude) as u8
    } else {
        (128 + magnitude.min(127)) as u8
    }
}

#[cfg(test)]
mod edge_tests {
    use super::edge_vector;

    #[test]
    fn adr0064_i2_bitmap_sdf_fractional_edges_match_freetype() {
        assert_eq!(
            edge_vector([0, 0, 0, 0, 190, 255, 0, 255, 255]),
            (-9238, -9238)
        );
        assert_eq!(edge_vector([255, 255, 255, 255, 128, 0, 0, 0, 0]), (0, 0));
        assert_eq!(
            edge_vector([0, 64, 255, 0, 64, 255, 0, 64, 255]),
            (16384, 0)
        );
    }
}

fn sqrt_fixed(value: i32) -> i32 {
    if value == 0 {
        return 0;
    }
    let n = ((value as u64) << 16) - 1;
    let mut q = 1u64 << ((17 + value.ilog2()) >> 1);
    loop {
        let next = (q + n / q + 1) >> 1;
        if next == q {
            return q as i32;
        }
        q = next;
    }
}

// compute_edge_distance samples the centered 3x3 coverage neighborhood.
// Coverage 255 represents 255/256 here, including fully covered pixels.
fn edge_vector(pixels: [u8; 9]) -> (i32, i32) {
    use sofel_geom::{div_fix, mul_fix};
    let [a, b, c, d, alpha, f, g, h, i] = pixels.map(|a| i32::from(a) * 256);
    let (x, y) = super::sdf_math::unit_vector(
        c + i - a - g + mul_fix(f, 92681) - mul_fix(d, 92681),
        g + i - a - c + mul_fix(h, 92681) - mul_fix(b, 92681),
    );
    let distance = if x == 0 || y == 0 {
        ONE / 2 - alpha
    } else {
        let major = x.abs().max(y.abs());
        let minor = x.abs().min(y.abs());
        let threshold = div_fix(minor, major) / 2;
        let corner = |coverage| sqrt_fixed(2 * mul_fix(major, mul_fix(minor, coverage)));
        if alpha < threshold {
            (major + minor) / 2 - corner(alpha)
        } else if alpha < ONE - threshold {
            mul_fix(ONE / 2 - alpha, major)
        } else {
            -(major + minor) / 2 + corner(ONE - alpha)
        }
    };
    (mul_fix(x, distance), mul_fix(y, distance))
}

#[test]
fn adr0064_i2_square_bitmap_sdf_matches_freetype() {
    let source = super::Bitmap {
        left: 0,
        top: 7,
        width: 7,
        rows: 7,
        pixels: &[255; 49],
    };
    let mut output = alloc::vec::Vec::new();
    let result = bitmap_sdf(source, 8, &mut output).unwrap();
    assert_eq!(
        (result.left, result.top, result.width, result.rows),
        (-8, 15, 23, 23)
    );
    assert_eq!(
        &result.pixels[11 * 23..12 * 23],
        &[
            8, 24, 40, 56, 72, 88, 104, 120, 135, 151, 167, 183, 167, 151, 135, 120, 104, 88, 72,
            56, 40, 24, 0
        ]
    );
}

/// Errors converting a top-down grayscale bitmap to a signed distance field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapSdfError {
    /// Spread must be between two and 32 pixels, inclusive.
    InvalidSpread,
    /// The source buffer length differs from width times rows.
    InvalidBitmap,
    /// Dimensions, origin or temporary storage exceed the supported range.
    TooLarge,
}

/// Compute FreeType's bitmap-input signed distance field with the given spread.
/// Output rows are tightly packed, in the same top-down order as the input.
///
/// # Errors
/// Returns an error for invalid input or an unrepresentable result.
pub fn bitmap_sdf<'a>(
    source: super::Bitmap<'_>,
    spread: u8,
    output: &'a mut alloc::vec::Vec<u8>,
) -> Result<super::Bitmap<'a>, BitmapSdfError> {
    use BitmapSdfError::{InvalidBitmap, InvalidSpread, TooLarge};
    if !(2..=32).contains(&spread) {
        return Err(InvalidSpread);
    }
    let (w, h) = (source.width as usize, source.rows as usize);
    if w.checked_mul(h) != Some(source.pixels.len()) {
        return Err(InvalidBitmap);
    }
    if w == 0 || h == 0 {
        output.clear();
        return Ok(super::Bitmap {
            pixels: output,
            ..source
        });
    }
    let pad = usize::from(spread);
    let width = source
        .width
        .checked_add(u32::from(spread) * 2)
        .ok_or(TooLarge)?;
    let rows = source
        .rows
        .checked_add(u32::from(spread) * 2)
        .ok_or(TooLarge)?;
    let left = source.left.checked_sub(i32::from(spread)).ok_or(TooLarge)?;
    let top = source.top.checked_add(i32::from(spread)).ok_or(TooLarge)?;
    let stride = width as usize;
    let len = stride.checked_mul(rows as usize).ok_or(TooLarge)?;
    let mut map = alloc::vec::Vec::new();
    map.try_reserve_exact(len).map_err(|_| TooLarge)?;
    map.resize(len, Distance::FAR);
    output.clear();
    output.try_reserve_exact(len).map_err(|_| TooLarge)?;
    output.resize(len, 0);
    for (y, src) in source.pixels.chunks_exact(w).enumerate() {
        let start = (y + pad) * stride + pad;
        output[start..start + w].copy_from_slice(src);
    }
    for y in pad..pad + h {
        for x in pad..pad + w {
            let index = y * stride + x;
            if output[index] == 0 {
                continue;
            }
            let above = index - stride;
            let below = index + stride;
            let neighborhood = [
                above - 1,
                above,
                above + 1,
                index - 1,
                index,
                index + 1,
                below - 1,
                below,
                below + 1,
            ]
            .map(|i| output[i]);
            if output[index] < 255 || neighborhood.contains(&0) {
                let (gx, gy) = edge_vector(neighborhood);
                map[index] = Distance::new(gx, gy);
            }
        }
    }
    transform(&mut map, stride, rows as usize);
    for (a, d) in output.iter_mut().zip(map) {
        *a = encode(d.length, *a, spread);
    }
    Ok(super::Bitmap {
        left,
        top,
        width,
        rows,
        pixels: output,
    })
}
