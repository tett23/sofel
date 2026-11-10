//! HarfBuzz 15.1.0's `hb_priority_queue_t<int64_t>`
//! (`src/hb-priority-queue.hh`): a binary min-heap of `(priority, value)`
//! stored in an array, children of `i` at `2i + 1` and `2i + 2`.
//!
//! Items of equal priority come out in the order the heap's sift steps
//! leave them, not in insertion order; the repacker's results depend on
//! that order, so the sift steps are HarfBuzz's exactly (an item moves up
//! only past a strictly larger parent, and down only to a strictly
//! smaller child, preferring the right child on ties).

use alloc::vec::Vec;

/// A min-heap of `(priority, value)`.
#[derive(Clone, Debug, Default)]
pub(crate) struct PriorityQueue {
    heap: Vec<(i64, u32)>,
}

impl PriorityQueue {
    /// An empty queue.
    pub(crate) fn new() -> PriorityQueue {
        PriorityQueue { heap: Vec::new() }
    }

    /// Adds an item.
    pub(crate) fn insert(&mut self, priority: i64, value: u32) {
        self.heap.push((priority, value));
        self.bubble_up(self.heap.len().saturating_sub(1));
    }

    /// Removes and returns the item of the lowest priority.
    pub(crate) fn pop_minimum(&mut self) -> Option<(i64, u32)> {
        let last = self.heap.pop()?;
        let Some(first) = self.heap.first_mut() else {
            return Some(last);
        };
        let result = core::mem::replace(first, last);
        self.bubble_down(0);
        Some(result)
    }

    fn priority(&self, i: usize) -> i64 {
        self.heap.get(i).map_or(i64::MAX, |p| p.0)
    }

    fn bubble_down(&mut self, mut index: usize) {
        loop {
            let left = index.wrapping_mul(2).wrapping_add(1);
            let right = left.wrapping_add(1);
            let len = self.heap.len();
            if left >= len {
                return;
            }
            let has_right = right < len;
            let me = self.priority(index);
            if me <= self.priority(left) && (!has_right || me <= self.priority(right)) {
                return;
            }
            let child = if !has_right || self.priority(left) < self.priority(right) {
                left
            } else {
                right
            };
            self.heap.swap(index, child);
            index = child;
        }
    }

    fn bubble_up(&mut self, mut index: usize) {
        while index != 0 {
            let parent = index.wrapping_sub(1) / 2;
            if self.priority(parent) <= self.priority(index) {
                return;
            }
            self.heap.swap(index, parent);
            index = parent;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )]
    use super::*;

    fn drain(q: &mut PriorityQueue) -> Vec<(i64, u32)> {
        core::iter::from_fn(|| q.pop_minimum()).collect()
    }

    #[test]
    fn pops_in_priority_order() {
        let mut q = PriorityQueue::new();
        for (p, v) in [(10, 0), (3, 1), (7, 2), (1, 3), (8, 4), (2, 5)] {
            q.insert(p, v);
        }
        let got: Vec<i64> = drain(&mut q).iter().map(|x| x.0).collect();
        assert_eq!(got, [1, 2, 3, 7, 8, 10]);
        assert!(q.heap.is_empty());
        assert_eq!(q.pop_minimum(), None);
    }

    // The order of equal priorities is the heap's, as in HarfBuzz
    // (traced by hand from hb-priority-queue.hh).
    #[test]
    fn ties_follow_the_heap() {
        let mut q = PriorityQueue::new();
        for v in 0..5 {
            q.insert(1, v);
        }
        // Heap [0,1,2,3,4]: nothing moves on insert. Each pop moves the
        // last item to the root, which then sinks only past a strictly
        // smaller child, so it stays at the root.
        let got: Vec<u32> = drain(&mut q).iter().map(|x| x.1).collect();
        assert_eq!(got, [0, 4, 3, 2, 1]);

        let mut q = PriorityQueue::new();
        q.insert(5, 0);
        q.insert(1, 1);
        q.insert(1, 2);
        q.insert(1, 3);
        // Insert 1 swaps up past 5: [1:1, 5:0]; insert 2 stays: [1:1,5:0,1:2];
        // insert 3 at index 3 swaps past 5:0 (index 1) and stops at the
        // equal root: [1:1, 1:3, 1:2, 5:0].
        let got: Vec<u32> = drain(&mut q).iter().map(|x| x.1).collect();
        // Pop 1 -> root 5:0, children 1:3 and 1:2 are equal, so it goes
        // right: [1:2, 1:3, 5:0]. Pop 2 -> [5:0, 1:3] -> [1:3, 5:0].
        assert_eq!(got, [1, 2, 3, 0]);
    }

    #[test]
    fn right_child_is_preferred_on_ties() {
        let mut q = PriorityQueue::new();
        for (p, v) in [(0, 0), (2, 1), (2, 2), (9, 3)] {
            q.insert(p, v);
        }
        // Pop 0: last item 9:3 to the root; children 2:1 and 2:2 are
        // equal, so it swaps with the right one, 2:2.
        assert_eq!(q.pop_minimum(), Some((0, 0)));
        assert_eq!(q.pop_minimum(), Some((2, 2)));
        assert_eq!(q.pop_minimum(), Some((2, 1)));
        assert_eq!(q.pop_minimum(), Some((9, 3)));
    }
}
