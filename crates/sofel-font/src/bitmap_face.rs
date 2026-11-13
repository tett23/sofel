//! Fixed-strike faces from BDF, PCF and Windows font resources.
use crate::{
    Font, FontError,
    read::{GlyphId, ReadError},
};
use sofel_bitmapfont::{BdfFont, Glyph, pcf::PcfFont, winfnt::WinFont};

/// A fixed bitmap format recognized by its file signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapFormat {
    /// Bitmap Distribution Format.
    Bdf,
    /// Portable Compiled Format.
    Pcf,
    /// A Windows FNT file or FON resource container.
    Windows,
}

impl BitmapFormat {
    /// Identifies a supported bitmap font signature.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if BdfFont::is_bdf(bytes) {
            Some(Self::Bdf)
        } else if PcfFont::is_pcf(bytes) {
            Some(Self::Pcf)
        } else if WinFont::is_win_font(bytes) {
            Some(Self::Windows)
        } else {
            None
        }
    }
    /// Format name for diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Self::Bdf => "BDF",
            Self::Pcf => "PCF",
            Self::Windows => "FNT/FON",
        }
    }
    /// Opens the selected face in this format.
    pub fn open(self, bytes: &[u8], index: u32) -> Result<BitmapFace, FontError> {
        let invalid = FontError::Read(ReadError::InvalidValue);
        match self {
            Self::Bdf | Self::Pcf if index != 0 => Err(FontError::IndexOutOfRange),
            Self::Bdf => BdfFont::parse(bytes)
                .map(BitmapFace::Bdf)
                .map_err(|_| invalid),
            Self::Pcf => PcfFont::parse(bytes)
                .map(BitmapFace::Pcf)
                .map_err(|_| invalid),
            Self::Windows => WinFont::parse(bytes, index)
                .map(BitmapFace::Windows)
                .map_err(|_| invalid),
        }
    }
}

impl Font<'_> {
    /// Opens a numbered BDF, PCF, FNT or FON bitmap face.
    pub fn from_bitmap(bytes: &[u8], index: u32) -> Result<BitmapFace, FontError> {
        BitmapFormat::detect(bytes)
            .ok_or(FontError::Read(ReadError::InvalidValue))?
            .open(bytes, index)
    }
}

/// Owned bitmap face with the format's native mapping and fixed strike.
#[derive(Clone, Debug)]
pub enum BitmapFace {
    /// BDF face, with one to eight coverage bits per pixel.
    Bdf(BdfFont),
    /// Monochrome PCF face.
    Pcf(PcfFont),
    /// Monochrome FNT face or selected Windows resource.
    Windows(WinFont),
}

impl BitmapFace {
    /// Number of faces in the input.
    pub fn face_count(&self) -> u32 {
        match self {
            Self::Windows(font) => font.num_faces(),
            _ => 1,
        }
    }
    /// Number of glyphs including the missing glyph.
    pub fn glyph_count(&self) -> usize {
        match self {
            Self::Bdf(f) => f.num_glyphs(),
            Self::Pcf(f) => f.num_glyphs(),
            Self::Windows(f) => f.num_glyphs(),
        }
    }
    /// Horizontal and vertical strike sizes in 26.6 pixels per em.
    pub fn ppem_26_6(&self) -> (u32, u32) {
        match self {
            Self::Bdf(f) => f.ppem_26_6(),
            Self::Pcf(f) => f.ppem_26_6(),
            Self::Windows(f) => f.ppem_26_6(),
        }
    }
    /// Number of packed coverage bits per pixel.
    pub fn bits_per_pixel(&self) -> u8 {
        match self {
            Self::Bdf(f) => f.bits_per_pixel(),
            _ => 1,
        }
    }
    /// Glyph for a character in the face's own encoding.
    pub fn nominal_glyph(&self, code: u32) -> Option<GlyphId> {
        match self {
            Self::Bdf(f) => f.nominal_glyph(code),
            Self::Pcf(f) => f.nominal_glyph(code),
            Self::Windows(f) => f.nominal_glyph(code),
        }
        .map(GlyphId)
    }
    /// Pixel metrics and packed bitmap for a glyph at its fixed strike.
    pub fn glyph(&self, id: GlyphId) -> Option<&Glyph> {
        match self {
            Self::Bdf(f) => f.glyph(id.0),
            Self::Pcf(f) => f.glyph(id.0),
            Self::Windows(f) => f.glyph(id.0),
        }
    }
}
