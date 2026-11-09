use crate::Point;

/// Receives an outline as drawing commands (ADR 0009).
///
/// The order follows FreeType's `FT_Outline_Decompose`: each contour
/// starts with `move_to`, returns to its start point with an explicit
/// line or curve, and ends with `close`.
pub trait Pen {
    fn move_to(&mut self, p: Point);
    fn line_to(&mut self, p: Point);
    fn quad_to(&mut self, control: Point, p: Point);
    fn cubic_to(&mut self, control1: Point, control2: Point, p: Point);
    fn close(&mut self);
}

/// One drawing command, for recording and replaying.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Command {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
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
