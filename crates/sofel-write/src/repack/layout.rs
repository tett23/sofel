//! The Coverage and ClassDef tables as the repacker reads and rewrites
//! them: HarfBuzz 15.1.0's `src/graph/coverage-graph.hh`,
//! `src/graph/classdef-graph.hh`, and the parts of
//! `OT/Layout/Common/Coverage*.hh` and `hb-ot-layout-common.hh` (`ClassDef`)
//! they call (iteration, class lookup and serialization).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::graph::{Graph, GraphError, R};
use super::set::HbSet;

fn be(b: &[u8], at: usize, n: usize) -> u32 {
    b.get(at..at.saturating_add(n))
        .map_or(0, |s| s.iter().fold(0u32, |a, &x| (a << 8) | u32::from(x)))
}

fn put(out: &mut Vec<u8>, v: u32, n: usize) {
    let b = v.to_be_bytes();
    out.extend_from_slice(b.get(4usize.saturating_sub(n)..).unwrap_or(&[]));
}

/// `Coverage::sanitize` of the graph (`graph::Coverage::sanitize`).
pub(crate) fn coverage_sanitize(b: &[u8]) -> R<()> {
    let len = b.len();
    if len < 2 {
        return Err(GraphError::SanitizeFailure);
    }
    let (min, count_size, item) = match be(b, 0, 2) {
        1 => (4usize, 2usize, 2usize),
        2 => (4, 2, 6),
        3 => (5, 3, 3),
        4 => (5, 3, 8),
        _ => return Err(GraphError::SanitizeFailure),
    };
    if len < min {
        return Err(GraphError::SanitizeFailure);
    }
    let count = be(b, 2, count_size) as usize;
    if len < min.saturating_add(count.saturating_mul(item)) {
        return Err(GraphError::SanitizeFailure);
    }
    Ok(())
}

/// The glyphs of a Coverage table in coverage index order
/// (`Coverage::iter`).
pub(crate) fn coverage_glyphs(b: &[u8]) -> Vec<u32> {
    let (gsize, count_size) = match be(b, 0, 2) {
        1 | 2 => (2usize, 2usize),
        3 | 4 => (3, 3),
        _ => return Vec::new(),
    };
    let count = be(b, 2, count_size) as usize;
    let base = 2usize.saturating_add(count_size);
    match be(b, 0, 2) {
        1 | 3 => (0..count)
            .map(|i| be(b, base.saturating_add(i.saturating_mul(gsize)), gsize))
            .collect(),
        _ => {
            let rec = gsize.saturating_mul(2).saturating_add(2);
            let field = |i: usize, f: usize| {
                be(
                    b,
                    base.saturating_add(i.saturating_mul(rec))
                        .saturating_add(f.saturating_mul(gsize)),
                    if f == 2 { 2 } else { gsize },
                )
            };
            let mut out = Vec::new();
            // CoverageFormat2_4::iter_t.
            let mut coverage = 0u32;
            let mut i = 0usize;
            let mut j = if count > 0 { field(0, 0) } else { 0 };
            if count > 0 && field(0, 0) > field(0, 1) {
                i = count;
            }
            while i < count {
                out.push(j);
                if j >= field(i, 1) {
                    i = i.wrapping_add(1);
                    if i < count {
                        let old = coverage;
                        j = field(i, 0);
                        coverage = field(i, 2);
                        if coverage != old.wrapping_add(1) {
                            i = count;
                        }
                    }
                    continue;
                }
                coverage = coverage.wrapping_add(1);
                j = j.wrapping_add(1);
            }
            out
        }
    }
}

/// `Coverage::serialize`; `None` when it fails or is longer than
/// `max_size` (HarfBuzz serializes into a buffer of that size).
pub(crate) fn coverage_bytes(glyphs: &[u32], max_size: usize) -> Option<Vec<u8>> {
    let count = glyphs.len();
    let mut num_ranges = 0usize;
    let mut last = 0xFFFF_FFFEu32;
    let mut max = 0u32;
    let mut unsorted = false;
    for &g in glyphs {
        if last != 0xFFFF_FFFE && g < last {
            unsorted = true;
        }
        if last.wrapping_add(1) != g {
            num_ranges = num_ranges.wrapping_add(1);
        }
        last = g;
        max = max.max(g);
    }
    let mut format = if !unsorted && count <= num_ranges.saturating_mul(3) {
        1u32
    } else {
        2
    };
    if max > 0xFFFF {
        format = format.wrapping_add(2);
    }
    if max > 0xFF_FFFF {
        return None;
    }
    let (gsize, len_max) = if format >= 3 {
        (3usize, 0xFF_FFFFusize)
    } else {
        (2, 0xFFFF)
    };
    let mut out = Vec::new();
    put(&mut out, format, 2);
    if format == 1 || format == 3 {
        if count > len_max {
            return None;
        }
        put(&mut out, u32::try_from(count).ok()?, gsize);
        glyphs.iter().for_each(|&g| put(&mut out, g, gsize));
    } else {
        let mut ranges = 0usize;
        let mut last = 0xFFFF_FFFEu32;
        for &g in glyphs {
            if last.wrapping_add(1) != g {
                ranges = ranges.wrapping_add(1);
            }
            last = g;
        }
        if ranges > len_max {
            return None;
        }
        put(&mut out, u32::try_from(ranges).ok()?, gsize);
        // RangeRecord: first, last, value (u16).
        let mut recs: Vec<(u32, u32, u32)> = Vec::with_capacity(ranges);
        let mut last = 0xFFFF_FFFEu32;
        let mut unsorted = false;
        for (n, &g) in glyphs.iter().enumerate() {
            if last.wrapping_add(1) != g {
                if last != 0xFFFF_FFFE && last.wrapping_add(1) > g {
                    unsorted = true;
                }
                recs.push((g, g, u32::try_from(n).unwrap_or(u32::MAX)));
            }
            if let Some(r) = recs.last_mut() {
                r.1 = g;
            }
            last = g;
        }
        if unsorted {
            recs.sort_by_key(|r| r.0);
        }
        for (f, l, v) in recs {
            put(&mut out, f, gsize);
            put(&mut out, l, gsize);
            put(&mut out, v & 0xFFFF, 2);
        }
    }
    (out.len() <= max_size).then_some(out)
}

/// `ClassDef::sanitize` of the graph (`graph::ClassDef::sanitize`).
pub(crate) fn class_def_sanitize(b: &[u8]) -> R<()> {
    let len = b.len();
    if len < 2 {
        return Err(GraphError::SanitizeFailure);
    }
    let (min, count_at, count_size, item) = match be(b, 0, 2) {
        1 => (6usize, 4usize, 2usize, 2usize),
        2 => (4, 2, 2, 6),
        3 => (8, 5, 3, 3),
        4 => (5, 2, 3, 8),
        _ => return Err(GraphError::SanitizeFailure),
    };
    if len < min {
        return Err(GraphError::SanitizeFailure);
    }
    let count = be(b, count_at, count_size) as usize;
    if len < min.saturating_add(count.saturating_mul(item)) {
        return Err(GraphError::SanitizeFailure);
    }
    Ok(())
}

/// `ClassDef::get_class`.
pub(crate) fn class_of(b: &[u8], glyph: u32) -> u32 {
    match be(b, 0, 2) {
        1 | 3 => {
            let s = if be(b, 0, 2) == 1 { 2usize } else { 3 };
            let start = be(b, 2, s);
            let count = be(b, 2usize.saturating_add(s), s);
            let i = glyph.wrapping_sub(start);
            if i >= count {
                return 0;
            }
            let base = 2usize.saturating_add(s.saturating_mul(2));
            be(b, base.saturating_add((i as usize).saturating_mul(s)), s)
        }
        2 | 4 => {
            let s = if be(b, 0, 2) == 2 { 2usize } else { 3 };
            let count = be(b, 2, s);
            let rec = s.saturating_mul(2).saturating_add(2);
            let base = 2usize.saturating_add(s);
            let at = |i: u32, f: usize| {
                let o = base
                    .saturating_add((i as usize).saturating_mul(rec))
                    .saturating_add(f.saturating_mul(s));
                be(b, o, if f == 2 { 2 } else { s })
            };
            // hb_bsearch over [first, last].
            let (mut min, mut max) = (0i64, i64::from(count).wrapping_sub(1));
            while min <= max {
                let mid = min.wrapping_add(max).div_euclid(2);
                let mid32 = u32::try_from(mid).unwrap_or(u32::MAX);
                if glyph < at(mid32, 0) {
                    max = mid.wrapping_sub(1);
                } else if glyph > at(mid32, 1) {
                    min = mid.wrapping_add(1);
                } else {
                    return at(mid32, 2);
                }
            }
            0
        }
        _ => 0,
    }
}

/// `ClassDef::serialize` of `(glyph, class)` pairs (class 0 pairs are
/// dropped); `None` when it fails or is longer than `max_size`.
pub(crate) fn class_def_bytes(pairs: &[(u32, u32)], max_size: usize) -> Option<Vec<u8>> {
    let glyphs: Vec<(u32, u32)> = pairs.iter().copied().filter(|p| p.1 != 0).collect();
    let mut format = 2u32;
    if let Some(&(glyph_min, first_klass)) = glyphs.first() {
        let mut glyph_max = glyph_min;
        let mut class_max = 0u32;
        let mut num_ranges = 1u64;
        let mut prev_gid = glyph_min;
        let mut prev_klass = first_klass;
        for &(g, k) in &glyphs {
            class_max = class_max.max(k);
            if g == glyph_min {
                continue;
            }
            glyph_max = glyph_max.max(g);
            if g != prev_gid.wrapping_add(1) || k != prev_klass {
                num_ranges = num_ranges.wrapping_add(1);
            }
            prev_gid = g;
            prev_klass = k;
        }
        let mut best = u64::MAX;
        let span = u64::from(glyph_max.wrapping_sub(glyph_min).wrapping_add(1));
        let mut consider = |candidate: u32, size: u64| {
            if size < best {
                best = size;
                format = candidate;
            }
        };
        if glyph_max <= 0xFFFF && span <= 0xFFFF && class_max <= 0xFFFF {
            consider(1, 6u64.wrapping_add(2u64.wrapping_mul(span)));
        }
        if glyph_max <= 0xFFFF && num_ranges <= 0xFFFF && class_max <= 0xFFFF {
            consider(2, 4u64.wrapping_add(num_ranges.wrapping_mul(6)));
        }
        if glyph_max <= 0xFF_FFFF && span <= 0xFF_FFFF && class_max <= 0xFF_FFFF {
            consider(3, 8u64.wrapping_add(3u64.wrapping_mul(span)));
        }
        if glyph_max <= 0xFF_FFFF && num_ranges <= 0xFF_FFFF && class_max <= 0xFFFF {
            consider(4, 5u64.wrapping_add(num_ranges.wrapping_mul(8)));
        }
        if best == u64::MAX {
            return None;
        }
    }
    let mut out = Vec::new();
    put(&mut out, format, 2);
    let s = if format >= 3 { 3usize } else { 2 };
    match format {
        1 | 3 => {
            let Some(&(glyph_min, _)) = glyphs.first() else {
                put(&mut out, 0, s);
                put(&mut out, 0, s);
                return (out.len() <= max_size).then_some(out);
            };
            let glyph_max = glyphs.iter().fold(0u32, |a, p| a.max(p.0));
            let count = glyph_max.wrapping_sub(glyph_min).wrapping_add(1);
            if count as usize > if s == 2 { 0xFFFF } else { 0xFF_FFFF } {
                return None;
            }
            put(&mut out, glyph_min, s);
            put(&mut out, count, s);
            let mut values = alloc::vec![0u32; count as usize];
            for &(g, k) in &glyphs {
                if let Some(v) = values.get_mut(g.wrapping_sub(glyph_min) as usize) {
                    *v = k;
                }
            }
            values.into_iter().for_each(|v| put(&mut out, v, s));
        }
        _ => {
            let mut recs: Vec<(u32, u32, u32)> = Vec::new();
            let mut unsorted = false;
            if let Some(&(g0, k0)) = glyphs.first() {
                recs.push((g0, g0, k0));
                let (mut prev_gid, mut prev_klass) = (g0, k0);
                for &(g, k) in glyphs.iter().skip(1) {
                    if g != prev_gid.wrapping_add(1) || k != prev_klass {
                        if g < prev_gid {
                            unsorted = true;
                        }
                        if let Some(r) = recs.last_mut() {
                            r.1 = prev_gid;
                        }
                        recs.push((g, g, k));
                    }
                    prev_klass = k;
                    prev_gid = g;
                }
                if let Some(r) = recs.last_mut() {
                    r.1 = prev_gid;
                }
            }
            if recs.len() > if s == 2 { 0xFFFF } else { 0xFF_FFFF } {
                return None;
            }
            if unsorted {
                recs.sort_by_key(|r| r.0);
            }
            put(&mut out, u32::try_from(recs.len()).ok()?, s);
            for (f, l, k) in recs {
                put(&mut out, f, s);
                put(&mut out, l, s);
                put(&mut out, k, 2);
            }
        }
    }
    (out.len() <= max_size).then_some(out)
}

/// `Coverage::make_coverage`: replaces `dest`'s bytes with a Coverage of
/// `glyphs`.
pub(crate) fn make_coverage(g: &mut Graph, glyphs: &[u32], dest: u32, max_size: usize) -> R<()> {
    let bytes = coverage_bytes(glyphs, max_size).ok_or(GraphError::AllocationFailure)?;
    g.set_new_buffer(dest, bytes);
    Ok(())
}

/// `Coverage::add_coverage`: a new Coverage of `glyphs` linked from
/// `link_position` of `parent`.
pub(crate) fn add_coverage(
    g: &mut Graph,
    parent: u32,
    link_position: u32,
    glyphs: &[u32],
    max_size: usize,
) -> R<()> {
    let prime = g.new_node(None)?;
    make_coverage(g, glyphs, prime, max_size)?;
    g.vm(parent)?.add_real_link(2, prime, link_position);
    g.vm(prime)?.add_parent(parent, false);
    Ok(())
}

/// The glyphs of a Coverage vertex, after the null and sanitize checks.
pub(crate) fn coverage_of(g: &Graph, id: u32) -> R<Vec<u32>> {
    if g.is_null(id) {
        return Err(GraphError::InvalidArgument);
    }
    let b = g.bytes(id);
    coverage_sanitize(b)?;
    Ok(coverage_glyphs(b))
}

/// `Coverage::clone_coverage`: a Coverage of the glyphs at coverage
/// indices `[start, end)` of `coverage_id`, linked from `new_parent`.
pub(crate) fn clone_coverage(
    g: &mut Graph,
    coverage_id: u32,
    new_parent: u32,
    link_position: u32,
    start: u32,
    end: u32,
) -> R<()> {
    let size = g.table_size(coverage_id);
    let glyphs: Vec<u32> = coverage_of(g, coverage_id)?
        .into_iter()
        .zip(0u32..)
        .filter(|&(_, i)| i >= start && i < end)
        .map(|(gl, _)| gl)
        .collect();
    add_coverage(g, new_parent, link_position, &glyphs, size)
}

/// `Coverage::filter_coverage`: keeps the glyphs at coverage indices
/// `[start, end)`.
pub(crate) fn filter_coverage(g: &mut Graph, existing: u32, start: u32, end: u32) -> R<()> {
    let size = g.table_size(existing);
    let glyphs: Vec<u32> = coverage_of(g, existing)?
        .into_iter()
        .zip(0u32..)
        .filter(|&(_, i)| i >= start && i < end)
        .map(|(gl, _)| gl)
        .collect();
    make_coverage(
        g,
        &glyphs,
        existing,
        size.saturating_mul(2).saturating_add(100),
    )
}

/// `ClassDef::make_class_def`.
pub(crate) fn make_class_def(
    g: &mut Graph,
    pairs: &[(u32, u32)],
    dest: u32,
    max_size: usize,
) -> R<()> {
    let bytes = class_def_bytes(pairs, max_size).ok_or(GraphError::AllocationFailure)?;
    g.set_new_buffer(dest, bytes);
    Ok(())
}

/// `ClassDef::add_class_def`.
pub(crate) fn add_class_def(
    g: &mut Graph,
    parent: u32,
    link_position: u32,
    pairs: &[(u32, u32)],
    max_size: usize,
) -> R<()> {
    let prime = g.new_node(None)?;
    make_class_def(g, pairs, prime, max_size)?;
    g.vm(parent)?.add_real_link(2, prime, link_position);
    g.vm(prime)?.add_parent(parent, false);
    Ok(())
}

/// `class_def_size_estimator_t`: the sizes of the Coverage and ClassDef
/// tables of a growing set of classes.
#[derive(Clone, Debug)]
pub(crate) struct ClassDefSizeEstimator {
    num_ranges_per_class: BTreeMap<u32, u32>,
    glyphs_per_class: BTreeMap<u32, HbSet>,
    included_classes: HbSet,
    included_glyphs: HbSet,
    included_class_def_glyphs: HbSet,
    coverage_range_count: u32,
    class_def_range_count: u32,
    class_max: u32,
    coverage_size_val: u32,
}

impl ClassDefSizeEstimator {
    /// `create` from `(glyph, class)` pairs.
    pub(crate) fn new(pairs: impl IntoIterator<Item = (u32, u32)>) -> ClassDefSizeEstimator {
        let mut e = ClassDefSizeEstimator {
            num_ranges_per_class: BTreeMap::new(),
            glyphs_per_class: BTreeMap::new(),
            included_classes: HbSet::new(),
            included_glyphs: HbSet::new(),
            included_class_def_glyphs: HbSet::new(),
            coverage_range_count: 0,
            class_def_range_count: 0,
            class_max: 0,
            coverage_size_val: 0,
        };
        e.reset();
        for (gid, klass) in pairs {
            e.glyphs_per_class.entry(klass).or_default().add(gid);
        }
        let counts: Vec<(u32, u32)> = e
            .glyphs_per_class
            .iter()
            .filter(|(k, _)| **k != 0)
            .map(|(k, s)| (*k, u32::try_from(s.ranges().len()).unwrap_or(u32::MAX)))
            .collect();
        e.num_ranges_per_class.extend(counts);
        e
    }

    /// `reset`: no class included.
    pub(crate) fn reset(&mut self) {
        self.included_glyphs.clear();
        self.included_class_def_glyphs.clear();
        self.included_classes.clear();
        self.coverage_range_count = 0;
        self.class_def_range_count = 0;
        self.class_max = 0;
        self.coverage_size_val = self.compute_coverage_size();
    }

    /// `coverage_size`.
    pub(crate) fn coverage_size(&self) -> u32 {
        self.coverage_size_val
    }

    /// `add_class_def_size`: includes `klass` and returns the ClassDef
    /// size.
    pub(crate) fn add_class_def_size(&mut self, klass: u32) -> u32 {
        if !self.included_classes.has(klass) {
            let glyphs = self.glyphs_per_class.get(&klass).cloned();
            if let Some(glyphs) = &glyphs {
                for (start, end) in glyphs.ranges() {
                    self.coverage_range_count = self.coverage_range_count.wrapping_add(1);
                    if start != 0 && self.included_glyphs.has(start.wrapping_sub(1)) {
                        self.coverage_range_count = self.coverage_range_count.wrapping_sub(1);
                    }
                    if self.included_glyphs.has(end.wrapping_add(1)) {
                        self.coverage_range_count = self.coverage_range_count.wrapping_sub(1);
                    }
                }
                self.included_glyphs.union(glyphs);
                if klass != 0 {
                    self.included_class_def_glyphs.union(glyphs);
                }
                self.coverage_size_val = self.compute_coverage_size();
            }
            if klass != 0 {
                self.class_def_range_count = self
                    .class_def_range_count
                    .wrapping_add(self.num_ranges_per_class.get(&klass).copied().unwrap_or(0));
                if glyphs.is_some() && klass > self.class_max {
                    self.class_max = klass;
                }
            }
            self.included_classes.add(klass);
        }
        self.class_def_size()
    }

    fn glyph_id_size(glyphs: &HbSet) -> u32 {
        if glyphs.max().is_some_and(|m| m > 0xFFFF) {
            3
        } else {
            2
        }
    }

    fn compute_coverage_size(&self) -> u32 {
        let glyph_size = Self::glyph_id_size(&self.included_glyphs);
        let base = 2u32.wrapping_add(glyph_size);
        let range_size = glyph_size.wrapping_mul(3);
        let f1 = base.wrapping_add(glyph_size.wrapping_mul(self.included_glyphs.population()));
        let f2 = base.wrapping_add(range_size.wrapping_mul(self.coverage_range_count));
        f1.min(f2)
    }

    fn class_def_size(&self) -> u32 {
        let (Some(min), Some(max)) = (
            self.included_class_def_glyphs.min(),
            self.included_class_def_glyphs.max(),
        ) else {
            return 4;
        };
        let mut best = u32::MAX;
        let span = max.wrapping_sub(min).wrapping_add(1);
        let rc = self.class_def_range_count;
        let cm = self.class_max;
        if max <= 0xFFFF && span <= 0xFFFF && cm <= 0xFFFF {
            best = best.min(6u32.wrapping_add(span.wrapping_mul(2)));
        }
        if max <= 0xFFFF && rc <= 0xFFFF && cm <= 0xFFFF {
            best = best.min(4u32.wrapping_add(rc.wrapping_mul(6)));
        }
        if max <= 0xFF_FFFF && span <= 0xFF_FFFF && cm <= 0xFF_FFFF {
            best = best.min(8u32.wrapping_add(span.wrapping_mul(3)));
        }
        if max <= 0xFF_FFFF && rc <= 0xFF_FFFF && cm <= 0xFFFF {
            best = best.min(5u32.wrapping_add(rc.wrapping_mul(8)));
        }
        best
    }
}
