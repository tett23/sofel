//! HarfBuzz 15.1.0's offset overflow resolution, the repacker
//! (`src/hb-repacker.hh`, `src/graph/`, `docs/repacker.md`).
//!
//! When an offset of a serialized table does not fit in its field, the
//! objects are reordered (a topological sort by shortest distance from
//! the root), the subgraphs under 32-bit (and 24-bit) offsets are moved to
//! their own spaces, shared objects are duplicated and objects are moved
//! closer to their parents, round by round. For GSUB and GPOS, large
//! PairPos, MarkBasePos and LigatureSubst subtables are split and lookups
//! are promoted to extension lookups. Every step, limit and tie-break is
//! HarfBuzz's, so the result is byte for byte HarfBuzz's.

mod graph;
mod gsubgpos;
mod hbmap;
mod layout;
mod ligature;
mod markbasepos;
mod pack;
mod pairpos;
mod queue;
mod set;

#[cfg(test)]
mod tests;

use alloc::vec::Vec;

pub use graph::GraphError;
use graph::{Graph, R};
use gsubgpos::{Context, GPOS, GSUB};
use pack::{Overflow, will_overflow};
use set::HbSet;
use sofel_read::Tag;

use crate::WriteError;
use crate::serialize::{Object, Packed, Whence};

/// `HB_REPACKER_MAX_ITERATIONS`.
const MAX_ITERATIONS: u32 = 500;

/// `lookup_size_t`.
#[derive(Clone, Copy, Debug)]
struct LookupSize {
    lookup_index: u32,
    size: usize,
    num_subtables: u32,
}

/// `lookup_size_t::cmp`: the most subtables per byte first, then the
/// highest lookup index.
fn lookup_size_cmp(a: &LookupSize, b: &LookupSize) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    let spb_a = f64::from(a.num_subtables) / a.size as f64;
    let spb_b = f64::from(b.num_subtables) / b.size as f64;
    if spb_a == spb_b {
        return b.lookup_index.cmp(&a.lookup_index);
    }
    let d = spb_b - spb_a;
    if d < 0.0 {
        Ordering::Less
    } else if d > 0.0 {
        Ordering::Greater
    } else {
        b.lookup_index.cmp(&a.lookup_index)
    }
}

/// `_presplit_subtables_if_needed`.
fn presplit_subtables_if_needed(c: &mut Context, g: &mut Graph) -> R<()> {
    let lookups: Vec<u32> = c.lookups.iter().copied().collect();
    for li in lookups {
        c.split_subtables_if_needed(g, li)?;
    }
    Ok(())
}

/// `_promote_extensions_if_needed`.
fn promote_extensions_if_needed(c: &mut Context, g: &mut Graph) -> R<()> {
    if c.lookups.is_empty() {
        return Ok(());
    }
    let mut total_lookup_table_sizes = 0u32;
    let mut lookup_sizes = Vec::with_capacity(c.lookups.len());
    for &li in &c.lookups {
        total_lookup_table_sizes = total_lookup_table_sizes
            .wrapping_add(u32::try_from(g.table_size(li)).unwrap_or(u32::MAX));
        let mut visited = HbSet::new();
        lookup_sizes.push(LookupSize {
            lookup_index: li,
            size: g
                .find_subgraph_size(li, &mut visited, u32::MAX)
                .unwrap_or(0),
            num_subtables: gsubgpos::number_of_subtables(g, li),
        });
    }
    lookup_sizes.sort_by(lookup_size_cmp);

    let lookup_list_size = g.table_size(c.lookup_list_index);
    let l2_l3_size = lookup_list_size.wrapping_add(total_lookup_table_sizes as usize);
    let mut l3_l4_size = total_lookup_table_sizes as usize;
    let mut l4_plus_size = 0usize;
    for p in &lookup_sizes {
        let subtables_size = p.num_subtables.wrapping_mul(8) as usize;
        l3_l4_size = l3_l4_size.wrapping_add(subtables_size);
        l4_plus_size = l4_plus_size.wrapping_add(subtables_size);
    }

    let mut layers_full = false;
    for p in &lookup_sizes {
        if gsubgpos::is_extension(g, p.lookup_index, c.table_tag) {
            continue;
        }
        if !layers_full {
            let lookup_size = g.table_size(p.lookup_index);
            let mut visited = HbSet::new();
            let subtables_size = g
                .find_subgraph_size(p.lookup_index, &mut visited, 1)
                .unwrap_or(0)
                .wrapping_sub(lookup_size);
            let remaining_size = p
                .size
                .wrapping_sub(subtables_size)
                .wrapping_sub(lookup_size);
            l3_l4_size = l3_l4_size.wrapping_add(subtables_size);
            l3_l4_size = l3_l4_size.wrapping_sub(p.num_subtables.wrapping_mul(8) as usize);
            l4_plus_size = l4_plus_size
                .wrapping_add(subtables_size)
                .wrapping_add(remaining_size);
            if l2_l3_size < 1 << 16 && l3_l4_size < 1 << 16 && l4_plus_size < 1 << 16 {
                continue;
            }
            layers_full = true;
        }
        c.make_extension(g, p.lookup_index)?;
    }
    Ok(())
}

/// `_try_isolating_subgraphs`.
fn try_isolating_subgraphs(overflows: &[Overflow], g: &mut Graph) -> R<bool> {
    let mut space = 0u32;
    let mut roots_to_isolate = HbSet::new();
    for r in overflows.iter().rev() {
        let (overflow_space, root) = g.space_for(r.parent);
        if overflow_space == 0 {
            continue;
        }
        if g.num_roots_for_space(overflow_space) <= 1 {
            continue;
        }
        if space == 0 {
            space = overflow_space;
        }
        if space == overflow_space {
            roots_to_isolate.add(root);
        }
    }
    if roots_to_isolate.is_empty() {
        return Ok(false);
    }
    let maximum_to_move = (g.num_roots_for_space(space) / 2).max(1);
    if roots_to_isolate.population() > maximum_to_move {
        let mut extra = roots_to_isolate.population().wrapping_sub(maximum_to_move);
        for &id in &g.ordering {
            if extra == 0 {
                break;
            }
            if roots_to_isolate.has(id) {
                roots_to_isolate.del(id);
                extra = extra.wrapping_sub(1);
            }
        }
    }
    g.isolate_subgraph(&mut roots_to_isolate)?;
    g.move_to_new_space(&roots_to_isolate)?;
    Ok(true)
}

/// `_resolve_shared_overflow`.
fn resolve_shared_overflow(overflows: &[Overflow], index: usize, g: &mut Graph) -> R<bool> {
    let Some(r) = overflows.get(index) else {
        return Ok(false);
    };
    let mut parents = HbSet::new();
    parents.add(r.parent);
    for r2 in overflows.get(..index).unwrap_or(&[]).iter().rev() {
        if r2.child == r.child {
            parents.add(r2.parent);
        }
    }
    let mut result = g.duplicate_parents(&parents, r.child);
    if result == Err(GraphError::InvalidArgument) && parents.population() > 2 {
        if let Some(m) = parents.min() {
            parents.del(m);
        }
        result = g.duplicate_parents(&parents, r.child);
    }
    let clone = match result {
        Ok(c) => c,
        Err(GraphError::InvalidArgument) => return Ok(false),
        Err(e) => return Err(e),
    };
    if parents.population() > 1 {
        g.vm(clone)?.give_max_priority();
    }
    Ok(true)
}

/// `_process_overflows`.
fn process_overflows(overflows: &[Overflow], bumped: &mut HbSet, g: &mut Graph) -> R<bool> {
    let mut attempted = false;
    for (i, r) in overflows.iter().enumerate().rev() {
        if g.v(r.child)?.is_shared() && resolve_shared_overflow(overflows, i, g)? {
            return Ok(true);
        }
        if g.v(r.child)?.is_leaf() && !bumped.has(r.parent) {
            let raised = g.raise_childrens_priority(r.parent)?;
            bumped.add(r.parent);
            if raised {
                attempted = true;
            }
            continue;
        }
    }
    Ok(attempted)
}

/// `_assign_spaces_and_sort`.
fn assign_spaces_and_sort(g: &mut Graph) -> R<()> {
    if g.assign_spaces()? {
        g.sort_shortest_distance()
    } else {
        g.sort_shortest_distance_if_needed()
    }
}

/// `_gsub_gpos_specialization`.
fn gsub_gpos_specialization(tag: u32, always_recalculate_extensions: bool, g: &mut Graph) -> R<()> {
    if !always_recalculate_extensions {
        return assign_spaces_and_sort(g);
    }
    let mut c = match Context::create(tag, g) {
        Ok(c) => c,
        Err(GraphError::SanitizeFailure) => return assign_spaces_and_sort(g),
        Err(e) => return Err(e),
    };
    presplit_subtables_if_needed(&mut c, g)?;
    promote_extensions_if_needed(&mut c, g)?;
    g.sort_shortest_distance_if_needed()?;
    if !will_overflow(g, None)? {
        return Ok(());
    }
    assign_spaces_and_sort(g)
}

/// `hb_resolve_graph_overflows`.
fn resolve_graph_overflows(
    tag: u32,
    max_rounds: u32,
    always_recalculate_extensions: bool,
    g: &mut Graph,
) -> R<()> {
    g.sort_shortest_distance()?;
    if !will_overflow(g, None)? {
        return Ok(());
    }
    let is_gsub_or_gpos = tag == GPOS || tag == GSUB;
    if is_gsub_or_gpos {
        gsub_gpos_specialization(tag, always_recalculate_extensions, g)?;
    }
    let mut round = 0u32;
    let mut total_iterations = 0u32;
    let mut overflows = Vec::new();
    while round < max_rounds && total_iterations < MAX_ITERATIONS {
        if !will_overflow(g, Some(&mut overflows))? {
            break;
        }
        total_iterations = total_iterations.wrapping_add(1);
        let mut bumped = HbSet::new();
        if !try_isolating_subgraphs(&overflows, g)? {
            round = round.wrapping_add(1);
            if !process_overflows(&overflows, &mut bumped, g)? {
                break;
            }
        }
        g.sort_shortest_distance()?;
    }
    if will_overflow(g, None)? {
        if is_gsub_or_gpos && !always_recalculate_extensions {
            return resolve_graph_overflows(tag, max_rounds, true, g);
        }
        return Err(GraphError::OverflowResolutionFailed);
    }
    Ok(())
}

/// `hb_resolve_overflows`: repacks the packed objects (index 0 the null
/// object, the last the root) and serializes them.
fn resolve_overflows(
    packed: &[Object],
    tag: u32,
    max_rounds: u32,
    recalculate_extensions: bool,
) -> R<Vec<u8>> {
    let mut g = Graph::create(packed)?;
    resolve_graph_overflows(tag, max_rounds, recalculate_extensions, &mut g)?;
    pack::serialize(&g)
}

fn to_write_error(e: GraphError) -> WriteError {
    match e {
        GraphError::OverflowResolutionFailed => WriteError::OffsetOverflow,
        e => WriteError::Repack(e),
    }
}

/// The objects with their offsets written as the serializer's
/// `resolve_links` leaves them: the repacker reads some offset fields
/// (whether GSUB/GPOS 1.2's `lookupList2` is set).
fn with_resolved_offsets(objs: &[Object]) -> Vec<Object> {
    let mut starts = alloc::vec![0usize; objs.len()];
    let mut pos = 0usize;
    for (i, o) in objs.iter().enumerate().skip(1).rev() {
        if let Some(s) = starts.get_mut(i) {
            *s = pos;
        }
        pos = pos.saturating_add(o.bytes.len());
    }
    let mut out: Vec<Object> = objs.to_vec();
    for (i, o) in out.iter_mut().enumerate().skip(1) {
        let head = starts.get(i).copied().unwrap_or(0);
        let tail = head.saturating_add(o.bytes.len());
        for l in &o.real_links {
            let Some(&child) = starts.get(l.objidx as usize).filter(|_| l.objidx != 0) else {
                return objs.to_vec();
            };
            let to32 = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
            let base = match l.whence {
                Whence::Head => head,
                Whence::Tail => tail,
                Whence::Absolute => 0,
            };
            let offset = to32(child).wrapping_sub(to32(base)).wrapping_sub(l.bias);
            crate::serialize::write_offset(
                &mut o.bytes,
                l.position as usize,
                l.width,
                l.signed,
                offset,
            );
        }
    }
    out
}

impl Packed {
    /// Lays the objects out, repacking them when an offset overflows, as
    /// HarfBuzz's `_hb_subset_repack` does for the table `tag` (GSUB and
    /// GPOS subtables may be split and their lookups promoted to
    /// extension lookups).
    ///
    /// # Errors
    ///
    /// [`WriteError::OffsetOverflow`] when the overflows cannot be
    /// resolved, [`WriteError::Repack`] when the graph cannot be repacked,
    /// and [`WriteError::Serialize`] when the objects are not valid.
    pub fn repack(&self, tag: Tag) -> Result<Vec<u8>, WriteError> {
        match self.resolve() {
            Ok(b) => Ok(b),
            Err(e) if e == crate::Errors::OFFSET_OVERFLOW => {
                let objs = with_resolved_offsets(self.objects());
                resolve_overflows(&objs, u32::from_be_bytes(tag.0), 32, false)
                    .map_err(to_write_error)
            }
            Err(e) => Err(WriteError::Serialize(e)),
        }
    }
}

/// Serializes caller-built objects into one table, repacking them to
/// resolve offset overflows (HarfBuzz's `hb_subset_serialize_or_fail`).
///
/// `objects` are the objects without the null object; the last is the
/// root. A link's `objidx` names `objects[objidx - 1]` (index 0 is the
/// null object, which no link may name). For `GSUB` and `GPOS`, subtables
/// may be split and lookups promoted to extension lookups; pass another
/// tag (such as `Tag(*b"\0\0\0\0")`) to disable that.
///
/// # Errors
///
/// [`WriteError::OffsetOverflow`] when the overflows cannot be resolved
/// and [`WriteError::Repack`] when the objects do not form a valid graph.
pub fn serialize_objects(tag: Tag, objects: &[Object]) -> Result<Vec<u8>, WriteError> {
    let mut packed = Vec::with_capacity(objects.len().saturating_add(1));
    packed.push(Object::default());
    packed.extend_from_slice(objects);
    resolve_overflows(&packed, u32::from_be_bytes(tag.0), 20, true).map_err(to_write_error)
}
