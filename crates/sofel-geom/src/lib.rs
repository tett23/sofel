//! Fixed-point arithmetic that gives the same results as FreeType 2.15.0,
//! the [`Pen`] that carries outlines between crates (ADR 0009), in
//! [`float`] the floating-point operations that copy HarfBuzz's rounding
//! (ADR 0026), and the [`Painter`] that carries the paint commands of
//! color glyphs, with its values, transforms and helper functions
//! (ADR 0023).
//!
//! FreeType computes in C `long`, 64 bits on LP64 systems. This crate
//! takes and returns `i32` and computes in 64 bits; where FreeType's
//! result does not fit in `i32`, the result saturates.
//!
//! Paint values are `f32`, as in HarfBuzz and most 2D renderers. The
//! arithmetic is IEEE 754 without fused multiply-add, and the sine,
//! cosine and tangent are this crate's own ([`sin_cos`], [`tan`]), so
//! the results are the same on every target.

#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod fixed;
pub mod float;
mod paint;
mod paint_util;
mod pen;
pub mod sort;
mod trig;
mod trigf;

pub use fixed::{
    F2Dot14, F26Dot6, Fixed, Matrix, Point, div_fix, mul_div, mul_div_no_round, mul_fix, transform,
};
pub use paint::{
    Brush, Color, ColorLine, ColorRef, ColorStop, CompositeMode, Extend, GlyphExtents, Image,
    ImageFormat, Painter, PointF, Rect, Solid, Transform,
};
pub use paint_util::{
    interpolate_sweep_color, normalize_color_line, reduce_linear_anchors, sweep_gradient_tiles,
};
pub use pen::{Command, FloatCommand, FnPen, Pen};
pub use trig::vector_length;
pub use trigf::{cos, sin, sin_cos, tan};
