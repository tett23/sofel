//! PDF paint callbacks for positioned COLRv1 glyphs.
use super::{PdfPage, PdfPen, number};
use alloc::{format, string::String, vec::Vec};
use core::fmt::Write as _;
use sofel_geom::{
    Brush, Color, ColorLine, ColorStop, CompositeMode, Extend, Image, Painter, PointF, Rect, Solid,
    Transform, normalize_color_line, reduce_linear_anchors,
};

/// Receives paint commands and resolves glyph clips against caller-supplied paths.
pub struct PdfPainter<'a, F> {
    page: &'a mut PdfPage,
    outline: F,
    complete: bool,
    groups: Vec<CompositeMode>,
}

impl PdfPage {
    /// Renders one paint graph. Failed outlines and unsupported brushes roll back the glyph.
    pub fn paint<F>(
        &mut self,
        placement: Transform,
        outline: F,
        draw: impl FnOnce(&mut PdfPainter<'_, F>),
    ) -> bool
    where
        F: FnMut(u32, &mut PdfPen<'_>) -> bool,
    {
        let start = (
            self.stream.len(),
            self.states.len(),
            self.shadings.len(),
            self.soft_masks.len(),
            self.meshes.len(),
        );
        let mut p = PdfPainter {
            page: self,
            outline,
            complete: true,
            groups: Vec::new(),
        };
        p.push_transform(placement);
        draw(&mut p);
        p.pop_transform();
        if !p.complete {
            p.page.stream.truncate(start.0);
            p.page.states.truncate(start.1);
            p.page.shadings.truncate(start.2);
            p.page.soft_masks.truncate(start.3);
            p.page.meshes.truncate(start.4);
        }
        p.complete
    }
}

fn components(c: Color) -> String {
    let [r, g, b] = [c.r, c.g, c.b].map(|v| number(f32::from(v) / 255., 4));
    format!("{r} {g} {b}")
}

fn gradient_function(stops: &[ColorStop], alpha: bool) -> String {
    let samples = |c: Color| {
        if alpha {
            number(f32::from(c.a) / 255., 4)
        } else {
            components(c)
        }
    };
    let interpolate = |a: Color, b: Color| {
        format!(
            "<< /FunctionType 2 /Domain [0 1] /N 1 /C0 [{}] /C1 [{}] >>",
            samples(a),
            samples(b)
        )
    };
    if let [s] = stops {
        return interpolate(s.color.color, s.color.color);
    }
    if let [a, b] = stops {
        return interpolate(a.color.color, b.color.color);
    }
    let mut functions = String::new();
    let mut bounds = String::new();
    let mut encode = String::new();
    for (i, (a, b)) in stops.iter().zip(stops.iter().skip(1)).enumerate() {
        functions.push_str(&interpolate(a.color.color, b.color.color));
        if i != 0 {
            let _ = write!(bounds, "{} ", number(a.offset, 4));
        }
        encode.push_str("0 1 ");
    }
    format!(
        "<< /FunctionType 3 /Domain [0 1] /Functions [{functions}] /Bounds [{bounds}] /Encode [{encode}] >>"
    )
}

impl<F> PdfPainter<'_, F> {
    fn shade(&mut self, brush: &Brush<'_>, line: ColorLine<'_>) {
        if line.stops.is_empty() {
            return;
        }
        let mut stops = line.stops.to_vec();
        let (min, max) = normalize_color_line(&mut stops);
        let at = |a: f32, b: f32, t: f32| number(a + t * (b - a), 2);
        let (kind, coords) = match *brush {
            Brush::Linear { p0, p1, p2, .. } => {
                let (a, b) = reduce_linear_anchors(p0, p1, p2);
                (
                    2,
                    format!(
                        "{} {} {} {}",
                        at(a.x, b.x, min),
                        at(a.y, b.y, min),
                        at(a.x, b.x, max),
                        at(a.y, b.y, max)
                    ),
                )
            }
            Brush::Radial { c0, r0, c1, r1, .. } => (
                3,
                format!(
                    "{} {} {} {} {} {}",
                    at(c0.x, c1.x, min),
                    at(c0.y, c1.y, min),
                    at(r0, r1, min),
                    at(c0.x, c1.x, max),
                    at(c0.y, c1.y, max),
                    at(r0, r1, max)
                ),
            ),
            _ => return,
        };
        let extend = if line.extend == Extend::Pad {
            "/Extend [true true]"
        } else {
            ""
        };
        if stops.iter().any(|s| s.color.color.a != 255) {
            let center = match *brush {
                Brush::Linear { p0, p1, p2, .. } => {
                    let (a, b) = reduce_linear_anchors(p0, p1, p2);
                    PointF::new(a.x + min * (b.x - a.x), a.y + min * (b.y - a.y))
                }
                Brush::Radial { c0, r0, c1, r1, .. } => {
                    let u = if r0 + max * (r1 - r0) >= r0 + min * (r1 - r0) {
                        max
                    } else {
                        min
                    };
                    PointF::new(c0.x + u * (c1.x - c0.x), c0.y + u * (c1.y - c0.y))
                }
                _ => return,
            };
            let [x0, y0, x1, y1] = [
                center.x - 32768.,
                center.y - 32768.,
                center.x + 32768.,
                center.y + 32768.,
            ]
            .map(|v| number(v, 2));
            let alpha = gradient_function(&stops, true);
            let mask = self.page.soft_masks.len();
            self.page.soft_masks.push(format!("<< /Type /XObject /Subtype /Form /BBox [{x0} {y0} {x1} {y1}] /Group << /S /Transparency /CS /DeviceGray >> /Resources << /Shading << /SHa << /ShadingType {kind} /ColorSpace /DeviceGray /Coords [{coords}] /Function {alpha} {extend} >> >> >> /Length 8 >>\nstream\n/SHa sh\nendstream"));
            let _ = writeln!(self.page.stream, "/Mask{mask} gs");
        }
        let function = gradient_function(&stops, false);
        let id = self.page.shadings.len();
        self.page.shadings.push(format!("<< /ShadingType {kind} /ColorSpace /DeviceRGB /Coords [{coords}] /Function {function} {extend} >>"));
        let _ = writeln!(self.page.stream, "/SH{id} sh");
    }
}

impl<F: FnMut(u32, &mut PdfPen<'_>) -> bool> Painter for PdfPainter<'_, F> {
    fn push_transform(&mut self, t: Transform) {
        let [a, b, c, d] = [t.xx, t.yx, t.xy, t.yy].map(|n| number(n, 7));
        let _ = writeln!(
            self.page.stream,
            "q\n{a} {b} {c} {d} {} {} cm",
            number(t.dx, 2),
            number(t.dy, 2)
        );
    }
    fn pop_transform(&mut self) {
        self.page.stream.push_str("Q\n");
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        let [x, y, w, h] =
            [r.x_min, r.y_min, r.x_max - r.x_min, r.y_max - r.y_min].map(|v| number(v, 2));
        let _ = writeln!(self.page.stream, "q\n{x} {y} {w} {h} re W n");
    }
    fn push_clip_glyph(&mut self, gid: u32) {
        self.page.stream.push_str("q\n");
        self.complete &= (self.outline)(
            gid,
            &mut PdfPen {
                stream: &mut self.page.stream,
                transform: Transform::IDENTITY,
                current: PointF::default(),
            },
        );
        self.page.stream.push_str("W n\n");
    }
    fn pop_clip(&mut self) {
        self.page.stream.push_str("Q\n");
    }
    fn fill_glyph(&mut self, gid: u32, color: &Solid) {
        let mode = self
            .groups
            .last()
            .copied()
            .unwrap_or(CompositeMode::SrcOver);
        self.complete &= self
            .page
            .fill_path(Transform::IDENTITY, color.color, mode, |pen| {
                if (self.outline)(gid, pen) {
                    Ok(())
                } else {
                    Err(())
                }
            })
            .is_ok();
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        match *brush {
            Brush::Linear { line, .. } | Brush::Radial { line, .. } => self.shade(brush, line),
            Brush::Solid(s) => {
                self.page.begin_fill(
                    s.color,
                    self.groups
                        .last()
                        .copied()
                        .unwrap_or(CompositeMode::SrcOver),
                );
                self.page
                    .stream
                    .push_str("-32767 -32767 65534 65534 re f\nQ\n");
            }
            Brush::Sweep {
                line,
                center,
                start,
                end,
            } => self.complete &= self.page.sweep(line, center, start, end),
        }
    }
    fn image(&mut self, _: &Image<'_>) -> bool {
        self.complete = false;
        false
    }
    fn push_group(&mut self, mode: CompositeMode) {
        self.groups.push(mode);
        self.page.begin_fill(Color::new(0, 0, 0, 255), mode);
    }
    fn pop_group(&mut self, _: CompositeMode) {
        self.groups.pop();
        self.page.stream.push_str("Q\n");
    }
}
