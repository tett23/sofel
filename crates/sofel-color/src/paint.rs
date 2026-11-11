//! From a glyph to paint commands: HarfBuzz's `hb_ot_paint_glyph_or_fail`
//! (`src/hb-ot-font.cc`), which tries `COLR`, `SVG `, `CBDT` and `sbix`
//! in that order.

use sofel_bitmap::{cbdt_extents, cbdt_png, sbix_extents, sbix_png};
use sofel_outline::font::geom::{
    Brush, CompositeMode, GlyphExtents, Image, ImageFormat, Painter, Rect, Solid, Transform,
};
use sofel_outline::font::read::colr::{self, Colr};
use sofel_outline::font::read::svg::{self, Svg};
use sofel_outline::font::{Font, HbScale};

use crate::Scratch;
use crate::palette::{Colors, PaintOptions};
use crate::record::Recording;

/// The format [`crate::paint`] painted a glyph from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Painted {
    /// A `COLR` version 1 paint.
    ColrV1,
    /// `COLR` version 0 layers.
    ColrV0,
    /// An `SVG ` document.
    Svg,
    /// A `CBDT` PNG.
    Cbdt,
    /// An `sbix` PNG.
    Sbix,
    /// Not a color glyph: nothing was painted.
    None,
}

/// Where the commands go: a [`Painter`], and images whose data borrows
/// the font for `'a` (so that a [`Recording`] can keep them).
pub(crate) trait Sink<'a>: Painter {
    fn image_of(&mut self, image: Image<'a>) -> bool;
}

/// Any [`Painter`], passing images on as they are.
pub(crate) struct Direct<'p, P: ?Sized>(pub(crate) &'p mut P);

impl<P: Painter + ?Sized> Painter for Direct<'_, P> {
    fn push_transform(&mut self, t: Transform) {
        self.0.push_transform(t);
    }
    fn pop_transform(&mut self) {
        self.0.pop_transform();
    }
    fn push_clip_glyph(&mut self, glyph: u32) {
        self.0.push_clip_glyph(glyph);
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        self.0.push_clip_rectangle(r);
    }
    fn pop_clip(&mut self) {
        self.0.pop_clip();
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        self.0.fill(brush);
    }
    fn fill_glyph(&mut self, glyph: u32, color: &Solid) {
        self.0.fill_glyph(glyph, color);
    }
    fn image(&mut self, image: &Image<'_>) -> bool {
        self.0.image(image)
    }
    fn push_group(&mut self, mode: CompositeMode) {
        self.0.push_group(mode);
    }
    fn pop_group(&mut self, mode: CompositeMode) {
        self.0.pop_group(mode);
    }
    fn color_glyph(&mut self, glyph: u32) -> bool {
        self.0.color_glyph(glyph)
    }
}

impl<'a, P: Painter + ?Sized> Sink<'a> for Direct<'_, P> {
    fn image_of(&mut self, image: Image<'a>) -> bool {
        self.0.image(&image)
    }
}

impl<'a> Sink<'a> for Recording<'a> {
    fn image_of(&mut self, image: Image<'a>) -> bool {
        self.push_image(image);
        true
    }
}

pub(crate) fn colr<'a>(font: &Font<'a>) -> Option<Colr<'a>> {
    font.table(colr::TAG)
        .and_then(|d| Colr::new(d).ok())
        .filter(|c| c.num_base_glyphs() > 0 || c.version() != 0)
}

pub(crate) fn svg<'a>(font: &Font<'a>) -> Option<Svg<'a>> {
    font.table(svg::TAG)
        .and_then(|d| Svg::new(d).ok())
        .filter(|_| has_svg_list(font))
}

/// HarfBuzz's `SVG::has_data`: the document list offset is not 0.
fn has_svg_list(font: &Font<'_>) -> bool {
    font.table(svg::TAG)
        .and_then(|d| d.get(2..6))
        .is_some_and(|b| b.iter().any(|&x| x != 0))
}

/// `COLR`: version 1 paint first, then version 0 layers.
fn paint_colr<'a>(
    instance: sofel_outline::font::var::Instance<'_, 'a, '_, '_>,
    glyph: u32,
    colors: &Colors<'a>,
    scratch: &mut Scratch,
    spend: bool,
    sink: &mut impl Sink<'a>,
) -> Option<Painted> {
    let c = colr(instance.font())?;
    if c.version() >= 1 && c.base_glyph_paint(glyph).is_some() && scratch.v1_fits(&c) {
        return crate::v1::paint(instance, &c, glyph, colors, scratch, spend, sink)
            .then_some(Painted::ColrV1);
    }
    let record = c.find_base_glyph(glyph)?;
    if u32::from(record.glyph) != glyph {
        return None;
    }
    for (g, index) in c.glyph_layers(glyph) {
        let solid = colors.solid(index, 1.0);
        sink.fill_glyph(u32::from(g), &solid);
    }
    Some(Painted::ColrV0)
}

/// The document of `glyph` with HarfBuzz's checks; for [`crate::svg_document`]
/// and painting.
pub(crate) fn svg_doc<'a>(font: &Font<'a>, glyph: u32) -> Option<(usize, &'a [u8])> {
    let s = svg(font)?;
    let i = s.find(glyph)?;
    Some((i, s.document(i)?))
}

/// Whether `data` starts with the gzip magic number.
pub(crate) fn is_gzip(data: &[u8]) -> bool {
    data.starts_with(&[0x1f, 0x8b])
}

/// HarfBuzz's glyph box for painting bitmap glyphs
/// (`font->get_glyph_extents` at the default scale): `sbix`, then
/// `CBDT`.
fn bitmap_extents(font: &Font<'_>, glyph: u32, ppem: u16) -> Option<GlyphExtents> {
    let scale = HbScale::unscaled(font);
    sbix_extents(font, glyph, ppem, &scale).or_else(|| cbdt_extents(font, glyph, ppem, &scale))
}

/// Only the `COLR` part of [`paint_any`] (HarfBuzz's
/// `COLR::paint_glyph`, which `COLR::get_extents` uses).
pub(crate) fn paint_colr_only<'a>(
    instance: sofel_outline::font::var::Instance<'_, 'a, '_, '_>,
    glyph: u32,
    options: &PaintOptions<'a>,
    scratch: &mut Scratch,
    sink: &mut impl Sink<'a>,
) -> Option<Painted> {
    let colors = Colors::new(instance.font(), options);
    paint_colr(instance, glyph, &colors, scratch, false, sink)
}

pub(crate) fn paint_any<'a>(
    instance: sofel_outline::font::var::Instance<'_, 'a, '_, '_>,
    glyph: u32,
    options: &PaintOptions<'a>,
    scratch: &mut Scratch,
    sink: &mut impl Sink<'a>,
) -> Painted {
    let font = instance.font();
    let colors = Colors::new(font, options);
    if let Some(p) = paint_colr(instance, glyph, &colors, scratch, true, sink) {
        return p;
    }
    if let Some((_, doc)) = svg_doc(font, glyph) {
        let image = Image {
            format: ImageFormat::Svg { gzip: is_gzip(doc) },
            data: doc,
            width: 0,
            height: 0,
            extents: None,
        };
        if sink.image_of(image) {
            return Painted::Svg;
        }
    }
    let png = |p: sofel_bitmap::PngGlyph<'a>, extents: GlyphExtents| Image {
        format: ImageFormat::Png,
        data: p.data,
        width: p.pixel_extents.width.unsigned_abs(),
        height: p.pixel_extents.height.unsigned_abs(),
        extents: Some(extents),
    };
    if let Some(e) = bitmap_extents(font, glyph, options.ppem)
        && let Some(p) = cbdt_png(font, glyph, options.ppem)
        && sink.image_of(png(p, e))
    {
        return Painted::Cbdt;
    }
    if let Some(e) = bitmap_extents(font, glyph, options.ppem)
        && let Some(p) = sbix_png(font, glyph, options.ppem)
        && sink.image_of(png(p, e))
    {
        return Painted::Sbix;
    }
    Painted::None
}
