//! Small command-line tools built on sofel (ADR 0003). Each binary in
//! `src/bin` is a thin wrapper around a `run` function here, so that the
//! tools can also be driven from tests.

#![warn(missing_docs)]

pub mod glyph;
pub mod json;
pub mod render;
pub mod shape;
pub mod validate;
