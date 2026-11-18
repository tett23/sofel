//! Bounds-checked Windows FNT and NE and PE32 FON bitmap faces (sofel ADR 0065).
use alloc::vec::Vec;

use crate::Glyph;

use crate::noalloc::Event;
pub use crate::noalloc::winfnt::WinFontError;
use crate::noalloc::winfnt::{is_win_font, scan};

/// A single strike from a raw FNT or an NE or PE32 FON resource.
#[derive(Clone, Debug)]
pub struct WinFont {
    glyphs: Vec<Glyph>,
    num_faces: u32,
    first_char: u8,
    last_char: u8,
    default_glyph: usize,
    x_ppem_26_6: u32,
    y_ppem_26_6: u32,
}

impl WinFont {
    /// Whether the input has a raw FNT or MZ-wrapped FON signature.
    pub fn is_win_font(data: &[u8]) -> bool {
        is_win_font(data)
    }

    /// Parse `face` of a raw FNT or an NE or PE32 FON file.
    ///
    /// Allocates the glyph list and each glyph's pixels reordered into rows (ADR 0094).
    pub fn parse(data: &[u8], face: u32) -> Result<Self, WinFontError> {
        Self::parse_with_offset(data, face).map_err(|_| WinFontError)
    }

    /// Read the face count independently of the validity of the first strike.
    pub fn face_count(file: &[u8]) -> Result<u32, WinFontError> {
        crate::noalloc::winfnt::face_count(file)
    }

    /// Validate every face without allocating (sofel ADR 0096).
    pub fn validate(file: &[u8], report: &mut dyn FnMut(crate::noalloc::Problem)) {
        crate::noalloc::winfnt::validate(file, report);
    }

    /// Parse a face and report the file offset of an invalid FNT field.
    ///
    /// Allocates the glyph list and each glyph's pixels reordered into rows (ADR 0094).
    pub fn parse_with_offset(file: &[u8], face: u32) -> Result<Self, usize> {
        let mut glyphs = Vec::new();
        let mut bitmap = Vec::new();
        let summary = scan(file, face, &mut |event| match event {
            Event::Pixel(byte) => bitmap.push(byte),
            Event::Glyph(head) => glyphs.push(Glyph::from_head(head, core::mem::take(&mut bitmap))),
        })?;
        Ok(Self {
            glyphs,
            num_faces: summary.num_faces,
            first_char: summary.first_char,
            last_char: summary.last_char,
            default_glyph: summary.default_glyph,
            x_ppem_26_6: summary.x_ppem_26_6,
            y_ppem_26_6: summary.y_ppem_26_6,
        })
    }

    /// Number of faces in the raw FNT or NE and PE32 FON container.
    pub fn num_faces(&self) -> u32 {
        self.num_faces
    }
    /// Number of glyph IDs, including the default-character slot zero.
    pub fn num_glyphs(&self) -> usize {
        self.glyphs.len().saturating_add(1)
    }
    /// Strike dimensions in 26.6 pixels.
    pub fn ppem_26_6(&self) -> (u32, u32) {
        (self.x_ppem_26_6, self.y_ppem_26_6)
    }
    /// Map the FNT character range to one-based glyph IDs.
    pub fn nominal_glyph(&self, codepoint: u32) -> Option<u32> {
        if codepoint < u32::from(self.first_char) || codepoint > u32::from(self.last_char) {
            return None;
        }
        codepoint
            .checked_sub(u32::from(self.first_char))?
            .checked_add(1)
    }
    /// Return a glyph; zero uses the default character.
    pub fn glyph(&self, id: u32) -> Option<&Glyph> {
        self.glyphs.get(if id == 0 {
            self.default_glyph
        } else {
            usize::try_from(id.checked_sub(1)?).ok()?
        })
    }
}
