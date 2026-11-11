//! Zero-copy views of the GSUB and GPOS FeatureVariations table (ADR 0027).
//!
//! The views check headers and arrays when constructed. Offsets are
//! followed only when requested, so a damaged child returns an error
//! without preventing callers from reading unrelated records.

use crate::be::{self, to_u32, to_usize};
use crate::{Diagnostic, Kind, ReadError, Tag};

fn need(data: &[u8], len: usize) -> Result<(), ReadError> {
    (data.len() >= len)
        .then_some(())
        .ok_or(ReadError::truncated(len, data.len()))
}

fn array_end(start: usize, count: usize, width: usize) -> Result<usize, ReadError> {
    start
        .checked_add(count.checked_mul(width).ok_or(ReadError::OutOfBounds)?)
        .ok_or(ReadError::OutOfBounds)
}

fn follow<'a, T>(
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

/// GSUB or GPOS FeatureVariations, version 1.x.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FeatureVariations<'a> {
    data: &'a [u8],
    version: u32,
    count: u32,
}

impl<'a> FeatureVariations<'a> {
    /// Reads the table header and its eight-byte variation records.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        need(data, 8)?;
        let version = be::u32_at(data, 0).ok_or(ReadError::OutOfBounds)?;
        if version >> 16 != 1 {
            return Err(ReadError::UnknownVersion(version));
        }
        let count = be::u32_at(data, 4).ok_or(ReadError::OutOfBounds)?;
        let end = array_end(8, to_usize(count), 8)?;
        need(data, end)?;
        if version >= 0x0001_0001 {
            need(data, array_end(end, 1, 4)?)?;
            let lookup_count = be::u32_at(data, end).ok_or(ReadError::OutOfBounds)?;
            let records_start = end.checked_add(4).ok_or(ReadError::OutOfBounds)?;
            need(data, array_end(records_start, to_usize(lookup_count), 6)?)?;
        }
        Ok(Self {
            data,
            version,
            count,
        })
    }

    /// The raw table bytes, starting with the version.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The 16.16 version number.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The number of variation records.
    pub fn len(&self) -> u32 {
        self.count
    }

    /// Whether there are no variation records.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Variation record `i`, if present.
    pub fn record(&self, i: u32) -> Option<VariationRecord<'a>> {
        if i >= self.count {
            return None;
        }
        let at = 8usize.checked_add(to_usize(i).checked_mul(8)?)?;
        Some(VariationRecord {
            data: self.data,
            at,
        })
    }

    /// The number of LookupVariationRecords in version 1.1 and later.
    pub fn lookup_record_count(&self) -> u32 {
        if self.version < 0x0001_0001 {
            return 0;
        }
        let Some(at) = array_end(8, to_usize(self.count), 8).ok() else {
            return 0;
        };
        be::u32_at(self.data, at).unwrap_or(0)
    }

    /// LookupVariationRecord `i`, if its complete array is present.
    pub fn lookup_record(&self, i: u32) -> Option<LookupVariationRecord<'a>> {
        let count = self.lookup_record_count();
        if i >= count {
            return None;
        }
        let start = array_end(12, to_usize(self.count), 8).ok()?;
        let end = array_end(start, to_usize(count), 6).ok()?;
        self.data.get(..end)?;
        let at = array_end(start, to_usize(i), 6).ok()?;
        Some(LookupVariationRecord {
            data: self.data,
            at,
        })
    }
}

/// A feature index and a FeatureLookupsTable offset relative to FeatureVariations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LookupVariationRecord<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> LookupVariationRecord<'a> {
    /// The feature whose lookups this record changes.
    pub fn feature_index(&self) -> u16 {
        be::u16_at(self.data, self.at).unwrap_or(0)
    }

    /// The referenced table, absent for a null offset.
    pub fn feature_lookups(&self) -> Result<Option<FeatureLookupsTable<'a>>, ReadError> {
        let field = self.at.checked_add(2).ok_or(ReadError::OutOfBounds)?;
        let offset = be::u32_at(self.data, field).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, offset, FeatureLookupsTable::new)
    }
}

/// The lookup conditions for one feature.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FeatureLookupsTable<'a> {
    data: &'a [u8],
    count: u32,
}

impl<'a> FeatureLookupsTable<'a> {
    /// Reads the header and condition record array.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        need(data, 10)?;
        let count = be::u32_at(data, 6).ok_or(ReadError::OutOfBounds)?;
        need(data, array_end(10, to_usize(count), 8)?)?;
        Ok(Self { data, count })
    }

    /// The bytes from the start of the table to the parent end.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// The 16.16 version number.
    pub fn version(&self) -> u32 {
        be::u32_at(self.data, 0).unwrap_or(0)
    }
    /// Flags; bit zero includes the selected Feature lookups.
    pub fn flags(&self) -> u16 {
        be::u16_at(self.data, 4).unwrap_or(0)
    }
    /// Number of condition records.
    pub fn len(&self) -> u32 {
        self.count
    }
    /// Whether there are no condition records.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Condition record `i`, if present.
    pub fn record(&self, i: u32) -> Option<LookupConditionRecord<'a>> {
        if i >= self.count {
            return None;
        }
        let at = array_end(10, to_usize(i), 8).ok()?;
        Some(LookupConditionRecord {
            data: self.data,
            at,
        })
    }
}

/// A condition offset and a LookupIndexList offset relative to FeatureLookupsTable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LookupConditionRecord<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> LookupConditionRecord<'a> {
    /// The condition offset relative to FeatureLookupsTable.
    pub fn condition_offset(&self) -> u32 {
        be::u32_at(self.data, self.at).unwrap_or(0)
    }

    /// The condition, absent for a null offset (which evaluates true).
    pub fn condition(&self) -> Result<Option<Condition<'a>>, ReadError> {
        follow(self.data, self.condition_offset(), Condition::new)
    }

    /// The lookup list, absent for a null offset.
    pub fn lookup_indices(&self) -> Result<Option<LookupIndexList<'a>>, ReadError> {
        let field = self.at.checked_add(4).ok_or(ReadError::OutOfBounds)?;
        let offset = be::u32_at(self.data, field).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, offset, LookupIndexList::new)
    }
}

/// A checked list of lookup indices.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LookupIndexList<'a> {
    data: &'a [u8],
    count: u16,
}

impl<'a> LookupIndexList<'a> {
    /// Reads the count and the full index array.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        need(data, 2)?;
        let count = be::u16_at(data, 0).ok_or(ReadError::OutOfBounds)?;
        need(data, array_end(2, usize::from(count), 2)?)?;
        Ok(Self { data, count })
    }

    /// Raw bytes of the index list.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// Number of lookup indices.
    pub fn len(&self) -> u16 {
        self.count
    }
    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// Lookup index `i`, if present.
    pub fn get(&self, i: u16) -> Option<u16> {
        if i >= self.count {
            return None;
        }
        let at = array_end(2, usize::from(i), 2).ok()?;
        be::u16_at(self.data, at)
    }
}

/// One pair of a ConditionSet and a FeatureTableSubstitution.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VariationRecord<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> VariationRecord<'a> {
    /// The ConditionSet, absent for a null offset.
    pub fn conditions(&self) -> Result<Option<ConditionSet<'a>>, ReadError> {
        let off = be::u32_at(self.data, self.at).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, off, ConditionSet::new)
    }

    /// The replacement features, absent for a null offset.
    pub fn substitutions(&self) -> Result<Option<FeatureTableSubstitution<'a>>, ReadError> {
        let field = self.at.checked_add(4).ok_or(ReadError::OutOfBounds)?;
        let off = be::u32_at(self.data, field).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, off, FeatureTableSubstitution::new)
    }
}

/// An AND of condition offsets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ConditionSet<'a> {
    data: &'a [u8],
    count: u16,
}

impl<'a> ConditionSet<'a> {
    /// Reads the condition offset array.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        need(data, 2)?;
        let count = be::u16_at(data, 0).ok_or(ReadError::OutOfBounds)?;
        need(data, array_end(2, usize::from(count), 4)?)?;
        Ok(Self { data, count })
    }

    /// The bytes from the start of this set to the end of its parent.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The number of conditions.
    pub fn len(&self) -> u16 {
        self.count
    }

    /// Whether there are no conditions.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Condition `i`; a null offset gives `Ok(None)`.
    pub fn condition(&self, i: u16) -> Result<Option<Condition<'a>>, ReadError> {
        if i >= self.count {
            return Ok(None);
        }
        let field = 2usize
            .checked_add(
                usize::from(i)
                    .checked_mul(4)
                    .ok_or(ReadError::OutOfBounds)?,
            )
            .ok_or(ReadError::OutOfBounds)?;
        let offset = be::u32_at(self.data, field).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, offset, Condition::new)
    }
}

/// One condition of format 1–5, or an unknown format kept for validation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Condition<'a> {
    data: &'a [u8],
    format: u16,
}

impl<'a> Condition<'a> {
    /// Reads a condition header and the fixed portion of its format.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        need(data, 2)?;
        let format = be::u16_at(data, 0).ok_or(ReadError::OutOfBounds)?;
        match format {
            1 | 2 => need(data, 8)?,
            3 | 4 => {
                need(data, 3)?;
                let count = usize::from(be::u8_at(data, 2).ok_or(ReadError::OutOfBounds)?);
                need(data, array_end(3, count, 3)?)?;
            }
            5 => need(data, 5)?,
            _ => {}
        }
        Ok(Self { data, format })
    }

    /// Raw condition bytes, including any following child data.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The condition format.
    pub fn format(&self) -> u16 {
        self.format
    }

    /// Format 1: `(axisIndex, minimum, maximum)` in raw F2Dot14.
    pub fn axis_range(&self) -> Option<(u16, i16, i16)> {
        (self.format == 1).then(|| {
            Some((
                be::u16_at(self.data, 2)?,
                be::i16_at(self.data, 4)?,
                be::i16_at(self.data, 6)?,
            ))
        })?
    }

    /// Format 2: `(defaultValue, variationIndex)`.
    pub fn value(&self) -> Option<(i16, u32)> {
        (self.format == 2).then(|| Some((be::i16_at(self.data, 2)?, be::u32_at(self.data, 4)?)))?
    }

    /// Formats 3 and 4 have 0–255 children; format 5 has one.
    pub fn children_len(&self) -> Option<u8> {
        match self.format {
            3 | 4 => be::u8_at(self.data, 2),
            5 => Some(1),
            _ => None,
        }
    }

    /// Child `i` of AND, OR or NOT. A null offset gives `Ok(None)`.
    pub fn child(&self, i: u8) -> Result<Option<Condition<'a>>, ReadError> {
        let offset = match self.format {
            3 | 4 if i < self.children_len().ok_or(ReadError::OutOfBounds)? => {
                let at = 3usize
                    .checked_add(
                        usize::from(i)
                            .checked_mul(3)
                            .ok_or(ReadError::OutOfBounds)?,
                    )
                    .ok_or(ReadError::OutOfBounds)?;
                be::u24_at(self.data, at).ok_or(ReadError::OutOfBounds)?
            }
            5 if i == 0 => be::u24_at(self.data, 2).ok_or(ReadError::OutOfBounds)?,
            _ => return Ok(None),
        };
        follow(self.data, offset, Condition::new)
    }
}

/// Version 1.0 FeatureTableSubstitution.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FeatureTableSubstitution<'a> {
    data: &'a [u8],
    count: u16,
}

impl<'a> FeatureTableSubstitution<'a> {
    /// Reads the substitution record array.
    pub fn new(data: &'a [u8]) -> Result<Self, ReadError> {
        need(data, 6)?;
        let count = be::u16_at(data, 4).ok_or(ReadError::OutOfBounds)?;
        need(data, array_end(6, usize::from(count), 6)?)?;
        Ok(Self { data, count })
    }

    /// Raw bytes of this subtable.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The 16.16 version number.
    pub fn version(&self) -> u32 {
        be::u32_at(self.data, 0).unwrap_or(0)
    }

    /// The number of substitution records.
    pub fn len(&self) -> u16 {
        self.count
    }

    /// Whether there are no substitution records.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Substitution record `i`, if present.
    pub fn record(&self, i: u16) -> Option<SubstitutionRecord<'a>> {
        if i >= self.count {
            return None;
        }
        let at = 6usize.checked_add(usize::from(i).checked_mul(6)?)?;
        Some(SubstitutionRecord {
            data: self.data,
            at,
        })
    }
}

/// A replacement feature index and its Feature table offset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SubstitutionRecord<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> SubstitutionRecord<'a> {
    /// The feature index to replace.
    pub fn feature_index(&self) -> u16 {
        be::u16_at(self.data, self.at).unwrap_or(0)
    }

    /// The replacement Feature table bytes, or absent for a null offset.
    pub fn feature(&self) -> Result<Option<&'a [u8]>, ReadError> {
        let field = self.at.checked_add(2).ok_or(ReadError::OutOfBounds)?;
        let offset = be::u32_at(self.data, field).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, offset, Ok)
    }
}

struct Validator<'a, 'r> {
    root: &'a [u8],
    tag: Tag,
    base_at: u32,
    axis_count: Option<u16>,
    feature_count: Option<usize>,
    lookup_count: Option<usize>,
    report: &'r mut dyn FnMut(Diagnostic),
    budget: usize,
    exhausted: bool,
}

impl Validator<'_, '_> {
    fn at(&self, child: &[u8]) -> usize {
        to_usize(self.base_at).saturating_add(self.root.len().saturating_sub(child.len()))
    }

    fn emit(&mut self, at: usize, kind: Kind) {
        (self.report)(Diagnostic::new(self.tag, to_u32(at), kind));
    }

    fn error(&mut self, at: usize, err: ReadError) {
        let kind = match err {
            ReadError::Truncated { needed, available } => Kind::Truncated {
                needed: to_u32(needed),
                available: to_u32(available),
            },
            ReadError::UnknownVersion(found) => Kind::UnknownVersion { found },
            ReadError::UnknownFormat(found) => Kind::UnknownFormat {
                found: u32::from(found),
            },
            ReadError::OutOfBounds | ReadError::InvalidValue => Kind::OffsetOutOfBounds {
                offset: to_u32(at),
                length: 0,
                available: to_u32(self.root.len()),
            },
        };
        self.emit(at, kind);
    }

    fn spend(&mut self) -> bool {
        if self.exhausted {
            return false;
        }
        if self.budget == 0 {
            self.exhausted = true;
            self.emit(to_usize(self.base_at), Kind::WorkLimitExceeded);
            return false;
        }
        self.budget = self.budget.saturating_sub(1);
        true
    }

    fn condition(&mut self, c: Condition<'_>, depth: u8) {
        if !self.spend() {
            return;
        }
        let at = self.at(c.data());
        if depth == 0 {
            self.emit(at, Kind::WorkLimitExceeded);
            return;
        }
        match c.format() {
            1 => {
                if let Some((axis, _, _)) = c.axis_range()
                    && let Some(count) = self.axis_count
                    && axis >= count
                {
                    self.emit(
                        at.saturating_add(2),
                        Kind::ConditionAxisOutOfRange {
                            axis: u32::from(axis),
                            count: u32::from(count),
                        },
                    );
                }
            }
            2 => {}
            3..=5 => {
                for i in 0..c.children_len().unwrap_or(0) {
                    if !self.spend() {
                        return;
                    }
                    match c.child(i) {
                        Ok(Some(child)) => self.condition(child, depth.saturating_sub(1)),
                        Ok(None) => {}
                        Err(e) => self.error(at, e),
                    }
                }
            }
            found => self.emit(
                at,
                Kind::UnknownConditionFormat {
                    found: u32::from(found),
                },
            ),
        }
    }

    fn set(&mut self, set: ConditionSet<'_>) {
        let at = self.at(set.data());
        for i in 0..set.len() {
            if !self.spend() {
                return;
            }
            match set.condition(i) {
                Ok(Some(c)) => self.condition(c, 64),
                Ok(None) => {}
                Err(e) => self.error(at.saturating_add(2).saturating_add(usize::from(i) * 4), e),
            }
        }
    }

    fn substitutions(&mut self, subs: FeatureTableSubstitution<'_>) {
        let at = self.at(subs.data());
        if subs.version() >> 16 != 1 {
            self.emit(
                at,
                Kind::UnknownVersion {
                    found: subs.version(),
                },
            );
        }
        for i in 0..subs.len() {
            if !self.spend() {
                return;
            }
            let Some(record) = subs.record(i) else {
                break;
            };
            let field = at.saturating_add(6).saturating_add(usize::from(i) * 6);
            match record.feature() {
                Ok(Some(feature)) => {
                    let needed =
                        be::u16_at(feature, 2).and_then(|n| array_end(4, usize::from(n), 2).ok());
                    match needed {
                        Some(len) if len <= feature.len() => {}
                        Some(len) => {
                            self.error(self.at(feature), ReadError::truncated(len, feature.len()))
                        }
                        None => {
                            self.error(self.at(feature), ReadError::truncated(4, feature.len()))
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => self.error(field.saturating_add(2), e),
            }
        }
    }

    fn feature_lookups(&mut self, table: FeatureLookupsTable<'_>) {
        let at = self.at(table.data());
        if table.version() >> 16 != 1 {
            self.emit(
                at,
                Kind::UnknownVersion {
                    found: table.version(),
                },
            );
        }
        if table.flags() & !1 != 0 {
            self.emit(
                at.saturating_add(4),
                Kind::ReservedBitsSet {
                    found: u32::from(table.flags()),
                },
            );
        }
        for i in 0..table.len() {
            if !self.spend() {
                return;
            }
            let Some(record) = table.record(i) else {
                break;
            };
            let field = at
                .saturating_add(10)
                .saturating_add(to_usize(i).saturating_mul(8));
            match record.condition() {
                Ok(Some(condition)) => self.condition(condition, 64),
                Ok(None) => {}
                Err(e) => self.error(field, e),
            }
            match record.lookup_indices() {
                Ok(Some(indices)) => {
                    for j in 0..indices.len() {
                        if !self.spend() {
                            return;
                        }
                        let index = indices.get(j).unwrap_or(0);
                        if let Some(count) = self.lookup_count
                            && usize::from(index) >= count
                        {
                            self.emit(
                                self.at(indices.data())
                                    .saturating_add(2)
                                    .saturating_add(usize::from(j) * 2),
                                Kind::LookupIndexOutOfRange {
                                    index: u32::from(index),
                                    count: to_u32(count),
                                },
                            );
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => self.error(field.saturating_add(4), e),
            }
        }
    }
}

/// Validates a FeatureVariations table embedded at `base_at` in a GSUB
/// or GPOS table. The axis and lookup counts are used when available.
pub fn validate(
    data: &[u8],
    tag: Tag,
    base_at: u32,
    axis_count: Option<u16>,
    feature_count: Option<usize>,
    lookup_count: Option<usize>,
    report: &mut dyn FnMut(Diagnostic),
) {
    let budget = data.len().saturating_mul(64).clamp(16384, 0x3fff_ffff);
    let mut v = Validator {
        root: data,
        tag,
        base_at,
        axis_count,
        feature_count,
        lookup_count,
        report,
        budget,
        exhausted: false,
    };
    let fv = match FeatureVariations::new(data) {
        Ok(v) => v,
        Err(e) => {
            v.error(to_usize(base_at), e);
            return;
        }
    };
    for i in 0..fv.len() {
        if !v.spend() {
            return;
        }
        let Some(record) = fv.record(i) else {
            break;
        };
        let record_at = to_usize(base_at)
            .saturating_add(8)
            .saturating_add(to_usize(i).saturating_mul(8));
        match record.conditions() {
            Ok(Some(set)) => v.set(set),
            Ok(None) => {}
            Err(e) => v.error(record_at, e),
        }
        match record.substitutions() {
            Ok(Some(subs)) => v.substitutions(subs),
            Ok(None) => {}
            Err(e) => v.error(record_at.saturating_add(4), e),
        }
    }
    for i in 0..fv.lookup_record_count() {
        if !v.spend() {
            return;
        }
        let Some(record) = fv.lookup_record(i) else {
            break;
        };
        let at = to_usize(base_at)
            .saturating_add(12)
            .saturating_add(to_usize(fv.len()).saturating_mul(8))
            .saturating_add(to_usize(i).saturating_mul(6));
        if let Some(count) = v.feature_count
            && usize::from(record.feature_index()) >= count
        {
            v.emit(
                at,
                Kind::FeatureIndexOutOfRange {
                    index: u32::from(record.feature_index()),
                    count: to_u32(count),
                },
            );
        }
        match record.feature_lookups() {
            Ok(Some(table)) => v.feature_lookups(table),
            Ok(None) => {}
            Err(e) => v.error(at.saturating_add(2), e),
        }
    }
}
