//! [`RangeSet`]: a set of `u32` values with HarfBuzz's `hb_set_t`
//! semantics, including inverted sets.

use alloc::vec::Vec;

/// HarfBuzz's invalid value (`HB_SET_VALUE_INVALID`), never a member.
pub const INVALID: u32 = u32::MAX;
const LAST: u32 = INVALID - 1;

/// A set of `u32` values below [`INVALID`], as sorted, disjoint,
/// non-adjacent inclusive ranges and an inversion flag (HarfBuzz's
/// `hb_set_t`). An inverted set holds every value not in the ranges.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RangeSet {
    ranges: Vec<(u32, u32)>,
    inverted: bool,
}

impl RangeSet {
    /// The empty set.
    pub const fn new() -> RangeSet {
        RangeSet {
            ranges: Vec::new(),
            inverted: false,
        }
    }

    /// Every value (an inverted empty set).
    pub const fn all() -> RangeSet {
        RangeSet {
            ranges: Vec::new(),
            inverted: true,
        }
    }

    /// A set of the values of `it`.
    pub fn from_values(it: impl IntoIterator<Item = u32>) -> RangeSet {
        it.into_iter().fold(RangeSet::new(), |s, v| s.with(v))
    }

    /// Whether the set is inverted.
    pub fn is_inverted(&self) -> bool {
        self.inverted
    }

    /// Removes every value (`hb_set_clear`), keeping the set not inverted.
    pub fn clear(&mut self) {
        self.ranges.clear();
        self.inverted = false;
    }

    /// Complements the set (`hb_set_invert`).
    pub fn invert(&mut self) {
        self.inverted = !self.inverted;
    }

    /// Adds `v` (`hb_set_add`).
    pub fn add(&mut self, v: u32) {
        self.add_range(v, v);
    }

    /// The set with `v` added.
    pub fn with(mut self, v: u32) -> RangeSet {
        self.add(v);
        self
    }

    /// Adds `first..=last` (`hb_set_add_range`); nothing when
    /// `first > last` or `first` is [`INVALID`].
    pub fn add_range(&mut self, first: u32, last: u32) {
        if first > last || first == INVALID {
            return;
        }
        let last = last.min(LAST);
        if self.inverted {
            raw_remove(&mut self.ranges, first, last);
        } else {
            raw_add(&mut self.ranges, first, last);
        }
    }

    /// Removes `v` (`hb_set_del`).
    pub fn remove(&mut self, v: u32) {
        self.remove_range(v, v);
    }

    /// Removes `first..=last` (`hb_set_del_range`).
    pub fn remove_range(&mut self, first: u32, last: u32) {
        if first > last || first == INVALID {
            return;
        }
        let last = last.min(LAST);
        if self.inverted {
            raw_add(&mut self.ranges, first, last);
        } else {
            raw_remove(&mut self.ranges, first, last);
        }
    }

    /// Whether `v` is in the set (`hb_set_has`).
    pub fn contains(&self, v: u32) -> bool {
        if v == INVALID {
            return false;
        }
        let i = self.ranges.partition_point(|r| r.1 < v);
        let inside = self.ranges.get(i).is_some_and(|r| r.0 <= v);
        inside != self.inverted
    }

    /// The number of values (`hb_set_get_population`).
    pub fn len(&self) -> u64 {
        let n: u64 = self
            .ranges
            .iter()
            .map(|r| u64::from(r.1.saturating_sub(r.0)).saturating_add(1))
            .sum();
        if self.inverted {
            u64::from(INVALID).saturating_sub(n)
        } else {
            n
        }
    }

    /// Whether the set is empty (`hb_set_is_empty`).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The members as ranges, ascending (`hb_set_next_range`).
    pub fn ranges(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        let complement = self.inverted;
        let mut prev_end: Option<u32> = None;
        let mut i = 0usize;
        let mut done = false;
        core::iter::from_fn(move || {
            if !complement {
                let r = self.ranges.get(i).copied();
                i = i.saturating_add(1);
                return r;
            }
            loop {
                if done {
                    return None;
                }
                let start = match prev_end {
                    None => 0,
                    Some(e) if e >= LAST => {
                        done = true;
                        return None;
                    }
                    Some(e) => e.saturating_add(1),
                };
                match self.ranges.get(i).copied() {
                    Some((a, b)) => {
                        i = i.saturating_add(1);
                        prev_end = Some(b);
                        if a > start {
                            return Some((start, a.saturating_sub(1)));
                        }
                    }
                    None => {
                        done = true;
                        return Some((start, LAST));
                    }
                }
            }
        })
    }

    /// The members, ascending.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.ranges().flat_map(|(a, b)| a..=b)
    }

    /// The smallest member, or [`INVALID`] (`hb_set_get_min`).
    pub fn min(&self) -> u32 {
        self.ranges().next().map_or(INVALID, |r| r.0)
    }

    /// The largest member, or [`INVALID`] (`hb_set_get_max`).
    pub fn max(&self) -> u32 {
        if self.inverted {
            match self.ranges.last() {
                Some(&(_, b)) if b >= LAST => self.ranges().last().map_or(INVALID, |r| r.1),
                _ => LAST,
            }
        } else {
            self.ranges.last().map_or(INVALID, |r| r.1)
        }
    }

    /// The smallest member greater than `v`; `INVALID` starts from the
    /// beginning (`hb_set_next`).
    pub fn next(&self, v: u32) -> u32 {
        let from = if v == INVALID { 0 } else { v.saturating_add(1) };
        if from == INVALID {
            return INVALID;
        }
        self.ranges()
            .find(|r| r.1 >= from)
            .map_or(INVALID, |r| r.0.max(from))
    }

    /// Adds every member of `other` (`hb_set_union`).
    pub fn union(&mut self, other: &RangeSet) {
        if !self.inverted && !other.inverted {
            other
                .ranges
                .iter()
                .for_each(|&(a, b)| raw_add(&mut self.ranges, a, b));
        } else {
            let members: Vec<(u32, u32)> = other.ranges().collect();
            members.into_iter().for_each(|(a, b)| self.add_range(a, b));
        }
    }

    /// Removes every value at or above `v` (`del_range (v, INVALID)`).
    pub fn truncate(&mut self, v: u32) {
        self.remove_range(v, LAST);
    }
}

fn raw_add(r: &mut Vec<(u32, u32)>, a: u32, b: u32) {
    // Ranges touching or overlapping [a, b] merge with it.
    let lo = r.partition_point(|x| x.1.saturating_add(1) < a && x.1 != LAST);
    let hi = r.partition_point(|x| x.0 <= b.saturating_add(1) || b == LAST);
    let (na, nb) = r
        .get(lo..hi)
        .unwrap_or(&[])
        .iter()
        .fold((a, b), |(s, e), x| (s.min(x.0), e.max(x.1)));
    r.splice(lo..hi.max(lo), core::iter::once((na, nb)));
}

fn raw_remove(r: &mut Vec<(u32, u32)>, a: u32, b: u32) {
    let lo = r.partition_point(|x| x.1 < a);
    let hi = r.partition_point(|x| x.0 <= b);
    if lo >= hi {
        return;
    }
    let first = r.get(lo).copied();
    let last = r.get(hi.saturating_sub(1)).copied();
    let mut keep = Vec::new();
    if let Some((s, _)) = first.filter(|x| x.0 < a) {
        keep.push((s, a.saturating_sub(1)));
    }
    if let Some((_, e)) = last.filter(|x| x.1 > b) {
        keep.push((b.saturating_add(1), e));
    }
    r.splice(lo..hi, keep);
}
