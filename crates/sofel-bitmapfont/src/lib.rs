//! BDF bitmap faces (sofel ADR 0065).
#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]
extern crate alloc;

/// PCF bitmap faces.
pub mod pcf;

/// Windows FNT and NE and PE32 FON bitmap faces.
pub mod winfnt;

/// Allocation-free readers and validators (sofel ADR 0096, ADR 0098).
pub mod noalloc;

use alloc::vec::Vec;

pub use crate::noalloc::bdf::BdfError;

/// One BDF glyph and its packed monochrome bitmap.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    /// Unicode encoding, absent for `ENCODING -1`.
    pub encoding: Option<u32>,
    /// Horizontal device advance in pixels.
    pub advance: i32,
    /// Bitmap width in pixels.
    pub width: u16,
    /// Bitmap height in pixels.
    pub height: u16,
    /// Bitmap left bearing.
    pub left: i16,
    /// Bitmap top bearing.
    pub top: i16,
    /// Packed rows, most significant sample first.
    pub bitmap: Vec<u8>,
}

impl Glyph {
    pub(crate) fn from_head(head: noalloc::GlyphHead, bitmap: Vec<u8>) -> Self {
        Self {
            encoding: head.encoding,
            advance: head.advance,
            width: head.width,
            height: head.height,
            left: head.left,
            top: head.top,
            bitmap,
        }
    }
}

/// A single-strike BDF face. Glyph zero uses `DEFAULT_CHAR` or the first glyph.
#[derive(Clone, Debug)]
pub struct BdfFont {
    x_ppem_26_6: u32,
    y_ppem_26_6: u32,
    bits_per_pixel: u8,
    glyphs: Vec<Glyph>,
    default_glyph: usize,
    encoded_count: usize,
}

impl BdfFont {
    /// True if the input begins with a BDF signature.
    pub fn is_bdf(data: &[u8]) -> bool {
        data.starts_with(b"STARTFONT ")
    }

    /// Parse a BDF 2.1 face. Malformed dimensions, rows and glyph counts fail.
    ///
    /// Allocates the glyph list and each glyph's pixels decoded from hexadecimal (ADR 0094).
    pub fn parse(data: &[u8]) -> Result<Self, BdfError> {
        Self::parse_with_offset(data).map_err(|_| BdfError)
    }

    /// Parse a BDF face, reporting the first byte of the failing line or EOF.
    ///
    /// Allocates the glyph list and each glyph's pixels decoded from hexadecimal (ADR 0094).
    pub fn parse_with_offset(data: &[u8]) -> Result<Self, usize> {
        let mut glyphs = Vec::new();
        let mut bitmap = Vec::new();
        let summary = noalloc::bdf::scan(data, &mut |event| match event {
            noalloc::Event::Pixel(byte) => bitmap.push(byte),
            noalloc::Event::Glyph(head) => {
                let bitmap = core::mem::take(&mut bitmap);
                // The BDF driver exposes a reserved empty slot for an unencoded glyph.
                glyphs.push(if head.encoding.is_some() {
                    Glyph::from_head(head, bitmap)
                } else {
                    Glyph::default()
                });
            }
        })?;
        glyphs.sort_by_key(|glyph| glyph.encoding.unwrap_or(u32::MAX));
        let encoded_count = glyphs.partition_point(|g| g.encoding.is_some());
        let default_glyph = summary
            .default_char
            .and_then(|codepoint| {
                glyphs
                    .iter()
                    .rposition(|glyph| glyph.encoding.unwrap_or(0) == codepoint)
            })
            .unwrap_or(0);
        Ok(Self {
            x_ppem_26_6: summary.x_ppem_26_6,
            y_ppem_26_6: summary.y_ppem_26_6,
            bits_per_pixel: summary.bits_per_pixel,
            glyphs,
            default_glyph,
            encoded_count,
        })
    }

    /// Validate a BDF face without allocating (sofel ADR 0096).
    pub fn validate(data: &[u8], report: &mut dyn FnMut(noalloc::Problem)) {
        noalloc::bdf::validate(data, report);
    }

    /// Number of glyph IDs, including the missing-glyph slot zero.
    pub fn num_glyphs(&self) -> usize {
        self.glyphs.len().saturating_add(1)
    }
    /// Return the BDF strike's horizontal and vertical ppem in 26.6 units.
    pub fn ppem_26_6(&self) -> (u32, u32) {
        (self.x_ppem_26_6, self.y_ppem_26_6)
    }
    /// Bits in one packed bitmap sample (1, 2, 4 or 8).
    pub fn bits_per_pixel(&self) -> u8 {
        self.bits_per_pixel
    }
    /// Map a Unicode scalar to FreeType's one-based glyph ID.
    pub fn nominal_glyph(&self, codepoint: u32) -> Option<u32> {
        let mut low = 0;
        let mut high = self.encoded_count;
        let mut mid = high / 2;
        while low < high {
            let code = self.glyphs.get(mid)?.encoding?;
            match codepoint.cmp(&code) {
                core::cmp::Ordering::Equal => {
                    // FreeType's BDF cmap stores and returns a 16-bit glyph index.
                    let id = u16::try_from(mid & 0xffff).ok()?.wrapping_add(1);
                    return (id != 0).then_some(u32::from(id));
                }
                core::cmp::Ordering::Less => high = mid,
                core::cmp::Ordering::Greater => low = mid.checked_add(1)?,
            }
            // FreeType predicts the next index from the distance between character codes.
            let step = i64::from(codepoint).saturating_sub(i64::from(code));
            mid = isize::try_from(step)
                .ok()
                .and_then(|delta| mid.checked_add_signed(delta))
                .filter(|index| (low..high).contains(index))
                .unwrap_or_else(|| low.midpoint(high));
        }
        None
    }
    /// Return a glyph by FreeType glyph ID. ID zero uses the first glyph.
    pub fn glyph(&self, id: u32) -> Option<&Glyph> {
        let index = if id == 0 {
            self.default_glyph
        } else {
            usize::try_from(id.saturating_sub(1)).ok()?
        };
        self.glyphs.get(index)
    }
}
