//! Combine the silhouettes of SVG clipping shapes on a common 4×4 subpixel
//! grid. Union and intersection precede conversion to pixel coverage, so
//! adjacent children sharing a pixel do not leave a translucent seam.
use sofel_raster::{Bitmap, FillRule, RasterError, Rasterizer, RenderOptions};
use sofel_svg::ClipPath;

#[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)] // At most 16 samples × 255 coverage; the rounded result fits in u8.
pub(super) fn coverage(
    source: &Bitmap<'_>,
    paths: &[ClipPath],
    raster: &mut Rasterizer,
    scale: f32,
    ppem: u16,
) -> Result<Vec<u8>, RasterError> {
    let mut stack = vec![u16::MAX; source.pixels.len()];
    let mut union = vec![0_u16; source.pixels.len()];
    for path in paths {
        union.fill(0);
        for element in path {
            raster.clear();
            element.shape.draw(scale * 4.0, raster);
            let ppem = ppem.saturating_mul(4);
            let mask = raster.render(RenderOptions {
                fill: if element.even_odd {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                },
                overlap: false,
                ppem: Some((ppem, ppem)),
            })?;
            add_silhouette(&mut union, source, &mask);
        }
        for (visible, child) in stack.iter_mut().zip(&union) {
            *visible &= *child;
        }
    }
    Ok(source
        .pixels
        .iter()
        .zip(stack)
        .map(|(&coverage, samples)| {
            // At most 255×16 plus the rounding bias.
            (u32::from(coverage) * samples.count_ones() + 8).div_euclid(16) as u8
        })
        .collect())
}

// The rasterizer bounds both source and mask dimensions. The intersection
// clips rows and columns to nonnegative in-bitmap indices before conversion.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
fn add_silhouette(bits: &mut [u16], source: &Bitmap<'_>, mask: &Bitmap<'_>) {
    if mask.width == 0 || source.width == 0 {
        return;
    }
    let offset_x = i64::from(mask.left) - i64::from(source.left) * 4;
    let offset_y = i64::from(source.top) * 4 - i64::from(mask.top);
    let first_col = (-offset_x).clamp(0, i64::from(mask.width)) as usize;
    let last_col =
        (i64::from(source.width) * 4 - offset_x).clamp(0, i64::from(mask.width)) as usize;
    let first_row = (-offset_y).clamp(0, i64::from(mask.rows)) as usize;
    let last_row = (i64::from(source.rows) * 4 - offset_y).clamp(0, i64::from(mask.rows)) as usize;
    for (row, line) in mask
        .pixels
        .chunks_exact(mask.width as usize)
        .enumerate()
        .take(last_row)
        .skip(first_row)
    {
        let y = (offset_y + row as i64) as usize;
        for (column, &alpha) in line.iter().enumerate().take(last_col).skip(first_col) {
            if alpha >= 128 {
                let x = (offset_x + column as i64) as usize;
                let index = (y / 4) * source.width as usize + x / 4;
                if let Some(pixel) = bits.get_mut(index) {
                    *pixel |= 1 << (4 * (y % 4) + x % 4);
                }
            }
        }
    }
}
