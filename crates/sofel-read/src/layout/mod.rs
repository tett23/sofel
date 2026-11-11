//! The OpenType layout common table formats shared by `GDEF`, `GSUB` and
//! `GPOS` (ADR 0020): [`Coverage`] and [`ClassDef`] (formats 1 to 4, the
//! last two with 24-bit glyph ids from ISO/IEC 14496-22 5th edition),
//! [`ScriptList`], [`Script`], [`LangSys`], [`FeatureList`], [`Feature`],
//! [`LookupList`], [`Lookup`], the `GSUB` and `GPOS` header
//! ([`LayoutTable`], versions 1.0 to 1.2) and the sequence context
//! subtables shared by both ([`SequenceContext`],
//! [`ChainedSequenceContext`]).
//!
//! The layouts follow HarfBuzz 15.1.0 (`hb-ot-layout-common.hh`,
//! `hb-ot-layout-gsubgpos.hh`, `OT/Layout/Common/`), including the
//! 24-bit formats of its default build. Methods that follow an offset
//! return `Ok(None)` for a null offset and an error when the table it
//! points to cannot be read. Coverage and ClassDef tables held directly
//! by a subtable read as empty when their offset is null or they cannot
//! be read, as HarfBuzz reads a neutered offset as its Null object.
//! Searches use HarfBuzz's binary search over the big-endian bytes, so
//! that unsorted records give what HarfBuzz finds.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/chapter2>

pub(crate) mod context;
pub mod damage;
pub mod sanitize;
pub(crate) mod walk;

use core::cmp::Ordering;

use crate::be::{self, to_usize};
use crate::{GlyphId, ReadError, Tag};

pub use context::{
    ChainRule, ChainRuleSet, ChainRuleSets, ChainedSequenceContext, CoverageArray, GlyphArray,
    LookupRecords, Rule, RuleSet, RuleSets, SequenceContext, SequenceLookupRecord,
};

/// The bits of a lookup's `lookupFlag`.
pub mod lookup_flag {
    /// Cursive attachment goes from right to left.
    pub const RIGHT_TO_LEFT: u16 = 0x0001;
    /// Skip base glyphs.
    pub const IGNORE_BASE_GLYPHS: u16 = 0x0002;
    /// Skip ligatures.
    pub const IGNORE_LIGATURES: u16 = 0x0004;
    /// Skip marks.
    pub const IGNORE_MARKS: u16 = 0x0008;
    /// The lookup has a `markFilteringSet`.
    pub const USE_MARK_FILTERING_SET: u16 = 0x0010;
    /// Reserved bits; they should be 0.
    pub const RESERVED: u16 = 0x00E0;
    /// The mark attachment class to keep (0 for all).
    pub const MARK_ATTACHMENT_TYPE_MASK: u16 = 0xFF00;
}

/// An unsigned integer of `width` bytes (2, 3 or 4) at `at`.
pub(crate) fn uint(data: &[u8], at: usize, width: usize) -> Option<u32> {
    match width {
        2 => be::u16_at(data, at).map(u32::from),
        3 => be::u24_at(data, at),
        4 => be::u32_at(data, at),
        _ => None,
    }
}

/// The table at `offset` from the start of `data`, built by `make`:
/// `Ok(None)` for a null offset, [`ReadError::OutOfBounds`] when the
/// offset is past the data.
pub(crate) fn follow<'a, T>(
    data: &'a [u8],
    offset: u32,
    make: impl FnOnce(&'a [u8]) -> Result<T, ReadError>,
) -> Result<Option<T>, ReadError> {
    if offset == 0 {
        return Ok(None);
    }
    let tail = be::tail_at(data, to_usize(offset)).ok_or(ReadError::OutOfBounds)?;
    make(tail).map(Some)
}

/// A count of `count_width` bytes at `at` and the bytes of that many
/// elements of `size` bytes after it.
pub(crate) fn counted(
    data: &[u8],
    at: usize,
    count_width: usize,
    size: usize,
) -> Result<(usize, &[u8]), ReadError> {
    let header = at.saturating_add(count_width);
    let n = uint(data, at, count_width).ok_or(ReadError::truncated(header, data.len()))?;
    let n = to_usize(n);
    let len = n.saturating_mul(size);
    let bytes = be::slice_at(data, header, len)
        .ok_or(ReadError::truncated(header.saturating_add(len), data.len()))?;
    Ok((n, bytes))
}

/// Fails with [`ReadError::Truncated`] unless `data` has `len` bytes.
pub(crate) fn need(data: &[u8], len: usize) -> Result<(), ReadError> {
    if data.len() < len {
        Err(ReadError::truncated(len, data.len()))
    } else {
        Ok(())
    }
}

/// HarfBuzz's binary search (`hb_bsearch_impl`): the index of an entry
/// for which `cmp` (the key compared with entry `i`) is `Equal`, probing
/// the middle of `[min, max]` rounded down.
pub(crate) fn hb_bsearch(len: usize, cmp: impl Fn(usize) -> Option<Ordering>) -> Option<usize> {
    let (mut min, mut max) = (0usize, len.checked_sub(1)?);
    while min <= max {
        let mid = min.saturating_add(max) / 2;
        match cmp(mid)? {
            Ordering::Less => max = mid.checked_sub(1)?,
            Ordering::Greater => min = mid.saturating_add(1),
            Ordering::Equal => return Some(mid),
        }
    }
    None
}

/// Bytes `[i * size, (i + 1) * size)` of `data`.
pub(crate) fn record(data: &[u8], i: usize, size: usize) -> Option<&[u8]> {
    be::slice_at(data, i.checked_mul(size)?, size)
}

/// The table at the offset of `width` bytes read at `at` in `data`, read
/// as empty when the offset is null or the table cannot be built (a
/// neutered offset in HarfBuzz).
pub(crate) fn or_empty<'a, T>(
    data: &'a [u8],
    at: usize,
    width: usize,
    make: impl FnOnce(&'a [u8]) -> Result<T, ReadError>,
    empty: T,
) -> T {
    let offset = uint(data, at, width).unwrap_or(0);
    follow(data, offset, make).ok().flatten().unwrap_or(empty)
}

/// A view over a Coverage table: format 1 (glyph array) and 2 (range
/// records) with 16-bit glyph ids, 3 and 4 with 24-bit glyph ids, counts
/// and coverage indices.
#[derive(Clone, Copy, Debug)]
pub struct Coverage<'a> {
    data: &'a [u8],
    format: u16,
    records: &'a [u8],
    count: usize,
}

impl<'a> Coverage<'a> {
    /// A coverage of no glyphs (HarfBuzz's Null Coverage), format 0.
    pub const fn empty() -> Coverage<'static> {
        Coverage {
            data: &[],
            format: 0,
            records: &[],
            count: 0,
        }
    }

    /// A view over the table.
    ///
    /// Fails with [`ReadError::UnknownFormat`] for a format other than 1
    /// to 4, and with [`ReadError::Truncated`] if the records do not fit.
    pub fn new(data: &'a [u8]) -> Result<Coverage<'a>, ReadError> {
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        let (count_width, size) = match format {
            1 => (2, 2),
            2 => (2, 6),
            3 => (3, 3),
            4 => (3, 9),
            _ => return Err(ReadError::UnknownFormat(format)),
        };
        let (count, records) = counted(data, 2, count_width, size)?;
        Ok(Coverage {
            data,
            format,
            records,
            count,
        })
    }

    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `coverageFormat`: 1 to 4, or 0 for [`Coverage::empty`].
    pub fn format(&self) -> u16 {
        self.format
    }
    /// The number of glyph records (formats 1 and 3) or range records
    /// (formats 2 and 4).
    pub fn num_records(&self) -> usize {
        self.count
    }
    fn width(&self) -> usize {
        if self.format >= 3 { 3 } else { 2 }
    }
    fn record_size(&self) -> usize {
        match self.format {
            1 => 2,
            2 => 6,
            3 => 3,
            _ => 9,
        }
    }
    /// Glyph record `i` of a format 1 or 3 table.
    pub fn glyph(&self, i: usize) -> Option<GlyphId> {
        if self.format != 1 && self.format != 3 {
            return None;
        }
        let w = self.width();
        record(self.records, i, w).and_then(|r| uint(r, 0, w).map(GlyphId))
    }
    /// Range record `i` of a format 2 or 4 table: the first and last
    /// glyph and `startCoverageIndex`.
    pub fn range(&self, i: usize) -> Option<(GlyphId, GlyphId, u32)> {
        if self.format != 2 && self.format != 4 {
            return None;
        }
        let w = self.width();
        let r = record(self.records, i, self.record_size())?;
        Some((
            GlyphId(uint(r, 0, w)?),
            GlyphId(uint(r, w, w)?),
            uint(r, w.saturating_mul(2), w)?,
        ))
    }

    /// The coverage index of `glyph` (HarfBuzz `get_coverage`), by binary
    /// search over the records. In formats 2 and 4 the index is
    /// `startCoverageIndex + glyph - start` of the range found, unless
    /// the range starts after its end.
    pub fn get(&self, glyph: GlyphId) -> Option<u32> {
        let g = glyph.0;
        match self.format {
            1 | 3 => {
                let i = hb_bsearch(self.count, |i| self.glyph(i).map(|x| g.cmp(&x.0)))?;
                u32::try_from(i).ok()
            }
            2 | 4 => {
                let i = hb_bsearch(self.count, |i| {
                    self.range(i).map(|(first, last, _)| {
                        if g < first.0 {
                            Ordering::Less
                        } else if g <= last.0 {
                            Ordering::Equal
                        } else {
                            Ordering::Greater
                        }
                    })
                })?;
                let (first, last, start) = self.range(i)?;
                (first <= last).then(|| start.wrapping_add(g.wrapping_sub(first.0)))
            }
            _ => None,
        }
    }

    /// Every covered glyph as inclusive ranges in table order: one range
    /// per glyph in formats 1 and 3, one per range record in formats 2
    /// and 4 (skipping ranges that start after their end).
    pub fn ranges(&self) -> impl Iterator<Item = (GlyphId, GlyphId)> + 'a {
        let c = *self;
        (0..c.count).filter_map(move |i| match c.format {
            1 | 3 => c.glyph(i).map(|g| (g, g)),
            _ => c
                .range(i)
                .and_then(|(first, last, _)| (first <= last).then_some((first, last))),
        })
    }
}

/// A view over a ClassDef table: format 1 (class array) and 2 (range
/// records) with 16-bit glyph ids, 3 (24-bit glyph ids, counts and
/// classes) and 4 (24-bit glyph ids and count, 16-bit classes).
#[derive(Clone, Copy, Debug)]
pub struct ClassDef<'a> {
    data: &'a [u8],
    format: u16,
    start: u32,
    records: &'a [u8],
    count: usize,
}

impl<'a> ClassDef<'a> {
    /// A class definition that puts every glyph in class 0 (HarfBuzz's
    /// Null ClassDef), format 0.
    pub const fn empty() -> ClassDef<'static> {
        ClassDef {
            data: &[],
            format: 0,
            start: 0,
            records: &[],
            count: 0,
        }
    }

    /// A view over the table.
    ///
    /// Fails with [`ReadError::UnknownFormat`] for a format other than 1
    /// to 4, and with [`ReadError::Truncated`] if the records do not fit.
    pub fn new(data: &'a [u8]) -> Result<ClassDef<'a>, ReadError> {
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        let (start, (count, records)) = match format {
            1 => (
                be::u16_at(data, 2)
                    .map(u32::from)
                    .ok_or(ReadError::truncated(4, data.len()))?,
                counted(data, 4, 2, 2)?,
            ),
            2 => (0, counted(data, 2, 2, 6)?),
            3 => (
                be::u24_at(data, 2).ok_or(ReadError::truncated(5, data.len()))?,
                counted(data, 5, 3, 3)?,
            ),
            4 => (0, counted(data, 2, 3, 8)?),
            _ => return Err(ReadError::UnknownFormat(format)),
        };
        Ok(ClassDef {
            data,
            format,
            start,
            records,
            count,
        })
    }

    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `classFormat`: 1 to 4, or 0 for [`ClassDef::empty`].
    pub fn format(&self) -> u16 {
        self.format
    }
    /// `startGlyphID` of a format 1 or 3 table; 0 for other formats.
    pub fn start_glyph(&self) -> GlyphId {
        GlyphId(self.start)
    }
    /// The number of class values (formats 1 and 3) or range records
    /// (formats 2 and 4).
    pub fn num_records(&self) -> usize {
        self.count
    }
    /// Class value `i` of a format 1 or 3 table.
    pub fn class_value(&self, i: usize) -> Option<u32> {
        match self.format {
            1 => record(self.records, i, 2).and_then(|r| uint(r, 0, 2)),
            3 => record(self.records, i, 3).and_then(|r| uint(r, 0, 3)),
            _ => None,
        }
    }
    /// Range record `i` of a format 2 or 4 table: the first and last
    /// glyph and the class.
    pub fn range(&self, i: usize) -> Option<(GlyphId, GlyphId, u32)> {
        let (w, size) = match self.format {
            2 => (2, 6),
            4 => (3, 8),
            _ => return None,
        };
        let r = record(self.records, i, size)?;
        Some((
            GlyphId(uint(r, 0, w)?),
            GlyphId(uint(r, w, w)?),
            uint(r, w.saturating_mul(2), 2)?,
        ))
    }

    /// The class of `glyph`; 0 when it is not listed (HarfBuzz
    /// `get_class`). Format 3 classes are 24 bits wide, so the result is
    /// a `u32`.
    pub fn get(&self, glyph: GlyphId) -> u32 {
        match self.format {
            1 | 3 => self
                .class_value(to_usize(glyph.0.wrapping_sub(self.start)))
                .unwrap_or(0),
            2 | 4 => hb_bsearch(self.count, |i| {
                self.range(i).map(|(first, last, _)| {
                    if glyph < first {
                        Ordering::Less
                    } else if glyph <= last {
                        Ordering::Equal
                    } else {
                        Ordering::Greater
                    }
                })
            })
            .and_then(|i| self.range(i))
            .map_or(0, |(_, _, class)| class),
            _ => 0,
        }
    }
}

/// A view over a list of tagged records (ScriptList, FeatureList): a
/// count and records of a tag and a 16-bit offset from the list.
#[derive(Clone, Copy, Debug)]
struct TagRecords<'a> {
    data: &'a [u8],
    records: &'a [u8],
    count: usize,
}

impl<'a> TagRecords<'a> {
    fn new(data: &'a [u8], at: usize) -> Result<TagRecords<'a>, ReadError> {
        let (count, records) = counted(data, at, 2, 6)?;
        Ok(TagRecords {
            data,
            records,
            count,
        })
    }
    fn record(&self, i: usize) -> Option<(Tag, u16)> {
        let r = record(self.records, i, 6)?;
        Some((Tag(*be::array_at::<4>(r, 0)?), be::u16_at(r, 4)?))
    }
    fn tag(&self, i: usize) -> Option<Tag> {
        self.record(i).map(|r| r.0)
    }
    fn find(&self, tag: Tag) -> Option<usize> {
        hb_bsearch(self.count, |i| self.tag(i).map(|t| tag.cmp(&t)))
    }
    fn get<T>(
        &self,
        i: usize,
        make: impl FnOnce(&'a [u8]) -> Result<T, ReadError>,
    ) -> Result<Option<T>, ReadError> {
        let (_, offset) = self.record(i).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, u32::from(offset), make)
    }
}

/// A view over a ScriptList table.
#[derive(Clone, Copy, Debug)]
pub struct ScriptList<'a>(TagRecords<'a>);

impl<'a> ScriptList<'a> {
    /// A view over the count and the ScriptRecords. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<ScriptList<'a>, ReadError> {
        TagRecords::new(data, 0).map(ScriptList)
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.0.data
    }
    /// `scriptCount`.
    pub fn len(&self) -> usize {
        self.0.count
    }
    /// Whether there are no scripts.
    pub fn is_empty(&self) -> bool {
        self.0.count == 0
    }
    /// The tag of record `index`.
    pub fn tag(&self, index: usize) -> Option<Tag> {
        self.0.tag(index)
    }
    /// The offset of the Script table of record `index` from the start of
    /// the list.
    pub fn script_offset(&self, index: usize) -> Option<u16> {
        self.0.record(index).map(|r| r.1)
    }
    /// The Script table of record `index`; [`ReadError::OutOfBounds`]
    /// for an index past the records.
    pub fn script(&self, index: usize) -> Result<Option<Script<'a>>, ReadError> {
        self.0.get(index, Script::new)
    }
    /// The index of the record for `tag`, by HarfBuzz's binary search
    /// (`RecordArrayOf::find_index`).
    pub fn find(&self, tag: Tag) -> Option<usize> {
        self.0.find(tag)
    }
}

/// A view over a Script table.
#[derive(Clone, Copy, Debug)]
pub struct Script<'a>(TagRecords<'a>);

impl<'a> Script<'a> {
    /// A view over the default offset, the count and the LangSysRecords.
    /// Fails with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<Script<'a>, ReadError> {
        TagRecords::new(data, 2).map(Script)
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.0.data
    }
    /// `defaultLangSysOffset`; 0 for none.
    pub fn default_lang_sys_offset(&self) -> u16 {
        be::u16_at(self.0.data, 0).unwrap_or(0)
    }
    /// The default language system.
    pub fn default_lang_sys(&self) -> Result<Option<LangSys<'a>>, ReadError> {
        follow(
            self.0.data,
            u32::from(self.default_lang_sys_offset()),
            LangSys::new,
        )
    }
    /// `langSysCount`.
    pub fn num_lang_sys(&self) -> usize {
        self.0.count
    }
    /// The tag of LangSysRecord `index`.
    pub fn lang_sys_tag(&self, index: usize) -> Option<Tag> {
        self.0.tag(index)
    }
    /// The offset of the LangSys table of record `index` from the start
    /// of the Script table.
    pub fn lang_sys_offset(&self, index: usize) -> Option<u16> {
        self.0.record(index).map(|r| r.1)
    }
    /// The LangSys table of record `index`; [`ReadError::OutOfBounds`]
    /// for an index past the records.
    pub fn lang_sys(&self, index: usize) -> Result<Option<LangSys<'a>>, ReadError> {
        self.0.get(index, LangSys::new)
    }
    /// The index of the LangSysRecord for `tag`, by HarfBuzz's binary
    /// search.
    pub fn find_lang_sys(&self, tag: Tag) -> Option<usize> {
        self.0.find(tag)
    }
}

/// A view over a LangSys table.
#[derive(Clone, Copy, Debug)]
pub struct LangSys<'a> {
    data: &'a [u8],
    indices: &'a [u8],
}

impl<'a> LangSys<'a> {
    /// A view over the header and the feature indices. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<LangSys<'a>, ReadError> {
        let (_, indices) = counted(data, 4, 2, 2)?;
        Ok(LangSys { data, indices })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `requiredFeatureIndex`; 0xFFFF for none.
    pub fn required_feature_index(&self) -> u16 {
        be::u16_at(self.data, 2).unwrap_or(0xFFFF)
    }
    /// `featureIndexCount`.
    pub fn num_features(&self) -> usize {
        self.indices.len() / 2
    }
    /// Feature index `i`.
    pub fn feature_index(&self, i: usize) -> Option<u16> {
        be::u16_at(self.indices, i.checked_mul(2)?)
    }
}

/// A view over a FeatureList table.
#[derive(Clone, Copy, Debug)]
pub struct FeatureList<'a>(TagRecords<'a>);

impl<'a> FeatureList<'a> {
    /// A view over the count and the FeatureRecords. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<FeatureList<'a>, ReadError> {
        TagRecords::new(data, 0).map(FeatureList)
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.0.data
    }
    /// `featureCount`.
    pub fn len(&self) -> usize {
        self.0.count
    }
    /// Whether there are no features.
    pub fn is_empty(&self) -> bool {
        self.0.count == 0
    }
    /// The tag of record `index`.
    pub fn tag(&self, index: usize) -> Option<Tag> {
        self.0.tag(index)
    }
    /// The offset of the Feature table of record `index` from the start
    /// of the list.
    pub fn feature_offset(&self, index: usize) -> Option<u16> {
        self.0.record(index).map(|r| r.1)
    }
    /// The Feature table of record `index`; [`ReadError::OutOfBounds`]
    /// for an index past the records.
    pub fn feature(&self, index: usize) -> Result<Option<Feature<'a>>, ReadError> {
        self.0.get(index, Feature::new)
    }
}

/// A view over a Feature table.
#[derive(Clone, Copy, Debug)]
pub struct Feature<'a> {
    data: &'a [u8],
    indices: &'a [u8],
}

impl<'a> Feature<'a> {
    /// A view over the header and the lookup indices. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<Feature<'a>, ReadError> {
        let (_, indices) = counted(data, 2, 2, 2)?;
        Ok(Feature { data, indices })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `featureParamsOffset` from the start of the Feature table; 0 for
    /// none. HarfBuzz 15.1.0 reads it relative to the Feature table for
    /// every tag, `size` included.
    pub fn feature_params_offset(&self) -> u16 {
        be::u16_at(self.data, 0).unwrap_or(0)
    }
    /// `lookupIndexCount`.
    pub fn num_lookups(&self) -> usize {
        self.indices.len() / 2
    }
    /// Lookup index `i`.
    pub fn lookup_index(&self, i: usize) -> Option<u16> {
        be::u16_at(self.indices, i.checked_mul(2)?)
    }
}

/// A view over a LookupList table: 16-bit offsets, or 32-bit offsets in
/// the `lookupList2` of a version 1.2 `GSUB` or `GPOS`.
#[derive(Clone, Copy, Debug)]
pub struct LookupList<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
    width: usize,
}

impl<'a> LookupList<'a> {
    /// A view over the count and the offsets, 32 bits wide when `wide`.
    /// Fails with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8], wide: bool) -> Result<LookupList<'a>, ReadError> {
        let width = if wide { 4 } else { 2 };
        let (_, offsets) = counted(data, 0, 2, width)?;
        Ok(LookupList {
            data,
            offsets,
            width,
        })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// Whether the offsets are 32 bits wide.
    pub fn is_wide(&self) -> bool {
        self.width == 4
    }
    /// `lookupCount`.
    pub fn len(&self) -> usize {
        self.offsets.len().checked_div(self.width).unwrap_or(0)
    }
    /// Whether there are no lookups.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The offset of lookup `index` from the start of the list.
    pub fn lookup_offset(&self, index: usize) -> Option<u32> {
        uint(self.offsets, index.checked_mul(self.width)?, self.width)
    }
    /// Lookup `index`; [`ReadError::OutOfBounds`] for an index past the
    /// offsets.
    pub fn lookup(&self, index: usize) -> Result<Option<Lookup<'a>>, ReadError> {
        let offset = self.lookup_offset(index).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, offset, Lookup::new)
    }
}

/// A view over a Lookup table.
#[derive(Clone, Copy, Debug)]
pub struct Lookup<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
}

impl<'a> Lookup<'a> {
    /// A view over the header, the subtable offsets and
    /// `markFilteringSet` (when the flag asks for it). Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<Lookup<'a>, ReadError> {
        need(data, 6)?;
        let (count, offsets) = counted(data, 4, 2, 2)?;
        let flag = be::u16_at(data, 2).unwrap_or(0);
        if flag & lookup_flag::USE_MARK_FILTERING_SET != 0 {
            need(data, count.saturating_mul(2).saturating_add(8))?;
        }
        Ok(Lookup { data, offsets })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `lookupType` as stored (7 for a `GSUB` extension, 9 for `GPOS`).
    pub fn lookup_type(&self) -> u16 {
        be::u16_at(self.data, 0).unwrap_or(0)
    }
    /// `lookupFlag`.
    pub fn flag(&self) -> u16 {
        be::u16_at(self.data, 2).unwrap_or(0)
    }
    /// `markFilteringSet`, when the flag has
    /// [`lookup_flag::USE_MARK_FILTERING_SET`].
    pub fn mark_filtering_set(&self) -> Option<u16> {
        if self.flag() & lookup_flag::USE_MARK_FILTERING_SET == 0 {
            return None;
        }
        be::u16_at(self.data, self.offsets.len().saturating_add(6))
    }
    /// `subTableCount`.
    pub fn num_subtables(&self) -> usize {
        self.offsets.len() / 2
    }
    /// The offset of subtable `i` from the start of the lookup.
    pub fn subtable_offset(&self, i: usize) -> Option<u16> {
        be::u16_at(self.offsets, i.checked_mul(2)?)
    }
    /// The bytes from the start of subtable `i` to the end of the data;
    /// `None` for a null offset or one past the data.
    pub fn subtable_data(&self, i: usize) -> Option<&'a [u8]> {
        match self.subtable_offset(i)? {
            0 => None,
            o => be::tail_at(self.data, usize::from(o)),
        }
    }

    /// The type an extension subtable `i` points to, read as HarfBuzz
    /// reads it after sanitizing: 0 for a null or short subtable, a
    /// format other than 1, or one that points to another extension.
    pub(crate) fn extension_type(&self, i: usize, extension: u16) -> u16 {
        let Some(d) = self.subtable_data(i) else {
            return 0;
        };
        match (be::u16_at(d, 0), be::u16_at(d, 2), d.len() >= 8) {
            (Some(1), Some(t), true) if t != extension => t,
            _ => 0,
        }
    }

    /// The type of the lookup after following extensions: for an
    /// extension lookup (`lookupType` equal to `extension`), the type of
    /// its first subtable as HarfBuzz reads it (0 when it cannot be
    /// read); otherwise `lookupType`.
    pub fn effective_type(&self, extension: u16) -> u16 {
        let t = self.lookup_type();
        if t == extension {
            self.extension_type(0, extension)
        } else {
            t
        }
    }

    /// Whether all subtables of an extension lookup (`lookupType` equal
    /// to `extension`) point to the same type, as HarfBuzz's sanitizer
    /// requires; when they do not, HarfBuzz drops the whole lookup. Always
    /// true for other lookups.
    pub fn extension_types_agree(&self, extension: u16) -> bool {
        if self.lookup_type() != extension {
            return true;
        }
        let first = self.extension_type(0, extension);
        (1..self.num_subtables()).all(|i| self.extension_type(i, extension) == first)
    }
}

/// `GSUB` and `GPOS` version 1.0.
pub const VERSION_1_0: u32 = 0x0001_0000;
/// Version 1.1, which adds the FeatureVariations offset.
pub const VERSION_1_1: u32 = 0x0001_0001;
/// Version 1.2 (HarfBuzz, beyond 64K glyphs), which adds 32-bit offsets
/// to the lists.
pub const VERSION_1_2: u32 = 0x0001_0002;

/// A view over the `GSUB` or `GPOS` header, versions 1.0, 1.1 (the
/// FeatureVariations offset is available as a checked view) and 1.2 (HarfBuzz's
/// `scriptList2`, `featureList2` and `lookupList2`, used when not null).
#[derive(Clone, Copy, Debug)]
pub struct LayoutTable<'a> {
    data: &'a [u8],
    version: u32,
}

impl<'a> LayoutTable<'a> {
    /// A view over the header.
    ///
    /// Fails with [`ReadError::UnknownVersion`] if `majorVersion` is not
    /// 1, and with [`ReadError::Truncated`] if the header (10 bytes, 14
    /// from version 1.1 on, 26 from 1.2 on) does not fit.
    pub fn new(data: &'a [u8]) -> Result<LayoutTable<'a>, ReadError> {
        let version = be::u32_at(data, 0).ok_or(ReadError::truncated(4, data.len()))?;
        if version >> 16 != 1 {
            return Err(ReadError::UnknownVersion(version));
        }
        need(data, header_size(version))?;
        Ok(LayoutTable { data, version })
    }
    /// The bytes of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `majorVersion` and `minorVersion` as one value.
    pub fn version(&self) -> u32 {
        self.version
    }
    fn offset(&self, narrow: usize, wide: usize) -> u32 {
        let w = if self.version >= VERSION_1_2 {
            be::u32_at(self.data, wide).unwrap_or(0)
        } else {
            0
        };
        if w != 0 {
            w
        } else {
            be::u16_at(self.data, narrow).map_or(0, u32::from)
        }
    }
    /// The offset of the ScriptList: `scriptList2` when version 1.2 has
    /// one, `scriptListOffset` otherwise.
    pub fn script_list_offset(&self) -> u32 {
        self.offset(4, 14)
    }
    /// The offset of the FeatureList, chosen like
    /// [`LayoutTable::script_list_offset`].
    pub fn feature_list_offset(&self) -> u32 {
        self.offset(6, 18)
    }
    /// The offset of the LookupList, chosen like
    /// [`LayoutTable::script_list_offset`].
    pub fn lookup_list_offset(&self) -> u32 {
        self.offset(8, 22)
    }
    /// Whether the lookup list is `lookupList2`, whose offsets are 32
    /// bits wide.
    pub fn has_wide_lookup_list(&self) -> bool {
        self.version >= VERSION_1_2 && be::u32_at(self.data, 22).is_some_and(|o| o != 0)
    }
    /// The script list.
    pub fn script_list(&self) -> Result<Option<ScriptList<'a>>, ReadError> {
        follow(self.data, self.script_list_offset(), ScriptList::new)
    }
    /// The feature list.
    pub fn feature_list(&self) -> Result<Option<FeatureList<'a>>, ReadError> {
        follow(self.data, self.feature_list_offset(), FeatureList::new)
    }
    /// The lookup list.
    pub fn lookup_list(&self) -> Result<Option<LookupList<'a>>, ReadError> {
        let wide = self.has_wide_lookup_list();
        follow(self.data, self.lookup_list_offset(), |d| {
            LookupList::new(d, wide)
        })
    }
    /// `featureVariationsOffset` (version 1.1 on); `None` before.
    pub fn feature_variations_offset(&self) -> Option<u32> {
        if self.version >= VERSION_1_1 {
            be::u32_at(self.data, 10)
        } else {
            None
        }
    }

    /// The checked FeatureVariations view, absent before version 1.1 or
    /// when the offset is zero.
    pub fn feature_variations(
        &self,
    ) -> Result<Option<crate::feature_variations::FeatureVariations<'a>>, ReadError> {
        let Some(offset) = self.feature_variations_offset() else {
            return Ok(None);
        };
        if offset == 0 {
            return Ok(None);
        }
        let data = be::tail_at(self.data, be::to_usize(offset)).ok_or(ReadError::OutOfBounds)?;
        crate::feature_variations::FeatureVariations::new(data).map(Some)
    }
}

/// The size of the `GSUB` or `GPOS` header of `version`.
pub(crate) fn header_size(version: u32) -> usize {
    if version >= VERSION_1_2 {
        26
    } else if version >= VERSION_1_1 {
        14
    } else {
        10
    }
}
