//! The box of painted glyphs: HarfBuzz's `hb_paint_extents_context_t`
//! (`src/hb-paint-extents.hh`, `src/hb-paint-extents.cc`) and the bounds
//! of `src/hb-geometry.hh` (`hb_extents_t`, `hb_bounds_t`), as a
//! [`Painter`]. Glyph outlines are represented by their boxes.

use alloc::vec::Vec;

use sofel_outline::font::geom::{
    Brush, CompositeMode, GlyphExtents, Image, Painter, PointF, Rect, Transform,
};

/// `hb_extents_t<float>`: void when `x_min > x_max`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Extents {
    pub(crate) x_min: f32,
    pub(crate) y_min: f32,
    pub(crate) x_max: f32,
    pub(crate) y_max: f32,
}

impl Default for Extents {
    fn default() -> Extents {
        Extents::VOID
    }
}

fn min(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

fn max(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// C's `round` (halfway away from zero) of a finite `f64`.
fn round_c(v: f64) -> f64 {
    let t = trunc(v);
    if (v - t).abs() >= 0.5 {
        t + v.signum()
    } else {
        t
    }
}

fn trunc(v: f64) -> f64 {
    if v.abs() >= 4_503_599_627_370_496.0 {
        return v;
    }
    // |v| < 2^52.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let t = (v as i64) as f64;
    t
}

/// `clamp_to_hb_position`: clamped to `i32` and truncated.
fn position(v: f64) -> i32 {
    let c = v.clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    // Clamped to the i32 range.
    #[allow(clippy::cast_possible_truncation)]
    let i = c as i32;
    i
}

impl Extents {
    pub(crate) const VOID: Extents = Extents {
        x_min: 0.0,
        y_min: 0.0,
        x_max: -1.0,
        y_max: -1.0,
    };

    /// `hb_extents_t (const hb_glyph_extents_t &)`.
    pub(crate) fn from_glyph(e: &GlyphExtents) -> Extents {
        let x0 = f64::from(e.x_bearing);
        let y0 = f64::from(e.y_bearing);
        let x1 = x0 + f64::from(e.width);
        let y1 = y0 + f64::from(e.height);
        // `(Float)` of the f64 values.
        #[allow(clippy::cast_possible_truncation)]
        let f = |v: f64| v as f32;
        Extents {
            x_min: f(x0.min(x1)),
            y_min: f(y0.min(y1)),
            x_max: f(x0.max(x1)),
            y_max: f(y0.max(y1)),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.x_min >= self.x_max || self.y_min >= self.y_max
    }

    pub(crate) fn is_void(&self) -> bool {
        self.x_min > self.x_max
    }

    fn union(&mut self, o: &Extents) {
        if o.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = *o;
            return;
        }
        self.x_min = min(self.x_min, o.x_min);
        self.y_min = min(self.y_min, o.y_min);
        self.x_max = max(self.x_max, o.x_max);
        self.y_max = max(self.y_max, o.y_max);
    }

    fn intersect(&mut self, o: &Extents) {
        if o.is_empty() || self.is_empty() {
            *self = Extents::VOID;
            return;
        }
        self.x_min = max(self.x_min, o.x_min);
        self.y_min = max(self.y_min, o.y_min);
        self.x_max = min(self.x_max, o.x_max);
        self.y_max = min(self.y_max, o.y_max);
    }

    fn add_point(&mut self, x: f32, y: f32) {
        if self.is_void() {
            *self = Extents {
                x_min: x,
                y_min: y,
                x_max: x,
                y_max: y,
            };
        } else {
            self.x_min = min(self.x_min, x);
            self.y_min = min(self.y_min, y);
            self.x_max = max(self.x_max, x);
            self.y_max = max(self.y_max, y);
        }
    }

    /// `hb_transform_t::transform_extents`.
    pub(crate) fn transform(&self, t: &Transform) -> Extents {
        if self.is_void() {
            return *self;
        }
        let corners = [
            (self.x_min, self.y_min),
            (self.x_min, self.y_max),
            (self.x_max, self.y_min),
            (self.x_max, self.y_max),
        ];
        corners.iter().fold(Extents::VOID, |mut e, &(x, y)| {
            let p = t.apply(PointF::new(x, y));
            e.add_point(p.x, p.y);
            e
        })
    }

    /// `to_glyph_extents` (not mirrored): C's `round` of each edge.
    pub(crate) fn to_glyph(self) -> GlyphExtents {
        let (x0, y0) = (
            round_c(f64::from(self.x_min)),
            round_c(f64::from(self.y_min)),
        );
        let (x1, y1) = (
            round_c(f64::from(self.x_max)),
            round_c(f64::from(self.y_max)),
        );
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return GlyphExtents::default();
        }
        GlyphExtents {
            x_bearing: position(x0),
            y_bearing: position(y1),
            width: position(x1 - x0),
            height: position(y0 - y1),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Status {
    Unbounded,
    Bounded,
    Empty,
}

/// `hb_bounds_t<float>`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Bounds {
    status: Status,
    extents: Extents,
}

impl Bounds {
    const UNBOUNDED: Bounds = Bounds {
        status: Status::Unbounded,
        extents: Extents::VOID,
    };
    const EMPTY: Bounds = Bounds {
        status: Status::Empty,
        extents: Extents::VOID,
    };

    fn of(extents: Extents) -> Bounds {
        Bounds {
            status: if extents.is_empty() {
                Status::Empty
            } else {
                Status::Bounded
            },
            extents,
        }
    }

    fn union(&mut self, o: &Bounds) {
        match o.status {
            Status::Unbounded => self.status = Status::Unbounded,
            Status::Bounded => match self.status {
                Status::Empty => *self = *o,
                Status::Bounded => self.extents.union(&o.extents),
                Status::Unbounded => {}
            },
            Status::Empty => {}
        }
    }

    fn intersect(&mut self, o: &Bounds) {
        match o.status {
            Status::Empty => self.status = Status::Empty,
            Status::Bounded => match self.status {
                Status::Unbounded => *self = *o,
                Status::Bounded => {
                    self.extents.intersect(&o.extents);
                    if self.extents.is_empty() {
                        self.status = Status::Empty;
                    }
                }
                Status::Empty => {}
            },
            Status::Unbounded => {}
        }
    }
}

/// HarfBuzz's paint-extents context, with a function giving the box of
/// a glyph outline (in the painter's units).
pub(crate) struct ExtentsPainter<'s, F> {
    transforms: &'s mut Vec<Transform>,
    clips: &'s mut Vec<Bounds>,
    groups: &'s mut Vec<Bounds>,
    glyph_box: F,
}

/// The stacks of an [`ExtentsPainter`], kept in the caller's scratch.
#[derive(Clone, Default, Debug)]
pub(crate) struct ExtentsStacks {
    transforms: Vec<Transform>,
    clips: Vec<Bounds>,
    groups: Vec<Bounds>,
}

impl<'s, F: FnMut(u32) -> Option<Extents>> ExtentsPainter<'s, F> {
    pub(crate) fn new(stacks: &'s mut ExtentsStacks, glyph_box: F) -> ExtentsPainter<'s, F> {
        stacks.transforms.clear();
        stacks.clips.clear();
        stacks.groups.clear();
        stacks.transforms.push(Transform::IDENTITY);
        stacks.clips.push(Bounds::UNBOUNDED);
        stacks.groups.push(Bounds::EMPTY);
        ExtentsPainter {
            transforms: &mut stacks.transforms,
            clips: &mut stacks.clips,
            groups: &mut stacks.groups,
            glyph_box,
        }
    }

    /// The painted extents (`get_extents`).
    pub(crate) fn extents(&self) -> Extents {
        self.groups.last().map_or(Extents::VOID, |g| g.extents)
    }

    fn push_clip(&mut self, e: Extents) {
        let t = self.transforms.last().copied().unwrap_or_default();
        let mut b = Bounds::of(e.transform(&t));
        if let Some(c) = self.clips.last() {
            b.intersect(c);
        }
        self.clips.push(b);
    }

    fn paint(&mut self) {
        let clip = self.clips.last().copied().unwrap_or(Bounds::UNBOUNDED);
        if let Some(g) = self.groups.last_mut() {
            g.union(&clip);
        }
    }
}

impl<F: FnMut(u32) -> Option<Extents>> Painter for ExtentsPainter<'_, F> {
    fn push_transform(&mut self, t: Transform) {
        let top = self.transforms.last().copied().unwrap_or_default();
        self.transforms.push(top.multiply(&t));
    }
    fn pop_transform(&mut self) {
        self.transforms.pop();
    }
    fn push_clip_glyph(&mut self, glyph: u32) {
        let e = (self.glyph_box)(glyph)
            .filter(|e| e.x_min != e.x_max || e.y_min != e.y_max)
            .unwrap_or(Extents::VOID);
        self.push_clip(e);
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        self.push_clip(Extents {
            x_min: r.x_min,
            y_min: r.y_min,
            x_max: r.x_max,
            y_max: r.y_max,
        });
    }
    fn pop_clip(&mut self) {
        self.clips.pop();
    }
    fn fill(&mut self, _brush: &Brush<'_>) {
        self.paint();
    }
    fn image(&mut self, image: &Image<'_>) -> bool {
        let Some(e) = image.extents else {
            return false;
        };
        self.push_clip(Extents::from_glyph(&GlyphExtents {
            height: e.height,
            ..e
        }));
        self.paint();
        self.pop_clip();
        true
    }
    fn push_group(&mut self, _mode: CompositeMode) {
        self.groups.push(Bounds::EMPTY);
    }
    fn pop_group(&mut self, mode: CompositeMode) {
        let src = self.groups.pop().unwrap_or(Bounds::EMPTY);
        let Some(backdrop) = self.groups.last_mut() else {
            return;
        };
        use CompositeMode::*;
        match mode {
            Clear => backdrop.status = Status::Empty,
            Src | SrcOut => *backdrop = src,
            Dest | DestOut => {}
            SrcIn | DestIn => backdrop.intersect(&src),
            _ => backdrop.union(&src),
        }
    }
}
