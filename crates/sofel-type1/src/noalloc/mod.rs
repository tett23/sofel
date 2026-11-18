//! Code that does not allocate (sofel ADR 0098): the validators of
//! sofel ADR 0096 and the charstring metrics.
//!
//! Nothing here names an allocating type or calls the crate's other
//! modules; `scripts/check-deps.sh` checks both. The parsers outside call
//! the readers here, so a validator and its parser read a format the same
//! way.

/// AFM pair metrics.
pub mod afm;
pub(crate) mod charstrings;
/// CID-keyed Type 1 fonts.
pub mod cid;
pub(crate) mod cipher;
pub(crate) mod encoding;
/// Type 1 charstring side bearings and advances.
pub mod metrics;
pub(crate) mod mm;
/// PFB sections.
pub mod pfb;
/// PFM pair metrics.
pub mod pfm;
pub(crate) mod text;
/// Type 1 fonts in PFA and PFB files.
pub mod type1;
/// Type 42 fonts.
pub mod type42;

/// One problem found by a validator: where it is and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The byte offset in the validated file.
    pub offset: usize,
    /// A description for people.
    pub reason: &'static str,
}
