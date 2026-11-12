//! Connects OT-SVG image paint commands to the shared raster surface (sofel ADR 0072).

use sofel_font::geom::{Brush, CompositeMode, Image, ImageFormat, Painter, Rect, Transform};
use sofel_raster::{BgraImage, FillRule, Rasterizer, RenderOptions, blend_sampled};
use sofel_svg::{Scene, parse_shapes};

/// A painter that accepts the OT-SVG image for one glyph.
pub struct SvgImagePainter<'a> {
    glyph: u32,
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
            ppem,
            upem,
            raster,
            image,
        }
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
        let Ok(scene) = parse_shapes(image.data, self.glyph) else {
            return false;
        };
        render_scene(&scene, self.ppem, self.upem, self.raster, self.image)
    }
    fn push_group(&mut self, _mode: CompositeMode) {}
    fn pop_group(&mut self, _mode: CompositeMode) {}
}

/// Sends SVG paths to the shared rasterizer and samples their gradients.
pub fn render_scene(
    scene: &Scene,
    ppem: u16,
    upem: u16,
    raster: &mut Rasterizer,
    image: &mut BgraImage,
) -> bool {
    if ppem == 0 || upem == 0 || scene.shapes.is_empty() {
        return false;
    }
    let scale = f32::from(ppem) / f32::from(upem);
    for shape in &scene.shapes {
        raster.clear();
        shape.draw(scale, raster);
        let Ok(bitmap) = raster.render(RenderOptions {
            fill: FillRule::NonZero,
            overlap: false,
            ppem: Some((ppem, ppem)),
        }) else {
            return false;
        };
        blend_sampled(image, &bitmap, |x, y| scene.sample(shape, x, y, scale));
    }
    image.width > 0 && image.rows > 0
}
