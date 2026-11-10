//! ADR 0023: `blend_layer` against FreeType's `tt_face_colr_blend_layer`
//! written out directly (C `int` arithmetic, pointer offsets).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_raster::{BgraImage, Bitmap, blend_layer};

/// FreeType's slot: buffer `None` until the first allocation.
#[derive(Default)]
struct Slot {
    left: i32,
    top: i32,
    width: i32,
    rows: i32,
    buffer: Option<Vec<u8>>,
}

fn reference(dst: &mut Slot, src: &Bitmap<'_>, (b, g, r, alpha): (i32, i32, i32, i32)) {
    if dst.buffer.is_none() {
        dst.left = src.left;
        dst.top = src.top;
        dst.width = src.width as i32;
        dst.rows = src.rows as i32;
        let size = (dst.width * 4 * dst.rows) as usize;
        dst.buffer = (size > 0).then(|| vec![0; size]);
    } else {
        let x_min = dst.left.min(src.left);
        let x_max = (dst.left + dst.width).max(src.left + src.width as i32);
        let y_min = (dst.top - dst.rows).min(src.top - src.rows as i32);
        let y_max = dst.top.max(src.top);
        if x_min != dst.left
            || x_max != dst.left + dst.width
            || y_min != dst.top - dst.rows
            || y_max != dst.top
        {
            let width = x_max - x_min;
            let rows = y_max - y_min;
            let pitch = width * 4;
            let mut buf = vec![0u8; (rows * pitch) as usize];
            let old = dst.buffer.take().unwrap();
            let q = pitch * (y_max - dst.top) + 4 * (dst.left - x_min);
            for y in 0..dst.rows {
                let p = (y * dst.width * 4) as usize;
                let q = (q + y * pitch) as usize;
                let n = (dst.width * 4) as usize;
                buf[q..q + n].copy_from_slice(&old[p..p + n]);
            }
            dst.buffer = Some(buf);
            dst.top = y_max;
            dst.left = x_min;
            dst.width = width;
            dst.rows = rows;
        }
    }
    let Some(buf) = dst.buffer.as_mut() else {
        return;
    };
    let pitch = dst.width * 4;
    let base = pitch * (dst.top - src.top) + 4 * (src.left - dst.left);
    for y in 0..src.rows as i32 {
        for x in 0..src.width as i32 {
            let aa = src.pixels[(y * src.width as i32 + x) as usize] as i32;
            let fa = alpha * aa / 255;
            let fb = b * fa / 255;
            let fg = g * fa / 255;
            let fr = r * fa / 255;
            let ba2 = 255 - fa;
            let d = (base + y * pitch + 4 * x) as usize;
            buf[d] = (buf[d] as i32 * ba2 / 255 + fb) as u8;
            buf[d + 1] = (buf[d + 1] as i32 * ba2 / 255 + fg) as u8;
            buf[d + 2] = (buf[d + 2] as i32 * ba2 / 255 + fr) as u8;
            buf[d + 3] = (buf[d + 3] as i32 * ba2 / 255 + fa) as u8;
        }
    }
}

struct Layer {
    left: i32,
    top: i32,
    width: u32,
    rows: u32,
    pixels: Vec<u8>,
    bgra: [u8; 4],
}

fn layer(left: i32, top: i32, width: u32, rows: u32, seed: u32, bgra: [u8; 4]) -> Layer {
    let mut s = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let pixels = (0..width * rows)
        .map(|i| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            match i % 5 {
                0 => 0,
                1 => 255,
                _ => s as u8,
            }
        })
        .collect();
    Layer {
        left,
        top,
        width,
        rows,
        pixels,
        bgra,
    }
}

fn check(layers: &[Layer]) {
    let mut image = BgraImage::default();
    let mut slot = Slot::default();
    for l in layers {
        let b = Bitmap {
            left: l.left,
            top: l.top,
            width: l.width,
            rows: l.rows,
            pixels: &l.pixels,
        };
        blend_layer(&mut image, &b, l.bgra);
        let [cb, cg, cr, ca] = l.bgra.map(i32::from);
        reference(&mut slot, &b, (cb, cg, cr, ca));
        assert_eq!(
            (image.left, image.top, image.width, image.rows),
            (slot.left, slot.top, slot.width as u32, slot.rows as u32)
        );
        assert_eq!(image.pixels, slot.buffer.clone().unwrap_or_default());
    }
}

#[test]
fn two_separate_layers() {
    check(&[
        layer(0, 10, 5, 6, 1, [10, 200, 30, 255]),
        layer(8, -2, 4, 3, 2, [250, 0, 128, 255]),
    ]);
}

#[test]
fn two_overlapping_layers() {
    check(&[
        layer(0, 10, 8, 8, 3, [10, 200, 30, 255]),
        layer(2, 8, 4, 4, 4, [250, 0, 128, 200]),
        layer(-3, 12, 12, 14, 5, [1, 2, 3, 128]),
    ]);
}

#[test]
fn alpha_zero_and_full() {
    check(&[
        layer(0, 4, 4, 4, 6, [255, 255, 255, 255]),
        layer(0, 4, 4, 4, 7, [0, 0, 0, 0]),
        layer(1, 3, 2, 2, 8, [0, 0, 0, 255]),
    ]);
}

#[test]
fn empty_layers_set_the_box_later() {
    check(&[
        layer(3, 3, 0, 0, 9, [0, 0, 0, 255]),
        layer(5, 7, 0, 4, 10, [0, 0, 0, 255]),
        layer(1, 2, 3, 2, 11, [9, 8, 7, 255]),
        layer(0, 0, 0, 0, 12, [0, 0, 0, 255]),
        layer(-6, 9, 2, 1, 13, [100, 50, 25, 77]),
    ]);
}

#[test]
fn many_random_layers() {
    for seed in 0..200u32 {
        let n = 1 + seed % 6;
        let layers: Vec<Layer> = (0..n)
            .map(|k| {
                let s = seed * 31 + k * 7;
                layer(
                    (s % 23) as i32 - 11,
                    (s % 19) as i32 - 5,
                    s % 9,
                    (s / 3) % 8,
                    s,
                    [(s * 3) as u8, (s * 5) as u8, (s * 11) as u8, (s * 13) as u8],
                )
            })
            .collect();
        check(&layers);
    }
}

#[test]
fn clear_forgets_the_image() {
    let mut image = BgraImage::default();
    let l = layer(0, 2, 2, 2, 1, [1, 2, 3, 255]);
    let b = Bitmap {
        left: l.left,
        top: l.top,
        width: l.width,
        rows: l.rows,
        pixels: &l.pixels,
    };
    blend_layer(&mut image, &b, l.bgra);
    image.clear();
    assert_eq!(image, BgraImage::default());
}
