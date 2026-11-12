//! Small command-line tools built on sofel (ADR 0003). Each binary in
//! `src/bin` is a thin wrapper around a `run` function here, so that the
//! tools can also be driven from tests.

#![warn(missing_docs)]

pub mod color_raster;
pub mod glyph;
pub mod json;
pub mod paint;
pub mod render;
pub mod shape;
pub mod subset;
pub mod validate;
pub mod variation_options;
