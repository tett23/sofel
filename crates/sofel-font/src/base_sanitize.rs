//! HarfBuzz's sanitizer for `BASE` (ADR 0017): whether HarfBuzz 15.1.0
//! keeps the table or replaces it with an empty one.
//!
//! Ported from HarfBuzz 15.1.0 (commit
//! 3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372): the `sanitize` methods of
//! `src/hb-ot-layout-base-table.hh`, of `Device`, `HintingDevice`,
//! `VariationDevice`, `ItemVariationStore`, `VarRegionList` and `VarData`
//! in `src/hb-ot-layout-common.hh`, and `hb_sanitize_context_t` in
//! `src/hb-sanitize.hh`. Copyright © the HarfBuzz authors. Used under
//! HarfBuzz's "Old MIT" license; see THIRD_PARTY_NOTICES.md.
//!
//! HarfBuzz 15.1.0 does not neuter bad offsets: any table that does not
//! fit, an unknown BaseCoord format, a major version other than 1 or an
//! ItemVariationStore that is not format 1 rejects the whole table. Like
//! HarfBuzz, every array checked consumes its size in bytes from an
//! operation budget of 64 operations per byte of the table (at least
//! 16384), so that tables whose offsets are shared many times over are
//! rejected too; this also bounds the work.

use sofel_read::be;

/// HarfBuzz's `HB_SANITIZE_MAX_OPS_FACTOR`, `_MIN` and `_MAX`.
const OPS_FACTOR: u64 = 64;
const OPS_MIN: u64 = 16384;
const OPS_MAX: u64 = 0x3FFF_FFFF;

/// Whether HarfBuzz's sanitizer accepts `data` as a `BASE` table.
pub(crate) fn sane(data: &[u8]) -> bool {
    let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
    let ops = len
        .checked_mul(OPS_FACTOR)
        .map_or(OPS_MAX, |m| m.clamp(OPS_MIN, OPS_MAX));
    let mut c = Ctx {
        data,
        ops: i64::try_from(ops).unwrap_or(0),
    };
    c.base()
}

struct Ctx<'a> {
    data: &'a [u8],
    ops: i64,
}

impl Ctx<'_> {
    /// `check_struct`: `len` bytes at `at` are there (no operations).
    fn fits(&self, at: usize, len: usize) -> bool {
        at.checked_add(len)
            .is_some_and(|end| end <= self.data.len())
    }

    /// `check_range`: `len` bytes at `at` are there, and the budget
    /// stays above 0 after taking `len` from it.
    fn range(&mut self, at: usize, len: u64) -> bool {
        let Ok(n) = usize::try_from(len) else {
            return false;
        };
        if !self.fits(at, n) {
            return false;
        }
        self.ops = self
            .ops
            .saturating_sub(i64::try_from(len).unwrap_or(i64::MAX));
        self.ops > 0
    }

    fn u16(&self, at: usize) -> Option<u16> {
        be::u16_at(self.data, at)
    }

    /// An `Offset16To` at `field`, from `base`: null, or a table that
    /// `check` accepts.
    fn offset16(&mut self, field: usize, base: usize, check: fn(&mut Self, usize) -> bool) -> bool {
        match self.u16(field) {
            None => false,
            Some(0) => true,
            Some(off) => check(self, base.saturating_add(usize::from(off))),
        }
    }

    /// An `Offset32To` at `field`, from `base`.
    fn offset32(&mut self, field: usize, base: usize, check: fn(&mut Self, usize) -> bool) -> bool {
        match be::u32_at(self.data, field) {
            None => false,
            Some(0) => true,
            Some(off) => check(self, base.saturating_add(be::to_usize(off))),
        }
    }

    /// An array of `count` (at `at`) records of `size` bytes after the
    /// count; the start of the records and the count.
    fn array(&mut self, at: usize, size: u64) -> Option<(usize, usize)> {
        let n = self.u16(at)?;
        let first = at.saturating_add(2);
        self.range(first, u64::from(n).saturating_mul(size))
            .then_some((first, usize::from(n)))
    }

    fn base(&mut self) -> bool {
        if !self.fits(0, 8) || self.u16(0) != Some(1) {
            return false;
        }
        let version = be::u32_at(self.data, 0).unwrap_or(0);
        self.offset16(4, 0, Self::axis)
            && self.offset16(6, 0, Self::axis)
            && (version < 0x0001_0001 || self.offset32(8, 0, Self::var_store))
    }

    fn axis(&mut self, at: usize) -> bool {
        self.fits(at, 4)
            && self.offset16(at, at, Self::tag_list)
            && self.offset16(at.saturating_add(2), at, Self::script_list)
    }

    fn tag_list(&mut self, at: usize) -> bool {
        self.array(at, 4).is_some()
    }

    fn script_list(&mut self, at: usize) -> bool {
        let Some((first, n)) = self.array(at, 6) else {
            return false;
        };
        (0..n).all(|i| {
            let record = first.saturating_add(i.saturating_mul(6));
            self.offset16(record.saturating_add(4), at, Self::script)
        })
    }

    fn script(&mut self, at: usize) -> bool {
        if !(self.fits(at, 6)
            && self.offset16(at, at, Self::values)
            && self.offset16(at.saturating_add(2), at, Self::min_max))
        {
            return false;
        }
        let Some((first, n)) = self.array(at.saturating_add(4), 6) else {
            return false;
        };
        (0..n).all(|i| {
            let record = first.saturating_add(i.saturating_mul(6));
            self.offset16(record.saturating_add(4), at, Self::min_max)
        })
    }

    fn values(&mut self, at: usize) -> bool {
        if !self.fits(at, 4) {
            return false;
        }
        let Some((first, n)) = self.array(at.saturating_add(2), 2) else {
            return false;
        };
        (0..n).all(|i| self.offset16(first.saturating_add(i.saturating_mul(2)), at, Self::coord))
    }

    fn min_max(&mut self, at: usize) -> bool {
        if !(self.fits(at, 6)
            && self.offset16(at, at, Self::coord)
            && self.offset16(at.saturating_add(2), at, Self::coord))
        {
            return false;
        }
        let Some((first, n)) = self.array(at.saturating_add(4), 8) else {
            return false;
        };
        (0..n).all(|i| {
            let record = first.saturating_add(i.saturating_mul(8));
            self.offset16(record.saturating_add(4), at, Self::coord)
                && self.offset16(record.saturating_add(6), at, Self::coord)
        })
    }

    fn coord(&mut self, at: usize) -> bool {
        match self.u16(at) {
            Some(1) => self.fits(at, 4),
            Some(2) => self.fits(at, 8),
            Some(3) => self.fits(at, 6) && self.offset16(at.saturating_add(4), at, Self::device),
            Some(4) => self.fits(at, 9),
            _ => false,
        }
    }

    fn device(&mut self, at: usize) -> bool {
        if !self.fits(at, 6) {
            return false;
        }
        let (start, end) = (
            self.u16(at).unwrap_or(0),
            self.u16(at.saturating_add(2)).unwrap_or(0),
        );
        match self.u16(at.saturating_add(4)).unwrap_or(0) {
            f @ 1..=3 => {
                let size = sofel_read::device::size(start, end, f);
                self.range(at, u64::try_from(size).unwrap_or(u64::MAX))
            }
            _ => true,
        }
    }

    fn var_store(&mut self, at: usize) -> bool {
        if !self.fits(at, 8) || self.u16(at) != Some(1) {
            return false;
        }
        if !self.offset32(at.saturating_add(2), at, Self::region_list) {
            return false;
        }
        let Some((first, n)) = self.array(at.saturating_add(6), 4) else {
            return false;
        };
        (0..n).all(|i| {
            self.offset32(
                first.saturating_add(i.saturating_mul(4)),
                at,
                Self::var_data,
            )
        })
    }

    fn region_list(&mut self, at: usize) -> bool {
        if !self.fits(at, 4) {
            return false;
        }
        let axes = u64::from(self.u16(at).unwrap_or(0));
        let regions = u64::from(self.u16(at.saturating_add(2)).unwrap_or(0));
        let bytes = axes.saturating_mul(regions).saturating_mul(6);
        // hb_unsigned_mul_overflows: the size must fit 32 bits.
        bytes <= u64::from(u32::MAX) && self.range(at.saturating_add(4), bytes)
    }

    fn var_data(&mut self, at: usize) -> bool {
        if !self.fits(at, 6) {
            return false;
        }
        let items = u64::from(self.u16(at).unwrap_or(0));
        let word_size_count = self.u16(at.saturating_add(2)).unwrap_or(0);
        let Some((first, n)) = self.array(at.saturating_add(4), 2) else {
            return false;
        };
        let words = word_size_count & 0x7FFF;
        if usize::from(words) > n {
            return false;
        }
        let width = if word_size_count & 0x8000 != 0 { 2 } else { 1 };
        let row = (u64::from(words).saturating_add(u64::try_from(n).unwrap_or(u64::MAX)))
            .saturating_mul(width);
        let bytes = items.saturating_mul(row);
        bytes <= u64::from(u32::MAX) && self.range(first.saturating_add(n.saturating_mul(2)), bytes)
    }
}
