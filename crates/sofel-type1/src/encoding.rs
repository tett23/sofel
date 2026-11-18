//! Type 1 glyph-name encoding.

use crate::encoding_data::STANDARD_NAMES;
use crate::noalloc::encoding::{Declared, scan};

pub use crate::noalloc::encoding::EncodingError;

/// The glyph name assigned to each eight-bit character code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoding<'a> {
    names: [Option<&'a [u8]>; 256],
}

impl<'a> Encoding<'a> {
    /// Read a standard or explicit Type 1 encoding of at most 256 entries.
    pub fn parse(cleartext: &'a [u8]) -> Result<Self, EncodingError> {
        let mut names = [None; 256];
        let declared = scan(cleartext, &mut |code, name| {
            if let Some(slot) = names.get_mut(code) {
                *slot = name;
            }
        })?;
        Ok(Self {
            names: match declared {
                Declared::Standard => STANDARD_NAMES,
                Declared::Explicit => names,
            },
        })
    }

    /// The glyph name assigned to `code`, absent for `.notdef`.
    pub fn name(&self, code: u8) -> Option<&'a [u8]> {
        self.names.get(usize::from(code)).copied().flatten()
    }
}
