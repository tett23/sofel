//! Zero-copy, allocation-free readers and validators for sfnt font tables.
//!
//! Views are built over `&[u8]`. Building a view checks the header and
//! the extent of the arrays it describes once; reads after that return
//! `None` for out-of-range input instead of filling in defaults
//! (ADR 0007). Every table also has a validator that reports
//! [`Diagnostic`]s to a callback without allocating.
//!
//! No input makes this crate panic: indexing, unchecked arithmetic and
//! lossy casts are denied by clippy lints, and the crate is fuzzed.

#![no_std]
#![forbid(unsafe_code)]

pub mod be;
pub mod cmap;
mod diag;
mod error;
pub mod glyf;
pub mod head;
pub mod hhea;
pub mod hmtx;
pub mod loca;
pub mod maxp;
pub mod sfnt;
mod types;
pub mod validate;

pub use diag::{DetailValue, Diagnostic, Kind, Severity};
pub use error::ReadError;
pub use types::{CountWidth, F2Dot14, Fixed, GlyphId, LongDateTime, Tag};
