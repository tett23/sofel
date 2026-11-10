//! `RangeSet` against `hb_set_t`'s semantics (ADR 0022).

#![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

use sofel_subset::{INVALID, RangeSet};

#[test]
fn add_remove_merge() {
    let mut s = RangeSet::new();
    s.add_range(10, 20);
    s.add_range(21, 30);
    s.add(5);
    assert_eq!(s.ranges().collect::<Vec<_>>(), vec![(5, 5), (10, 30)]);
    s.remove_range(12, 14);
    assert_eq!(
        s.ranges().collect::<Vec<_>>(),
        vec![(5, 5), (10, 11), (15, 30)]
    );
    s.add_range(4, 16);
    assert_eq!(s.ranges().collect::<Vec<_>>(), vec![(4, 30)]);
    assert_eq!(s.len(), 27);
    assert!(s.contains(4) && s.contains(30) && !s.contains(31));
    s.add_range(9, 3);
    assert_eq!(s.len(), 27);
}

#[test]
fn inverted() {
    let mut s = RangeSet::new();
    s.add_range(1, 2);
    s.invert();
    assert!(s.contains(0) && !s.contains(1) && s.contains(3));
    assert_eq!(s.len(), u64::from(INVALID) - 2);
    assert_eq!(s.min(), 0);
    assert_eq!(s.max(), INVALID - 1);
    assert_eq!(s.next(0), 3);
    s.add(1);
    assert!(s.contains(1));
    s.remove(7);
    assert_eq!(
        s.ranges().take(3).collect::<Vec<_>>(),
        vec![(0, 1), (3, 6), (8, INVALID - 1)]
    );
    assert!(!RangeSet::all().contains(INVALID));
}

#[test]
fn bounds() {
    let mut s = RangeSet::new();
    s.add(0);
    s.add(INVALID - 1);
    s.add(INVALID);
    assert_eq!(s.len(), 2);
    assert_eq!((s.min(), s.max()), (0, INVALID - 1));
    assert_eq!(s.next(INVALID), 0);
    assert_eq!(s.next(0), INVALID - 1);
    assert_eq!(s.next(INVALID - 1), INVALID);
    s.truncate(1);
    assert_eq!(s.iter().collect::<Vec<_>>(), vec![0]);
    let e = RangeSet::new();
    assert_eq!((e.min(), e.max()), (INVALID, INVALID));
}

#[test]
fn union() {
    let mut a = RangeSet::from_values([1, 2, 3]);
    a.union(&RangeSet::from_values([3, 4, 10]));
    assert_eq!(a.ranges().collect::<Vec<_>>(), vec![(1, 4), (10, 10)]);
}
