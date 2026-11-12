//! The scale shaping positions glyphs at (ADR 0020, 位置): HarfBuzz's
//! `x_scale`, `y_scale`, `x_ppem` and `y_ppem` of a font.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-font.hh` and
//! `src/hb-ot-layout-common.hh` (`HintingDevice::get_delta`), MIT; see
//! THIRD_PARTY_NOTICES.md.

use sofel_font::{Font, HbScale};

/// The scale and pixel size of a font for shaping.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Scale {
    /// The scale (`x_scale`, `y_scale`) with HarfBuzz's arithmetic.
    pub scale: HbScale,
    /// The horizontal pixels per em; 0 for none, which turns Device
    /// tables off (`x_ppem`).
    pub x_ppem: u32,
    /// The vertical pixels per em; 0 for none (`y_ppem`).
    pub y_ppem: u32,
}

impl Scale {
    /// A scale with pixel sizes.
    pub fn new(scale: HbScale, x_ppem: u32, y_ppem: u32) -> Scale {
        Scale {
            scale,
            x_ppem,
            y_ppem,
        }
    }
    /// HarfBuzz's default for a font: the scale `unitsPerEm` (positions
    /// in font units) and no pixel size.
    pub fn unscaled(font: &Font<'_>) -> Scale {
        Scale::new(HbScale::unscaled(font), 0, 0)
    }
}
