//! HVAR and VVAR metric variation tables (ADR 0027).

use crate::be::{self, to_u32, to_usize};
use crate::delta_set_index_map::{self, DeltaSetIndexMap};
use crate::item_variation::{self, ItemVariationStore};
use crate::{Diagnostic, Kind, ReadError, Tag};

/// Horizontal metrics variation table tag.
pub const TAG: Tag = Tag::new(b"HVAR");
/// Vertical metrics variation table tag.
pub const TAG_VVAR: Tag = Tag::new(b"VVAR");

#[derive(Clone, Copy, Debug)]
struct Metrics<'a> {
    data: &'a [u8],
}

impl<'a> Metrics<'a> {
    fn new(data: &'a [u8], vertical: bool) -> Result<Self, ReadError> {
        let header = if vertical { 24 } else { 20 };
        let version = be::u32_at(data, 0).ok_or(ReadError::truncated(header, data.len()))?;
        if version >> 16 != 1 {
            return Err(ReadError::UnknownVersion(version));
        }
        be::slice_at(data, 0, header).ok_or(ReadError::truncated(header, data.len()))?;
        Ok(Self { data })
    }

    fn store(&self) -> Result<ItemVariationStore<'a>, ReadError> {
        let off = be::u32_at(self.data, 4).ok_or(ReadError::OutOfBounds)?;
        if off == 0 {
            return Err(ReadError::OutOfBounds);
        }
        ItemVariationStore::new(
            be::tail_at(self.data, to_usize(off)).ok_or(ReadError::OutOfBounds)?,
        )
    }

    fn map(&self, at: usize) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        let off = be::u32_at(self.data, at).ok_or(ReadError::OutOfBounds)?;
        if off == 0 {
            return Ok(None);
        }
        DeltaSetIndexMap::new(be::tail_at(self.data, to_usize(off)).ok_or(ReadError::OutOfBounds)?)
            .map(Some)
    }
}

/// A zero-copy HVAR view.
#[derive(Clone, Copy, Debug)]
pub struct Hvar<'a>(Metrics<'a>);

impl<'a> Hvar<'a> {
    /// Opens an HVAR table.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        Metrics::new(data, false).map(Self)
    }
    /// The ItemVariationStore.
    pub fn store(&self) -> Result<ItemVariationStore<'a>, ReadError> {
        self.0.store()
    }
    /// The advance-width map, or `None` for implicit `(0, glyph)` indices.
    pub fn advance_map(&self) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        self.0.map(8)
    }
    /// The left side-bearing map.
    pub fn lsb_map(&self) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        self.0.map(12)
    }
    /// The right side-bearing map.
    pub fn rsb_map(&self) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        self.0.map(16)
    }
}

/// A zero-copy VVAR view.
#[derive(Clone, Copy, Debug)]
pub struct Vvar<'a>(Metrics<'a>);

impl<'a> Vvar<'a> {
    /// Opens a VVAR table.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        Metrics::new(data, true).map(Self)
    }
    /// The ItemVariationStore.
    pub fn store(&self) -> Result<ItemVariationStore<'a>, ReadError> {
        self.0.store()
    }
    /// The advance-height map.
    pub fn advance_map(&self) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        self.0.map(8)
    }
    /// The top side-bearing map.
    pub fn tsb_map(&self) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        self.0.map(12)
    }
    /// The bottom side-bearing map.
    pub fn bsb_map(&self) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        self.0.map(16)
    }
    /// The vertical-origin map.
    pub fn vorg_map(&self) -> Result<Option<DeltaSetIndexMap<'a>>, ReadError> {
        self.0.map(20)
    }
}

/// Validates HVAR or VVAR and each referenced map and store.
pub fn validate(data: &[u8], tag: Tag, axes: Option<u16>, report: &mut dyn FnMut(Diagnostic)) {
    let vertical = tag == TAG_VVAR;
    let table = match Metrics::new(data, vertical) {
        Ok(t) => t,
        Err(ReadError::UnknownVersion(found)) => {
            return report(Diagnostic::new(tag, 0, Kind::UnknownVersion { found }));
        }
        Err(_) => {
            return report(Diagnostic::new(
                tag,
                0,
                Kind::Truncated {
                    needed: if vertical { 24 } else { 20 },
                    available: to_u32(data.len()),
                },
            ));
        }
    };
    let store = table.store().ok();
    for at in [4usize, 8, 12, 16, 20]
        .into_iter()
        .filter(|at| *at < if vertical { 24 } else { 20 })
    {
        let off = be::u32_at(data, at).unwrap_or(0);
        if off == 0 {
            if at == 4 {
                report(Diagnostic::new(
                    tag,
                    to_u32(at),
                    Kind::OffsetOutOfBounds {
                        offset: 0,
                        length: 8,
                        available: to_u32(data.len()),
                    },
                ));
            }
            continue;
        }
        let Some(tail) = be::tail_at(data, to_usize(off)) else {
            report(Diagnostic::new(
                tag,
                to_u32(at),
                Kind::OffsetOutOfBounds {
                    offset: off,
                    length: if at == 4 { 8 } else { 4 },
                    available: to_u32(data.len()),
                },
            ));
            continue;
        };
        if at == 4 {
            item_variation::validate(tail, tag, off, axes, report);
        } else {
            delta_set_index_map::validate(tail, tag, off, report);
            if let (Some(store), Ok(map)) = (store, DeltaSetIndexMap::new(tail)) {
                let header = if map.format() == 0 { 4usize } else { 6 };
                for i in 0..map.len() {
                    let index = map.map(i);
                    if index == u32::MAX {
                        continue;
                    }
                    let outer = (index >> 16) as u16;
                    let inner = u16::try_from(index & 0xffff).unwrap_or(0);
                    if !store.item_data(outer).is_some_and(|d| inner < d.item_count) {
                        report(Diagnostic::new(
                            tag,
                            off.saturating_add(to_u32(header))
                                .saturating_add(i.saturating_mul(u32::from(map.width()))),
                            Kind::VariationIndexOutOfRange {
                                outer: u32::from(outer),
                                inner: u32::from(inner),
                            },
                        ));
                    }
                }
            }
        }
    }
}
