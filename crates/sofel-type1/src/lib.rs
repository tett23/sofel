//! Type 1 fonts and their attached metrics (sofel ADR 0065).
#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]
extern crate alloc;

/// Adobe Font Metrics attachments.
pub mod afm;

/// Type 1 PFB stream container.
pub mod pfb;

/// PFM kerning metrics.
pub mod pfm;

/// Type 1 eexec and charstring decryption.
pub mod cipher;

/// Unpacked Type 1 font programs.
pub mod source;

/// Type 1 charstring and subroutine index.
pub mod charstrings;

/// Type 1 character encoding.
pub mod encoding;

/// Type 1 face character and glyph queries.
pub mod face;

/// Unscaled Type 1 charstring metrics.
pub mod metrics;
