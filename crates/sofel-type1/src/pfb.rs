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

/// The storage kind of one PFB section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    /// PostScript text.
    Ascii,
    /// Binary encrypted data.
    Binary,
}

/// One PFB section borrowing its payload from the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section<'a> {
    /// The section kind.
    pub kind: SectionKind,
    /// The unframed payload.
    pub data: &'a [u8],
}

/// A checked sequence of PFB sections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pfb<'a> {
    sections: Vec<Section<'a>>,
}

impl<'a> Pfb<'a> {
    /// Parse PFB framing without copying its payloads.
    pub fn parse(data: &'a [u8], max_output: usize) -> Result<Self, PfbError> {
        let mut at = 0usize;
        let mut total = 0usize;
        let mut sections = Vec::new();
        loop {
            let header = data
                .get(at..at.checked_add(2).ok_or(PfbError)?)
                .ok_or(PfbError)?;
            if header.first() != Some(&0x80) {
                return Err(PfbError);
            }
            let kind = *header.get(1).ok_or(PfbError)?;
            at = at.checked_add(2).ok_or(PfbError)?;
            if kind == 3 {
                return if sections.is_empty() || at != data.len() {
                    Err(PfbError)
                } else {
                    Ok(Self { sections })
                };
            }
            let kind = match kind {
                1 => SectionKind::Ascii,
                2 if !sections.is_empty() => SectionKind::Binary,
                _ => return Err(PfbError),
            };
            let size = data
                .get(at..at.checked_add(4).ok_or(PfbError)?)
                .ok_or(PfbError)?;
            let length =
                usize::try_from(u32::from_le_bytes(size.try_into().map_err(|_| PfbError)?))
                    .map_err(|_| PfbError)?;
            at = at.checked_add(4).ok_or(PfbError)?;
            let end = at.checked_add(length).ok_or(PfbError)?;
            let payload = data.get(at..end).ok_or(PfbError)?;
            total = total.checked_add(length).ok_or(PfbError)?;
            if total > max_output {
                return Err(PfbError);
            }
            sections.push(Section {
                kind,
                data: payload,
            });
            at = end;
        }
    }

    /// Return the source-order sections.
    pub fn sections(&self) -> &[Section<'a>] {
        &self.sections
    }
}

/// Reassemble the ASCII and binary PFB segments in their original order.
///
/// The output is bounded by both `max_output` and the total input length.
pub fn decode_pfb(data: &[u8], max_output: usize) -> Result<Vec<u8>, PfbError> {
    let pfb = Pfb::parse(data, max_output)?;
    let mut output = Vec::new();
    for section in pfb.sections() {
        output.extend_from_slice(section.data);
    }
    Ok(output)
}
