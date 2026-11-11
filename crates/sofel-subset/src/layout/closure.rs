//! The GSUB glyph closure (`hb_closure_context_t`,
//! `hb_ot_layout_lookups_substitute_closure`), the lookup closure of GSUB
//! and GPOS (`hb_closure_lookups_context_t`, `GSUBGPOS::closure_lookups`)
//! and the subtable `intersects` tests.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::context;
use super::data::{Cov, D};
use super::limits::{CLOSURE_MAX_STAGES, MAX_LOOKUP_VISIT_COUNT, MAX_NESTING_LEVEL};
use super::{Gsubgpos, Kind, subtables};
use crate::bits::BitSet;
use crate::set::INVALID;

/// The glyph closure state (`hb_closure_context_t`).
pub(crate) struct Closure<'a> {
    pub table: Gsubgpos<'a>,
    pub num_glyphs: u32,
    pub glyphs: BitSet,
    pub output: BitSet,
    pub stack: Vec<BitSet>,
    nesting_left: u32,
    done_count: BTreeMap<u32, u64>,
    done_set: BTreeMap<u32, BitSet>,
    lookup_count: u32,
}

impl<'a> Closure<'a> {
    fn new(table: Gsubgpos<'a>, num_glyphs: u32, glyphs: BitSet) -> Closure<'a> {
        Closure {
            table,
            num_glyphs,
            glyphs,
            output: BitSet::new(),
            stack: Vec::new(),
            nesting_left: MAX_NESTING_LEVEL,
            done_count: BTreeMap::new(),
            done_set: BTreeMap::new(),
            lookup_count: 0,
        }
    }

    /// `parent_active_glyphs`.
    pub fn parent(&self) -> &BitSet {
        self.stack.last().unwrap_or(&self.glyphs)
    }

    /// `previous_parent_active_glyphs`.
    pub fn previous_parent(&self) -> &BitSet {
        match self.stack.len() {
            0 | 1 => &self.glyphs,
            n => self.stack.get(n.wrapping_sub(2)).unwrap_or(&self.glyphs),
        }
    }

    /// `lookup_limit_exceeded`.
    pub fn limit_exceeded(&self) -> bool {
        self.lookup_count > MAX_LOOKUP_VISIT_COUNT
    }

    fn should_visit(&mut self, index: u32) -> bool {
        let over = self.lookup_count > MAX_LOOKUP_VISIT_COUNT;
        self.lookup_count = self.lookup_count.saturating_add(1);
        if over {
            return false;
        }
        !self.is_done(index)
    }

    fn is_done(&mut self, index: u32) -> bool {
        let mut pop = self.glyphs.len();
        if pop == u64::from(INVALID) {
            pop = pop.wrapping_sub(1);
        }
        if self.done_count.get(&index) != Some(&pop) {
            self.done_count.insert(index, pop);
            self.done_set.entry(index).or_default().clear();
        }
        let parent = self.stack.last().unwrap_or(&self.glyphs);
        let covered = self.done_set.entry(index).or_default();
        if parent.is_subset(covered) {
            return true;
        }
        covered.union(parent);
        false
    }

    fn flush(&mut self) {
        self.output.truncate(self.num_glyphs);
        let out = core::mem::take(&mut self.output);
        self.glyphs.union(&out);
        self.stack.clear();
    }

    /// `recurse`: the closure of lookup `index` from a contextual rule.
    pub fn recurse(&mut self, index: u32, covered: &mut BitSet, seq: u32, end: u32) {
        if self.nesting_left == 0 {
            return;
        }
        self.nesting_left = self.nesting_left.wrapping_sub(1);
        if self.should_visit(index) {
            let lookup = self.table.lookup(index as usize);
            if may_have_non_1to1(lookup, true) {
                covered.add_range(seq, end);
            }
            self.dispatch(lookup);
        }
        self.nesting_left = self.nesting_left.wrapping_add(1);
    }

    fn lookup_closure(&mut self, index: u32) {
        if !self.should_visit(index) {
            return;
        }
        let lookup = self.table.lookup(index as usize);
        self.dispatch(lookup);
        self.flush();
    }

    fn dispatch(&mut self, lookup: D<'a>) {
        for (k, st) in subtables(lookup, true) {
            self.subtable(k, st);
        }
    }

    fn subtable(&mut self, k: Kind, st: D<'a>) {
        match k {
            Kind::SingleSubst => self.single(st),
            Kind::MultipleSubst | Kind::AlternateSubst => {
                if st.u16(0) != 1 {
                    return;
                }
                let cov = Cov(st.off16(2));
                let n = usize::from(st.u16(4));
                let mut add = Vec::new();
                for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                    if !self.parent().has(g) {
                        continue;
                    }
                    let seq = st.off16(6usize.saturating_add(i.saturating_mul(2)));
                    let m = usize::from(seq.u16(0));
                    (0..m).for_each(|j| add.push(u32::from(seq.u16_at(2, j))));
                }
                add.into_iter().for_each(|g| self.output.add(g));
            }
            Kind::LigatureSubst => {
                if st.u16(0) != 1 {
                    return;
                }
                let cov = Cov(st.off16(2));
                let n = usize::from(st.u16(4));
                let mut add = Vec::new();
                for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                    if !self.parent().has(g) {
                        continue;
                    }
                    let set = st.off16(6usize.saturating_add(i.saturating_mul(2)));
                    for j in 0..usize::from(set.u16(0)) {
                        let lig = set.off16(2usize.saturating_add(j.saturating_mul(2)));
                        if ligature_intersects(lig, &self.glyphs) {
                            add.push(u32::from(lig.u16(0)));
                        }
                    }
                }
                add.into_iter().for_each(|g| self.output.add(g));
            }
            Kind::Context => context::context_closure(self, st),
            Kind::ChainContext => context::chain_closure(self, st),
            Kind::ReverseChain => {
                if st.u16(0) != 1 || !reverse_intersects(st, &self.glyphs) {
                    return;
                }
                let cov = Cov(st.off16(2));
                let (_, _, subst) = reverse_parts(st);
                let n = usize::from(subst.u16(0));
                let add: Vec<u32> = cov
                    .glyphs()
                    .into_iter()
                    .take(n)
                    .enumerate()
                    .filter(|&(_, g)| self.parent().has(g))
                    .map(|(i, _)| u32::from(subst.u16_at(2, i)))
                    .collect();
                add.into_iter().for_each(|g| self.output.add(g));
            }
            _ => {}
        }
    }

    fn single(&mut self, st: D<'a>) {
        let cov = Cov(st.off16(2));
        match st.u16(0) {
            1 => {
                let d = u32::from(st.u16(4));
                let mask = 0xFFFFu32;
                if cov.population() >= u64::from(mask) {
                    return;
                }
                let mut inter = BitSet::new();
                cov.intersect_set(self.parent(), &mut inter);
                let min_before = inter.min();
                let max_before = inter.max();
                let min_after = min_before.wrapping_add(d) & mask;
                let max_after = max_before.wrapping_add(d) & mask;
                if inter.len() == u64::from(max_before.wrapping_sub(min_before).wrapping_add(1))
                    && ((min_before <= min_after && min_after <= max_before)
                        || (min_before <= max_after && max_after <= max_before))
                {
                    return;
                }
                let add: Vec<u32> = inter.iter().map(|g| g.wrapping_add(d) & mask).collect();
                add.into_iter().for_each(|g| self.output.add(g));
            }
            2 => {
                let n = usize::from(st.u16(4));
                let parent = self.parent();
                let mut add = Vec::new();
                if n as u64 > parent.len().saturating_mul(4) {
                    for g in parent.iter() {
                        let Some(i) = cov.get(g) else { continue };
                        let i = i as usize;
                        if i >= n {
                            continue;
                        }
                        add.push(u32::from(st.u16_at(6, i)));
                    }
                } else {
                    for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
                        if parent.has(g) {
                            add.push(u32::from(st.u16_at(6, i)));
                        }
                    }
                }
                add.into_iter().for_each(|g| self.output.add(g));
            }
            _ => {}
        }
    }
}

/// `Ligature::intersects`: every component is in `glyphs`.
pub(crate) fn ligature_intersects(lig: D<'_>, glyphs: &BitSet) -> bool {
    let n = usize::from(lig.u16(2)).saturating_sub(1);
    (0..n).all(|i| glyphs.has(u32::from(lig.u16_at(4, i))))
}

/// The backtrack array, the lookahead array and the substitute array of
/// a ReverseChainSingleSubst format 1.
pub(crate) fn reverse_parts(st: D<'_>) -> (D<'_>, D<'_>, D<'_>) {
    let backtrack = st.at(4);
    let nb = usize::from(backtrack.u16(0));
    let lookahead = backtrack.at(2usize.saturating_add(nb.saturating_mul(2)));
    let nl = usize::from(lookahead.u16(0));
    let subst = lookahead.at(2usize.saturating_add(nl.saturating_mul(2)));
    (backtrack, lookahead, subst)
}

fn reverse_intersects(st: D<'_>, glyphs: &BitSet) -> bool {
    if !Cov(st.off16(2)).intersects(glyphs) {
        return false;
    }
    let (b, l, _) = reverse_parts(st);
    let all = |arr: D<'_>| {
        (0..usize::from(arr.u16(0)))
            .all(|i| Cov(st.follow(u32::from(arr.u16_at(2, i)))).intersects(glyphs))
    };
    all(b) && all(l)
}

/// `may_have_non_1to1` of a GSUB lookup.
pub(crate) fn may_have_non_1to1(lookup: D<'_>, gsub: bool) -> bool {
    subtables(lookup, gsub).iter().any(|(k, _)| {
        matches!(
            k,
            Kind::MultipleSubst | Kind::LigatureSubst | Kind::Context | Kind::ChainContext
        )
    })
}

/// Whether a subtable applies to some glyph of `glyphs` (`intersects`).
pub(crate) fn subtable_intersects(k: Kind, st: D<'_>, glyphs: &BitSet) -> bool {
    let cov = Cov(st.off16(2));
    match k {
        Kind::SingleSubst => matches!(st.u16(0), 1 | 2) && cov.intersects(glyphs),
        Kind::MultipleSubst | Kind::AlternateSubst => st.u16(0) == 1 && cov.intersects(glyphs),
        Kind::LigatureSubst => {
            if st.u16(0) != 1 {
                return false;
            }
            let n = usize::from(st.u16(4));
            cov.glyphs()
                .into_iter()
                .take(n)
                .enumerate()
                .filter(|&(_, g)| glyphs.has(g))
                .any(|(i, _)| {
                    let set = st.off16(6usize.saturating_add(i.saturating_mul(2)));
                    (0..usize::from(set.u16(0))).any(|j| {
                        ligature_intersects(
                            set.off16(2usize.saturating_add(j.saturating_mul(2))),
                            glyphs,
                        )
                    })
                })
        }
        Kind::Context => context::context_intersects(st, glyphs),
        Kind::ChainContext => context::chain_intersects(st, glyphs),
        Kind::ReverseChain => st.u16(0) == 1 && reverse_intersects(st, glyphs),
        Kind::SinglePos => matches!(st.u16(0), 1 | 2) && cov.intersects(glyphs),
        Kind::PairPos => match st.u16(0) {
            1 => super::gpos::pair1_intersects(st, glyphs),
            2 => cov.intersects(glyphs) && super::data::Cls(st.off16(10)).intersects(glyphs),
            _ => false,
        },
        Kind::CursivePos => st.u16(0) == 1 && cov.intersects(glyphs),
        Kind::MarkBasePos | Kind::MarkLigPos | Kind::MarkMarkPos => {
            st.u16(0) == 1 && cov.intersects(glyphs) && Cov(st.off16(4)).intersects(glyphs)
        }
        Kind::Unknown => false,
    }
}

/// Whether some subtable of a lookup applies to `glyphs`.
pub(crate) fn lookup_intersects(lookup: D<'_>, gsub: bool, glyphs: &BitSet) -> bool {
    subtables(lookup, gsub)
        .into_iter()
        .any(|(k, st)| subtable_intersects(k, st, glyphs))
}

/// `hb_ot_layout_lookups_substitute_closure`: grows `glyphs` by the
/// substitutions of `lookups`.
pub(crate) fn substitute_closure(
    table: Gsubgpos<'_>,
    num_glyphs: u32,
    lookups: &BitSet,
    glyphs: BitSet,
) -> BitSet {
    let mut c = Closure::new(table, num_glyphs, glyphs);
    let mut iteration = 0u32;
    loop {
        c.lookup_count = 0;
        let before = c.glyphs.len();
        for i in lookups.iter() {
            c.lookup_closure(i);
        }
        let again = iteration <= CLOSURE_MAX_STAGES && before != c.glyphs.len();
        iteration = iteration.wrapping_add(1);
        if !again {
            break;
        }
    }
    c.flush();
    c.glyphs
}

/// The lookup closure state (`hb_closure_lookups_context_t`).
pub(crate) struct LookupClosure<'a, 'g> {
    pub table: Gsubgpos<'a>,
    pub glyphs: &'g BitSet,
    visited: BitSet,
    inactive: BitSet,
    nesting_left: u32,
    lookup_count: u32,
}

impl<'a> LookupClosure<'a, '_> {
    /// `lookup_limit_exceeded`.
    pub fn limit_exceeded(&self) -> bool {
        self.lookup_count > MAX_LOOKUP_VISIT_COUNT
    }

    /// `recurse`.
    pub fn recurse(&mut self, index: u32) {
        if self.nesting_left == 0 {
            return;
        }
        if self.limit_exceeded() || self.visited.has(index) {
            return;
        }
        self.nesting_left = self.nesting_left.wrapping_sub(1);
        self.lookup(index);
        self.nesting_left = self.nesting_left.wrapping_add(1);
    }

    fn lookup(&mut self, index: u32) {
        let over = self.lookup_count > MAX_LOOKUP_VISIT_COUNT;
        self.lookup_count = self.lookup_count.saturating_add(1);
        if over || self.visited.has(index) {
            return;
        }
        self.visited.add(index);
        let lookup = self.table.lookup(index as usize);
        if !lookup_intersects(lookup, self.table.gsub, self.glyphs) {
            self.inactive.add(index);
            return;
        }
        for (k, st) in subtables(lookup, self.table.gsub) {
            match k {
                Kind::Context => context::context_closure_lookups(self, st),
                Kind::ChainContext => context::chain_closure_lookups(self, st),
                _ => {}
            }
        }
    }
}

/// `GSUBGPOS::closure_lookups`: adds the lookups reached from `lookups`
/// through contextual rules that apply to `glyphs`, and removes the
/// lookups that apply to none of them.
pub(crate) fn closure_lookups(table: Gsubgpos<'_>, glyphs: &BitSet, lookups: &mut BitSet) {
    let mut c = LookupClosure {
        table,
        glyphs,
        visited: BitSet::new(),
        inactive: BitSet::new(),
        nesting_left: MAX_NESTING_LEVEL,
        lookup_count: 0,
    };
    for i in lookups.iter().collect::<Vec<_>>() {
        c.lookup(i);
    }
    lookups.union(&c.visited);
    lookups.subtract(&c.inactive);
}
