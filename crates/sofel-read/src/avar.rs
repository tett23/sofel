//! The `avar` (axis variations) table: the segment maps of versions 1
//! and 2 (ADR 0022).
//!
//! Version 2 appends offsets to a DeltaSetIndexMap and an
//! ItemVariationStore, both exposed as checked views (ADR 0027).
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/avar>

use crate::be::{self, to_u32, to_usize};
use crate::delta_set_index_map::{self, DeltaSetIndexMap};
use crate::item_variation::{self, ItemVariationStore};
use crate::{Diagnostic, F2Dot14, Kind, ReadError, Tag};

/// The `avar` tag.
pub const TAG: Tag = Tag::new(b"avar");

/// The size of the header.
const HEADER: usize = 8;
/// The size of the version 2 tail (two 32-bit offsets).
const V2_TAIL: usize = 8;

/// The segment map of one axis: `(fromCoordinate, toCoordinate)` pairs.
#[derive(Clone, Copy, Debug)]
pub struct SegmentMap<'a> {
    pairs: &'a [[u8; 4]],
}

impl SegmentMap<'_> {
    /// `positionMapCount`.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// Whether the map has no pairs.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Pair `index`: `(fromCoordinate, toCoordinate)`.
    pub fn pair(&self, index: usize) -> Option<(F2Dot14, F2Dot14)> {
        self.pairs.get(index).map(|p| {
            (
                F2Dot14(i16::from_be_bytes([p[0], p[1]])),
                F2Dot14(i16::from_be_bytes([p[2], p[3]])),
            )
        })
    }
}

/// A view over the `avar` table.
#[derive(Clone, Copy, Debug)]
pub struct Avar<'a> {
    data: &'a [u8],
    major: u16,
    axis_count: usize,
    /// Where the data after the segment maps starts.
    end: usize,
}

/// Walks the segment maps: where map `index` starts (or, for `index` ==
/// the axis count, where they end), if the maps before it fit.
fn map_at(data: &[u8], index: usize) -> Option<usize> {
    (0..index).try_fold(HEADER, |at, _| {
        let n = usize::from(be::u16_at(data, at)?);
        let next = at.saturating_add(2).saturating_add(n.saturating_mul(4));
        (next <= data.len()).then_some(next)
    })
}

/// What the table needs, as far as it can be read.
fn needed(data: &[u8]) -> usize {
    let axes = be::u16_at(data, 6).map_or(0, usize::from);
    let tail = if be::u16_at(data, 0) == Some(2) {
        V2_TAIL
    } else {
        0
    };
    let mut at = HEADER;
    for _ in 0..axes {
        match be::u16_at(data, at) {
            Some(n) => {
                at = at
                    .saturating_add(2)
                    .saturating_add(usize::from(n).saturating_mul(4))
            }
            None => return at.saturating_add(2),
        }
    }
    at.saturating_add(tail)
}

impl<'a> Avar<'a> {
    /// A view over the header and the segment maps.
    ///
    /// Fails with [`ReadError::UnknownVersion`] unless the major version
    /// is 1 or 2, and with [`ReadError::Truncated`] when the header, a
    /// segment map or the version 2 offsets do not fit (HarfBuzz's
    /// `avar::sanitize`).
    pub fn new(data: &'a [u8]) -> Result<Avar<'a>, ReadError> {
        let truncated = ReadError::truncated(needed(data), data.len());
        let h = data.first_chunk::<HEADER>().ok_or(truncated)?;
        let major = u16::from_be_bytes([h[0], h[1]]);
        if !matches!(major, 1 | 2) {
            return Err(ReadError::UnknownVersion(u32::from_be_bytes([
                h[0], h[1], h[2], h[3],
            ])));
        }
        let axis_count = usize::from(u16::from_be_bytes([h[6], h[7]]));
        let end = map_at(data, axis_count).ok_or(truncated)?;
        if major == 2 {
            be::slice_at(data, end, V2_TAIL).ok_or(truncated)?;
        }
        Ok(Avar {
            data,
            major,
            axis_count,
            end,
        })
    }

    /// `majorVersion`: 1 or 2.
    pub fn major_version(&self) -> u16 {
        self.major
    }

    /// `axisCount`.
    pub fn axis_count(&self) -> usize {
        self.axis_count
    }

    /// The segment map of axis `index`.
    pub fn segment_map(&self, index: usize) -> Option<SegmentMap<'a>> {
        if index >= self.axis_count {
            return None;
        }
        let at = map_at(self.data, index)?;
        let n = usize::from(be::u16_at(self.data, at)?);
        Some(SegmentMap {
            pairs: be::chunks_at::<4>(self.data, at.saturating_add(2), n)?,
        })
    }

    /// The version 2 axis index map. An absent or zero offset is `None`.
    /// A nonzero offset to invalid data returns an error.
    pub fn axis_index_map(&self) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        self.v2_offsets().map_or(Ok(None), |(offset, _)| {
            self.child(offset, DeltaSetIndexMap::new)
        })
    }

    /// The version 2 item variation store. An absent or zero offset is `None`.
    /// A nonzero offset to invalid data returns an error.
    pub fn variation_store(&self) -> Result<Option<ItemVariationStore<'a>>, ReadError> {
        self.v2_offsets().map_or(Ok(None), |(_, offset)| {
            self.child(offset, ItemVariationStore::new)
        })
    }

    fn child<T>(
        &self,
        offset: u32,
        open: impl FnOnce(&'a [u8]) -> Result<T, ReadError>,
    ) -> Result<Option<T>, ReadError> {
        if offset == 0 {
            return Ok(None);
        }
        let data = be::tail_at(self.data, to_usize(offset)).ok_or(ReadError::OutOfBounds)?;
        open(data).map(Some)
    }

    /// The version 2 offsets: `(axisIndexMapOffset, varStoreOffset)`.
    pub fn v2_offsets(&self) -> Option<(u32, u32)> {
        if self.major != 2 {
            return None;
        }
        Some((
            be::u32_at(self.data, self.end)?,
            be::u32_at(self.data, self.end.saturating_add(4))?,
        ))
    }
}

/// Checks an `avar` table: an unknown version and truncation (and
/// nothing else then), and an axis count other than `fvar_axes` (when
/// given), and the optional version 2 map and store, including store axes.
pub fn validate(data: &[u8], fvar_axes: Option<u16>, report: &mut dyn FnMut(Diagnostic)) {
    let mut emit = |offset: usize, kind: Kind| report(Diagnostic::new(TAG, to_u32(offset), kind));
    let avar = match Avar::new(data) {
        Ok(a) => a,
        Err(ReadError::UnknownVersion(found)) => return emit(0, Kind::UnknownVersion { found }),
        Err(_) => {
            return emit(
                0,
                Kind::Truncated {
                    needed: to_u32(needed(data)),
                    available: to_u32(data.len()),
                },
            );
        }
    };
    if let Some(expected) = fvar_axes
        && avar.axis_count() != usize::from(expected)
    {
        emit(
            6,
            Kind::AxisCountMismatch {
                found: to_u32(avar.axis_count()),
                expected: u32::from(expected),
            },
        );
    }
    if let Some((map_offset, store_offset)) = avar.v2_offsets() {
        for (offset, is_store) in [(map_offset, false), (store_offset, true)] {
            if offset == 0 {
                continue;
            }
            let Some(child) = be::tail_at(data, to_usize(offset)) else {
                report(Diagnostic::new(
                    TAG,
                    to_u32(avar.end.saturating_add(if is_store { 4 } else { 0 })),
                    Kind::OffsetOutOfBounds {
                        offset,
                        length: 0,
                        available: to_u32(data.len()),
                    },
                ));
                continue;
            };
            if is_store {
                item_variation::validate(
                    child,
                    TAG,
                    offset,
                    u16::try_from(avar.axis_count()).ok(),
                    report,
                );
            } else {
                delta_set_index_map::validate(child, TAG, offset, report);
            }
        }
    }
}
