//! HarfBuzz 15.1.0 `OT/glyf/path-builder.hh` from loaded glyf points.

use sofel_geom::PointF;
use sofel_geom::float::i32_to_f32;

use crate::{OutlineError, Scratch, TAG_CUBIC, TAG_ON};

/// A HarfBuzz glyf drawing command with f32 coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HbOutlineCommand {
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

#[derive(Default)]
struct Builder {
    first_on: Option<PointF>,
    first_off: Option<PointF>,
    first_off2: Option<PointF>,
    last_off: Option<PointF>,
    last_off2: Option<PointF>,
}

#[allow(clippy::arithmetic_side_effects)] // HarfBuzz calculates implied midpoints in f32.
fn mid(a: PointF, b: PointF) -> PointF {
    PointF::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

impl Builder {
    fn point(&mut self, p: PointF, tag: u8, emit: &mut impl FnMut(HbOutlineCommand)) {
        let on = tag & TAG_ON != 0;
        let cubic = !on && tag & TAG_CUBIC != 0;
        if self.first_on.is_none() {
            if on {
                self.first_on = Some(p);
                emit(HbOutlineCommand::MoveTo(p));
            } else if cubic && self.first_off2.is_none() {
                self.first_off2 = self.first_off;
                self.first_off = Some(p);
            } else if let Some(first_off) = self.first_off {
                let middle = mid(first_off, p);
                self.first_on = Some(middle);
                self.last_off = Some(p);
                emit(HbOutlineCommand::MoveTo(middle));
            } else {
                self.first_off = Some(p);
            }
            return;
        }
        if let Some(last_off) = self.last_off {
            if on {
                if let Some(last_off2) = self.last_off2 {
                    emit(HbOutlineCommand::CubicTo(last_off2, last_off, p));
                    self.last_off2 = None;
                } else {
                    emit(HbOutlineCommand::QuadTo(last_off, p));
                }
                self.last_off = None;
            } else if cubic && self.last_off2.is_none() {
                self.last_off2 = Some(last_off);
                self.last_off = Some(p);
            } else {
                let middle = mid(last_off, p);
                if cubic {
                    if let Some(last_off2) = self.last_off2 {
                        emit(HbOutlineCommand::CubicTo(last_off2, last_off, middle));
                    }
                    self.last_off2 = None;
                } else {
                    emit(HbOutlineCommand::QuadTo(last_off, middle));
                }
                self.last_off = Some(p);
            }
        } else if on {
            emit(HbOutlineCommand::LineTo(p));
        } else {
            self.last_off = Some(p);
        }
    }

    fn end(&mut self, emit: &mut impl FnMut(HbOutlineCommand)) {
        if let (Some(first_off), Some(last_off)) = (self.first_off, self.last_off) {
            let middle = mid(last_off, self.first_off2.unwrap_or(first_off));
            if let Some(last_off2) = self.last_off2 {
                emit(HbOutlineCommand::CubicTo(last_off2, last_off, middle));
            } else {
                emit(HbOutlineCommand::QuadTo(last_off, middle));
            }
            self.last_off = None;
        }
        if let (Some(first_off), Some(first_on)) = (self.first_off, self.first_on) {
            if let Some(first_off2) = self.first_off2 {
                emit(HbOutlineCommand::CubicTo(first_off2, first_off, first_on));
            } else {
                emit(HbOutlineCommand::QuadTo(first_off, first_on));
            }
        } else if let (Some(last_off), Some(first_on)) = (self.last_off, self.first_on) {
            if let Some(last_off2) = self.last_off2 {
                emit(HbOutlineCommand::CubicTo(last_off2, last_off, first_on));
            } else {
                emit(HbOutlineCommand::QuadTo(last_off, first_on));
            }
        } else if let Some(first_on) = self.first_on {
            emit(HbOutlineCommand::LineTo(first_on));
        } else if let (Some(first_off2), Some(first_off)) = (self.first_off2, self.first_off) {
            let middle = mid(first_off2, first_off);
            emit(HbOutlineCommand::MoveTo(middle));
            emit(HbOutlineCommand::CubicTo(first_off2, first_off, middle));
        } else if let Some(first_off) = self.first_off {
            emit(HbOutlineCommand::MoveTo(first_off));
            emit(HbOutlineCommand::QuadTo(first_off, first_off));
        }
        *self = Self::default();
        emit(HbOutlineCommand::Close);
    }
}

impl Scratch {
    /// Decompose the last loaded glyf outline with HarfBuzz's f32
    /// implied midpoints. `x_factor` and `y_factor` are the font's
    /// scale divided by its units per em.
    #[allow(clippy::arithmetic_side_effects)] // HarfBuzz scales each point in f32.
    pub fn hb_decompose(
        &self,
        x_factor: f32,
        y_factor: f32,
        mut emit: impl FnMut(HbOutlineCommand),
    ) -> Result<(), OutlineError> {
        let mut first = 0;
        for &last in &self.ends {
            let points = self
                .points
                .get(first..=last)
                .ok_or(OutlineError::InvalidOutline)?;
            let tags = self
                .tags
                .get(first..=last)
                .ok_or(OutlineError::InvalidOutline)?;
            let mut builder = Builder::default();
            for (&point, &tag) in points.iter().zip(tags) {
                let p = PointF::new(
                    i32_to_f32(point.x) * x_factor,
                    i32_to_f32(point.y) * y_factor,
                );
                builder.point(p, tag, &mut emit);
            }
            builder.end(&mut emit);
            first = last.saturating_add(1);
        }
        Ok(())
    }
}
