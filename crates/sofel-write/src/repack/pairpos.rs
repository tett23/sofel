//! Splitting PairPos subtables (HarfBuzz 15.1.0's
//! `src/graph/pairpos-graph.hh`).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::graph::{Graph, GraphError, R};
use super::gsubgpos::{Context, actuate_split};
use super::layout::{
    ClassDefSizeEstimator, add_class_def, add_coverage, class_def_sanitize, class_of,
    clone_coverage, coverage_glyphs, coverage_of, coverage_sanitize, make_class_def, make_coverage,
};
use super::set::HbSet;

/// `PairPosFormat1_3<SmallTypes>::min_size`.
const F1_MIN: u32 = 10;
/// `PairPosFormat2_4<SmallTypes>::min_size`.
const F2_MIN: u32 = 16;

/// `ValueFormat::get_len`: the number of values (the low 8 bits).
fn value_len(f: u32) -> u32 {
    (f & 0xFF).count_ones()
}

/// `ValueFormat::get_device_table_indices`.
fn device_table_indices(f: u32) -> Vec<u32> {
    let mut i = (f & 0xF).count_ones();
    let mut out = Vec::new();
    for bit in [0x10u32, 0x20, 0x40, 0x80] {
        if f & bit != 0 {
            out.push(i);
            i = i.wrapping_add(1);
        }
    }
    out
}

/// `PairPos::split_subtables` (after `split_subtable`'s sanitize check).
pub(crate) fn split(c: &mut Context, g: &mut Graph, this: u32) -> R<Vec<u32>> {
    if g.is_null(this) || sanitize(g, this).is_err() {
        return Ok(Vec::new());
    }
    match g.rd16(this, 0) {
        1 => split_format1(c, g, this),
        2 => split_format2(c, g, this),
        _ => Ok(Vec::new()),
    }
}

/// `PairPos::sanitize`.
pub(crate) fn sanitize(g: &Graph, this: u32) -> R<()> {
    let len = g.table_size(this);
    if len < 2 {
        return Err(GraphError::SanitizeFailure);
    }
    match g.rd16(this, 0) {
        1 => {
            let count = g.rd16(this, 8) as usize;
            if len < F1_MIN as usize || len < (F1_MIN as usize).saturating_add(count * 2) {
                return Err(GraphError::SanitizeFailure);
            }
            Ok(())
        }
        2 => format2_sanitize(
            len,
            g.rd16(this, 12),
            g.rd16(this, 14),
            g.rd16(this, 4),
            g.rd16(this, 6),
        ),
        _ => Err(GraphError::SanitizeFailure),
    }
}

/// `PairPosFormat2::sanitize` of a table of `len` bytes.
pub(crate) fn format2_sanitize(len: usize, class1: u32, class2: u32, vf1: u32, vf2: u32) -> R<()> {
    if len < F2_MIN as usize {
        return Err(GraphError::SanitizeFailure);
    }
    let need = u64::from(class1).saturating_mul(class1_record_size(class2, vf1, vf2));
    if ((len.wrapping_sub(F2_MIN as usize)) as u64) < need {
        return Err(GraphError::SanitizeFailure);
    }
    Ok(())
}

fn class1_record_size(class2: u32, vf1: u32, vf2: u32) -> u64 {
    u64::from(class2).wrapping_mul(u64::from(value_len(vf1).wrapping_add(value_len(vf2))) * 2)
}

fn split_format1(c: &mut Context, g: &mut Graph, this: u32) -> R<Vec<u32>> {
    let mut visited = HbSet::new();
    let coverage_id = g.index_for_offset(this, 2)?;
    let coverage_size = u32::try_from(g.table_size(coverage_id)).unwrap_or(u32::MAX);
    let mut partial_coverage_size = 4u32;
    let mut accumulated = F1_MIN;
    let mut split_points = Vec::new();
    let pair_sets = g.v(this)?.position_to_index_map();
    for i in 0..g.rd16(this, 8) {
        let pos = F1_MIN.wrapping_add(i.wrapping_mul(2));
        let Some(&idx) = pair_sets.get(&pos) else {
            return Err(GraphError::InvalidArgument);
        };
        let delta = u32::try_from(
            g.find_subgraph_size(idx, &mut visited, u32::MAX)
                .unwrap_or(0),
        )
        .unwrap_or(u32::MAX)
        .wrapping_add(2);
        partial_coverage_size = partial_coverage_size.wrapping_add(2);
        accumulated = accumulated.wrapping_add(delta);
        let total = accumulated.wrapping_add(partial_coverage_size.min(coverage_size));
        if total >= 1 << 16 {
            split_points.push(i);
            accumulated = F1_MIN.wrapping_add(delta);
            partial_coverage_size = 6;
            visited.clear();
        }
    }
    let count = g.rd16(this, 8);
    let _ = c;
    actuate_split(
        g,
        &split_points,
        count,
        |g, start, end| format1_clone_range(g, this, start, end),
        |g, n| format1_shrink(g, this, n),
    )
}

fn format1_shrink(g: &mut Graph, this: u32, count: u32) -> R<()> {
    let old = g.rd16(this, 8);
    if count >= old {
        return Ok(());
    }
    g.wr16(this, 8, count);
    g.shrink_buffer(this, (old.wrapping_sub(count) as usize).wrapping_mul(2));
    let cov = g.mutable_index_for_offset(this, 2)?;
    let glyphs = coverage_of(g, cov)?;
    let size = g.table_size(cov);
    let kept: Vec<u32> = glyphs
        .into_iter()
        .zip(0u32..)
        .filter(|&(_, i)| i < count)
        .map(|(gl, _)| gl)
        .collect();
    make_coverage(g, &kept, cov, size)
}

fn format1_clone_range(g: &mut Graph, this: u32, start: u32, end: u32) -> R<u32> {
    let num = end.wrapping_sub(start);
    let prime_size = (F1_MIN as usize).wrapping_add((num as usize).wrapping_mul(2));
    let prime = Context::create_node(g, prime_size)?;
    g.wr16(prime, 0, g.rd16(this, 0));
    g.wr16(prime, 4, g.rd16(this, 4));
    g.wr16(prime, 6, g.rd16(this, 6));
    g.wr16(prime, 8, num);
    g.vm(this)?.sort_real_links();
    g.move_children(
        this,
        F1_MIN.wrapping_add(start.wrapping_mul(2)),
        F1_MIN.wrapping_add(end.wrapping_mul(2)),
        prime,
        F1_MIN,
        2,
    )?;
    let coverage_id = g.index_for_offset(this, 2)?;
    clone_coverage(g, coverage_id, prime, 2, start, end)?;
    Ok(prime)
}

struct Format2 {
    this: u32,
    class1_record_size: u32,
    max_coverage_size: u32,
    max_class_def_size: u32,
    format1_device: Vec<u32>,
    format2_device: Vec<u32>,
}

fn coverage_table(g: &Graph, this: u32) -> R<Vec<u32>> {
    let id = g.index_for_offset(this, 2)?;
    coverage_of(g, id)
}

fn class_def_table(g: &Graph, id: u32) -> R<Vec<u8>> {
    if g.is_null(id) {
        return Err(GraphError::InvalidArgument);
    }
    let b = g.bytes(id);
    class_def_sanitize(b)?;
    Ok(b.to_vec())
}

fn split_format2(c: &mut Context, g: &mut Graph, this: u32) -> R<Vec<u32>> {
    let class_def_2_size =
        u32::try_from(g.table_size(g.index_for_offset(this, 10)?)).unwrap_or(u32::MAX);
    let coverage = coverage_table(g, this)?;
    let class_def_1 = class_def_table(g, g.index_for_offset(this, 8)?)?;
    let mut estimator = ClassDefSizeEstimator::new(
        coverage
            .iter()
            .map(|&gid| (gid, class_of(&class_def_1, gid))),
    );

    let class1_count = g.rd16(this, 12);
    let class2_count = g.rd16(this, 14);
    let vf1 = g.rd16(this, 4);
    let vf2 = g.rd16(this, 6);
    let class1_record_size =
        u32::try_from(class1_record_size(class2_count, vf1, vf2)).unwrap_or(u32::MAX);
    let value_1_len = value_len(vf1);
    let value_2_len = value_len(vf2);
    let total_value_len = value_1_len.wrapping_add(value_2_len);

    let mut accumulated = F2_MIN;
    let mut coverage_size;
    let mut class_def_1_size;
    let mut max_coverage_size = 4u32;
    let mut max_class_def_1_size = 4u32;
    let mut split_points = Vec::new();

    let device_tables = g.v(this)?.position_to_index_map();
    let format1_device = device_table_indices(vf1);
    let format2_device = device_table_indices(vf2);
    let has_device_tables = !format1_device.is_empty() || !format2_device.is_empty();

    let mut visited = HbSet::new();
    for i in 0..class1_count {
        let mut delta = class1_record_size;
        class_def_1_size = estimator.add_class_def_size(i);
        coverage_size = estimator.coverage_size();
        max_coverage_size = max_coverage_size.max(coverage_size);
        max_class_def_1_size = max_class_def_1_size.max(class_def_1_size);
        if has_device_tables {
            for j in 0..class2_count {
                let value1_index =
                    total_value_len.wrapping_mul(class2_count.wrapping_mul(i).wrapping_add(j));
                let value2_index = value1_index.wrapping_add(value_1_len);
                delta = delta.wrapping_add(value_record_children_size(
                    g,
                    &device_tables,
                    &format1_device,
                    value1_index,
                    &mut visited,
                ));
                delta = delta.wrapping_add(value_record_children_size(
                    g,
                    &device_tables,
                    &format2_device,
                    value2_index,
                    &mut visited,
                ));
            }
        }
        accumulated = accumulated.wrapping_add(delta);
        let largest = coverage_size.max(class_def_1_size).max(class_def_2_size);
        let total = accumulated
            .wrapping_add(coverage_size)
            .wrapping_add(class_def_1_size)
            .wrapping_add(class_def_2_size)
            .wrapping_sub(largest);
        if total >= 1 << 16 {
            split_points.push(i);
            accumulated = F2_MIN.wrapping_add(delta);
            estimator.reset();
            estimator.add_class_def_size(i);
            visited.clear();
        }
    }

    let sc = Format2 {
        this,
        class1_record_size,
        max_coverage_size,
        max_class_def_size: max_class_def_1_size,
        format1_device,
        format2_device,
    };
    let _ = c;
    actuate_split(
        g,
        &split_points,
        class1_count,
        |g, start, end| format2_clone_range(g, &sc, start, end),
        |g, n| format2_shrink(g, &sc, n),
    )
}

/// `size_of_value_record_children`.
fn value_record_children_size(
    g: &mut Graph,
    device_tables: &BTreeMap<u32, u32>,
    indices: &[u32],
    value_record_index: u32,
    visited: &mut HbSet,
) -> u32 {
    let mut size = 0u32;
    for &i in indices {
        let position = F2_MIN.wrapping_add(value_record_index.wrapping_add(i).wrapping_mul(2));
        let Some(&obj) = device_tables.get(&position) else {
            continue;
        };
        let s = g.find_subgraph_size(obj, visited, u32::MAX).unwrap_or(0);
        size = size.wrapping_add(u32::try_from(s).unwrap_or(u32::MAX));
    }
    size
}

fn format2_clone_range(g: &mut Graph, sc: &Format2, start: u32, end: u32) -> R<u32> {
    let this = sc.this;
    let num = end.wrapping_sub(start);
    let prime_size =
        (F2_MIN as usize).wrapping_add((num as usize).wrapping_mul(sc.class1_record_size as usize));
    let prime = Context::create_node(g, prime_size)?;
    for off in [0usize, 4, 6] {
        g.wr16(prime, off, g.rd16(this, off));
    }
    g.wr16(prime, 12, num);
    g.wr16(prime, 14, g.rd16(this, 14));
    clone_class1_records(g, sc, prime, start, end)?;

    let coverage_id = g.index_for_offset(this, 2)?;
    let class_def_1_id = g.index_for_offset(this, 8)?;
    if g.is_null(coverage_id) || g.is_null(class_def_1_id) {
        return Err(GraphError::InvalidArgument);
    }
    coverage_sanitize(g.bytes(coverage_id))?;
    class_def_sanitize(g.bytes(class_def_1_id))?;
    let class_def_1 = g.bytes(class_def_1_id).to_vec();
    let klass_map: Vec<(u32, u32)> = coverage_glyphs(g.bytes(coverage_id))
        .into_iter()
        .map(|gid| (gid, class_of(&class_def_1, gid)))
        .filter(|&(_, k)| k >= start && k < end)
        .map(|(gid, k)| (gid, k.wrapping_sub(start)))
        .collect();
    let glyphs: Vec<u32> = klass_map.iter().map(|p| p.0).collect();
    add_coverage(g, prime, 2, &glyphs, sc.max_coverage_size as usize)?;
    add_class_def(g, prime, 8, &klass_map, sc.max_class_def_size as usize)?;
    let class_def_2_id = g.index_for_offset(this, 10)?;
    g.vm(prime)?.add_real_link(2, class_def_2_id, 10);
    g.vm(class_def_2_id)?.add_parent(prime, false);
    Ok(prime)
}

fn clone_class1_records(g: &mut Graph, sc: &Format2, prime: u32, start: u32, end: u32) -> R<()> {
    let rs = sc.class1_record_size as usize;
    let from = (F2_MIN as usize).wrapping_add((start as usize).wrapping_mul(rs));
    let n = (end.wrapping_sub(start) as usize).wrapping_mul(rs);
    let src: Vec<u8> = (0..n)
        .map(|k| g.rd(sc.this, from.wrapping_add(k), 1).to_be_bytes()[3])
        .collect();
    for (k, b) in src.into_iter().enumerate() {
        g.wr(prime, (F2_MIN as usize).wrapping_add(k), 1, u32::from(b));
    }
    if sc.format1_device.is_empty() && sc.format2_device.is_empty() {
        return Ok(());
    }
    g.vm(sc.this)?.sort_real_links();
    let rs32 = sc.class1_record_size;
    g.move_children(
        sc.this,
        F2_MIN.wrapping_add(start.wrapping_mul(rs32)),
        F2_MIN.wrapping_add(end.wrapping_mul(rs32)),
        prime,
        F2_MIN,
        2,
    )
}

fn format2_shrink(g: &mut Graph, sc: &Format2, count: u32) -> R<()> {
    let this = sc.this;
    let old = g.rd16(this, 12);
    if count >= old {
        return Ok(());
    }
    g.wr16(this, 12, count);
    g.shrink_buffer(
        this,
        (old.wrapping_sub(count) as usize).wrapping_mul(sc.class1_record_size as usize),
    );
    let cov = g.mutable_index_for_offset(this, 2)?;
    let cov_glyphs = coverage_of(g, cov)?;
    let cd1 = g.mutable_index_for_offset(this, 8)?;
    let cd1_bytes = class_def_table(g, cd1)?;
    let klass_map: Vec<(u32, u32)> = cov_glyphs
        .into_iter()
        .map(|gid| (gid, class_of(&cd1_bytes, gid)))
        .filter(|&(_, k)| k < count)
        .collect();
    let glyphs: Vec<u32> = klass_map.iter().map(|p| p.0).collect();
    make_coverage(
        g,
        &glyphs,
        cov,
        4usize.wrapping_add(glyphs.len().wrapping_mul(2)),
    )?;
    let size = g.table_size(cd1);
    make_class_def(g, &klass_map, cd1, size)
}
