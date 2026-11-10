//! ADR 0014: sweeping only the touched cells gives the same images as
//! sweeping the whole dense array (the reference sweep of ADR 0012).

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
use sofel_raster::{FillRule, PixelBox, Rasterizer, RenderOptions};

fn p(x: i32, y: i32) -> Point {
    Point { x, y }
}

fn polygon(r: &mut dyn Pen, pts: &[(i32, i32)]) {
    r.move_to(p(pts[0].0, pts[0].1));
    for &(x, y) in &pts[1..] {
        r.line_to(p(x, y));
    }
    r.line_to(p(pts[0].0, pts[0].1));
    r.close();
}

fn rect(r: &mut dyn Pen, x0: i32, y0: i32, x1: i32, y1: i32) {
    polygon(r, &[(x0, y0), (x0, y1), (x1, y1), (x1, y0)]);
}

type Shape = fn(&mut dyn Pen);

/// Shapes whose touched columns differ from row to row.
const SHAPES: [(&str, Shape); 8] = [
    ("triangle", |r| polygon(r, &[(5, 3), (700, 40), (90, 650)])),
    ("diamond", |r| {
        polygon(r, &[(300, -20), (610, 290), (300, 600), (-10, 290)])
    }),
    ("two columns", |r| {
        rect(r, 0, 0, 70, 500);
        rect(r, 430, 30, 500, 470);
    }),
    ("overlapping squares", |r| {
        rect(r, 10, 10, 300, 300);
        rect(r, 150, 150, 450, 450);
    }),
    ("thin lines", |r| {
        rect(r, 3, 0, 9, 400);
        polygon(r, &[(0, 100), (400, 300), (400, 310), (0, 110)]);
    }),
    ("pixel boundaries", |r| {
        rect(r, 64, 64, 192, 128);
        rect(r, 32, 96, 160, 224);
    }),
    ("hole", |r| {
        rect(r, 0, 0, 400, 400);
        polygon(r, &[(100, 100), (300, 100), (300, 300), (100, 300)]);
    }),
    ("curves", |r| {
        r.move_to(p(0, 0));
        r.quad_to(p(900, 50), p(1200, 1300));
        r.cubic_to(p(700, 1600), p(-300, 1700), p(-200, 400));
        r.line_to(p(0, 0));
        r.close();
    }),
];

fn options(fill: FillRule, overlap: bool) -> RenderOptions {
    RenderOptions {
        fill,
        overlap,
        ppem: None,
    }
}

fn render(r: &mut Rasterizer, shape: Shape, o: RenderOptions) -> (i32, i32, u32, u32, Vec<u8>) {
    r.clear();
    shape(r);
    let b = r.render(o).unwrap();
    (b.left, b.top, b.width, b.rows, b.pixels.to_vec())
}

fn reference() -> Rasterizer {
    let mut r = Rasterizer::new();
    r.set_reference_sweep(true);
    r
}

#[test]
fn touched_ranges_match_the_full_sweep() {
    for (name, shape) in SHAPES {
        for fill in [FillRule::NonZero, FillRule::EvenOdd] {
            for overlap in [false, true] {
                let o = options(fill, overlap);
                let want = render(&mut reference(), shape, o);
                assert!(want.4.iter().any(|&v| v > 0), "{name}: empty");
                for cells in [8192, 1, 13, 200] {
                    let got = render(&mut Rasterizer::with_band_cells(cells), shape, o);
                    assert_eq!(
                        got, want,
                        "{name}, {fill:?}, overlap {overlap}, {cells} cells"
                    );
                }
            }
        }
    }
}

#[test]
fn nothing_is_left_from_the_previous_outline() {
    let (_, big) = SHAPES[7];
    for (name, shape) in SHAPES {
        for overlap in [false, true] {
            let o = options(FillRule::NonZero, overlap);
            for cells in [8192, 13] {
                let mut r = Rasterizer::with_band_cells(cells);
                render(&mut r, big, o);
                let again = render(&mut r, shape, o);
                let fresh = render(&mut Rasterizer::with_band_cells(cells), shape, o);
                assert_eq!(again, fresh, "{name}, overlap {overlap}, {cells} cells");
            }
        }
    }
}

/// Moves every point by `shift`, then multiplies it by `mul`.
struct Moved<'a> {
    pen: &'a mut Rasterizer,
    shift: Point,
    mul: i32,
}

impl Moved<'_> {
    fn m(&self, q: Point) -> Point {
        p(
            (q.x + self.shift.x) * self.mul,
            (q.y + self.shift.y) * self.mul,
        )
    }
}

impl Pen for Moved<'_> {
    fn move_to(&mut self, q: Point) {
        let q = self.m(q);
        self.pen.move_to(q);
    }
    fn line_to(&mut self, q: Point) {
        let q = self.m(q);
        self.pen.line_to(q);
    }
    fn quad_to(&mut self, c: Point, q: Point) {
        let (c, q) = (self.m(c), self.m(q));
        self.pen.quad_to(c, q);
    }
    fn cubic_to(&mut self, c1: Point, c2: Point, q: Point) {
        let (c1, c2, q) = (self.m(c1), self.m(c2), self.m(q));
        self.pen.cubic_to(c1, c2, q);
    }
    fn close(&mut self) {
        self.pen.close();
    }
}

/// Renders `shape` into a box narrower than it, so that columns left of
/// the box collapse into the first one and those right of it are
/// dropped (as `render_placed` with a caller's box allows).
fn clipped(r: &mut Rasterizer, shape: Shape, overlap: bool) -> Vec<u8> {
    let b = PixelBox {
        left: 2,
        top: 9,
        width: 4,
        rows: 6,
    };
    r.clear();
    shape(&mut Moved {
        pen: r,
        shift: b.shift(),
        mul: if overlap { 4 } else { 1 },
    });
    r.render_placed(b, options(FillRule::NonZero, overlap))
        .unwrap()
        .pixels
        .to_vec()
}

#[test]
fn columns_outside_the_box() {
    for (name, shape) in SHAPES {
        for overlap in [false, true] {
            let want = clipped(&mut reference(), shape, overlap);
            let got = clipped(&mut Rasterizer::new(), shape, overlap);
            assert_eq!(got, want, "{name}, overlap {overlap}");
        }
    }
}
