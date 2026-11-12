//! SVG path geometry in document units. The bounds are computed once, before
//! pixel sampling; curve control boxes are not the object bounding box.
use super::transform::{separator, take_number, whitespace};
use alloc::vec::Vec;
use sofel_geom::{Pen, Point};

type Pair = (f32, f32);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Segment {
    Move(Pair),
    Line(Pair),
    Quad(Pair, Pair),
    Cubic(Pair, Pair, Pair),
    Close,
}

/// A parsed SVG path with its untransformed geometric bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    segments: Vec<Segment>,
    pub(crate) bounds: (f32, f32, f32, f32),
}

fn sqrt(x: f64) -> f64 {
    if x <= 0. {
        return 0.;
    }
    let mut y = f64::from_bits((x.to_bits() >> 1).saturating_add(0x1ff8_0000_0000_0000));
    for _ in 0..6 {
        y = 0.5 * (y + x / y);
    }
    y
}

#[derive(Default)]
struct Bounds(Option<(f32, f32, f32, f32)>);

impl Bounds {
    fn point(&mut self, (x, y): Pair) {
        self.0 = Some(self.0.map_or((x, y, x, y), |(a, b, c, d)| {
            (a.min(x), b.min(y), c.max(x), d.max(y))
        }));
    }

    #[allow(clippy::cast_possible_truncation)] // Evaluate in f64 before rounding to the stored f32 bounds.
    fn cubic(&mut self, a: Pair, b: Pair, c: Pair, d: Pair) {
        self.point(a);
        self.point(d);
        let eval = |a: f32, b: f32, c: f32, d: f32, t: f64| {
            let u = 1. - t;
            (u * u * u * f64::from(a)
                + 3. * u * u * t * f64::from(b)
                + 3. * u * t * t * f64::from(c)
                + t * t * t * f64::from(d)) as f32
        };
        for (v0, v1, v2, v3) in [(a.0, b.0, c.0, d.0), (a.1, b.1, c.1, d.1)] {
            let [v0, v1, v2, v3] = [v0, v1, v2, v3].map(f64::from);
            let q = v3 - 3. * v2 + 3. * v1 - v0;
            let l = 2. * (v2 - 2. * v1 + v0);
            let k = v1 - v0;
            let disc = l * l - 4. * q * k;
            if disc < 0. {
                continue;
            }
            let roots = if q == 0. {
                [-k / l, f64::NAN]
            } else {
                // Product of the roots is k/q; avoid subtracting near-equals.
                let z = -0.5 * (l + sqrt(disc).copysign(l));
                [z / q, k / z]
            };
            for t in roots {
                if t > 0. && t < 1. {
                    self.point((eval(a.0, b.0, c.0, d.0, t), eval(a.1, b.1, c.1, d.1, t)));
                }
            }
        }
    }
}

fn reflected(previous: Option<Pair>, (x, y): Pair) -> Pair {
    previous.map_or((x, y), |(a, b)| (x + (x - a), y + (y - b)))
}

fn finite((x, y): Pair) -> bool {
    x.is_finite() && y.is_finite()
}

// SVG 1.1 implementation notes F.6: convert endpoint parameters to a unit
// circle, then approximate each sweep of at most 90 degrees by one cubic.
#[allow(clippy::cast_possible_truncation)] // Map in f64 and reject nonfinite f32 controls before storing.
fn push_arc(
    segments: &mut Vec<Segment>,
    bounds: &mut Bounds,
    from: Pair,
    end: Pair,
    ellipse: [f32; 3],
    large: bool,
    sweep: bool,
) -> Option<()> {
    if from == end {
        return Some(());
    }
    let [rx, ry, degrees] = ellipse;
    let (mut rx, mut ry) = (f64::from(rx.abs()), f64::from(ry.abs()));
    if rx == 0. || ry == 0. {
        segments.push(Segment::Line(end));
        bounds.point(from);
        bounds.point(end);
        return Some(());
    }
    let (sin, cos) = sofel_geom::sin_cos((degrees % 360.).to_radians());
    let (sin, cos) = (f64::from(sin), f64::from(cos));
    let (x0, y0) = (f64::from(from.0), f64::from(from.1));
    let (x1, y1) = (f64::from(end.0), f64::from(end.1));
    let (dx, dy) = ((x0 - x1) * 0.5, (y0 - y1) * 0.5);
    let (mut x, mut y) = ((cos * dx + sin * dy) / rx, (-sin * dx + cos * dy) / ry);
    let mut q = x * x + y * y;
    if q > 1. {
        let scale = sqrt(q);
        rx *= scale;
        ry *= scale;
        x /= scale;
        y /= scale;
        q = 1.;
    }
    let sign = if sweep { 1. } else { -1. };
    let factor = (if large == sweep { -1. } else { 1. }) * sqrt((1. - q) / q);
    let (cx, cy) = (factor * y, -factor * x);
    let map = |u: f64, v: f64| {
        let (x, y) = (rx * (u + cx), ry * (v + cy));
        (
            ((x0 + x1) * 0.5 + cos * x - sin * y) as f32,
            ((y0 + y1) * 0.5 + sin * x + cos * y) as f32,
        )
    };
    let mut start = (x - cx, y - cy);
    let finish = (-x - cx, -y - cy);
    let mut previous = from;
    for _ in 0..4 {
        let (sx, sy) = start;
        let cross = sx * finish.1 - sy * finish.0;
        let dot = sx * finish.0 + sy * finish.1;
        // A tiny tolerance keeps quarter-turns on one segment after rotation.
        let last = cross * sign >= 0.0 && dot >= -1e-12;
        let (ex, ey) = if last {
            finish
        } else {
            (-sign * sy, sign * sx)
        };
        let one_plus_dot = 1. + (sx * ex + sy * ey).clamp(-1., 1.);
        let k = (4. / 3.) * (sx * ey - sy * ex) / (one_plus_dot + sqrt(2. * one_plus_dot));
        let b = map(sx - k * sy, sy + k * sx);
        let c = map(ex + k * ey, ey - k * ex);
        let p = if last { end } else { map(ex, ey) };
        if !finite(b) || !finite(c) || !finite(p) {
            return None;
        }
        segments.push(Segment::Cubic(b, c, p));
        bounds.cubic(previous, b, c, p);
        if last {
            return Some(());
        }
        previous = p;
        start = (ex, ey);
    }
    None
}

impl Path {
    pub(crate) fn parse(mut input: &str) -> Option<Self> {
        let mut segments = Vec::new();
        let mut bounds = Bounds::default();
        let mut current = (0., 0.);
        let mut start = current;
        let (mut quad, mut cubic) = (None, None);
        let mut command = None;
        input = whitespace(input);
        while let Some(&first) = input.as_bytes().first() {
            if first.is_ascii_alphabetic() {
                command = Some(first);
                input = whitespace(input.get(1..)?);
            } else {
                input = separator(input);
            }
            let cmd = command?;
            let kind = cmd.to_ascii_uppercase();
            if segments.is_empty() && kind != b'M' {
                return None;
            }
            let origin = if cmd.is_ascii_lowercase() {
                current
            } else {
                (0., 0.)
            };
            let count = match kind {
                b'M' | b'L' | b'T' => 2,
                b'H' | b'V' => 1,
                b'Q' | b'S' => 4,
                b'C' => 6,
                b'A' => 7,
                b'Z' => 0,
                _ => return None,
            };
            let mut values = [0.; 7];
            for (index, value) in values.get_mut(..count)?.iter_mut().enumerate() {
                if index != 0 {
                    input = separator(input);
                }
                *value = if kind == b'A' && matches!(index, 3 | 4) {
                    let flag = match input.as_bytes().first()? {
                        b'0' => 0.,
                        b'1' => 1.,
                        _ => return None,
                    };
                    input = input.get(1..)?;
                    flag
                } else {
                    take_number(&mut input)?
                };
            }
            let [a, b, c, d, e, f, g] = values;
            let pt = |x, y| (origin.0 + x, origin.1 + y);
            if kind == b'A' {
                let end = pt(f, g);
                if !finite(end) {
                    return None;
                }
                push_arc(
                    &mut segments,
                    &mut bounds,
                    current,
                    end,
                    [a, b, c],
                    d != 0.,
                    e != 0.,
                )?;
                current = end;
                quad = None;
                cubic = None;
                input = whitespace(input);
                continue;
            }
            let (segment, end) = match kind {
                b'M' => {
                    let p = pt(a, b);
                    start = p;
                    command = Some(if cmd == b'M' { b'L' } else { b'l' });
                    (Segment::Move(p), p)
                }
                b'L' => {
                    let p = pt(a, b);
                    (Segment::Line(p), p)
                }
                b'H' => {
                    let p = (origin.0 + a, current.1);
                    (Segment::Line(p), p)
                }
                b'V' => {
                    let p = (current.0, origin.1 + a);
                    (Segment::Line(p), p)
                }
                b'Q' => {
                    let p = pt(c, d);
                    (Segment::Quad(pt(a, b), p), p)
                }
                b'T' => {
                    let p = pt(a, b);
                    (Segment::Quad(reflected(quad, current), p), p)
                }
                b'C' => {
                    let p = pt(e, f);
                    (Segment::Cubic(pt(a, b), pt(c, d), p), p)
                }
                b'S' => {
                    let p = pt(c, d);
                    (Segment::Cubic(reflected(cubic, current), pt(a, b), p), p)
                }
                b'Z' => {
                    command = None;
                    (Segment::Close, start)
                }
                _ => return None,
            };
            if !finite(end) {
                return None;
            }
            quad = None;
            cubic = None;
            match segment {
                Segment::Quad(control, p) => {
                    if !finite(control) {
                        return None;
                    }
                    quad = Some(control);
                    // Degree elevation preserves the quadratic exactly.
                    let elevated = |a: Pair| {
                        (
                            (a.0 / 3.) + control.0 * (2. / 3.),
                            (a.1 / 3.) + control.1 * (2. / 3.),
                        )
                    };
                    bounds.cubic(current, elevated(current), elevated(p), p);
                }
                Segment::Cubic(b, c, p) => {
                    if !finite(b) || !finite(c) {
                        return None;
                    }
                    cubic = Some(c);
                    bounds.cubic(current, b, c, p);
                }
                Segment::Line(p) => {
                    bounds.point(current);
                    bounds.point(p);
                }
                _ => {}
            }
            segments.push(segment);
            current = end;
            input = whitespace(input);
        }
        Some(Self {
            segments,
            bounds: bounds.0?,
        })
    }

    pub(crate) fn draw(&self, point: impl Fn(f32, f32) -> Point, pen: &mut impl Pen) {
        let mut start = None;
        let mut end = None;
        let close = |pen: &mut _, start: Option<Point>, end: Option<Point>| {
            if let Some(first) = start {
                if Some(first) != end {
                    Pen::line_to(pen, first);
                }
                Pen::close(pen);
            }
        };
        let p = |(x, y)| point(x, y);
        for segment in &self.segments {
            match *segment {
                Segment::Move(to) => {
                    close(pen, start, end);
                    let to = p(to);
                    pen.move_to(to);
                    start = Some(to);
                    end = start;
                }
                Segment::Close => {
                    close(pen, start, end);
                    end = start.or(end);
                    start = None;
                }
                edge => {
                    // A drawing command after Z starts at the preceding subpath origin.
                    if start.is_none()
                        && let Some(last) = end
                    {
                        pen.move_to(last);
                        start = end;
                    }
                    end = Some(match edge {
                        Segment::Line(to) => {
                            let to = p(to);
                            pen.line_to(to);
                            to
                        }
                        Segment::Quad(b, to) => {
                            let to = p(to);
                            pen.quad_to(p(b), to);
                            to
                        }
                        Segment::Cubic(b, c, to) => {
                            let to = p(to);
                            pen.cubic_to(p(b), p(c), to);
                            to
                        }
                        _ => continue,
                    });
                }
            }
        }
        close(pen, start, end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0066_i3_object_bounds_use_curve_extrema() {
        let curve = Path::parse("M0 0 C0 4 4 4 4 0 Z");
        assert_eq!(curve.map(|p| p.bounds), Some((0., 0., 4., 3.)));
        let quadratic = Path::parse("M0 0 Q2 4 4 0 Z");
        assert_eq!(quadratic.map(|p| p.bounds), Some((0., 0., 4., 2.)));
    }
}
