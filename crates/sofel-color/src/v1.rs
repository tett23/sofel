//! `COLR` version 1: HarfBuzz's `COLR::paint_glyph` and the `paint_glyph`
//! of each paint format (`src/OT/Color/COLR/COLR.hh`), the transforms of
//! `src/hb-paint.hh`, the cycle detection of `src/hb-decycler.hh` and the
//! boundedness check of `src/hb-paint-bounded.hh`.
//!
//! Variable paint fields use HarfBuzz variation-store arithmetic.

use alloc::vec::Vec;

use sofel_outline::font::geom::{
    Brush, ColorLine, ColorStop, CompositeMode, Extend, Image, Painter, PointF, Rect, Transform,
};
use sofel_outline::font::read::colr::{self, Colr, Paint};
use sofel_outline::font::read::{F2Dot14, Fixed};
use sofel_outline::font::var::{HbF32, Instance, StoreKind};

use crate::Scratch;
use crate::paint::{Direct, Sink};
use crate::palette::Colors;

/// `HB_MAX_NESTING_LEVEL`.
pub const MAX_NESTING_LEVEL: i32 = 64;
/// `HB_MAX_GRAPH_EDGE_COUNT`.
pub const MAX_GRAPH_EDGE_COUNT: i32 = 16384;
/// `HB_BUDGET_GLYPH`: the work allowed per glyph.
pub const BUDGET_GLYPH: i64 = 1 << 24;
/// `HB_BUDGET_16`: the cost of one color stop.
const BUDGET_STOP: i64 = 16;

/// HarfBuzz's `HB_PI`.
const PI: f32 = core::f32::consts::PI;

/// `(float) v`.
fn i32_f32(v: i32) -> f32 {
    // Rounds to nearest, as C's conversion does.
    #[allow(clippy::cast_precision_loss)]
    let f = v as f32;
    f
}

/// `F2DOT14::to_float`.
fn f2(v: F2Dot14, delta: f32) -> f32 {
    (f32::from(v.0) + delta) / 16384.0
}

/// `F16DOT16::to_float`.
fn fixed(v: Fixed, delta: f32) -> f32 {
    (i32_f32(v.0) + delta) / 65536.0
}

/// The work memory of the walk: the decyclers and the color stops.
#[derive(Clone, Default, Debug)]
pub(crate) struct V1Stacks {
    glyphs: Vec<u32>,
    layers: Vec<u32>,
    bounded_glyphs: Vec<u32>,
    bounded_layers: Vec<u32>,
    stops: Vec<ColorStop>,
}

/// `hb_decycler_t` over a stack of the values being visited: a new
/// node's value is a cycle when it equals the value of the node at half
/// its depth (the tortoise).
fn visit(stack: &mut [u32], value: u32) -> bool {
    let k = stack.len().saturating_sub(1);
    if let Some(last) = stack.last_mut() {
        *last = value;
    }
    if k == 0 {
        return true;
    }
    stack.get(k / 2).is_none_or(|&t| t != value)
}

struct Ctx<'c, 'a> {
    colr: &'c Colr<'a>,
    delta: &'c dyn Fn(Option<u32>, u16) -> f32,
    colors: &'c Colors<'a>,
    depth_left: i32,
    edge_count: i32,
    budget: i64,
    /// Whether color stops cost work (HarfBuzz's receiver fetches them);
    /// the box and boundedness passes do not fetch them.
    spend: bool,
    glyphs: &'c mut Vec<u32>,
    layers: &'c mut Vec<u32>,
    stops: &'c mut Vec<ColorStop>,
}

/// `hb_paint_funcs_t::push_transform`: -0 translations become 0.
fn push(sink: &mut dyn Painter, mut t: Transform) {
    if t.dx == 0.0 {
        t.dx = 0.0;
    }
    if t.dy == 0.0 {
        t.dy = 0.0;
    }
    sink.push_transform(t);
}

pub(crate) fn deltas<'f, 'd, 'l, 's>(
    instance: Instance<'f, 'd, 'l, 's>,
    colr: &Colr<'d>,
) -> impl Fn(Option<u32>, u16) -> f32 + use<'f, 'd, 'l, 's> {
    let map = colr.var_index_map().ok().flatten();
    let active = instance.location().hb_nonzero();
    move |base: Option<u32>, offset: u16| {
        let Some(index) = base.and_then(|v| v.checked_add(u32::from(offset))) else {
            return 0.0;
        };
        if !active || index == u32::MAX {
            return 0.0;
        }
        let index = map.map_or(index, |map| map.map(index));
        let [a, b, c, d] = index.to_be_bytes();
        instance
            .item_delta::<HbF32>(
                StoreKind::Colr,
                u16::from_be_bytes([a, b]),
                u16::from_be_bytes([c, d]),
            )
            .unwrap_or(0.0)
    }
}

pub(crate) fn clip_edges(b: &colr::ClipBox, delta: &dyn Fn(Option<u32>, u16) -> f32) -> [i32; 4] {
    use sofel_outline::font::geom::float::{ceilf, floorf};
    #[allow(clippy::cast_possible_truncation)]
    let edge = |v: i16, adjustment: f32| i32::from(v).saturating_add(adjustment as i32);
    [
        edge(b.x_min, floorf(delta(b.var_index_base, 0))),
        edge(b.y_min, floorf(delta(b.var_index_base, 1))),
        edge(b.x_max, ceilf(delta(b.var_index_base, 2))),
        edge(b.y_max, ceilf(delta(b.var_index_base, 3))),
    ]
}

fn clip_rect(b: &colr::ClipBox, delta: &dyn Fn(Option<u32>, u16) -> f32) -> Rect {
    let [x_min, y_min, x_max, y_max] = clip_edges(b, delta).map(i32_f32);
    Rect {
        x_min,
        y_min,
        x_max,
        y_max,
    }
}

impl<'c, 'a> Ctx<'c, 'a> {
    fn recurse(&mut self, offset: Option<usize>, sink: &mut dyn Painter) {
        if self.depth_left <= 0 || self.edge_count <= 0 || self.budget < 0 {
            return;
        }
        self.depth_left = self.depth_left.saturating_sub(1);
        self.edge_count = self.edge_count.saturating_sub(1);
        if let Some(o) = offset {
            self.dispatch(o, sink);
        }
        self.depth_left = self.depth_left.saturating_add(1);
    }

    fn line(&mut self, line: Option<colr::ColorLine<'a>>) -> Extend {
        self.stops.clear();
        let Some(line) = line else {
            return Extend::Pad;
        };
        let n = line.len();
        self.stops
            .extend((0..n).filter_map(|i| line.stop(i)).map(|s| ColorStop {
                offset: f2(s.offset, (self.delta)(s.var_index_base, 0)),
                color: self.colors.solid(
                    s.palette_index,
                    f2(s.alpha, (self.delta)(s.var_index_base, 1)),
                ),
            }));
        if self.spend {
            let cost = BUDGET_STOP.saturating_mul(i64::try_from(n).unwrap_or(i64::MAX));
            self.budget = self.budget.saturating_sub(cost);
        }
        match line.extend {
            1 => Extend::Repeat,
            2 => Extend::Reflect,
            _ => Extend::Pad,
        }
    }

    fn dispatch(&mut self, offset: usize, sink: &mut dyn Painter) {
        let Some(paint) = self.colr.paint(offset) else {
            return;
        };
        let delta = self.delta;
        let point = |base, index, x: i16, y: i16| {
            PointF::new(
                f32::from(x) + delta(base, index),
                f32::from(y) + delta(base, index.saturating_add(1)),
            )
        };
        match paint {
            Paint::ColrLayers {
                num_layers,
                first_layer,
            } => {
                self.layers.push(0);
                let end = first_layer.saturating_add(u32::from(num_layers));
                for i in first_layer..end {
                    if !visit(self.layers, i) {
                        break;
                    }
                    let p = usize::try_from(i)
                        .ok()
                        .and_then(|i| self.colr.layer_paint(i));
                    self.recurse(p, sink);
                }
                self.layers.pop();
            }
            Paint::Solid {
                palette_index,
                alpha,
                var_index_base,
            } => {
                let s = self
                    .colors
                    .solid(palette_index, f2(alpha, delta(var_index_base, 0)));
                sink.fill(&Brush::Solid(s));
            }
            Paint::LinearGradient {
                color_line,
                points: [x0, y0, x1, y1, x2, y2],
                var_index_base,
            } => {
                let extend = self.line(color_line);
                sink.fill(&Brush::Linear {
                    line: ColorLine {
                        stops: self.stops,
                        extend,
                    },
                    p0: point(var_index_base, 0, x0, y0),
                    p1: point(var_index_base, 2, x1, y1),
                    p2: point(var_index_base, 4, x2, y2),
                });
            }
            Paint::RadialGradient {
                color_line,
                c0,
                r0,
                c1,
                r1,
                var_index_base,
            } => {
                let extend = self.line(color_line);
                sink.fill(&Brush::Radial {
                    line: ColorLine {
                        stops: self.stops,
                        extend,
                    },
                    c0: point(var_index_base, 0, c0.0, c0.1),
                    r0: f32::from(r0) + delta(var_index_base, 2),
                    c1: point(var_index_base, 3, c1.0, c1.1),
                    r1: f32::from(r1) + delta(var_index_base, 5),
                });
            }
            Paint::SweepGradient {
                color_line,
                center,
                start_angle,
                end_angle,
                var_index_base,
            } => {
                let extend = self.line(color_line);
                sink.fill(&Brush::Sweep {
                    line: ColorLine {
                        stops: self.stops,
                        extend,
                    },
                    center: point(var_index_base, 0, center.0, center.1),
                    start: (f2(start_angle, delta(var_index_base, 2)) + 1.0) * PI,
                    end: (f2(end_angle, delta(var_index_base, 3)) + 1.0) * PI,
                });
            }
            Paint::Glyph { paint, glyph } => {
                let solid = paint.and_then(|o| match self.colr.paint(o) {
                    Some(Paint::Solid {
                        palette_index,
                        alpha,
                        var_index_base,
                    }) => Some(
                        self.colors
                            .solid(palette_index, f2(alpha, delta(var_index_base, 0))),
                    ),
                    _ => None,
                });
                if self.depth_left > 0
                    && self.edge_count > 0
                    && let Some(s) = solid
                {
                    self.edge_count = self.edge_count.saturating_sub(1);
                    sink.fill_glyph(glyph, &s);
                    return;
                }
                sink.push_clip_glyph(glyph);
                self.recurse(paint, sink);
                sink.pop_clip();
            }
            Paint::ColrGlyph { glyph } => {
                let glyph = u32::from(glyph);
                self.glyphs.push(0);
                if visit(self.glyphs, glyph) && !sink.color_glyph(glyph) {
                    let paint = self.colr.base_glyph_paint(glyph);
                    let clip = self.colr.clip_box(glyph);
                    if let Some(b) = &clip {
                        sink.push_clip_rectangle(clip_rect(b, self.delta));
                    }
                    if paint.is_some() {
                        self.recurse(paint, sink);
                    }
                    if clip.is_some() {
                        sink.pop_clip();
                    }
                }
                self.glyphs.pop();
            }
            Paint::Transform { paint, transform } => {
                let t = transform.map_or(Transform::scale(0.0, 0.0), |a| Transform {
                    xx: fixed(a.xx, delta(a.var_index_base, 0)),
                    yx: fixed(a.yx, delta(a.var_index_base, 1)),
                    xy: fixed(a.xy, delta(a.var_index_base, 2)),
                    yy: fixed(a.yy, delta(a.var_index_base, 3)),
                    dx: fixed(a.dx, delta(a.var_index_base, 4)),
                    dy: fixed(a.dy, delta(a.var_index_base, 5)),
                });
                push(sink, t);
                self.recurse(paint, sink);
                sink.pop_transform();
            }
            Paint::Translate {
                paint,
                delta: shift,
                var_index_base,
            } => {
                push(
                    sink,
                    Transform::translate(
                        f32::from(shift.0) + delta(var_index_base, 0),
                        f32::from(shift.1) + delta(var_index_base, 1),
                    ),
                );
                self.recurse(paint, sink);
                sink.pop_transform();
            }
            Paint::Scale {
                paint,
                scale,
                center,
                var_index_base,
            } => {
                let uniform = matches!(self.colr.paint_format(offset), Some(20..=23));
                let y_index = u16::from(!uniform);
                let (sx, sy) = (
                    f2(scale.0, delta(var_index_base, 0)),
                    f2(scale.1, delta(var_index_base, y_index)),
                );
                let t = match center {
                    None => Transform::scale(sx, sy),
                    Some((cx, cy)) => Transform::scale_around(
                        sx,
                        sy,
                        f32::from(cx) + delta(var_index_base, y_index.saturating_add(1)),
                        f32::from(cy) + delta(var_index_base, y_index.saturating_add(2)),
                    ),
                };
                push(sink, t);
                self.recurse(paint, sink);
                sink.pop_transform();
            }
            Paint::Rotate {
                paint,
                angle,
                center,
                var_index_base,
            } => {
                let a = f2(angle, delta(var_index_base, 0)) * PI;
                let t = match center {
                    None => Transform::rotate(a),
                    Some((cx, cy)) => Transform::rotate_around(
                        a,
                        f32::from(cx) + delta(var_index_base, 1),
                        f32::from(cy) + delta(var_index_base, 2),
                    ),
                };
                push(sink, t);
                self.recurse(paint, sink);
                sink.pop_transform();
            }
            Paint::Skew {
                paint,
                angles,
                center,
                var_index_base,
            } => {
                let (sx, sy) = (
                    -f2(angles.0, delta(var_index_base, 0)) * PI,
                    f2(angles.1, delta(var_index_base, 1)) * PI,
                );
                let t = match center {
                    None => Transform::skew(sx, sy),
                    Some((cx, cy)) => Transform::skew_around(
                        sx,
                        sy,
                        f32::from(cx) + delta(var_index_base, 2),
                        f32::from(cy) + delta(var_index_base, 3),
                    ),
                };
                push(sink, t);
                self.recurse(paint, sink);
                sink.pop_transform();
            }
            Paint::Composite {
                source,
                mode,
                backdrop,
            } => {
                let mode = CompositeMode::from_u8(mode).unwrap_or(CompositeMode::Clear);
                sink.push_group(CompositeMode::SrcOver);
                self.recurse(backdrop, sink);
                sink.push_group(mode);
                self.recurse(source, sink);
                sink.pop_group(mode);
                sink.pop_group(CompositeMode::SrcOver);
            }
        }
    }
}

/// HarfBuzz's `hb_paint_bounded_context_t`: whether everything painted
/// is inside a clip.
#[derive(Default)]
struct Bounded {
    bounded: bool,
    clips: u32,
    groups: Vec<bool>,
}

impl Bounded {
    fn paint(&mut self) {
        if self.clips == 0 {
            self.bounded = false;
        }
    }
}

impl Painter for Bounded {
    fn push_transform(&mut self, _t: Transform) {}
    fn pop_transform(&mut self) {}
    fn push_clip_glyph(&mut self, _glyph: u32) {
        self.clips = self.clips.saturating_add(1);
    }
    fn push_clip_rectangle(&mut self, _r: Rect) {
        self.clips = self.clips.saturating_add(1);
    }
    fn pop_clip(&mut self) {
        self.clips = self.clips.saturating_sub(1);
    }
    fn fill(&mut self, _brush: &Brush<'_>) {
        self.paint();
    }
    fn image(&mut self, _image: &Image<'_>) -> bool {
        self.paint();
        true
    }
    fn push_group(&mut self, _mode: CompositeMode) {
        self.groups.push(self.bounded);
        self.bounded = true;
    }
    fn pop_group(&mut self, mode: CompositeMode) {
        let src = self.bounded;
        self.bounded = self.groups.pop().unwrap_or(true);
        use CompositeMode::*;
        match mode {
            Clear => self.bounded = true,
            Src | SrcOut => self.bounded = src,
            Dest | DestOut => {}
            SrcIn | DestIn => self.bounded = self.bounded && src,
            _ => self.bounded = self.bounded || src,
        }
    }
}

/// Walks `paint` of a root glyph into `sink` with a fresh context.
#[allow(clippy::too_many_arguments)]
fn walk<'a>(
    delta: &dyn Fn(Option<u32>, u16) -> f32,
    colr: &Colr<'a>,
    colors: &Colors<'a>,
    glyph: u32,
    paint: usize,
    spend: bool,
    glyphs: &mut Vec<u32>,
    layers: &mut Vec<u32>,
    stops: &mut Vec<ColorStop>,
    sink: &mut dyn Painter,
) {
    glyphs.clear();
    layers.clear();
    glyphs.push(0);
    visit(glyphs, glyph);
    let mut ctx = Ctx {
        delta,
        colr,
        colors,
        depth_left: MAX_NESTING_LEVEL,
        edge_count: MAX_GRAPH_EDGE_COUNT,
        budget: BUDGET_GLYPH,
        spend,
        glyphs,
        layers,
        stops,
    };
    ctx.recurse(Some(paint), sink);
}

/// Paints the version 1 paint of `glyph` (HarfBuzz's `COLR::paint_glyph`
/// with `clip` true, without the root scale): within the clip box, or,
/// without one, only when everything painted is inside a clip. `false`
/// when the glyph has no version 1 paint. With `spend`, color stops cost
/// work as when HarfBuzz's receiver fetches them.
pub(crate) fn paint<'a>(
    instance: Instance<'_, 'a, '_, '_>,
    colr: &Colr<'a>,
    glyph: u32,
    colors: &Colors<'a>,
    scratch: &mut Scratch,
    spend: bool,
    sink: &mut impl Sink<'a>,
) -> bool {
    let Some(paint) = colr.base_glyph_paint(glyph) else {
        return false;
    };
    let delta = deltas(instance, colr);
    let s = &mut scratch.v1;
    let clip = colr.clip_box(glyph);
    let bounded = clip.is_some() || {
        let mut b = Bounded {
            bounded: true,
            ..Bounded::default()
        };
        walk(
            &delta,
            colr,
            colors,
            glyph,
            paint,
            false,
            &mut s.bounded_glyphs,
            &mut s.bounded_layers,
            &mut s.stops,
            &mut Direct(&mut b),
        );
        b.bounded
    };
    if let Some(b) = &clip {
        sink.push_clip_rectangle(clip_rect(b, &delta));
    }
    if bounded {
        walk(
            &delta,
            colr,
            colors,
            glyph,
            paint,
            spend,
            &mut s.glyphs,
            &mut s.layers,
            &mut s.stops,
            sink,
        );
    }
    if clip.is_some() {
        sink.pop_clip();
    }
    true
}
