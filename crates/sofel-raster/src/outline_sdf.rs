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
