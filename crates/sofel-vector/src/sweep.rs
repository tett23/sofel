//! Cubic sweep-gradient sectors from HarfBuzz hb-vector-paint-svg.cc.
//! Copyright © the HarfBuzz authors. See THIRD_PARTY_NOTICES.md.
use crate::pdf::number;
use alloc::string::String;
use core::fmt::Write as _;
use sofel_geom::{
    Color, ColorLine, PointF, float::ceilf, normalize_color_line, sin_cos, sweep_gradient_tiles,
};

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}
fn dot(a: PointF, b: PointF) -> f32 {
    a.x * b.x + a.y * b.y
}
fn direction(angle: f32) -> PointF {
    let (y, x) = sin_cos(angle);
    PointF::new(x, y)
}
fn control(p: PointF, middle: PointF, tangent: PointF) -> Option<PointF> {
    let d = dot(tangent, p);
    if d.abs() < 1e-6 {
        return None;
    }
    let k = dot(PointF::new(p.x - middle.x, p.y - middle.y), p) / d;
    let c = PointF::new(middle.x + tangent.x * k, middle.y + tangent.y * k);
    Some(PointF::new(
        c.x + (c.x - p.x) * 0.33333,
        c.y + (c.y - p.y) * 0.33333,
    ))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn patch(out: &mut String, center: PointF, a0: f32, c0: Color, a1: f32, c1: Color) {
    // Tiles have already been clipped to one turn.
    let n = ceilf((a1 - a0).abs() / (core::f32::consts::PI / 16.)).max(1.) as u8;
    let point = |p: PointF| {
        alloc::format!(
            "{},{}",
            number(center.x + p.x * 32767., 2),
            number(center.y + p.y * 32767., 2)
        )
    };
    let mut p0 = direction(a0);
    for step in 1..=n {
        let k = f32::from(step) / f32::from(n);
        let prev = f32::from(step.saturating_sub(1)) / f32::from(n);
        let p1 = direction(lerp(a0, a1, k));
        let sum = PointF::new(p0.x + p1.x, p0.y + p1.y);
        // The sector is at most pi/16 wide, so the sum of the two unit
        // vectors has length close to two. Refine in f64 before rounding to f32.
        let length2 = f64::from(dot(sum, sum));
        let mut length = 2.;
        for _ in 0..4 {
            length = (length + length2 / length) * 0.5;
        }
        let inv = 1. / length as f32;
        let mid = PointF::new(sum.x * inv, sum.y * inv);
        let tangent = PointF::new(-mid.y, mid.x);
        if let (Some(ca), Some(cb)) = (control(p0, mid, tangent), control(p1, mid, tangent)) {
            let channel = |a: u8, b: u8| {
                let (a, b) = (f32::from(a) / 255., f32::from(b) / 255.);
                let value = lerp(lerp(a, b, prev), lerp(a, b, k), 0.5).clamp(0., 1.) * 255.;
                (value + 0.5) as u8
            };
            let color = Color::new(
                channel(c0.r, c1.r),
                channel(c0.g, c1.g),
                channel(c0.b, c1.b),
                channel(c0.a, c1.a),
            );
            let _ = write!(
                out,
                "<path d=\"M{},{}L{}C{} {} {}Z\"",
                number(center.x, 2),
                number(center.y, 2),
                point(p0),
                point(ca),
                point(cb),
                point(p1)
            );
            super::fill_color(out, color);
            out.push_str("/>\n");
        }
        p0 = p1;
    }
}

pub(super) fn write(
    out: &mut String,
    center: PointF,
    line: ColorLine<'_>,
    start: f32,
    end: f32,
) -> bool {
    let mut stops = line.stops.to_vec();
    let (min, max) = normalize_color_line(&mut stops);
    let mut fits = true;
    sweep_gradient_tiles(
        &mut stops,
        line.extend,
        lerp(start, end, min),
        lerp(start, end, max),
        &mut |a, c, b, d| {
            if out.len() < (16 << 20) {
                patch(out, center, a, c, b, d);
            } else {
                fits = false;
            }
        },
    );
    fits && out.len() <= (16 << 20)
}
