//! Rendering one glyph the way FreeType does (ADR 0012): load the
//! outline without hinting, find the bitmap box from its control box,
//! move the outline to the bitmap origin (and multiply it by 4 when it
//! has the overlap flag) before decomposing it, and rasterize. Color
//! glyphs follow the COLR version 0 layer records (ADR 0023).

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
    outlines: &Outlines<'_, '_, '_>,
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

/// Render an unhinted TrueType glyph to packed monochrome rows. The
/// outline is translated and scaled before implicit on-curve points are
/// computed, as in FreeType's black rasterizer.
///
/// # Errors
/// Returns an error on an invalid outline or unsupported bitmap dimensions.
pub fn render_mono<'r>(
    outlines: &Outlines<'_, '_, '_>,
    glyph: GlyphId,
    ppem: u16,
    scratch: &mut Scratch,
    raster: &'r mut Rasterizer,
) -> Result<sofel_raster::MonoBitmap<'r>, RenderError> {
    let info = outlines
        .load(glyph, Scale::pixels(outlines.font(), ppem, ppem), scratch)
        .map_err(RenderError::Outline)?;
    let c = info.cbox;
    let bounds = sofel_raster::mono_pixel_box(c.x_min, c.y_min, c.x_max, c.y_max);
    let high_precision = ppem < 24 && bounds.width.saturating_add(bounds.rows) < 258;
    let multiplier = if high_precision { 64 } else { 1 };
    let shift = bounds.shift();
    let offset = sofel_font::geom::Point {
        x: shift.x.saturating_sub(32),
        y: shift.y.saturating_sub(32),
    };
    raster.clear();
    scratch
        .decompose(offset, multiplier, raster)
        .map_err(RenderError::Outline)?;
    let options = sofel_raster::MonoOptions {
        high_precision,
        ..Default::default()
    };
    raster
        .render_mono_placed(bounds, options, Some((ppem, ppem)))
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

/// The usable prefix of `glyph`'s COLR version 0 layer records.
/// A range extending past the layer array is rejected as a whole;
/// invalid glyph or palette indices end the prefix. `None` is the
/// foreground color.
pub fn color_layers(font: &sofel_font::Font<'_>, glyph: u32) -> Vec<(u32, Option<u16>)> {
    use sofel_read::colr::{Colr, FOREGROUND, TAG};
    let Some(colr) = font.table(TAG).and_then(|data| Colr::new(data).ok()) else {
        return Vec::new();
    };
    let Some(base) = colr.find_base_glyph(glyph) else {
        return Vec::new();
    };
    let start = usize::from(base.first_layer);
    let Some(end) = start.checked_add(usize::from(base.num_layers)) else {
        return Vec::new();
    };
    if end > colr.num_layers() {
        return Vec::new();
    }
    let max_palette = sofel_color::palette_entry_count(font);
    (start..end)
        .map_while(|index| {
            let (layer_glyph, entry) = colr.layer(index)?;
            (u32::from(layer_glyph) < font.num_glyphs()
                && (entry == FOREGROUND || entry < max_palette))
                .then_some((
                    u32::from(layer_glyph),
                    (entry != FOREGROUND).then_some(entry),
                ))
        })
        .collect()
}

/// Renders the usable COLR version 0 layers of `glyph` at `ppem` pixels
/// (ADR 0023). Each layer is rendered with [`render`] and combined in
/// record order with [`blend_layer`]. `options.ppem` is not used.
///
/// `image` is cleared first. When the glyph has no layers, or a layer
/// cannot be rendered, the glyph's own outline is rendered in gray
/// instead.
///
/// # Errors
///
/// As [`render`], when the glyph's own outline cannot be loaded or (in
/// the gray fallback) rendered.
pub fn render_color<'r>(
    outlines: &Outlines<'_, '_, '_>,
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
    if layers.is_empty() {
        return render(outlines, glyph, ppem, scratch, raster).map(ColorGlyph::Gray);
    }
    let palette: Vec<Color> = match options.palette {
        Palette::Font(index) => {
            let selected = if index < sofel_color::palette_count(font) {
                index
            } else {
                0
            };
            sofel_color::palette_colors(font, selected).collect()
        }
        Palette::Custom(colors) => colors.to_vec(),
    };
    for (layer_glyph, entry) in layers {
        let color = match entry {
            None => options.foreground,
            Some(index) => palette
                .get(usize::from(index))
                .copied()
                .unwrap_or(Color::new(0, 0, 0, 0)),
        };
        match render(outlines, GlyphId(layer_glyph), ppem, scratch, raster) {
            Ok(bitmap) => blend_layer(image, &bitmap, [color.b, color.g, color.r, color.a]),
            Err(_) => {
                image.clear();
                return render(outlines, glyph, ppem, scratch, raster).map(ColorGlyph::Gray);
            }
        }
    }
    Ok(ColorGlyph::Color)
}
