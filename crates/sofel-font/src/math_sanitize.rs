//! HarfBuzz's sanitizer for `MATH` (ADR 0091): whether HarfBuzz 15.1.0
//! keeps the table or replaces it with an empty one.
//!
//! Ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372): the `sanitize` methods of
//! `src/hb-ot-math-table.hh` and of `Coverage` (formats 1 to 4) in
//! `src/OT/Layout/Common/Coverage.hh`, with the context and `Device` of
//! [`crate::base_sanitize`]. Copyright © the HarfBuzz authors. Used under
//! HarfBuzz's "Old MIT" license; see THIRD_PARTY_NOTICES.md.
//!
//! As for `BASE` (ADR 0017),
//! HarfBuzz does not neuter bad offsets: a major version other than 1 or
//! any structure or array that does not fit rejects the whole table, and
//! every array checked takes its size in bytes from the operation budget
//! (64 per byte of the table, at least 16384). A Coverage or Device of an
//! unknown format is accepted; the queries read such a Coverage as empty.

use crate::base_sanitize::Ctx;

/// The size of the MathConstants table.
const CONSTANTS_SIZE: usize = 214;
/// The offset of the first MathValueRecord of MathConstants.
const CONSTANTS_FIRST_RECORD: usize = 8;
/// The number of MathValueRecords of MathConstants.
const CONSTANTS_RECORDS: usize = 51;

/// Whether HarfBuzz's sanitizer accepts `data` as a `MATH` table.
pub(crate) fn sane(data: &[u8]) -> bool {
    Ctx::new(data).math()
}

/// `step` bytes apart from `first`, the `i`th position.
fn nth(first: usize, step: usize, i: usize) -> usize {
    first.saturating_add(step.saturating_mul(i))
}

impl Ctx<'_> {
    /// `MATH::sanitize`: the version (major 1), then each subtable. The
    /// header itself is not checked as a whole, only field by field.
    fn math(&mut self) -> bool {
        self.fits(0, 4)
            && self.u16(0) == Some(1)
            && self.offset16(4, 0, Self::math_constants)
            && self.offset16(6, 0, Self::math_glyph_info)
            && self.offset16(8, 0, Self::math_variants)
    }

    /// `MathValueRecord::sanitize`: the record, and its Device relative
    /// to `parent`.
    fn value_record(&mut self, at: usize, parent: usize) -> bool {
        self.fits(at, 4) && self.offset16(at.saturating_add(2), parent, Self::device)
    }

    /// `n` MathValueRecords from `first`, Devices relative to `parent`.
    fn value_records(&mut self, first: usize, n: usize, parent: usize) -> bool {
        (0..n).all(|i| self.value_record(nth(first, 4, i), parent))
    }

    fn math_constants(&mut self, at: usize) -> bool {
        self.fits(at, CONSTANTS_SIZE)
            && self.value_records(
                at.saturating_add(CONSTANTS_FIRST_RECORD),
                CONSTANTS_RECORDS,
                at,
            )
    }

    fn math_glyph_info(&mut self, at: usize) -> bool {
        self.fits(at, 8)
            && self.offset16(at, at, Self::coverage_and_values)
            && self.offset16(at.saturating_add(2), at, Self::coverage_and_values)
            && self.offset16(at.saturating_add(4), at, Self::coverage)
            && self.offset16(at.saturating_add(6), at, Self::math_kern_info)
    }

    /// MathItalicsCorrectionInfo and MathTopAccentAttachment: a Coverage
    /// and an `Array16Of<MathValueRecord>`, both relative to the table.
    fn coverage_and_values(&mut self, at: usize) -> bool {
        if !(self.fits(at, 4) && self.offset16(at, at, Self::coverage)) {
            return false;
        }
        self.array(at.saturating_add(2), 4)
            .is_some_and(|(first, n)| self.value_records(first, n, at))
    }

    fn math_kern_info(&mut self, at: usize) -> bool {
        if !(self.fits(at, 4) && self.offset16(at, at, Self::coverage)) {
            return false;
        }
        let Some((first, n)) = self.array(at.saturating_add(2), 8) else {
            return false;
        };
        // Each MathKernInfoRecord: four MathKern offsets from MathKernInfo.
        (0..n).all(|i| {
            let record = nth(first, 8, i);
            (0..4).all(|k| self.offset16(nth(record, 2, k), at, Self::math_kern))
        })
    }

    /// `MathKern`: `2 * heightCount + 1` MathValueRecords, Devices
    /// relative to the MathKern.
    fn math_kern(&mut self, at: usize) -> bool {
        let Some(heights) = self.u16(at) else {
            return false;
        };
        let count = u64::from(heights).saturating_mul(2).saturating_add(1);
        let first = at.saturating_add(2);
        self.range(first, count.saturating_mul(4))
            && self.value_records(first, usize::try_from(count).unwrap_or(0), at)
    }

    fn math_variants(&mut self, at: usize) -> bool {
        if !(self.fits(at, 10)
            && self.offset16(at.saturating_add(2), at, Self::coverage)
            && self.offset16(at.saturating_add(4), at, Self::coverage))
        {
            return false;
        }
        let vertical = usize::from(self.u16(at.saturating_add(6)).unwrap_or(0));
        let horizontal = usize::from(self.u16(at.saturating_add(8)).unwrap_or(0));
        let count = vertical.saturating_add(horizontal);
        let first = at.saturating_add(10);
        self.range(
            first,
            u64::try_from(count).unwrap_or(u64::MAX).saturating_mul(2),
        ) && (0..count).all(|i| self.offset16(nth(first, 2, i), at, Self::math_glyph_construction))
    }

    fn math_glyph_construction(&mut self, at: usize) -> bool {
        self.fits(at, 4)
            && self.offset16(at, at, Self::math_glyph_assembly)
            && self.array(at.saturating_add(2), 4).is_some()
    }

    fn math_glyph_assembly(&mut self, at: usize) -> bool {
        self.fits(at, 6)
            && self.value_record(at, at)
            && self.array(at.saturating_add(4), 10).is_some()
    }

    /// `Coverage::sanitize`: formats 1 and 2 (16-bit counts of 2- and
    /// 6-byte records) and 3 and 4 (24-bit counts of 3- and 9-byte
    /// records); any other format is accepted.
    fn coverage(&mut self, at: usize) -> bool {
        let wide = |c: &mut Self, size: u64| {
            let count = at.saturating_add(2);
            c.u24(count).is_some_and(|n| {
                c.range(count.saturating_add(3), u64::from(n).saturating_mul(size))
            })
        };
        match self.u16(at) {
            None => false,
            Some(1) => self.array(at.saturating_add(2), 2).is_some(),
            Some(2) => self.array(at.saturating_add(2), 6).is_some(),
            Some(3) => wide(self, 3),
            Some(4) => wide(self, 9),
            Some(_) => true,
        }
    }
}
