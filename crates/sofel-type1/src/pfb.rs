//! PFB sections containing Type 1 PostScript and encrypted charstrings.

use alloc::vec::Vec;
use core::fmt;

/// Invalid PFB structure or exceeded output limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PfbError;

impl fmt::Display for PfbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid PFB font")
    }
}

impl core::error::Error for PfbError {}

pub use crate::noalloc::pfb::{Section, SectionKind};

/// A checked sequence of PFB sections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pfb<'a> {
    sections: Vec<Section<'a>>,
}

impl<'a> Pfb<'a> {
    /// Parse PFB framing without copying its payloads.
    ///
    /// Allocates the list of sections (ADR 0094).
    pub fn parse(data: &'a [u8], max_output: usize) -> Result<Self, PfbError> {
        Self::parse_with_offset(data, max_output).map_err(|_| PfbError)
    }

    /// Parse the framing and locate the invalid byte or truncated section.
    ///
    /// Allocates the list of sections (ADR 0094).
    pub fn parse_with_offset(data: &'a [u8], max_output: usize) -> Result<Self, usize> {
        let mut sections = Vec::new();
        crate::noalloc::pfb::sections(data, max_output, &mut |section| sections.push(section))?;
        Ok(Self { sections })
    }

    /// Return the source-order sections.
    pub fn sections(&self) -> &[Section<'a>] {
        &self.sections
    }
}

/// Reassemble the ASCII and binary PFB segments in their original order.
///
/// The output is bounded by both `max_output` and the total input length.
///
/// Allocates the list of sections and the joined bytes (ADR 0094).
pub fn decode_pfb(data: &[u8], max_output: usize) -> Result<Vec<u8>, PfbError> {
    let pfb = Pfb::parse(data, max_output)?;
    let mut output = Vec::new();
    for section in pfb.sections() {
        output.extend_from_slice(section.data);
    }
    Ok(output)
}
