//! Translate a selected SVG scene into outline clips, transforms and Painter fills.
#![allow(clippy::arithmetic_side_effects)]
use crate::{ClipPath, GlyphScene, GradientKind, Scene, Shape, Stop, shape_box};
use alloc::vec::Vec;
use sofel_geom::{Brush, Color, ColorLine, ColorStop, Extend, Painter, PointF, Solid, Transform};

/// A painter that also accepts a selected SVG outline and its intersecting clip paths.
/// Outlines are already positioned in glyph space. [`Painter::pop_clip`] ends this clip.
pub trait OutlinePainter<'scene>: Painter {
    /// Clip to the shape silhouette, intersected with each union of clip-path children.
    fn push_svg_clip(&mut self, shape: &'scene Shape, paths: &'scene [ClipPath]);
}

fn matrix([xx, yx, xy, yy, dx, dy]: [f32; 6]) -> Transform {
    Transform {
        xx,
        yx,
        xy,
        yy,
        dx,
        dy,
    }
}

fn solid(stop: &Stop) -> Solid {
    let [r, g, b, a] = stop.rgba;
    Solid {
        color: Color::new(r, g, b, a),
        ..Default::default()
    }
}

impl GlyphScene {
    /// Emit the selected glyph's clips, resolved brushes and balanced transforms in document order.
    pub fn paint<'s>(&'s self, painter: &mut impl OutlinePainter<'s>) -> bool {
        if self.clips.len() != self.scene.shapes.len() {
            return false;
        }
        self.scene.paint_shapes(Some(&self.clips), painter)
    }
}

impl Scene {
    /// Emit shape silhouettes and fills for a scene with no additional clip paths.
    pub fn paint<'s>(&'s self, painter: &mut impl OutlinePainter<'s>) -> bool {
        self.paint_shapes(None, painter)
    }

    fn paint_shapes<'s>(
        &'s self,
        clips: Option<&'s [Vec<ClipPath>]>,
        painter: &mut impl OutlinePainter<'s>,
    ) -> bool {
        let mut stops = Vec::new();
        for (index, shape) in self.shapes.iter().enumerate() {
            let Some(gradient) = self.gradients.get(shape.gradient) else {
                return false;
            };
            let paths = clips
                .and_then(|paths| paths.get(index))
                .map_or(&[][..], Vec::as_slice);
            painter.push_svg_clip(shape, paths);
            if let [stop] = gradient.stops.as_slice() {
                painter.fill(&Brush::Solid(solid(stop)));
            } else {
                let (x0, y0, x1, y1) = shape_box(&shape.kind);
                let bbox = if gradient.user_space {
                    Transform::IDENTITY
                } else {
                    Transform {
                        xx: x1 - x0,
                        yy: y1 - y0,
                        dx: x0,
                        dy: y0,
                        ..Transform::IDENTITY
                    }
                };
                let transforms = [
                    Transform::scale(1., -1.),
                    matrix(shape.transform),
                    bbox,
                    matrix(gradient.transform),
                ];
                for t in transforms {
                    painter.push_transform(t);
                }
                stops.clear();
                stops.extend(gradient.stops.iter().map(|s| ColorStop {
                    offset: s.offset,
                    color: solid(s),
                }));
                let line = ColorLine {
                    extend: Extend::Pad,
                    stops: &stops,
                };
                let brush = match gradient.kind {
                    GradientKind::Linear { x1, y1, x2, y2 } => Brush::Linear {
                        line,
                        p0: PointF::new(x1, y1),
                        p1: PointF::new(x2, y2),
                        p2: PointF::new(x1 - (y2 - y1), y1 + (x2 - x1)),
                    },
                    GradientKind::Radial { cx, cy, fx, fy, r } => Brush::Radial {
                        line,
                        c0: PointF::new(fx, fy),
                        r0: 0.,
                        c1: PointF::new(cx, cy),
                        r1: r,
                    },
                };
                painter.fill(&brush);
                for _ in transforms {
                    painter.pop_transform();
                }
            }
            painter.pop_clip();
        }
        !self.shapes.is_empty()
    }
}

/// Sample one SVG brush in its local coordinate system, interpolating straight RGBA stops.
pub fn sample_brush(brush: &Brush<'_>, point: PointF) -> [u8; 4] {
    let rgba = |c: Color| [c.r, c.g, c.b, c.a];
    let (kind, line) = match brush {
        Brush::Solid(s) => return rgba(s.color),
        Brush::Linear { p0, p1, line, .. } => (
            GradientKind::Linear {
                x1: p0.x,
                y1: p0.y,
                x2: p1.x,
                y2: p1.y,
            },
            line,
        ),
        Brush::Radial {
            c0, c1, r1, line, ..
        } => (
            GradientKind::Radial {
                cx: c1.x,
                cy: c1.y,
                fx: c0.x,
                fy: c0.y,
                r: *r1,
            },
            line,
        ),
        Brush::Sweep { .. } => return [0; 4],
    };
    crate::sample_stops(
        kind.fraction(point.x, point.y),
        line.stops.iter().map(|s| (s.offset, rgba(s.color.color))),
    )
}
