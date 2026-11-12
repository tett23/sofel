//! Gradient payloads from HarfBuzz 15.1.0 src/hb-gpu-paint.cc.
//! Copyright © 2026 Behdad Esfahbod (Old MIT). See THIRD_PARTY_NOTICES.md.
use super::{OutOfRange, Texel};
use alloc::{vec, vec::Vec};
use sofel_geom::{
    Brush, Color, ColorLine, PointF, Transform, normalize_color_line, reduce_linear_anchors,
};

pub(super) fn rgba(c: Color) -> Texel {
    [c.r, c.g, c.b, c.a].map(|v| {
        let q = u32::from(v).saturating_mul(32767).saturating_add(127) / 255;
        i16::try_from(q).unwrap_or(i16::MAX)
    })
}

#[allow(clippy::cast_possible_truncation)] // Rust's saturating cast includes NaN -> 0.
fn word(v: f32) -> i16 {
    v as i16
}

pub(super) struct Gradient {
    pub kind: i16,
    pub count: i16,
    pub extend: i16,
    pub data: Vec<Texel>,
}

pub(super) fn line<'a>(brush: &Brush<'a>) -> Option<ColorLine<'a>> {
    match brush {
        Brush::Solid(_) => None,
        Brush::Linear { line, .. } | Brush::Radial { line, .. } | Brush::Sweep { line, .. } => {
            Some(*line)
        }
    }
}

impl Gradient {
    pub fn new(brush: &Brush<'_>, line: ColorLine<'_>, t: Transform) -> Result<Self, OutOfRange> {
        let count = i16::try_from(line.stops.len()).map_err(|_| OutOfRange)?;
        let mut stops = line.stops.to_vec();
        let (min, max) = normalize_color_line(&mut stops);
        let between = |a: f32, b: f32, v: f32| a + v * (b - a);
        let frame = |a: PointF, b: PointF| {
            let start = PointF::new(between(a.x, b.x, min), between(a.y, b.y, min));
            let end = PointF::new(between(a.x, b.x, max), between(a.y, b.y, max));
            let origin = t.apply(start);
            [
                word(origin.x),
                word(origin.y),
                word(end.x - start.x),
                word(end.y - start.y),
            ]
        };
        let (kind, mut data) = match *brush {
            Brush::Linear { p0, p1, p2, .. } => {
                let (a, b) = reduce_linear_anchors(p0, p1, p2);
                (0, vec![frame(a, b)])
            }
            Brush::Radial { c0, c1, r0, r1, .. } => (
                1,
                vec![
                    frame(c0, c1),
                    [word(between(r0, r1, min)), word(between(r0, r1, max)), 0, 0],
                ],
            ),
            Brush::Sweep {
                center, start, end, ..
            } => {
                let c = t.apply(center);
                let angle =
                    |v| word(between(start, end, v) * core::f32::consts::FRAC_1_PI * 16384.);
                (2, vec![[word(c.x), word(c.y), angle(min), angle(max)]])
            }
            Brush::Solid(_) => return Err(OutOfRange),
        };
        let determinant = t.xx * t.yy - t.xy * t.yx;
        let inv = if determinant.abs() > 1e-12 {
            1. / determinant
        } else {
            0.
        };
        data.push([t.yy, -t.xy, -t.yx, t.xx].map(|v| word(v * inv * 1024.)));
        for stop in stops {
            data.push([
                word(stop.offset.clamp(-1., 1.) * 32767.),
                i16::from(stop.color.source.index.is_none()),
                0,
                0,
            ]);
            data.push(rgba(stop.color.color));
        }
        Ok(Self {
            kind,
            data,
            count,
            extend: i16::from(line.extend as u8),
        })
    }
}
