//! HarfBuzz 15.1.0's `src/graph/serialize.hh`: finding the offsets that
//! would overflow in the current order, and writing the graph out.

use alloc::vec::Vec;

use super::graph::{Graph, GraphError, R};
use super::set::HbSet;
use crate::serialize::{Link, Whence};

/// An overflowing link (`overflow_record_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Overflow {
    pub(crate) parent: u32,
    pub(crate) child: u32,
}

/// `compute_offset`.
fn compute_offset(g: &Graph, parent: u32, link: &Link) -> R<i64> {
    let p = g.v(parent)?;
    let c = g.v(link.objidx)?;
    let offset = match link.whence {
        Whence::Head => c.start.wrapping_sub(p.start),
        Whence::Tail => c.start.wrapping_sub(p.end),
        Whence::Absolute => c.start,
    };
    Ok(i64::from(offset).wrapping_sub(i64::from(link.bias)))
}

/// `is_valid_offset`.
fn is_valid_offset(offset: i64, link: &Link) -> bool {
    if link.width == 0 {
        return link.signed || offset >= 0;
    }
    if link.signed {
        if link.width == 4 {
            return (-(1i64 << 31)..(1i64 << 31)).contains(&offset);
        }
        return (-(1i64 << 15)..(1i64 << 15)).contains(&offset);
    }
    match link.width {
        4 => (0..(1i64 << 32)).contains(&offset),
        3 => (0..(1i64 << 24)).contains(&offset),
        _ => (0..(1i64 << 16)).contains(&offset),
    }
}

/// `will_overflow`: whether an offset overflows in the current order.
/// With `overflows`, every overflowing (parent, child) pair is recorded
/// once, in the order of the parents.
pub(crate) fn will_overflow(g: &mut Graph, mut overflows: Option<&mut Vec<Overflow>>) -> R<bool> {
    if let Some(o) = overflows.as_deref_mut() {
        o.clear();
    }
    g.update_positions();
    let mut children = HbSet::new();
    for &parent in &g.ordering {
        children.clear();
        for link in &g.v(parent)?.real_links {
            let offset = compute_offset(g, parent, link)?;
            if is_valid_offset(offset, link) {
                continue;
            }
            let Some(o) = overflows.as_deref_mut() else {
                return Ok(true);
            };
            if children.has(link.objidx) {
                continue;
            }
            o.push(Overflow {
                parent,
                child: link.objidx,
            });
            children.add(link.objidx);
        }
    }
    Ok(overflows.is_some_and(|o| !o.is_empty()))
}

/// Writes an offset as `hb_serialize_context_t::assign_offset`; false
/// when it does not fit (the field then holds the truncated value).
fn assign_offset(out: &mut [u8], at: usize, link: &Link, offset: u32) -> bool {
    let w = usize::from(link.width);
    let fits = match (link.width, link.signed) {
        (4, true) => offset <= 0x7FFF_FFFF,
        (2, true) => offset <= 0x7FFF,
        (4, false) => true,
        (3, _) => offset <= 0xFF_FFFF,
        _ => offset <= 0xFFFF,
    };
    let b = offset.to_be_bytes();
    if let (Some(dst), Some(src)) = (
        out.get_mut(at..at.saturating_add(w)),
        b.get(4usize.saturating_sub(w)..),
    ) {
        dst.copy_from_slice(src);
    }
    fits
}

/// `graph::serialize`: the bytes of the vertices in the current order
/// with their offsets written.
pub(crate) fn serialize(g: &Graph) -> R<Vec<u8>> {
    let size = g.total_size_in_bytes();
    if size == 0 {
        return Ok(Vec::new());
    }
    if size > i32::MAX as usize {
        return Err(GraphError::AllocationFailure);
    }
    // The objects are packed from the last of the order to the first, so
    // a link only reaches a child packed before its parent, and an empty
    // object is never packed (its links write nothing).
    let n = g.len();
    let mut starts: Vec<Option<usize>> = alloc::vec![None; n];
    let mut pos = 0usize;
    for &i in &g.ordering {
        let s = g.table_size(i);
        if s > 0
            && let Some(slot) = starts.get_mut(i as usize)
        {
            *slot = Some(pos);
        }
        pos = pos.wrapping_add(s);
    }
    let mut out: Vec<u8> = Vec::with_capacity(size);
    for &i in &g.ordering {
        out.extend_from_slice(g.bytes(i));
    }
    let mut packed = alloc::vec![false; n];
    let mut other_error = false;
    let mut overflow = false;
    let mut pending: Vec<(usize, Link, usize, usize)> = Vec::new();
    for &i in g.ordering.iter().rev() {
        let v = g.v(i)?;
        let size = v.table_size();
        let head = starts.get(i as usize).copied().flatten();
        for link in &v.real_links {
            if link.width == 0 {
                continue;
            }
            let p = link.position as usize;
            if p > size || size.wrapping_sub(p) < usize::from(link.width) {
                other_error = true;
                continue;
            }
            let Some(head) = head else { continue };
            if let Some(s) = out.get_mut(
                head.saturating_add(p)
                    ..head
                        .saturating_add(p)
                        .saturating_add(usize::from(link.width)),
            ) {
                s.fill(0);
            }
            let child_packed = packed.get(link.objidx as usize).copied().unwrap_or(false);
            let child_start = starts.get(link.objidx as usize).copied().flatten();
            if let (true, Some(cs)) = (child_packed, child_start) {
                pending.push((head, *link, size, cs));
            }
        }
        if let Some(p) = packed.get_mut(i as usize) {
            *p = true;
        }
    }
    if other_error {
        return Err(GraphError::AllocationFailure);
    }
    for (head, link, size, child) in pending {
        let to32 = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        let offset = match link.whence {
            Whence::Head => to32(child).wrapping_sub(to32(head)),
            Whence::Tail => to32(child).wrapping_sub(to32(head.wrapping_add(size))),
            Whence::Absolute => to32(child),
        }
        .wrapping_sub(link.bias);
        overflow |= !assign_offset(
            &mut out,
            head.wrapping_add(link.position as usize),
            &link,
            offset,
        );
    }
    if overflow {
        return Err(GraphError::OverflowResolutionFailed);
    }
    Ok(out)
}
