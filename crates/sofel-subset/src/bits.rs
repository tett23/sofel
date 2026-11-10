//! [`BitSet`]: a dense set of small `u32` values with the operations of
//! HarfBuzz's `hb_set_t` that the layout closure uses.
//!
//! Glyph ids, lookup, feature and class indices are at most 24 bits, so
//! a bit array is both simpler and faster than [`RangeSet`] for the many
//! membership tests and unions of the closure. Values of 2^24 and above
//! are never members.

use alloc::vec::Vec;

use crate::set::{INVALID, RangeSet};

/// The largest value a [`BitSet`] holds.
const CAP: u32 = 0x00FF_FFFF;

/// A set of values up to 2^24 - 1, ascending when iterated.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct BitSet {
    words: Vec<u64>,
    pop: u64,
}

fn split(v: u32) -> (usize, u32) {
    ((v >> 6) as usize, v & 63)
}

impl BitSet {
    /// The empty set.
    pub fn new() -> BitSet {
        BitSet::default()
    }

    /// The members of `s` up to the cap (`s` must not be inverted to be
    /// copied exactly).
    pub fn from_range_set(s: &RangeSet) -> BitSet {
        let mut b = BitSet::new();
        s.ranges().for_each(|(a, z)| b.add_range(a, z));
        b
    }

    /// The members as a [`RangeSet`].
    pub fn to_range_set(&self) -> RangeSet {
        let mut r = RangeSet::new();
        let mut start: Option<(u32, u32)> = None;
        for v in self.iter() {
            start = match start {
                Some((a, z)) if z.wrapping_add(1) == v => Some((a, v)),
                Some((a, z)) => {
                    r.add_range(a, z);
                    Some((v, v))
                }
                None => Some((v, v)),
            };
        }
        if let Some((a, z)) = start {
            r.add_range(a, z);
        }
        r
    }

    /// A set of the values of `it`.
    pub fn from_values(it: impl IntoIterator<Item = u32>) -> BitSet {
        let mut b = BitSet::new();
        it.into_iter().for_each(|v| b.add(v));
        b
    }

    /// Whether `v` is a member (`has`).
    pub fn has(&self, v: u32) -> bool {
        let (w, b) = split(v);
        self.words.get(w).is_some_and(|x| x >> b & 1 == 1)
    }

    /// Adds `v`; values above the cap are ignored.
    pub fn add(&mut self, v: u32) {
        if v > CAP {
            return;
        }
        let (w, b) = split(v);
        if self.words.len() <= w {
            self.words.resize(w.saturating_add(1), 0);
        }
        if let Some(x) = self.words.get_mut(w)
            && *x >> b & 1 == 0
        {
            *x |= 1 << b;
            self.pop = self.pop.saturating_add(1);
        }
    }

    /// Adds `a..=b` (`add_range`); nothing when `a > b`.
    pub fn add_range(&mut self, a: u32, b: u32) {
        if a > b || a > CAP {
            return;
        }
        let b = b.min(CAP);
        let (wa, _) = split(a);
        let (wb, _) = split(b);
        if self.words.len() <= wb {
            self.words.resize(wb.saturating_add(1), 0);
        }
        for w in wa..=wb {
            let base = u32::try_from(w).unwrap_or(0) << 6;
            let lo = a.max(base).wrapping_sub(base);
            let hi = b.min(base | 63).wrapping_sub(base);
            let mask = (u64::MAX >> 63u32.wrapping_sub(hi)) & (u64::MAX << lo);
            if let Some(x) = self.words.get_mut(w) {
                *x |= mask;
            }
        }
        self.recount();
    }

    /// Removes `v` (`del`).
    pub fn remove(&mut self, v: u32) {
        let (w, b) = split(v);
        if let Some(x) = self.words.get_mut(w)
            && *x >> b & 1 == 1
        {
            *x &= !(1 << b);
            self.pop = self.pop.saturating_sub(1);
        }
    }

    /// Removes every value at or above `v` (`del_range (v, INVALID)`).
    pub fn truncate(&mut self, v: u32) {
        let doomed: Vec<u32> = self.iter().filter(|&x| x >= v).collect();
        doomed.into_iter().for_each(|x| self.remove(x));
    }

    /// The number of members (`get_population`).
    pub fn len(&self) -> u64 {
        self.pop
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.pop == 0
    }

    /// Removes every member.
    pub fn clear(&mut self) {
        self.words.clear();
        self.pop = 0;
    }

    /// The members, ascending.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.words.iter().enumerate().flat_map(|(i, &w)| {
            let base = u32::try_from(i).unwrap_or(0) << 6;
            let mut bits = w;
            core::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let t = bits.trailing_zeros();
                bits &= bits.wrapping_sub(1);
                Some(base | t)
            })
        })
    }

    /// The smallest member greater than `v`, or the smallest member when
    /// `v` is [`INVALID`]; [`INVALID`] when there is none (`next`).
    pub fn next(&self, v: u32) -> u32 {
        let from = if v == INVALID {
            0
        } else {
            match v.checked_add(1) {
                Some(f) if f <= CAP => f,
                _ => return INVALID,
            }
        };
        let (mut w, b) = split(from);
        let mut mask = u64::MAX << b;
        while let Some(&x) = self.words.get(w) {
            let m = x & mask;
            if m != 0 {
                let base = u32::try_from(w).unwrap_or(0) << 6;
                return base | m.trailing_zeros();
            }
            w = w.saturating_add(1);
            mask = u64::MAX;
        }
        INVALID
    }

    /// The smallest member, or [`INVALID`].
    pub fn min(&self) -> u32 {
        self.next(INVALID)
    }

    /// The largest member, or [`INVALID`].
    pub fn max(&self) -> u32 {
        self.words
            .iter()
            .enumerate()
            .rev()
            .find(|(_, w)| **w != 0)
            .map_or(INVALID, |(i, w)| {
                (u32::try_from(i).unwrap_or(0) << 6) | 63u32.wrapping_sub(w.leading_zeros())
            })
    }

    fn recount(&mut self) {
        self.pop = self.words.iter().map(|w| u64::from(w.count_ones())).sum();
    }

    /// Adds every member of `other` (`union_`).
    pub fn union(&mut self, other: &BitSet) {
        if self.words.len() < other.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        self.words
            .iter_mut()
            .zip(other.words.iter())
            .for_each(|(a, b)| *a |= *b);
        self.recount();
    }

    /// Removes every member of `other` (`subtract`).
    pub fn subtract(&mut self, other: &BitSet) {
        self.words
            .iter_mut()
            .zip(other.words.iter())
            .for_each(|(a, b)| *a &= !*b);
        self.recount();
    }

    /// Whether every member is in `other` (`is_subset`).
    pub fn is_subset(&self, other: &BitSet) -> bool {
        self.words
            .iter()
            .enumerate()
            .all(|(i, &w)| w & !other.words.get(i).copied().unwrap_or(0) == 0)
    }

    /// Whether a member lies in `a..=b` (`intersects (first, last)`).
    pub fn intersects_range(&self, a: u32, b: u32) -> bool {
        if a > b {
            return false;
        }
        // `next (INVALID)` is the smallest member, which covers `a == 0`.
        let n = self.next(a.wrapping_sub(1));
        n != INVALID && n <= b
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hb_set_semantics() {
        let mut s = BitSet::new();
        assert_eq!(
            (s.min(), s.max(), s.next(INVALID)),
            (INVALID, INVALID, INVALID)
        );
        s.add_range(60, 130);
        s.add(5);
        assert_eq!(s.len(), 72);
        assert_eq!((s.min(), s.max()), (5, 130));
        // `next (INVALID)` starts from the beginning.
        assert_eq!(s.next(INVALID), 5);
        assert_eq!(s.next(5), 60);
        assert_eq!(s.next(130), INVALID);
        assert!(s.intersects_range(0, 5) && !s.intersects_range(131, 200));
        assert!(s.intersects_range(6, 60) && !s.intersects_range(6, 59));
        let mut t = s.clone();
        t.truncate(64);
        assert_eq!((t.len(), t.max()), (5, 63));
        assert!(t.is_subset(&s) && !s.is_subset(&t));
        s.subtract(&t);
        assert_eq!((s.len(), s.min()), (67, 64));
        s.union(&t);
        assert_eq!(s.len(), 72);
        assert_eq!(
            s.to_range_set(),
            BitSet::from_range_set(&s.to_range_set()).to_range_set()
        );
        assert_eq!(
            s.to_range_set().ranges().collect::<alloc::vec::Vec<_>>(),
            [(5, 5), (60, 130)]
        );
        // Values beyond 24 bits are never members.
        s.add(0x0100_0000);
        assert!(!s.has(0x0100_0000) && s.len() == 72);
    }
}
