//! Blending gray layers into a color image as FreeType renders COLR
//! version 0 glyphs (ADR 0023): `tt_face_colr_blend_layer` of
//! `src/sfnt/ttcolr.c`, FreeType 2.15.0 (commit
//! 37515cf56ba3349656049edbed89ffc6f6466f2d). Copyright (C) 2018-2026 by
//! Shao Yu Zhang, Alexei Podtelezhnikov, Werner Lemberg and Dominik Röttsches. Used
//! under the FreeType License (FTL); see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use crate::Bitmap;

/// A premultiplied BGRA image placed like a [`Bitmap`]: 4 bytes per
/// pixel (blue, green, red, alpha), `4 × width` bytes per row, rows from
/// the top. Empty until the first layer with pixels.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct BgraImage {
    /// The x of the left column, in pixels from the origin.
    pub left: i32,
    /// The y of the top edge, in pixels upwards from the origin.
    pub top: i32,
    /// The number of columns.
    pub width: u32,
    /// The number of rows.
    pub rows: u32,
    /// The pixels.
    pub pixels: Vec<u8>,
}

fn usize_of(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

/// `a * b / 255` in `int`, truncated, of values in 0..=255.
fn mul_div255(a: u32, b: u32) -> u32 {
    a.saturating_mul(b) / 255
}

impl BgraImage {
    /// Forgets the image, keeping the memory.
    pub fn clear(&mut self) {
        *self = BgraImage {
            pixels: core::mem::take(&mut self.pixels),
            ..BgraImage::default()
        };
        self.pixels.clear();
    }
}

/// Blends the coverage of `src` in color `bgra` (blue, green, red,
/// alpha, not premultiplied) over `dst`, as FreeType's
/// `tt_face_colr_blend_layer`: the first layer with pixels sets the
/// image's box, later layers widen it (the old pixels moved into the new
/// box), and each pixel becomes `dst × (255 − fa) / 255 + color × fa / 255`
/// in integers, with `fa = alpha × coverage / 255`.
pub fn blend_layer(dst: &mut BgraImage, src: &Bitmap<'_>, bgra: [u8; 4]) {
    if dst.pixels.is_empty() {
        dst.left = src.left;
        dst.top = src.top;
        dst.width = src.width;
        dst.rows = src.rows;
        dst.pixels.clear();
        dst.pixels.resize(
            usize_of(src.width)
                .saturating_mul(4)
                .saturating_mul(usize_of(src.rows)),
            0,
        );
    } else {
        let (dl, dt) = (i64::from(dst.left), i64::from(dst.top));
        let (sl, st) = (i64::from(src.left), i64::from(src.top));
        let d_right = dl.saturating_add(i64::from(dst.width));
        let d_bottom = dt.saturating_sub(i64::from(dst.rows));
        let x_min = dl.min(sl);
        let x_max = d_right.max(sl.saturating_add(i64::from(src.width)));
        let y_min = d_bottom.min(st.saturating_sub(i64::from(src.rows)));
        let y_max = dt.max(st);
        if x_min != dl || x_max != d_right || y_min != d_bottom || y_max != dt {
            let width = u32::try_from(x_max.saturating_sub(x_min)).unwrap_or(0);
            let rows = u32::try_from(y_max.saturating_sub(y_min)).unwrap_or(0);
            let pitch = usize_of(width).saturating_mul(4);
            let mut buf = alloc::vec![0u8; pitch.saturating_mul(usize_of(rows))];
            let oy = usize::try_from(y_max.saturating_sub(dt)).unwrap_or(0);
            let ox = usize::try_from(dl.saturating_sub(x_min))
                .unwrap_or(0)
                .saturating_mul(4);
            let old_pitch = usize_of(dst.width).saturating_mul(4);
            for (y, row) in dst.pixels.chunks(old_pitch.max(1)).enumerate() {
                let at = (oy.saturating_add(y))
                    .saturating_mul(pitch)
                    .saturating_add(ox);
                if let Some(d) = buf.get_mut(at..at.saturating_add(row.len())) {
                    d.copy_from_slice(row);
                }
            }
            dst.pixels = buf;
            dst.top = i32::try_from(y_max).unwrap_or(i32::MAX);
            dst.left = i32::try_from(x_min).unwrap_or(i32::MIN);
            dst.width = width;
            dst.rows = rows;
        }
    }
    let [b, g, r, alpha] = bgra.map(u32::from);
    let pitch = usize_of(dst.width).saturating_mul(4);
    let oy = usize::try_from(i64::from(dst.top).saturating_sub(i64::from(src.top))).unwrap_or(0);
    let ox = usize::try_from(i64::from(src.left).saturating_sub(i64::from(dst.left)))
        .unwrap_or(0)
        .saturating_mul(4);
    let src_pitch = usize_of(src.width);
    for y in 0..usize_of(src.rows) {
        let start = y.saturating_mul(src_pitch);
        let Some(line) = src.pixels.get(start..start.saturating_add(src_pitch)) else {
            break;
        };
        let at = oy
            .saturating_add(y)
            .saturating_mul(pitch)
            .saturating_add(ox);
        let Some(out) = dst
            .pixels
            .get_mut(at..at.saturating_add(src_pitch.saturating_mul(4)))
        else {
            break;
        };
        for (&aa, px) in line.iter().zip(out.as_chunks_mut::<4>().0) {
            let fa = mul_div255(alpha, u32::from(aa));
            let (fb, fg, fr) = (mul_div255(b, fa), mul_div255(g, fa), mul_div255(r, fa));
            let ba2 = 255u32.saturating_sub(fa);
            let mix = |d: u8, f: u32| {
                u8::try_from(mul_div255(u32::from(d), ba2).saturating_add(f) & 0xFF).unwrap_or(0)
            };
            let [db, dg, dr, da] = *px;
            *px = [mix(db, fb), mix(dg, fg), mix(dr, fr), mix(da, fa)];
        }
    }
}
