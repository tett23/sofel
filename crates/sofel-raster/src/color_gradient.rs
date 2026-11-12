//! COLRv1 gradients in premultiplied BGRA, following HarfBuzz 15.1.0
//! hb-raster-paint.cc (Old MIT, see THIRD_PARTY_NOTICES.md).
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
use alloc::vec::Vec;
use sofel_geom::{
    Brush, Color, ColorStop, Extend, PointF, float::floorf, normalize_color_line,
    reduce_linear_anchors,
};

enum Geometry {
    Linear(PointF, f32, f32, f32),
    Radial(PointF, f32, f32, f32, f32, f32),
}

/// A normalized color line and its geometry in glyph space.
pub struct ColorGradient {
    stops: Vec<ColorStop>,
    extend: Extend,
    geometry: Geometry,
    quantized: bool,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}
fn premul(c: Color) -> [f32; 4] {
    let a = f32::from(c.a) / 255.;
    [
        f32::from(c.b) / 255. * a,
        f32::from(c.g) / 255. * a,
        f32::from(c.r) / 255. * a,
        a,
    ]
}
fn endpoint(c: Color) -> [u8; 4] {
    let mul =
        |v: u8| (((u32::from(v).saturating_mul(u32::from(c.a))).saturating_add(255)) >> 8) as u8;
    [mul(c.b), mul(c.g), mul(c.r), c.a]
}
fn byte(v: f32) -> u8 {
    (v * 255. + 0.5) as u8
}

impl ColorGradient {
    /// Prepare a linear or radial brush for repeated pixel samples.
    pub fn new(brush: &Brush<'_>, clip_pixels: usize) -> Option<Self> {
        let line = match brush {
            Brush::Linear { line, .. } | Brush::Radial { line, .. } => line,
            _ => return None,
        };
        let mut stops = line.stops.to_vec();
        let (min, max) = normalize_color_line(&mut stops);
        let geometry = match *brush {
            Brush::Linear { p0, p1, p2, .. } => {
                let (a, b) = reduce_linear_anchors(p0, p1, p2);
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let start = PointF::new(a.x + min * dx, a.y + min * dy);
                let dx = a.x + max * dx - start.x;
                let dy = a.y + max * dy - start.y;
                let denom = dx * dx + dy * dy;
                Geometry::Linear(start, dx, dy, if denom > 0. { 1. / denom } else { 0. })
            }
            Brush::Radial { c0, c1, r0, r1, .. } => {
                let center = PointF::new(lerp(c0.x, c1.x, min), lerp(c0.y, c1.y, min));
                let dx = lerp(c0.x, c1.x, max) - center.x;
                let dy = lerp(c0.y, c1.y, max) - center.y;
                let r = lerp(r0, r1, min);
                let dr = lerp(r0, r1, max) - r;
                Geometry::Radial(center, r, dx, dy, dr, dx * dx + dy * dy - dr * dr)
            }
            _ => return None,
        };
        Some(Self {
            stops,
            extend: line.extend,
            geometry,
            quantized: clip_pixels >= 64 * 64,
        })
    }

    /// Sample at a glyph-space point. The callback supplies IEEE single-precision square root.
    pub fn sample(&self, p: PointF, sqrt: impl Fn(f32) -> f32) -> [u8; 4] {
        let t = match self.geometry {
            Geometry::Linear(start, dx, dy, inv) => {
                ((p.x - start.x) * dx + (p.y - start.y) * dy) * inv
            }
            Geometry::Radial(center, r, dx, dy, dr, a) => {
                let x = p.x - center.x;
                let y = p.y - center.y;
                let b = -2. * (x * dx + y * dy + r * dr);
                let c = x * x + y * y - r * r;
                if a.abs() > 1e-10 {
                    let disc = b * b - 4. * a * c;
                    if disc < 0. {
                        return [0; 4];
                    }
                    let root = sqrt(disc);
                    let t = (-b + root) / (2. * a);
                    if r + t * dr >= 0. {
                        t
                    } else {
                        (-b - root) / (2. * a)
                    }
                } else if b.abs() >= 1e-10 {
                    -c / b
                } else {
                    return [0; 4];
                }
            }
        };
        let t = if t.is_finite() { t } else { 0. };
        let t = match self.extend {
            Extend::Pad => t.clamp(0., 1.),
            Extend::Repeat => t - floorf(t),
            Extend::Reflect => {
                let u = t.abs() % 2.;
                if u > 1. { 2. - u } else { u }
            }
        };
        let t = if self.quantized {
            ((t.clamp(0., 1.) * 255. + 0.5) as u8) as f32 / 255.
        } else {
            t
        };
        let Some(first) = self.stops.first() else {
            return [0; 4];
        };
        if t <= first.offset {
            return endpoint(first.color.color);
        }
        let Some((a, b)) = self
            .stops
            .windows(2)
            .filter_map(|s| s.first().zip(s.get(1)))
            .find(|(_, b)| t < b.offset)
        else {
            return self
                .stops
                .last()
                .map_or([0; 4], |s| endpoint(s.color.color));
        };
        let k = (t - a.offset) / (b.offset - a.offset);
        let aa = premul(a.color.color);
        let bb = premul(b.color.color);
        let mut result = [0; 4];
        for ((channel, a), b) in result.iter_mut().zip(aa).zip(bb) {
            *channel = byte(lerp(a, b, k));
        }
        result
    }
}
