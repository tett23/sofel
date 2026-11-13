//! Type 1 fonts and their attached metrics (sofel ADR 0065).
#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]
extern crate alloc;

/// Adobe Font Metrics attachments.
pub mod afm;

/// Type 1 PFB stream container.
pub mod pfb;
