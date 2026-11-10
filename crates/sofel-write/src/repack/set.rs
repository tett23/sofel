//! The parts of HarfBuzz's `hb_set_t` the repacker uses: an ordered set of
//! `u32` that can be inverted.

use alloc::collections::BTreeSet;

/// `hb_set_t`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HbSet {
    set: BTreeSet<u32>,
    inverted: bool,
}

impl HbSet {
    /// An empty set.
    pub(crate) fn new() -> HbSet {
        HbSet::default()
    }

    /// Whether `v` is in the set.
    pub(crate) fn has(&self, v: u32) -> bool {
        self.set.contains(&v) != self.inverted
    }

    /// Adds `v`.
    pub(crate) fn add(&mut self, v: u32) {
        if self.inverted {
            self.set.remove(&v);
        } else {
            self.set.insert(v);
        }
    }

    /// Removes `v`.
    pub(crate) fn del(&mut self, v: u32) {
        if self.inverted {
            self.set.insert(v);
        } else {
            self.set.remove(&v);
        }
    }

    /// Removes everything (and the inversion).
    pub(crate) fn clear(&mut self) {
        self.set.clear();
        self.inverted = false;
    }

    /// `invert`.
    pub(crate) fn invert(&mut self) {
        self.inverted = !self.inverted;
    }

    /// The number of members (of a set that is not inverted).
    pub(crate) fn population(&self) -> u32 {
        if self.inverted {
            return u32::MAX;
        }
        u32::try_from(self.set.len()).unwrap_or(u32::MAX)
    }

    /// Whether the set is empty.
    pub(crate) fn is_empty(&self) -> bool {
        !self.inverted && self.set.is_empty()
    }

    /// The smallest member.
    pub(crate) fn min(&self) -> Option<u32> {
        if self.inverted {
            return (0..=u32::MAX).find(|v| !self.set.contains(v));
        }
        self.set.first().copied()
    }

    /// The largest member.
    pub(crate) fn max(&self) -> Option<u32> {
        self.set.last().copied()
    }

    /// The members in ascending order (of a set that is not inverted).
    pub(crate) fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.set.iter().copied()
    }

    /// The runs of consecutive members (`next_range`).
    pub(crate) fn ranges(&self) -> alloc::vec::Vec<(u32, u32)> {
        let mut out: alloc::vec::Vec<(u32, u32)> = alloc::vec::Vec::new();
        for v in self.iter() {
            match out.last_mut() {
                Some(r) if r.1.checked_add(1) == Some(v) => r.1 = v,
                _ => out.push((v, v)),
            }
        }
        out
    }

    /// Adds every member of `other` (`union_`).
    pub(crate) fn union(&mut self, other: &HbSet) {
        other.iter().for_each(|v| self.add(v));
    }
}

impl FromIterator<u32> for HbSet {
    fn from_iter<I: IntoIterator<Item = u32>>(iter: I) -> Self {
        HbSet {
            set: iter.into_iter().collect(),
            inverted: false,
        }
    }
}
