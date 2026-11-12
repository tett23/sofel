//! HarfBuzz 15.1.0 `hb_outline_t`: recording and synthetic glyph drawing.

use alloc::vec::Vec;

use sofel_font::GlyphExtents;
use sofel_geom::float::{floor, hb_clamp_to_i32};
use sofel_geom::{Command, PointF};

/// A command point recorded by HarfBuzz's outline pen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutlinePoint {
    /// Horizontal coordinate in the font's scale.
    pub x: f32,
    /// Vertical coordinate in the font's scale.
    pub y: f32,
    kind: PointKind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PointKind {
    Move,
    Line,
    Quad,
    Cubic,
}

/// HarfBuzz's recorded outline: command points and contour ends.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlyphOutline {
    points: Vec<OutlinePoint>,
    contours: Vec<usize>,
}

impl GlyphOutline {
    /// An empty recorded outline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Points in recording order. Quadratic commands have two points,
    /// cubic commands three, and each contour starts with a move point.
    pub fn points(&self) -> &[OutlinePoint] {
        &self.points
    }

    /// Exclusive end positions of the recorded contours.
    pub fn contours(&self) -> &[usize] {
        &self.contours
    }

    /// Record a move command.
    pub fn move_to(&mut self, p: PointF) {
        self.push(p, PointKind::Move);
    }
    /// Record a line command.
    pub fn line_to(&mut self, p: PointF) {
        self.push(p, PointKind::Line);
    }
    /// Record a quadratic command.
    pub fn quad_to(&mut self, control: PointF, to: PointF) {
        self.push(control, PointKind::Quad);
        self.push(to, PointKind::Quad);
    }
    /// Record a cubic command.
    pub fn cubic_to(&mut self, first: PointF, second: PointF, to: PointF) {
        self.push(first, PointKind::Cubic);
        self.push(second, PointKind::Cubic);
        self.push(to, PointKind::Cubic);
    }
    /// Close the current contour.
    pub fn close(&mut self) {
        self.contours.push(self.points.len());
    }

    fn push(&mut self, p: PointF, kind: PointKind) {
        self.points.push(OutlinePoint {
            x: p.x,
            y: p.y,
            kind,
        });
    }

    /// Record a `sofel-geom` command in the supplied x/y scale.
    #[allow(clippy::arithmetic_side_effects)] // Drawing coordinates use HarfBuzz f32 multiplication.
    pub fn record_command(&mut self, command: Command, x_factor: f32, y_factor: f32) {
        let scaled =
            |p: sofel_geom::Point| PointF::new((p.x as f32) * x_factor, (p.y as f32) * y_factor);
        match command {
            Command::MoveTo(p) => self.move_to(scaled(p)),
            Command::LineTo(p) => self.line_to(scaled(p)),
            Command::QuadTo(c, p) => self.quad_to(scaled(c), scaled(p)),
            Command::CubicTo(c1, c2, p) => self.cubic_to(scaled(c1), scaled(c2), scaled(p)),
            Command::Close => self.close(),
        }
    }

    /// Scale coordinates as HarfBuzz's subfont draw adaptor does.
    #[allow(clippy::arithmetic_side_effects)] // HarfBuzz multiplies each f32 coordinate.
    pub(crate) fn scale(&mut self, x: f32, y: f32) {
        for p in &mut self.points {
            p.x *= x;
            p.y *= y;
        }
    }

    /// Translate each recorded command point.
    #[allow(clippy::arithmetic_side_effects)] // HarfBuzz uses f32 addition.
    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        for p in &mut self.points {
            p.x += dx;
            p.y += dy;
        }
    }

    /// Slant before emboldening, matching `hb_outline_t::slant`.
    #[allow(clippy::arithmetic_side_effects)] // HarfBuzz uses f32 multiply-add in this order.
    pub(crate) fn slant(&mut self, slant_xy: f32) {
        for p in &mut self.points {
            p.x += slant_xy * p.y;
        }
    }

    /// Bounds of all command and control points as HarfBuzz reports them.
    #[allow(clippy::arithmetic_side_effects)] // Bounds differences fit f64 before clamping.
    pub fn extents(&self) -> GlyphExtents {
        let Some(first) = self.points.first() else {
            return GlyphExtents::default();
        };
        let (mut x_min, mut y_min, mut x_max, mut y_max) = (first.x, first.y, first.x, first.y);
        for p in &self.points {
            x_min = x_min.min(p.x);
            y_min = y_min.min(p.y);
            x_max = x_max.max(p.x);
            y_max = y_max.max(p.y);
        }
        let x0 = c_round(f64::from(x_min));
        let y0 = c_round(f64::from(y_min));
        let x1 = c_round(f64::from(x_max));
        let y1 = c_round(f64::from(y_max));
        if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
            return GlyphExtents::default();
        }
        GlyphExtents {
            x_bearing: hb_clamp_to_i32(x0),
            y_bearing: hb_clamp_to_i32(y1),
            width: hb_clamp_to_i32(x1 - x0),
            height: hb_clamp_to_i32(y0 - y1),
        }
    }

    #[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // Contour ends are written by `close`.
    fn control_area(&self) -> f32 {
        let mut area = 0.0;
        let mut first = 0;
        for &end in &self.contours {
            for i in first..end {
                let j = if i + 1 < end { i + 1 } else { first };
                let a = self.points[i];
                let b = self.points[j];
                area += a.x * b.y - a.y * b.x;
            }
            first = end;
        }
        area * 0.5
    }

    /// `hb_outline_t::embolden`, including its contour orientation.
    #[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // HarfBuzz f32 arithmetic and recorded contour bounds.
    pub(crate) fn embolden(
        &mut self,
        mut x_strength: f32,
        mut y_strength: f32,
        x_shift: f32,
        y_shift: f32,
    ) {
        if (x_strength == 0.0 && y_strength == 0.0) || self.points.is_empty() {
            return;
        }
        x_strength /= 2.0;
        y_strength /= 2.0;
        let orientation_negative = self.control_area() < 0.0;
        let mut first = 0;
        for &end in &self.contours {
            if end <= first {
                continue;
            }
            let last = end - 1;
            let (mut i, mut j) = (last, first);
            let mut anchor_index = None;
            let (mut in_x, mut in_y, mut l_in) = (0.0, 0.0, 0.0);
            let (mut anchor_x, mut anchor_y, mut l_anchor) = (0.0, 0.0, 0.0);
            while j != i && Some(i) != anchor_index {
                let (out_x, out_y, l_out) = if Some(j) == anchor_index {
                    (anchor_x, anchor_y, l_anchor)
                } else {
                    let mut x = self.points[j].x - self.points[i].x;
                    let mut y = self.points[j].y - self.points[i].y;
                    let len = hypotf(x, y);
                    if len != 0.0 {
                        x /= len;
                        y /= len;
                    }
                    (x, y, len)
                };
                if l_out == 0.0 {
                    j = if j < last { j + 1 } else { first };
                    continue;
                }
                if l_in != 0.0 {
                    if anchor_index.is_none() {
                        anchor_index = Some(i);
                        anchor_x = in_x;
                        anchor_y = in_y;
                        l_anchor = l_in;
                    }
                    let mut d = in_x * out_x + in_y * out_y;
                    let (shift_x, shift_y) = if d > -15.0 / 16.0 {
                        d += 1.0;
                        let mut sx = in_y + out_y;
                        let mut sy = in_x + out_x;
                        if orientation_negative {
                            sx = -sx;
                        } else {
                            sy = -sy;
                        }
                        let mut q = out_x * in_y - out_y * in_x;
                        if orientation_negative {
                            q = -q;
                        }
                        let l = l_in.min(l_out);
                        sx = if x_strength * q <= l * d {
                            sx * x_strength / d
                        } else {
                            sx * l / q
                        };
                        sy = if y_strength * q <= l * d {
                            sy * y_strength / d
                        } else {
                            sy * l / q
                        };
                        (sx, sy)
                    } else {
                        (0.0, 0.0)
                    };
                    while i != j {
                        self.points[i].x += x_shift + shift_x;
                        self.points[i].y += y_shift + shift_y;
                        i = if i < last { i + 1 } else { first };
                    }
                } else {
                    i = j;
                }
                in_x = out_x;
                in_y = out_y;
                l_in = l_out;
                j = if j < last { j + 1 } else { first };
            }
            first = end;
        }
    }
}

/// C `round` (ties away from zero), used by `hb_extents_t`.
fn c_round(v: f64) -> f64 {
    if v < 0.0 {
        -floor(-v + 0.5)
    } else {
        floor(v + 0.5)
    }
}

/// Finite `hypotf` for HarfBuzz's outline join normalization. Widening
/// before squaring avoids overflow and keeps the f32 inputs exact.
#[allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)] // Newton iteration on f64, rounded once to f32.
fn hypotf(x: f32, y: f32) -> f32 {
    let a = f64::from(x);
    let b = f64::from(y);
    let squared = a * a + b * b;
    if squared == 0.0 {
        return 0.0;
    }
    if !squared.is_finite() {
        return f32::INFINITY;
    }
    // A bit-level exponent estimate is within a small factor of sqrt.
    let mut root = f64::from_bits((squared.to_bits() >> 1).wrapping_add(0x1FF8_0000_0000_0000));
    for _ in 0..8 {
        root = 0.5 * (root + squared / root);
    }
    root as f32
}

#[cfg(test)]
mod tests {
    use super::hypotf;
    use sofel_geom::float::i32_to_f32;

    #[test]
    #[allow(clippy::arithmetic_side_effects)] // Bounded deterministic inputs.
    fn adr0063_i2_hypotf_matches_rounded_length_for_outline_deltas() {
        // These bounded, binary-fraction deltas have exact squares and sums in f64.
        // Platform hypotf implementations can differ by one f32 ULP.
        #[allow(clippy::cast_possible_truncation)]
        let reference = |x: f32, y: f32| {
            let (x, y) = (f64::from(x), f64::from(y));
            (x * x + y * y).sqrt() as f32
        };
        for (x, y) in [
            (-1966.5, 911.375),
            (219.0, -152.0),
            (408.0, 0.0),
            (219.0, -152.5),
            (109.5, -76.0),
        ] {
            assert_eq!(hypotf(x, y).to_bits(), reference(x, y).to_bits(), "{x} {y}");
        }
        let mut state = 0x9E37_79B9u32;
        for _ in 0..10_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = i32_to_f32((state & 0x7FFF).cast_signed()) / 8.0 - 2048.0;
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let y = i32_to_f32((state & 0x7FFF).cast_signed()) / 8.0 - 2048.0;
            assert_eq!(hypotf(x, y).to_bits(), reference(x, y).to_bits(), "{x} {y}");
        }
    }
}
