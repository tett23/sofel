//! SVG paint callbacks for positioned color glyphs.
use crate::{SvgPage, SvgPen, pdf::number};
use alloc::{string::String, vec::Vec};
use core::fmt::Write as _;
use sofel_geom::{
    Brush, Color, ColorLine, CompositeMode, Extend, Image, Painter, Rect, Solid, Transform,
    normalize_color_line, reduce_linear_anchors,
};

// hb-view uses six subpixel bits when building vector pages. Paint parameters
// are written in that coordinate system while glyph paths keep font units.
const SUBPIXEL: f32 = 64.;
fn coord(v: f32) -> String {
    number(v / SUBPIXEL, 2)
}

/// Paints one color glyph using the caller's outline resolver.
pub struct SvgPainter<'a, F> {
    page: &'a mut SvgPage,
    outline: F,
    transforms: Vec<bool>,
    groups: Vec<bool>,
    glyph_clips: Vec<bool>,
    complete: bool,
}

impl SvgPage {
    /// Appends a glyph's paint commands atomically. Returns false if a brush
    /// or image cannot be represented, or an outline cannot be resolved.
    pub fn paint<F>(
        &mut self,
        placement: Transform,
        outline: F,
        draw: impl FnOnce(&mut SvgPainter<'_, F>),
    ) -> bool
    where
        F: FnMut(u32, &mut SvgPen<'_>) -> bool,
    {
        let mut layer = Self::new(0, 0, 0, 0, None);
        layer.clip_count = self.clip_count;
        layer.path_count = self.path_count;
        layer.gradient_count = self.gradient_count;
        let mut painter = SvgPainter {
            page: &mut layer,
            outline,
            transforms: Vec::new(),
            groups: Vec::new(),
            glyph_clips: Vec::new(),
            complete: true,
        };
        painter.push_transform(Transform {
            dx: placement.dx * SUBPIXEL,
            dy: placement.dy * SUBPIXEL,
            ..placement
        });
        painter.push_transform(Transform::scale(SUBPIXEL, SUBPIXEL));
        draw(&mut painter);
        painter.pop_transform();
        painter.pop_transform();
        if !painter.complete {
            return false;
        }
        self.body.push_str(&layer.body);
        self.clips.push_str(&layer.clips);
        self.clip_count = layer.clip_count;
        self.path_count = layer.path_count;
        self.gradient_count = layer.gradient_count;
        self.has_image = true;
        true
    }
}

fn spread(mode: Extend) -> &'static str {
    match mode {
        Extend::Pad => "pad",
        Extend::Reflect => "reflect",
        Extend::Repeat => "repeat",
    }
}

impl<F> SvgPainter<'_, F> {
    fn gradient(&mut self, brush: &Brush<'_>, line: ColorLine<'_>) {
        if line.stops.is_empty() {
            return;
        }
        let mut stops = line.stops.to_vec();
        let (min, max) = normalize_color_line(&mut stops);
        let id = self.page.gradient_count;
        self.page.gradient_count = id.saturating_add(1);
        let defs = &mut self.page.clips;
        let at = |a: f32, b: f32, u: f32| coord(a + u * (b - a));
        let tag = match *brush {
            Brush::Linear { p0, p1, p2, .. } => {
                let (a, b) = reduce_linear_anchors(p0, p1, p2);
                let (x1, y1, x2, y2) = (
                    at(a.x, b.x, min),
                    at(a.y, b.y, min),
                    at(a.x, b.x, max),
                    at(a.y, b.y, max),
                );
                let _ = write!(
                    defs,
                    "<linearGradient id=\"gr{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\""
                );
                "linearGradient"
            }
            Brush::Radial { c0, r0, c1, r1, .. } => {
                let (cx, cy, r, fx, fy) = (
                    at(c0.x, c1.x, max),
                    at(c0.y, c1.y, max),
                    at(r0, r1, max),
                    at(c0.x, c1.x, min),
                    at(c0.y, c1.y, min),
                );
                let _ = write!(
                    defs,
                    "<radialGradient id=\"gr{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" fx=\"{fx}\" fy=\"{fy}\""
                );
                let fr = r0 + min * (r1 - r0);
                if fr > 0. {
                    let _ = write!(defs, " fr=\"{}\"", coord(fr));
                }
                "radialGradient"
            }
            _ => return,
        };
        let _ = writeln!(defs, " spreadMethod=\"{}\">", spread(line.extend));
        for stop in stops {
            let Color { r, g, b, a } = stop.color.color;
            let _ = write!(
                defs,
                "<stop offset=\"{}\" stop-color=\"rgb({r},{g},{b})\"",
                number(stop.offset, 4)
            );
            if a != 255 {
                let _ = write!(defs, " stop-opacity=\"{}\"", number(f32::from(a) / 255., 4));
            }
            defs.push_str("/>\n");
        }
        let _ = writeln!(defs, "</{tag}>");
        let _ = writeln!(
            self.page.body,
            "<rect x=\"-32767\" y=\"-32767\" width=\"65534\" height=\"65534\" fill=\"url(#gr{id})\"/>"
        );
    }
}

impl<F: FnMut(u32, &mut SvgPen<'_>) -> bool> Painter for SvgPainter<'_, F> {
    fn push_transform(&mut self, t: Transform) {
        let open = t != Transform::IDENTITY;
        self.transforms.push(open);
        if open {
            let [a, b, c, d] = [t.xx, t.yx, t.xy, t.yy].map(|v| number(v, 7));
            let _ = writeln!(
                self.page.body,
                "<g transform=\"matrix({a},{b},{c},{d},{},{})\">",
                coord(t.dx),
                coord(t.dy)
            );
        }
    }
    fn pop_transform(&mut self) {
        if self.transforms.pop() == Some(true) {
            self.page.body.push_str("</g>\n");
        }
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        self.glyph_clips.push(false);
        let id = self.page.clip_count;
        self.page.clip_count = id.saturating_add(1);
        let [x, y, w, h] = [r.x_min, r.y_min, r.x_max - r.x_min, r.y_max - r.y_min].map(coord);
        let _ = writeln!(
            self.page.clips,
            "<clipPath id=\"c{id}\"><rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"/></clipPath>"
        );
        let _ = writeln!(self.page.body, "<g clip-path=\"url(#c{id})\">");
    }
    fn push_clip_glyph(&mut self, gid: u32) {
        self.glyph_clips.push(true);
        self.push_transform(Transform::scale(1. / SUBPIXEL, 1. / SUBPIXEL));
        let id = self.page.path_count;
        self.page.path_count = id.saturating_add(1);
        let mut path = String::new();
        self.complete &= (self.outline)(
            gid,
            &mut SvgPen {
                body: &mut path,
                transform: Transform::IDENTITY,
            },
        );
        let _ = writeln!(
            self.page.clips,
            "<path id=\"p{id}\" d=\"{path}\"/>\n<clipPath id=\"clip-p{id}\"><use href=\"#p{id}\"/></clipPath>"
        );
        let _ = writeln!(self.page.body, "<g clip-path=\"url(#clip-p{id})\">");
        self.push_transform(Transform::scale(SUBPIXEL, SUBPIXEL));
    }
    fn pop_clip(&mut self) {
        let glyph = self.glyph_clips.pop() == Some(true);
        if glyph {
            self.pop_transform();
        }
        self.page.body.push_str("</g>\n");
        if glyph {
            self.pop_transform();
        }
    }
    fn fill_glyph(&mut self, gid: u32, color: &Solid) {
        self.push_transform(Transform::scale(1. / SUBPIXEL, 1. / SUBPIXEL));
        self.complete &= self
            .page
            .fill_path(Transform::IDENTITY, color.color, |pen| {
                if (self.outline)(gid, pen) {
                    Ok(())
                } else {
                    Err(())
                }
            })
            .is_ok();
        self.pop_transform();
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        match *brush {
            Brush::Solid(s) => {
                let Color { r, g, b, a } = s.color;
                let _ = write!(
                    self.page.body,
                    "<rect x=\"-32767\" y=\"-32767\" width=\"65534\" height=\"65534\" fill=\"rgb({r},{g},{b})\""
                );
                super::opacity(&mut self.page.body, a);
                self.page.body.push_str("/>\n");
            }
            Brush::Linear { line, .. } | Brush::Radial { line, .. } => self.gradient(brush, line),
            Brush::Sweep {
                center,
                line,
                start,
                end,
            } => {
                let center = sofel_geom::PointF::new(center.x / SUBPIXEL, center.y / SUBPIXEL);
                self.complete &= crate::sweep::write(&mut self.page.body, center, line, start, end);
            }
        }
    }
    fn image(&mut self, image: &Image<'_>) -> bool {
        let ok = self
            .page
            .draw_image(image, Transform::scale(1. / SUBPIXEL, 1. / SUBPIXEL), None);
        self.complete &= ok;
        ok
    }
    fn push_group(&mut self, mode: CompositeMode) {
        use CompositeMode::*;
        let name = match mode {
            SrcOver => Some("normal"),
            Plus | Screen => Some("screen"),
            Xor | Difference => Some("difference"),
            Overlay => Some("overlay"),
            Darken => Some("darken"),
            Lighten => Some("lighten"),
            ColorDodge => Some("color-dodge"),
            ColorBurn => Some("color-burn"),
            HardLight => Some("hard-light"),
            SoftLight => Some("soft-light"),
            Exclusion => Some("exclusion"),
            Multiply => Some("multiply"),
            HslHue => Some("hue"),
            HslSaturation => Some("saturation"),
            HslColor => Some("color"),
            HslLuminosity => Some("luminosity"),
            _ => None,
        };
        self.groups.push(name.is_some());
        if let Some(name) = name {
            let _ = writeln!(self.page.body, "<g style=\"mix-blend-mode:{name}\">");
        }
    }
    fn pop_group(&mut self, _: CompositeMode) {
        if self.groups.pop() == Some(true) {
            self.page.body.push_str("</g>\n");
        }
    }
}
