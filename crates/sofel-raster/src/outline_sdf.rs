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
