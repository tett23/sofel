//! Image paint callback sampling, following HarfBuzz hb-raster-paint.cc (Old MIT).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use crate::BgraImage;
use sofel_geom::{GlyphExtents, PointF, Transform};

impl BgraImage {
    /// Paint a top-down BGRA bitmap into the transformed glyph-space rectangle.
    /// Source placement comes from `bounds`. The clip is one coverage byte per destination pixel.
    pub fn paint_bitmap(
        &mut self,
        source: &Self,
        bounds: GlyphExtents,
        transform: Transform,
        clip: &[u8],
    ) -> Option<()> {
        let width = usize::try_from(self.width).ok()?;
        let count = width.checked_mul(usize::try_from(self.rows).ok()?)?;
        let sw = usize::try_from(source.width).ok()?;
        let sh = usize::try_from(source.rows).ok()?;
        let texels = source.pixels.as_chunks::<4>().0;
        if width == 0
            || sw == 0
            || sh == 0
            || source.pixels.len() != sw.checked_mul(sh)?.checked_mul(4)?
            || clip.len() != count
            || self.pixels.len() != count.checked_mul(4)?
            || bounds.width == 0
            || bounds.height == 0
        {
            return None;
        }
        let d = transform.xx * transform.yy - transform.xy * transform.yx;
        if !d.is_finite() || d.abs() < 1e-10 {
            return None;
        }
        let inv = 1. / d;
        let back = Transform {
            xx: transform.yy * inv,
            xy: -transform.xy * inv,
            yx: -transform.yx * inv,
            yy: transform.xx * inv,
            dx: (transform.xy * transform.dy - transform.yy * transform.dx) * inv,
            dy: (transform.yx * transform.dx - transform.xx * transform.dy) * inv,
        };
        let sx = bounds.width as f32 / sw as f32;
        let sy = bounds.height as f32 / sh as f32;
        for (row, (dst, mask)) in self
            .pixels
            .chunks_exact_mut(width * 4)
            .zip(clip.chunks_exact(width))
            .enumerate()
        {
            let mut point = back.apply(PointF::new(
                self.left as f32,
                self.top as f32 - row as f32 - 1.,
            ));
            for (pixel, &alpha) in dst.as_chunks_mut::<4>().0.iter_mut().zip(mask) {
                let x = (point.x - bounds.x_bearing as f32) / sx;
                let y = (sh - 1) as f32 - (point.y - bounds.y_bearing as f32) / sy;
                point.x += back.xx;
                point.y += back.yx;
                if alpha == 0
                    || !(0. ..=(sw - 1) as f32).contains(&x)
                    || !(0. ..=(sh - 1) as f32).contains(&y)
                {
                    continue;
                }
                let x0 = x as usize;
                let y0 = y as usize;
                let x1 = (x0 + 1).min(sw - 1);
                let y1 = (y0 + 1).min(sh - 1);
                let fx = x - x0 as f32;
                let fy = y - y0 as f32;
                let texel = |x, y| texels.get((sh - 1 - y) * sw + x);
                let samples = [
                    texel(x0, y0)?,
                    texel(x1, y0)?,
                    texel(x0, y1)?,
                    texel(x1, y1)?,
                ];
                let weights = [
                    (1. - fx) * (1. - fy),
                    fx * (1. - fy),
                    (1. - fx) * fy,
                    fx * fy,
                ];
                let mut sampled = [0.; 4];
                for (neighbor, weight) in samples.into_iter().zip(weights) {
                    for (out, &channel) in sampled.iter_mut().zip(neighbor) {
                        *out += f32::from(channel) * weight;
                    }
                }
                let mul = |a: u8, b: u8| ((u32::from(a) * u32::from(b) + 255) >> 8) as u8;
                let src = sampled.map(|c| mul((c + 0.5) as u8, alpha));
                for (out, channel) in pixel.iter_mut().zip(src) {
                    *out = channel.saturating_add(mul(*out, 255 - src[3]));
                }
            }
        }
        Some(())
    }
}
