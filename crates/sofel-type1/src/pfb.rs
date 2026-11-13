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

/// Reassemble the ASCII and binary PFB segments in their original order.
///
/// The output is bounded by both `max_output` and the total input length.
pub fn decode_pfb(data: &[u8], max_output: usize) -> Result<Vec<u8>, PfbError> {
    let mut at = 0usize;
    let mut output = Vec::new();
    let mut first = true;
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
            return if first || at != data.len() {
                Err(PfbError)
            } else {
                Ok(output)
            };
        }
        if !matches!(kind, 1 | 2) || (first && kind != 1) {
            return Err(PfbError);
        }
        first = false;
        let size = data
            .get(at..at.checked_add(4).ok_or(PfbError)?)
            .ok_or(PfbError)?;
        let length = u32::from_le_bytes(size.try_into().map_err(|_| PfbError)?);
        let length = usize::try_from(length).map_err(|_| PfbError)?;
        at = at.checked_add(4).ok_or(PfbError)?;
        let end = at.checked_add(length).ok_or(PfbError)?;
        let section = data.get(at..end).ok_or(PfbError)?;
        let output_length = output.len().checked_add(length).ok_or(PfbError)?;
        if output_length > max_output {
            return Err(PfbError);
        }
        output.extend_from_slice(section);
        at = end;
    }
}
