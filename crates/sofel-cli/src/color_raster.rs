//! Connects OT-SVG image paint commands to the shared raster surface (sofel ADR 0072).

use sofel_font::geom::{
    Brush, CompositeMode, Image, ImageFormat, Painter, PointF, Rect, Transform,
};
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
    render_shapes(ppem, upem, raster, image, |p| scene.paint(p))
}

/// Renders a selected SVG glyph, intersecting every shape with its clip stack.
pub fn render_glyph_scene(
    glyph: &sofel_svg::GlyphScene,
    ppem: u16,
    upem: u16,
    raster: &mut Rasterizer,
    image: &mut BgraImage,
) -> bool {
    render_shapes(ppem, upem, raster, image, |p| glyph.paint(p))
}

struct RasterScenePainter<'a, 's> {
    raster: &'a mut Rasterizer,
    image: &'a mut BgraImage,
    ppem: u16,
    scale: f32,
    outline: Option<(&'s sofel_svg::Shape, &'s [sofel_svg::ClipPath])>,
    clip_raster: Option<Rasterizer>,
    transforms: Vec<Transform>,
    valid: bool,
}

fn render_shapes<'s>(
    ppem: u16,
    upem: u16,
    raster: &mut Rasterizer,
    image: &mut BgraImage,
    emit: impl FnOnce(&mut RasterScenePainter<'_, 's>) -> bool,
) -> bool {
    if ppem == 0 || upem == 0 {
        return false;
    }
    let mut p = RasterScenePainter {
        raster,
        image,
        ppem,
        scale: f32::from(ppem) / f32::from(upem),
        outline: None,
        clip_raster: None,
        transforms: Vec::new(),
        valid: true,
    };
    emit(&mut p) && p.valid && p.image.width > 0 && p.image.rows > 0
}

impl<'s> sofel_svg::OutlinePainter<'s> for RasterScenePainter<'_, 's> {
    fn push_svg_clip(&mut self, shape: &'s sofel_svg::Shape, paths: &'s [sofel_svg::ClipPath]) {
        self.outline = Some((shape, paths));
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn brush_point(transforms: &[Transform], mut p: PointF) -> Option<PointF> {
    for t in transforms {
        let x = p.x - t.dx;
        let y = p.y - t.dy;
        let det = t.xx * t.yy - t.xy * t.yx;
        if det == 0. {
            return None;
        }
        p = if t.xy == 0. && t.yx == 0. {
            PointF::new(x / t.xx, y / t.yy)
        } else {
            PointF::new((t.yy * x - t.xy * y) / det, (t.xx * y - t.yx * x) / det)
        };
    }
    Some(p)
}

impl RasterScenePainter<'_, '_> {
    fn draw_fill(&mut self, brush: &Brush<'_>) -> bool {
        let Some((shape, paths)) = self.outline else {
            return false;
        };
        let scale = self.scale;
        let ppem = self.ppem;
        self.raster.clear();
        shape.draw(scale, self.raster);
        let Ok(bitmap) = self.raster.render(RenderOptions {
            fill: FillRule::NonZero,
            overlap: false,
            ppem: Some((ppem, ppem)),
        }) else {
            return false;
        };
        let pixels;
        let bitmap = if paths.is_empty() {
            bitmap
        } else {
            let Ok(mask) = clip::coverage(
                &bitmap,
                paths,
                self.clip_raster.get_or_insert_with(Rasterizer::new),
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
        blend_sampled(self.image, &bitmap, |x, y| {
            brush_point(&self.transforms, PointF::new(x / scale, y / scale))
                .map_or([0; 4], |point| sofel_svg::sample_brush(brush, point))
        });
        true
    }
}

impl Painter for RasterScenePainter<'_, '_> {
    fn push_transform(&mut self, t: Transform) {
        self.transforms.push(t);
    }
    fn pop_transform(&mut self) {
        self.valid &= self.transforms.pop().is_some();
    }
    fn push_clip_glyph(&mut self, _: u32) {
        self.valid = false;
    }
    fn push_clip_rectangle(&mut self, _: Rect) {
        self.valid = false;
    }
    fn pop_clip(&mut self) {
        self.valid &= self.outline.take().is_some();
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        self.valid &= self.draw_fill(brush);
    }
    fn image(&mut self, _: &Image<'_>) -> bool {
        false
    }
    fn push_group(&mut self, _: CompositeMode) {
        self.valid = false;
    }
    fn pop_group(&mut self, _: CompositeMode) {
        self.valid = false;
    }
}
