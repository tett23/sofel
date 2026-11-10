//! HarfBuzz's binary search, shared by the tables that look records up
//! by glyph id.

use core::cmp::Ordering;

/// HarfBuzz's `hb_bsearch_impl`: the index of an entry for which
/// `cmp(i)` (the key compared with entry `i`) is `Equal`, probing the
/// middle of `[min, max]` rounded down. `None` when not found or when
/// an entry cannot be read.
pub(crate) fn hb_bsearch(len: usize, cmp: impl Fn(usize) -> Option<Ordering>) -> Option<usize> {
    let (mut min, mut max) = (0usize, len.checked_sub(1)?);
    while min <= max {
        let mid = min.saturating_add(max) / 2;
        match cmp(mid)? {
            Ordering::Less => max = mid.checked_sub(1)?,
            Ordering::Greater => min = mid.saturating_add(1),
            Ordering::Equal => return Some(mid),
        }
    }
    None
}
