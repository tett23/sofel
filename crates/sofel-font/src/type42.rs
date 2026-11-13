//! The Type 42 glyph order and the embedded TrueType glyph order.

use sofel_read::GlyphId;
use sofel_type1::type42::Type42Source;

use crate::{Font, FontError};

/// One Type 42 face, with PostScript glyph indices as its public indices.
#[derive(Clone, Copy, Debug)]
pub struct Type42Face<'a> {
    source: &'a Type42Source,
    font: Font<'a>,
}

impl<'a> Type42Face<'a> {
    /// Open the embedded sfnt and retain the Type 42 glyph mapping.
    pub fn new(source: &'a Type42Source, index: u32) -> Result<Self, FontError> {
        let font = Font::new(source.sfnt(), index)?;
        Ok(Self { source, font })
    }

    /// Number of Type 42 CharStrings entries.
    pub fn num_glyphs(&self) -> u32 {
        u32::try_from(self.source.num_glyphs()).unwrap_or(u32::MAX)
    }

    /// Units per em from the embedded TrueType font.
    pub fn units_per_em(&self) -> u16 {
        self.font.units_per_em()
    }

    /// Type 42 glyph from the PostScript glyph name's Unicode value.
    pub fn nominal_glyph(&self, codepoint: u32) -> Option<GlyphId> {
        self.source.unicode_glyph(codepoint).map(GlyphId)
    }

    /// Type 42 glyph from its eight-bit PostScript Encoding.
    pub fn encoded_glyph(&self, code: u8) -> Option<GlyphId> {
        self.source.encoded_glyph(code).map(GlyphId)
    }

    /// PostScript glyph name at a Type 42 glyph index.
    pub fn glyph_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.source.glyph_name(usize::try_from(glyph.0).ok()?)
    }

    /// Translate a Type 42 glyph index to an embedded TrueType glyph index.
    pub fn embedded_glyph(&self, glyph: GlyphId) -> Option<GlyphId> {
        self.source
            .embedded_glyph(usize::try_from(glyph.0).ok()?)
            .filter(|index| *index < self.font.num_glyphs())
            .map(GlyphId)
    }

    /// Unscaled horizontal advance of a Type 42 glyph.
    pub fn h_advance(&self, glyph: GlyphId) -> Option<u32> {
        Some(
            self.font
                .default_instance()
                .advance(self.embedded_glyph(glyph)?),
        )
    }

    /// Embedded TrueType face for outline and table queries with
    /// [`Type42Face::embedded_glyph`].
    pub fn inner_font(&self) -> Font<'a> {
        self.font
    }
}
