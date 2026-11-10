//! MVAR metric-value variation records (ADR 0027).

use crate::be::{self, to_u32, to_usize};
use crate::item_variation::{self, ItemVariationStore};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// The MVAR table tag.
pub const TAG: Tag = Tag::new(b"MVAR");

/// An MVAR value record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueRecord {
    /// Metric tag.
    pub tag: Tag,
    /// Outer ItemVariationData index.
    pub outer: u16,
    /// Inner item index.
    pub inner: u16,
}

/// A zero-copy MVAR view.
#[derive(Clone, Copy, Debug)]
pub struct Mvar<'a> {
    data: &'a [u8],
    record_size: usize,
    record_count: usize,
}

impl<'a> Mvar<'a> {
    /// Opens MVAR version 1 with records of at least eight bytes.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        let version = be::u32_at(data, 0).ok_or(ReadError::truncated(12, data.len()))?;
        if version >> 16 != 1 {
            return Err(ReadError::UnknownVersion(version));
        }
        let record_size =
            usize::from(be::u16_at(data, 6).ok_or(ReadError::truncated(12, data.len()))?);
        if record_size < 8 {
            return Err(ReadError::InvalidValue);
        }
        let record_count =
            usize::from(be::u16_at(data, 8).ok_or(ReadError::truncated(12, data.len()))?);
        let needed = 12usize
            .checked_add(
                record_size
                    .checked_mul(record_count)
                    .ok_or(ReadError::OutOfBounds)?,
            )
            .ok_or(ReadError::OutOfBounds)?;
        be::slice_at(data, 0, needed).ok_or(ReadError::truncated(needed, data.len()))?;
        Ok(Self {
            data,
            record_size,
            record_count,
        })
    }

    /// The number of value records.
    pub fn record_count(&self) -> usize {
        self.record_count
    }

    /// Record `index`.
    pub fn record(&self, index: usize) -> Option<ValueRecord> {
        if index >= self.record_count {
            return None;
        }
        let at = 12usize.checked_add(index.checked_mul(self.record_size)?)?;
        Some(ValueRecord {
            tag: Tag::from_u32(be::u32_at(self.data, at)?),
            outer: be::u16_at(self.data, at.checked_add(4)?)?,
            inner: be::u16_at(self.data, at.checked_add(6)?)?,
        })
    }

    /// Finds a tag in sorted value records by binary search.
    pub fn find(&self, tag: Tag) -> Option<ValueRecord> {
        let mut low = 0;
        let mut high = self.record_count;
        while low < high {
            let mid = low.saturating_add(high.saturating_sub(low) / 2);
            let record = self.record(mid)?;
            match record.tag.cmp(&tag) {
                core::cmp::Ordering::Less => low = mid.saturating_add(1),
                core::cmp::Ordering::Greater => high = mid,
                core::cmp::Ordering::Equal => return Some(record),
            }
        }
        None
    }

    /// The ItemVariationStore.
    pub fn store(&self) -> Result<ItemVariationStore<'a>, ReadError> {
        let off = be::u16_at(self.data, 10).ok_or(ReadError::OutOfBounds)?;
        if off == 0 {
            return Err(ReadError::OutOfBounds);
        }
        ItemVariationStore::new(
            be::tail_at(self.data, usize::from(off)).ok_or(ReadError::OutOfBounds)?,
        )
    }
}

/// Validates MVAR records and the referenced ItemVariationStore.
pub fn validate(data: &[u8], axes: Option<u16>, report: &mut dyn FnMut(Diagnostic)) {
    let mvar = match Mvar::new(data) {
        Ok(v) => v,
        Err(ReadError::UnknownVersion(found)) => {
            return report(Diagnostic::new(TAG, 0, Kind::UnknownVersion { found }));
        }
        Err(ReadError::Truncated { needed, available }) => {
            return report(Diagnostic::new(
                TAG,
                0,
                Kind::Truncated {
                    needed: to_u32(needed),
                    available: to_u32(available),
                },
            ));
        }
        Err(_) => {
            return report(Diagnostic::new(
                TAG,
                6,
                Kind::OffsetOutOfBounds {
                    offset: 12,
                    length: 8,
                    available: to_u32(data.len()),
                },
            ));
        }
    };
    let mut previous = None;
    let store = mvar.store().ok();
    for i in 0..mvar.record_count() {
        let Some(record) = mvar.record(i) else {
            break;
        };
        if previous.is_some_and(|p| p >= record.tag) {
            report(Diagnostic::new(
                TAG,
                to_u32(12usize.saturating_add(i.saturating_mul(mvar.record_size))),
                Kind::TagsNotSorted { index: to_u32(i) },
            ));
        }
        previous = Some(record.tag);
        if let Some(store) = store
            && (record.outer, record.inner) != (u16::MAX, u16::MAX)
            && !store
                .item_data(record.outer)
                .is_some_and(|d| record.inner < d.item_count)
        {
            report(Diagnostic::new(
                TAG,
                to_u32(12usize.saturating_add(i.saturating_mul(mvar.record_size))),
                Kind::VariationIndexOutOfRange {
                    outer: u32::from(record.outer),
                    inner: u32::from(record.inner),
                },
            ));
        }
    }
    let off = be::u16_at(data, 10).unwrap_or(0);
    if off == 0 || be::tail_at(data, usize::from(off)).is_none() {
        report(Diagnostic::new(
            TAG,
            10,
            Kind::OffsetOutOfBounds {
                offset: u32::from(off),
                length: 8,
                available: to_u32(data.len()),
            },
        ));
    } else {
        if let Some(tail) = be::tail_at(data, to_usize(u32::from(off))) {
            item_variation::validate(tail, TAG, u32::from(off), axes, report);
        }
    }
}
