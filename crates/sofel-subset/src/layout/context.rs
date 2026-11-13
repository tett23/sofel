//! Contextual and chained contextual lookups shared by GSUB and GPOS
//! (`hb-ot-layout-gsubgpos.hh`: `Rule`, `RuleSet`, `ContextFormat1_4` to
//! `ContextFormat3_6`, `ChainRule`, `ChainRuleSet`,
//! `ChainContextFormat1_4` to `ChainContextFormat3`): their closure,
//! lookup closure, `intersects` and subsetting.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::closure::{Closure, LookupClosure};
use super::common::Sub;
use super::data::{Cls, Cov, D, n16, n32};
use crate::bits::BitSet;

/// How the values of a rule are matched (`ContextFormat`).
#[derive(Clone, Copy)]
pub(crate) enum Vals<'a> {
    /// Glyph ids (`SimpleContext`).
    Glyph,
    /// Classes of a ClassDef (`ClassBasedContext`).
    Class(Cls<'a>),
    /// Offsets to Coverage tables from the subtable
    /// (`CoverageBasedContext`).
    Cov(D<'a>),
}

fn intersects_value(glyphs: &BitSet, v: u32, vals: Vals<'_>) -> bool {
    match vals {
        Vals::Glyph => glyphs.has(v),
        Vals::Class(cd) => cd.intersects_class(glyphs, v),
        Vals::Cov(base) => Cov(base.follow(v)).intersects(glyphs),
    }
}

/// `array_is_subset_of`.
fn all_intersect(glyphs: &BitSet, values: &[u32], vals: Vals<'_>) -> bool {
    values.iter().all(|&v| intersects_value(glyphs, v, vals))
}

/// The cache of `intersected_class_glyphs` (`intersected_class_cache_t`):
/// by class, whatever glyph set it was computed for.
type ClassGlyphsCache = BTreeMap<u32, BitSet>;

fn intersected(
    glyphs: &BitSet,
    v: u32,
    vals: Vals<'_>,
    cache: &mut ClassGlyphsCache,
    out: &mut BitSet,
) {
    match vals {
        Vals::Glyph => out.add(v),
        Vals::Class(cd) => {
            if let Some(s) = cache.get(&v) {
                out.union(s);
                return;
            }
            let mut s = BitSet::new();
            cd.intersected_class_glyphs(glyphs, v, &mut s);
            out.union(&s);
            cache.insert(v, s);
        }
        Vals::Cov(base) => Cov(base.follow(v)).intersect_set(glyphs, out),
    }
}

/// A lookup record: `(sequenceIndex, lookupListIndex)`.
pub(crate) type Record = (u16, u16);

fn records(d: D<'_>, at: usize, n: usize) -> Vec<Record> {
    (0..n)
        .map(|i| {
            let p = at.saturating_add(i.saturating_mul(4));
            (d.u16(p), d.u16(p.saturating_add(2)))
        })
        .collect()
}

fn values(d: D<'_>, at: usize, n: usize, width: u8) -> Vec<u32> {
    (0..n)
        .map(|i| {
            d.uint(
                at.saturating_add(i.saturating_mul(usize::from(width))),
                width,
            )
        })
        .collect()
}

/// A `Rule`: the input count (with the first glyph), the input values
/// from the second, the lookup records.
pub(crate) struct Rule {
    pub input_count: u16,
    pub input: Vec<u32>,
    pub lookups: Vec<Record>,
}

/// Reads a `Rule`.
fn rule(d: D<'_>, width: u8) -> Rule {
    let input_count = d.u16(0);
    let n = usize::from(input_count.saturating_sub(1));
    let lookup_count = usize::from(d.u16(2));
    Rule {
        input_count,
        input: (0..n)
            .map(|i| {
                d.uint(
                    4usize.saturating_add(i.saturating_mul(usize::from(width))),
                    width,
                )
            })
            .collect(),
        lookups: records(
            d,
            4usize.saturating_add(n.saturating_mul(usize::from(width))),
            lookup_count,
        ),
    }
}

/// Header offsets widen to 32 bits in format 4; set offsets and glyph IDs widen to 24.
struct GlyphRuleSets<'a> {
    table: D<'a>,
    coverage: Cov<'a>,
    count: usize,
    width: u8,
}

impl<'a> GlyphRuleSets<'a> {
    fn new(table: D<'a>) -> Self {
        let wide = table.u16(0) == 4;
        let width = if wide { 3 } else { 2 };
        Self {
            table,
            coverage: Cov(if wide { table.off32(2) } else { table.off16(2) }),
            count: table.uint(if wide { 6 } else { 4 }, width) as usize,
            width,
        }
    }
    fn set(&self, index: usize) -> D<'a> {
        let start: usize = if self.width == 3 { 9 } else { 6 };
        self.table.follow(self.table.uint(
            start.saturating_add(index.saturating_mul(usize::from(self.width))),
            self.width,
        ))
    }
}

struct ClassRuleSets<'a> {
    table: D<'a>,
    coverage: D<'a>,
    classes: D<'a>,
    count: usize,
    start: usize,
    outer: u8,
    width: u8,
}
impl<'a> ClassRuleSets<'a> {
    fn new(table: D<'a>) -> Self {
        let (outer, width) = if table.u16(0) == 5 { (4, 3) } else { (2, 2) };
        let count_at = if outer == 4 { 10usize } else { 6 };
        Self {
            table,
            coverage: if outer == 4 {
                table.off32(2)
            } else {
                table.off16(2)
            },
            classes: if outer == 4 {
                table.off32(6)
            } else {
                table.off16(4)
            },
            count: table.uint(count_at, width) as usize,
            start: count_at.saturating_add(usize::from(width)),
            outer,
            width,
        }
    }
    fn set(&self, index: usize) -> D<'a> {
        self.table.follow(
            self.table.uint(
                self.start
                    .saturating_add(index.saturating_mul(usize::from(self.width))),
                self.width,
            ),
        )
    }
}

/// A `ChainRule`.
pub(crate) struct ChainRule {
    pub backtrack: Vec<u32>,
    pub input_count: u16,
    pub input: Vec<u32>,
    pub lookahead: Vec<u32>,
    pub lookups: Vec<Record>,
}

/// Reads a `ChainRule`.
fn chain_rule(d: D<'_>, width: u8) -> ChainRule {
    let nb = usize::from(d.u16(0));
    let mut at = 2usize.saturating_add(nb.saturating_mul(usize::from(width)));
    let backtrack = values(d, 2, nb, width);
    let input_count = d.u16(at);
    let ni = usize::from(input_count.saturating_sub(1));
    let input = values(d, at.saturating_add(2), ni, width);
    at = at
        .saturating_add(2)
        .saturating_add(ni.saturating_mul(usize::from(width)));
    let nl = usize::from(d.u16(at));
    let lookahead = values(d, at.saturating_add(2), nl, width);
    at = at
        .saturating_add(2)
        .saturating_add(nl.saturating_mul(usize::from(width)));
    let nr = usize::from(d.u16(at));
    ChainRule {
        backtrack,
        input_count,
        input,
        lookahead,
        lookups: records(d, at.saturating_add(2), nr),
    }
}

/// Follows each offset after the 16-bit count at `count_at`.
fn offsets(d: D<'_>, count_at: usize, width: u8) -> Vec<D<'_>> {
    values(
        d,
        count_at.saturating_add(2),
        usize::from(d.u16(count_at)),
        width,
    )
    .into_iter()
    .map(|off| d.follow(off))
    .collect()
}

/// Coverage offsets occupy three bytes in format 6 and two in format 3.
fn coverage_offset_width(st: D<'_>) -> u8 {
    if st.u16(0) == 6 { 3 } else { 2 }
}

fn coverage_offsets(st: D<'_>, start: usize, count: usize) -> Vec<u32> {
    let width = coverage_offset_width(st);
    (0..count)
        .map(|i| {
            st.uint(
                start.saturating_add(i.saturating_mul(usize::from(width))),
                width,
            )
        })
        .collect()
}

fn raw_offsets(st: D<'_>, count_at: usize) -> Vec<u32> {
    coverage_offsets(
        st,
        count_at.saturating_add(2),
        usize::from(st.u16(count_at)),
    )
}

/// A ChainContextFormat3: the backtrack, input and lookahead coverage
/// offsets and the lookup records.
pub(crate) struct Chain3 {
    pub backtrack: Vec<u32>,
    pub input: Vec<u32>,
    pub lookahead: Vec<u32>,
    pub lookups: Vec<Record>,
}

/// Reads a ChainContextFormat3.
pub(crate) fn chain3(st: D<'_>) -> Chain3 {
    let width = usize::from(coverage_offset_width(st));
    let backtrack = raw_offsets(st, 2);
    let mut at = 4usize.saturating_add(backtrack.len().saturating_mul(width));
    let input = raw_offsets(st, at);
    at = at
        .saturating_add(2)
        .saturating_add(input.len().saturating_mul(width));
    let lookahead = raw_offsets(st, at);
    at = at
        .saturating_add(2)
        .saturating_add(lookahead.len().saturating_mul(width));
    let nr = usize::from(st.u16(at));
    Chain3 {
        backtrack,
        input,
        lookahead,
        lookups: records(st, at.saturating_add(2), nr),
    }
}

/// `context_closure_recurse_lookups`.
#[allow(clippy::too_many_arguments)]
fn recurse_lookups(
    c: &mut Closure<'_>,
    input_count: u32,
    input: &[u32],
    lookups: &[Record],
    value: u32,
    vals: Vals<'_>,
    cache: &mut ClassGlyphsCache,
) {
    let mut covered = BitSet::new();
    let mut pos = BitSet::new();
    for &(seq, index) in lookups {
        let seq = u32::from(seq);
        if seq >= input_count {
            continue;
        }
        let mut has_pos = false;
        if !covered.has(seq) {
            has_pos = true;
            pos.clear();
            if seq == 0 {
                match vals {
                    Vals::Glyph => pos.add(value),
                    Vals::Class(_) => {
                        let parent = c.parent().clone();
                        intersected(&parent, value, vals, cache, &mut pos);
                    }
                    Vals::Cov(_) => pos = c.parent().clone(),
                }
            } else {
                let v = input
                    .get(seq.wrapping_sub(1) as usize)
                    .copied()
                    .unwrap_or(0);
                let glyphs = c.glyphs.clone();
                intersected(&glyphs, v, vals, cache, &mut pos);
            }
        }
        covered.add(seq);
        let active = if has_pos {
            core::mem::take(&mut pos)
        } else {
            c.glyphs.clone()
        };
        c.stack.push(active);
        let end = if matches!(vals, Vals::Cov(_)) {
            input_count.wrapping_add(1)
        } else {
            input_count
        };
        c.recurse(u32::from(index), &mut covered, seq, end);
        c.stack.pop();
    }
}

/// `context_closure_lookup`.
fn closure_rule(
    c: &mut Closure<'_>,
    input_count: u16,
    input: &[u32],
    lookups: &[Record],
    value: u32,
    vals: Vals<'_>,
    cache: &mut ClassGlyphsCache,
) {
    if all_intersect(&c.glyphs, input, vals) {
        recurse_lookups(
            c,
            u32::from(input_count),
            input,
            lookups,
            value,
            vals,
            cache,
        );
    }
}

fn rule_set_closure(
    c: &mut Closure<'_>,
    set: D<'_>,
    widths: (u8, u8),
    value: u32,
    vals: Vals<'_>,
    cache: &mut ClassGlyphsCache,
) {
    if c.limit_exceeded() {
        return;
    }
    for r in offsets(set, 0, widths.0) {
        if c.limit_exceeded() {
            continue;
        }
        let r = rule(r, widths.1);
        closure_rule(c, r.input_count, &r.input, &r.lookups, value, vals, cache);
    }
}

fn chain_rule_set_closure(
    c: &mut Closure<'_>,
    set: D<'_>,
    width: u8,
    value: u32,
    vals: [Vals<'_>; 3],
    cache: &mut ClassGlyphsCache,
) {
    if c.limit_exceeded() {
        return;
    }
    for r in offsets(set, 0, width) {
        if c.limit_exceeded() {
            continue;
        }
        let r = chain_rule(r, width);
        if chain_all_intersect(&c.glyphs, &r.backtrack, &r.input, &r.lookahead, vals) {
            recurse_lookups(
                c,
                u32::from(r.input_count),
                &r.input,
                &r.lookups,
                value,
                vals[1],
                cache,
            );
        }
    }
}

fn chain_all_intersect(
    glyphs: &BitSet,
    backtrack: &[u32],
    input: &[u32],
    lookahead: &[u32],
    vals: [Vals<'_>; 3],
) -> bool {
    let b = all_intersect(glyphs, backtrack, vals[0]);
    let i = all_intersect(glyphs, input, vals[1]);
    let l = all_intersect(glyphs, lookahead, vals[2]);
    b && i && l
}

/// The closure of a Context subtable.
pub(crate) fn context_closure(c: &mut Closure<'_>, st: D<'_>) {
    let mut cache = ClassGlyphsCache::new();
    match st.u16(0) {
        1 | 4 => {
            let sets = GlyphRuleSets::new(st);
            let cov = sets.coverage;
            let mut cur = BitSet::new();
            cov.intersect_set(c.parent(), &mut cur);
            c.stack.push(cur);
            let n = sets.count;
            for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                if !c.previous_parent().has(g) {
                    continue;
                }
                let set = sets.set(i);
                rule_set_closure(c, set, (2, sets.width), g, Vals::Glyph, &mut cache);
            }
            c.stack.pop();
        }
        2 | 5 => {
            let sets = ClassRuleSets::new(st);
            let cov = Cov(sets.coverage);
            if !cov.intersects(&c.glyphs) {
                return;
            }
            let mut cur = BitSet::new();
            cov.intersect_set(c.parent(), &mut cur);
            c.stack.push(cur);
            let cd = Cls(sets.classes);
            let n = sets.count;
            for i in 0..n {
                let k = n32(i);
                if !cd.intersects_class(c.parent(), k) {
                    continue;
                }
                let set = sets.set(i);
                rule_set_closure(c, set, (sets.width, 2), k, Vals::Class(cd), &mut cache);
            }
            c.stack.pop();
        }
        3 | 6 => {
            let count = usize::from(st.u16(2));
            let covs = coverage_offsets(st, 6, count);
            let first = Cov(st.follow(covs.first().copied().unwrap_or(0)));
            if !first.intersects(&c.glyphs) {
                return;
            }
            let mut cur = BitSet::new();
            first.intersect_set(c.parent(), &mut cur);
            c.stack.push(cur);
            let lookups = records(
                st,
                6usize.saturating_add(count.saturating_mul(usize::from(coverage_offset_width(st)))),
                usize::from(st.u16(4)),
            );
            let input = covs.get(1..).unwrap_or(&[]);
            closure_rule(c, n16(count), input, &lookups, 0, Vals::Cov(st), &mut cache);
            c.stack.pop();
        }
        _ => {}
    }
}

/// The closure of a ChainContext subtable.
pub(crate) fn chain_closure(c: &mut Closure<'_>, st: D<'_>) {
    let mut cache = ClassGlyphsCache::new();
    match st.u16(0) {
        1 | 4 => {
            let sets = GlyphRuleSets::new(st);
            let cov = sets.coverage;
            let mut cur = BitSet::new();
            cov.intersect_set(c.parent(), &mut cur);
            c.stack.push(cur);
            let n = sets.count;
            for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                if !c.previous_parent().has(g) {
                    continue;
                }
                let set = sets.set(i);
                chain_rule_set_closure(c, set, sets.width, g, [Vals::Glyph; 3], &mut cache);
            }
            c.stack.pop();
        }
        2 => {
            let cov = Cov(st.off16(2));
            if !cov.intersects(&c.glyphs) {
                return;
            }
            let mut cur = BitSet::new();
            cov.intersect_set(c.parent(), &mut cur);
            c.stack.push(cur);
            let vals = [
                Vals::Class(Cls(st.off16(4))),
                Vals::Class(Cls(st.off16(6))),
                Vals::Class(Cls(st.off16(8))),
            ];
            let input_cd = Cls(st.off16(6));
            let n = usize::from(st.u16(10));
            for i in 0..n {
                let k = n32(i);
                if !input_cd.intersects_class(c.parent(), k) {
                    continue;
                }
                let set = st.off16(12usize.saturating_add(i.saturating_mul(2)));
                chain_rule_set_closure(c, set, 2, k, vals, &mut cache);
            }
            c.stack.pop();
        }
        3 | 6 => {
            let ch = chain3(st);
            let first = Cov(st.follow(ch.input.first().copied().unwrap_or(0)));
            if !first.intersects(&c.glyphs) {
                return;
            }
            let mut cur = BitSet::new();
            first.intersect_set(c.parent(), &mut cur);
            c.stack.push(cur);
            let vals = [Vals::Cov(st); 3];
            let input = ch.input.get(1..).unwrap_or(&[]);
            if chain_all_intersect(&c.glyphs, &ch.backtrack, input, &ch.lookahead, vals) {
                recurse_lookups(
                    c,
                    n32(ch.input.len()),
                    input,
                    &ch.lookups,
                    0,
                    vals[1],
                    &mut cache,
                );
            }
            c.stack.pop();
        }
        _ => {}
    }
}

/// `intersects` of a Context subtable.
pub(crate) fn context_intersects(st: D<'_>, glyphs: &BitSet) -> bool {
    match st.u16(0) {
        1 | 4 => {
            let sets = GlyphRuleSets::new(st);
            let cov = sets.coverage;
            let n = sets.count;
            cov.glyphs()
                .into_iter()
                .take(n)
                .enumerate()
                .filter(|&(_, g)| glyphs.has(g))
                .any(|(i, _)| {
                    let set = sets.set(i);
                    offsets(set, 0, 2)
                        .into_iter()
                        .any(|r| all_intersect(glyphs, &rule(r, sets.width).input, Vals::Glyph))
                })
        }
        2 | 5 => {
            let sets = ClassRuleSets::new(st);
            let cov = Cov(sets.coverage);
            if !cov.intersects(glyphs) {
                return false;
            }
            let cd = Cls(sets.classes);
            let mut retained = BitSet::new();
            cov.intersect_set(glyphs, &mut retained);
            let mut classes = BitSet::new();
            cd.intersected_classes(&retained, &mut classes);
            let n = sets.count;
            (0..n).any(|i| {
                let k = n32(i);
                cd.intersects_class(glyphs, k)
                    && classes.has(k)
                    && offsets(sets.set(i), 0, sets.width)
                        .into_iter()
                        .any(|r| all_intersect(glyphs, &rule(r, 2).input, Vals::Class(cd)))
            })
        }
        3 | 6 => {
            let count = usize::from(st.u16(2));
            let covs = coverage_offsets(st, 6, count);
            if !Cov(st.follow(covs.first().copied().unwrap_or(0))).intersects(glyphs) {
                return false;
            }
            all_intersect(glyphs, covs.get(1..).unwrap_or(&[]), Vals::Cov(st))
        }
        _ => false,
    }
}

/// `intersects` of a ChainContext subtable.
pub(crate) fn chain_intersects(st: D<'_>, glyphs: &BitSet) -> bool {
    match st.u16(0) {
        1 | 4 => {
            let sets = GlyphRuleSets::new(st);
            let cov = sets.coverage;
            let n = sets.count;
            cov.glyphs()
                .into_iter()
                .take(n)
                .enumerate()
                .filter(|&(_, g)| glyphs.has(g))
                .any(|(i, _)| {
                    let set = sets.set(i);
                    offsets(set, 0, sets.width).into_iter().any(|r| {
                        let r = chain_rule(r, sets.width);
                        chain_all_intersect(
                            glyphs,
                            &r.backtrack,
                            &r.input,
                            &r.lookahead,
                            [Vals::Glyph; 3],
                        )
                    })
                })
        }
        2 => {
            let cov = Cov(st.off16(2));
            if !cov.intersects(glyphs) {
                return false;
            }
            let vals = [
                Vals::Class(Cls(st.off16(4))),
                Vals::Class(Cls(st.off16(6))),
                Vals::Class(Cls(st.off16(8))),
            ];
            let input_cd = Cls(st.off16(6));
            let mut retained = BitSet::new();
            cov.intersect_set(glyphs, &mut retained);
            let mut classes = BitSet::new();
            input_cd.intersected_classes(&retained, &mut classes);
            let n = usize::from(st.u16(10));
            (0..n).any(|i| {
                let k = n32(i);
                input_cd.intersects_class(glyphs, k)
                    && classes.has(k)
                    && offsets(st.off16(12usize.saturating_add(i.saturating_mul(2))), 0, 2)
                        .into_iter()
                        .any(|r| {
                            let r = chain_rule(r, 2);
                            chain_all_intersect(glyphs, &r.backtrack, &r.input, &r.lookahead, vals)
                        })
            })
        }
        3 | 6 => {
            let ch = chain3(st);
            if !Cov(st.follow(ch.input.first().copied().unwrap_or(0))).intersects(glyphs) {
                return false;
            }
            chain_all_intersect(
                glyphs,
                &ch.backtrack,
                ch.input.get(1..).unwrap_or(&[]),
                &ch.lookahead,
                [Vals::Cov(st); 3],
            )
        }
        _ => false,
    }
}

fn recurse_records(c: &mut LookupClosure<'_, '_>, lookups: &[Record]) {
    for &(_, index) in lookups {
        c.recurse(u32::from(index));
    }
}

/// The lookup closure of a Context subtable.
pub(crate) fn context_closure_lookups(c: &mut LookupClosure<'_, '_>, st: D<'_>) {
    let glyphs = c.glyphs;
    match st.u16(0) {
        1 | 4 => {
            let sets = GlyphRuleSets::new(st);
            let cov = sets.coverage;
            let n = sets.count;
            for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                if !glyphs.has(g) {
                    continue;
                }
                let set = sets.set(i);
                rule_set_closure_lookups(c, set, (2, sets.width), Vals::Glyph);
            }
        }
        2 | 5 => {
            let sets = ClassRuleSets::new(st);
            if !Cov(sets.coverage).intersects(glyphs) {
                return;
            }
            let cd = Cls(sets.classes);
            let n = sets.count;
            for i in 0..n {
                if !cd.intersects_class(glyphs, n32(i)) {
                    continue;
                }
                let set = sets.set(i);
                rule_set_closure_lookups(c, set, (sets.width, 2), Vals::Class(cd));
            }
        }
        3 | 6 => {
            if !context_intersects(st, glyphs) {
                return;
            }
            let count = usize::from(st.u16(2));
            let lookups = records(
                st,
                6usize.saturating_add(count.saturating_mul(usize::from(coverage_offset_width(st)))),
                usize::from(st.u16(4)),
            );
            recurse_records(c, &lookups);
        }
        _ => {}
    }
}

fn rule_set_closure_lookups(
    c: &mut LookupClosure<'_, '_>,
    set: D<'_>,
    widths: (u8, u8),
    vals: Vals<'_>,
) {
    if c.limit_exceeded() {
        return;
    }
    for r in offsets(set, 0, widths.0) {
        if c.limit_exceeded() {
            continue;
        }
        let r = rule(r, widths.1);
        if !all_intersect(c.glyphs, &r.input, vals) {
            continue;
        }
        recurse_records(c, &r.lookups);
    }
}

fn chain_rule_set_closure_lookups(
    c: &mut LookupClosure<'_, '_>,
    set: D<'_>,
    width: u8,
    vals: [Vals<'_>; 3],
) {
    if c.limit_exceeded() {
        return;
    }
    for r in offsets(set, 0, width) {
        if c.limit_exceeded() {
            continue;
        }
        let r = chain_rule(r, width);
        if !chain_all_intersect(c.glyphs, &r.backtrack, &r.input, &r.lookahead, vals) {
            continue;
        }
        recurse_records(c, &r.lookups);
    }
}

/// The lookup closure of a ChainContext subtable.
pub(crate) fn chain_closure_lookups(c: &mut LookupClosure<'_, '_>, st: D<'_>) {
    let glyphs = c.glyphs;
    match st.u16(0) {
        1 | 4 => {
            let sets = GlyphRuleSets::new(st);
            let cov = sets.coverage;
            let n = sets.count;
            for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                if !glyphs.has(g) {
                    continue;
                }
                let set = sets.set(i);
                chain_rule_set_closure_lookups(c, set, sets.width, [Vals::Glyph; 3]);
            }
        }
        2 => {
            if !Cov(st.off16(2)).intersects(glyphs) {
                return;
            }
            let vals = [
                Vals::Class(Cls(st.off16(4))),
                Vals::Class(Cls(st.off16(6))),
                Vals::Class(Cls(st.off16(8))),
            ];
            let input_cd = Cls(st.off16(6));
            let n = usize::from(st.u16(10));
            for i in 0..n {
                if !input_cd.intersects_class(glyphs, n32(i)) {
                    continue;
                }
                let set = st.off16(12usize.saturating_add(i.saturating_mul(2)));
                chain_rule_set_closure_lookups(c, set, 2, vals);
            }
        }
        3 | 6 => {
            if !chain_intersects(st, glyphs) {
                return;
            }
            recurse_records(c, &chain3(st).lookups);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------
// Subsetting.

/// `serialize_lookuprecord_array`: the records whose lookup is kept,
/// renumbered; returns their number.
fn write_records(s: &mut Sub<'_, '_>, lookups: &[Record]) -> u32 {
    let map = s.lookup_map();
    let mut n = 0u32;
    for &(seq, index) in lookups {
        let Some(&new) = map.get(&u32::from(index)) else {
            continue;
        };
        s.s.u16(seq);
        s.s.u16((new & 0xFFFF) as u16);
        n = n.wrapping_add(1);
    }
    n
}

/// `Rule::subset`: `mapping` is the glyph map (`None`) or a class map.
fn rule_subset(
    s: &mut Sub<'_, '_>,
    d: D<'_>,
    width: u8,
    klass_map: Option<&BTreeMap<u32, u32>>,
) -> bool {
    let r = rule(d, width);
    if r.input_count == 0 {
        return false;
    }
    let mapped: Option<Vec<u32>> = r
        .input
        .iter()
        .map(|&v| match klass_map {
            Some(m) => m.get(&v).copied(),
            None => s.p.glyphs.new_gid(v),
        })
        .collect();
    let Some(mapped) = mapped else {
        return false;
    };
    s.s.u16(r.input_count);
    let count_at = s.s.u16(0);
    mapped.iter().for_each(|&v| {
        if width == 3 {
            s.s.u24(v);
        } else {
            s.s.u16((v & 0xFFFF) as u16);
        }
    });
    let n = write_records(s, &r.lookups);
    s.s.check_assign_u16(count_at, u64::from(n), sofel_write::Errors::INT_OVERFLOW)
}

/// `RuleSetOf::subset`.
fn rule_set_subset(
    s: &mut Sub<'_, '_>,
    set: D<'_>,
    widths: (u8, u8),
    klass_map: Option<&BTreeMap<u32, u32>>,
) -> bool {
    let snap = s.s.snapshot();
    let count_at = s.s.u16(0);
    let mut n = 0u16;
    for i in 0..usize::from(set.u16(0)) {
        let off = set.uint(
            2usize.saturating_add(i.saturating_mul(usize::from(widths.0))),
            widths.0,
        );
        if off == 0 {
            continue;
        }
        let r = set.follow(off);
        let o_snap = s.s.snapshot();
        let at = s.s.zeros(usize::from(widths.0));
        if s.serialize_subset(at, widths.0, |s| rule_subset(s, r, widths.1, klass_map)) {
            n = n.wrapping_add(1);
        } else {
            s.s.revert(o_snap);
        }
    }
    s.s.set_u16(count_at, n);
    if n == 0 {
        s.s.revert(snap);
    }
    n != 0
}

/// `ChainRule::subset`: glyph maps (`None`) or the three class maps.
fn chain_rule_subset(
    s: &mut Sub<'_, '_>,
    d: D<'_>,
    width: u8,
    maps: Option<[&BTreeMap<u32, u32>; 3]>,
) -> bool {
    let r = chain_rule(d, width);
    let lists = [&r.backtrack, &r.input, &r.lookahead];
    let mapped: Option<Vec<Vec<u32>>> = match maps {
        None => {
            let gs = &s.p.layout.glyphset_gsub;
            if !lists.iter().all(|l| l.iter().all(|&g| gs.has(g))) {
                return false;
            }
            lists
                .iter()
                .map(|l| l.iter().map(|&g| s.p.glyphs.new_gid(g)).collect())
                .collect()
        }
        Some(m) => lists
            .iter()
            .zip(m.iter())
            .map(|(l, map)| l.iter().map(|v| map.get(v).copied()).collect())
            .collect(),
    };
    let Some(mapped) = mapped else {
        return false;
    };
    let counts = [
        n16(r.backtrack.len()),
        r.input_count,
        n16(r.lookahead.len()),
    ];
    for (count, values) in counts.iter().zip(mapped.iter()) {
        s.s.u16(*count);
        values.iter().for_each(|&v| {
            if width == 3 {
                s.s.u24(v);
            } else {
                s.s.u16((v & 0xFFFF) as u16);
            }
        });
    }
    let count_at = s.s.u16(n16(r.lookups.len()));
    let n = write_records(s, &r.lookups);
    s.s.check_assign_u16(count_at, u64::from(n), sofel_write::Errors::INT_OVERFLOW);
    true
}

/// `ChainRuleSetOf::subset`.
fn chain_rule_set_subset(
    s: &mut Sub<'_, '_>,
    set: D<'_>,
    width: u8,
    maps: Option<[&BTreeMap<u32, u32>; 3]>,
) -> bool {
    let snap = s.s.snapshot();
    let count_at = s.s.u16(0);
    let mut n = 0u16;
    for i in 0..usize::from(set.u16(0)) {
        let off = set.uint(
            2usize.saturating_add(i.saturating_mul(usize::from(width))),
            width,
        );
        if off == 0 {
            continue;
        }
        let r = set.follow(off);
        let o_snap = s.s.snapshot();
        let at = s.s.zeros(usize::from(width));
        if s.serialize_subset(at, width, |s| chain_rule_subset(s, r, width, maps)) {
            n = n.wrapping_add(1);
        } else {
            s.s.revert(o_snap);
        }
    }
    s.s.set_u16(count_at, n);
    if n == 0 {
        s.s.revert(snap);
    }
    n != 0
}

/// Subsets a Context subtable.
pub(crate) fn context_subset(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    match st.u16(0) {
        1 | 4 => {
            let sets = GlyphRuleSets::new(st);
            s.s.u16(st.u16(0));
            let outer: u8 = if sets.width == 3 { 4 } else { 2 };
            let cov_at = s.s.zeros(usize::from(outer));
            let count_at = s.s.zeros(usize::from(sets.width));
            let cov = sets.coverage;
            let n = sets.count;
            let mut new_cov = Vec::new();
            let mut count = 0u32;
            for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                if !s.p.layout.glyphset_gsub.has(g) {
                    continue;
                }
                let set = sets.set(i);
                if set.is_null() {
                    continue;
                }
                if s.offset_append(sets.width, |s| {
                    rule_set_subset(s, set, (2, sets.width), None)
                }) {
                    count = count.wrapping_add(1);
                    new_cov.push(s.p.glyphs.new_gid(g).unwrap_or(crate::set::INVALID));
                }
            }
            if sets.width == 3 {
                s.s.set_u24(count_at, count)
            } else {
                s.s.set_u16(count_at, n16(count as usize))
            }
            s.serialize_serialize(cov_at, outer, |s| s.coverage(&new_cov));
            !new_cov.is_empty()
        }
        2 | 5 => {
            let sets = ClassRuleSets::new(st);
            s.s.u16(st.u16(0));
            let cov_at = s.s.zeros(usize::from(sets.outer));
            let cd_at = s.s.zeros(usize::from(sets.outer));
            let count_at = s.s.zeros(usize::from(sets.width));
            let cov_d = sets.coverage;
            if !s.serialize_subset_opt(cov_at, sets.outer, cov_d, |s| s.coverage_subset(Cov(cov_d)))
            {
                return false;
            }
            let cd = sets.classes;
            let mut klass_map = BTreeMap::new();
            s.serialize_subset_opt(cd_at, sets.outer, cd, |s| {
                s.class_def_subset(Cls(cd), Some(&mut klass_map), true, true, None)
            });
            let mut retained = BitSet::new();
            Cov(cov_d).intersect_set(&s.p.layout.glyphset_gsub, &mut retained);
            let mut classes = BitSet::new();
            Cls(cd).intersected_classes(&retained, &mut classes);
            let mut non_zero: Option<usize> = None;
            let mut index = 0usize;
            let mut snapshot = s.s.snapshot();
            let n = sets.count;
            for i in 0..n {
                let k = n32(i);
                if !klass_map.contains_key(&k) {
                    continue;
                }
                let at = s.s.zeros(usize::from(sets.width));
                let set = sets.set(i);
                let ok = classes.has(k)
                    && s.serialize_subset_opt(at, sets.width, set, |s| {
                        rule_set_subset(s, set, (sets.width, 2), Some(&klass_map))
                    });
                if ok {
                    non_zero = Some(index);
                    snapshot = s.s.snapshot();
                }
                index = index.wrapping_add(1);
            }
            let Some(nz) = non_zero else {
                return false;
            };
            s.s.revert(snapshot);
            if sets.width == 3 {
                s.s.set_u24(count_at, n32(nz.saturating_add(1)));
            } else {
                s.s.set_u16(count_at, n16(nz.saturating_add(1)));
            }
            true
        }
        3 | 6 => {
            s.s.u16(st.u16(0));
            let width = coverage_offset_width(st);
            let count = usize::from(st.u16(2));
            s.s.u16(n16(count));
            let lc_at = s.s.u16(0);
            for off in coverage_offsets(st, 6, count) {
                let c = st.follow(off);
                let at = s.s.zeros(usize::from(width));
                if !s.serialize_subset_opt(at, width, c, |s| s.coverage_subset(Cov(c))) {
                    return false;
                }
            }
            let lookups = records(
                st,
                6usize.saturating_add(count.saturating_mul(usize::from(coverage_offset_width(st)))),
                usize::from(st.u16(4)),
            );
            let n = write_records(s, &lookups);
            s.s.check_assign_u16(lc_at, u64::from(n), sofel_write::Errors::INT_OVERFLOW)
        }
        _ => false,
    }
}

/// Subsets a ChainContext subtable.
pub(crate) fn chain_subset(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    match st.u16(0) {
        1 | 4 => {
            let sets = GlyphRuleSets::new(st);
            s.s.u16(st.u16(0));
            let outer: u8 = if sets.width == 3 { 4 } else { 2 };
            let cov_at = s.s.zeros(usize::from(outer));
            let count_at = s.s.zeros(usize::from(sets.width));
            let cov = sets.coverage;
            let n = sets.count;
            let mut new_cov = Vec::new();
            let mut count = 0u32;
            for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                if !s.p.layout.glyphset_gsub.has(g) {
                    continue;
                }
                let set = sets.set(i);
                if set.is_null() {
                    continue;
                }
                if s.offset_append(sets.width, |s| {
                    chain_rule_set_subset(s, set, sets.width, None)
                }) {
                    count = count.wrapping_add(1);
                    new_cov.push(s.p.glyphs.new_gid(g).unwrap_or(crate::set::INVALID));
                }
            }
            if sets.width == 3 {
                s.s.set_u24(count_at, count)
            } else {
                s.s.set_u16(count_at, n16(count as usize))
            }
            s.serialize_serialize(cov_at, outer, |s| s.coverage(&new_cov));
            !new_cov.is_empty()
        }
        2 => {
            s.s.u16(2);
            let cov_at = s.s.u16(0);
            let ats = [s.s.u16(0), s.s.u16(0), s.s.u16(0)];
            let count_at = s.s.u16(0);
            let cov_d = st.off16(2);
            s.serialize_subset_opt(cov_at, 2, cov_d, |s| s.coverage_subset(Cov(cov_d)));
            let mut maps: [BTreeMap<u32, u32>; 3] = Default::default();
            for (j, (at, map)) in ats.iter().zip(maps.iter_mut()).enumerate() {
                let cd = st.off16(4usize.saturating_add(j.saturating_mul(2)));
                s.serialize_subset_opt(*at, 2, cd, |s| {
                    s.class_def_subset(Cls(cd), Some(map), true, true, None)
                });
            }
            let input_cd = Cls(st.off16(6));
            let mut retained = BitSet::new();
            Cov(cov_d).intersect_set(&s.p.layout.glyphset_gsub, &mut retained);
            let mut classes = BitSet::new();
            input_cd.intersected_classes(&retained, &mut classes);
            let mut non_zero: Option<usize> = None;
            let mut index = 0usize;
            let mut last = s.s.snapshot();
            let n = usize::from(st.u16(10));
            let refs = [&maps[0], &maps[1], &maps[2]];
            for i in 0..n {
                let k = n32(i);
                if !maps[1].contains_key(&k) {
                    continue;
                }
                let at = s.s.u16(0);
                let set = st.off16(12usize.saturating_add(i.saturating_mul(2)));
                let ok = classes.has(k)
                    && s.serialize_subset_opt(at, 2, set, |s| {
                        chain_rule_set_subset(s, set, 2, Some(refs))
                    });
                if ok {
                    last = s.s.snapshot();
                    non_zero = Some(index);
                }
                index = index.wrapping_add(1);
            }
            let Some(nz) = non_zero else {
                return false;
            };
            if index > nz {
                s.s.revert(last);
            }
            s.s.set_u16(count_at, n16(nz.wrapping_add(1)));
            true
        }
        3 | 6 => {
            s.s.u16(st.u16(0));
            let width = coverage_offset_width(st);
            let ch = chain3(st);
            for list in [&ch.backtrack, &ch.input, &ch.lookahead] {
                s.s.u16(0);
                let count_at = s.s.len().wrapping_sub(2);
                let mut n = 0u16;
                for &off in list.iter() {
                    let c = st.follow(off);
                    let at = s.s.zeros(usize::from(width));
                    n = n.wrapping_add(1);
                    s.s.set_u16(count_at, n);
                    if !s.serialize_subset_opt(at, width, c, |s| s.coverage_subset(Cov(c))) {
                        return false;
                    }
                }
            }
            let lc_at = s.s.u16(n16(ch.lookups.len()));
            let n = write_records(s, &ch.lookups);
            s.s.check_assign_u16(lc_at, u64::from(n), sofel_write::Errors::INT_OVERFLOW)
        }
        _ => false,
    }
}
