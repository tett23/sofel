//! Small command-line tools built on sofel (ADR 0003). Each binary in
//! `src/bin` is a thin wrapper around a `run` function here, so that the
//! tools can also be driven from tests.

#![warn(missing_docs)]

mod colr_raster;
pub mod color_raster;
pub mod glyph;
pub mod json;
pub mod paint;
pub mod render;
pub mod shape;
pub mod subset;
mod text_image;
pub mod validate;
pub mod variation_options;
