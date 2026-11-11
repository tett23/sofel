//! Nested references checked while loading a lookup subtable.
use super::super::uint;
use super::super::{ChainedSequenceContext as Chain, CoverageArray, SequenceContext as Context};
use super::target;
use crate::{ReadError, gpos::PosSubtable, gsub::SubstSubtable};

pub(super) struct Budget(usize);
impl Budget {
    pub(super) fn new(bytes: usize) -> Self {
        Self(bytes.saturating_mul(64).clamp(16384, 0x3fffffff))
    }
    pub(super) fn fits(&self, n: usize) -> bool {
        n < self.0
    }
    pub(super) fn take(&mut self, n: usize) -> bool {
        self.0 = self.0.saturating_sub(n);
        self.0 > 0
    }
}

fn covers(array: CoverageArray<'_>, budget: &mut Budget) -> bool {
    budget.take(array.offsets().len())
        && (0..array.len()).all(|i| match array.get(i) {
            Ok(Some(c)) => budget.take(c.records.len()),
            Ok(None) | Err(ReadError::UnknownFormat(_)) => true,
            Err(_) => false,
        })
}

// All three arrays store nullable offsets to sets, which in turn store nullable offsets to records.
macro_rules! sets {
    ($sets:expr,$width:expr,$inner:expr,$budget:expr,$bytes:expr) => {{
        let sets = $sets;
        $budget.take(sets.len().saturating_mul($width))
            && (0..sets.len()).all(|i| match sets.get(i) {
                Ok(Some(set)) => {
                    $budget.take(set.len().saturating_mul($inner))
                        && (0..set.len()).all(|j| {
                            set.get(j)
                                .is_ok_and(|r| r.is_none_or(|r| $budget.take(($bytes)(r))))
                        })
                }
                Ok(None) => true,
                Err(_) => false,
            })
    }};
}

fn context(value: Context<'_>, fmt: u16, budget: &mut Budget) -> bool {
    match value {
        Context::Glyphs { rule_sets, .. } | Context::Classes { rule_sets, .. } => {
            sets!(
                rule_sets,
                if fmt >= 4 { 3 } else { 2 },
                if fmt == 5 { 3 } else { 2 },
                budget,
                |r: crate::layout::Rule| r
                    .input
                    .data()
                    .len()
                    .saturating_add(r.lookups.data().len())
            )
        }
        Context::Coverages { coverages, lookups } => {
            covers(coverages, budget) && budget.take(lookups.data().len())
        }
    }
}

fn chain(value: Chain<'_>, fmt: u16, budget: &mut Budget) -> bool {
    match value {
        Chain::Glyphs { rule_sets, .. } | Chain::Classes { rule_sets, .. } => {
            sets!(
                rule_sets,
                if fmt >= 4 { 3 } else { 2 },
                if fmt >= 4 { 3 } else { 2 },
                budget,
                |r: crate::layout::ChainRule| r.lookups.data().len()
            )
        }
        Chain::Coverages {
            backtrack,
            input,
            lookahead,
            lookups,
        } => {
            covers(backtrack, budget)
                && covers(input, budget)
                && covers(lookahead, budget)
                && budget.take(lookups.data().len())
        }
    }
}

pub(super) fn subst(value: SubstSubtable<'_>, fmt: u16, budget: &mut Budget) -> bool {
    match value {
        SubstSubtable::SingleDelta { coverage, .. } => {
            if !budget.fits(coverage.records.len()) {
                return false;
            }
            let population = if matches!(coverage.format(), 1 | 3) {
                coverage.num_records() as u32
            } else {
                coverage.ranges().fold(0u32, |n, (first, last)| {
                    n.wrapping_add(last.0.saturating_sub(first.0).saturating_add(1))
                })
            };
            budget.take((population >> 1) as usize)
        }
        SubstSubtable::SingleList { substitutes, .. } => budget.take(substitutes.data().len()),
        SubstSubtable::Multiple {
            sequences: arrays, ..
        }
        | SubstSubtable::Alternate {
            alternate_sets: arrays,
            ..
        } => {
            budget.take(arrays.len().saturating_mul(if fmt == 1 { 2 } else { 3 }))
                && (0..arrays.len()).all(|i| {
                    arrays
                        .get(i)
                        .is_ok_and(|a| a.is_none_or(|g| budget.take(g.data().len())))
                })
        }
        SubstSubtable::Ligature { ligature_sets, .. } => sets!(
            ligature_sets,
            if fmt == 1 { 2 } else { 3 },
            if fmt == 1 { 2 } else { 3 },
            budget,
            |l: crate::gsub::Ligature| l.components.data().len()
        ),
        SubstSubtable::Context(c) => context(c, fmt, budget),
        SubstSubtable::ChainContext(c) => chain(c, fmt, budget),
        SubstSubtable::ReverseChainSingle {
            backtrack,
            lookahead,
            substitutes,
            ..
        } => {
            covers(backtrack, budget)
                && covers(lookahead, budget)
                && budget.take(substitutes.data().len())
        }
    }
}

pub(super) fn pos(value: PosSubtable<'_>, data: &[u8], fmt: u16, budget: &mut Budget) -> bool {
    match value {
        PosSubtable::PairGlyphs {
            pair_sets,
            format1,
            format2,
            ..
        } => {
            let width: usize = if fmt == 1 { 2 } else { 3 };
            let stride = width
                .saturating_add(format1.size())
                .saturating_add(format2.size());
            budget.take(pair_sets.len().saturating_mul(width))
                && (0..pair_sets.len()).all(|i| {
                    pair_sets.get(i).is_ok_and(|set| {
                        set.is_none_or(|set| budget.take(set.len().saturating_mul(stride)))
                    })
                })
        }
        PosSubtable::PairClasses {
            format1,
            format2,
            class1_count,
            class2_count,
            ..
        } => {
            let (w, offsets) = if fmt == 2 {
                (2, [8, 10])
            } else {
                (4, [10, 14])
            };
            budget.take(
                usize::from(class1_count)
                    .saturating_mul(usize::from(class2_count))
                    .saturating_mul(format1.size().saturating_add(format2.size())),
            ) && offsets.into_iter().all(|at| {
                uint(data, at, w).is_some_and(|off| {
                    target(data, off, |d| super::lookups::class_bytes(d, budget))
                })
            })
        }
        PosSubtable::Context(c) => context(c, fmt, budget),
        PosSubtable::ChainContext(c) => chain(c, fmt, budget),
        PosSubtable::MarkLig { ligatures, .. } => {
            let width = if fmt == 1 { 2 } else { 3 };
            budget.take(ligatures.len().saturating_mul(width))
                && (0..ligatures.len()).all(|i| {
                    ligatures.get(i).is_ok_and(|m| {
                        m.is_none_or(|m| {
                            budget.take(
                                m.rows()
                                    .saturating_mul(usize::from(m.cols()))
                                    .saturating_mul(width),
                            )
                        })
                    })
                })
        }
        PosSubtable::Single { .. } | PosSubtable::SingleArray { .. } => {
            let at = if fmt >= 3 { 6 } else { 4 };
            let size = crate::gpos::ValueFormat(crate::be::u16_at(data, at).unwrap_or(0)).size();
            let count = match value {
                PosSubtable::SingleArray { values, .. } => values.len(),
                _ => 1,
            };
            budget.take(size.saturating_mul(count))
        }
        PosSubtable::Cursive { entry_exits, .. } => budget.take(
            entry_exits
                .len()
                .saturating_mul(if fmt == 1 { 4 } else { 6 }),
        ),
        _ => true,
    }
}
