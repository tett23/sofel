//! Palettes (`CPAL`) as HarfBuzz's `hb_ot_color_palette_*` and FreeType's
//! `FT_Palette_Data_Get` give them, and the colors of the paint.

use sofel_outline::font::Font;
use sofel_outline::font::geom::float::{hb_clamp_to_i32, hb_roundf32};
use sofel_outline::font::geom::{Color, ColorRef, Solid};
use sofel_outline::font::read::colr::FOREGROUND;
use sofel_outline::font::read::cpal::{self, Cpal, NO_NAME};

/// The palette to paint with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Palette<'a> {
    /// The font's palette `i`; past the palettes, palette 0 (as
    /// HarfBuzz).
    Font(u16),
    /// The caller's colors, for example a font palette with some entries
    /// replaced (CSS `override-colors`). Entries past it are transparent
    /// black.
    Custom(&'a [Color]),
}

impl Default for Palette<'_> {
    fn default() -> Self {
        Palette::Font(0)
    }
}

/// The choices of [`crate::paint`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PaintOptions<'a> {
    /// The palette.
    pub palette: Palette<'a>,
    /// The foreground color (palette index 0xFFFF in `COLR`).
    pub foreground: Color,
    /// The pixels per em for choosing a bitmap strike; 0 for the largest.
    pub ppem: u16,
}

impl Default for PaintOptions<'_> {
    fn default() -> Self {
        PaintOptions {
            palette: Palette::Font(0),
            foreground: Color::new(0, 0, 0, 255),
            ppem: 0,
        }
    }
}

pub(crate) fn cpal<'a>(font: &Font<'a>) -> Option<Cpal<'a>> {
    font.table(cpal::TAG).and_then(|d| Cpal::new(d).ok())
}

/// The number of palettes (`hb_ot_color_palette_get_count`); 0 without a
/// readable `CPAL`.
pub fn palette_count(font: &Font<'_>) -> u16 {
    cpal(font).map_or(0, |c| c.num_palettes())
}

/// The name id of palette `i` (`hb_ot_color_palette_get_name_id`);
/// 0xFFFF for none.
pub fn palette_name_id(font: &Font<'_>, i: u16) -> u16 {
    cpal(font).map_or(NO_NAME, |c| c.palette_name_id(i))
}

/// The flags of palette `i` (`hb_ot_color_palette_get_flags`): bit 0 for
/// light backgrounds, bit 1 for dark ones.
pub fn palette_flags(font: &Font<'_>, i: u16) -> u32 {
    cpal(font).map_or(0, |c| c.palette_flags(i))
}

/// The name id of palette entry `c`
/// (`hb_ot_color_palette_color_get_name_id`); 0xFFFF for none.
pub fn palette_color_name_id(font: &Font<'_>, c: u16) -> u16 {
    cpal(font).map_or(NO_NAME, |p| p.color_name_id(c))
}

fn color_of(bgra: [u8; 4]) -> Color {
    let [b, g, r, a] = bgra;
    Color::new(r, g, b, a)
}

/// The colors of palette `i` (`hb_ot_color_palette_get_colors`): its
/// entries that are in the color records; none past the palettes.
pub fn palette_colors<'a>(font: &Font<'a>, i: u16) -> impl Iterator<Item = Color> + 'a {
    let c = cpal(font);
    let n = c.map_or(0, |c| c.palette_len(i));
    (0..n).filter_map(move |e| c.and_then(|c| c.color(i, e)).map(color_of))
}

/// The number of palette entries (`numPaletteEntries`).
pub fn palette_entry_count(font: &Font<'_>) -> u16 {
    cpal(font).map_or(0, |c| c.num_palette_entries())
}

/// The first palette whose flags include all of `flags`, for CSS's
/// `font-palette: light` (`flags` 1) and `dark` (2).
pub fn find_palette(font: &Font<'_>, flags: u32) -> Option<u16> {
    let c = cpal(font)?;
    (0..c.num_palettes()).find(|&p| flags != 0 && c.palette_flags(p) & flags == flags)
}

/// The colors of a paint: the chosen palette and the foreground.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Colors<'a> {
    cpal: Option<Cpal<'a>>,
    font_palette: u16,
    custom: Option<&'a [Color]>,
    foreground: Color,
}

/// HarfBuzz's `roundf` (ADR 0026) of a value in 0..=255, converted to
/// a byte.
fn round_byte(v: f32) -> u8 {
    u8::try_from(hb_clamp_to_i32(hb_roundf32(v)).clamp(0, 255)).unwrap_or(0)
}

impl<'a> Colors<'a> {
    pub(crate) fn new(font: &Font<'a>, options: &PaintOptions<'a>) -> Colors<'a> {
        let cpal = cpal(font);
        let (font_palette, custom) = match options.palette {
            Palette::Font(i) => {
                let n = cpal.map_or(0, |c| c.num_palettes());
                (if i < n { i } else { 0 }, None)
            }
            Palette::Custom(c) => (0, Some(c)),
        };
        Colors {
            cpal,
            font_palette,
            custom,
            foreground: options.foreground,
        }
    }

    /// The color of palette entry `index` (0xFFFF: the foreground) with
    /// the font's `alpha`, as HarfBuzz's `hb_paint_context_t::get_color`.
    pub(crate) fn solid(&self, index: u16, alpha: f32) -> Solid {
        let base = if index == FOREGROUND {
            self.foreground
        } else if let Some(c) = self.custom {
            c.get(usize::from(index)).copied().unwrap_or_default()
        } else {
            self.cpal
                .and_then(|c| c.color(self.font_palette, index))
                .map_or(Color::default(), color_of)
        };
        let alpha = if alpha > 1.0 {
            1.0
        } else if alpha >= 0.0 {
            alpha
        } else {
            // Below zero and NaN (HarfBuzz's hb_clamp keeps NaN, and
            // roundf of NaN converts to 0).
            0.0
        };
        Solid {
            color: Color {
                a: round_byte(f32::from(base.a) * alpha),
                ..base
            },
            source: ColorRef {
                index: (index != FOREGROUND).then_some(index),
                alpha,
            },
        }
    }
}
