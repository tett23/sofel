//! The Item Variation Store shared by CFF2, HVAR, VVAR, MVAR, GDEF and
//! others: the variation regions and the ItemVariationData subtables
//! (ADR 0021, ADR 0027, ADR 0030).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/otvarcommonformats>

use crate::be::{self, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// A view over an Item Variation Store (format 1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ItemVariationStore<'a> {
    data: &'a [u8],
    regions_at: usize,
    axis_count: u16,
    region_count: u16,
    data_count: u16,
}

/// A variation region: one `(start, peak, end)` of 2.14 numbers per
/// axis.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Region<'a> {
    axes: &'a [u8],
}

impl Region<'_> {
    /// The number of axes.
    pub fn len(&self) -> usize {
        self.axes.len() / 6
    }

    /// Whether there are no axes.
    pub fn is_empty(&self) -> bool {
        self.axes.is_empty()
    }

    /// `(startCoord, peakCoord, endCoord)` of axis `i`, raw 2.14.
    pub fn axis(&self, i: usize) -> Option<(i16, i16, i16)> {
        let at = i.checked_mul(6)?;
        Some((
            be::i16_at(self.axes, at)?,
            be::i16_at(self.axes, at.saturating_add(2))?,
            be::i16_at(self.axes, at.saturating_add(4))?,
        ))
    }
}

/// A view over an ItemVariationData subtable, including its delta rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ItemVariationData<'a> {
    /// `itemCount`.
    pub item_count: u16,
    /// `wordDeltaCount`.
    pub word_delta_count: u16,
    indexes: &'a [u8],
    rows: &'a [u8],
}

impl ItemVariationData<'_> {
    /// `regionIndexCount`.
    pub fn region_index_count(&self) -> u16 {
        u16::try_from(self.indexes.len() / 2).unwrap_or(u16::MAX)
    }

    /// Region index `k`.
    pub fn region_index(&self, k: usize) -> Option<u16> {
        be::u16_at(self.indexes, k.checked_mul(2)?)
    }

    /// The raw delta at `inner` and `column`; a cut-off row gives `None`.
    pub fn delta(&self, inner: u16, column: u16) -> Option<i32> {
        if inner >= self.item_count || column >= self.region_index_count() {
            return None;
        }
        let long = self.word_delta_count & 0x8000 != 0;
        let words = usize::from(self.word_delta_count & 0x7fff);
        let count = usize::from(self.region_index_count());
        if words > count {
            return None;
        }
        let big = if long { 4usize } else { 2 };
        let small = if long { 2usize } else { 1 };
        let row_size = words
            .checked_mul(big)?
            .checked_add(count.checked_sub(words)?.checked_mul(small)?)?;
        let row_at = usize::from(inner).checked_mul(row_size)?;
        let row = be::slice_at(self.rows, row_at, row_size)?;
        let column = usize::from(column);
        if column < words {
            let at = column.checked_mul(big)?;
            if long {
                be::i32_at(row, at)
            } else {
                be::i16_at(row, at).map(i32::from)
            }
        } else {
            let at = words
                .checked_mul(big)?
                .checked_add(column.checked_sub(words)?.checked_mul(small)?)?;
            if long {
                be::i16_at(row, at).map(i32::from)
            } else {
                be::i8_at(row, at).map(i32::from)
            }
        }
    }

    fn rows_size(&self) -> Option<usize> {
        let words = usize::from(self.word_delta_count & 0x7fff);
        let count = usize::from(self.region_index_count());
        let long = self.word_delta_count & 0x8000 != 0;
        words
            .checked_mul(if long { 4 } else { 2 })?
            .checked_add(
                count
                    .checked_sub(words)?
                    .checked_mul(if long { 2 } else { 1 })?,
            )?
            .checked_mul(usize::from(self.item_count))
    }
}

impl<'a> ItemVariationStore<'a> {
    /// A view over the store at the start of `data`.
    ///
    /// Fails with [`ReadError::UnknownFormat`] when the format is not 1,
    /// and with [`ReadError::Truncated`] when the header, the
    /// ItemVariationData offsets or the region list do not fit. The
    /// ItemVariationData subtables are checked when read.
    pub fn new(data: &'a [u8]) -> Result<ItemVariationStore<'a>, ReadError> {
        let truncated = |n: usize| ReadError::truncated(n, data.len());
        let format = be::u16_at(data, 0).ok_or(truncated(8))?;
        if format != 1 {
            return Err(ReadError::UnknownFormat(format));
        }
        let regions_at = to_usize(be::u32_at(data, 2).ok_or(truncated(8))?);
        let data_count = be::u16_at(data, 6).ok_or(truncated(8))?;
        let offsets_end = 8usize.saturating_add(usize::from(data_count).saturating_mul(4));
        if offsets_end > data.len() {
            return Err(truncated(offsets_end));
        }
        let axis_count =
            be::u16_at(data, regions_at).ok_or(truncated(regions_at.saturating_add(4)))?;
        let region_count = be::u16_at(data, regions_at.saturating_add(2))
            .ok_or(truncated(regions_at.saturating_add(4)))?;
        let regions_end = regions_at.saturating_add(4).saturating_add(
            usize::from(region_count)
                .saturating_mul(usize::from(axis_count))
                .saturating_mul(6),
        );
        if regions_end > data.len() {
            return Err(truncated(regions_end));
        }
        Ok(ItemVariationStore {
            data,
            regions_at,
            axis_count,
            region_count,
            data_count,
        })
    }

    /// The data the view was built over.
    pub fn bytes(&self) -> &'a [u8] {
        self.data
    }

    /// The number of axes of every region (`axisCount`).
    pub fn axis_count(&self) -> u16 {
        self.axis_count
    }

    /// The number of regions (`regionCount`).
    pub fn region_count(&self) -> u16 {
        self.region_count
    }

    /// Region `i`.
    pub fn region(&self, i: u16) -> Option<Region<'a>> {
        if i >= self.region_count {
            return None;
        }
        let size = usize::from(self.axis_count).saturating_mul(6);
        let at = self
            .regions_at
            .saturating_add(4)
            .saturating_add(usize::from(i).saturating_mul(size));
        Some(Region {
            axes: be::slice_at(self.data, at, size)?,
        })
    }

    /// The number of ItemVariationData subtables
    /// (`itemVariationDataCount`).
    pub fn data_count(&self) -> u16 {
        self.data_count
    }

    /// The offset of ItemVariationData `i` from the start of the store.
    pub fn data_offset(&self, i: u16) -> Option<u32> {
        if i >= self.data_count {
            return None;
        }
        be::u32_at(
            self.data,
            8usize.saturating_add(usize::from(i).saturating_mul(4)),
        )
    }

    /// ItemVariationData `i`: `None` when it is outside the data or its
    /// region indexes do not fit.
    pub fn item_data(&self, i: u16) -> Option<ItemVariationData<'a>> {
        let at = to_usize(self.data_offset(i)?);
        let item_count = be::u16_at(self.data, at)?;
        let word_delta_count = be::u16_at(self.data, at.saturating_add(2))?;
        let n = be::u16_at(self.data, at.saturating_add(4))?;
        let indexes = be::slice_at(
            self.data,
            at.saturating_add(6),
            usize::from(n).saturating_mul(2),
        )?;
        let rows_at = at
            .checked_add(6)?
            .checked_add(usize::from(n).checked_mul(2)?)?;
        Some(ItemVariationData {
            item_count,
            word_delta_count,
            indexes,
            rows: self.data.get(rows_at..)?,
        })
    }

    /// The raw delta for a store data index, row and region column.
    pub fn delta(&self, outer: u16, inner: u16, column: u16) -> Option<i32> {
        self.item_data(outer)?.delta(inner, column)
    }
}

/// Validates an Item Variation Store at the start of `data`, which is at
/// `base` in table `tag`: the format, the header, the region list, each
/// ItemVariationData header, region indexes and delta rows, and that every region
/// index is below the region count (all Error). With `axes`, also that
/// the regions have that many axes (Error).
pub fn validate(
    data: &[u8],
    tag: Tag,
    base: u32,
    axes: Option<u16>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let at = |o: usize| base.saturating_add(u32::try_from(o).unwrap_or(u32::MAX));
    let store = match ItemVariationStore::new(data) {
        Ok(s) => s,
        Err(e) => {
            let kind = match e {
                ReadError::UnknownFormat(f) => Kind::UnknownFormat {
                    found: u32::from(f),
                },
                ReadError::Truncated { needed, available } => Kind::Truncated {
                    needed: be::to_u32(needed),
                    available: be::to_u32(available),
                },
                _ => Kind::OffsetOutOfBounds {
                    offset: 0,
                    length: 0,
                    available: be::to_u32(data.len()),
                },
            };
            report(Diagnostic::new(tag, base, kind));
            return;
        }
    };
    if let Some(axes) = axes
        && axes != store.axis_count()
    {
        report(Diagnostic::new(
            tag,
            at(store.regions_at),
            Kind::AxisCountMismatch {
                found: u32::from(store.axis_count()),
                expected: u32::from(axes),
            },
        ));
    }
    for i in 0..store.data_count() {
        let offset = store.data_offset(i).unwrap_or(0);
        let Some(d) = store.item_data(i) else {
            report(Diagnostic::new(
                tag,
                at(8usize.saturating_add(usize::from(i).saturating_mul(4))),
                Kind::OffsetOutOfBounds {
                    offset,
                    length: 6,
                    available: be::to_u32(data.len()),
                },
            ));
            continue;
        };
        let words = d.word_delta_count & 0x7fff;
        if words > d.region_index_count() {
            report(Diagnostic::new(
                tag,
                at(to_usize(offset).saturating_add(2)),
                Kind::WordDeltaCountOutOfRange {
                    found: u32::from(words),
                    count: u32::from(d.region_index_count()),
                },
            ));
        } else if let Some(needed) = d.rows_size()
            && needed > d.rows.len()
        {
            report(Diagnostic::new(
                tag,
                at(to_usize(offset)
                    .saturating_add(6)
                    .saturating_add(d.indexes.len())),
                Kind::Truncated {
                    needed: be::to_u32(needed),
                    available: be::to_u32(d.rows.len()),
                },
            ));
        }
        for k in 0..usize::from(d.region_index_count()) {
            let r = d.region_index(k).unwrap_or(0);
            if r >= store.region_count() {
                report(Diagnostic::new(
                    tag,
                    at(to_usize(offset)
                        .saturating_add(6)
                        .saturating_add(k.saturating_mul(2))),
                    Kind::RegionIndexOutOfRange {
                        index: u32::from(r),
                        count: u32::from(store.region_count()),
                    },
                ));
            }
        }
    }
}
