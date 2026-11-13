//! Common character and glyph access to a Type 1 program.

use alloc::vec::Vec;
use core::fmt;

use crate::afm::Afm;
use crate::charstrings::Charstrings;
use crate::encoding::Encoding;
use crate::metrics::{Metrics, parse_metrics};
use crate::multiple_masters::MultipleMasters;
use crate::pfm::Pfm;
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
    multiple_masters: Option<MultipleMasters<'a>>,
    attached: Option<AttachedMetrics<'a>>,
}

#[derive(Clone, Debug)]
enum AttachedMetrics<'a> {
    Afm(Afm<'a>),
    Pfm(Vec<(usize, usize, i16)>),
}

impl<'a> Type1Face<'a> {
    /// Open a previously unpacked Type 1 source.
    pub fn new(source: &'a Type1Source) -> Result<Self, FaceError> {
        let encoding = Encoding::parse(source.cleartext()).map_err(|_| FaceError)?;
        let charstrings = Charstrings::parse(source.private()).map_err(|_| FaceError)?;
        let multiple_masters = MultipleMasters::parse(source.cleartext()).map_err(|_| FaceError)?;
        Ok(Self {
            encoding,
            charstrings,
            multiple_masters,
            attached: None,
        })
    }

    /// Multiple Masters axis metadata, when the Type 1 source has it.
    pub fn multiple_masters(&self) -> Option<&MultipleMasters<'a>> {
        self.multiple_masters.as_ref()
    }

    /// Number of indexed glyphs.
    pub fn num_glyphs(&self) -> usize {
        self.charstrings.glyph_count()
    }

    /// The glyph index for an eight-bit encoded character.
    pub fn nominal_glyph(&self, code: u8) -> Option<usize> {
        self.charstrings.glyph_index(self.encoding.name(code)?)
    }

    /// Unicode mapping synthesized from PostScript glyph names.
    pub fn unicode_glyph(&self, codepoint: u32) -> Option<usize> {
        (1..self.num_glyphs()).find(|&gid| {
            self.glyph_name(gid).and_then(crate::type42::glyph_unicode) == Some(codepoint)
        })
    }

    /// The PostScript name of a glyph index.
    pub fn glyph_name(&self, glyph: usize) -> Option<&'a [u8]> {
        self.charstrings.glyph_name(glyph)
    }

    /// The decrypted charstring of a glyph index.
    pub fn glyph_program(&self, glyph: usize) -> Option<Vec<u8>> {
        self.charstrings.glyph_program(self.glyph_name(glyph)?)
    }

    /// Decrypted local Type 1 subroutine by index.
    pub fn subroutine_program(&self, index: usize) -> Option<Vec<u8>> {
        self.charstrings.subroutine_program(index)
    }

    /// The unscaled side bearings and advances of a glyph.
    pub fn glyph_metrics(&self, glyph: usize) -> Option<Metrics> {
        parse_metrics(&self.glyph_program(glyph)?).ok()
    }

    /// Attach AFM metrics to the face, replacing any previous AFM data.
    pub fn attach_afm(&mut self, data: &'a [u8]) -> Result<(), FaceError> {
        let afm = Afm::parse(data).map_err(|_| FaceError)?;
        self.attached = Some(AttachedMetrics::Afm(afm));
        Ok(())
    }

    /// Attach PFM metrics to the face, replacing any previous metrics data.
    pub fn attach_pfm(&mut self, data: &[u8]) -> Result<(), FaceError> {
        let pfm = Pfm::parse(data).map_err(|_| FaceError)?;
        let pairs = pfm
            .pairs()
            .map(|(left, right, value)| {
                (
                    self.nominal_glyph(left).unwrap_or(0),
                    self.nominal_glyph(right).unwrap_or(0),
                    value,
                )
            })
            .collect();
        self.attached = Some(AttachedMetrics::Pfm(pairs));
        Ok(())
    }

    /// The unscaled horizontal and vertical adjustment of a glyph pair.
    pub fn kerning(&self, left: usize, right: usize) -> (i32, i32) {
        match &self.attached {
            Some(AttachedMetrics::Afm(afm)) => {
                let Some(left) = self.glyph_name(left) else {
                    return (0, 0);
                };
                let Some(right) = self.glyph_name(right) else {
                    return (0, 0);
                };
                afm.kerning(left, right)
            }
            Some(AttachedMetrics::Pfm(pairs)) => pairs
                .iter()
                .find(|(a, b, _)| *a == left && *b == right)
                .map_or((0, 0), |(_, _, x)| (i32::from(*x), 0)),
            None => (0, 0),
        }
    }
}
