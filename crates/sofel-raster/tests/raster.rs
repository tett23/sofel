#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_geom::{Pen, Point};
use sofel_raster::{FillRule, RasterError, Rasterizer, RenderOptions};

fn p(x: i32, y: i32) -> Point {
    Point { x, y }
}

/// A closed polygon in 26.6 units, wound as given.
fn polygon(r: &mut Rasterizer, pts: &[(i32, i32)]) {
    r.move_to(p(pts[0].0, pts[0].1));
    for &(x, y) in &pts[1..] {
        r.line_to(p(x, y));
    }
    r.line_to(p(pts[0].0, pts[0].1));
    r.close();
}

fn rect(r: &mut Rasterizer, x0: i32, y0: i32, x1: i32, y1: i32) {
    polygon(r, &[(x0, y0), (x0, y1), (x1, y1), (x1, y0)]);
}

fn opts() -> RenderOptions {
    RenderOptions {
        fill: FillRule::NonZero,
        overlap: false,
        ppem: None,
    }
}

#[test]
fn full_pixels_are_255() {
    let mut r = Rasterizer::new();
    rect(&mut r, 0, 0, 128, 128);
    let b = r.render(opts()).unwrap();
    assert_eq!((b.left, b.top, b.width, b.rows), (0, 2, 2, 2));
    assert_eq!(b.pixels, &[255, 255, 255, 255]);
}

#[test]
fn partial_coverage() {
    // x from 0.5 to 1 pixel, one row: half the pixel.
    let mut r = Rasterizer::new();
    rect(&mut r, 32, 0, 64, 64);
    let b = r.render(opts()).unwrap();
    assert_eq!((b.left, b.width, b.rows), (0, 1, 1));
    assert_eq!(b.pixels, &[128]);
    // A quarter pixel.
    let mut r = Rasterizer::new();
    rect(&mut r, 0, 0, 32, 32);
    assert_eq!(r.render(opts()).unwrap().pixels, &[64]);
}

#[test]
fn placement_follows_the_control_box() {
    // From (-1.5, -0.25) to (0.75, 1.5) pixels.
    let mut r = Rasterizer::new();
    rect(&mut r, -96, -16, 48, 96);
    let b = r.render(opts()).unwrap();
    assert_eq!((b.left, b.top, b.width, b.rows), (-2, 2, 3, 3));
    // Rows are stored from the top.
    assert_eq!(&b.pixels[3..6], &[128, 255, 192]);
}

#[test]
fn fill_rules() {
    // The same square twice, wound the same way.
    let draw = |r: &mut Rasterizer| {
        rect(r, 0, 0, 64, 64);
        rect(r, 0, 0, 64, 64);
    };
    let mut r = Rasterizer::new();
    draw(&mut r);
    assert_eq!(r.render(opts()).unwrap().pixels, &[255]);
    let mut r = Rasterizer::new();
    draw(&mut r);
    let even_odd = RenderOptions {
        fill: FillRule::EvenOdd,
        ..opts()
    };
    assert_eq!(r.render(even_odd).unwrap().pixels, &[0]);
    // Opposite windings cancel under the non-zero rule.
    let mut r = Rasterizer::new();
    rect(&mut r, 0, 0, 64, 64);
    polygon(&mut r, &[(0, 0), (64, 0), (64, 64), (0, 64)]);
    assert_eq!(r.render(opts()).unwrap().pixels, &[0]);
}

#[test]
fn overlapping_contours_with_the_overlap_flag() {
    // Two half-covering rectangles that overlap in the middle quarter.
    let draw = |r: &mut Rasterizer| {
        rect(r, 0, 0, 40, 64);
        rect(r, 24, 0, 64, 64);
    };
    let mut r = Rasterizer::new();
    draw(&mut r);
    let plain = r.render(opts()).unwrap().pixels.to_vec();
    let mut r = Rasterizer::new();
    draw(&mut r);
    let over = r
        .render(RenderOptions {
            overlap: true,
            ..opts()
        })
        .unwrap()
        .pixels
        .to_vec();
    assert_eq!(plain, vec![255]);
    // 4x4 samples, each (255 + 8) / 16 = 16, sum 256 -> 255.
    assert_eq!(over, vec![255]);
    // A half pixel with the overlap flag: 8 samples of 16.
    let mut r = Rasterizer::new();
    rect(&mut r, 0, 0, 32, 64);
    assert_eq!(
        r.render(RenderOptions {
            overlap: true,
            ..opts()
        })
        .unwrap()
        .pixels,
        &[128]
    );
}

fn curvy(r: &mut Rasterizer) {
    r.move_to(p(0, 0));
    r.quad_to(p(900, 50), p(1200, 1300));
    r.cubic_to(p(700, 1600), p(-300, 1700), p(-200, 400));
    r.line_to(p(0, 0));
    r.close();
}

#[test]
fn bands_do_not_change_the_result() {
    let mut big = Rasterizer::new();
    curvy(&mut big);
    let a = big.render(opts()).unwrap().pixels.to_vec();
    for cells in [1, 7, 30, 100] {
        let mut small = Rasterizer::with_band_cells(cells);
        curvy(&mut small);
        assert_eq!(
            small.render(opts()).unwrap().pixels,
            &a[..],
            "{cells} cells per band"
        );
        let mut small = Rasterizer::with_band_cells(cells);
        curvy(&mut small);
        let mut big = Rasterizer::new();
        curvy(&mut big);
        let o = RenderOptions {
            overlap: true,
            ..opts()
        };
        assert_eq!(
            small.render(o).unwrap().pixels,
            big.render(o).unwrap().pixels
        );
    }
    assert!(a.iter().any(|&v| v == 255) && a.iter().any(|&v| v > 0 && v < 255));
}

#[test]
fn empty_and_degenerate() {
    let mut r = Rasterizer::new();
    let b = r.render(opts()).unwrap();
    assert_eq!(
        (b.left, b.top, b.width, b.rows, b.pixels.len()),
        (0, 0, 0, 0, 0)
    );
    // A horizontal line has no area.
    let mut r = Rasterizer::new();
    polygon(&mut r, &[(0, 0), (64, 0)]);
    let b = r.render(opts()).unwrap();
    assert!(b.pixels.iter().all(|&v| v == 0));
}

#[test]
fn limits() {
    let mut r = Rasterizer::new();
    rect(&mut r, 0, 0, 64 * 0x10000, 64);
    assert_eq!(r.render(opts()).err(), Some(RasterError::TooLarge));
    // FreeType refuses boxes wider or taller than 16 times the ppem.
    let mut r = Rasterizer::new();
    rect(&mut r, 0, 0, 64 * 33, 64);
    assert_eq!(
        r.render(RenderOptions {
            ppem: Some((2, 2)),
            ..opts()
        })
        .err(),
        Some(RasterError::TooLarge)
    );
    let mut r = Rasterizer::new();
    rect(&mut r, 0, 0, 64 * 32, 64);
    assert!(
        r.render(RenderOptions {
            ppem: Some((2, 2)),
            ..opts()
        })
        .is_ok()
    );
    // The overlap path needs width * 4 <= 0x7FFF.
    let mut r = Rasterizer::new();
    rect(&mut r, 0, 0, 64 * 0x2000, 64);
    assert_eq!(
        r.render(RenderOptions {
            overlap: true,
            ..opts()
        })
        .err(),
        Some(RasterError::TooLarge)
    );
}

#[test]
fn rasterizers_are_reusable() {
    let mut r = Rasterizer::new();
    curvy(&mut r);
    let a = r.render(opts()).unwrap().pixels.to_vec();
    r.clear();
    curvy(&mut r);
    assert_eq!(r.render(opts()).unwrap().pixels, &a[..]);
}

#[test]
fn i2_render_mode_harmony_separates_horizontal_and_vertical_subpixels() {
    use sofel_raster::LcdMode;
    let mut raster = Rasterizer::new();
    rect(&mut raster, 0, 0, 64, 64);
    let horizontal = raster.render_harmony(LcdMode::Horizontal, opts()).unwrap();
    assert_eq!(
        (
            horizontal.left,
            horizontal.top,
            horizontal.width,
            horizontal.rows
        ),
        (-1, 1, 9, 1)
    );
    assert_eq!(horizontal.pixels, &[0, 0, 84, 172, 255, 172, 84, 0, 0]);
    let vertical = raster.render_harmony(LcdMode::Vertical, opts()).unwrap();
    assert_eq!(
        (vertical.left, vertical.top, vertical.width, vertical.rows),
        (0, 2, 1, 9)
    );
    assert_eq!(vertical.pixels, &[0, 0, 84, 172, 255, 172, 84, 0, 0]);
}

#[test]
fn i2_filtered_lcd_covers_a_pixel_in_both_orientations() {
    use sofel_raster::LcdMode;
    let mut raster = Rasterizer::new();
    rect(&mut raster, 0, 0, 64, 64);
    for (mode, bounds) in [
        (LcdMode::Horizontal, (-1, 1, 9, 1)),
        (LcdMode::Vertical, (0, 2, 1, 9)),
    ] {
        let bitmap = raster
            .render_filtered_lcd(mode, [8, 77, 86, 77, 8], opts())
            .unwrap();
        assert_eq!((bitmap.left, bitmap.top, bitmap.width, bitmap.rows), bounds);
        assert_eq!(bitmap.pixels, &[0, 8, 85, 170, 239, 170, 85, 8, 0]);
        let raw = raster.render_filtered_lcd(mode, [0; 5], opts()).unwrap();
        assert_eq!(raw.pixels, &[0, 0, 0, 255, 255, 255, 0, 0, 0]);
    }
}
