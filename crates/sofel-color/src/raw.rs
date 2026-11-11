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

/// A COLR table opened with FreeType's variation-store checks. Reuse it when
/// walking more than one glyph of the same face.
#[derive(Clone, Copy, Debug)]
pub struct RawPaintGraph<'a>(pub(crate) Colr<'a>);

impl<'a> RawPaintGraph<'a> {
    /// Opens the paint graph, checking the variation store and map once.
    pub fn new(font: &Font<'a>) -> Option<Self> {
        use sofel_outline::font::read::{Tag, fvar::Fvar};
        let table = colr(font)?;
        let fvar = font
            .table(Tag::new(b"fvar"))
            .and_then(|bytes| Fvar::new(bytes).ok());
        if let Some(fvar) = fvar
            && table.version() == 1
        {
            let bytes = font.table(sofel_outline::font::read::colr::TAG)?;
            if usize::try_from(table.var_index_map_offset()).ok()? >= bytes.len() {
                return None;
            }
            if let Some(store) = table.variation_store().ok()? {
                if store.data_count() == 0
                    || usize::from(store.axis_count()) != fvar.axis_count()
                    || store.region_count() >= 32768
                {
                    return None;
                }
                for i in 0..store.data_count() {
                    let data = store.item_data(i)?;
                    if data.region_index_count() > store.region_count()
                        || (0..data.region_index_count()).any(|j| {
                            data.region_index(usize::from(j))
                                .is_none_or(|r| r >= store.region_count())
                        })
                    {
                        return None;
                    }
                }
                if store.axis_count() != 0
                    && let Some(map) = table.var_index_map().ok()?
                {
                    for i in 0..map.len() {
                        let index = map.map(i);
                        if index == u32::MAX {
                            continue;
                        }
                        let [a, b, c, d] = index.to_be_bytes();
                        if u16::from_be_bytes([c, d])
                            >= store.item_data(u16::from_be_bytes([a, b]))?.item_count
                        {
                            return None;
                        }
                    }
                }
            }
        }
        Some(Self(table))
    }

    /// Finds a glyph's root paint.
    pub fn base_paint(self, glyph: u32) -> Option<PaintRef<'a>> {
        Some(PaintRef {
            colr: self.0,
            offset: self.0.base_glyph_paint_ft(glyph)?,
        })
    }

    /// Returns the stored clip box.
    pub fn clip_box(self, glyph: u32) -> Option<ClipBox> {
        self.0.clip_box(glyph)
    }

    /// Opens a child from the LayerList.
    pub fn layer_paint(self, index: usize) -> Option<PaintRef<'a>> {
        Some(PaintRef {
            colr: self.0,
            offset: self.0.layer_paint(index)?,
        })
    }
}

/// The version 1 paint of `glyph` (its BaseGlyphPaintRecord), found as
/// FreeType's `FT_Get_Color_Glyph_Paint` finds it.
pub fn base_paint<'a>(font: &Font<'a>, glyph: u32) -> Option<PaintRef<'a>> {
    RawPaintGraph::new(font)?.base_paint(glyph)
}

/// The clip box of `glyph`, as stored.
pub fn clip_box(font: &Font<'_>, glyph: u32) -> Option<ClipBox> {
    RawPaintGraph::new(font)?.clip_box(glyph)
}

/// The number of paints in the LayerList.
pub fn layer_paint_count(font: &Font<'_>) -> usize {
    RawPaintGraph::new(font).map_or(0, |g| g.0.num_layer_paints())
}

/// Paint `i` of the LayerList (the layers of `PaintColrLayers`).
pub fn layer_paint<'a>(font: &Font<'a>, i: usize) -> Option<PaintRef<'a>> {
    RawPaintGraph::new(font)?.layer_paint(i)
}
