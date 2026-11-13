//! Character and glyph access to a bare CFF 1 face.

use sofel_read::GlyphId;
use sofel_read::cff::charstring::{self, Context, NullSink, Work};
use sofel_read::cff::ft;
use sofel_type1::type42::glyph_unicode;

use crate::{Cff, CffError};

/// A bare CFF 1 face opened without an sfnt wrapper.
#[derive(Clone, Copy, Debug)]
pub struct BareCffFace<'a> {
    cff: Cff<'a>,
}

impl<'a> BareCffFace<'a> {
    /// Open a face by its zero-based Name INDEX position.
    pub fn new(data: &'a [u8], index: u32) -> Result<Self, CffError> {
        Ok(Self {
            cff: Cff::from_bare(data, index)?,
        })
    }

    /// Number of glyphs in this face's CharStrings INDEX.
    pub fn num_glyphs(&self) -> u32 {
        self.cff.num_glyphs()
    }

    /// Glyph selected by the PostScript glyph name's Unicode value.
    pub fn nominal_glyph(&self, codepoint: u32) -> Option<GlyphId> {
        (1..self.num_glyphs())
            .map(GlyphId)
            .find(|&glyph| self.glyph_name(glyph).and_then(glyph_unicode) == Some(codepoint))
    }

    /// Glyph selected by the CFF eight-bit Encoding.
    pub fn encoded_glyph(&self, code: u8) -> Option<GlyphId> {
        let glyph = self.cff.encoding()?.glyph(code, &self.cff.charset()?);
        (glyph != 0).then_some(GlyphId(glyph))
    }

    /// PostScript glyph name at an index.
    pub fn glyph_name(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.cff.ft_glyph_name(glyph)
    }

    /// Unscaled horizontal advance decoded from the Type 2 charstring.
    pub fn h_advance(&self, glyph: GlyphId) -> Option<i32> {
        let sub = self.cff.subfont(glyph)?;
        let data = self.cff.char_strings().ft_element(glyph.0)?;
        let (global_subrs, global_subrs_sorted) = self.cff.global_subrs();
        let top = self.cff.top_dict();
        let private = &sub.private;
        let blend = self.cff.default_blend();
        let context = Context {
            version: self.cff.version(),
            char_strings: self.cff.char_strings(),
            charset: self.cff.charset(),
            global_subrs,
            global_subrs_sorted,
            local_subrs: sub.local_subrs,
            local_subrs_sorted: self.cff.local_subrs_sorted(sub.fd),
            charstring_type: top.charstring_type,
            default_width_x: private.default_width_x,
            nominal_width_x: private.nominal_width_x,
            max_stack: top.maxstack,
            has_variations: false,
            vsindex: private.vsindex,
            blend: &blend,
            random_seed: ft::low32(private.initial_random_seed.cast_unsigned()),
            scale: 0x400,
        };
        let decoded =
            charstring::decode(data, &context, &mut Work::default(), &mut NullSink).ok()?;
        let width = i64::from(decoded.width >> 16);
        let transformed = ft::mul_fix(width, sub.matrix.m[0]);
        i32::try_from(transformed.checked_add(sub.matrix.offset.0)?).ok()
    }

    /// Parsed CFF data for outline drawing and CFF queries.
    pub fn cff(&self) -> &Cff<'a> {
        &self.cff
    }
}
