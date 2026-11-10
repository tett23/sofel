//! Rendering one glyph the way FreeType does (ADR 0012): load the
//! outline without hinting, find the bitmap box from its control box,
//! move the outline to the bitmap origin (and multiply it by 4 when it
//! has the overlap flag) before decomposing it, and rasterize.

use std::fmt;

use sofel_font::read::GlyphId;
use sofel_outline::{OutlineError, Outlines, Scale, Scratch};
use sofel_raster::{Bitmap, FillRule, RasterError, Rasterizer, RenderOptions, pixel_box};

/// Why [`render`] failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderError {
    /// The outline could not be loaded or decomposed.
    Outline(OutlineError),
    /// The bitmap could not be rendered.
    Raster(RasterError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Outline(e) => write!(f, "{e}"),
            RenderError::Raster(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// Renders `glyph` at `ppem` pixels as `FT_Set_Pixel_Sizes`,
/// `FT_Load_Glyph` with `FT_LOAD_NO_HINTING` and `FT_Render_Glyph` with
/// `FT_RENDER_MODE_NORMAL` would, with the non-zero fill rule.
///
/// `scratch` and `raster` are caches the caller keeps and reuses across
/// glyphs; the outline previously recorded in `raster` is cleared. The
/// returned [`Bitmap`] borrows its pixels from `raster`. An empty glyph
/// gives a bitmap with no pixels.
///
/// # Errors
///
/// [`RenderError::Outline`] if the glyph's outline cannot be loaded, and
/// [`RenderError::Raster`] if the bitmap would be too large, including
/// wider or taller than 16 times `ppem` as in FreeType.
///
/// `outlines` is the font's [`Outlines`], which the caller makes once per
/// font.
pub fn render<'r>(
    outlines: &Outlines<'_>,
    glyph: GlyphId,
    ppem: u16,
    scratch: &mut Scratch,
    raster: &'r mut Rasterizer,
) -> Result<Bitmap<'r>, RenderError> {
    let font = outlines.font();
    let info = outlines
        .load(glyph, Scale::pixels(font, ppem, ppem), scratch)
        .map_err(RenderError::Outline)?;
    let c = info.cbox;
    let b = pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
    raster.clear();
    let multiplier = if info.overlap { 4 } else { 1 };
    scratch
        .decompose(b.shift(), multiplier, raster)
        .map_err(RenderError::Outline)?;
    let options = RenderOptions {
        fill: FillRule::NonZero,
        overlap: info.overlap,
        ppem: Some((ppem, ppem)),
    };
    raster
        .render_placed(b, options)
        .map_err(RenderError::Raster)
}
