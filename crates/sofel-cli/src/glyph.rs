//! Rendering one glyph the way FreeType does (ADR 0012): load the
//! outline without hinting, find the bitmap box from its control box,
//! move the outline to the bitmap origin (and multiply it by 4 when it
//! has the overlap flag) before decomposing it, and rasterize. Color
//! glyphs blend their `COLR` version 0 layers as FreeType's
//! `FT_LOAD_COLOR` does (ADR 0023).

use std::fmt;

use sofel_color::{PaintOptions, Palette};
use sofel_font::geom::Color;
use sofel_font::read::GlyphId;
use sofel_outline::{OutlineError, Outlines, Scale, Scratch};
use sofel_raster::{
    BgraImage, Bitmap, FillRule, RasterError, Rasterizer, RenderOptions, blend_layer, pixel_box,
};

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

/// What [`render_color`] drew.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorGlyph<'r> {
    /// The glyph's `COLR` version 0 layers, blended into the caller's
    /// [`BgraImage`].
    Color,
    /// The glyph has no layers, or a layer could not be drawn: the gray
    /// bitmap of the glyph's own outline, as FreeType then falls back
    /// to.
    Gray(Bitmap<'r>),
}

/// The `COLR` version 0 layers of `glyph` (each layer's glyph and palette
/// entry) that FreeType's `FT_Get_Color_Glyph_Layer` gives and
/// [`render_color`] draws: none when the
/// base glyph's record reaches past the layer records, and up to the
/// first layer whose glyph is past the font's glyphs or whose palette
/// entry (other than the foreground, `None`) is past the palette
/// entries.
pub fn color_layers(font: &sofel_font::Font<'_>, glyph: u32) -> Vec<(u32, Option<u16>)> {
    use sofel_read::colr::{Colr, TAG};
    let fits = font
        .table(TAG)
        .and_then(|d| Colr::new(d).ok())
        .and_then(|c| {
            let r = c.find_base_glyph(glyph)?;
            let end = usize::from(r.first_layer).saturating_add(usize::from(r.num_layers));
            Some(r.num_layers > 0 && end <= c.num_layers())
        })
        .unwrap_or(false);
    if !fits {
        return Vec::new();
    }
    let entries = sofel_color::palette_entry_count(font);
    sofel_color::layers(font, glyph)
        .take_while(|&(g, c)| g < font.num_glyphs() && c.is_none_or(|c| c < entries))
        .collect()
}

/// Renders `glyph` at `ppem` pixels in color as FreeType's
/// `FT_Load_Glyph` with `FT_LOAD_COLOR | FT_LOAD_NO_HINTING |
/// FT_LOAD_NO_BITMAP` and `FT_Render_Glyph` with `FT_RENDER_MODE_NORMAL`
/// do (ADR 0023): each `COLR` version 0 layer ([`color_layers`]) is rendered like
/// [`render`] and blended with [`blend_layer`] into `image` in the color
/// of its palette entry from `options` (`options.foreground` for entry
/// 0xFFFF; `options.ppem` is not used).
///
/// `image` is cleared first. When the glyph has no layers, or a layer
/// cannot be rendered, the glyph's own outline is rendered in gray
/// instead, as FreeType does.
///
/// # Errors
///
/// As [`render`], when the glyph's own outline cannot be loaded or (in
/// the gray fallback) rendered.
pub fn render_color<'r>(
    outlines: &Outlines<'_>,
    glyph: GlyphId,
    ppem: u16,
    options: &PaintOptions<'_>,
    scratch: &mut Scratch,
    raster: &'r mut Rasterizer,
    image: &mut BgraImage,
) -> Result<ColorGlyph<'r>, RenderError> {
    let font = outlines.font();
    image.clear();
    outlines
        .load(glyph, Scale::pixels(font, ppem, ppem), scratch)
        .map_err(RenderError::Outline)?;
    let layers = color_layers(font, glyph.0);
    if !layers.is_empty() {
        let palette: Vec<Color> = match options.palette {
            Palette::Font(i) => {
                let i = if i < sofel_color::palette_count(font) {
                    i
                } else {
                    0
                };
                sofel_color::palette_colors(font, i).collect()
            }
            Palette::Custom(c) => c.to_vec(),
        };
        let all = layers.iter().all(|&(g, c)| {
            let color = c.map_or(options.foreground, |c| {
                palette
                    .get(usize::from(c))
                    .copied()
                    .unwrap_or(Color::new(0, 0, 0, 0))
            });
            match render(outlines, GlyphId(g), ppem, scratch, raster) {
                Ok(b) => {
                    blend_layer(image, &b, [color.b, color.g, color.r, color.a]);
                    true
                }
                Err(_) => false,
            }
        });
        if all {
            return Ok(ColorGlyph::Color);
        }
        image.clear();
    }
    render(outlines, glyph, ppem, scratch, raster).map(ColorGlyph::Gray)
}
