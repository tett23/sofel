//! Nested references checked while loading a lookup subtable.
use super::super::uint;
use super::super::{ChainedSequenceContext as Chain, CoverageArray, SequenceContext as Context};
use super::{classes, target};
use crate::{ReadError, gpos::PosSubtable, gsub::SubstSubtable};

pub(super) struct Budget(usize);
impl Budget {
    pub(super) fn new(bytes: usize) -> Self {
        Self(bytes.saturating_mul(64).clamp(16384, 0x3fffffff))
    }
    pub(super) fn take(&mut self, n: usize) -> bool {
        self.0 = self.0.saturating_sub(n);
        self.0 > 0
    }
}

fn covers(array: CoverageArray<'_>, budget: &mut Budget) -> bool {
    budget.take(array.len())
        && (0..array.len())
            .all(|i| matches!(array.get(i), Ok(_) | Err(ReadError::UnknownFormat(_))))
}

// All three arrays store nullable offsets to sets, which in turn store nullable offsets to records.
macro_rules! sets {
    ($sets:expr,$budget:expr) => {{
        let sets = $sets;
        $budget.take(sets.len())
            && (0..sets.len()).all(|i| match sets.get(i) {
                Ok(Some(set)) => {
                    $budget.take(set.len()) && (0..set.len()).all(|j| set.get(j).is_ok())
                }
                Ok(None) => true,
                Err(_) => false,
            })
    }};
}

fn context(value: Context<'_>, budget: &mut Budget) -> bool {
    match value {
        Context::Glyphs { rule_sets, .. } | Context::Classes { rule_sets, .. } => {
            sets!(rule_sets, budget)
        }
        Context::Coverages { coverages, .. } => covers(coverages, budget),
    }
}

fn chain(value: Chain<'_>, budget: &mut Budget) -> bool {
    match value {
        Chain::Glyphs { rule_sets, .. } | Chain::Classes { rule_sets, .. } => {
            sets!(rule_sets, budget)
        }
        Chain::Coverages {
            backtrack,
            input,
            lookahead,
            ..
        } => covers(backtrack, budget) && covers(input, budget) && covers(lookahead, budget),
    }
}

pub(super) fn subst(value: SubstSubtable<'_>, budget: &mut Budget) -> bool {
    match value {
        SubstSubtable::SingleDelta { coverage, .. } => {
            if !budget.take(coverage.num_records()) {
                return false;
            }
            let size = coverage.ranges().fold(0usize, |n, (first, last)| {
                n.saturating_add((last.0.saturating_sub(first.0).saturating_add(1)) as usize)
            });
            budget.take(size / 2)
        }
        SubstSubtable::SingleList { .. } => true,
        SubstSubtable::Multiple {
            sequences: arrays, ..
        }
        | SubstSubtable::Alternate {
            alternate_sets: arrays,
            ..
        } => budget.take(arrays.len()) && (0..arrays.len()).all(|i| arrays.get(i).is_ok()),
        SubstSubtable::Ligature { ligature_sets, .. } => sets!(ligature_sets, budget),
        SubstSubtable::Context(c) => context(c, budget),
        SubstSubtable::ChainContext(c) => chain(c, budget),
        SubstSubtable::ReverseChainSingle {
            backtrack,
            lookahead,
            ..
        } => covers(backtrack, budget) && covers(lookahead, budget),
    }
}

pub(super) fn pos(value: PosSubtable<'_>, data: &[u8], fmt: u16, budget: &mut Budget) -> bool {
    match value {
        PosSubtable::PairGlyphs { pair_sets, .. } => {
            budget.take(pair_sets.len()) && (0..pair_sets.len()).all(|i| pair_sets.get(i).is_ok())
        }
        PosSubtable::PairClasses { .. } => {
            let (w, offsets) = if fmt == 2 {
                (2, [8, 10])
            } else {
                (4, [10, 14])
            };
            offsets
                .into_iter()
                .all(|at| uint(data, at, w).is_some_and(|off| target(data, off, classes)))
        }
        PosSubtable::Context(c) => context(c, budget),
        PosSubtable::ChainContext(c) => chain(c, budget),
        PosSubtable::MarkLig { ligatures, .. } => {
            budget.take(ligatures.len()) && (0..ligatures.len()).all(|i| ligatures.get(i).is_ok())
        }
        _ => true,
    }
}
