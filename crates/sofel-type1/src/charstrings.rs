//! Index Type 1 charstrings after eexec decryption.

use crate::cipher::decrypt_charstring;
use crate::noalloc::charstrings::{Item, Slice, Span, scan};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

/// Invalid or truncated Type 1 charstring dictionary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharstringError;

impl fmt::Display for CharstringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 1 charstrings")
    }
}

impl core::error::Error for CharstringError {}

/// Glyph programs and local subroutines borrowed from a decrypted dictionary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Charstrings<'a> {
    glyphs: Vec<(&'a [u8], &'a [u8])>,
    subrs: Vec<Option<&'a [u8]>>,
    len_iv: i32,
}

impl<'a> Charstrings<'a> {
    /// Read `/Subrs` and `/CharStrings` binary objects with checked lengths.
    ///
    /// Allocates the tables of the CharStrings and Subrs (ADR 0094).
    pub fn parse(data: &'a [u8]) -> Result<Self, CharstringError> {
        Self::parse_with_offset(data).map_err(|_| CharstringError)
    }

    /// Return a byte offset in the decrypted private dictionary on failure.
    ///
    /// Allocates the tables of the CharStrings and Subrs (ADR 0094).
    pub fn parse_with_offset(data: &'a [u8]) -> Result<Self, usize> {
        let span = |(start, end): Span| data.get(start..end).unwrap_or(&[]);
        let mut glyphs = Vec::new();
        let mut subrs = Vec::new();
        let len_iv = scan(&mut Slice(data), &mut |item| match item {
            Item::SubrCount(count) => subrs = vec![None; count],
            Item::Subr { index, program } => {
                if let Some(slot) = subrs.get_mut(index) {
                    *slot = Some(span(program));
                }
            }
            Item::Glyph { name, program } => glyphs.push((span(name), span(program))),
        })?;
        Ok(Self {
            glyphs,
            subrs,
            len_iv,
        })
    }

    /// Number of glyph charstrings in source order.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// The encrypted program for a named glyph.
    pub fn glyph(&self, name: &[u8]) -> Option<&'a [u8]> {
        self.glyphs
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, bytes)| *bytes)
    }

    /// The glyph name at its source-order index.
    pub fn glyph_name(&self, index: usize) -> Option<&'a [u8]> {
        self.glyphs.get(index).map(|(name, _)| *name)
    }

    /// The source-order index of a glyph name.
    pub fn glyph_index(&self, name: &[u8]) -> Option<usize> {
        self.glyphs
            .iter()
            .position(|(candidate, _)| *candidate == name)
    }

    /// The encrypted local subroutine at `index`.
    pub fn subroutine(&self, index: usize) -> Option<&'a [u8]> {
        self.subrs.get(index).copied().flatten()
    }

    /// The number of random bytes before each encrypted charstring.
    pub fn len_iv(&self) -> i32 {
        self.len_iv
    }

    /// The decrypted program for a named glyph.
    ///
    /// Allocates the decrypted charstring (ADR 0094).
    pub fn glyph_program(&self, name: &[u8]) -> Option<Vec<u8>> {
        decrypt_charstring(self.glyph(name)?, self.len_iv)
    }

    /// The decrypted local subroutine at `index`.
    ///
    /// Allocates the decrypted subroutine (ADR 0094).
    pub fn subroutine_program(&self, index: usize) -> Option<Vec<u8>> {
        decrypt_charstring(self.subroutine(index)?, self.len_iv)
    }
}
