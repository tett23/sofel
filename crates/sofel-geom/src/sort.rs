//! Sorting with the tie order of HarfBuzz 15.1.0, from hb-algs.hh.
//! Copyright © the HarfBuzz authors. See THIRD_PARTY_NOTICES.md.
use core::cmp::Ordering;

/// Orders records exactly as HarfBuzz does, including ties.
pub fn harfbuzz_sort<T: Copy + Default>(base: &mut [T], cmp: &dyn Fn(&T, &T) -> Ordering) {
    let depth = 2u32.saturating_mul(usize::BITS.saturating_sub(base.len().leading_zeros()));
    qsort_loop(base, cmp, depth);
    for i in 1..base.len() {
        let mut j = i;
        while j > 0 {
            let (Some(a), Some(b)) = (base.get(j.saturating_sub(1)), base.get(j)) else {
                break;
            };
            if cmp(a, b) != Ordering::Greater {
                break;
            }
            base.swap(j.saturating_sub(1), j);
            j = j.saturating_sub(1);
        }
    }
}

fn at<T: Copy + Default>(base: &[T], i: usize) -> T {
    base.get(i).copied().unwrap_or_default()
}

fn qsort_loop<T: Copy + Default>(
    mut base: &mut [T],
    cmp: &dyn Fn(&T, &T) -> Ordering,
    mut depth: u32,
) {
    while base.len() > 24 {
        if depth == 0 {
            heapsort(base, cmp);
            return;
        }
        depth = depth.saturating_sub(1);
        let nel = base.len();
        let last = nel.saturating_sub(1);
        let mid = nel / 2;
        let c = |base: &[T], i: usize, j: usize| cmp(&at(base, i), &at(base, j));
        if c(base, 0, mid) == Ordering::Greater {
            base.swap(0, mid);
        }
        if c(base, mid, last) == Ordering::Greater {
            base.swap(mid, last);
            if c(base, 0, mid) == Ordering::Greater {
                base.swap(0, mid);
            }
        }
        let p = last.saturating_sub(1);
        base.swap(mid, p);
        let pivot = at(base, p);
        let (mut i, mut j) = (0usize, p);
        loop {
            i = i.saturating_add(1);
            while cmp(&at(base, i), &pivot) == Ordering::Less {
                i = i.saturating_add(1);
            }
            j = j.saturating_sub(1);
            while cmp(&at(base, j), &pivot) == Ordering::Greater {
                j = j.saturating_sub(1);
            }
            if i >= j {
                break;
            }
            base.swap(i, j);
        }
        base.swap(i, p);
        let left = i;
        let right = nel.saturating_sub(left).saturating_sub(1);
        let (l, r) = base.split_at_mut(i);
        let r = r.get_mut(1..).unwrap_or_default();
        if left < right {
            qsort_loop(l, cmp, depth);
            base = r;
        } else {
            qsort_loop(r, cmp, depth);
            base = l;
        }
    }
}

fn heapsort<T: Copy + Default>(base: &mut [T], cmp: &dyn Fn(&T, &T) -> Ordering) {
    let sift = |base: &mut [T], mut root: usize, end: usize| loop {
        let child = root.saturating_mul(2).saturating_add(1);
        if child >= end {
            break;
        }
        let mut child = child;
        if child.saturating_add(1) < end
            && cmp(&at(base, child), &at(base, child.saturating_add(1))) == Ordering::Less
        {
            child = child.saturating_add(1);
        }
        if cmp(&at(base, root), &at(base, child)) != Ordering::Less {
            break;
        }
        base.swap(root, child);
        root = child;
    };
    let nel = base.len();
    if nel < 2 {
        return;
    }
    for i in (0..nel / 2).rev() {
        sift(base, i, nel);
    }
    for end in (1..nel).rev() {
        base.swap(0, end);
        sift(base, 0, end);
    }
}
