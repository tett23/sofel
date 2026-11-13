//! sofel ADR 0076: stroking never mutates the loaded outline or writes on error.
#![allow(clippy::unwrap_used)]
use sofel_geom::{Command, Fixed, FnPen};
use sofel_outline::{Scratch, StrokeBorder, StrokeError, StrokeJoin, StrokeStyle};

#[test]
fn inv1_stroke_reference_empty_outline() {
    let scratch = Scratch::default();
    let mut commands = Vec::new();
    scratch
        .stroke(
            StrokeStyle {
                radius: 0,
                join: StrokeJoin::Round,
                miter_limit: Fixed(1 << 16),
            },
            StrokeBorder::Full,
            &mut FnPen(|command| commands.push(command)),
        )
        .unwrap();
    assert_eq!(commands, Vec::<Command>::new());
}

#[test]
fn inv3_stroke_failure_negative_radius_writes_nothing() {
    let scratch = Scratch::default();
    let mut commands = Vec::new();
    let result = scratch.stroke(
        StrokeStyle {
            radius: -1,
            join: StrokeJoin::Round,
            miter_limit: Fixed(1 << 16),
        },
        StrokeBorder::Full,
        &mut FnPen(|command| commands.push(command)),
    );
    assert_eq!(result, Err(StrokeError::InvalidRadius));
    assert!(commands.is_empty());
}
