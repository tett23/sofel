//! Reading layout structures the way HarfBuzz's `OT::` types read them:
//! a byte view whose reads past the end give zeros (HarfBuzz's Null
//! object), offsets that are null or point outside the table give the
//! empty view, and Coverage and ClassDef with HarfBuzz's search,
//! iteration and intersection rules (`hb-ot-layout-common.hh`,
//! `OT/Layout/Common/`).

use alloc::vec::Vec;

use crate::bits::BitSet;
use crate::set::INVALID;

/// A structure's bytes, from its start to the end of the table.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct D<'a>(pub &'a [u8]);

impl<'a> D<'a> {
    /// Whether the view is empty (the Null object).
    pub fn is_null(self) -> bool {
        self.0.is_empty()
    }

    /// The byte at `at`, 0 past the end.
    pub fn u8(self, at: usize) -> u8 {
        self.0.get(at).copied().unwrap_or(0)
    }

    /// A big-endian `u16`, 0 past the end.
    pub fn u16(self, at: usize) -> u16 {
        u16::from_be_bytes([self.u8(at), self.u8(at.saturating_add(1))])
    }

    /// A big-endian `u32`, 0 past the end.
    pub fn u32(self, at: usize) -> u32 {
        u32::from_be_bytes([
            self.u8(at),
            self.u8(at.saturating_add(1)),
            self.u8(at.saturating_add(2)),
            self.u8(at.saturating_add(3)),
        ])
    }

    /// A layout operand stored in two or three bytes.
    pub fn uint(self, at: usize, width: u8) -> u32 {
        if width == 3 {
            self.u32(at) >> 8
        } else {
            u32::from(self.u16(at))
        }
    }
    /// The view at `off` bytes in; empty past the end.
    pub fn at(self, off: usize) -> D<'a> {
        D(self.0.get(off..).unwrap_or(&[]))
    }

    /// The view an offset of `off` points to: empty for a null offset or
    /// one outside the table (HarfBuzz neuters such offsets).
    pub fn follow(self, off: u32) -> D<'a> {
        if off == 0 {
            return D(&[]);
        }
        self.at(off as usize)
    }

    /// The view the 16-bit offset at `at` points to.
    pub fn off16(self, at: usize) -> D<'a> {
        self.follow(u32::from(self.u16(at)))
    }

    /// The view the 32-bit offset at `at` points to.
    pub fn off32(self, at: usize) -> D<'a> {
        self.follow(self.u32(at))
    }

    /// `n` bytes at `at`, as far as they exist.
    pub fn slice(self, at: usize, n: usize) -> &'a [u8] {
        let end = at.saturating_add(n).min(self.0.len());
        self.0.get(at..end).unwrap_or(&[])
    }

    /// The `i`th `u16` of an array starting at `at`.
    pub fn u16_at(self, at: usize, i: usize) -> u16 {
        self.u16(at.saturating_add(i.saturating_mul(2)))
    }
}

/// MultipleSubst and AlternateSubst share a Coverage and an array of sequence offsets.
#[derive(Clone, Copy)]
pub(crate) struct SequenceSets<'a> {
    table: D<'a>,
    pub width: u8,
}

impl<'a> SequenceSets<'a> {
    pub fn new(table: D<'a>) -> Option<Self> {
        let width = match table.u16(0) {
            1 => 2,
            2 => 3,
            _ => return None,
        };
        Some(Self { table, width })
    }
    pub fn coverage(self) -> Cov<'a> {
        Cov(if self.width == 3 {
            self.table.off32(2)
        } else {
            self.table.off16(2)
        })
    }
    fn uint(self, data: D<'_>, at: usize) -> u32 {
        if self.width == 3 {
            data.u32(at) >> 8
        } else {
            u32::from(data.u16(at))
        }
    }
    pub fn count(self) -> usize {
        self.uint(self.table, if self.width == 3 { 6 } else { 4 }) as usize
    }
    pub fn sequence(self, index: usize) -> Option<impl Iterator<Item = u32> + 'a> {
        let width = usize::from(self.width);
        let start: usize = if width == 3 { 9 } else { 6 };
        let set = self.table.follow(self.uint(
            self.table,
            start.saturating_add(index.saturating_mul(width)),
        ));
        if set.is_null() {
            return None;
        }
        let count = self.uint(set, 0) as usize;
        if count.checked_add(1)?.checked_mul(width)? > set.0.len() {
            return None;
        }
        Some((0..count).map(move |i| self.uint(set, i.saturating_add(1).saturating_mul(width))))
    }
}

/// SingleSubst uses a long Coverage offset and 24-bit operands in formats 3 and 4.
#[derive(Clone, Copy)]
pub(crate) struct SingleSubst<'a>(pub D<'a>);

impl<'a> SingleSubst<'a> {
    pub fn coverage(self) -> Cov<'a> {
        Cov(if self.0.u16(0) >= 3 {
            self.0.off32(2)
        } else {
            self.0.off16(2)
        })
    }

    pub fn mask(self) -> u32 {
        if self.0.u16(0) >= 3 {
            0xFF_FFFF
        } else {
            0xFFFF
        }
    }

    /// Operand zero is the delta or array length. Subsequent operands are substitutes.
    pub fn operand(self, index: usize) -> u32 {
        if self.0.u16(0) >= 3 {
            self.0.u32(6usize.saturating_add(index.saturating_mul(3))) >> 8
        } else {
            u32::from(self.0.u16_at(4, index))
        }
    }
}

/// `v` as a `u32` (indices and counts of 16-bit arrays always fit).
pub(crate) fn n32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// `v` truncated to 16 bits, as HarfBuzz assigns counts to 16-bit fields.
pub(crate) fn n16(v: usize) -> u16 {
    u16::try_from(v & 0xFFFF).unwrap_or(0)
}

/// The low 16 bits of `v` (a glyph id, class or index of a 16-bit field).
pub(crate) fn lo16(v: u32) -> u16 {
    (v & 0xFFFF) as u16
}

/// `hb_bit_storage`: the number of bits needed for `v`.
pub(crate) fn bit_storage(v: u64) -> u64 {
    u64::from(64u32.wrapping_sub(v.leading_zeros()))
}

/// HarfBuzz's binary search (`hb_bsearch_impl`) over `n` items, `cmp`
/// comparing the key with item `i`.
pub(crate) fn bsearch(n: usize, cmp: impl Fn(usize) -> core::cmp::Ordering) -> Option<usize> {
    // `min..=max` of HarfBuzz is `lo..hi` here; `mid` is the same.
    let mut lo = 0usize;
    let mut hi = n;
    while lo < hi {
        let mid = lo.wrapping_add(hi.wrapping_sub(1)) / 2;
        match cmp(mid) {
            core::cmp::Ordering::Less => hi = mid,
            core::cmp::Ordering::Greater => lo = mid.wrapping_add(1),
            core::cmp::Ordering::Equal => return Some(mid),
        }
    }
    None
}

/// A Coverage table (formats 1 through 4).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Cov<'a>(pub D<'a>);

impl<'a> Cov<'a> {
    fn checked(self) -> sofel_font::read::layout::Coverage<'a> {
        sofel_font::read::layout::Coverage::new(self.0.0)
            .unwrap_or_else(|_| sofel_font::read::layout::Coverage::empty())
    }

    fn format(self) -> u16 {
        self.0.u16(0)
    }

    fn count(self) -> usize {
        if self.format() >= 3 {
            self.checked().num_records()
        } else {
            usize::from(self.0.u16(2))
        }
    }

    fn glyph1(self, i: usize) -> u32 {
        if self.format() == 3 {
            self.checked().glyph(i).map_or(0, |g| g.0)
        } else {
            u32::from(self.0.u16_at(4, i))
        }
    }

    /// `(first, last, start coverage index)` of range `i` of format 2.
    fn range(self, i: usize) -> (u32, u32, u32) {
        if self.format() == 4 {
            return self
                .checked()
                .range(i)
                .map_or((1, 0, 0), |(a, z, v)| (a.0, z.0, v));
        }
        let at = 4usize.saturating_add(i.saturating_mul(6));
        (
            u32::from(self.0.u16(at)),
            u32::from(self.0.u16(at.saturating_add(2))),
            u32::from(self.0.u16(at.saturating_add(4))),
        )
    }

    /// The coverage index of `g` (`get_coverage`).
    pub fn get(self, g: u32) -> Option<u32> {
        match self.format() {
            1 | 3 => bsearch(self.count(), |i| g.cmp(&self.glyph1(i)))
                .map(|i| u32::try_from(i).unwrap_or(INVALID)),
            2 | 4 => {
                let (first, last, value) = bsearch(self.count(), |i| {
                    let (a, z, _) = self.range(i);
                    if g < a {
                        core::cmp::Ordering::Less
                    } else if g <= z {
                        core::cmp::Ordering::Equal
                    } else {
                        core::cmp::Ordering::Greater
                    }
                })
                .map_or((1, 0, 0), |i| self.range(i));
                (first <= last).then(|| value.wrapping_add(g.wrapping_sub(first)))
            }
            _ => None,
        }
    }

    /// Whether `g` is covered.
    pub fn has(self, g: u32) -> bool {
        self.get(g).is_some()
    }

    /// The number of covered glyphs (`get_population`).
    pub fn population(self) -> u64 {
        match self.format() {
            1 | 3 => self.count() as u64,
            2 | 4 => (0..self.count())
                .map(|i| {
                    let (a, z, _) = self.range(i);
                    if z < a {
                        0
                    } else {
                        u64::from(z.wrapping_sub(a)).wrapping_add(1)
                    }
                })
                .sum(),
            _ => u64::from(INVALID),
        }
    }

    /// The glyphs in coverage order (`Coverage::iter`, which stops at a
    /// format 2 range whose coverage index does not follow on).
    pub fn glyphs(self) -> Vec<u32> {
        match self.format() {
            1 | 3 => (0..self.count()).map(|i| self.glyph1(i)).collect(),
            2 | 4 => {
                let n = self.count();
                let mut out = Vec::new();
                if n == 0 {
                    return out;
                }
                let (first, last, _) = self.range(0);
                if first > last {
                    return out;
                }
                let mut i = 0usize;
                let mut j = first;
                let mut coverage = 0u32;
                loop {
                    out.push(j);
                    if j >= self.range(i).1 {
                        i = i.saturating_add(1);
                        if i >= n {
                            return out;
                        }
                        let old = coverage;
                        let (a, _, v) = self.range(i);
                        j = a;
                        coverage = v;
                        if coverage != old.wrapping_add(1) {
                            return out;
                        }
                        continue;
                    }
                    coverage = coverage.wrapping_add(1);
                    j = j.wrapping_add(1);
                }
            }
            _ => Vec::new(),
        }
    }

    /// Whether a glyph of `glyphs` is covered (`intersects`).
    pub fn intersects(self, glyphs: &BitSet) -> bool {
        match self.format() {
            1 | 3 => {
                let n = self.count() as u64;
                if n > glyphs.len().saturating_mul(bit_storage(n)) {
                    return glyphs.iter().any(|g| self.has(g));
                }
                (0..self.count()).any(|i| glyphs.has(self.glyph1(i)))
            }
            2 | 4 => {
                let n = self.count() as u64;
                if n > glyphs.len().saturating_mul(bit_storage(n)) {
                    return glyphs.iter().any(|g| self.has(g));
                }
                (0..self.count()).any(|i| {
                    let (a, z, _) = self.range(i);
                    glyphs.intersects_range(a, z)
                })
            }
            _ => false,
        }
    }

    /// Adds the covered glyphs of `glyphs` to `out` (`intersect_set`).
    pub fn intersect_set(self, glyphs: &BitSet, out: &mut BitSet) {
        match self.format() {
            1 | 3 => (0..self.count())
                .map(|i| self.glyph1(i))
                .filter(|&g| glyphs.has(g))
                .for_each(|g| out.add(g)),
            2 | 4 => {
                let mut last = 0u32;
                for i in 0..self.count() {
                    let (a, z, _) = self.range(i);
                    if a < last {
                        break;
                    }
                    last = z;
                    let mut g = glyphs.next(a.wrapping_sub(1));
                    while g != INVALID && g <= z {
                        out.add(g);
                        g = glyphs.next(g);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A ClassDef table (formats 1 through 4).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Cls<'a>(pub D<'a>);

impl<'a> Cls<'a> {
    fn checked(self) -> sofel_font::read::layout::ClassDef<'a> {
        sofel_font::read::layout::ClassDef::new(self.0.0)
            .unwrap_or_else(|_| sofel_font::read::layout::ClassDef::empty())
    }

    fn format(self) -> u16 {
        self.0.u16(0)
    }

    fn start(self) -> u32 {
        if self.format() == 3 {
            self.checked().start_glyph().0
        } else {
            u32::from(self.0.u16(2))
        }
    }

    fn count1(self) -> usize {
        if self.format() == 3 {
            self.checked().num_records()
        } else {
            usize::from(self.0.u16(4))
        }
    }

    fn value1(self, i: usize) -> u32 {
        if self.format() == 3 {
            self.checked().class_value(i).unwrap_or(0)
        } else {
            u32::from(self.0.u16_at(6, i))
        }
    }

    fn count2(self) -> usize {
        if self.format() == 4 {
            self.checked().num_records()
        } else {
            usize::from(self.0.u16(2))
        }
    }

    fn range(self, i: usize) -> (u32, u32, u32) {
        if self.format() == 4 {
            return self
                .checked()
                .range(i)
                .map_or((1, 0, 0), |(a, z, v)| (a.0, z.0, v));
        }
        let at = 4usize.saturating_add(i.saturating_mul(6));
        (
            u32::from(self.0.u16(at)),
            u32::from(self.0.u16(at.saturating_add(2))),
            u32::from(self.0.u16(at.saturating_add(4))),
        )
    }

    fn find2(self, g: u32) -> Option<usize> {
        bsearch(self.count2(), |i| {
            let (a, z, _) = self.range(i);
            if g < a {
                core::cmp::Ordering::Less
            } else if g <= z {
                core::cmp::Ordering::Equal
            } else {
                core::cmp::Ordering::Greater
            }
        })
    }

    /// The class of `g` (`get_class`).
    pub fn get(self, g: u32) -> u32 {
        match self.format() {
            1 | 3 => {
                let i = g.wrapping_sub(self.start()) as usize;
                if i < self.count1() { self.value1(i) } else { 0 }
            }
            2 | 4 => self.find2(g).map_or(0, |i| self.range(i).2),
            _ => 0,
        }
    }

    /// The number of glyphs listed (`get_population`).
    pub fn population(self) -> u64 {
        match self.format() {
            1 | 3 => self.count1() as u64,
            2 | 4 => (0..self.count2())
                .map(|i| {
                    let (a, z, _) = self.range(i);
                    if z < a {
                        0
                    } else {
                        u64::from(z.wrapping_sub(a)).wrapping_add(1)
                    }
                })
                .sum(),
            _ => u64::from(INVALID),
        }
    }
}

impl<'a> Cls<'a> {
    /// Whether a glyph of `glyphs` has a class other than 0
    /// (`intersects`).
    pub fn intersects(self, glyphs: &BitSet) -> bool {
        match self.format() {
            1 | 3 => {
                let start = self.start();
                let end = start.wrapping_add(n32(self.count1()));
                let mut g = glyphs.next(start.wrapping_sub(1));
                while g != INVALID && g < end {
                    if self.value1(g.wrapping_sub(start) as usize) != 0 {
                        return true;
                    }
                    g = glyphs.next(g);
                }
                false
            }
            2 | 4 => {
                let n = self.count2() as u64;
                if n > glyphs.len().saturating_mul(bit_storage(n)) {
                    return glyphs.iter().any(|g| self.get(g) != 0);
                }
                (0..self.count2()).any(|i| {
                    let (a, z, v) = self.range(i);
                    glyphs.intersects_range(a, z) && v != 0
                })
            }
            _ => false,
        }
    }

    /// Whether a glyph of `glyphs` has class `klass` (`intersects_class`).
    pub fn intersects_class(self, glyphs: &BitSet, klass: u32) -> bool {
        match self.format() {
            1 | 3 => {
                let count = self.count1();
                let start = self.start();
                if klass == 0 {
                    let g = glyphs.next(INVALID);
                    if g == INVALID {
                        return false;
                    }
                    if g < start {
                        return true;
                    }
                    let g = start.wrapping_add(n32(count)).wrapping_sub(1);
                    if glyphs.next(g) != INVALID {
                        return true;
                    }
                }
                (0..count)
                    .any(|i| self.value1(i) == klass && glyphs.has(start.wrapping_add(n32(i))))
            }
            2 | 4 => {
                let n = self.count2();
                if klass == 0 {
                    if n == 0 {
                        return !glyphs.is_empty();
                    }
                    let mut g = INVALID;
                    let mut last = INVALID;
                    for i in 0..n {
                        let (a, z, _) = self.range(i);
                        if a != last.wrapping_add(1) {
                            g = glyphs.next(g);
                            if g == INVALID {
                                break;
                            }
                            if g < a {
                                return true;
                            }
                        }
                        g = z;
                        last = z;
                    }
                    if g != INVALID && glyphs.next(g) != INVALID {
                        return true;
                    }
                }
                (0..n).any(|i| {
                    let (a, z, v) = self.range(i);
                    v == klass && glyphs.intersects_range(a, z)
                })
            }
            _ => false,
        }
    }

    /// Adds the glyphs of `glyphs` that have class `klass` to `out`
    /// (`intersected_class_glyphs`).
    pub fn intersected_class_glyphs(self, glyphs: &BitSet, klass: u32, out: &mut BitSet) {
        match self.format() {
            1 | 3 => {
                let count = self.count1();
                let start = self.start();
                if klass == 0 {
                    let mut g = glyphs.next(INVALID);
                    while g != INVALID && g < start {
                        out.add(g);
                        g = glyphs.next(g);
                    }
                    let mut g = glyphs.next(start.wrapping_add(n32(count)).wrapping_sub(1));
                    while g != INVALID {
                        out.add(g);
                        g = glyphs.next(g);
                    }
                    return;
                }
                (0..count)
                    .filter(|&i| self.value1(i) == klass && glyphs.has(start.wrapping_add(n32(i))))
                    .for_each(|i| out.add(start.wrapping_add(n32(i))));
            }
            2 | 4 => {
                let n = self.count2();
                if klass == 0 {
                    let mut g = INVALID;
                    for i in 0..n {
                        let (a, z, _) = self.range(i);
                        g = glyphs.next(g);
                        if g == INVALID {
                            return;
                        }
                        while g < a {
                            out.add(g);
                            g = glyphs.next(g);
                            if g == INVALID {
                                return;
                            }
                        }
                        g = z;
                    }
                    loop {
                        g = glyphs.next(g);
                        if g == INVALID {
                            return;
                        }
                        out.add(g);
                    }
                }
                let count = n as u64;
                if count > glyphs.len().saturating_mul(bit_storage(count)) {
                    for g in glyphs.iter() {
                        if self.find2(g).is_some_and(|i| self.range(i).2 == klass) {
                            out.add(g);
                        }
                    }
                    return;
                }
                for i in 0..n {
                    let (a, z, v) = self.range(i);
                    if v != klass {
                        continue;
                    }
                    let end = z.wrapping_add(1);
                    let mut g = glyphs.next(a.wrapping_sub(1));
                    while g != INVALID && g < end {
                        out.add(g);
                        g = glyphs.next(g);
                    }
                }
            }
            _ => {}
        }
    }

    /// The classes of the glyphs of `glyphs` (`intersected_classes`).
    pub fn intersected_classes(self, glyphs: &BitSet, out: &mut BitSet) {
        if glyphs.is_empty() {
            return;
        }
        match self.format() {
            1 | 3 => {
                let count = self.count1();
                if count == 0 {
                    out.add(0);
                    return;
                }
                let start = self.start();
                let end = start.wrapping_add(n32(count)).wrapping_sub(1);
                if glyphs.min() < start || glyphs.max() > end {
                    out.add(0);
                }
                (0..count)
                    .filter(|&i| glyphs.has(start.wrapping_add(n32(i))))
                    .for_each(|i| out.add(self.value1(i)));
            }
            2 | 4 => {
                let n = self.count2();
                if n == 0 {
                    out.add(0);
                    return;
                }
                let mut g = INVALID;
                for i in 0..n {
                    let (a, z, _) = self.range(i);
                    g = glyphs.next(g);
                    if g == INVALID {
                        break;
                    }
                    if g < a {
                        out.add(0);
                        break;
                    }
                    g = z;
                }
                if g != INVALID && glyphs.next(g) != INVALID {
                    out.add(0);
                }
                (0..n).for_each(|i| {
                    let (a, z, v) = self.range(i);
                    if glyphs.intersects_range(a, z) {
                        out.add(v);
                    }
                });
            }
            _ => {}
        }
    }

    /// `(glyph, class)` for `ClassDef::subset`: the listed glyphs with a
    /// class other than 0 that `keep` accepts, mapped by `map`; format 1
    /// in glyph order, format 2 by `glyphset` or by range as HarfBuzz
    /// chooses.
    pub fn subset_pairs(
        self,
        glyphset: &BitSet,
        num_glyphs: u32,
        map: impl Fn(u32) -> Option<u32>,
        keep: impl Fn(u32) -> bool,
    ) -> (Vec<(u32, u32)>, BitSet) {
        let mut pairs = Vec::new();
        let mut klasses = BitSet::new();
        let mut push = |g: u32, k: u32| {
            if k == 0 {
                return;
            }
            let Some(n) = map(g) else { return };
            if !keep(g) {
                return;
            }
            pairs.push((n, k));
            klasses.add(k);
        };
        match self.format() {
            1 | 3 => {
                let start = self.start();
                (0..self.count1()).for_each(|i| push(start.wrapping_add(n32(i)), self.value1(i)));
            }
            2 | 4 => {
                let n = self.count2() as u64;
                if glyphset.len().saturating_mul(bit_storage(n)) < self.population() {
                    glyphset.iter().for_each(|g| push(g, self.get(g)));
                } else {
                    for i in 0..self.count2() {
                        let (a, z, v) = self.range(i);
                        if v == 0 {
                            continue;
                        }
                        let end = z.saturating_add(1).min(num_glyphs);
                        (a..end).for_each(|g| push(g, v));
                    }
                }
            }
            _ => {}
        }
        (pairs, klasses)
    }
}
