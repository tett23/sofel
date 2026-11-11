//! The `BASE` (baseline) table, versions 1.0 and 1.1 (ADR 0017).
//!
//! The table has a horizontal and a vertical [`Axis`]. Each axis has a
//! [`BaseTagList`] (the baselines it defines) and a [`BaseScriptList`];
//! each [`BaseScript`] has [`BaseValues`] (one [`BaseCoord`] per
//! baseline tag), a default [`MinMax`] and a [`MinMax`] per language
//! system, and each [`MinMax`] has a minimum and a maximum
//! [`BaseCoord`] and the same per feature. A [`BaseCoord`] of format 3
//! has a [`Device`] table.
//!
//! Methods that follow an offset return `Ok(None)` for a null offset
//! and an error when the table it points to cannot be read. The `find`
//! methods search the records with HarfBuzz's binary search
//! (`hb_bsearch_impl`), so that unsorted records give what HarfBuzz
//! finds.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/base>

use crate::be::{self, to_u32};
use crate::device::Device;
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

/// The `BASE` tag.
pub const TAG: Tag = Tag::new(b"BASE");
/// Version 1.0.
pub const VERSION_1_0: u32 = 0x0001_0000;
/// Version 1.1, which adds the offset to an ItemVariationStore.
pub const VERSION_1_1: u32 = 0x0001_0001;

/// The table at `offset` from the start of `data`, built by `make`:
/// `Ok(None)` for a null offset, [`ReadError::OutOfBounds`] when the
/// offset is past the data.
fn follow<'a, T>(
    data: &'a [u8],
    offset: u16,
    make: impl FnOnce(&'a [u8]) -> Result<T, ReadError>,
) -> Result<Option<T>, ReadError> {
    if offset == 0 {
        return Ok(None);
    }
    let tail = be::tail_at(data, usize::from(offset)).ok_or(ReadError::OutOfBounds)?;
    make(tail).map(Some)
}

/// HarfBuzz's binary search (`hb_bsearch_impl`): the index of an entry
/// whose tag equals `key`, probing the middle of `[min, max]` rounded
/// down.
fn hb_bsearch(len: usize, key: Tag, tag_at: impl Fn(usize) -> Option<Tag>) -> Option<usize> {
    let (mut min, mut max) = (0usize, len.checked_sub(1)?);
    while min <= max {
        let mid = min.saturating_add(max) / 2;
        let t = tag_at(mid)?;
        match key.cmp(&t) {
            core::cmp::Ordering::Less => max = mid.checked_sub(1)?,
            core::cmp::Ordering::Greater => min = mid.saturating_add(1),
            core::cmp::Ordering::Equal => return Some(mid),
        }
    }
    None
}

fn tag_of(b: &[u8; 4]) -> Tag {
    Tag(*b)
}

fn u16_of(a: u8, b: u8) -> u16 {
    u16::from_be_bytes([a, b])
}

/// A count at `at` and `count` records of `N` bytes after it.
fn records<const N: usize>(data: &[u8], at: usize) -> Result<&[[u8; N]], ReadError> {
    let header = at.saturating_add(2);
    let n = be::u16_at(data, at).ok_or(ReadError::truncated(header, data.len()))?;
    let needed = header.saturating_add(usize::from(n).saturating_mul(N));
    be::chunks_at::<N>(data, header, usize::from(n)).ok_or(ReadError::truncated(needed, data.len()))
}

/// A view over the `BASE` table header.
#[derive(Clone, Copy, Debug)]
pub struct Base<'a> {
    data: &'a [u8],
    version: u32,
}

impl<'a> Base<'a> {
    /// A view over the header.
    ///
    /// Fails with [`ReadError::UnknownVersion`] if `majorVersion` is not
    /// 1, and with [`ReadError::Truncated`] if the header (12 bytes from
    /// version 1.1 on, 8 before) does not fit.
    pub fn new(data: &'a [u8]) -> Result<Base<'a>, ReadError> {
        let h = data
            .first_chunk::<8>()
            .ok_or(ReadError::truncated(8, data.len()))?;
        let version = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
        if version >> 16 != 1 {
            return Err(ReadError::UnknownVersion(version));
        }
        if version >= VERSION_1_1 && data.len() < 12 {
            return Err(ReadError::truncated(12, data.len()));
        }
        Ok(Base { data, version })
    }

    /// The bytes of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `majorVersion` and `minorVersion` as one value (0x00010000 or
    /// 0x00010001; later minor versions read as 1.1).
    pub fn version(&self) -> u32 {
        self.version
    }
    /// The offset of the horizontal axis (`horizAxisOffset`); 0 for none.
    pub fn horiz_axis_offset(&self) -> u16 {
        be::u16_at(self.data, 4).unwrap_or(0)
    }
    /// The offset of the vertical axis (`vertAxisOffset`); 0 for none.
    pub fn vert_axis_offset(&self) -> u16 {
        be::u16_at(self.data, 6).unwrap_or(0)
    }
    /// Opens the optional ItemVariationStore (ADR 0027).
    /// Null offsets return `None`; malformed non-null stores return an error.
    pub fn variation_store(
        &self,
    ) -> Result<Option<crate::item_variation::ItemVariationStore<'a>>, ReadError> {
        let offset = self.item_var_store_offset().unwrap_or(0);
        if offset == 0 {
            return Ok(None);
        }
        let data = crate::be::tail_at(self.data, crate::be::to_usize(offset))
            .ok_or(ReadError::OutOfBounds)?;
        crate::item_variation::ItemVariationStore::new(data).map(Some)
    }

    /// `itemVarStoreOffset` (from version 1.1 on); `None` before.
    pub fn item_var_store_offset(&self) -> Option<u32> {
        if self.version >= VERSION_1_1 {
            be::u32_at(self.data, 8)
        } else {
            None
        }
    }
    /// The horizontal axis.
    pub fn horiz_axis(&self) -> Result<Option<Axis<'a>>, ReadError> {
        follow(self.data, self.horiz_axis_offset(), Axis::new)
    }
    /// The vertical axis.
    pub fn vert_axis(&self) -> Result<Option<Axis<'a>>, ReadError> {
        follow(self.data, self.vert_axis_offset(), Axis::new)
    }
    /// The vertical axis when `vertical`, the horizontal one otherwise.
    pub fn axis(&self, vertical: bool) -> Result<Option<Axis<'a>>, ReadError> {
        if vertical {
            self.vert_axis()
        } else {
            self.horiz_axis()
        }
    }
}

/// A view over an Axis table.
#[derive(Clone, Copy, Debug)]
pub struct Axis<'a> {
    data: &'a [u8],
}

impl<'a> Axis<'a> {
    /// A view over the two offsets. Fails with [`ReadError::Truncated`]
    /// if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<Axis<'a>, ReadError> {
        if data.len() < 4 {
            return Err(ReadError::truncated(4, data.len()));
        }
        Ok(Axis { data })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `baseTagListOffset`; 0 for none.
    pub fn base_tag_list_offset(&self) -> u16 {
        be::u16_at(self.data, 0).unwrap_or(0)
    }
    /// `baseScriptListOffset`; 0 for none.
    pub fn base_script_list_offset(&self) -> u16 {
        be::u16_at(self.data, 2).unwrap_or(0)
    }
    /// The baseline tags.
    pub fn base_tag_list(&self) -> Result<Option<BaseTagList<'a>>, ReadError> {
        follow(self.data, self.base_tag_list_offset(), BaseTagList::new)
    }
    /// The scripts.
    pub fn base_script_list(&self) -> Result<Option<BaseScriptList<'a>>, ReadError> {
        follow(
            self.data,
            self.base_script_list_offset(),
            BaseScriptList::new,
        )
    }
}

/// A view over a BaseTagList table: the baseline tags of an axis.
#[derive(Clone, Copy, Debug)]
pub struct BaseTagList<'a> {
    tags: &'a [[u8; 4]],
}

impl<'a> BaseTagList<'a> {
    /// A view over the count and the tags. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<BaseTagList<'a>, ReadError> {
        Ok(BaseTagList {
            tags: records::<4>(data, 0)?,
        })
    }
    /// `baseTagCount`.
    pub fn len(&self) -> usize {
        self.tags.len()
    }
    /// Whether there are no tags.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
    /// Tag `index`.
    pub fn tag(&self, index: usize) -> Option<Tag> {
        self.tags.get(index).map(tag_of)
    }
    /// The index of `tag`, by HarfBuzz's binary search.
    pub fn find(&self, tag: Tag) -> Option<usize> {
        hb_bsearch(self.len(), tag, |i| self.tag(i))
    }
}

/// A view over a BaseScriptList table.
#[derive(Clone, Copy, Debug)]
pub struct BaseScriptList<'a> {
    data: &'a [u8],
    records: &'a [[u8; 6]],
}

impl<'a> BaseScriptList<'a> {
    /// A view over the count and the records. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<BaseScriptList<'a>, ReadError> {
        Ok(BaseScriptList {
            data,
            records: records::<6>(data, 0)?,
        })
    }
    /// `baseScriptCount`.
    pub fn len(&self) -> usize {
        self.records.len()
    }
    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    /// Record `index`: its script tag and the offset of its BaseScript
    /// table from the start of the list.
    pub fn record(&self, index: usize) -> Option<(Tag, u16)> {
        self.records
            .get(index)
            .map(|r| (Tag([r[0], r[1], r[2], r[3]]), u16_of(r[4], r[5])))
    }
    /// The BaseScript table of record `index`;
    /// [`ReadError::OutOfBounds`] for an index past the records.
    pub fn script(&self, index: usize) -> Result<Option<BaseScript<'a>>, ReadError> {
        let (_, offset) = self.record(index).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, offset, BaseScript::new)
    }
    /// The index of the record for `tag`, by HarfBuzz's binary search.
    pub fn find(&self, tag: Tag) -> Option<usize> {
        hb_bsearch(self.len(), tag, |i| self.record(i).map(|(t, _)| t))
    }
}

/// A view over a BaseScript table.
#[derive(Clone, Copy, Debug)]
pub struct BaseScript<'a> {
    data: &'a [u8],
    records: &'a [[u8; 6]],
}

impl<'a> BaseScript<'a> {
    /// A view over the offsets, the count and the BaseLangSys records.
    /// Fails with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<BaseScript<'a>, ReadError> {
        Ok(BaseScript {
            data,
            records: records::<6>(data, 4)?,
        })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `baseValuesOffset`; 0 for none.
    pub fn base_values_offset(&self) -> u16 {
        be::u16_at(self.data, 0).unwrap_or(0)
    }
    /// `defaultMinMaxOffset`; 0 for none.
    pub fn default_min_max_offset(&self) -> u16 {
        be::u16_at(self.data, 2).unwrap_or(0)
    }
    /// The baseline values.
    pub fn base_values(&self) -> Result<Option<BaseValues<'a>>, ReadError> {
        follow(self.data, self.base_values_offset(), BaseValues::new)
    }
    /// The default extents.
    pub fn default_min_max(&self) -> Result<Option<MinMax<'a>>, ReadError> {
        follow(self.data, self.default_min_max_offset(), MinMax::new)
    }
    /// `baseLangSysCount`.
    pub fn num_lang_sys(&self) -> usize {
        self.records.len()
    }
    /// BaseLangSys record `index`: its language system tag and the
    /// offset of its MinMax table from the start of the BaseScript table.
    pub fn lang_sys_record(&self, index: usize) -> Option<(Tag, u16)> {
        self.records
            .get(index)
            .map(|r| (Tag([r[0], r[1], r[2], r[3]]), u16_of(r[4], r[5])))
    }
    /// The MinMax table of BaseLangSys record `index`;
    /// [`ReadError::OutOfBounds`] for an index past the records.
    pub fn lang_sys_min_max(&self, index: usize) -> Result<Option<MinMax<'a>>, ReadError> {
        let (_, offset) = self.lang_sys_record(index).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, offset, MinMax::new)
    }
    /// The index of the BaseLangSys record for `tag`, by HarfBuzz's
    /// binary search.
    pub fn find_lang_sys(&self, tag: Tag) -> Option<usize> {
        hb_bsearch(self.num_lang_sys(), tag, |i| {
            self.lang_sys_record(i).map(|(t, _)| t)
        })
    }
}

/// A view over a BaseValues table.
#[derive(Clone, Copy, Debug)]
pub struct BaseValues<'a> {
    data: &'a [u8],
    offsets: &'a [[u8; 2]],
}

impl<'a> BaseValues<'a> {
    /// A view over the default index, the count and the offsets. Fails
    /// with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<BaseValues<'a>, ReadError> {
        Ok(BaseValues {
            data,
            offsets: records::<2>(data, 2)?,
        })
    }
    /// `defaultBaselineIndex`: the index of the script's default
    /// baseline in the BaseTagList.
    pub fn default_baseline_index(&self) -> u16 {
        be::u16_at(self.data, 0).unwrap_or(0)
    }
    /// `baseCoordCount`.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }
    /// Whether there are no coordinates.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The offset of coordinate `index` from the start of the table.
    pub fn coord_offset(&self, index: usize) -> Option<u16> {
        self.offsets.get(index).map(|o| u16_of(o[0], o[1]))
    }
    /// Coordinate `index`; `Ok(None)` also for an index past the
    /// offsets (HarfBuzz reads a null offset there).
    pub fn coord(&self, index: usize) -> Result<Option<BaseCoord<'a>>, ReadError> {
        follow(
            self.data,
            self.coord_offset(index).unwrap_or(0),
            BaseCoord::new,
        )
    }
}

/// A view over a MinMax table.
#[derive(Clone, Copy, Debug)]
pub struct MinMax<'a> {
    data: &'a [u8],
    records: &'a [[u8; 8]],
}

impl<'a> MinMax<'a> {
    /// A view over the offsets, the count and the FeatMinMax records.
    /// Fails with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MinMax<'a>, ReadError> {
        Ok(MinMax {
            data,
            records: records::<8>(data, 4)?,
        })
    }
    /// `minCoordOffset`; 0 for none.
    pub fn min_coord_offset(&self) -> u16 {
        be::u16_at(self.data, 0).unwrap_or(0)
    }
    /// `maxCoordOffset`; 0 for none.
    pub fn max_coord_offset(&self) -> u16 {
        be::u16_at(self.data, 2).unwrap_or(0)
    }
    /// The minimum extent.
    pub fn min_coord(&self) -> Result<Option<BaseCoord<'a>>, ReadError> {
        follow(self.data, self.min_coord_offset(), BaseCoord::new)
    }
    /// The maximum extent.
    pub fn max_coord(&self) -> Result<Option<BaseCoord<'a>>, ReadError> {
        follow(self.data, self.max_coord_offset(), BaseCoord::new)
    }
    /// `featMinMaxCount`.
    pub fn num_features(&self) -> usize {
        self.records.len()
    }
    /// FeatMinMax record `index`: its feature tag and the offsets of
    /// its minimum and maximum coordinates from the start of the MinMax
    /// table.
    pub fn feature_record(&self, index: usize) -> Option<(Tag, u16, u16)> {
        self.records.get(index).map(|r| {
            (
                Tag([r[0], r[1], r[2], r[3]]),
                u16_of(r[4], r[5]),
                u16_of(r[6], r[7]),
            )
        })
    }
    /// The minimum extent of FeatMinMax record `index`;
    /// [`ReadError::OutOfBounds`] for an index past the records.
    pub fn feature_min_coord(&self, index: usize) -> Result<Option<BaseCoord<'a>>, ReadError> {
        let (_, min, _) = self.feature_record(index).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, min, BaseCoord::new)
    }
    /// The maximum extent of FeatMinMax record `index`;
    /// [`ReadError::OutOfBounds`] for an index past the records.
    pub fn feature_max_coord(&self, index: usize) -> Result<Option<BaseCoord<'a>>, ReadError> {
        let (_, _, max) = self.feature_record(index).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, max, BaseCoord::new)
    }
    /// The index of the FeatMinMax record for `tag`, by HarfBuzz's
    /// binary search.
    pub fn find_feature(&self, tag: Tag) -> Option<usize> {
        hb_bsearch(self.num_features(), tag, |i| {
            self.feature_record(i).map(|(t, _, _)| t)
        })
    }
}

/// The size of a BaseCoord table of `format`; `None` for unknown formats.
fn coord_size(format: u16) -> Option<usize> {
    match format {
        1 => Some(4),
        2 => Some(8),
        3 => Some(6),
        4 => Some(9),
        _ => None,
    }
}

/// A view over a BaseCoord table, formats 1 to 4.
///
/// Format 1 is a coordinate; formats 2 and 4 add a reference glyph (a
/// 16-bit glyph id in format 2, 24-bit in format 4) and a contour point
/// on it; format 3 adds a Device table.
#[derive(Clone, Copy, Debug)]
pub struct BaseCoord<'a> {
    data: &'a [u8],
    format: u16,
}

impl<'a> BaseCoord<'a> {
    /// A view over the table.
    ///
    /// Fails with [`ReadError::UnknownFormat`] for a format other than
    /// 1 to 4, and with [`ReadError::Truncated`] if the table does not
    /// fit.
    pub fn new(data: &'a [u8]) -> Result<BaseCoord<'a>, ReadError> {
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        let size = coord_size(format).ok_or(ReadError::UnknownFormat(format))?;
        if data.len() < size {
            return Err(ReadError::truncated(size, data.len()));
        }
        Ok(BaseCoord { data, format })
    }
    /// The bytes from the start of the table to the end of the data it
    /// was built from.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `format`: 1 to 4.
    pub fn format(&self) -> u16 {
        self.format
    }
    /// `coordinate`, in font units.
    pub fn coordinate(&self) -> i16 {
        be::i16_at(self.data, 2).unwrap_or(0)
    }
    /// `referenceGlyph` (formats 2 and 4).
    pub fn reference_glyph(&self) -> Option<GlyphId> {
        match self.format {
            2 => be::u16_at(self.data, 4).map(GlyphId::from),
            4 => be::u24_at(self.data, 4).map(GlyphId),
            _ => None,
        }
    }
    /// `baseCoordPoint`, the contour point on the reference glyph
    /// (formats 2 and 4).
    pub fn coord_point(&self) -> Option<u16> {
        match self.format {
            2 => be::u16_at(self.data, 6),
            4 => be::u16_at(self.data, 7),
            _ => None,
        }
    }
    /// `deviceOffset` from the start of the BaseCoord table (format 3);
    /// 0 for none.
    pub fn device_offset(&self) -> Option<u16> {
        (self.format == 3).then(|| be::u16_at(self.data, 4).unwrap_or(0))
    }

    /// The Device or VariationIndex table of a format 3 coordinate;
    /// `Ok(None)` for other formats.
    pub fn device(&self) -> Result<Option<Device<'a>>, ReadError> {
        follow(self.data, self.device_offset().unwrap_or(0), Device::new)
    }
}

/// Checks a `BASE` table: an unknown version and truncation of the
/// header (and nothing else then), offsets whose tables do not fit,
/// BaseTagList, BaseScriptList, BaseLangSys and FeatMinMax records whose
/// tags do not ascend, BaseValues whose number of coordinates is not the
/// number of baseline tags, `defaultBaselineIndex` not below the number
/// of tags, and unknown BaseCoord formats. Offsets are those of the
/// offset fields that point to the bad tables, or of the records.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let base = match Base::new(data) {
        Ok(b) => b,
        Err(ReadError::UnknownVersion(found)) => {
            return report(Diagnostic::new(TAG, 0, Kind::UnknownVersion { found }));
        }
        Err(_) => {
            let needed = if data.len() >= 8 { 12 } else { 8 };
            return report(Diagnostic::new(
                TAG,
                0,
                Kind::Truncated {
                    needed,
                    available: to_u32(data.len()),
                },
            ));
        }
    };
    crate::validate::variation_store(
        data,
        base.item_var_store_offset().unwrap_or(0),
        8,
        TAG,
        None,
        report,
    );
    let mut v = Validator { data, report };
    for (field, offset) in [(4, base.horiz_axis_offset()), (6, base.vert_axis_offset())] {
        v.axis(field, offset);
    }
}

/// The state of [`validate`]: the table and the callback. Positions are
/// from the start of the table.
struct Validator<'d, 'r> {
    data: &'d [u8],
    report: &'r mut dyn FnMut(Diagnostic),
}

impl<'d> Validator<'d, '_> {
    fn emit(&mut self, at: usize, kind: Kind) {
        (self.report)(Diagnostic::new(TAG, to_u32(at), kind));
    }

    fn out_of_bounds(&mut self, field: usize, target: usize, length: usize) {
        let available = to_u32(self.data.len());
        self.emit(
            field,
            Kind::OffsetOutOfBounds {
                offset: to_u32(target),
                length: to_u32(length),
                available,
            },
        );
    }

    /// The table at `base + offset` (the offset read at `field`), built
    /// by `make`; reports it when it does not fit.
    fn table<T>(
        &mut self,
        field: usize,
        base: usize,
        offset: u16,
        make: impl FnOnce(&'d [u8]) -> Result<T, ReadError>,
    ) -> Option<(usize, T)> {
        if offset == 0 {
            return None;
        }
        let at = base.saturating_add(usize::from(offset));
        let data: &'d [u8] = self.data;
        let result = be::tail_at(data, at)
            .ok_or(ReadError::OutOfBounds)
            .and_then(make);
        match result {
            Ok(t) => Some((at, t)),
            Err(ReadError::UnknownFormat(f)) => {
                self.emit(
                    field,
                    Kind::UnknownFormat {
                        found: u32::from(f),
                    },
                );
                None
            }
            Err(ReadError::Truncated { needed, .. }) => {
                self.out_of_bounds(field, at, needed);
                None
            }
            Err(_) => {
                self.out_of_bounds(field, at, 0);
                None
            }
        }
    }

    /// Reports records whose tags do not ascend; `first` is the position
    /// of the first record.
    fn sorted(&mut self, first: usize, size: usize, tags: impl Iterator<Item = Tag>) {
        let mut previous: Option<Tag> = None;
        for (i, t) in tags.enumerate() {
            if previous.is_some_and(|p| t <= p) {
                let at = first.saturating_add(i.saturating_mul(size));
                self.emit(at, Kind::TagsNotSorted { index: to_u32(i) });
            }
            previous = Some(t);
        }
    }

    fn axis(&mut self, field: usize, offset: u16) {
        let Some((at, axis)) = self.table(field, 0, offset, Axis::new) else {
            return;
        };
        let tags = self.table(at, at, axis.base_tag_list_offset(), BaseTagList::new);
        let tag_count = tags.map_or(0, |(_, t)| t.len());
        if let Some((list_at, list)) = tags {
            self.sorted(
                list_at.saturating_add(2),
                4,
                (0..list.len()).filter_map(|i| list.tag(i)),
            );
        }
        let field = at.saturating_add(2);
        let Some((list_at, list)) = self.table(
            field,
            at,
            axis.base_script_list_offset(),
            BaseScriptList::new,
        ) else {
            return;
        };
        let first = list_at.saturating_add(2);
        self.sorted(
            first,
            6,
            (0..list.len()).filter_map(|i| list.record(i).map(|r| r.0)),
        );
        for i in 0..list.len() {
            let Some((_, offset)) = list.record(i) else {
                continue;
            };
            let record = first.saturating_add(i.saturating_mul(6)).saturating_add(4);
            if let Some((script_at, script)) = self.table(record, list_at, offset, BaseScript::new)
            {
                self.script(script_at, &script, tag_count);
            }
        }
    }

    fn script(&mut self, at: usize, script: &BaseScript<'_>, tag_count: usize) {
        if let Some((values_at, values)) =
            self.table(at, at, script.base_values_offset(), BaseValues::new)
        {
            if values.len() != tag_count {
                self.emit(
                    values_at.saturating_add(2),
                    Kind::BaseCoordCountMismatch {
                        tags: to_u32(tag_count),
                        coords: to_u32(values.len()),
                    },
                );
            }
            let index = values.default_baseline_index();
            if usize::from(index) >= tag_count {
                self.emit(
                    values_at,
                    Kind::DefaultBaselineOutOfRange {
                        index: u32::from(index),
                        count: to_u32(tag_count),
                    },
                );
            }
            for i in 0..values.len() {
                let field = values_at
                    .saturating_add(4)
                    .saturating_add(i.saturating_mul(2));
                self.coord(field, values_at, values.coord_offset(i).unwrap_or(0));
            }
        }
        let field = at.saturating_add(2);
        self.min_max(field, at, script.default_min_max_offset());
        let first = at.saturating_add(6);
        self.sorted(
            first,
            6,
            (0..script.num_lang_sys()).filter_map(|i| script.lang_sys_record(i).map(|r| r.0)),
        );
        for i in 0..script.num_lang_sys() {
            if let Some((_, offset)) = script.lang_sys_record(i) {
                let field = first.saturating_add(i.saturating_mul(6)).saturating_add(4);
                self.min_max(field, at, offset);
            }
        }
    }

    fn min_max(&mut self, field: usize, base: usize, offset: u16) {
        let Some((at, mm)) = self.table(field, base, offset, MinMax::new) else {
            return;
        };
        self.coord(at, at, mm.min_coord_offset());
        self.coord(at.saturating_add(2), at, mm.max_coord_offset());
        let first = at.saturating_add(6);
        self.sorted(
            first,
            8,
            (0..mm.num_features()).filter_map(|i| mm.feature_record(i).map(|r| r.0)),
        );
        for i in 0..mm.num_features() {
            if let Some((_, min, max)) = mm.feature_record(i) {
                let record = first.saturating_add(i.saturating_mul(8));
                self.coord(record.saturating_add(4), at, min);
                self.coord(record.saturating_add(6), at, max);
            }
        }
    }

    fn coord(&mut self, field: usize, base: usize, offset: u16) {
        let Some((at, coord)) = self.table(field, base, offset, BaseCoord::new) else {
            return;
        };
        if let Some(device) = coord.device_offset() {
            self.table(at.saturating_add(4), at, device, Device::new);
        }
    }
}
