//! Splitting MarkBasePos subtables (HarfBuzz 15.1.0's
//! `src/graph/markbasepos-graph.hh`).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::graph::{Graph, GraphError, R};
use super::gsubgpos::{Context, actuate_split};
use super::layout::{add_coverage, coverage_of, make_coverage};
use super::set::HbSet;

/// `MarkBasePosFormat1_2<SmallTypes>::static_size`.
const MBP_SIZE: usize = 12;
/// `MarkRecord<SmallTypes>::static_size`.
const MARK_RECORD: u32 = 4;

/// `AnchorMatrix::sanitize`.
pub(crate) fn anchor_matrix_sanitize(len: usize, rows: u32, class_count: u32) -> R<()> {
    if len < 2 {
        return Err(GraphError::SanitizeFailure);
    }
    let need = 2u64
        .saturating_mul(u64::from(class_count))
        .saturating_mul(u64::from(rows));
    if (len.wrapping_sub(2) as u64) < need {
        return Err(GraphError::SanitizeFailure);
    }
    Ok(())
}

fn mark_array_sanitize(g: &Graph, id: u32) -> R<()> {
    let len = g.table_size(id);
    if len < 2 {
        return Err(GraphError::SanitizeFailure);
    }
    let count = g.rd16(id, 0) as usize;
    if len < 2usize.saturating_add(count.saturating_mul(MARK_RECORD as usize)) {
        return Err(GraphError::SanitizeFailure);
    }
    Ok(())
}

fn not_null(g: &Graph, id: u32) -> R<u32> {
    if g.is_null(id) {
        return Err(GraphError::InvalidArgument);
    }
    Ok(id)
}

/// `as_table<MarkArray>` (`mutable`: `as_mutable_table`).
fn mark_array(g: &mut Graph, this: u32, mutable: bool) -> R<u32> {
    let id = if mutable {
        g.mutable_index_for_offset(this, 8)?
    } else {
        g.index_for_offset(this, 8)?
    };
    not_null(g, id)?;
    mark_array_sanitize(g, id)?;
    Ok(id)
}

/// `as_table<AnchorMatrix>` with `class_count`.
fn anchor_matrix(g: &mut Graph, this: u32, class_count: u32, mutable: bool) -> R<u32> {
    let id = if mutable {
        g.mutable_index_for_offset(this, 10)?
    } else {
        g.index_for_offset(this, 10)?
    };
    not_null(g, id)?;
    anchor_matrix_sanitize(g.table_size(id), g.rd16(id, 0), class_count)?;
    Ok(id)
}

/// The class of mark record `mark` (0 past the array, as HarfBuzz's
/// `Null (MarkRecord)`).
fn mark_class(g: &Graph, ma: u32, mark: u32) -> u32 {
    if mark >= g.rd16(ma, 0) {
        return 0;
    }
    g.rd16(ma, 2usize.wrapping_add((mark as usize).wrapping_mul(4)))
}

#[derive(Clone, Debug, Default)]
struct ClassInfo {
    marks: HbSet,
    child_indices: Vec<u32>,
}

struct Split {
    this: u32,
    class_to_info: Vec<ClassInfo>,
    mark_array_links: BTreeMap<u32, u32>,
}

impl Split {
    fn marks_for(&self, start: u32, end: u32) -> HbSet {
        (start..end)
            .filter_map(|k| self.class_to_info.get(k as usize))
            .flat_map(|i| i.marks.iter())
            .collect()
    }
}

/// `MarkBasePos::split_subtables` (after `split_subtable`'s sanitize
/// check).
pub(crate) fn split(c: &mut Context, g: &mut Graph, this: u32) -> R<Vec<u32>> {
    let _ = c;
    let len = g.table_size(this);
    if g.is_null(this) || len < 2 || g.rd16(this, 0) != 1 || len < MBP_SIZE {
        return Ok(Vec::new());
    }
    let mut visited = HbSet::new();
    let base_coverage_id = g.index_for_offset(this, 4)?;
    let base_size = u32::try_from(
        MBP_SIZE
            .wrapping_add(4)
            .wrapping_add(g.table_size(base_coverage_id)),
    )
    .unwrap_or(u32::MAX);
    let class_to_info = class_info(g, this)?;
    let class_count = g.rd16(this, 6);
    let base_array = anchor_matrix(g, this, class_count, false)?;
    let base_count = g.rd16(base_array, 0);

    let mut partial_coverage_size = 4u32;
    let mut accumulated = base_size;
    let mut split_points = Vec::new();
    for klass in 0..class_count {
        let Some(info) = class_to_info.get(klass as usize) else {
            break;
        };
        let marks = info.marks.population();
        partial_coverage_size = partial_coverage_size.wrapping_add(marks.wrapping_mul(2));
        let mut delta = MARK_RECORD
            .wrapping_mul(marks)
            .wrapping_add(base_count.wrapping_mul(2));
        for &obj in &info.child_indices {
            let s = g
                .find_subgraph_size(obj, &mut visited, u32::MAX)
                .unwrap_or(0);
            delta = delta.wrapping_add(u32::try_from(s).unwrap_or(u32::MAX));
        }
        accumulated = accumulated.wrapping_add(delta);
        let total = accumulated.wrapping_add(partial_coverage_size);
        if total >= 1 << 16 {
            split_points.push(klass);
            accumulated = base_size.wrapping_add(delta);
            partial_coverage_size = 4u32.wrapping_add(marks.wrapping_mul(2));
            visited.clear();
        }
    }

    let mark_array_id = g.index_for_offset(this, 8)?;
    let mark_array_links = g.v(mark_array_id)?.position_to_index_map();
    let sc = Split {
        this,
        class_to_info,
        mark_array_links,
    };
    actuate_split(
        g,
        &split_points,
        class_count,
        |g, start, end| clone_range(g, &sc, start, end),
        |g, n| shrink(g, &sc, n),
    )
}

/// `get_class_info`.
fn class_info(g: &mut Graph, this: u32) -> R<Vec<ClassInfo>> {
    let class_count = g.rd16(this, 6);
    if class_count == 0 {
        return Ok(Vec::new());
    }
    let mut info = alloc::vec![ClassInfo::default(); class_count as usize];
    let ma = mark_array(g, this, false)?;
    for mark in 0..g.rd16(ma, 0) {
        let klass = mark_class(g, ma, mark);
        if let Some(i) = info.get_mut(klass as usize) {
            i.marks.add(mark);
        }
    }
    for l in &g.v(ma)?.real_links {
        let mark = l.position.wrapping_sub(2) / MARK_RECORD;
        let klass = mark_class(g, ma, mark);
        if let Some(i) = info.get_mut(klass as usize) {
            i.child_indices.push(l.objidx);
        }
    }
    let base_array_id = g.index_for_offset(this, 10)?;
    for l in &g.v(base_array_id)?.real_links {
        let index = l.position.wrapping_sub(2) / 2;
        let klass = index.checked_rem(class_count).unwrap_or(0);
        if let Some(i) = info.get_mut(klass as usize) {
            i.child_indices.push(l.objidx);
        }
    }
    Ok(info)
}

fn shrink(g: &mut Graph, sc: &Split, count: u32) -> R<()> {
    let this = sc.this;
    let old = g.rd16(this, 6);
    if count >= old {
        return Ok(());
    }
    g.wr16(this, 6, count);
    let cov = g.mutable_index_for_offset(this, 2)?;
    let glyphs = coverage_of(g, cov)?;
    let marks = sc.marks_for(0, count);
    let kept: Vec<u32> = glyphs
        .into_iter()
        .zip(0u32..)
        .filter(|&(_, i)| marks.has(i))
        .map(|(gl, _)| gl)
        .collect();
    make_coverage(
        g,
        &kept,
        cov,
        4usize.wrapping_add(2usize.wrapping_mul(marks.population() as usize)),
    )?;
    let base_array = anchor_matrix(g, this, old, true)?;
    anchor_matrix_shrink(g, base_array, old, count)?;
    let ma = mark_array(g, this, true)?;
    mark_array_shrink(g, &sc.mark_array_links, ma, count)
}

/// `AnchorMatrix::shrink`.
fn anchor_matrix_shrink(g: &mut Graph, id: u32, old_count: u32, new_count: u32) -> R<()> {
    if new_count >= old_count {
        return Err(GraphError::InvalidArgument);
    }
    let rows = g.rd16(id, 0) as usize;
    g.set_table_len(
        id,
        2usize.wrapping_add(rows.wrapping_mul(2).wrapping_mul(new_count as usize)),
    );
    let v = g.vm(id)?;
    for l in &mut v.real_links {
        let index = l.position.wrapping_sub(2) / 2;
        let base = index.checked_div(old_count).unwrap_or(0);
        let klass = index.checked_rem(old_count).unwrap_or(0);
        if klass >= new_count {
            return Err(GraphError::InvalidArgument);
        }
        let new_index = base.wrapping_mul(new_count).wrapping_add(klass);
        l.position = 2u32.wrapping_add(new_index.wrapping_mul(2));
    }
    Ok(())
}

/// `AnchorMatrix::clone`.
fn anchor_matrix_clone(g: &mut Graph, id: u32, start: u32, end: u32, class_count: u32) -> R<u32> {
    let rows = g.rd16(id, 0);
    let new_count = end.wrapping_sub(start);
    let size = 2usize.wrapping_add(
        2usize
            .wrapping_mul(new_count as usize)
            .wrapping_mul(rows as usize),
    );
    let prime = Context::create_node(g, size)?;
    g.wr16(prime, 0, rows);
    let mut i = 0usize;
    while let Some(&link) = g.v(id)?.real_links.get(i) {
        let old_index = link.position.wrapping_sub(2) / 2;
        let Some(klass) = old_index.checked_rem(class_count) else {
            i = i.wrapping_add(1);
            continue;
        };
        if klass < start || klass >= end {
            i = i.wrapping_add(1);
            continue;
        }
        let base = old_index.checked_div(class_count).unwrap_or(0);
        let new_index = base
            .wrapping_mul(new_count)
            .wrapping_add(klass.wrapping_sub(start));
        let child = link.objidx;
        g.add_link(2u32.wrapping_add(new_index.wrapping_mul(2)), prime, child)?;
        g.vm(child)?.remove_parent(id);
        g.vm(id)?.remove_real_link_unordered(i);
    }
    g.vm(id)?.sort_real_links();
    Ok(prime)
}

/// `MarkArray::shrink`.
fn mark_array_shrink(
    g: &mut Graph,
    mark_array_links: &BTreeMap<u32, u32>,
    id: u32,
    new_count: u32,
) -> R<()> {
    let links: Vec<u32> = g.v(id)?.real_links.iter().map(|l| l.objidx).collect();
    for c in links {
        g.vm(c)?.remove_parent(id);
    }
    g.vm(id)?.real_links.clear();
    let mut new_index = 0u32;
    for rec in 0..g.rd16(id, 0) {
        let klass = g.rd16(id, 2usize.wrapping_add((rec as usize).wrapping_mul(4)));
        if klass >= new_count {
            continue;
        }
        if new_index < g.rd16(id, 0) {
            g.wr16(
                id,
                2usize.wrapping_add((new_index as usize).wrapping_mul(4)),
                klass,
            );
        }
        let position = 2u32.wrapping_add(rec.wrapping_mul(4)).wrapping_add(2);
        let Some(&obj) = mark_array_links.get(&position) else {
            new_index = new_index.wrapping_add(1);
            continue;
        };
        g.add_link(4u32.wrapping_add(new_index.wrapping_mul(4)), id, obj)?;
        new_index = new_index.wrapping_add(1);
    }
    g.wr16(id, 0, new_index);
    g.set_table_len(
        id,
        2usize.wrapping_add((MARK_RECORD as usize).wrapping_mul(new_index as usize)),
    );
    Ok(())
}

/// `MarkArray::clone`.
fn mark_array_clone(
    g: &mut Graph,
    id: u32,
    pos_to_index: &BTreeMap<u32, u32>,
    marks: &HbSet,
    start_class: u32,
) -> R<u32> {
    let n = marks.population();
    let size = 2usize.wrapping_add((MARK_RECORD as usize).wrapping_mul(n as usize));
    let prime = Context::create_node(g, size)?;
    g.wr16(prime, 0, n);
    for (i, mark) in marks.iter().enumerate() {
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        let klass = mark_class(g, id, mark).wrapping_sub(start_class);
        g.wr16(
            prime,
            2usize.wrapping_add((i as usize).wrapping_mul(4)),
            klass,
        );
        let offset_pos = 4u32.wrapping_add(mark.wrapping_mul(4));
        if let Some(&anchor) = pos_to_index.get(&offset_pos) {
            g.add_link(4u32.wrapping_add(i.wrapping_mul(4)), prime, anchor)?;
        }
    }
    let mut j = 0usize;
    while let Some(&link) = g.v(id)?.real_links.get(j) {
        let mark = link.position.wrapping_sub(2) / MARK_RECORD;
        if !marks.has(mark) {
            j = j.wrapping_add(1);
            continue;
        }
        g.vm(link.objidx)?.remove_parent(id);
        g.vm(id)?.remove_real_link_unordered(j);
    }
    g.vm(id)?.sort_real_links();
    Ok(prime)
}

fn clone_range(g: &mut Graph, sc: &Split, start: u32, end: u32) -> R<u32> {
    let this = sc.this;
    let prime = Context::create_node(g, MBP_SIZE)?;
    g.wr16(prime, 0, g.rd16(this, 0));
    g.wr16(prime, 6, end.wrapping_sub(start));
    let base_coverage_id = g.index_for_offset(this, 4)?;
    g.add_link(4, prime, base_coverage_id)?;
    let mark_coverage = g.index_for_offset(this, 2)?;
    let glyphs = coverage_of(g, mark_coverage)?;
    let marks = sc.marks_for(start, end);
    let kept: Vec<u32> = glyphs
        .into_iter()
        .zip(0u32..)
        .filter(|&(_, i)| marks.has(i))
        .map(|(gl, _)| gl)
        .collect();
    add_coverage(
        g,
        prime,
        2,
        &kept,
        (marks.population() as usize)
            .wrapping_mul(2)
            .wrapping_add(4),
    )?;
    let ma = mark_array(g, this, true)?;
    let new_mark_array = mark_array_clone(g, ma, &sc.mark_array_links, &marks, start)?;
    g.add_link(8, prime, new_mark_array)?;
    let class_count = g.rd16(this, 6);
    let base_array = anchor_matrix(g, this, class_count, true)?;
    let new_base_array = anchor_matrix_clone(g, base_array, start, end, g.rd16(this, 6))?;
    g.add_link(10, prime, new_base_array)?;
    Ok(prime)
}
