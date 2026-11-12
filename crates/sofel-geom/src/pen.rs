use crate::{Point, PointF};

/// A drawing command with floating-point coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FloatCommand {
    /// Start a contour.
    MoveTo(PointF),
    /// Add a straight segment.
    LineTo(PointF),
    /// Add a quadratic Bézier segment.
    QuadTo(PointF, PointF),
    /// Add a cubic Bézier segment.
    CubicTo(PointF, PointF, PointF),
    /// End a contour.
    Close,
}

/// Receives an outline as drawing commands (ADR 0009).
///
/// The order follows FreeType's `FT_Outline_Decompose`: each contour
/// starts with `move_to`, returns to its start point with an explicit
/// line or curve, and ends with `close`.
pub trait Pen {
    /// Starts a contour at `p`.
    fn move_to(&mut self, p: Point);
    /// Draws a straight line from the current point to `p`.
    fn line_to(&mut self, p: Point);
    /// Draws a quadratic Bézier curve from the current point to `p`.
    fn quad_to(&mut self, control: Point, p: Point);
    /// Draws a cubic Bézier curve from the current point to `p`.
    fn cubic_to(&mut self, control1: Point, control2: Point, p: Point);
    /// Ends the current contour, which the previous command has already
    /// brought back to its start point.
    fn close(&mut self);
}

/// One drawing command, for recording and replaying.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Command {
    /// [`Pen::move_to`].
    MoveTo(Point),
    /// [`Pen::line_to`].
    LineTo(Point),
    /// [`Pen::quad_to`]: the control point, then the end point.
    QuadTo(Point, Point),
    /// [`Pen::cubic_to`]: the two control points, then the end point.
    CubicTo(Point, Point, Point),
    /// [`Pen::close`].
    Close,
}

impl Command {
    /// Sends this command to `pen`.
    pub fn apply(self, pen: &mut impl Pen) {
        match self {
            Command::MoveTo(p) => pen.move_to(p),
            Command::LineTo(p) => pen.line_to(p),
            Command::QuadTo(c, p) => pen.quad_to(c, p),
            Command::CubicTo(c1, c2, p) => pen.cubic_to(c1, c2, p),
            Command::Close => pen.close(),
        }
    }
}

/// A [`Pen`] that passes every command to a function.
pub struct FnPen<F>(pub F);

impl<F: FnMut(Command)> Pen for FnPen<F> {
    fn move_to(&mut self, p: Point) {
        (self.0)(Command::MoveTo(p));
    }
    fn line_to(&mut self, p: Point) {
        (self.0)(Command::LineTo(p));
    }
    fn quad_to(&mut self, control: Point, p: Point) {
        (self.0)(Command::QuadTo(control, p));
    }
    fn cubic_to(&mut self, control1: Point, control2: Point, p: Point) {
        (self.0)(Command::CubicTo(control1, control2, p));
    }
    fn close(&mut self) {
        (self.0)(Command::Close);
    }
}
