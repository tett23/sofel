//! The `COLR` (color) table: version 0's base glyph and layer records
//! and the lists of version 1 (ADR 0023).
//!
//! Lookups follow HarfBuzz 15.1.0's `src/OT/Color/COLR/COLR.hh`: binary
//! search by glyph id, layer ranges cut at the end of the layer records.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/colr>

use core::cmp::Ordering;

use crate::be::{self, to_u32, to_usize};
use crate::search::hb_bsearch;
use crate::{Diagnostic, F2Dot14, Fixed, Kind, ReadError, Tag};

/// The `COLR` tag.
pub const TAG: Tag = Tag::new(b"COLR");

/// The palette index of the foreground color.
pub const FOREGROUND: u16 = 0xFFFF;

/// A version 0 base glyph record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BaseGlyph {
    /// The glyph.
    pub glyph: u16,
    /// Its first layer record.
    pub first_layer: u16,
    /// Its number of layers.
    pub num_layers: u16,
}

/// A clip box (`ClipBox` formats 1 and 2), in font units.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClipBox {
    /// The format: 1, or 2 with variations.
    pub format: u8,
    /// The left edge.
    pub x_min: i16,
    /// The bottom edge.
    pub y_min: i16,
    /// The right edge.
    pub x_max: i16,
    /// The top edge.
    pub y_max: i16,
    /// `varIndexBase` of format 2.
    pub var_index_base: Option<u32>,
}

/// A view over the `COLR` table. Building it checks the header, the
/// version 0 record arrays and the version 1 lists' record arrays.
#[derive(Clone, Copy, Debug)]
pub struct Colr<'a> {
    data: &'a [u8],
    version: u16,
    base_glyphs: &'a [[u8; 6]],
    layers: &'a [[u8; 4]],
    /// Offset of the BaseGlyphList and its records.
    base_glyph_list: Option<(usize, &'a [[u8; 6]])>,
    /// Offset of the LayerList and its paint offsets.
    layer_list: Option<(usize, &'a [[u8; 4]])>,
    /// Offset of the ClipList and its records.
    clip_list: Option<(usize, &'a [[u8; 7]])>,
    var_index_map: u32,
    var_store: u32,
}

/// A list's offset and its records.
type List<'a, const N: usize> = (usize, &'a [[u8; N]]);

/// `count` records of `N` bytes after a `u32` count at `at`, or `None`
/// for a 0 offset.
fn list<const N: usize>(
    data: &[u8],
    at: u32,
    header: usize,
) -> Result<Option<List<'_, N>>, ReadError> {
    if at == 0 {
        return Ok(None);
    }
    let at = to_usize(at);
    let count = be::u32_at(data, at.saturating_add(header.saturating_sub(4)))
        .ok_or(ReadError::OutOfBounds)?;
    let records = be::chunks_at::<N>(data, at.saturating_add(header), to_usize(count))
        .ok_or(ReadError::OutOfBounds)?;
    Ok(Some((at, records)))
}

impl<'a> Colr<'a> {
    /// A view over `data`.
    ///
    /// Fails with [`ReadError::Truncated`] when the header does not fit,
    /// and with [`ReadError::OutOfBounds`] when the base glyph or layer
    /// records, or a version 1 list's records, do not.
    pub fn new(data: &'a [u8]) -> Result<Colr<'a>, ReadError> {
        let version = be::u16_at(data, 0).ok_or(ReadError::truncated(14, data.len()))?;
        let header = if version == 0 { 14 } else { 34 };
        let h = be::slice_at(data, 0, header).ok_or(ReadError::truncated(header, data.len()))?;
        let u16_ = |at: usize| be::u16_at(h, at).unwrap_or(0);
        let u32_ = |at: usize| be::u32_at(h, at).unwrap_or(0);
        let base_glyphs = be::chunks_at::<6>(data, to_usize(u32_(4)), usize::from(u16_(2)))
            .ok_or(ReadError::OutOfBounds)?;
        let layers = be::chunks_at::<4>(data, to_usize(u32_(8)), usize::from(u16_(12)))
            .ok_or(ReadError::OutOfBounds)?;
        let (mut base_glyph_list, mut layer_list, mut clip_list) = (None, None, None);
        let (mut var_index_map, mut var_store) = (0, 0);
        if version != 0 {
            base_glyph_list = list::<6>(data, u32_(14), 4)?;
            layer_list = list::<4>(data, u32_(18), 4)?;
            clip_list = list::<7>(data, u32_(22), 5)?;
            var_index_map = u32_(26);
            var_store = u32_(30);
        }
        Ok(Colr {
            data,
            version,
            base_glyphs,
            layers,
            base_glyph_list,
            layer_list,
            clip_list,
            var_index_map,
            var_store,
        })
    }

    /// The table's bytes.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// The table version.
    pub fn version(&self) -> u16 {
        self.version
    }

    /// `numBaseGlyphRecords`.
    pub fn num_base_glyphs(&self) -> usize {
        self.base_glyphs.len()
    }
    /// Base glyph record `i`.
    pub fn base_glyph(&self, i: usize) -> Option<BaseGlyph> {
        self.base_glyphs.get(i).map(|r| BaseGlyph {
            glyph: u16::from_be_bytes([r[0], r[1]]),
            first_layer: u16::from_be_bytes([r[2], r[3]]),
            num_layers: u16::from_be_bytes([r[4], r[5]]),
        })
    }
    /// The base glyph record of `glyph`, by HarfBuzz's binary search.
    pub fn find_base_glyph(&self, glyph: u32) -> Option<BaseGlyph> {
        let i = hb_bsearch(self.base_glyphs.len(), |i| {
            self.base_glyph(i).map(|r| glyph.cmp(&u32::from(r.glyph)))
        })?;
        self.base_glyph(i)
    }
    /// `numLayerRecords`.
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }
    /// Layer record `i`: its glyph and palette index.
    pub fn layer(&self, i: usize) -> Option<(u16, u16)> {
        self.layers.get(i).map(|r| {
            (
                u16::from_be_bytes([r[0], r[1]]),
                u16::from_be_bytes([r[2], r[3]]),
            )
        })
    }
    /// The layers of `glyph` (glyph and palette index), as HarfBuzz's
    /// `get_glyph_layers` gives them: the range of its base glyph record
    /// cut at the end of the layer records; none without a record.
    pub fn glyph_layers(&self, glyph: u32) -> impl Iterator<Item = (u16, u16)> + use<'a> {
        let layers = self.layers;
        let range = self.find_base_glyph(glyph).map_or(0..0, |r| {
            let first = usize::from(r.first_layer).min(layers.len());
            first
                ..first
                    .saturating_add(usize::from(r.num_layers))
                    .min(layers.len())
        });
        layers.get(range).unwrap_or_default().iter().map(|r| {
            (
                u16::from_be_bytes([r[0], r[1]]),
                u16::from_be_bytes([r[2], r[3]]),
            )
        })
    }

    /// The number of BaseGlyphPaintRecords (version 1).
    pub fn num_base_glyph_paints(&self) -> usize {
        self.base_glyph_list.map_or(0, |(_, r)| r.len())
    }
    /// BaseGlyphPaintRecord `i`: its glyph and the offset of its paint
    /// from the start of the table.
    pub fn base_glyph_paint_record(&self, i: usize) -> Option<(u16, usize)> {
        let (at, records) = self.base_glyph_list?;
        records.get(i).map(|r| {
            let off = to_usize(u32::from_be_bytes([r[2], r[3], r[4], r[5]]));
            (u16::from_be_bytes([r[0], r[1]]), at.saturating_add(off))
        })
    }
    /// The offset, from the start of the table, of the paint of `glyph`
    /// (version 1), by HarfBuzz's binary search.
    pub fn base_glyph_paint(&self, glyph: u32) -> Option<usize> {
        let i = hb_bsearch(self.num_base_glyph_paints(), |i| {
            self.base_glyph_paint_record(i)
                .map(|(g, _)| glyph.cmp(&u32::from(g)))
        })?;
        self.base_glyph_paint_record(i).map(|(_, off)| off)
    }
    /// The offset of the paint of `glyph`, by FreeType's binary search
    /// (`find_base_glyph_v1_record`), which probes differently from
    /// HarfBuzz's on records out of order.
    pub fn base_glyph_paint_ft(&self, glyph: u32) -> Option<usize> {
        let (mut min, mut max) = (0usize, self.num_base_glyph_paints());
        while min < max {
            let mid = min.saturating_add(max.saturating_sub(min) / 2);
            let (g, off) = self.base_glyph_paint_record(mid)?;
            match u32::from(g).cmp(&glyph) {
                Ordering::Less => min = mid.saturating_add(1),
                Ordering::Greater => max = mid,
                Ordering::Equal => return Some(off),
            }
        }
        None
    }

    /// The number of paints in the LayerList (version 1).
    pub fn num_layer_paints(&self) -> usize {
        self.layer_list.map_or(0, |(_, r)| r.len())
    }
    /// The offset, from the start of the table, of layer paint `i`.
    pub fn layer_paint(&self, i: usize) -> Option<usize> {
        let (at, records) = self.layer_list?;
        records
            .get(i)
            .map(|r| at.saturating_add(to_usize(u32::from_be_bytes(*r))))
    }
    /// The number of ClipRecords (version 1).
    pub fn num_clips(&self) -> usize {
        self.clip_list.map_or(0, |(_, r)| r.len())
    }
    /// ClipRecord `i`: its first and last glyph and the offset of its
    /// clip box from the start of the table.
    pub fn clip_record(&self, i: usize) -> Option<(u16, u16, usize)> {
        let (at, records) = self.clip_list?;
        records.get(i).map(|r| {
            (
                u16::from_be_bytes([r[0], r[1]]),
                u16::from_be_bytes([r[2], r[3]]),
                at.saturating_add(to_usize(be::u24([r[4], r[5], r[6]]))),
            )
        })
    }
    /// The clip box at `offset` from the start of the table; `None` for
    /// an unknown format or one that does not fit.
    pub fn clip_box_at(&self, offset: usize) -> Option<ClipBox> {
        let format = be::u8_at(self.data, offset)?;
        let i = |k: usize| be::i16_at(self.data, offset.saturating_add(k));
        let var_index_base = match format {
            1 => None,
            2 => Some(be::u32_at(self.data, offset.saturating_add(9))?),
            _ => return None,
        };
        Some(ClipBox {
            format,
            x_min: i(1)?,
            y_min: i(3)?,
            x_max: i(5)?,
            y_max: i(7)?,
            var_index_base,
        })
    }
    /// The clip box of `glyph` (version 1): HarfBuzz's binary search over
    /// the clip records by glyph range.
    pub fn clip_box(&self, glyph: u32) -> Option<ClipBox> {
        let i = hb_bsearch(self.num_clips(), |i| {
            self.clip_record(i).map(|(start, end, _)| {
                if glyph < u32::from(start) {
                    Ordering::Less
                } else if glyph <= u32::from(end) {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                }
            })
        })?;
        self.clip_record(i)
            .and_then(|(_, _, off)| self.clip_box_at(off))
    }
    /// Opens the optional DeltaSetIndexMap (ADR 0027).
    /// Null offsets return `None`; malformed non-null maps return an error.
    pub fn var_index_map(
        &self,
    ) -> Result<Option<crate::delta_set_index_map::DeltaSetIndexMap<'a>>, ReadError> {
        if self.var_index_map == 0 {
            return Ok(None);
        }
        let data = crate::be::tail_at(self.data, crate::be::to_usize(self.var_index_map))
            .ok_or(ReadError::OutOfBounds)?;
        crate::delta_set_index_map::DeltaSetIndexMap::new(data).map(Some)
    }

    /// The offset of the DeltaSetIndexMap (0 for none).
    pub fn var_index_map_offset(&self) -> u32 {
        self.var_index_map
    }
    /// Opens the optional ItemVariationStore (ADR 0027).
    /// Null offsets return `None`; malformed non-null stores return an error.
    pub fn variation_store(
        &self,
    ) -> Result<Option<crate::item_variation::ItemVariationStore<'a>>, ReadError> {
        let offset = self.var_store_offset();
        if offset == 0 {
            return Ok(None);
        }
        let data = crate::be::tail_at(self.data, crate::be::to_usize(offset))
            .ok_or(ReadError::OutOfBounds)?;
        crate::item_variation::ItemVariationStore::new(data).map(Some)
    }

    /// The offset of the ItemVariationStore (0 for none).
    pub fn var_store_offset(&self) -> u32 {
        self.var_store
    }
}

/// Checks a `COLR` table: a truncated header and record arrays out of
/// bounds (and nothing else then); base glyph and paint records not in
/// ascending glyph order ([`Kind::GlyphsNotSorted`]) or repeating a
/// glyph ([`Kind::DuplicateGlyph`]); layer ranges past the layer records
/// ([`Kind::LayerIndexOutOfRange`]); glyph ids not below `num_glyphs`
/// when given; palette indices not below `palette_entries` (the
/// `numPaletteEntries` of `CPAL`) when given; clip records whose range
/// is reversed or out of order, and clip boxes of unknown format.
pub fn validate(
    data: &[u8],
    num_glyphs: Option<u32>,
    palette_entries: Option<u16>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let colr = match Colr::new(data) {
        Ok(c) => c,
        Err(ReadError::Truncated { needed, available }) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed),
                    available: to_u32(available),
                },
            );
        }
        Err(_) => {
            return emit(
                0,
                Kind::OffsetOutOfBounds {
                    offset: 0,
                    length: to_u32(data.len()),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    let glyph_check = |emit: &mut dyn FnMut(usize, Kind), at: usize, g: u32| {
        if let Some(n) = num_glyphs
            && g >= n
        {
            emit(
                at,
                Kind::GlyphIdOutOfRange {
                    glyph: g,
                    num_glyphs: n,
                },
            );
        }
    };
    let sorted =
        |emit: &mut dyn FnMut(usize, Kind), at: usize, i: usize, prev: Option<u16>, g: u16| {
            match prev {
                Some(p) if p == g => emit(
                    at,
                    Kind::DuplicateGlyph {
                        index: to_u32(i),
                        glyph: u32::from(g),
                    },
                ),
                Some(p) if g < p => emit(at, Kind::GlyphsNotSorted { index: to_u32(i) }),
                _ => {}
            }
        };
    let base_at = be::u32_at(data, 4).map_or(0, to_usize);
    let mut prev = None;
    let layers = to_u32(colr.num_layers());
    for i in 0..colr.num_base_glyphs() {
        let Some(r) = colr.base_glyph(i) else {
            continue;
        };
        let at = base_at.saturating_add(i.saturating_mul(6));
        sorted(&mut emit, at, i, prev, r.glyph);
        prev = Some(r.glyph);
        glyph_check(&mut emit, at, u32::from(r.glyph));
        let end = u32::from(r.first_layer).saturating_add(u32::from(r.num_layers));
        if end > layers {
            emit(
                at,
                Kind::LayerIndexOutOfRange {
                    index: end.saturating_sub(1),
                    count: layers,
                },
            );
        }
    }
    let layers_at = be::u32_at(data, 8).map_or(0, to_usize);
    for i in 0..colr.num_layers() {
        let Some((g, p)) = colr.layer(i) else {
            continue;
        };
        let at = layers_at.saturating_add(i.saturating_mul(4));
        glyph_check(&mut emit, at, u32::from(g));
        if let Some(n) = palette_entries
            && p != FOREGROUND
            && p >= n
        {
            emit(
                at,
                Kind::ColorIndexOutOfRange {
                    index: u32::from(p),
                    count: u32::from(n),
                },
            );
        }
    }
    let mut prev = None;
    let list_at = be::u32_at(data, 14).map_or(0, to_usize);
    for i in 0..colr.num_base_glyph_paints() {
        let Some((g, off)) = colr.base_glyph_paint_record(i) else {
            continue;
        };
        let at = list_at
            .saturating_add(4)
            .saturating_add(i.saturating_mul(6));
        sorted(&mut emit, at, i, prev, g);
        prev = Some(g);
        glyph_check(&mut emit, at, u32::from(g));
        if be::u8_at(data, off).is_none() {
            emit(
                at,
                Kind::OffsetOutOfBounds {
                    offset: to_u32(off),
                    length: 1,
                    available: to_u32(data.len()),
                },
            );
        }
    }
    validate_paints(&colr, num_glyphs, palette_entries, &mut emit);
    let clips_at = be::u32_at(data, 22).map_or(0, to_usize);
    let mut prev_end: Option<u16> = None;
    for i in 0..colr.num_clips() {
        let Some((start, end, off)) = colr.clip_record(i) else {
            continue;
        };
        let at = clips_at
            .saturating_add(5)
            .saturating_add(i.saturating_mul(7));
        if start > end {
            emit(
                at,
                Kind::StartAfterEnd {
                    index: to_u32(i),
                    start: u32::from(start),
                    end: u32::from(end),
                },
            );
        }
        if let Some(p) = prev_end
            && start <= p
        {
            emit(at, Kind::RecordsNotSorted { index: to_u32(i) });
        }
        prev_end = Some(end);
        match be::u8_at(data, off) {
            Some(1 | 2) if colr.clip_box_at(off).is_some() => {}
            Some(f @ (3..)) | Some(f @ 0) => emit(
                off,
                Kind::UnknownFormat {
                    found: u32::from(f),
                },
            ),
            _ => emit(
                at,
                Kind::OffsetOutOfBounds {
                    offset: to_u32(off),
                    length: 9,
                    available: to_u32(data.len()),
                },
            ),
        }
    }
    crate::validate::variation_store(data, colr.var_store_offset(), 30, TAG, None, report);
    let offset = colr.var_index_map_offset();
    if offset != 0 {
        if let Some(child) = be::tail_at(data, to_usize(offset)) {
            crate::delta_set_index_map::validate(child, TAG, offset, report);
        } else {
            report(Diagnostic::new(
                TAG,
                26,
                Kind::OffsetOutOfBounds {
                    offset,
                    length: 0,
                    available: to_u32(data.len()),
                },
            ));
        }
    }
}

/// An `Affine2x3` (`VarAffine2x3`): 16.16 values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Affine {
    /// `xx`.
    pub xx: Fixed,
    /// `yx`.
    pub yx: Fixed,
    /// `xy`.
    pub xy: Fixed,
    /// `yy`.
    pub yy: Fixed,
    /// `dx`.
    pub dx: Fixed,
    /// `dy`.
    pub dy: Fixed,
    /// `varIndexBase` of `VarAffine2x3`.
    pub var_index_base: Option<u32>,
}

/// A color stop (`ColorStop` or `VarColorStop`), raw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColorStop {
    /// `stopOffset`.
    pub offset: F2Dot14,
    /// `paletteIndex` (0xFFFF: the foreground).
    pub palette_index: u16,
    /// `alpha`.
    pub alpha: F2Dot14,
    /// `varIndexBase` of `VarColorStop`.
    pub var_index_base: Option<u32>,
}

/// A `ColorLine` or `VarColorLine`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColorLine<'a> {
    /// `extend`: 0 pad, 1 repeat, 2 reflect (others read as pad).
    pub extend: u8,
    stops: &'a [u8],
    var: bool,
    declared: u16,
}

impl<'a> ColorLine<'a> {
    fn new(data: &'a [u8], at: usize, var: bool) -> Option<ColorLine<'a>> {
        let extend = be::u8_at(data, at)?;
        let declared = be::u16_at(data, at.checked_add(1)?)?;
        let size = if var { 10 } else { 6 };
        // All the stops, or none when they do not all fit (FreeType's
        // `FT_Get_Colorline_Stops` reads none then).
        let stops = be::slice_at(
            data,
            at.checked_add(3)?,
            usize::from(declared).checked_mul(size)?,
        )
        .unwrap_or_default();
        Some(ColorLine {
            extend,
            stops,
            var,
            declared,
        })
    }
    /// `numStops` as stored; [`ColorLine::len`] is 0 when the stops do not
    /// all fit in the table.
    pub fn declared_len(&self) -> u16 {
        self.declared
    }
    /// The number of stops.
    pub fn len(&self) -> usize {
        if self.var {
            self.stops.len() / 10
        } else {
            self.stops.len() / 6
        }
    }
    /// Whether there are no stops.
    pub fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }
    /// Stop `i`.
    pub fn stop(&self, i: usize) -> Option<ColorStop> {
        let size = if self.var { 10 } else { 6 };
        let at = i.checked_mul(size)?;
        let s = self.stops.get(at..at.checked_add(size)?)?;
        Some(ColorStop {
            offset: F2Dot14(be::i16_at(s, 0)?),
            palette_index: be::u16_at(s, 2)?,
            alpha: F2Dot14(be::i16_at(s, 4)?),
            var_index_base: if self.var { be::u32_at(s, 6) } else { None },
        })
    }
}

/// A Paint table, raw (formats 1 to 33). Child paints are offsets from
/// the start of the `COLR` table; `None` for a 0 offset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Paint<'a> {
    /// Format 1, `PaintColrLayers`.
    ColrLayers {
        /// `numLayers`.
        num_layers: u8,
        /// `firstLayerIndex` in the LayerList.
        first_layer: u32,
    },
    /// Formats 2 and 3, `PaintSolid` and `PaintVarSolid`.
    Solid {
        /// `paletteIndex`.
        palette_index: u16,
        /// `alpha`.
        alpha: F2Dot14,
        /// `varIndexBase` (format 3).
        var_index_base: Option<u32>,
    },
    /// Formats 4 and 5: `PaintLinearGradient`, `PaintVarLinearGradient`.
    LinearGradient {
        /// The color line.
        color_line: Option<ColorLine<'a>>,
        /// `x0`, `y0`, `x1`, `y1`, `x2`, `y2`.
        points: [i16; 6],
        /// `varIndexBase` (format 5).
        var_index_base: Option<u32>,
    },
    /// Formats 6 and 7: `PaintRadialGradient`, `PaintVarRadialGradient`.
    RadialGradient {
        /// The color line.
        color_line: Option<ColorLine<'a>>,
        /// `x0`, `y0`.
        c0: (i16, i16),
        /// `radius0` (unsigned, as the specification and HarfBuzz read it).
        r0: u16,
        /// `x1`, `y1`.
        c1: (i16, i16),
        /// `radius1`.
        r1: u16,
        /// `varIndexBase` (format 7).
        var_index_base: Option<u32>,
    },
    /// Formats 8 and 9: `PaintSweepGradient`, `PaintVarSweepGradient`.
    SweepGradient {
        /// The color line.
        color_line: Option<ColorLine<'a>>,
        /// `centerX`, `centerY`.
        center: (i16, i16),
        /// `startAngle`.
        start_angle: F2Dot14,
        /// `endAngle`.
        end_angle: F2Dot14,
        /// `varIndexBase` (format 9).
        var_index_base: Option<u32>,
    },
    /// Formats 10 and 33 (24-bit glyph id): `PaintGlyph`.
    Glyph {
        /// The fill.
        paint: Option<usize>,
        /// The glyph.
        glyph: u32,
    },
    /// Format 11, `PaintColrGlyph`.
    ColrGlyph {
        /// The color glyph.
        glyph: u16,
    },
    /// Formats 12 and 13: `PaintTransform`, `PaintVarTransform`.
    Transform {
        /// The child.
        paint: Option<usize>,
        /// The transform; `None` when its offset is 0 or it does not fit.
        transform: Option<Affine>,
    },
    /// Formats 14 and 15: `PaintTranslate`, `PaintVarTranslate`.
    Translate {
        /// The child.
        paint: Option<usize>,
        /// `dx`, `dy`.
        delta: (i16, i16),
        /// `varIndexBase` (format 15).
        var_index_base: Option<u32>,
    },
    /// Formats 16 to 23: `PaintScale`, `PaintScaleAroundCenter`,
    /// `PaintScaleUniform`, `PaintScaleUniformAroundCenter` and their
    /// variable forms. A uniform scale has `scale.0 == scale.1`.
    Scale {
        /// The child.
        paint: Option<usize>,
        /// `scaleX`, `scaleY` (or `scale` twice).
        scale: (F2Dot14, F2Dot14),
        /// `centerX`, `centerY` of the forms around a center.
        center: Option<(i16, i16)>,
        /// `varIndexBase` of the variable forms.
        var_index_base: Option<u32>,
    },
    /// Formats 24 to 27: `PaintRotate`, `PaintRotateAroundCenter` and
    /// their variable forms.
    Rotate {
        /// The child.
        paint: Option<usize>,
        /// `angle`.
        angle: F2Dot14,
        /// `centerX`, `centerY` of the forms around a center.
        center: Option<(i16, i16)>,
        /// `varIndexBase` of the variable forms.
        var_index_base: Option<u32>,
    },
    /// Formats 28 to 31: `PaintSkew`, `PaintSkewAroundCenter` and their
    /// variable forms.
    Skew {
        /// The child.
        paint: Option<usize>,
        /// `xSkewAngle`, `ySkewAngle`.
        angles: (F2Dot14, F2Dot14),
        /// `centerX`, `centerY` of the forms around a center.
        center: Option<(i16, i16)>,
        /// `varIndexBase` of the variable forms.
        var_index_base: Option<u32>,
    },
    /// Format 32, `PaintComposite`.
    Composite {
        /// The source paint.
        source: Option<usize>,
        /// `compositeMode`.
        mode: u8,
        /// The backdrop paint.
        backdrop: Option<usize>,
    },
}

impl<'a> Colr<'a> {
    /// The format of the paint at `offset`.
    pub fn paint_format(&self, offset: usize) -> Option<u8> {
        be::u8_at(self.data, offset)
    }

    /// The paint at `offset` from the start of the table; `None` for an
    /// unknown format or one that does not fit.
    pub fn paint(&self, offset: usize) -> Option<Paint<'a>> {
        let d = self.data;
        let format = be::u8_at(d, offset)?;
        let at = |k: usize| offset.checked_add(k);
        let i16_ = |k: usize| at(k).and_then(|p| be::i16_at(d, p));
        let u16_ = |k: usize| at(k).and_then(|p| be::u16_at(d, p));
        let u32_ = |k: usize| at(k).and_then(|p| be::u32_at(d, p));
        let f2 = |k: usize| i16_(k).map(F2Dot14);
        let child = |k: usize| {
            at(k)
                .and_then(|p| be::u24_at(d, p))
                .map(|o| (o != 0).then(|| offset.saturating_add(to_usize(o))))
        };
        let line = |var: bool| child(1).map(|o| o.and_then(|o| ColorLine::new(d, o, var)));
        let var = |k: usize, is_var: bool| -> Option<Option<u32>> {
            if is_var {
                u32_(k).map(Some)
            } else {
                Some(None)
            }
        };
        let odd = format % 2 == 1;
        Some(match format {
            1 => Paint::ColrLayers {
                num_layers: be::u8_at(d, at(1)?)?,
                first_layer: u32_(2)?,
            },
            2 | 3 => Paint::Solid {
                palette_index: u16_(1)?,
                alpha: f2(3)?,
                var_index_base: var(5, format == 3)?,
            },
            4 | 5 => Paint::LinearGradient {
                color_line: line(format == 5)?,
                points: [
                    i16_(4)?,
                    i16_(6)?,
                    i16_(8)?,
                    i16_(10)?,
                    i16_(12)?,
                    i16_(14)?,
                ],
                var_index_base: var(16, format == 5)?,
            },
            6 | 7 => Paint::RadialGradient {
                color_line: line(format == 7)?,
                c0: (i16_(4)?, i16_(6)?),
                r0: u16_(8)?,
                c1: (i16_(10)?, i16_(12)?),
                r1: u16_(14)?,
                var_index_base: var(16, format == 7)?,
            },
            8 | 9 => Paint::SweepGradient {
                color_line: line(format == 9)?,
                center: (i16_(4)?, i16_(6)?),
                start_angle: f2(8)?,
                end_angle: f2(10)?,
                var_index_base: var(12, format == 9)?,
            },
            10 => Paint::Glyph {
                paint: child(1)?,
                glyph: u32::from(u16_(4)?),
            },
            33 => Paint::Glyph {
                paint: child(1)?,
                glyph: at(4).and_then(|p| be::u24_at(d, p))?,
            },
            11 => Paint::ColrGlyph { glyph: u16_(1)? },
            12 | 13 => {
                let t = child(4)?;
                let affine = t.and_then(|t| {
                    let f = |k: usize| be::i32_at(d, t.checked_add(k)?).map(Fixed);
                    Some(Affine {
                        xx: f(0)?,
                        yx: f(4)?,
                        xy: f(8)?,
                        yy: f(12)?,
                        dx: f(16)?,
                        dy: f(20)?,
                        var_index_base: if format == 13 {
                            Some(be::u32_at(d, t.checked_add(24)?)?)
                        } else {
                            None
                        },
                    })
                });
                Paint::Transform {
                    paint: child(1)?,
                    transform: affine,
                }
            }
            14 | 15 => Paint::Translate {
                paint: child(1)?,
                delta: (i16_(4)?, i16_(6)?),
                var_index_base: var(8, odd)?,
            },
            16 | 17 => Paint::Scale {
                paint: child(1)?,
                scale: (f2(4)?, f2(6)?),
                center: None,
                var_index_base: var(8, odd)?,
            },
            18 | 19 => Paint::Scale {
                paint: child(1)?,
                scale: (f2(4)?, f2(6)?),
                center: Some((i16_(8)?, i16_(10)?)),
                var_index_base: var(12, odd)?,
            },
            20 | 21 => Paint::Scale {
                paint: child(1)?,
                scale: (f2(4)?, f2(4)?),
                center: None,
                var_index_base: var(6, odd)?,
            },
            22 | 23 => Paint::Scale {
                paint: child(1)?,
                scale: (f2(4)?, f2(4)?),
                center: Some((i16_(6)?, i16_(8)?)),
                var_index_base: var(10, odd)?,
            },
            24 | 25 => Paint::Rotate {
                paint: child(1)?,
                angle: f2(4)?,
                center: None,
                var_index_base: var(6, odd)?,
            },
            26 | 27 => Paint::Rotate {
                paint: child(1)?,
                angle: f2(4)?,
                center: Some((i16_(6)?, i16_(8)?)),
                var_index_base: var(10, odd)?,
            },
            28 | 29 => Paint::Skew {
                paint: child(1)?,
                angles: (f2(4)?, f2(6)?),
                center: None,
                var_index_base: var(8, odd)?,
            },
            30 | 31 => Paint::Skew {
                paint: child(1)?,
                angles: (f2(4)?, f2(6)?),
                center: Some((i16_(8)?, i16_(10)?)),
                var_index_base: var(12, odd)?,
            },
            32 => Paint::Composite {
                source: child(1)?,
                mode: be::u8_at(d, at(4)?)?,
                backdrop: child(5)?,
            },
            _ => return None,
        })
    }
}

/// Walks the version 1 paints from the base glyph paints and the layer
/// list: unknown formats and paints or color lines that do not fit,
/// layer ranges past the LayerList, palette indices and glyph ids out
/// of range, unknown composite modes, and graphs nested deeper than 64
/// or with more edges than the table has bytes
/// ([`Kind::WorkLimitExceeded`], once).
fn validate_paints(
    colr: &Colr<'_>,
    num_glyphs: Option<u32>,
    palette_entries: Option<u16>,
    emit: &mut dyn FnMut(usize, Kind),
) {
    const MAX_DEPTH: usize = 64;
    let data = colr.data();
    let roots = (0..colr.num_base_glyph_paints())
        .filter_map(|i| colr.base_glyph_paint_record(i).map(|(_, o)| o))
        .chain((0..colr.num_layer_paints()).filter_map(|i| colr.layer_paint(i)));
    let mut work = data.len();
    let mut stack: [(usize, usize); MAX_DEPTH] = [(0, 0); MAX_DEPTH];
    let mut limit_reported = false;
    let layers = to_u32(colr.num_layer_paints());
    let out_of_bounds = |emit: &mut dyn FnMut(usize, Kind), at: usize| {
        emit(
            at,
            Kind::OffsetOutOfBounds {
                offset: to_u32(at),
                length: 1,
                available: to_u32(data.len()),
            },
        );
    };
    for root in roots {
        let mut depth = 1usize;
        if let Some(s) = stack.first_mut() {
            *s = (root, 0);
        }
        while depth > 0 {
            let Some(&(offset, _)) = stack.get(depth.saturating_sub(1)) else {
                break;
            };
            depth = depth.saturating_sub(1);
            if work == 0 {
                if !limit_reported {
                    emit(offset, Kind::WorkLimitExceeded);
                }
                return;
            }
            work = work.saturating_sub(1);
            let Some(format) = colr.paint_format(offset) else {
                out_of_bounds(emit, offset);
                continue;
            };
            let Some(paint) = colr.paint(offset) else {
                if matches!(format, 1..=33) {
                    out_of_bounds(emit, offset);
                } else {
                    emit(
                        offset,
                        Kind::UnknownFormat {
                            found: u32::from(format),
                        },
                    );
                }
                continue;
            };
            let mut children: [Option<usize>; 2] = [None, None];
            let palette = |emit: &mut dyn FnMut(usize, Kind), p: u16| {
                if let Some(n) = palette_entries
                    && p != FOREGROUND
                    && p >= n
                {
                    emit(
                        offset,
                        Kind::ColorIndexOutOfRange {
                            index: u32::from(p),
                            count: u32::from(n),
                        },
                    );
                }
            };
            let glyph = |emit: &mut dyn FnMut(usize, Kind), g: u32| {
                if let Some(n) = num_glyphs
                    && g >= n
                {
                    emit(
                        offset,
                        Kind::GlyphIdOutOfRange {
                            glyph: g,
                            num_glyphs: n,
                        },
                    );
                }
            };
            let line_offset = || be::u24_at(data, offset.saturating_add(1)).map_or(0, to_usize);
            match paint {
                Paint::ColrLayers {
                    num_layers,
                    first_layer,
                } => {
                    let end = first_layer.saturating_add(u32::from(num_layers));
                    if end > layers {
                        emit(
                            offset,
                            Kind::LayerIndexOutOfRange {
                                index: end.saturating_sub(1),
                                count: layers,
                            },
                        );
                    }
                }
                Paint::Solid { palette_index, .. } => palette(emit, palette_index),
                Paint::LinearGradient { color_line, .. }
                | Paint::RadialGradient { color_line, .. }
                | Paint::SweepGradient { color_line, .. } => match color_line {
                    None if line_offset() != 0 => {
                        out_of_bounds(emit, offset.saturating_add(line_offset()));
                    }
                    Some(l) => {
                        if l.len() != usize::from(l.declared_len()) {
                            out_of_bounds(emit, offset.saturating_add(line_offset()));
                        }
                        (0..l.len()).filter_map(|i| l.stop(i)).for_each(|st| {
                            palette(emit, st.palette_index);
                        });
                    }
                    None => {}
                },
                Paint::Glyph { paint, glyph: g } => {
                    glyph(emit, g);
                    children[0] = paint;
                }
                Paint::ColrGlyph { glyph: g } => glyph(emit, u32::from(g)),
                Paint::Transform { paint, transform } => {
                    if transform.is_none() {
                        out_of_bounds(emit, offset);
                    }
                    children[0] = paint;
                }
                Paint::Translate { paint, .. }
                | Paint::Scale { paint, .. }
                | Paint::Rotate { paint, .. }
                | Paint::Skew { paint, .. } => children[0] = paint,
                Paint::Composite {
                    source,
                    mode,
                    backdrop,
                } => {
                    if mode > 27 {
                        emit(
                            offset,
                            Kind::UnknownFormat {
                                found: u32::from(mode),
                            },
                        );
                    }
                    children = [source, backdrop];
                }
            }
            for c in children.into_iter().flatten() {
                match stack.get_mut(depth) {
                    Some(slot) => {
                        *slot = (c, 0);
                        depth = depth.saturating_add(1);
                    }
                    None => {
                        if !limit_reported {
                            emit(offset, Kind::WorkLimitExceeded);
                            limit_reported = true;
                        }
                    }
                }
            }
        }
    }
}

impl Colr<'_> {
    /// Whether every paint, color line and transform that the version 1
    /// lists reach fits in the table, nested at most 64 deep, within
    /// HarfBuzz's sanitizer budget (64 operations per byte, at least
    /// 16384). HarfBuzz drops the version 1 lists when this fails; a
    /// table without them passes.
    pub fn v1_fits(&self) -> bool {
        const MAX_DEPTH: usize = 64;
        let data = self.data;
        let mut ops = data.len().saturating_mul(64).max(16384);
        // Room for two children at every level.
        let mut stack = [0usize; 2 * MAX_DEPTH + 2];
        let mut depths = [0usize; 2 * MAX_DEPTH + 2];
        let roots = (0..self.num_base_glyph_paints())
            .filter_map(|i| self.base_glyph_paint_record(i).map(|(_, o)| o))
            .chain((0..self.num_layer_paints()).filter_map(|i| self.layer_paint(i)));
        for root in roots {
            let mut top = 1usize;
            if let (Some(s), Some(d)) = (stack.first_mut(), depths.first_mut()) {
                *s = root;
                *d = 1;
            }
            while top > 0 {
                top = top.saturating_sub(1);
                let (Some(&offset), Some(&depth)) = (stack.get(top), depths.get(top)) else {
                    return false;
                };
                if ops == 0 || depth > MAX_DEPTH {
                    return false;
                }
                ops = ops.saturating_sub(1);
                let Some(format) = self.paint_format(offset) else {
                    return false;
                };
                if !matches!(format, 1..=33) {
                    continue;
                }
                let Some(paint) = self.paint(offset) else {
                    return false;
                };
                let line_off = || be::u24_at(data, offset.saturating_add(1)).unwrap_or(0);
                let children: [Option<usize>; 2] = match paint {
                    Paint::LinearGradient { color_line, .. }
                    | Paint::RadialGradient { color_line, .. }
                    | Paint::SweepGradient { color_line, .. } => {
                        match color_line {
                            None if line_off() != 0 => return false,
                            Some(l) if l.len() != usize::from(l.declared_len()) => return false,
                            _ => {}
                        }
                        [None, None]
                    }
                    Paint::Transform { paint, transform } => {
                        let t = be::u24_at(data, offset.saturating_add(4)).unwrap_or(0);
                        if transform.is_none() && t != 0 {
                            return false;
                        }
                        [paint, None]
                    }
                    Paint::Glyph { paint, .. }
                    | Paint::Translate { paint, .. }
                    | Paint::Scale { paint, .. }
                    | Paint::Rotate { paint, .. }
                    | Paint::Skew { paint, .. } => [paint, None],
                    Paint::Composite {
                        source, backdrop, ..
                    } => [source, backdrop],
                    _ => [None, None],
                };
                for c in children.into_iter().flatten() {
                    let (Some(s), Some(d)) = (stack.get_mut(top), depths.get_mut(top)) else {
                        return false;
                    };
                    *s = c;
                    *d = depth.saturating_add(1);
                    top = top.saturating_add(1);
                }
            }
        }
        true
    }
}
