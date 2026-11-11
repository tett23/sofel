//! Splitting LigatureSubst subtables (HarfBuzz 15.1.0's
//! `src/graph/ligature-graph.hh`).

use alloc::vec::Vec;

use super::graph::{Graph, GraphError, INVALID, R};
use super::gsubgpos::{Context, actuate_split};
use super::layout::{coverage_glyphs, filter_coverage, make_coverage};
use super::set::HbSet;

/// `LigatureSubstFormat1_2<SmallTypes>::min_size`.
const LS_MIN: u32 = 6;

fn liga_set_position(i: u32) -> u32 {
    LS_MIN.wrapping_add(i.wrapping_mul(2))
}

/// `LigatureSet::sanitize`.
fn liga_set_sanitize(g: &Graph, id: u32) -> R<()> {
    let len = g.table_size(id);
    if len < 2 {
        return Err(GraphError::SanitizeFailure);
    }
    let count = g.rd16(id, 0) as usize;
    if len < 2usize.saturating_add(count.saturating_mul(2)) {
        return Err(GraphError::SanitizeFailure);
    }
    Ok(())
}

/// `as_table<LigatureSet>` / `as_mutable_table<LigatureSet>` of set `i`.
fn liga_set(g: &mut Graph, this: u32, i: u32, mutable: bool) -> R<u32> {
    let id = if mutable {
        g.mutable_index_for_offset(this, liga_set_position(i))?
    } else {
        g.index_for_offset(this, liga_set_position(i))?
    };
    if g.is_null(id) {
        return Err(GraphError::InvalidArgument);
    }
    liga_set_sanitize(g, id)?;
    Ok(id)
}

/// `LigatureSubst::split_subtables` (after `split_subtable`'s sanitize
/// check).
pub(crate) fn split(c: &mut Context, g: &mut Graph, this: u32) -> R<Vec<u32>> {
    let _ = c;
    let len = g.table_size(this);
    if g.is_null(this) || len < 2 || g.rd16(this, 0) != 1 {
        return Ok(Vec::new());
    }
    let count = g.rd16(this, 4) as usize;
    if len < LS_MIN as usize || len < (LS_MIN as usize).saturating_add(count.saturating_mul(2)) {
        return Ok(Vec::new());
    }
    let split_points = compute_split_points(g, this)?;
    if split_points.is_empty() {
        return Ok(Vec::new());
    }
    let total = total_number_ligas(g, this)?;
    let counts = liga_counts(g, this)?;
    actuate_split(
        g,
        &split_points,
        total,
        |g, start, end| clone_range(g, this, &counts, start, end),
        |g, n| shrink(g, this, total, &counts, n),
    )
}

fn total_number_ligas(g: &mut Graph, this: u32) -> R<u32> {
    let mut total = 0u32;
    for i in 0..g.rd16(this, 4) {
        let ls = liga_set(g, this, i, false)?;
        total = total.wrapping_add(g.rd16(ls, 0));
    }
    Ok(total)
}

fn liga_counts(g: &mut Graph, this: u32) -> R<Vec<u32>> {
    let mut out = Vec::new();
    for i in 0..g.rd16(this, 4) {
        let ls = liga_set(g, this, i, false)?;
        out.push(g.rd16(ls, 0));
    }
    Ok(out)
}

/// `ligature_index_to_object_id`.
fn ligature_index_to_object_id(g: &Graph, ls: u32) -> R<Vec<u32>> {
    let mut map = alloc::vec![INVALID; g.rd16(ls, 0) as usize];
    for l in &g.v(ls)?.real_links {
        if l.position < 2 {
            continue;
        }
        if let Some(m) = map.get_mut((l.position.wrapping_sub(2) / 2) as usize) {
            *m = l.objidx;
        }
    }
    Ok(map)
}

fn compute_split_points(g: &mut Graph, this: u32) -> R<Vec<u32>> {
    let base_size = LS_MIN;
    let mut accumulated = base_size;
    let mut ligature_index = 0u32;
    let mut split_points = Vec::new();
    for i in 0..g.rd16(this, 4) {
        accumulated = accumulated.wrapping_add(4);
        let ls = liga_set(g, this, i, false)?;
        let index_to_id = ligature_index_to_object_id(g, ls)?;
        for &liga_id in &index_to_id {
            let index = ligature_index;
            ligature_index = ligature_index.wrapping_add(1);
            if liga_id == INVALID {
                continue;
            }
            let liga_size = u32::try_from(g.table_size(liga_id)).unwrap_or(u32::MAX);
            accumulated = accumulated.wrapping_add(2).wrapping_add(liga_size);
            if accumulated >= 1 << 16 {
                split_points.push(index);
                accumulated = base_size
                    .wrapping_add(4)
                    .wrapping_add(2)
                    .wrapping_add(liga_size);
            }
        }
    }
    Ok(split_points)
}

fn new_liga_set(g: &mut Graph, count: u32) -> R<u32> {
    let prime = Context::create_node(g, 2usize.wrapping_add((count as usize).wrapping_mul(2)))?;
    g.wr16(prime, 0, count);
    Ok(prime)
}

fn clear_virtual_links(g: &mut Graph, node: u32) -> R<()> {
    let children: Vec<u32> = g.v(node)?.virtual_links.iter().map(|l| l.objidx).collect();
    for c in children {
        g.vm(c)?.remove_parent(node);
    }
    g.vm(node)?.virtual_links.clear();
    Ok(())
}

fn add_virtual_link(g: &mut Graph, from: u32, to: u32) -> R<()> {
    if from as usize >= g.len() || to as usize >= g.len() {
        return Err(GraphError::OutOfBounds);
    }
    g.vm(to)?.add_parent(from, true);
    g.vm(from)?.add_virtual_link(to);
    Ok(())
}

/// `current_liga_set_bounds`: the first and one past the last liga index
/// still linked.
fn current_liga_set_bounds(g: &Graph, ls: u32) -> (u32, u32) {
    let mut min = INVALID;
    let mut max = 0u32;
    if let Ok(v) = g.v(ls) {
        for l in v.real_links.iter().filter(|l| l.position >= 2) {
            let i = l.position.wrapping_sub(2) / 2;
            min = min.min(i);
            max = max.max(i);
        }
    }
    (min, max.wrapping_add(1))
}

/// `compact_liga_set`.
fn compact_liga_set(g: &mut Graph, ls: u32) -> R<()> {
    let len = g.rd16(ls, 0);
    let links = u32::try_from(g.v(ls)?.real_links.len()).unwrap_or(u32::MAX);
    if len <= links {
        return Ok(());
    }
    let to_remove = len.wrapping_sub(links);
    let v = g.vm(ls)?;
    v.sort_real_links();
    let mut pos = 2u32;
    for l in &mut v.real_links {
        l.position = pos;
        pos = pos.wrapping_add(2);
    }
    g.wr16(ls, 0, links);
    g.shrink_buffer(ls, (to_remove as usize).wrapping_mul(2));
    Ok(())
}

fn clone_range(g: &mut Graph, this: u32, liga_counts: &[u32], start: u32, end: u32) -> R<u32> {
    let set_count = g.rd16(this, 4);
    let prime_size = (LS_MIN as usize).wrapping_add((set_count as usize).wrapping_mul(2));
    let prime = Context::create_node(g, prime_size)?;
    g.wr16(prime, 0, g.rd16(this, 0));
    g.wr16(prime, 4, set_count);

    let coverage_id = g.index_for_offset(this, 2)?;
    let coverage_prime = g.duplicate(coverage_id, false)?;
    g.vm(prime)?.add_real_link(2, coverage_prime, 2);
    g.vm(coverage_prime)?.add_parent(prime, false);

    let mut count = 0u32;
    let mut liga_set_count = 0u32;
    let mut liga_set_start = INVALID;
    let mut liga_set_end = 0u32;
    for (i, &num_ligas) in liga_counts.iter().enumerate() {
        let i = u32::try_from(i).unwrap_or(INVALID);
        let mut current_start = count;
        let mut current_end = count.wrapping_add(num_ligas);
        if current_start >= end || start >= current_end {
            count = count.wrapping_add(num_ligas);
            continue;
        }
        let ls = liga_set(g, this, i, true)?;
        let bounds = current_liga_set_bounds(g, ls);
        current_start = count.wrapping_add(bounds.0).max(current_start);
        current_end = count.wrapping_add(bounds.1).min(current_end);

        let liga_set_prime;
        if current_start >= start && current_end <= end {
            liga_set_end = i;
            liga_set_start = liga_set_start.min(i);
            liga_set_prime = g.move_child(
                this,
                liga_set_position(i),
                prime,
                liga_set_position(liga_set_count),
                2,
            )?;
            liga_set_count = liga_set_count.wrapping_add(1);
            compact_liga_set(g, ls)?;
        } else {
            let start_index = start.max(current_start).wrapping_sub(count);
            let end_index = end.min(current_end).wrapping_sub(count);
            if end_index < start_index {
                return Err(GraphError::InvalidArgument);
            }
            liga_set_prime = new_liga_set(g, end_index.wrapping_sub(start_index))?;
            g.move_children(
                ls,
                2u32.wrapping_add(start_index.wrapping_mul(2)),
                2u32.wrapping_add(end_index.wrapping_mul(2)),
                liga_set_prime,
                2,
                2,
            )?;
            liga_set_end = i;
            liga_set_start = liga_set_start.min(i);
            g.add_link(liga_set_position(liga_set_count), prime, liga_set_prime)?;
            liga_set_count = liga_set_count.wrapping_add(1);
        }

        clear_virtual_links(g, liga_set_prime)?;
        add_virtual_link(g, liga_set_prime, coverage_prime)?;
        let children: Vec<u32> = g
            .v(liga_set_prime)?
            .real_links
            .iter()
            .map(|l| l.objidx)
            .collect();
        for c in children {
            clear_virtual_links(g, c)?;
            add_virtual_link(g, c, coverage_prime)?;
        }
        count = count.wrapping_add(num_ligas);
    }

    let len = g.rd16(prime, 4);
    g.shrink_buffer(
        prime,
        (len.wrapping_sub(liga_set_count) as usize).wrapping_mul(2),
    );
    g.wr16(prime, 4, liga_set_count);
    filter_coverage(
        g,
        coverage_prime,
        liga_set_start,
        liga_set_end.wrapping_add(1),
    )?;
    Ok(prime)
}

fn shrink(g: &mut Graph, this: u32, old_count: u32, liga_counts: &[u32], count: u32) -> R<()> {
    if count >= old_count {
        return Ok(());
    }
    let mut count = count;
    let mut retained = HbSet::new();
    let mut new_liga_set_count = 0u32;
    for (i, &num_ligas) in liga_counts.iter().enumerate() {
        let i = u32::try_from(i).unwrap_or(INVALID);
        let ls = liga_set(g, this, i, true)?;
        clear_virtual_links(g, ls)?;
        retained.add(ls);
        let index_to_id = ligature_index_to_object_id(g, ls)?;
        for &liga in &index_to_id {
            if liga != INVALID {
                clear_virtual_links(g, liga)?;
                retained.add(liga);
            }
        }
        if num_ligas >= count {
            let to_remove = num_ligas.wrapping_sub(count);
            new_liga_set_count = i.wrapping_add(1);
            g.shrink_buffer(ls, (to_remove as usize).wrapping_mul(2));
            g.wr16(ls, 0, count);
            break;
        }
        count = count.wrapping_sub(num_ligas);
    }

    let set_count = g.rd16(this, 4);
    g.shrink_buffer(
        this,
        (set_count.wrapping_sub(new_liga_set_count) as usize).wrapping_mul(2),
    );
    g.wr16(this, 4, new_liga_set_count);

    let mut coverage = g.index_for_offset(this, 2)?;
    let coverage_size = g.table_size(coverage);
    let glyphs = coverage_glyphs(g.bytes(coverage));
    if g.v(coverage)?.is_shared() {
        coverage = g.remap_child(this, coverage)?;
    }
    for r in retained.iter() {
        add_virtual_link(g, r, coverage)?;
    }
    let kept: Vec<u32> = glyphs
        .into_iter()
        .zip(0u32..)
        .filter(|&(_, i)| i < new_liga_set_count)
        .map(|(gl, _)| gl)
        .collect();
    make_coverage(g, &kept, coverage, coverage_size)
}
