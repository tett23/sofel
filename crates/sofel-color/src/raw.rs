//! The raw paint graph of `COLR` version 1, as FreeType's
//! `FT_Get_Color_Glyph_Paint`, `FT_Get_Paint`, `FT_Get_Paint_Layers`,
//! `FT_Get_Colorline_Stops` and `FT_Get_Color_Glyph_ClipBox` give it:
//! the values as stored, for callers that share layers and subgraphs
//! themselves.

use sofel_outline::font::Font;
use sofel_outline::font::read::colr::{ClipBox, Colr, Paint};

use crate::paint::colr;

/// A paint table of a font's `COLR`.
#[derive(Clone, Copy, Debug)]
pub struct PaintRef<'a> {
    colr: Colr<'a>,
    offset: usize,
}

impl<'a> PaintRef<'a> {
    /// The paint's offset from the start of `COLR`; equal offsets are the
    /// same (shared) paint.
    pub fn offset(&self) -> usize {
        self.offset
    }
    /// The paint format (1 to 33).
    pub fn format(&self) -> Option<u8> {
        self.colr.paint_format(self.offset)
    }
    /// The paint's values; `None` for an unknown format or a paint that
    /// does not fit. Child paints are offsets for [`PaintRef::at`].
    pub fn paint(&self) -> Option<Paint<'a>> {
        self.colr.paint(self.offset)
    }
    /// The paint at `offset` of the same table (a child of [`PaintRef::paint`]).
    pub fn at(&self, offset: usize) -> PaintRef<'a> {
        PaintRef {
            colr: self.colr,
            offset,
        }
    }
}

/// The version 1 paint of `glyph` (its BaseGlyphPaintRecord), found as
/// FreeType's `FT_Get_Color_Glyph_Paint` finds it.
pub fn base_paint<'a>(font: &Font<'a>, glyph: u32) -> Option<PaintRef<'a>> {
    let c = colr(font)?;
    let offset = c.base_glyph_paint_ft(glyph)?;
    Some(PaintRef { colr: c, offset })
}

/// The clip box of `glyph`, as stored.
pub fn clip_box(font: &Font<'_>, glyph: u32) -> Option<ClipBox> {
    colr(font)?.clip_box(glyph)
}

/// The number of paints in the LayerList.
pub fn layer_paint_count(font: &Font<'_>) -> usize {
    colr(font).map_or(0, |c| c.num_layer_paints())
}

/// Paint `i` of the LayerList (the layers of `PaintColrLayers`).
pub fn layer_paint<'a>(font: &Font<'a>, i: usize) -> Option<PaintRef<'a>> {
    let c = colr(font)?;
    let offset = c.layer_paint(i)?;
    Some(PaintRef { colr: c, offset })
}
