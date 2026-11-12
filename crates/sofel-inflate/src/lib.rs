//! Bounded DEFLATE, zlib and gzip decoding (RFC 1951, 1950 and 1952).
#![no_std]
extern crate alloc;
use alloc::vec::Vec;

mod blocks;
mod wrappers;
pub use wrappers::{crc32, gzip, zlib};

/// Failure to decode the entire input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Malformed, truncated, or unsupported input.
    Invalid,
    /// A wrapper checksum or length disagrees with the decoded data.
    Checksum,
    /// Decoding would exceed the caller's output bound.
    Limit,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid compressed data",
            Self::Checksum => "compression checksum mismatch",
            Self::Limit => "decompressed size limit exceeded",
        })
    }
}
impl core::error::Error for Error {}
type Result<T> = core::result::Result<T, Error>;

/// Decodes a complete raw DEFLATE stream with an explicit byte limit.
///
/// # Errors
/// Returns an error for invalid or trailing data or if the result exceeds `limit`.
pub fn deflate(input: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    if blocks::decode(input, &mut out, limit, 32768)? != input.len() {
        return Err(Error::Invalid);
    }
    Ok(out)
}
