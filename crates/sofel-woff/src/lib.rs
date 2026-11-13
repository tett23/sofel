//! Single-face sfnt encoding for web delivery (ADR 0070).
#![no_std]

extern crate alloc;
use alloc::vec::Vec;
use core::fmt;
use sofel_read::{
    Tag,
    sfnt::{Sfnt, TableRecord},
};
use sofel_write::checksum;

mod compress;
mod decode;
pub use decode::{DecodeError, decode_woff};
mod woff;
mod woff2;
pub use woff2::decode_woff2;

/// Output container.
#[derive(Clone, Copy, Debug)]
pub enum Format {
    /// WOFF 1.0 with independent zlib streams.
    Woff,
    /// WOFF2 with an untransformed Brotli font data stream.
    Woff2,
}

/// Reason why a font cannot be represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// The input directory, ranges, padding, or checksums are invalid.
    InvalidSfnt,
    /// A container length or offset exceeds the format limit.
    TooLarge,
}
impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidSfnt => "invalid sfnt directory, padding or checksum",
            Self::TooLarge => "web font exceeds 32-bit size",
        })
    }
}
impl core::error::Error for EncodeError {}

type Result<T> = core::result::Result<T, EncodeError>;

fn invalid() -> EncodeError {
    EncodeError::InvalidSfnt
}
fn size(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| EncodeError::TooLarge)
}
fn aligned(n: usize) -> Result<usize> {
    n.checked_next_multiple_of(4).ok_or_else(invalid)
}
fn table(record: TableRecord, input: &[u8]) -> Result<&[u8]> {
    record.data(input).ok_or_else(invalid)
}

/// Validates and encodes a complete, single-face font.
///
/// # Errors
/// Returns an error for an invalid or unrepresentable font, or a compression failure.
pub fn encode(input: &[u8], format: Format) -> Result<Vec<u8>> {
    size(input.len())?;
    let font = Sfnt::new(input).map_err(|_| invalid())?;
    let count = font.num_tables();
    if count == 0 || count >= 4096 || !matches!(font.version(), 0x10000 | 0x4f54544f | 0x74727565) {
        return Err(invalid());
    }
    let range = 16u32 << count.ilog2();
    if u32::from(font.entry_selector()) != count.ilog2()
        || u32::from(font.search_range()) != range
        || u32::from(font.range_shift()) != (16 * u32::from(count)).saturating_sub(range)
    {
        return Err(invalid());
    }
    let mut records: Vec<_> = font.records().collect();
    if !records.is_sorted_by(|a, b| a.tag < b.tag) || checksum(input) != 0xb1b0afba {
        return Err(invalid());
    }
    let head = font
        .table(Tag::new(b"head"))
        .filter(|h| h.len() >= 54)
        .ok_or_else(invalid)?;
    let adjustment = u32::from_be_bytes(
        head.get(8..12)
            .and_then(|b| b.try_into().ok())
            .ok_or_else(invalid)?,
    );
    records.sort_by_key(|r| (r.offset, r.length));
    let mut end = 12usize.saturating_add(16 * usize::from(count));
    for &r in &records {
        if !r
            .tag
            .to_u32()
            .to_be_bytes()
            .iter()
            .all(|b| (0x20..=0x7e).contains(b))
        {
            return Err(invalid());
        }
        let start = r.offset as usize;
        if start < end
            || !start.is_multiple_of(4)
            || (matches!(format, Format::Woff) && start != end)
        {
            return Err(invalid());
        }
        let data = table(r, input)?;
        let sum = checksum(data);
        if (if r.tag == Tag::new(b"head") {
            sum.wrapping_sub(adjustment)
        } else {
            sum
        }) != r.checksum
        {
            return Err(invalid());
        }
        let raw_end = start.checked_add(data.len()).ok_or_else(invalid)?;
        end = aligned(raw_end)?;
        if input
            .get(raw_end..end)
            .is_none_or(|p| p.iter().any(|&b| b != 0))
        {
            return Err(invalid());
        }
    }
    if matches!(format, Format::Woff) && input.len() != end {
        return Err(invalid());
    }
    match format {
        Format::Woff => woff::encode(font, &records),
        Format::Woff2 => woff2::encode(font, records),
    }
}

fn header(signature: &[u8; 4], flavor: u32, count: u16, sfnt_size: u32) -> Vec<u8> {
    let mut h = Vec::from(signature);
    h.extend(flavor.to_be_bytes());
    h.extend(0u32.to_be_bytes());
    h.extend(count.to_be_bytes());
    h.extend(0u16.to_be_bytes());
    h.extend(sfnt_size.to_be_bytes());
    h
}
fn finish(mut bytes: Vec<u8>) -> Result<Vec<u8>> {
    let length = size(bytes.len())?;
    bytes
        .get_mut(8..12)
        .ok_or_else(invalid)?
        .copy_from_slice(&length.to_be_bytes());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn i3_webfont_header_size_limit() {
        assert!(size(u32::MAX as usize).is_ok());
        assert!(size((u32::MAX as usize).saturating_add(1)).is_err());
        assert!(aligned(usize::MAX).is_err());
    }
}
