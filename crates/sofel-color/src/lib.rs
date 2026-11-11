//! Color glyphs through the [`Painter`] (ADR 0023): `CPAL` palettes,
//! `COLR` layers and paints, `SVG ` documents and the PNG glyphs of
//! `CBDT` and `sbix`, as HarfBuzz 15.1.0 paints them; the glyph boxes
//! HarfBuzz gives for them; and the queries that font fallback needs.
//!
//! Ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372): `src/hb-ot-font.cc`
//! (`hb_ot_paint_glyph_or_fail`, `hb_ot_get_glyph_extents`),
//! `src/OT/Color/COLR/COLR.hh`, `src/OT/Color/CPAL/CPAL.hh`,
//! `src/OT/Color/svg/svg.hh`, `src/hb-ot-color.cc`,
//! `src/hb-paint-extents.hh` and `src/hb-paint-bounded.hh`. Copyright ©
//! the HarfBuzz authors. Used under HarfBuzz's "Old MIT" license; see
//! THIRD_PARTY_NOTICES.md.
//!
//! The commands are in font units with y growing upwards, without
//! HarfBuzz's root scale: the caller puts its scale and position in its
//! own transform. Numbers are `f32`, computed in HarfBuzz's order.
//!
//! # Example
//!
//! Record the commands of a glyph and look at them:
//!
//! ```no_run
//! use sofel_color::{PaintOptions, Painted, Recording, Scratch, paint_recording};
//! use sofel_outline::font::Font;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let data = std::fs::read("emoji.ttf")?;
//!     let font = Font::new(&data, 0)?;
//!     let glyph = font.nominal_glyph(0x1F600).ok_or("no glyph")?;
//!     let mut scratch = Scratch::default();
//!     let mut rec = Recording::new();
//!     let painted = paint_recording(&font, glyph.0, &PaintOptions::default(), &mut scratch, &mut rec);
//!     if painted != Painted::None {
//!         println!("{painted:?}: {} commands", rec.commands().len());
//!     }
//!     Ok(())
//! }
//! ```

#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

extern crate alloc;

mod extents;
mod paint;
mod palette;
mod raw;
mod record;
mod v1;

use sofel_outline::Outlines;
use sofel_outline::font::geom::{GlyphExtents, Painter, Transform};
use sofel_outline::font::read::colr::FOREGROUND;
use sofel_outline::font::{Font, HbScale};

use crate::extents::{Extents, ExtentsPainter, ExtentsStacks};
use crate::paint::{Direct, colr, paint_any, paint_colr_only, svg, svg_doc};

pub use paint::Painted;
pub use palette::{
    PaintOptions, Palette, find_palette, palette_color_name_id, palette_colors, palette_count,
    palette_entry_count, palette_flags, palette_name_id,
};
pub use raw::{PaintRef, RawPaintGraph, base_paint, clip_box, layer_paint, layer_paint_count};
pub use record::{PaintCommand, RecordedBrush, Recording};
pub use sofel_outline::font::read::colr::{
    Affine, ClipBox, ColorLine as RawColorLine, ColorStop as RawColorStop, Paint,
};

/// The work memory of [`paint`] and [`glyph_extents`]: color stops and
/// the glyphs and layers being walked. Keep it and reuse it across
/// glyphs (ADR 0003).
#[derive(Clone, Default, Debug)]
pub struct Scratch {
    pub(crate) v1: v1::V1Stacks,
    pub(crate) extents: ExtentsStacks,
    pub(crate) outline: sofel_outline::Scratch,
    /// The `COLR` table last checked (address and length) and whether
    /// its version 1 paints fit ([`Colr::v1_fits`]).
    ///
    /// [`Colr::v1_fits`]: sofel_outline::font::read::colr::Colr::v1_fits
    pub(crate) colr_fits: Option<(usize, usize, bool)>,
}

impl Scratch {
    /// Whether the version 1 paints of `colr` fit, remembered for the
    /// last table.
    pub(crate) fn v1_fits(&mut self, colr: &sofel_outline::font::read::colr::Colr<'_>) -> bool {
        let key = (colr.data().as_ptr() as usize, colr.data().len());
        match self.colr_fits {
            Some((p, l, fits)) if (p, l) == key => fits,
            _ => {
                let fits = colr.v1_fits();
                self.colr_fits = Some((key.0, key.1, fits));
                fits
            }
        }
    }
}

/// Paints `glyph` into `painter` as HarfBuzz's `hb_ot_paint_glyph_or_fail`
/// does: from `COLR`, else `SVG `, else `CBDT`, else `sbix`. Returns the
/// format used; [`Painted::None`] (nothing painted) for glyphs that are
/// not color glyphs, which a caller may fill with the foreground color
/// as HarfBuzz's `hb_font_paint_glyph` does.
pub fn paint<'a>(
    font: &Font<'a>,
    glyph: u32,
    options: &PaintOptions<'a>,
    scratch: &mut Scratch,
    painter: &mut impl Painter,
) -> Painted {
    paint_any(
        font.default_instance(),
        glyph,
        options,
        scratch,
        &mut Direct(painter),
    )
}

/// [`paint`] into a [`Recording`], which keeps images (their data
/// borrows the font).
pub fn paint_recording<'a>(
    font: &Font<'a>,
    glyph: u32,
    options: &PaintOptions<'a>,
    scratch: &mut Scratch,
    recording: &mut Recording<'a>,
) -> Painted {
    paint_any(font.default_instance(), glyph, options, scratch, recording)
}

/// Records color glyph commands at the supplied variation location.
pub fn paint_recording_at<'a>(
    instance: sofel_outline::font::var::Instance<'_, 'a, '_, '_>,
    glyph: u32,
    options: &PaintOptions<'a>,
    scratch: &mut Scratch,
    recording: &mut Recording<'a>,
) -> Painted {
    paint_any(instance, glyph, options, scratch, recording)
}

/// The `COLR` version 0 layers of `glyph`: each layer's glyph and its
/// palette entry, `None` for the foreground color
/// (`hb_ot_color_glyph_get_layers`, `FT_Get_Color_Glyph_Layer`).
pub fn layers<'a>(font: &Font<'a>, glyph: u32) -> impl Iterator<Item = (u32, Option<u16>)> + 'a {
    colr(font)
        .into_iter()
        .flat_map(move |c| c.glyph_layers(glyph))
        .map(|(g, p)| (u32::from(g), (p != FOREGROUND).then_some(p)))
}

/// The SVG document of a glyph.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SvgDocument<'a> {
    /// The document as stored (gzip-compressed when it starts with
    /// 0x1F 0x8B).
    pub data: &'a [u8],
    /// The document's index in the table.
    pub index: u32,
    /// The first glyph the document covers.
    pub start_glyph: u32,
    /// The last glyph the document covers.
    pub end_glyph: u32,
}

/// The SVG document of `glyph` (`hb_ot_color_glyph_reference_svg`,
/// `hb_ot_color_glyph_get_svg_document_index`,
/// `hb_ot_color_get_svg_document_glyph_range`). The glyph is the
/// element with `id="glyph<id>"` in it.
pub fn svg_document<'a>(font: &Font<'a>, glyph: u32) -> Option<SvgDocument<'a>> {
    let (i, data) = svg_doc(font, glyph)?;
    let r = svg(font)?.record(i)?;
    Some(SvgDocument {
        data,
        index: u32::try_from(i).ok()?,
        start_glyph: u32::from(r.start_glyph),
        end_glyph: u32::from(r.end_glyph),
    })
}

/// A color or bitmap format a font or glyph has.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ColorFormat {
    /// `COLR` version 1 paints.
    ColrV1,
    /// `COLR` version 0 layers.
    ColrV0,
    /// `SVG ` documents.
    Svg,
    /// `CBDT` bitmaps.
    Cbdt,
    /// `sbix` bitmaps.
    Sbix,
}

/// The color and bitmap formats of a font, as bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ColorFormats(pub u8);

impl ColorFormats {
    /// `COLR` version 0 layers (`hb_ot_color_has_layers`).
    pub const COLR_V0: u8 = 1;
    /// `COLR` version 1 paints (`hb_ot_color_has_paint`).
    pub const COLR_V1: u8 = 2;
    /// `CPAL` palettes (`hb_ot_color_has_palettes`).
    pub const CPAL: u8 = 4;
    /// `SVG ` (`hb_ot_color_has_svg`).
    pub const SVG: u8 = 8;
    /// `CBDT` (`hb_ot_color_has_png`).
    pub const CBDT: u8 = 16;
    /// `sbix` (`hb_ot_color_has_png`).
    pub const SBIX: u8 = 32;
    /// Strikes of `EBLC` or `bloc` (FreeType's `FT_HAS_FIXED_SIZES` for
    /// fonts without color bitmaps).
    pub const EBDT: u8 = 64;

    /// Whether every bit of `bits` is set.
    pub fn has(&self, bits: u8) -> bool {
        self.0 & bits == bits
    }
}

/// The color and bitmap formats `font` has, for choosing fonts.
pub fn color_formats(font: &Font<'_>) -> ColorFormats {
    use sofel_bitmap::StrikeTable;
    let c = colr(font);
    let mut bits = 0u8;
    if c.is_some_and(|c| c.num_base_glyphs() > 0) {
        bits |= ColorFormats::COLR_V0;
    }
    if c.is_some_and(|c| c.version() >= 1 && c.num_base_glyph_paints() > 0 && c.v1_fits()) {
        bits |= ColorFormats::COLR_V1;
    }
    if palette_count(font) > 0 {
        bits |= ColorFormats::CPAL;
    }
    if svg(font).is_some() {
        bits |= ColorFormats::SVG;
    }
    if sofel_bitmap::has_hb_cbdt(font) {
        bits |= ColorFormats::CBDT;
    }
    if sofel_bitmap::has_hb_sbix(font) {
        bits |= ColorFormats::SBIX;
    }
    if sofel_bitmap::strikes(font).any(|s| matches!(s.table, StrikeTable::Eblc | StrikeTable::Bloc))
    {
        bits |= ColorFormats::EBDT;
    }
    ColorFormats(bits)
}

/// The format [`paint`] would paint `glyph` from, without walking the
/// paint: a `COLR` version 1 record, a version 0 record, an `SVG `
/// document, a `CBDT` PNG or an `sbix` PNG in the largest strike. It
/// does not check that the table's version 1 paints fit
/// ([`color_formats`] does): for a broken table whose paints HarfBuzz
/// drops, it may say [`ColorFormat::ColrV1`] where [`paint`] paints
/// from version 0 or nothing.
pub fn glyph_color_format(font: &Font<'_>, glyph: u32) -> Option<ColorFormat> {
    if let Some(c) = colr(font) {
        if c.version() >= 1 && c.base_glyph_paint(glyph).is_some() {
            return Some(ColorFormat::ColrV1);
        }
        if c.find_base_glyph(glyph)
            .is_some_and(|r| u32::from(r.glyph) == glyph)
        {
            return Some(ColorFormat::ColrV0);
        }
    }
    if svg_doc(font, glyph).is_some() {
        return Some(ColorFormat::Svg);
    }
    let scale = HbScale::unscaled(font);
    if sofel_bitmap::cbdt_png(font, glyph, 0).is_some()
        && (sofel_bitmap::sbix_extents(font, glyph, 0, &scale).is_some()
            || sofel_bitmap::cbdt_extents(font, glyph, 0, &scale).is_some())
    {
        return Some(ColorFormat::Cbdt);
    }
    if sofel_bitmap::sbix_png(font, glyph, 0).is_some() {
        return Some(ColorFormat::Sbix);
    }
    None
}

/// `(float) v`.
fn to_f32(v: i32) -> f32 {
    // Rounds to nearest, as C's conversion does.
    #[allow(clippy::cast_precision_loss)]
    let f = v as f32;
    f
}

/// HarfBuzz's glyph box of `glyph` (`hb_font_get_glyph_extents` at the
/// default scale, `hb_ot_get_glyph_extents`): from `sbix`, `CBDT` (the
/// largest strikes) or `COLR`, else from the outlines
/// ([`Outlines::glyph_extents`]). In font units.
pub fn glyph_extents(
    outlines: &Outlines<'_>,
    glyph: u32,
    scratch: &mut Scratch,
) -> Option<GlyphExtents> {
    scaled_glyph_extents(
        outlines,
        glyph,
        &HbScale::unscaled(outlines.font()),
        scratch,
    )
}

/// [`glyph_extents`] at `scale` (HarfBuzz's font scale).
pub fn scaled_glyph_extents(
    outlines: &Outlines<'_>,
    glyph: u32,
    scale: &HbScale,
    scratch: &mut Scratch,
) -> Option<GlyphExtents> {
    let font = outlines.font();
    if let Some(e) = sofel_bitmap::sbix_extents(font, glyph, 0, scale) {
        return Some(e);
    }
    if let Some(e) = sofel_bitmap::cbdt_extents(font, glyph, 0, scale) {
        return Some(e);
    }
    if let Some(e) = colr_extents(outlines, glyph, scale, scratch) {
        return Some(e);
    }
    outlines
        .hb_extents()
        .scaled_glyph_extents(sofel_outline::font::read::GlyphId(glyph), scale)
}

/// HarfBuzz's `COLR::get_extents`: the clip box, else the painted box.
fn colr_extents(
    outlines: &Outlines<'_>,
    glyph: u32,
    scale: &HbScale,
    scratch: &mut Scratch,
) -> Option<GlyphExtents> {
    let font = outlines.font();
    let c = colr(font)?;
    if c.version() >= 1
        && let Some(b) = c.clip_box(glyph)
    {
        let e = GlyphExtents {
            x_bearing: i32::from(b.x_min),
            y_bearing: i32::from(b.y_max),
            width: i32::from(b.x_max).saturating_sub(i32::from(b.x_min)),
            height: i32::from(b.y_min).saturating_sub(i32::from(b.y_max)),
        };
        return Some(scale.scale_glyph_extents(e));
    }
    let upem = to_f32(i32::from(font.hb_units_per_em()));
    let root = Transform::scale(
        to_f32(scale.x_scale()) / upem,
        to_f32(scale.y_scale()) / upem,
    );
    let mut stacks = core::mem::take(&mut scratch.extents);
    let mut outline = core::mem::take(&mut scratch.outline);
    let (painted, e) = {
        // HarfBuzz draws the layer's outline and takes every point, as
        // the control box of the loaded outline does.
        let mut painter = ExtentsPainter::new(&mut stacks, |g| {
            let c = outlines
                .load(
                    sofel_outline::font::read::GlyphId(g),
                    sofel_outline::Scale::Unscaled,
                    &mut outline,
                )
                .ok()?
                .cbox;
            Some(GlyphExtents {
                x_bearing: c.x_min,
                y_bearing: c.y_max,
                width: c.x_max.saturating_sub(c.x_min),
                height: c.y_min.saturating_sub(c.y_max),
            })
        });
        painter.push_transform(root);
        let options = PaintOptions {
            foreground: Default::default(),
            ..PaintOptions::default()
        };
        let painted = paint_colr_only(font, glyph, &options, scratch, &mut Direct(&mut painter));
        (painted, painter.extents())
    };
    scratch.extents = stacks;
    scratch.outline = outline;
    painted?;
    Some(if e.is_void() {
        GlyphExtents::default()
    } else {
        Extents::to_glyph(e)
    })
}
