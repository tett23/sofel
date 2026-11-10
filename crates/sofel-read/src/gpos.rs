//! The `GPOS` (glyph positioning) table (ADR 0020).
//!
//! The header, lists and lookups are those of [`crate::layout`];
//! [`subtable`] reads a lookup's subtable as a [`PosSubtable`], following
//! an extension (type 9) to its type. Every lookup type 1 to 9 is read in
//! all formats of HarfBuzz 15.1.0, the 24-bit ones of ISO/IEC 14496-22
//! 5th edition included (single and pair adjustment 3 and 4; cursive and
//! the three mark attachments 2; context 4 to 6; chained context 4 and
//! 5).
//!
//! Device offsets in a [`ValueRecord`] are from the subtable that holds
//! it, except in pair adjustment format 1 and 3, where they are from the
//! PairSet (HarfBuzz's `base`). Anchor offsets are from the subtable
//! (cursive), the MarkArray, or the AnchorMatrix, as in HarfBuzz.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/gpos>

use crate::be;
use crate::device::Device;
use crate::layout::context::{class_def_at, coverage_at};
use crate::layout::walk::{Layout, Walker, layout};
use crate::layout::{
    ChainedSequenceContext, ClassDef, Coverage, Lookup, SequenceContext, counted, follow,
    hb_bsearch, need, or_empty, record, uint,
};
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

/// The `GPOS` tag.
pub const TAG: Tag = Tag::new(b"GPOS");

/// Lookup type 1, single adjustment.
pub const SINGLE: u16 = 1;
/// Lookup type 2, pair adjustment.
pub const PAIR: u16 = 2;
/// Lookup type 3, cursive attachment.
pub const CURSIVE: u16 = 3;
/// Lookup type 4, mark-to-base attachment.
pub const MARK_BASE: u16 = 4;
/// Lookup type 5, mark-to-ligature attachment.
pub const MARK_LIG: u16 = 5;
/// Lookup type 6, mark-to-mark attachment.
pub const MARK_MARK: u16 = 6;
/// Lookup type 7, contextual positioning.
pub const CONTEXT: u16 = 7;
/// Lookup type 8, chained contexts positioning.
pub const CHAIN_CONTEXT: u16 = 8;
/// Lookup type 9, extension positioning.
pub const EXTENSION: u16 = 9;

/// A `valueFormat`: which fields a [`ValueRecord`] has.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ValueFormat(pub u16);

impl ValueFormat {
    /// Horizontal adjustment for placement.
    pub const X_PLACEMENT: u16 = 0x0001;
    /// Vertical adjustment for placement.
    pub const Y_PLACEMENT: u16 = 0x0002;
    /// Horizontal adjustment for advance.
    pub const X_ADVANCE: u16 = 0x0004;
    /// Vertical adjustment for advance.
    pub const Y_ADVANCE: u16 = 0x0008;
    /// Device table for horizontal placement.
    pub const X_PLACEMENT_DEVICE: u16 = 0x0010;
    /// Device table for vertical placement.
    pub const Y_PLACEMENT_DEVICE: u16 = 0x0020;
    /// Device table for horizontal advance.
    pub const X_ADVANCE_DEVICE: u16 = 0x0040;
    /// Device table for vertical advance.
    pub const Y_ADVANCE_DEVICE: u16 = 0x0080;
    /// The Device table bits.
    pub const DEVICES: u16 = 0x00F0;
    /// Reserved bits (0x0F00 was used for multiple-master fonts); they
    /// take no space in a record, as in HarfBuzz.
    pub const RESERVED: u16 = 0xFF00;

    /// The number of 16-bit values in a record: the set bits among the
    /// low eight (HarfBuzz `get_len`).
    pub fn len(&self) -> usize {
        usize::try_from((self.0 & 0xFF).count_ones()).unwrap_or(0)
    }
    /// Whether a record has no values.
    pub fn is_empty(&self) -> bool {
        self.0 & 0xFF == 0
    }
    /// The size of a record in bytes.
    pub fn size(&self) -> usize {
        self.len().saturating_mul(2)
    }
    /// Whether a record has Device offsets.
    pub fn has_device(&self) -> bool {
        self.0 & Self::DEVICES != 0
    }
}

/// A ValueRecord with its raw values (0 for fields the format does not
/// have) and Device tables (`None` for fields the format does not have,
/// null offsets, and tables that cannot be read, which HarfBuzz
/// neuters).
#[derive(Clone, Copy, Debug, Default)]
pub struct ValueRecord<'a> {
    /// Horizontal adjustment for placement, in font units.
    pub x_placement: i16,
    /// Vertical adjustment for placement.
    pub y_placement: i16,
    /// Horizontal adjustment for advance.
    pub x_advance: i16,
    /// Vertical adjustment for advance.
    pub y_advance: i16,
    /// Device table for horizontal placement.
    pub x_placement_device: Option<Device<'a>>,
    /// Device table for vertical placement.
    pub y_placement_device: Option<Device<'a>>,
    /// Device table for horizontal advance.
    pub x_advance_device: Option<Device<'a>>,
    /// Device table for vertical advance.
    pub y_advance_device: Option<Device<'a>>,
    /// The format the record was read with.
    pub format: ValueFormat,
}

impl<'a> ValueRecord<'a> {
    /// Reads a record of `format` at the start of `values`, with Device
    /// offsets from the start of `base`. Missing bytes read as 0.
    pub fn read(values: &[u8], base: &'a [u8], format: ValueFormat) -> ValueRecord<'a> {
        let mut r = ValueRecord {
            format,
            ..ValueRecord::default()
        };
        let mut at = 0usize;
        let mut next = |bit: u16| -> Option<u16> {
            if format.0 & bit == 0 {
                return None;
            }
            let v = be::u16_at(values, at).unwrap_or(0);
            at = at.saturating_add(2);
            Some(v)
        };
        let signed = |v: Option<u16>| v.map_or(0, |v| i16::from_be_bytes(v.to_be_bytes()));
        r.x_placement = signed(next(ValueFormat::X_PLACEMENT));
        r.y_placement = signed(next(ValueFormat::Y_PLACEMENT));
        r.x_advance = signed(next(ValueFormat::X_ADVANCE));
        r.y_advance = signed(next(ValueFormat::Y_ADVANCE));
        let device =
            |o: Option<u16>| o.and_then(|o| follow(base, u32::from(o), Device::new).ok().flatten());
        r.x_placement_device = device(next(ValueFormat::X_PLACEMENT_DEVICE));
        r.y_placement_device = device(next(ValueFormat::Y_PLACEMENT_DEVICE));
        r.x_advance_device = device(next(ValueFormat::X_ADVANCE_DEVICE));
        r.y_advance_device = device(next(ValueFormat::Y_ADVANCE_DEVICE));
        r
    }
}

/// An Anchor table.
#[derive(Clone, Copy, Debug)]
pub enum Anchor<'a> {
    /// Format 1: design units only.
    Format1 {
        /// Horizontal value, in font units.
        x: i16,
        /// Vertical value.
        y: i16,
    },
    /// Format 2: design units plus a contour point (which HarfBuzz uses
    /// only when there is a ppem).
    Format2 {
        /// Horizontal value, in font units.
        x: i16,
        /// Vertical value.
        y: i16,
        /// The contour point index.
        point: u16,
    },
    /// Format 3: design units plus Device tables (`None` for null offsets
    /// and tables that cannot be read).
    Format3 {
        /// Horizontal value, in font units.
        x: i16,
        /// Vertical value.
        y: i16,
        /// Device table for the x coordinate.
        x_device: Option<Device<'a>>,
        /// Device table for the y coordinate.
        y_device: Option<Device<'a>>,
    },
}

impl<'a> Anchor<'a> {
    /// A view over the table.
    ///
    /// Fails with [`ReadError::UnknownFormat`] for a format other than 1
    /// to 3 (HarfBuzz reads such an anchor as found at (0, 0), unlike a
    /// null offset or a table that does not fit), and with
    /// [`ReadError::Truncated`] if it does not fit.
    pub fn new(data: &'a [u8]) -> Result<Anchor<'a>, ReadError> {
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        let size = match format {
            1 => 6,
            2 => 8,
            3 => 10,
            _ => return Err(ReadError::UnknownFormat(format)),
        };
        need(data, size)?;
        let x = be::i16_at(data, 2).unwrap_or(0);
        let y = be::i16_at(data, 4).unwrap_or(0);
        let device = |at| {
            let o = be::u16_at(data, at).map_or(0, u32::from);
            follow(data, o, Device::new).ok().flatten()
        };
        Ok(match format {
            1 => Anchor::Format1 { x, y },
            2 => Anchor::Format2 {
                x,
                y,
                point: be::u16_at(data, 6).unwrap_or(0),
            },
            _ => Anchor::Format3 {
                x,
                y,
                x_device: device(6),
                y_device: device(8),
            },
        })
    }
}

/// A `GPOS` subtable, after following an extension.
#[derive(Clone, Copy, Debug)]
pub enum PosSubtable<'a> {
    /// Single adjustment format 1 and 3: one value for every covered
    /// glyph.
    Single {
        /// The glyphs to adjust.
        coverage: Coverage<'a>,
        /// The value.
        value: ValueRecord<'a>,
    },
    /// Single adjustment format 2 and 4: a value per coverage index.
    SingleArray {
        /// The glyphs to adjust.
        coverage: Coverage<'a>,
        /// The values.
        values: ValueRecords<'a>,
    },
    /// Pair adjustment format 1 and 3: pair sets by first glyph.
    PairGlyphs {
        /// The first glyphs.
        coverage: Coverage<'a>,
        /// The format of the first glyph's values.
        format1: ValueFormat,
        /// The format of the second glyph's values.
        format2: ValueFormat,
        /// The pair sets, by coverage index.
        pair_sets: PairSets<'a>,
    },
    /// Pair adjustment format 2 and 4: values by class pair.
    PairClasses {
        /// The first glyphs.
        coverage: Coverage<'a>,
        /// The format of the first glyph's values.
        format1: ValueFormat,
        /// The format of the second glyph's values.
        format2: ValueFormat,
        /// The classes of the first glyphs.
        class_def1: ClassDef<'a>,
        /// The classes of the second glyphs.
        class_def2: ClassDef<'a>,
        /// `class1Count`.
        class1_count: u16,
        /// `class2Count`.
        class2_count: u16,
        /// The values.
        values: PairClassValues<'a>,
    },
    /// Cursive attachment, formats 1 and 2.
    Cursive {
        /// The glyphs with entry or exit anchors.
        coverage: Coverage<'a>,
        /// The anchors, by coverage index.
        entry_exits: EntryExits<'a>,
    },
    /// Mark-to-base attachment, formats 1 and 2.
    MarkBase {
        /// The marks.
        mark_coverage: Coverage<'a>,
        /// The bases.
        base_coverage: Coverage<'a>,
        /// The number of mark classes.
        class_count: u16,
        /// The marks' classes and anchors.
        marks: MarkArray<'a>,
        /// The bases' anchors, a row per base.
        bases: AnchorMatrix<'a>,
    },
    /// Mark-to-ligature attachment, formats 1 and 2.
    MarkLig {
        /// The marks.
        mark_coverage: Coverage<'a>,
        /// The ligatures.
        ligature_coverage: Coverage<'a>,
        /// The number of mark classes.
        class_count: u16,
        /// The marks' classes and anchors.
        marks: MarkArray<'a>,
        /// The ligatures' anchors.
        ligatures: LigatureArray<'a>,
    },
    /// Mark-to-mark attachment, formats 1 and 2.
    MarkMark {
        /// The attaching marks.
        mark1_coverage: Coverage<'a>,
        /// The marks attached to.
        mark2_coverage: Coverage<'a>,
        /// The number of mark classes.
        class_count: u16,
        /// The attaching marks' classes and anchors.
        marks: MarkArray<'a>,
        /// The anchors of the marks attached to, a row per mark.
        mark2s: AnchorMatrix<'a>,
    },
    /// Contextual positioning.
    Context(SequenceContext<'a>),
    /// Chained contexts positioning.
    ChainContext(ChainedSequenceContext<'a>),
}

/// The ValueRecords of single adjustment format 2 or 4.
#[derive(Clone, Copy, Debug)]
pub struct ValueRecords<'a> {
    base: &'a [u8],
    values: &'a [u8],
    count: usize,
    format: ValueFormat,
}

impl<'a> ValueRecords<'a> {
    /// The number of records.
    pub fn len(&self) -> usize {
        self.count
    }
    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// Record `i`.
    pub fn get(&self, i: usize) -> Option<ValueRecord<'a>> {
        if i >= self.count {
            return None;
        }
        let size = self.format.size();
        let at = i.checked_mul(size)?;
        Some(ValueRecord::read(
            be::tail_at(self.values, at)?,
            self.base,
            self.format,
        ))
    }
}

/// The offsets to PairSet tables of pair adjustment format 1 or 3.
#[derive(Clone, Copy, Debug)]
pub struct PairSets<'a> {
    base: &'a [u8],
    offsets: &'a [u8],
    width: usize,
    format1: ValueFormat,
    format2: ValueFormat,
}

impl<'a> PairSets<'a> {
    /// The number of pair sets.
    pub fn len(&self) -> usize {
        self.offsets.len().checked_div(self.width).unwrap_or(0)
    }
    /// Whether there are no pair sets.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The offset of pair set `i` from the start of the subtable.
    pub fn offset(&self, i: usize) -> Option<u32> {
        uint(self.offsets, i.checked_mul(self.width)?, self.width)
    }
    /// Pair set `i`; [`ReadError::OutOfBounds`] for an index past the
    /// offsets.
    pub fn get(&self, i: usize) -> Result<Option<PairSet<'a>>, ReadError> {
        let o = self.offset(i).ok_or(ReadError::OutOfBounds)?;
        let (w, f1, f2) = (self.width, self.format1, self.format2);
        follow(self.base, o, |d| PairSet::new(d, w, f1, f2))
    }
}

/// A view over a PairSet table: a count (16 or 24 bits) and records of a
/// second glyph (16 or 24 bits) and two ValueRecords.
#[derive(Clone, Copy, Debug)]
pub struct PairSet<'a> {
    data: &'a [u8],
    records: &'a [u8],
    count: usize,
    width: usize,
    format1: ValueFormat,
    format2: ValueFormat,
}

impl<'a> PairSet<'a> {
    /// A view over a set whose count and glyph ids are `width` bytes
    /// wide. Fails with [`ReadError::Truncated`] if it does not fit.
    pub fn new(
        data: &'a [u8],
        width: usize,
        format1: ValueFormat,
        format2: ValueFormat,
    ) -> Result<PairSet<'a>, ReadError> {
        let size = width
            .saturating_add(format1.size())
            .saturating_add(format2.size());
        let (count, records) = counted(data, 0, width, size)?;
        Ok(PairSet {
            data,
            records,
            count,
            width,
            format1,
            format2,
        })
    }
    fn size(&self) -> usize {
        self.width
            .saturating_add(self.format1.size())
            .saturating_add(self.format2.size())
    }
    /// The number of records.
    pub fn len(&self) -> usize {
        self.count
    }
    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// The second glyph of record `i`.
    pub fn second_glyph(&self, i: usize) -> Option<GlyphId> {
        let r = record(self.records, i, self.size())?;
        uint(r, 0, self.width).map(GlyphId)
    }
    /// Record `i`: the second glyph and the values of both glyphs, with
    /// Device offsets from the start of the PairSet.
    pub fn record(&self, i: usize) -> Option<(GlyphId, ValueRecord<'a>, ValueRecord<'a>)> {
        let r = record(self.records, i, self.size())?;
        let g = uint(r, 0, self.width)?;
        let v1 = be::tail_at(r, self.width)?;
        let v2 = be::tail_at(v1, self.format1.size())?;
        Some((
            GlyphId(g),
            ValueRecord::read(v1, self.data, self.format1),
            ValueRecord::read(v2, self.data, self.format2),
        ))
    }
    /// The values for `second`, by HarfBuzz's binary search.
    pub fn find(&self, second: GlyphId) -> Option<(ValueRecord<'a>, ValueRecord<'a>)> {
        let i = hb_bsearch(self.count, |i| self.second_glyph(i).map(|g| second.cmp(&g)))?;
        self.record(i).map(|(_, a, b)| (a, b))
    }
}

/// The matrix of value pairs of pair adjustment format 2 or 4.
#[derive(Clone, Copy, Debug)]
pub struct PairClassValues<'a> {
    base: &'a [u8],
    values: &'a [u8],
    class1_count: u16,
    class2_count: u16,
    format1: ValueFormat,
    format2: ValueFormat,
}

impl<'a> PairClassValues<'a> {
    /// The values for a pair of classes; `None` when a class is not below
    /// its count.
    pub fn get(&self, class1: u16, class2: u16) -> Option<(ValueRecord<'a>, ValueRecord<'a>)> {
        if class1 >= self.class1_count || class2 >= self.class2_count {
            return None;
        }
        let s1 = self.format1.size();
        let size = s1.saturating_add(self.format2.size());
        let index = usize::from(class1)
            .checked_mul(usize::from(self.class2_count))?
            .checked_add(usize::from(class2))?;
        let v1 = be::tail_at(self.values, index.checked_mul(size)?)?;
        let v2 = be::tail_at(v1, s1)?;
        Some((
            ValueRecord::read(v1, self.base, self.format1),
            ValueRecord::read(v2, self.base, self.format2),
        ))
    }
}

/// The entry and exit anchor offsets of cursive attachment.
#[derive(Clone, Copy, Debug)]
pub struct EntryExits<'a> {
    base: &'a [u8],
    records: &'a [u8],
    width: usize,
}

/// A followed anchor offset.
pub type AnchorResult<'a> = Result<Option<Anchor<'a>>, ReadError>;

impl<'a> EntryExits<'a> {
    /// The number of records.
    pub fn len(&self) -> usize {
        self.records
            .len()
            .checked_div(self.width.saturating_mul(2))
            .unwrap_or(0)
    }
    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    /// The entry and exit anchors of record `i`.
    pub fn get(&self, i: usize) -> Option<(AnchorResult<'a>, AnchorResult<'a>)> {
        let r = record(self.records, i, self.width.saturating_mul(2))?;
        let entry = uint(r, 0, self.width)?;
        let exit = uint(r, self.width, self.width)?;
        Some((
            follow(self.base, entry, Anchor::new),
            follow(self.base, exit, Anchor::new),
        ))
    }
}

/// A view over a MarkArray table: a count (16 or 24 bits) and records of
/// a class and an anchor offset (16 or 24 bits) from the MarkArray.
#[derive(Clone, Copy, Debug)]
pub struct MarkArray<'a> {
    data: &'a [u8],
    records: &'a [u8],
    width: usize,
}

impl<'a> MarkArray<'a> {
    /// A MarkArray with no records (HarfBuzz's Null MarkArray).
    pub const fn empty() -> MarkArray<'static> {
        MarkArray {
            data: &[],
            records: &[],
            width: 2,
        }
    }
    /// A view over an array whose count and offsets are `width` bytes
    /// wide. Fails with [`ReadError::Truncated`] if it does not fit.
    pub fn new(data: &'a [u8], width: usize) -> Result<MarkArray<'a>, ReadError> {
        let (_, records) = counted(data, 0, width, width.saturating_add(2))?;
        Ok(MarkArray {
            data,
            records,
            width,
        })
    }
    /// The number of records.
    pub fn len(&self) -> usize {
        self.records
            .len()
            .checked_div(self.width.saturating_add(2))
            .unwrap_or(0)
    }
    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    /// The class and anchor of record `i`.
    pub fn get(&self, i: usize) -> Option<(u16, AnchorResult<'a>)> {
        let r = record(self.records, i, self.width.saturating_add(2))?;
        let class = be::u16_at(r, 0)?;
        let o = uint(r, 2, self.width)?;
        Some((class, follow(self.data, o, Anchor::new)))
    }
}

/// A view over an AnchorMatrix table (BaseArray, Mark2Array,
/// LigatureAttach): a row count and `rows * class_count` anchor offsets
/// from the matrix.
#[derive(Clone, Copy, Debug)]
pub struct AnchorMatrix<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
    rows: usize,
    cols: u16,
    width: usize,
}

impl<'a> AnchorMatrix<'a> {
    /// A matrix with no rows (HarfBuzz's Null AnchorMatrix).
    pub const fn empty() -> AnchorMatrix<'static> {
        AnchorMatrix {
            data: &[],
            offsets: &[],
            rows: 0,
            cols: 0,
            width: 2,
        }
    }
    /// A view over a matrix with `cols` columns (the class count), whose
    /// row count is `row_width` bytes wide and whose offsets are `width`
    /// bytes wide. Fails with [`ReadError::Truncated`] if it does not fit.
    pub fn new(
        data: &'a [u8],
        cols: u16,
        row_width: usize,
        width: usize,
    ) -> Result<AnchorMatrix<'a>, ReadError> {
        let rows =
            to_rows(uint(data, 0, row_width)).ok_or(ReadError::truncated(row_width, data.len()))?;
        let n = rows.saturating_mul(usize::from(cols));
        let len = n.saturating_mul(width);
        let offsets = be::slice_at(data, row_width, len).ok_or(ReadError::truncated(
            row_width.saturating_add(len),
            data.len(),
        ))?;
        Ok(AnchorMatrix {
            data,
            offsets,
            rows,
            cols,
            width,
        })
    }
    /// The number of rows.
    pub fn rows(&self) -> usize {
        self.rows
    }
    /// The number of columns (the class count).
    pub fn cols(&self) -> u16 {
        self.cols
    }
    /// The offset at `row` and `class` from the start of the matrix.
    pub fn offset(&self, row: usize, class: u16) -> Option<u32> {
        if row >= self.rows || class >= self.cols {
            return None;
        }
        let i = row
            .checked_mul(usize::from(self.cols))?
            .checked_add(usize::from(class))?;
        uint(self.offsets, i.checked_mul(self.width)?, self.width)
    }
    /// The anchor at `row` and `class`; `Ok(None)` for a null offset or a
    /// row or class outside the matrix (HarfBuzz: not found).
    pub fn get(&self, row: usize, class: u16) -> AnchorResult<'a> {
        follow(self.data, self.offset(row, class).unwrap_or(0), Anchor::new)
    }
}

fn to_rows(v: Option<u32>) -> Option<usize> {
    v.map(be::to_usize)
}

/// A view over a LigatureArray table: a count (16 or 24 bits) and
/// offsets (16 or 24 bits) to LigatureAttach matrices, one per ligature.
#[derive(Clone, Copy, Debug)]
pub struct LigatureArray<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
    cols: u16,
    width: usize,
}

impl<'a> LigatureArray<'a> {
    /// A LigatureArray with no ligatures (HarfBuzz's Null).
    pub const fn empty() -> LigatureArray<'static> {
        LigatureArray {
            data: &[],
            offsets: &[],
            cols: 0,
            width: 2,
        }
    }
    /// A view over an array whose count and offsets are `width` bytes
    /// wide, for `cols` mark classes. Fails with [`ReadError::Truncated`]
    /// if it does not fit.
    pub fn new(data: &'a [u8], cols: u16, width: usize) -> Result<LigatureArray<'a>, ReadError> {
        let (_, offsets) = counted(data, 0, width, width)?;
        Ok(LigatureArray {
            data,
            offsets,
            cols,
            width,
        })
    }
    /// The number of ligatures.
    pub fn len(&self) -> usize {
        self.offsets.len().checked_div(self.width).unwrap_or(0)
    }
    /// Whether there are no ligatures.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The LigatureAttach matrix of ligature `i` (a row per component);
    /// [`ReadError::OutOfBounds`] for an index past the offsets.
    pub fn get(&self, i: usize) -> Result<Option<AnchorMatrix<'a>>, ReadError> {
        let o = uint(self.offsets, i.saturating_mul(self.width), self.width)
            .ok_or(ReadError::OutOfBounds)?;
        let (cols, w) = (self.cols, self.width);
        follow(self.data, o, |d| AnchorMatrix::new(d, cols, 2, w))
    }
}

/// `(offset width of 16- or 32-bit offsets, 16- or 24-bit width)` of a
/// format that is `narrow` or `wide`.
fn widths(format: u16, narrow: u16, wide: u16) -> Option<(usize, usize)> {
    if format == narrow {
        Some((2, 2))
    } else if format == wide {
        Some((4, 3))
    } else {
        None
    }
}

/// Reads the subtable at the start of `data` as lookup type `ty` (not an
/// extension).
pub fn parse(ty: u16, data: &[u8]) -> Result<PosSubtable<'_>, ReadError> {
    let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
    let unknown = ReadError::UnknownFormat(format);
    let vf = |at| ValueFormat(be::u16_at(data, at).unwrap_or(0));
    match ty {
        SINGLE => match format {
            1 | 3 => {
                let cw = if format == 1 { 2usize } else { 4 };
                need(data, cw.saturating_add(4))?;
                let f = vf(cw.saturating_add(2));
                let at = cw.saturating_add(4);
                need(data, at.saturating_add(f.size()))?;
                Ok(PosSubtable::Single {
                    coverage: coverage_at(data, 2, cw),
                    value: ValueRecord::read(be::tail_at(data, at).unwrap_or(&[]), data, f),
                })
            }
            2 | 4 => {
                let (cw, nw) = if format == 2 {
                    (2usize, 2usize)
                } else {
                    (4, 3)
                };
                need(data, cw.saturating_add(4))?;
                let f = vf(cw.saturating_add(2));
                let (count, values) = counted(data, cw.saturating_add(4), nw, f.size())?;
                Ok(PosSubtable::SingleArray {
                    coverage: coverage_at(data, 2, cw),
                    values: ValueRecords {
                        base: data,
                        values,
                        count,
                        format: f,
                    },
                })
            }
            _ => Err(unknown),
        },
        PAIR => match format {
            1 | 3 => {
                let (cw, w) = if format == 1 {
                    (2usize, 2usize)
                } else {
                    (4, 3)
                };
                need(data, cw.saturating_add(6))?;
                let (f1, f2) = (vf(cw.saturating_add(2)), vf(cw.saturating_add(4)));
                let (_, offsets) = counted(data, cw.saturating_add(6), w, w)?;
                Ok(PosSubtable::PairGlyphs {
                    coverage: coverage_at(data, 2, cw),
                    format1: f1,
                    format2: f2,
                    pair_sets: PairSets {
                        base: data,
                        offsets,
                        width: w,
                        format1: f1,
                        format2: f2,
                    },
                })
            }
            2 | 4 => {
                let cw = if format == 2 { 2usize } else { 4 };
                let at = cw.saturating_mul(3).saturating_add(10);
                need(data, at)?;
                let (f1, f2) = (vf(cw.saturating_add(2)), vf(cw.saturating_add(4)));
                let counts = cw.saturating_mul(3).saturating_add(6);
                let c1 = be::u16_at(data, counts).unwrap_or(0);
                let c2 = be::u16_at(data, counts.saturating_add(2)).unwrap_or(0);
                let len = usize::from(c1)
                    .saturating_mul(usize::from(c2))
                    .saturating_mul(f1.size().saturating_add(f2.size()));
                let values = be::slice_at(data, at, len)
                    .ok_or(ReadError::truncated(at.saturating_add(len), data.len()))?;
                Ok(PosSubtable::PairClasses {
                    coverage: coverage_at(data, 2, cw),
                    format1: f1,
                    format2: f2,
                    class_def1: class_def_at(data, cw.saturating_add(6), cw),
                    class_def2: class_def_at(data, cw.saturating_mul(2).saturating_add(6), cw),
                    class1_count: c1,
                    class2_count: c2,
                    values: PairClassValues {
                        base: data,
                        values,
                        class1_count: c1,
                        class2_count: c2,
                        format1: f1,
                        format2: f2,
                    },
                })
            }
            _ => Err(unknown),
        },
        CURSIVE => {
            let (cw, w) = widths(format, 1, 2).ok_or(unknown)?;
            need(data, cw.saturating_add(2))?;
            let (_, records) = counted(data, cw.saturating_add(2), w, w.saturating_mul(2))?;
            Ok(PosSubtable::Cursive {
                coverage: coverage_at(data, 2, cw),
                entry_exits: EntryExits {
                    base: data,
                    records,
                    width: w,
                },
            })
        }
        MARK_BASE | MARK_LIG | MARK_MARK => {
            let (cw, w) = widths(format, 1, 2).ok_or(unknown)?;
            let class_at = cw.saturating_mul(2).saturating_add(2);
            need(
                data,
                class_at
                    .saturating_add(2)
                    .saturating_add(cw.saturating_mul(2)),
            )?;
            let classes = be::u16_at(data, class_at).unwrap_or(0);
            let marks_at = class_at.saturating_add(2);
            let second_at = marks_at.saturating_add(cw);
            let marks = or_empty(
                data,
                marks_at,
                cw,
                |d| MarkArray::new(d, w),
                MarkArray::empty(),
            );
            let c1 = coverage_at(data, 2, cw);
            let c2 = coverage_at(data, cw.saturating_add(2), cw);
            Ok(match ty {
                MARK_BASE => PosSubtable::MarkBase {
                    mark_coverage: c1,
                    base_coverage: c2,
                    class_count: classes,
                    marks,
                    bases: or_empty(
                        data,
                        second_at,
                        cw,
                        |d| AnchorMatrix::new(d, classes, w, w),
                        AnchorMatrix::empty(),
                    ),
                },
                MARK_LIG => PosSubtable::MarkLig {
                    mark_coverage: c1,
                    ligature_coverage: c2,
                    class_count: classes,
                    marks,
                    ligatures: or_empty(
                        data,
                        second_at,
                        cw,
                        |d| LigatureArray::new(d, classes, w),
                        LigatureArray::empty(),
                    ),
                },
                _ => PosSubtable::MarkMark {
                    mark1_coverage: c1,
                    mark2_coverage: c2,
                    class_count: classes,
                    marks,
                    mark2s: or_empty(
                        data,
                        second_at,
                        cw,
                        |d| AnchorMatrix::new(d, classes, 2, w),
                        AnchorMatrix::empty(),
                    ),
                },
            })
        }
        CONTEXT => SequenceContext::new(data).map(PosSubtable::Context),
        CHAIN_CONTEXT => ChainedSequenceContext::new(data).map(PosSubtable::ChainContext),
        _ => Err(ReadError::InvalidValue),
    }
}

/// Subtable `i` of `lookup`, following an extension (type 9) to its type.
///
/// Fails like [`crate::gsub::subtable`].
pub fn subtable<'a>(lookup: &Lookup<'a>, i: usize) -> Result<PosSubtable<'a>, ReadError> {
    let data = lookup.subtable_data(i).ok_or(ReadError::OutOfBounds)?;
    match lookup.lookup_type() {
        EXTENSION => {
            let (ty, target) = crate::gsub::extension_target(data, EXTENSION)?;
            parse(ty, target)
        }
        ty => parse(ty, data),
    }
}

/// The lookup type after following extensions, from the first subtable.
pub fn effective_type(lookup: &Lookup<'_>) -> u16 {
    lookup.effective_type(EXTENSION)
}

/// Checks a `GPOS` table: everything [`crate::gsub::validate`] checks for
/// `GSUB`, and in the subtables reserved ValueFormat bits (Warning),
/// PairSet records whose second glyphs do not ascend, unknown Anchor
/// formats and Device tables that do not fit, and arrays and matrices
/// whose length is not the number of glyphs of their Coverage table.
pub fn validate(data: &[u8], mark_sets: Option<u16>, report: &mut dyn FnMut(Diagnostic)) {
    layout(
        TAG,
        data,
        Layout {
            extension: EXTENSION,
            max_type: CHAIN_CONTEXT,
            mark_sets,
            subtable: &mut validate_subtable,
        },
        report,
    );
}

fn value_format(w: &mut Walker<'_, '_>, at: usize) -> ValueFormat {
    let f = ValueFormat(u16::try_from(w.uint(at, 2)).unwrap_or(0));
    if f.0 & ValueFormat::RESERVED != 0 {
        w.emit(
            at,
            Kind::ReservedBitsSet {
                found: u32::from(f.0),
            },
        );
    }
    f
}

/// Validates the Device offsets of a ValueRecord of `format` at `at`,
/// from `base`.
fn value_devices(w: &mut Walker<'_, '_>, at: usize, base: usize, format: ValueFormat) {
    if !format.has_device() {
        return;
    }
    let mut p = at;
    for bit in 0..8u16 {
        let flag = 1u16 << bit;
        if format.0 & flag == 0 {
            continue;
        }
        if flag & ValueFormat::DEVICES != 0 {
            w.device(p, base);
        }
        p = p.saturating_add(2);
    }
}

fn anchor(w: &mut Walker<'_, '_>, field: usize, base: usize, width: usize) {
    if let Some((at, Anchor::Format3 { .. })) = w.follow(field, base, width, Anchor::new) {
        w.device(at.saturating_add(6), at);
        w.device(at.saturating_add(8), at);
    }
}

fn anchor_matrix(
    w: &mut Walker<'_, '_>,
    field: usize,
    base: usize,
    cw: usize,
    cols: u16,
    row_width: usize,
    width: usize,
) -> Option<usize> {
    let (at, m) = w.follow(field, base, cw, |d| {
        AnchorMatrix::new(d, cols, row_width, width)
    })?;
    let n = m.rows().saturating_mul(usize::from(cols));
    if !w.spend(n) {
        return Some(m.rows());
    }
    for i in 0..n {
        let f = at
            .saturating_add(row_width)
            .saturating_add(i.saturating_mul(width));
        anchor(w, f, at, width);
    }
    Some(m.rows())
}

fn validate_subtable(w: &mut Walker<'_, '_>, field: usize, at: usize, ty: u16) {
    match ty {
        CONTEXT => return w.context(field, at),
        CHAIN_CONTEXT => return w.chain_context(field, at),
        _ => {}
    }
    let view = match parse(ty, w.tail(at)) {
        Ok(v) => v,
        Err(e) => return w.error(field, at, e),
    };
    let format = w.uint(at, 2);
    let narrow = match ty {
        SINGLE | PAIR => format <= 2,
        _ => format == 1,
    };
    let (cw, nw) = if narrow { (2usize, 2usize) } else { (4, 3) };
    let covered = w.coverage(at.saturating_add(2), at, cw);
    let next = at.saturating_add(2).saturating_add(cw);
    match view {
        PosSubtable::Single { value, .. } => {
            value_format(w, next);
            value_devices(w, next.saturating_add(2), at, value.format);
        }
        PosSubtable::SingleArray { values, .. } => {
            let f = value_format(w, next);
            let count_at = next.saturating_add(2);
            w.count_matches(count_at, covered, values.len());
            if f.has_device() && w.spend(values.len()) {
                for i in 0..values.len() {
                    let v = count_at
                        .saturating_add(nw)
                        .saturating_add(i.saturating_mul(f.size()));
                    value_devices(w, v, at, f);
                }
            }
        }
        PosSubtable::PairGlyphs {
            format1,
            format2,
            pair_sets,
            ..
        } => {
            value_format(w, next);
            value_format(w, next.saturating_add(2));
            let count_at = next.saturating_add(4);
            w.count_matches(count_at, covered, pair_sets.len());
            for i in 0..pair_sets.len() {
                let f = count_at
                    .saturating_add(nw)
                    .saturating_add(i.saturating_mul(nw));
                let Some((set_at, set)) =
                    w.follow(f, at, nw, |d| PairSet::new(d, nw, format1, format2))
                else {
                    continue;
                };
                if !w.spend(set.len()) {
                    return;
                }
                let size = nw
                    .saturating_add(format1.size())
                    .saturating_add(format2.size());
                let mut previous = None;
                for j in 0..set.len() {
                    let r = set_at
                        .saturating_add(nw)
                        .saturating_add(j.saturating_mul(size));
                    let g = set.second_glyph(j);
                    if previous.is_some_and(|p| g <= p) {
                        w.emit(
                            r,
                            Kind::GlyphsNotSorted {
                                index: crate::be::to_u32(j),
                            },
                        );
                    }
                    previous = Some(g);
                    let v1 = r.saturating_add(nw);
                    value_devices(w, v1, set_at, format1);
                    value_devices(w, v1.saturating_add(format1.size()), set_at, format2);
                }
            }
        }
        PosSubtable::PairClasses {
            format1,
            format2,
            class1_count,
            class2_count,
            ..
        } => {
            value_format(w, next);
            value_format(w, next.saturating_add(2));
            w.class_def(next.saturating_add(4), at, cw);
            w.class_def(next.saturating_add(4).saturating_add(cw), at, cw);
            if !format1.has_device() && !format2.has_device() {
                return;
            }
            let n = usize::from(class1_count).saturating_mul(usize::from(class2_count));
            if !w.spend(n) {
                return;
            }
            let size = format1.size().saturating_add(format2.size());
            let first = at.saturating_add(cw.saturating_mul(3)).saturating_add(10);
            for i in 0..n {
                let v1 = first.saturating_add(i.saturating_mul(size));
                value_devices(w, v1, at, format1);
                value_devices(w, v1.saturating_add(format1.size()), at, format2);
            }
        }
        PosSubtable::Cursive { entry_exits, .. } => {
            w.count_matches(next, covered, entry_exits.len());
            if !w.spend(entry_exits.len()) {
                return;
            }
            for i in 0..entry_exits.len() {
                let r = next
                    .saturating_add(nw)
                    .saturating_add(i.saturating_mul(nw.saturating_mul(2)));
                anchor(w, r, at, nw);
                anchor(w, r.saturating_add(nw), at, nw);
            }
        }
        PosSubtable::MarkBase { class_count, .. }
        | PosSubtable::MarkLig { class_count, .. }
        | PosSubtable::MarkMark { class_count, .. } => {
            let second = w.coverage(next, at, cw);
            let marks_field = next.saturating_add(cw).saturating_add(2);
            if let Some((ma, marks)) = w.follow(marks_field, at, cw, |d| MarkArray::new(d, nw)) {
                w.count_matches(ma, covered, marks.len());
                if w.spend(marks.len()) {
                    for i in 0..marks.len() {
                        let r = ma
                            .saturating_add(nw)
                            .saturating_add(i.saturating_mul(nw.saturating_add(2)));
                        anchor(w, r.saturating_add(2), ma, nw);
                    }
                }
            }
            let second_field = marks_field.saturating_add(cw);
            match ty {
                MARK_LIG => {
                    let Some((la, ligs)) = w.follow(second_field, at, cw, |d| {
                        LigatureArray::new(d, class_count, nw)
                    }) else {
                        return;
                    };
                    w.count_matches(la, second, ligs.len());
                    for i in 0..ligs.len() {
                        let f = la.saturating_add(nw).saturating_add(i.saturating_mul(nw));
                        anchor_matrix(w, f, la, nw, class_count, 2, nw);
                    }
                }
                _ => {
                    let row_width = if ty == MARK_BASE { nw } else { 2 };
                    let rows = anchor_matrix(w, second_field, at, cw, class_count, row_width, nw);
                    if let Some(rows) = rows {
                        let matrix_at = at.saturating_add(be::to_usize(w.uint(second_field, cw)));
                        w.count_matches(matrix_at, second, rows);
                    }
                }
            }
        }
        PosSubtable::Context(_) | PosSubtable::ChainContext(_) => {}
    }
}
