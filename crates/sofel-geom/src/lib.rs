//! Fixed-point arithmetic that gives the same results as FreeType 2.15.0,
//! and the [`Pen`] that carries outlines between crates (ADR 0009).
//!
//! FreeType computes in C `long`, 64 bits on LP64 systems. This crate
//! takes and returns `i32` and computes in 64 bits; where FreeType's
//! result does not fit in `i32`, the result saturates.

#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod fixed;
mod pen;
mod trig;

pub use fixed::{
    F2Dot14, F26Dot6, Fixed, Matrix, Point, div_fix, mul_div, mul_div_no_round, mul_fix, transform,
};
pub use pen::{Command, FnPen, Pen};
pub use trig::vector_length;
