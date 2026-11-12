//! OpenType SVG document data (ADR 0066).
#![no_std]

extern crate alloc;
use alloc::borrow::Cow;

/// Reads an SVG document, borrowing plain bytes or expanding a gzip member.
///
/// # Errors
/// Returns a decompression error for invalid gzip data, a failed checksum,
/// or when the expanded data exceeds the supplied limit.
pub fn document(data: &[u8], limit: usize) -> Result<Cow<'_, [u8]>, sofel_inflate::Error> {
    if data.starts_with(&[0x1f, 0x8b]) {
        sofel_inflate::gzip(data, limit).map(Cow::Owned)
    } else {
        Ok(Cow::Borrowed(data))
    }
}
