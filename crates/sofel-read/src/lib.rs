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
//!
//! # Example
//!
//! ```
//! use sofel_read::sfnt::Sfnt;
//! use sofel_read::{Diagnostic, Tag, validate};
//!
//! fn main() -> Result<(), sofel_read::ReadError> {
//!     // A TrueType table directory with no tables.
//!     let file = [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
//!     let font = Sfnt::new(&file)?;
//!     assert_eq!(font.num_tables(), 0);
//!     assert_eq!(font.table(Tag::new(b"head")), None);
//!
//!     let mut problems = 0;
//!     validate::sfnt(&font, &mut |_: Diagnostic| problems += 1);
//!     assert_eq!(problems, 0);
//!     Ok(())
//! }
//! ```

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod base;
pub mod be;
pub mod cbdt;
pub mod cblc;
pub mod cff;
pub mod cmap;
pub mod colr;
pub mod cpal;
pub mod device;
mod diag;
mod error;
pub mod gdef;
pub mod glyf;
pub mod gpos;
pub mod gsub;
pub mod head;
pub mod hhea;
pub mod hmtx;
pub mod item_variation;
pub mod layout;
pub mod loca;
pub mod maxp;
pub mod name;
pub mod os2;
pub mod post;
pub mod sbix;
mod search;
pub mod sfnt;
pub mod svg;
mod types;
pub mod validate;
pub mod vhea;
pub mod vmtx;
pub mod vorg;

pub use diag::{DetailValue, Diagnostic, Kind, Severity};
pub use error::ReadError;
pub use types::{CountWidth, F2Dot14, Fixed, GlyphId, LongDateTime, Tag};
