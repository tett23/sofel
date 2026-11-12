//! Pure functions for mapping paint commands to renderers and output
//! formats (ADR 0023).
//!
//! Ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372): `src/hb-paint.cc`
//! (`hb_paint_reduce_linear_anchors`, `hb_paint_normalize_color_line`,
//! `hb_paint_sweep_gradient_tiles`) and `src/hb-paint.hh`
//! (`hb_color_lerp`). Copyright © the HarfBuzz authors. Used under
//! HarfBuzz's "Old MIT" license; see THIRD_PARTY_NOTICES.md.

use crate::paint::{Color, ColorStop, Extend, PointF};

/// HarfBuzz's `HB_PI` as `float`.
pub(crate) const PI: f32 = core::f32::consts::PI;
/// HarfBuzz's `HB_2_PI`.
const TWO_PI: f32 = 2.0 * PI;
/// `HB_PAINT_MAX_SWEEP_TILES`.
const MAX_SWEEP_TILES: u32 = 4096;

/// `hb_paint_reduce_linear_anchors`: the two points of the gradient axis
/// of COLR's three-point linear gradient. The end point is the foot of
/// `p1` on the line through `p0` perpendicular to `p2 - p0`; when `p2`
/// is (nearly) `p0`, the points come back unchanged.
pub fn reduce_linear_anchors(p0: PointF, p1: PointF, p2: PointF) -> (PointF, PointF) {
    let (q2x, q2y) = (p2.x - p0.x, p2.y - p0.y);
    let s = q2x * q2x + q2y * q2y;
    if s < 1e-6 {
        return (p0, p1);
    }
    let (q1x, q1y) = (p1.x - p0.x, p1.y - p0.y);
    let k = (q2x * q1x + q2y * q1y) / s;
    (p0, PointF::new(p1.x - k * q2x, p1.y - k * q2y))
}

/// `(a > b) - (a < b)`: NaN compares equal to everything.
fn greater(a: f32, b: f32) -> bool {
    a > b
}

/// `hb_paint_normalize_color_line`: sorts `stops` by offset (stably) and
/// rescales the offsets to 0..=1, returning the original smallest and
/// largest offset; `(0, 0)` for no stops. When all offsets are equal they
/// are left as they are.
pub fn normalize_color_line(stops: &mut [ColorStop]) -> (f32, f32) {
    // Insertion sort: stable, no allocation, and color lines are short.
    for i in 1..stops.len() {
        let mut j = i;
        while j > 0 {
            let prev = j.saturating_sub(1);
            let (Some(a), Some(b)) = (stops.get(prev), stops.get(j)) else {
                break;
            };
            if !greater(a.offset, b.offset) {
                break;
            }
            stops.swap(prev, j);
            j = prev;
        }
    }
    let Some(first) = stops.first() else {
        return (0.0, 0.0);
    };
    let (mn, mx) = stops
        .iter()
        .skip(1)
        .fold((first.offset, first.offset), |(mn, mx), s| {
            (min(mn, s.offset), max(mx, s.offset))
        });
    if mn != mx {
        for s in stops.iter_mut() {
            s.offset = (s.offset - mn) / (mx - mn);
        }
    }
    (mn, mx)
}

/// `hb_min`: `b` unless `a < b`.
fn min(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// `hb_max`: `b` unless `a > b`.
fn max(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// `hb_color_lerp`: componentwise, rounded; out-of-range and NaN `t`
/// stay defined.
pub fn interpolate_sweep_color(c0: Color, c1: Color, t: f32) -> Color {
    let one = |v0: u8, v1: u8| {
        let v = f32::from(v0) + t * (f32::from(v1) - f32::from(v0)) + 0.5;
        if v > 255.0 {
            255
        } else if v > 0.0 {
            // 0 < v <= 255: truncation as C's conversion does.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let u = v as u8;
            u
        } else {
            0
        }
    };
    Color::new(
        // hb_color_lerp passes shifts 0, 8, 16, 24 into HB_COLOR(b,g,r,a).
        one(c0.g, c1.g),
        one(c0.r, c1.r),
        one(c0.a, c1.a),
        one(c0.b, c1.b),
    )
}

/// `hb_clamp_to<uint8_t> ((double) v + 0.5)`.
fn clamp_u8(v: f32) -> u8 {
    let d = f64::from(v) + 0.5;
    if d >= 255.0 {
        255
    } else if d > 0.0 {
        // 0 < d < 255: truncation as C's conversion does.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let u = d as u8;
        u
    } else {
        0
    }
}

/// `(float) v` for an `i64`.
fn i64_to_f32(v: i64) -> f32 {
    // Rounds to nearest, as C's conversion does.
    #[allow(clippy::cast_precision_loss)]
    let f = v as f32;
    f
}

/// `hb_paint_sweep_gradient_tiles`: splits the sweep gradient of
/// `stops` (sorted, offsets in 0..=1; see [`normalize_color_line`]) from
/// `start` to `end` (radians) into angular segments within 0..=2π,
/// passing each one's start angle and color and end angle and color to
/// `emit`. When `end < start`, `stops` is reversed in place (as
/// HarfBuzz does). At most 4096 segments are emitted.
pub fn sweep_gradient_tiles(
    stops: &mut [ColorStop],
    extend: Extend,
    start: f32,
    end: f32,
    emit: &mut dyn FnMut(f32, Color, f32, Color),
) {
    let n = stops.len();
    let (Some(first), Some(last)) = (stops.first().copied(), stops.last().copied()) else {
        return;
    };
    let mut clip = |a0: f32, c0: Color, a1: f32, c1: Color| {
        let t0 = max(a0, 0.0);
        let t1 = min(a1, TWO_PI);
        if t0 > t1 {
            return;
        }
        let (mut d0, mut d1) = (c0, c1);
        if c0 != c1 {
            if t0 != a0 {
                d0 = interpolate_sweep_color(c0, c1, (t0 - a0) / (a1 - a0));
            }
            if t1 != a1 {
                d1 = interpolate_sweep_color(c0, c1, (t1 - a0) / (a1 - a0));
            }
        }
        emit(t0, d0, t1, d1);
    };

    if start == end {
        if extend == Extend::Pad {
            if start > 0.0 {
                clip(0.0, first.color.color, start, first.color.color);
            }
            if end < TWO_PI {
                clip(end, last.color.color, TWO_PI, last.color.color);
            }
        }
        return;
    }

    let (start, end) = if end < start {
        stops.reverse();
        for s in stops.iter_mut() {
            s.offset = 1.0 - s.offset;
        }
        (end, start)
    } else {
        (start, end)
    };

    let stops: &[ColorStop] = stops;
    let angle = |i: usize| {
        stops
            .get(i)
            .map_or(0.0, |s| start + s.offset * (end - start))
    };
    let color = |i: usize| stops.get(i).map_or(Color::default(), |s| s.color.color);
    let last_i = n.saturating_sub(1);

    if extend == Extend::Pad {
        let mut color0 = color(0);
        let pos = (0..n).find(|&p| angle(p) >= 0.0);
        let Some(mut pos) = pos else {
            let c = color(last_i);
            clip(0.0, c, TWO_PI, c);
            return;
        };
        if pos > 0 {
            let p = pos.saturating_sub(1);
            let f = (0.0 - angle(p)) / (angle(pos) - angle(p));
            color0 = interpolate_sweep_color(color(p), color(pos), f);
        }
        clip(0.0, color0, angle(pos), color(pos));
        pos = pos.saturating_add(1);
        while pos < n {
            let p = pos.saturating_sub(1);
            if angle(pos) <= TWO_PI {
                clip(angle(p), color(p), angle(pos), color(pos));
            } else {
                let f = (TWO_PI - angle(p)) / (angle(pos) - angle(p));
                let c1 = interpolate_sweep_color(color(p), color(pos), f);
                clip(angle(p), color(p), TWO_PI, c1);
                break;
            }
            pos = pos.saturating_add(1);
        }
        if pos == n {
            let c = color(last_i);
            clip(angle(last_i), c, TWO_PI, c);
        }
        return;
    }

    let span = angle(last_i) - angle(0);
    // Reversed to catch NaN.
    if span.abs().partial_cmp(&1e-6).is_none_or(|o| o.is_lt()) {
        return;
    }
    let span = span.abs();

    if span < TWO_PI / 4096.0 {
        // The pattern repeats faster than the tile resolution: fill the
        // circle with the period-average color.
        let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for i in 1..n {
            let p = i.saturating_sub(1);
            let w = (angle(i) - angle(p)) / span * 0.5;
            let (c0, c1) = (color(p), color(i));
            let sum = |x: u8, y: u8| {
                // At most 510: exact in f32.
                f32::from(u16::from(x).saturating_add(u16::from(y)))
            };
            r += w * sum(c0.r, c1.r);
            g += w * sum(c0.g, c1.g);
            b += w * sum(c0.b, c1.b);
            a += w * sum(c0.a, c1.a);
        }
        let avg = Color::new(clamp_u8(r), clamp_u8(g), clamp_u8(b), clamp_u8(a));
        clip(0.0, avg, TWO_PI, avg);
        return;
    }

    let dk = floor(-f64::from(max(angle(0), angle(last_i))) / f64::from(span)) - 1.0;
    if !(-1.0e18..=1.0e18).contains(&dk) {
        return;
    }
    // |dk| <= 1e18 was checked.
    #[allow(clippy::cast_possible_truncation)]
    let mut l = dk as i64;
    let (mut iterations, mut tiles) = (0u32, 0u32);
    loop {
        for i in 1..n {
            if iterations >= 256 * MAX_SWEEP_TILES {
                return;
            }
            iterations = iterations.saturating_add(1);
            let lf = i64_to_f32(l) * span;
            let (a0, a1, col0, col1) = if l % 2 != 0 && extend == Extend::Reflect {
                let (j0, j1) = (n.saturating_sub(i), n.saturating_sub(1).saturating_sub(i));
                (
                    angle(0) + angle(last_i) - angle(j0) + lf,
                    angle(0) + angle(last_i) - angle(j1) + lf,
                    color(j0),
                    color(j1),
                )
            } else {
                let p = i.saturating_sub(1);
                (angle(p) + lf, angle(i) + lf, color(p), color(i))
            };
            if a1 < 0.0 {
                continue;
            }
            if tiles >= MAX_SWEEP_TILES {
                return;
            }
            tiles = tiles.saturating_add(1);
            if a0 < 0.0 {
                let f = (0.0 - a0) / (a1 - a0);
                clip(0.0, interpolate_sweep_color(col0, col1, f), a1, col1);
            } else if a1 >= TWO_PI {
                let f = (TWO_PI - a0) / (a1 - a0);
                clip(a0, col0, TWO_PI, interpolate_sweep_color(col0, col1, f));
                return;
            } else {
                clip(a0, col0, a1, col1);
            }
        }
        l = l.saturating_add(1);
        if n < 2 {
            return;
        }
    }
}

/// C's `floor` for the finite values used here.
fn floor(v: f64) -> f64 {
    if v.is_nan() || v.abs() >= 4_503_599_627_370_496.0 {
        return v;
    }
    // |v| < 2^52 was checked.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let t = (v as i64) as f64;
    if t > v { t - 1.0 } else { t }
}
