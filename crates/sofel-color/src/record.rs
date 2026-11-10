//! Paint commands as values: [`Recording`] and [`Recording::replay`].

use alloc::vec::Vec;
use core::ops::Range;

use sofel_outline::font::geom::{
    Brush, ColorLine, ColorStop, CompositeMode, Extend, Image, Painter, PointF, Rect, Solid,
    Transform,
};

/// A [`Brush`] whose color stops are a range of [`Recording::stops`].
#[derive(Clone, PartialEq, Debug)]
pub enum RecordedBrush {
    /// [`Brush::Solid`].
    Solid(Solid),
    /// [`Brush::Linear`].
    Linear {
        /// The stops' range and the extend mode.
        stops: Range<usize>,
        /// The extend mode.
        extend: Extend,
        /// The start point.
        p0: PointF,
        /// The end point.
        p1: PointF,
        /// The rotation point.
        p2: PointF,
    },
    /// [`Brush::Radial`].
    Radial {
        /// The stops' range.
        stops: Range<usize>,
        /// The extend mode.
        extend: Extend,
        /// The start center.
        c0: PointF,
        /// The start radius.
        r0: f32,
        /// The end center.
        c1: PointF,
        /// The end radius.
        r1: f32,
    },
    /// [`Brush::Sweep`].
    Sweep {
        /// The stops' range.
        stops: Range<usize>,
        /// The extend mode.
        extend: Extend,
        /// The center.
        center: PointF,
        /// The start angle.
        start: f32,
        /// The end angle.
        end: f32,
    },
}

/// One [`Painter`] call.
#[derive(Clone, PartialEq, Debug)]
pub enum PaintCommand<'a> {
    /// [`Painter::push_transform`].
    PushTransform(Transform),
    /// [`Painter::pop_transform`].
    PopTransform,
    /// [`Painter::push_clip_glyph`].
    PushClipGlyph(u32),
    /// [`Painter::push_clip_rectangle`].
    PushClipRectangle(Rect),
    /// [`Painter::pop_clip`].
    PopClip,
    /// [`Painter::fill`].
    Fill(RecordedBrush),
    /// [`Painter::fill_glyph`].
    FillGlyph(u32, Solid),
    /// [`Painter::image`]; the data borrows the font.
    Image(Image<'a>),
    /// [`Painter::push_group`].
    PushGroup(CompositeMode),
    /// [`Painter::pop_group`].
    PopGroup(CompositeMode),
    /// [`Painter::color_glyph`] (recorded as asked; the recording answers
    /// `false`, so the glyph's paint follows).
    ColorGlyph(u32),
}

/// A [`Painter`] that keeps the commands as values (ADR 0023), for
/// caches, other threads and other languages. Reuse it with
/// [`Recording::clear`].
#[derive(Clone, Default, Debug)]
pub struct Recording<'a> {
    commands: Vec<PaintCommand<'a>>,
    stops: Vec<ColorStop>,
}

impl<'a> Recording<'a> {
    /// An empty recording.
    pub fn new() -> Recording<'a> {
        Recording::default()
    }
    /// Forgets the commands, keeping the memory.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.stops.clear();
    }
    /// The commands, in order.
    pub fn commands(&self) -> &[PaintCommand<'a>] {
        &self.commands
    }
    /// The color stops the gradients refer to.
    pub fn stops(&self) -> &[ColorStop] {
        &self.stops
    }
    /// The [`Brush`] of a recorded fill.
    pub fn brush(&self, b: &RecordedBrush) -> Brush<'_> {
        let line = |r: &Range<usize>, extend: Extend| ColorLine {
            stops: self.stops.get(r.clone()).unwrap_or_default(),
            extend,
        };
        match b {
            RecordedBrush::Solid(s) => Brush::Solid(*s),
            RecordedBrush::Linear {
                stops,
                extend,
                p0,
                p1,
                p2,
            } => Brush::Linear {
                line: line(stops, *extend),
                p0: *p0,
                p1: *p1,
                p2: *p2,
            },
            RecordedBrush::Radial {
                stops,
                extend,
                c0,
                r0,
                c1,
                r1,
            } => Brush::Radial {
                line: line(stops, *extend),
                c0: *c0,
                r0: *r0,
                c1: *c1,
                r1: *r1,
            },
            RecordedBrush::Sweep {
                stops,
                extend,
                center,
                start,
                end,
            } => Brush::Sweep {
                line: line(stops, *extend),
                center: *center,
                start: *start,
                end: *end,
            },
        }
    }

    /// Sends the commands to `painter`, in order. The answers of
    /// [`Painter::image`] and [`Painter::color_glyph`] are not used.
    pub fn replay(&self, painter: &mut impl Painter) {
        for c in &self.commands {
            match c {
                PaintCommand::PushTransform(t) => painter.push_transform(*t),
                PaintCommand::PopTransform => painter.pop_transform(),
                PaintCommand::PushClipGlyph(g) => painter.push_clip_glyph(*g),
                PaintCommand::PushClipRectangle(r) => painter.push_clip_rectangle(*r),
                PaintCommand::PopClip => painter.pop_clip(),
                PaintCommand::Fill(b) => painter.fill(&self.brush(b)),
                PaintCommand::FillGlyph(g, s) => painter.fill_glyph(*g, s),
                PaintCommand::Image(i) => {
                    painter.image(i);
                }
                PaintCommand::PushGroup(m) => painter.push_group(*m),
                PaintCommand::PopGroup(m) => painter.pop_group(*m),
                PaintCommand::ColorGlyph(g) => {
                    painter.color_glyph(*g);
                }
            }
        }
    }

    fn copy_stops(&mut self, line: &ColorLine<'_>) -> Range<usize> {
        let start = self.stops.len();
        self.stops.extend_from_slice(line.stops);
        start..self.stops.len()
    }

    /// Records an image whose data borrows the font.
    pub(crate) fn push_image(&mut self, image: Image<'a>) {
        self.commands.push(PaintCommand::Image(image));
    }
}

impl<'a> Painter for Recording<'a> {
    fn push_transform(&mut self, t: Transform) {
        self.commands.push(PaintCommand::PushTransform(t));
    }
    fn pop_transform(&mut self) {
        self.commands.push(PaintCommand::PopTransform);
    }
    fn push_clip_glyph(&mut self, glyph: u32) {
        self.commands.push(PaintCommand::PushClipGlyph(glyph));
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        self.commands.push(PaintCommand::PushClipRectangle(r));
    }
    fn pop_clip(&mut self) {
        self.commands.push(PaintCommand::PopClip);
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        let b = match brush {
            Brush::Solid(s) => RecordedBrush::Solid(*s),
            Brush::Linear { line, p0, p1, p2 } => RecordedBrush::Linear {
                stops: self.copy_stops(line),
                extend: line.extend,
                p0: *p0,
                p1: *p1,
                p2: *p2,
            },
            Brush::Radial {
                line,
                c0,
                r0,
                c1,
                r1,
            } => RecordedBrush::Radial {
                stops: self.copy_stops(line),
                extend: line.extend,
                c0: *c0,
                r0: *r0,
                c1: *c1,
                r1: *r1,
            },
            Brush::Sweep {
                line,
                center,
                start,
                end,
            } => RecordedBrush::Sweep {
                stops: self.copy_stops(line),
                extend: line.extend,
                center: *center,
                start: *start,
                end: *end,
            },
        };
        self.commands.push(PaintCommand::Fill(b));
    }
    fn fill_glyph(&mut self, glyph: u32, color: &Solid) {
        self.commands.push(PaintCommand::FillGlyph(glyph, *color));
    }
    /// Images reach a recording through [`crate::paint_recording`], which
    /// keeps the borrow of the font; an image sent here directly cannot
    /// be kept and is refused.
    fn image(&mut self, _image: &Image<'_>) -> bool {
        false
    }
    fn push_group(&mut self, mode: CompositeMode) {
        self.commands.push(PaintCommand::PushGroup(mode));
    }
    fn pop_group(&mut self, mode: CompositeMode) {
        self.commands.push(PaintCommand::PopGroup(mode));
    }
    fn color_glyph(&mut self, glyph: u32) -> bool {
        self.commands.push(PaintCommand::ColorGlyph(glyph));
        false
    }
}
