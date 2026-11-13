//! Closed glyph stroking as FreeType 2.15.0's `FT_Glyph_Stroke` and
//! `FT_Glyph_StrokeBorder` (sofel ADR 0076).

use alloc::vec::Vec;
use core::fmt;

use sofel_geom::{Command, Fixed, FnPen, Pen, Point};

use super::stroke_line::{Join, Stroker};
use super::{Scratch, decompose, slot_synth};

/// The corner treatment of a stroked outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeJoin {
    /// Circular arc.
    Round,
    /// Connect the two offset edges directly.
    Bevel,
    /// Clip a sharp miter at the limit.
    MiterVariable,
    /// Replace a sharp miter with a bevel.
    MiterFixed,
}

/// The stroke geometry in the loaded outline's coordinate units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrokeStyle {
    /// Half the stroke width; must be nonnegative.
    pub radius: i32,
    /// The corner treatment.
    pub join: StrokeJoin,
    /// Signed 16.16 limit, clamped to at least one by FreeType.
    pub miter_limit: Fixed,
}

/// Which part of the stroke to return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeBorder {
    /// Both sides of each closed contour.
    Full,
    /// The outside border of the whole outline.
    Outer,
    /// The inside border of the whole outline.
    Inner,
}

/// A stroke could not be generated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeError {
    /// The radius is negative.
    InvalidRadius,
    /// Coordinates, points, or contours exceed the supported range.
    TooLarge,
}

impl fmt::Display for StrokeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRadius => f.write_str("stroke radius is negative"),
            Self::TooLarge => f.write_str("stroke outline is too large"),
        }
    }
}

impl core::error::Error for StrokeError {}

impl Scratch {
    /// Stroke the last loaded closed glyph outline into `pen`.
    /// The source remains unchanged and no command is sent on error.
    pub fn stroke(
        &self,
        style: StrokeStyle,
        border: StrokeBorder,
        pen: &mut impl Pen,
    ) -> Result<(), StrokeError> {
        if style.radius < 0 {
            return Err(StrokeError::InvalidRadius);
        }
        let join = match style.join {
            StrokeJoin::Round => Join::Round,
            StrokeJoin::Bevel => Join::Bevel,
            StrokeJoin::MiterVariable => Join::MiterVariable,
            StrokeJoin::MiterFixed => Join::MiterFixed,
        };
        let mut source = Vec::<Command>::new();
        self.decompose(Point::default(), 1, &mut FnPen(|c| source.push(c)))
            .map_err(|_| StrokeError::TooLarge)?;
        let mut stroker = Stroker::new(style.radius, join, style.miter_limit.0);
        for command in source {
            match command {
                Command::MoveTo(p) => stroker.begin(p),
                Command::LineTo(p) => stroker.line_to(p).map_err(|_| StrokeError::TooLarge)?,
                Command::QuadTo(c, p) => {
                    stroker.conic_to(c, p).map_err(|_| StrokeError::TooLarge)?
                }
                Command::CubicTo(c1, c2, p) => stroker
                    .cubic_to(c1, c2, p)
                    .map_err(|_| StrokeError::TooLarge)?,
                Command::Close => stroker.end().map_err(|_| StrokeError::TooLarge)?,
            }
        }
        let outside_side =
            if slot_synth::true_type_orientation(&self.points, &self.ends) == Some(true) {
                0
            } else {
                1
            };
        let side = match border {
            StrokeBorder::Full => None,
            StrokeBorder::Outer => Some(outside_side),
            StrokeBorder::Inner => Some(if outside_side == 0 { 1 } else { 0 }),
        };
        let (points, tags, ends) = stroker.export(side).map_err(|_| StrokeError::TooLarge)?;
        let mut output = Vec::<Command>::new();
        decompose(
            &points,
            &tags,
            &ends,
            Point::default(),
            1,
            &mut FnPen(|c| output.push(c)),
        )
        .map_err(|_| StrokeError::TooLarge)?;
        for command in output {
            command.apply(pen);
        }
        Ok(())
    }
}
