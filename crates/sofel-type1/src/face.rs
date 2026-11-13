//! Common character and glyph access to a Type 1 program.

use alloc::vec::Vec;
use core::fmt;

use crate::charstrings::Charstrings;
use crate::encoding::Encoding;
use crate::source::Type1Source;

/// Invalid glyph dictionary or encoding in a Type 1 source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceError;

impl fmt::Display for FaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 1 face")
    }
}

impl core::error::Error for FaceError {}

/// One Type 1 face with its encoded character and glyph programs.
#[derive(Clone, Debug)]
pub struct Type1Face<'a> {
    encoding: Encoding<'a>,
    charstrings: Charstrings<'a>,
}

impl<'a> Type1Face<'a> {
    /// Open a previously unpacked Type 1 source.
    pub fn new(source: &'a Type1Source) -> Result<Self, FaceError> {
        let encoding = Encoding::parse(source.cleartext()).map_err(|_| FaceError)?;
        let charstrings = Charstrings::parse(source.private()).map_err(|_| FaceError)?;
        Ok(Self {
            encoding,
            charstrings,
        })
    }

    /// Number of indexed glyphs.
    pub fn num_glyphs(&self) -> usize {
        self.charstrings.glyph_count()
    }

    /// The glyph index for an eight-bit encoded character.
    pub fn nominal_glyph(&self, code: u8) -> Option<usize> {
        self.charstrings.glyph_index(self.encoding.name(code)?)
    }

    /// The PostScript name of a glyph index.
    pub fn glyph_name(&self, glyph: usize) -> Option<&'a [u8]> {
        self.charstrings.glyph_name(glyph)
    }

    /// The decrypted charstring of a glyph index.
    pub fn glyph_program(&self, glyph: usize) -> Option<Vec<u8>> {
        self.charstrings.glyph_program(self.glyph_name(glyph)?)
    }
}
