//! The scale shaping positions glyphs at (ADR 0020, 位置): HarfBuzz's
//! `x_scale`, `y_scale`, `x_ppem` and `y_ppem` of a font.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-font.hh` and
//! `src/hb-ot-layout-common.hh` (`HintingDevice::get_delta`), MIT; see
//! THIRD_PARTY_NOTICES.md.

use sofel_font::read::device::Device;
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

    /// `get_x_delta` of a Device table: `pixels × x_scale / x_ppem`, 0
    /// without a ppem.
    pub(crate) fn x_delta(&self, device: Option<&Device<'_>>) -> i32 {
        delta(device, self.x_ppem, self.scale.x_scale())
    }
    /// `get_y_delta` of a Device table.
    pub(crate) fn y_delta(&self, device: Option<&Device<'_>>) -> i32 {
        delta(device, self.y_ppem, self.scale.y_scale())
    }
}

/// `HintingDevice::get_delta`.
fn delta(device: Option<&Device<'_>>, ppem: u32, scale: i32) -> i32 {
    let Some(device) = device else { return 0 };
    if ppem == 0 {
        return 0;
    }
    let pixels = device.delta_pixels(ppem);
    if pixels == 0 {
        return 0;
    }
    let v = i64::from(pixels)
        .wrapping_mul(i64::from(scale))
        .checked_div(i64::from(ppem))
        .unwrap_or(0);
    let b = v.to_le_bytes();
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
