//! Connects OT-SVG image paint commands to the shared raster surface (sofel ADR 0072).

use sofel_font::geom::{Brush, CompositeMode, Image, ImageFormat, Painter, Rect, Transform};
use sofel_raster::{BgraImage, FillRule, Rasterizer, RenderOptions, blend_sampled};
use sofel_svg::{Scene, parse_glyph};

/// A painter that accepts the OT-SVG image for one glyph.
pub struct SvgImagePainter<'a> {
    glyph: u32,
    foreground: [u8; 4],
    ppem: u16,
    upem: u16,
    raster: &'a mut Rasterizer,
    image: &'a mut BgraImage,
}

impl<'a> SvgImagePainter<'a> {
    /// Creates a painter for one glyph and target image.
    pub fn new(
        glyph: u32,
        ppem: u16,
        upem: u16,
        raster: &'a mut Rasterizer,
        image: &'a mut BgraImage,
    ) -> Self {
        Self {
            glyph,
            foreground: [0, 0, 0, 255],
            ppem,
            upem,
            raster,
            image,
        }
    }
}

impl SvgImagePainter<'_> {
    /// Set the foreground supplied by the font's drawing context.
    pub fn set_foreground(&mut self, color: sofel_font::geom::Color) {
        self.foreground = [color.r, color.g, color.b, color.a];
    }
}

impl Painter for SvgImagePainter<'_> {
    fn push_transform(&mut self, _t: Transform) {}
    fn pop_transform(&mut self) {}
    fn push_clip_glyph(&mut self, _glyph: u32) {}
    fn push_clip_rectangle(&mut self, _r: Rect) {}
    fn pop_clip(&mut self) {}
    fn fill(&mut self, _brush: &Brush<'_>) {}
    fn image(&mut self, image: &Image<'_>) -> bool {
        if image.format != (ImageFormat::Svg { gzip: false }) {
            return false;
        }
        let Ok(scene) = parse_glyph(image.data, self.glyph, self.upem, self.foreground) else {
            return false;
        };
        render_glyph_scene(&scene, self.ppem, self.upem, self.raster, self.image)
    }
    fn push_group(&mut self, _mode: CompositeMode) {}
    fn pop_group(&mut self, _mode: CompositeMode) {}
}

mod clip;

/// Sends SVG paths to the shared rasterizer and samples their gradients.
pub fn render_scene(
    scene: &Scene,
    ppem: u16,
    upem: u16,
    raster: &mut Rasterizer,
    image: &mut BgraImage,
) -> bool {
    render_shapes(scene, None, ppem, upem, raster, image)
}

/// Renders a selected SVG glyph, intersecting every shape with its clip stack.
pub fn render_glyph_scene(
    glyph: &sofel_svg::GlyphScene,
    ppem: u16,
    upem: u16,
    raster: &mut Rasterizer,
    image: &mut BgraImage,
) -> bool {
    if glyph.clips.len() != glyph.scene.shapes.len() {
        return false;
    }
    render_shapes(&glyph.scene, Some(&glyph.clips), ppem, upem, raster, image)
}

fn render_shapes(
    scene: &Scene,
    clips: Option<&[Vec<sofel_svg::ClipPath>]>,
    ppem: u16,
    upem: u16,
    raster: &mut Rasterizer,
    image: &mut BgraImage,
) -> bool {
    if ppem == 0 || upem == 0 || scene.shapes.is_empty() {
        return false;
    }
    let scale = f32::from(ppem) / f32::from(upem);
    let mut clip_raster = None;
    for (index, shape) in scene.shapes.iter().enumerate() {
        raster.clear();
        shape.draw(scale, raster);
        let Ok(bitmap) = raster.render(RenderOptions {
            fill: FillRule::NonZero,
            overlap: false,
            ppem: Some((ppem, ppem)),
        }) else {
            return false;
        };
        let paths = clips
            .and_then(|stacks| stacks.get(index))
            .map_or(&[][..], Vec::as_slice);
        let pixels;
        let bitmap = if paths.is_empty() {
            bitmap
        } else {
            let Ok(mask) = clip::coverage(
                &bitmap,
                paths,
                clip_raster.get_or_insert_with(Rasterizer::new),
                scale,
                ppem,
            ) else {
                return false;
            };
            pixels = mask;
            sofel_raster::Bitmap {
                pixels: &pixels,
                ..bitmap
            }
        };
        blend_sampled(image, &bitmap, |x, y| scene.sample(shape, x, y, scale));
    }
    image.width > 0 && image.rows > 0
}
