#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adr0064_i2_outline_sdf_projects_inside_outside_and_at_line_endpoints() {
        let p = |x, y| sofel_geom::Point { x, y };
        for (a, b, q, expected) in [
            (p(0, 0), p(128, 0), p(64, 32), (32768, -1, 65536)),
            (p(0, 0), p(128, 0), p(144, 32), (36636, -1, -58617)),
            (p(0, 0), p(128, 0), p(0, 0), (0, -1, 0)),
            (p(12, 32), p(12, 32), p(20, 30), (8444, -1, 0)),
        ] {
            let d = line_distance(a, b, q);
            assert_eq!((d.distance, d.sign, d.cross), expected);
        }
    }
}

use super::sdf_math::unit_vector;
use sofel_geom::{Point, div_fix, mul_fix, vector_length};

struct SignedDistance {
    distance: i32,
    sign: i32,
    cross: i32,
}

// FreeType divides each 26.6 product before adding the dot product.
// The distance and the normalized corner cross product are 16.16.
fn line_distance(a: Point, b: Point, p: Point) -> SignedDistance {
    let dx = b.x.wrapping_sub(a.x);
    let dy = b.y.wrapping_sub(a.y);
    let px = p.x.wrapping_sub(a.x);
    let py = p.y.wrapping_sub(a.y);
    let dot = |x: i32, y: i32| {
        ((i64::from(x) * i64::from(dx) / 64) + (i64::from(y) * i64::from(dy) / 64)) as i32
    };
    let factor = div_fix(dot(px, py), dot(dx, dy)).clamp(0, 65536);
    let vx = mul_fix(dx.wrapping_mul(1024), factor).wrapping_sub(px.wrapping_mul(1024));
    let vy = mul_fix(dy.wrapping_mul(1024), factor).wrapping_sub(py.wrapping_mul(1024));
    let side = mul_fix(vx, dy).wrapping_sub(mul_fix(vy, dx));
    let cross = if factor == 0 || factor == 65536 {
        let (sx, sy) = unit_vector(dx, dy);
        let (nx, ny) = unit_vector(vx, vy);
        mul_fix(sx, ny) - mul_fix(sy, nx)
    } else {
        65536
    };
    SignedDistance {
        distance: vector_length(vx, vy),
        sign: if side < 0 { 1 } else { -1 },
        cross,
    }
}

#[test]
fn adr0064_i2_subdivided_edges_preserve_reference_rounding_and_order() {
    let p = |x, y| Point { x, y };
    let mut edges = alloc::vec::Vec::new();
    flatten_curve(Curve::Quad([p(-9, -5), p(11, 29), p(23, 1)]), &mut edges);
    assert_eq!(
        edges,
        [
            (p(9, 13), p(16, 11)),
            (p(16, 11), p(23, 1)),
            (p(-9, -5), p(0, 8)),
            (p(0, 8), p(9, 13))
        ]
    );
    edges.clear();
    flatten_curve(
        Curve::Cubic([p(-9, 0), p(-5, 0), p(2, 0), p(10, 0)]),
        &mut edges,
    );
    assert_eq!(edges, [(p(-9, 0), p(-1, 0)), (p(-1, 0), p(10, 0))]);
}

enum Curve {
    Quad([Point; 3]),
    Cubic([Point; 4]),
}

fn flatten_curve(curve: Curve, edges: &mut alloc::vec::Vec<(Point, Point)>) {
    let splits = match curve {
        Curve::Quad([a, b, c]) => {
            let bend = |a, b, c| (i64::from(a) + i64::from(c) - 2 * i64::from(b)).abs();
            let mut deviation = bend(a.x, b.x, c.x).max(bend(a.y, b.y, c.y));
            let mut n = 1;
            while deviation > 8 {
                deviation >>= 2;
                n *= 2;
            }
            n
        }
        Curve::Cubic(_) => 32,
    };
    split_curve(curve, splits, edges);
}

fn split_curve(curve: Curve, splits: u32, edges: &mut alloc::vec::Vec<(Point, Point)>) {
    let add = |a: Point, b: Point| Point {
        x: a.x.wrapping_add(b.x),
        y: a.y.wrapping_add(b.y),
    };
    let div = |a: Point, n| Point {
        x: a.x / n,
        y: a.y / n,
    };
    let (first, second, start, mid, end, flat) = match curve {
        Curve::Quad([a, b, c]) => {
            let (ab, bc) = (add(a, b), add(b, c));
            let mid = div(add(ab, bc), 4);
            (
                Curve::Quad([a, div(ab, 2), mid]),
                Curve::Quad([mid, div(bc, 2), c]),
                a,
                mid,
                c,
                false,
            )
        }
        Curve::Cubic([a, b, c, d]) => {
            let (ab, bc, cd) = (add(a, b), add(b, c), add(c, d));
            let (abc, bcd) = (add(ab, bc), add(bc, cd));
            let mid = div(add(abc, bcd), 8);
            let flat = |a, b, c, d| {
                let (a, b, c, d) = (i64::from(a), i64::from(b), i64::from(c), i64::from(d));
                (2 * a - 3 * b + d).abs() < 16 && (a - 3 * c + 2 * d).abs() < 16
            };
            (
                Curve::Cubic([a, div(ab, 2), div(abc, 4), mid]),
                Curve::Cubic([mid, div(bcd, 4), div(cd, 2), d]),
                a,
                mid,
                d,
                flat(a.x, b.x, c.x, d.x) && flat(a.y, b.y, c.y, d.y),
            )
        }
    };
    if splits <= 2 || flat {
        edges.extend([(start, mid), (mid, end)]);
    } else {
        // FreeType prepends each pair. Order matters when a pixel lies
        // at the same distance and corner angle from two segments.
        split_curve(second, splits / 2, edges);
        split_curve(first, splits / 2, edges);
    }
}

#[test]
fn adr0064_i2_outline_sdf_known_square_and_interior_saturation() {
    let p = |x, y| Point { x, y };
    let edges = [
        (p(512, 512), p(512, 1792)),
        (p(512, 1792), p(1792, 1792)),
        (p(1792, 1792), p(1792, 512)),
        (p(1792, 512), p(512, 512)),
    ];
    let pixels = draw_edges(&edges, 36, 36, 8, false).unwrap();
    assert_eq!(pixels[18 * 36 + 18], 255);
    assert_eq!(pixels[18 * 36 + 7], 120);
    assert_eq!(pixels[18 * 36 + 8], 136);
    assert_eq!(pixels[0], 0);
}

fn draw_edges(
    edges: &[(Point, Point)],
    width: usize,
    rows: usize,
    spread: u8,
    fill_left: bool,
) -> Result<alloc::vec::Vec<u8>, super::RasterError> {
    let len = width
        .checked_mul(rows)
        .ok_or(super::RasterError::TooLarge)?;
    if len > i32::MAX as usize {
        return Err(super::RasterError::TooLarge);
    }
    let limit = i32::from(spread) * 65536;
    let mut distances = alloc::vec::Vec::<Option<SignedDistance>>::new();
    distances
        .try_reserve_exact(len)
        .map_err(|_| super::RasterError::TooLarge)?;
    distances.resize_with(len, || None);
    let interval = |a: i32, b: i32, extent: usize| {
        let lo = (i64::from(a.min(b)) - 63) / 64 - i64::from(spread);
        let hi = (i64::from(a.max(b)) + 63) / 64 + i64::from(spread);
        lo.max(0) as usize..hi.clamp(0, extent as i64) as usize
    };
    for &(a, b) in edges {
        let xs = interval(a.x, b.x, width);
        for y in interval(a.y, b.y, rows) {
            for x in xs.clone() {
                let p = Point {
                    x: (x as i32) * 64 + 32,
                    y: (y as i32) * 64 + 32,
                };
                let mut d = line_distance(a, b, p);
                if d.distance > limit {
                    continue;
                }
                if fill_left {
                    d.sign = -d.sign;
                }
                let cell = &mut distances[(rows - 1 - y) * width + x];
                if cell.as_ref().is_none_or(|previous| {
                    if previous.distance.abs_diff(d.distance) <= 32 {
                        previous.cross.abs() <= d.cross.abs()
                    } else {
                        previous.distance > d.distance
                    }
                }) {
                    *cell = Some(d);
                }
            }
        }
    }
    let mut pixels = alloc::vec::Vec::new();
    pixels
        .try_reserve_exact(len)
        .map_err(|_| super::RasterError::TooLarge)?;
    for row in distances.chunks(width.max(1)) {
        let mut inside = false;
        for value in row {
            let d = value.as_ref().map_or(limit, |d| {
                inside = d.sign > 0;
                d.distance
            });
            pixels.push(super::bitmap_sdf::encode(
                d,
                if inside { 255 } else { 0 },
                spread,
            ));
        }
    }
    Ok(pixels)
}

impl super::Rasterizer {
    /// Render an outline translated to the bottom-left of its padded pixel
    /// box. The caller supplies FreeType's fill orientation and distance spread.
    ///
    /// # Errors
    /// Returns an error for invalid spread, bounds, or insufficient memory.
    pub fn render_outline_sdf_placed(
        &mut self,
        bounds: super::PixelBox,
        spread: u8,
        fill_left: bool,
    ) -> Result<super::Bitmap<'_>, super::BitmapSdfError> {
        use sofel_geom::Command;
        if !(2..=32).contains(&spread) {
            return Err(super::BitmapSdfError::InvalidSpread);
        }
        if super::too_large(
            bounds.left.into(),
            bounds.top.into(),
            bounds.width.into(),
            bounds.rows.into(),
            None,
        ) {
            return Err(super::BitmapSdfError::TooLarge);
        }
        let mut edges = alloc::vec::Vec::new();
        for contour in self
            .commands
            .chunk_by(|_, next| !matches!(next, Command::MoveTo(_)))
        {
            let Some((&Command::MoveTo(start), commands)) = contour.split_first() else {
                continue;
            };
            let mut last = start;
            for &command in commands {
                match command {
                    Command::LineTo(end) => {
                        if last != end {
                            edges.push((last, end));
                        }
                        last = end;
                    }
                    Command::QuadTo(c, end) => {
                        if c == last || c == end {
                            if last != end {
                                edges.push((last, end));
                            }
                        } else {
                            flatten_curve(Curve::Quad([last, c, end]), &mut edges);
                        }
                        last = end;
                    }
                    Command::CubicTo(c, d, end) => {
                        flatten_curve(Curve::Cubic([last, c, d, end]), &mut edges);
                        last = end;
                    }
                    Command::Close | Command::MoveTo(_) => break,
                }
            }
            if last != start {
                edges.push((last, start));
            }
        }
        self.pixels = draw_edges(
            &edges,
            bounds.width as usize,
            bounds.rows as usize,
            spread,
            fill_left,
        )
        .map_err(|_| super::BitmapSdfError::TooLarge)?;
        Ok(super::Bitmap {
            left: bounds.left,
            top: bounds.top,
            width: bounds.width,
            rows: bounds.rows,
            pixels: &self.pixels,
        })
    }
}
