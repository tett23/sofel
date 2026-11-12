//! Cubic-to-quadratic approximation from HarfBuzz hb-gpu-cu2qu.hh.
//! Copyright © the HarfBuzz authors. See THIRD_PARTY_NOTICES.md.
use super::{Outline, Point};

fn midpoint(a: Point, b: Point) -> Point {
    [a[0] + (b[0] - a[0]) * 0.5, a[1] + (b[1] - a[1]) * 0.5]
}
fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
fn mul(a: Point, v: f64) -> Point {
    [a[0] * v, a[1] * v]
}
fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn square(a: Point) -> f64 {
    a[0] * a[0] + a[1] * a[1]
}

fn inside([p0, p1, p2, p3]: [Point; 4], depth: u8) -> bool {
    if square(p1) <= 0.25 && square(p2) <= 0.25 {
        return true;
    }
    if depth == 8 {
        return false;
    }
    let mid = mul(add(add(p0, mul(add(p1, p2), 3.)), p3), 0.125);
    if square(mid) > 0.25 {
        return false;
    }
    let delta = mul(sub(sub(add(p3, p2), p1), p0), 0.125);
    let next = depth.saturating_add(1);
    inside([p0, midpoint(p0, p1), sub(mid, delta), mid], next)
        && inside([mid, add(mid, delta), midpoint(p2, p3), p3], next)
}

fn control([p0, p1, p2, p3]: [Point; 4]) -> Option<Point> {
    let a = sub(p1, p0);
    let d = sub(p3, p2);
    let denom = cross(a, d);
    if denom.abs() < 1e-12 {
        return None;
    }
    let q = add(p2, mul(d, cross(a, sub(p0, p2)) / denom));
    let e1 = sub(add(p0, mul(sub(q, p0), 2. / 3.)), p1);
    let e2 = sub(add(p3, mul(sub(q, p3), 2. / 3.)), p2);
    inside([[0., 0.], e1, e2, [0., 0.]], 0).then_some(q)
}

fn subdivide(outline: &mut Outline, curve: [Point; 4], depth: u8) {
    let [a, b, c, d] = curve;
    if outline.overflow {
        return;
    }
    if let Some(q) = control(curve) {
        outline.conic(q, d);
        return;
    }
    if depth == 10 {
        outline.conic(outline.current, d);
        return;
    }
    let ab = midpoint(a, b);
    let bc = midpoint(b, c);
    let cd = midpoint(c, d);
    let left = midpoint(ab, bc);
    let right = midpoint(bc, cd);
    let mid = midpoint(left, right);
    let next = depth.saturating_add(1);
    subdivide(outline, [a, ab, left, mid], next);
    subdivide(outline, [mid, right, cd, d], next);
}

pub(super) fn append(outline: &mut Outline, curve: [Point; 4]) {
    let [a, b, c, d] = curve;
    if !curve.iter().flatten().all(|v| v.is_finite()) {
        outline.overflow = true;
        return;
    }
    if a == d && b == d && c == d {
        return;
    }
    let direction = sub(d, a);
    let length = square(direction);
    if length > 0. {
        let inv = 1. / length;
        let close = |p| {
            let cross = cross(sub(p, a), direction);
            cross * cross * inv <= 0.25
        };
        if close(b) && close(c) {
            outline.conic(a, d);
            return;
        }
    }
    subdivide(outline, curve, 0);
}
