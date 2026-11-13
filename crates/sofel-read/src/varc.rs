//! Views of the VARC variable-composite table (sofel ADR 0067).

use crate::be;
use crate::cff::Version;
use crate::cff::hb::{HbIndex, Sanitizer};
use crate::layout::Coverage;
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

/// The VARC table tag.
pub const TAG: Tag = Tag::new(b"VARC");

/// Validates a VARC table, including its referenced stores and components.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let (offset, kind) = match Varc::new(data) {
        Ok(_) => return,
        Err(ReadError::UnknownVersion(found)) => (0, Kind::UnknownVersion { found }),
        Err(ReadError::Truncated { needed, available }) => (
            0,
            Kind::Truncated {
                needed: u32::try_from(needed).unwrap_or(u32::MAX),
                available: u32::try_from(available).unwrap_or(u32::MAX),
            },
        ),
        Err(_) => (0, Kind::InvalidVarc),
    };
    report(Diagnostic::new(TAG, offset, kind));
}

/// A checked view of a VARC table.
#[derive(Clone, Copy)]
pub struct Varc<'a> {
    data: &'a [u8],
    coverage: Coverage<'a>,
    records: HbIndex<'a>,
    axis_indices: Option<HbIndex<'a>>,
    condition_offset: Option<usize>,
    store_offset: Option<usize>,
}

impl<'a> Varc<'a> {
    /// Opens the version 1 header, coverage, and CFF2-indexed records.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        if data.len() < 24 {
            return Err(ReadError::truncated(24, data.len()));
        }
        let version = be::u32_at(data, 0).ok_or(ReadError::truncated(4, data.len()))?;
        if version >> 16 != 1 {
            return Err(ReadError::UnknownVersion(version));
        }
        let offset = |at| be::u32_at(data, at).and_then(|x| usize::try_from(x).ok());
        let coverage_offset = offset(4).ok_or(ReadError::truncated(8, data.len()))?;
        if coverage_offset == 0 {
            return Err(ReadError::InvalidValue);
        }
        let record_offset = offset(20).ok_or(ReadError::truncated(24, data.len()))?;
        let coverage = Coverage::new(
            data.get(coverage_offset..)
                .ok_or(ReadError::truncated(coverage_offset, data.len()))?,
        )?;
        let mut sanitizer = Sanitizer::new(data.len());
        let records = HbIndex::sanitize(
            data,
            i64::try_from(record_offset).unwrap_or(i64::MAX),
            Version::Cff2,
            &mut sanitizer,
        )
        .ok_or(ReadError::truncated(record_offset, data.len()))?;
        let axis_indices = match offset(16).ok_or(ReadError::truncated(20, data.len()))? {
            0 => None,
            at => Some(
                HbIndex::sanitize(
                    data,
                    i64::try_from(at).unwrap_or(i64::MAX),
                    Version::Cff2,
                    &mut sanitizer,
                )
                .ok_or(ReadError::OutOfBounds)?,
            ),
        };
        let subtable_offset = |at| {
            offset(at)
                .filter(|&at| at == 0 || at < data.len())
                .map(|at| (at != 0).then_some(at))
                .ok_or(ReadError::OutOfBounds)
        };
        let store_offset = subtable_offset(8)?;
        let condition_offset = subtable_offset(12)?;
        if let Some(at) = store_offset {
            MultiItemVariationStore::new(data.get(at..).ok_or(ReadError::OutOfBounds)?)?;
        }
        if let Some(at) = condition_offset {
            ConditionList::new(data.get(at..).ok_or(ReadError::OutOfBounds)?)?;
        }
        let view = Self {
            data,
            coverage,
            records,
            axis_indices,
            condition_offset,
            store_offset,
        };
        for index in 0..view.record_count() {
            let record = view.record_at(index).ok_or(ReadError::OutOfBounds)?;
            if Components::new(record, axis_indices).any(|part| part.is_none()) {
                return Err(ReadError::InvalidValue);
            }
        }
        Ok(view)
    }

    /// Raw table bytes.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Covered glyphs in record order.
    pub fn coverage(&self) -> Coverage<'a> {
        self.coverage
    }

    /// The record for a covered glyph.
    pub fn record(&self, glyph: GlyphId) -> Option<&'a [u8]> {
        self.coverage.get(glyph).and_then(|i| self.records.get(i))
    }

    /// Number of indexed glyph records.
    pub fn record_count(&self) -> u32 {
        self.records.len()
    }

    /// The record at a coverage index.
    pub fn record_at(&self, index: u32) -> Option<&'a [u8]> {
        self.records.get(index)
    }

    /// Components of a covered glyph in record order.
    pub fn components(&self, glyph: GlyphId) -> Option<Components<'a>> {
        self.coverage
            .get(glyph)
            .map(|index| Components::new(self.records.get(index).unwrap_or(&[]), self.axis_indices))
    }

    /// The packed axis indices for one component axis-list index.
    pub fn axis_indices(&self, index: u32) -> Option<&'a [u8]> {
        self.axis_indices?.get(index)
    }

    /// The condition list bytes, if present.
    pub fn condition_list(&self) -> Option<ConditionList<'a>> {
        ConditionList::new(self.data.get(self.condition_offset?..)?).ok()
    }

    /// The MultiItemVariationStore bytes, if present.
    pub fn variation_store(&self) -> Option<MultiItemVariationStore<'a>> {
        MultiItemVariationStore::new(self.data.get(self.store_offset?..)?).ok()
    }
}

/// A component's parsed fields and its original encoded length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Component {
    /// Variable flags.
    pub flags: u32,
    /// Referenced glyph.
    pub glyph: GlyphId,
    /// Offset of the glyph ID within this record.
    pub glyph_offset: usize,
    /// Width of the encoded glyph ID.
    pub glyph_size: usize,
    /// Optional condition-list index.
    pub condition_index: Option<u32>,
    /// Optional axis-index-list index.
    pub axis_indices_index: Option<u32>,
    /// Offset of the packed axis values within this component.
    pub axis_values_offset: Option<usize>,
    /// End offset of the packed axis values within this component.
    pub axis_values_end: Option<usize>,
    /// Number of packed axis values.
    pub axis_values_count: usize,
    /// Optional axis-value variation index.
    pub axis_values_var_index: Option<u32>,
    /// Optional transform variation index.
    pub transform_var_index: Option<u32>,
    /// Unscaled encoded transform values in flag order.
    pub transform: [i16; 9],
    /// Number of bytes consumed in this component.
    pub size: usize,
}

/// A cursor over the component records of one VARC glyph.
pub struct Components<'a> {
    remaining: &'a [u8],
    axis_indices: Option<HbIndex<'a>>,
}

impl<'a> Components<'a> {
    /// Reads components from one record; `axis_indices` is the table's
    /// axis-indices CFF2 INDEX, if present.
    pub fn new(record: &'a [u8], axis_indices: Option<HbIndex<'a>>) -> Self {
        Self {
            remaining: record,
            axis_indices,
        }
    }
}

impl Iterator for Components<'_> {
    type Item = Option<Component>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        let parsed = Component::parse(self.remaining, self.axis_indices);
        match parsed {
            Some(component) if component.size > 0 => {
                self.remaining = self.remaining.get(component.size..).unwrap_or(&[]);
                Some(Some(component))
            }
            _ => {
                self.remaining = &[];
                Some(None)
            }
        }
    }
}

impl Component {
    /// Decodes one component without reading past the record.
    pub fn parse(data: &[u8], axis_indices: Option<HbIndex<'_>>) -> Option<Self> {
        let mut at = 0;
        let flags = varint(data, &mut at)?;
        let glyph_offset = at;
        let glyph_size = if flags & (1 << 12) != 0 { 3 } else { 2 };
        let glyph = GlyphId(read_uint(data, &mut at, glyph_size)?);
        let condition_index = (flags & (1 << 7) != 0)
            .then(|| varint(data, &mut at))
            .flatten();
        if flags & (1 << 7) != 0 && condition_index.is_none() {
            return None;
        }
        let axis_indices_index = (flags & (1 << 1) != 0)
            .then(|| varint(data, &mut at))
            .flatten();
        let mut axis_values_offset = None;
        let mut axis_values_end = None;
        let mut axis_values_count = 0;
        if flags & (1 << 1) != 0 {
            let indices = axis_indices?.get(axis_indices_index?)?;
            let count = tuple_count(indices)?;
            axis_values_offset = Some(at);
            axis_values_count = count;
            skip_tuple(data, &mut at, count)?;
            axis_values_end = Some(at);
        }
        let axis_values_var_index = (flags & (1 << 2) != 0)
            .then(|| varint(data, &mut at))
            .flatten();
        if flags & (1 << 2) != 0 && axis_values_var_index.is_none() {
            return None;
        }
        let transform_var_index = (flags & (1 << 3) != 0)
            .then(|| varint(data, &mut at))
            .flatten();
        if flags & (1 << 3) != 0 && transform_var_index.is_none() {
            return None;
        }
        let mut transform = [0; 9];
        for (index, bit) in [4, 5, 6, 8, 9, 13, 14, 10, 11].into_iter().enumerate() {
            if flags & (1 << bit) != 0 {
                *transform.get_mut(index)? = u16::try_from(read_uint(data, &mut at, 2)?)
                    .ok()?
                    .cast_signed();
            }
        }
        for _ in 0..(flags >> 15).count_ones() {
            varint(data, &mut at)?;
        }
        Some(Self {
            flags,
            glyph,
            glyph_offset,
            glyph_size,
            condition_index,
            axis_indices_index,
            axis_values_offset,
            axis_values_end,
            axis_values_count,
            axis_values_var_index,
            transform_var_index,
            transform,
            size: at,
        })
    }
}

fn read_uint(data: &[u8], at: &mut usize, len: usize) -> Option<u32> {
    let end = at.checked_add(len)?;
    let bytes = data.get(*at..end)?;
    *at = end;
    Some(
        bytes
            .iter()
            .fold(0, |value, &byte| (value << 8) | u32::from(byte)),
    )
}

fn varint(data: &[u8], at: &mut usize) -> Option<u32> {
    let first = *data.get(*at)?;
    let len = if first < 0x80 {
        1
    } else if first < 0xc0 {
        2
    } else if first < 0xe0 {
        3
    } else if first < 0xf0 {
        4
    } else {
        5
    };
    let value = read_uint(data, at, len)?;
    Some(match len {
        1 => value,
        2 => value & 0x3fff,
        3 => value & 0x1f_ffff,
        4 => value & 0x0fff_ffff,
        _ => value,
    })
}

fn tuple_count(data: &[u8]) -> Option<usize> {
    let mut at = 0;
    let mut count = 0usize;
    while at < data.len() {
        let control = *data.get(at)?;
        at = at.checked_add(1)?;
        let run = usize::from(control & 0x3f) + 1;
        let width = match control & 0xc0 {
            0x80 => 0,
            0x00 => 1,
            0x40 => 2,
            _ => 4,
        };
        at = at.checked_add(run.checked_mul(width)?)?;
        if at > data.len() {
            return None;
        }
        count = count.checked_add(run)?;
    }
    Some(count)
}

fn skip_tuple(data: &[u8], at: &mut usize, count: usize) -> Option<()> {
    let mut skipped = 0;
    while skipped < count {
        let control = *data.get(*at)?;
        *at = at.checked_add(1)?;
        let run = usize::from(control & 0x3f) + 1;
        let width = match control & 0xc0 {
            0x80 => 0,
            0x00 => 1,
            0x40 => 2,
            _ => 4,
        };
        let end = at.checked_add(run.checked_mul(width)?)?;
        data.get(*at..end)?;
        *at = end;
        skipped = skipped.checked_add(run)?;
    }
    (skipped == count).then_some(())
}

/// Decodes the run-packed integer tuples used by VARC axis lists and values.
pub struct TupleValues<'a> {
    data: &'a [u8],
    at: usize,
    remaining_run: usize,
    width: usize,
}

impl<'a> TupleValues<'a> {
    /// Opens a complete packed tuple after validating every run.
    pub fn new(data: &'a [u8]) -> Option<Self> {
        tuple_count(data)?;
        Some(Self {
            data,
            at: 0,
            remaining_run: 0,
            width: 0,
        })
    }

    /// Number of values in the packed tuple.
    pub fn len(&self) -> usize {
        tuple_count(self.data).unwrap_or(0)
    }

    /// Whether the tuple has no values.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Iterator for TupleValues<'_> {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_run == 0 {
            let control = *self.data.get(self.at)?;
            self.at = self.at.checked_add(1)?;
            self.remaining_run = usize::from(control & 0x3f) + 1;
            self.width = match control & 0xc0 {
                0x80 => 0,
                0x00 => 1,
                0x40 => 2,
                _ => 4,
            };
        }
        self.remaining_run = self.remaining_run.checked_sub(1)?;
        if self.width == 0 {
            return Some(0);
        }
        let raw = read_uint(self.data, &mut self.at, self.width)?;
        let shift = 32usize.checked_sub(self.width.checked_mul(8)?)?;
        Some((raw << shift).cast_signed() >> shift)
    }
}

/// A checked condition-list view. Evaluation follows the VARC condition
/// formats with a caller-supplied variation delta lookup.
#[derive(Clone, Copy)]
pub struct ConditionList<'a> {
    data: &'a [u8],
    count: usize,
}

impl<'a> ConditionList<'a> {
    /// Opens the offset array and checks each referenced condition start.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        let count =
            usize::try_from(be::u32_at(data, 0).ok_or(ReadError::truncated(4, data.len()))?)
                .map_err(|_| ReadError::InvalidValue)?;
        let end = 4usize
            .checked_add(count.checked_mul(4).ok_or(ReadError::InvalidValue)?)
            .ok_or(ReadError::InvalidValue)?;
        if end > data.len() {
            return Err(ReadError::truncated(end, data.len()));
        }
        for index in 0..count {
            let at = 4usize.saturating_add(index.saturating_mul(4));
            let offset = usize::try_from(be::u32_at(data, at).ok_or(ReadError::OutOfBounds)?)
                .map_err(|_| ReadError::OutOfBounds)?;
            if offset != 0 && offset.checked_add(2).is_none_or(|end| end > data.len()) {
                return Err(ReadError::OutOfBounds);
            }
        }
        Ok(Self { data, count })
    }

    /// Number of conditions.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether no conditions are indexed.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Evaluate one condition with normalized 2.14 coordinates.
    /// `delta` resolves a MultiItemVariationStore index for format 2.
    pub fn evaluate(
        &self,
        index: usize,
        coords: &[i32],
        delta: &impl Fn(u32) -> f64,
    ) -> Option<bool> {
        let at = 4usize.checked_add(index.checked_mul(4)?)?;
        if index >= self.count {
            return None;
        }
        let offset = be::u32_at(self.data, at)? as usize;
        if offset == 0 {
            return Some(false);
        }
        self.evaluate_at(offset, coords, delta, 64)
    }

    fn evaluate_at(
        &self,
        at: usize,
        coords: &[i32],
        delta: &impl Fn(u32) -> f64,
        depth: usize,
    ) -> Option<bool> {
        if depth == 0 {
            return None;
        }
        match be::u16_at(self.data, at)? {
            1 => {
                let axis = usize::from(be::u16_at(self.data, at.checked_add(2)?)?);
                let min = i32::from(be::u16_at(self.data, at.checked_add(4)?)?.cast_signed());
                let max = i32::from(be::u16_at(self.data, at.checked_add(6)?)?.cast_signed());
                let value = coords.get(axis).copied().unwrap_or(0);
                Some(min <= value && value <= max)
            }
            2 => {
                let value = i16::from_be_bytes(
                    self.data
                        .get(at.checked_add(2)?..at.checked_add(4)?)?
                        .try_into()
                        .ok()?,
                );
                let var_index = be::u32_at(self.data, at.checked_add(4)?)?;
                Some(f64::from(value) + delta(var_index) > 0.0)
            }
            3 | 4 => {
                let and = be::u16_at(self.data, at)? == 3;
                let count = usize::from(*self.data.get(at.checked_add(2)?)?);
                let mut result = and;
                for index in 0..count {
                    let field = at.checked_add(3)?.checked_add(index.checked_mul(3)?)?;
                    let relative = read_uint_at(self.data, field, 3)? as usize;
                    let child = if relative == 0 {
                        false
                    } else {
                        self.evaluate_at(
                            at.checked_add(relative)?,
                            coords,
                            delta,
                            depth.checked_sub(1)?,
                        )?
                    };
                    if and && !child {
                        return Some(false);
                    }
                    if !and && child {
                        return Some(true);
                    }
                    result = child;
                }
                Some(result)
            }
            5 => {
                let relative = read_uint_at(self.data, at.checked_add(2)?, 3)? as usize;
                if relative == 0 {
                    Some(true)
                } else {
                    self.evaluate_at(
                        at.checked_add(relative)?,
                        coords,
                        delta,
                        depth.checked_sub(1)?,
                    )
                    .map(|value| !value)
                }
            }
            _ => None,
        }
    }
}

fn read_uint_at(data: &[u8], at: usize, len: usize) -> Option<u32> {
    let mut cursor = at;
    read_uint(data, &mut cursor, len)
}

/// A bounded view of VARC's multi-item variation store and sparse regions.
#[derive(Clone, Copy)]
pub struct MultiItemVariationStore<'a> {
    data: &'a [u8],
    regions_offset: usize,
    dataset_count: usize,
}

impl<'a> MultiItemVariationStore<'a> {
    /// Opens a format 1 store and its offset array.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        if be::u16_at(data, 0) != Some(1) {
            return Err(ReadError::UnknownFormat(be::u16_at(data, 0).unwrap_or(0)));
        }
        let regions_offset =
            be::u32_at(data, 2).ok_or(ReadError::truncated(6, data.len()))? as usize;
        let dataset_count =
            usize::from(be::u16_at(data, 6).ok_or(ReadError::truncated(8, data.len()))?);
        let end = 8usize
            .checked_add(
                dataset_count
                    .checked_mul(4)
                    .ok_or(ReadError::InvalidValue)?,
            )
            .ok_or(ReadError::InvalidValue)?;
        if end > data.len()
            || regions_offset
                .checked_add(4)
                .is_none_or(|at| at > data.len())
        {
            return Err(ReadError::OutOfBounds);
        }
        for i in 0..dataset_count {
            let field = 8usize.saturating_add(i.saturating_mul(4));
            let offset = be::u32_at(data, field).ok_or(ReadError::OutOfBounds)? as usize;
            if offset.checked_add(7).is_none_or(|at| at > data.len()) {
                return Err(ReadError::OutOfBounds);
            }
        }
        Ok(Self {
            data,
            regions_offset,
            dataset_count,
        })
    }

    /// Number of indexed multi-item data sets.
    pub fn dataset_count(&self) -> usize {
        self.dataset_count
    }

    /// Adds the deltas for an index to `values` at normalized 2.14 `coords`.
    /// An out-of-range outer index contributes zero, as in HarfBuzz.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn add_delta(&self, index: u32, coords: &[i32], values: &mut [f64]) -> Option<()> {
        let outer = usize::try_from(index >> 16).ok()?;
        let inner = index & 0xffff;
        if outer >= self.dataset_count {
            return Some(());
        }
        let field = 8usize.checked_add(outer.checked_mul(4)?)?;
        let dataset = be::u32_at(self.data, field)? as usize;
        if *self.data.get(dataset)? != 1 {
            return None;
        }
        let count = usize::from(be::u16_at(self.data, dataset.checked_add(1)?)?);
        let indices_start = dataset.checked_add(3)?;
        let tuple_offset_field = indices_start.checked_add(count.checked_mul(2)?)?;
        let tuple_offset = be::u32_at(self.data, tuple_offset_field)? as usize;
        let tuple_start = dataset.checked_add(tuple_offset)?;
        let mut sanitizer = Sanitizer::new(self.data.len());
        let tuples = HbIndex::sanitize(
            self.data,
            i64::try_from(tuple_start).ok()?,
            Version::Cff2,
            &mut sanitizer,
        )?;
        let packed = tuples.get(inner)?;
        let mut decoded = TupleValues::new(packed)?;
        for region in 0..count {
            let region_index = be::u16_at(
                self.data,
                indices_start.checked_add(region.checked_mul(2)?)?,
            )?;
            let scalar = self.region_scalar(usize::from(region_index), coords)?;
            for value in values.iter_mut() {
                let delta = decoded.next()?;
                *value += f64::from(delta) * f64::from(scalar);
            }
        }
        Some(())
    }

    fn region_scalar(&self, index: usize, coords: &[i32]) -> Option<f32> {
        let list = self.regions_offset;
        let count = be::u32_at(self.data, list)? as usize;
        if index >= count {
            return Some(0.0);
        }
        let field = list.checked_add(4)?.checked_add(index.checked_mul(4)?)?;
        let relative = be::u32_at(self.data, field)? as usize;
        let region = list.checked_add(relative)?;
        let axis_count = usize::from(be::u16_at(self.data, region)?);
        let mut scalar = 1.0f32;
        for i in 0..axis_count {
            let field = region.checked_add(2)?.checked_add(i.checked_mul(4)?)?;
            let relative = be::u32_at(self.data, field)? as usize;
            let axis = region.checked_add(relative)?;
            let axis_index = usize::from(be::u16_at(self.data, axis)?);
            let start = i32::from(be::u16_at(self.data, axis.checked_add(2)?)?.cast_signed());
            let peak = i32::from(be::u16_at(self.data, axis.checked_add(4)?)?.cast_signed());
            let end = i32::from(be::u16_at(self.data, axis.checked_add(6)?)?.cast_signed());
            let coord = coords.get(axis_index).copied().unwrap_or(0);
            let factor = if peak == 0 || coord == peak {
                1.0
            } else if coord == 0 {
                0.0
            } else if start > peak || peak > end || (start < 0 && end > 0) {
                1.0
            } else if coord <= start || coord >= end {
                0.0
            } else if coord < peak {
                coord.saturating_sub(start) as f32 / peak.saturating_sub(start) as f32
            } else {
                end.saturating_sub(coord) as f32 / end.saturating_sub(peak) as f32
            };
            scalar *= factor;
            if scalar == 0.0 {
                break;
            }
        }
        Some(scalar)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn adr0067_i3_packed_tuple_run_boundaries() {
        let bytes = [0x01, 0xff, 0x7f, 0x80, 0x40, 0xff, 0x00, 0xc0, 0, 0, 0, 1];
        let mut values = TupleValues::new(&bytes).unwrap();
        for expected in [-1, 127, 0, -256, 1] {
            assert_eq!(values.next(), Some(expected));
        }
        assert_eq!(values.next(), None);
        assert!(TupleValues::new(&[0x40, 0xff]).is_none());
        assert!(TupleValues::new(&[0xc0, 0, 0, 0]).is_none());
    }

    #[test]
    fn adr0067_i1_condition_range_boundaries() {
        let bytes = [
            0, 0, 0, 1, // count
            0, 0, 0, 8, // condition offset
            0, 1, // format 1
            0, 0, // axis 0
            0, 10, // inclusive start
            0, 20, // inclusive end
        ];
        let conditions = ConditionList::new(&bytes).unwrap();
        assert_eq!(conditions.evaluate(0, &[9], &|_| 0.0), Some(false));
        assert_eq!(conditions.evaluate(0, &[10], &|_| 0.0), Some(true));
        assert_eq!(conditions.evaluate(0, &[20], &|_| 0.0), Some(true));
        assert_eq!(conditions.evaluate(0, &[21], &|_| 0.0), Some(false));
    }

    #[test]
    fn adr0067_i3_null_negated_condition_is_bounded() {
        let bytes = [
            0, 0, 0, 1, // count
            0, 0, 0, 8, // condition offset
            0, 5, // negate
            0, 0, 0, // null child
        ];
        let conditions = ConditionList::new(&bytes).unwrap();
        assert_eq!(conditions.evaluate(0, &[], &|_| 0.0), Some(true));
    }

    #[test]
    fn adr0067_i1_sparse_region_delta_boundaries() {
        let bytes = [
            0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 34, // store header
            0, 0, 0, 1, 0, 0, 0, 8, // one sparse region
            0, 1, 0, 0, 0, 6, // one axis at region+6
            0, 0, 0, 0, 0x40, 0, 0x40, 0, // axis 0, 0..1..1
            1, 0, 1, 0, 0, 0, 0, 0, 9, // one dataset, one region
            0, 0, 0, 1, 1, 1, 3, 0, 5, // CFF2 INDEX with +5
        ];
        let store = MultiItemVariationStore::new(&bytes).unwrap();
        for (coord, expected) in [(0, 0.0), (8192, 2.5), (16384, 5.0)] {
            let mut value = [10.0];
            store.add_delta(0, &[coord], &mut value).unwrap();
            assert_eq!(value, [10.0 + expected]);
        }
    }
}
