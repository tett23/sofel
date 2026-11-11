//! Whether a `GSUB` lookup would substitute a glyph sequence
//! (`hb_ot_layout_lookup_would_substitute`): the shapers ask it of the
//! font's lookups to decide the forms of consonants (the indic shaper's
//! `rphf`, `pref`, `blwf`, `pstf` and `vatu`, ADR 0024).
//!
//! Each subtable answers as HarfBuzz's `would_apply`: single, multiple,
//! alternate and reverse chaining substitutions for one glyph in their
//! coverage, ligatures for the exact component sequence, and sequence
//! contexts for an input sequence of the same length, with no backtrack
//! or lookahead when `zero_context` is set. The format 2 and 3 contexts
//! do not look at their coverage (the lookup's digest has already
//! checked the first glyph), as HarfBuzz.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-layout.cc`
//! (`hb_ot_layout_lookup_would_substitute_with_buffer`),
//! `src/hb-ot-layout-gsubgpos.hh` (`hb_would_apply_context_t`,
//! `would_match_input`, the `would_apply` of the contexts) and
//! `src/OT/Layout/GSUB/`, MIT; see THIRD_PARTY_NOTICES.md.

use sofel_font::read::GlyphId;
use sofel_font::read::gsub::SubstSubtable;
use sofel_font::read::layout::{ChainedSequenceContext, SequenceContext};

use super::Table;
use super::apply::{MatchFn, RuleMatch};
use crate::buffer::{Buffer, Info};

/// `hb_would_apply_context_t`: the glyphs, whether context is not
/// allowed, and the buffer whose work budget (`max_ops`) the walk is
/// charged to.
pub(crate) struct WouldApply<'g, 'b> {
    pub(crate) glyphs: &'g [u32],
    pub(crate) zero_context: bool,
    pub(crate) buffer: Option<&'b mut Buffer>,
}

impl WouldApply<'_, '_> {
    /// `charge_ops`.
    fn charge_ops(&mut self, count: usize) -> bool {
        let Some(b) = self.buffer.as_deref_mut() else {
            return true;
        };
        let count = i32::try_from(count).unwrap_or(i32::MAX);
        if b.max_ops < count {
            b.max_ops = -1;
            b.successful = false;
            return false;
        }
        b.max_ops = b.max_ops.wrapping_sub(count);
        true
    }

    fn first(&self) -> GlyphId {
        GlyphId(self.glyphs.first().copied().unwrap_or(0))
    }

    /// `would_match_input`: `count` includes the first glyph, which is
    /// not matched.
    fn match_input(&self, count: usize, m: MatchFn<'_>) -> bool {
        count == self.glyphs.len()
            && self.glyphs.iter().skip(1).enumerate().all(|(i, &g)| {
                let info = Info {
                    codepoint: g,
                    ..Info::default()
                };
                m.matches(&info, i)
            })
    }

    /// `chain_context_would_apply_lookup`.
    fn chain_match(
        &self,
        backtrack: usize,
        lookahead: usize,
        count: usize,
        m: MatchFn<'_>,
    ) -> bool {
        (!self.zero_context || (backtrack == 0 && lookahead == 0)) && self.match_input(count, m)
    }
}

/// `RuleSet::would_apply` and `ChainRuleSet::would_apply` for the rule
/// set at `index` (none for an index past the end, HarfBuzz's `Null`).
fn context_would_apply(c: &mut WouldApply<'_, '_>, s: &SequenceContext<'_>) -> bool {
    let (sets, index, m) = match s {
        SequenceContext::Glyphs {
            coverage,
            rule_sets,
        } => (rule_sets, coverage.get(c.first()), RuleMatch::Glyphs),
        SequenceContext::Classes {
            class_def,
            rule_sets,
            ..
        } => (
            rule_sets,
            Some(class_def.get(c.first())),
            RuleMatch::Classes {
                backtrack: *class_def,
                input: *class_def,
                lookahead: *class_def,
            },
        ),
        SequenceContext::Coverages { coverages, .. } => {
            return c.match_input(coverages.len(), MatchFn::Coverage(*coverages, 1));
        }
    };
    let Some(set) = index
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| sets.get(i).ok().flatten())
    else {
        return false;
    };
    if !c.charge_ops(set.len()) {
        return false;
    }
    (0..set.len())
        .filter_map(|i| set.get(i).ok().flatten())
        .any(|r| c.match_input(r.input.len().wrapping_add(1), m.input(r.input)))
}

fn chain_context_would_apply(c: &mut WouldApply<'_, '_>, s: &ChainedSequenceContext<'_>) -> bool {
    let (sets, index, m) = match s {
        ChainedSequenceContext::Glyphs {
            coverage,
            rule_sets,
        } => (rule_sets, coverage.get(c.first()), RuleMatch::Glyphs),
        ChainedSequenceContext::Classes {
            backtrack_class_def,
            input_class_def,
            lookahead_class_def,
            rule_sets,
            ..
        } => (
            rule_sets,
            Some(input_class_def.get(c.first())),
            RuleMatch::Classes {
                backtrack: *backtrack_class_def,
                input: *input_class_def,
                lookahead: *lookahead_class_def,
            },
        ),
        ChainedSequenceContext::Coverages {
            backtrack,
            input,
            lookahead,
            ..
        } => {
            return c.chain_match(
                backtrack.len(),
                lookahead.len(),
                input.len(),
                MatchFn::Coverage(*input, 1),
            );
        }
    };
    let Some(set) = index
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| sets.get(i).ok().flatten())
    else {
        return false;
    };
    if !c.charge_ops(set.len()) {
        return false;
    }
    (0..set.len())
        .filter_map(|i| set.get(i).ok().flatten())
        .any(|r| {
            c.chain_match(
                r.backtrack.len(),
                r.lookahead.len(),
                r.input.len().wrapping_add(1),
                m.input(r.input),
            )
        })
}

/// A subtable's `would_apply`.
fn subtable_would_apply(c: &mut WouldApply<'_, '_>, s: &SubstSubtable<'_>) -> bool {
    let single = |c: &WouldApply<'_, '_>, coverage: &sofel_font::read::layout::Coverage<'_>| {
        c.glyphs.len() == 1 && coverage.get(c.first()).is_some()
    };
    match s {
        SubstSubtable::SingleDelta { coverage, .. }
        | SubstSubtable::SingleList { coverage, .. }
        | SubstSubtable::Multiple { coverage, .. }
        | SubstSubtable::Alternate { coverage, .. }
        | SubstSubtable::ReverseChainSingle { coverage, .. } => single(c, coverage),
        SubstSubtable::Ligature {
            coverage,
            ligature_sets,
        } => {
            let Some(set) = coverage
                .get(c.first())
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| ligature_sets.get(i).ok().flatten())
            else {
                return false;
            };
            if !c.charge_ops(set.len()) {
                return false;
            }
            (0..set.len())
                .filter_map(|i| set.get(i).ok().flatten())
                .any(|l| {
                    l.components.len().wrapping_add(1) == c.glyphs.len()
                        && c.glyphs
                            .iter()
                            .skip(1)
                            .enumerate()
                            .all(|(i, &g)| l.components.get(i) == Some(g))
                })
        }
        SubstSubtable::Context(ctx) => context_would_apply(c, ctx),
        SubstSubtable::ChainContext(ctx) => chain_context_would_apply(c, ctx),
    }
}

/// `hb_ot_layout_lookup_would_substitute_with_buffer`: whether `GSUB`
/// lookup `lookup_index` would substitute `c.glyphs`
/// (`SubstLookup::would_apply`).
pub(crate) fn lookup_would_substitute(
    gsub: &Table<'_, SubstSubtable<'_>>,
    lookup_index: u16,
    c: &mut WouldApply<'_, '_>,
) -> bool {
    let index = usize::from(lookup_index);
    if index >= gsub.lookup_count() {
        return false;
    }
    let Some(accel) = gsub.lookup(index) else {
        return false;
    };
    let Some(&first) = c.glyphs.first() else {
        return false;
    };
    if !accel.digest.may_have(first) {
        return false;
    }
    if !c.charge_ops(accel.subtables.len().wrapping_add(1)) {
        return false;
    }
    accel
        .subtables
        .iter()
        .any(|s| subtable_would_apply(c, &s.table))
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use sofel_font::read::gsub as gsub_view;

    use super::{WouldApply, lookup_would_substitute};
    use crate::buffer::Buffer;
    use crate::layout::{Table, subst_coverage};
    use crate::shapers::arabic_fallback::{ligature_subtable, single_subtable};

    fn be(v: &[u16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_be_bytes()).collect()
    }

    /// Chaining context format 3: one backtrack glyph 1 and the input
    /// glyphs 5, 6, no lookups.
    fn chain_subtable() -> Vec<u8> {
        // format, backtrack count, offset, input count, 2 offsets,
        // lookahead count, lookup count: 16 bytes, then the coverages.
        let mut s = be(&[3, 1, 16, 2, 22, 28, 0, 0]);
        s.extend(be(&[1, 1, 1, 1, 1, 5, 1, 1, 6]));
        s
    }

    /// A `GSUB` whose lookups 0 to 2 are a single substitution of 5, a
    /// ligature of 5, 6 and the chaining context.
    fn gsub() -> Vec<u8> {
        let subtables = [
            (1u16, single_subtable(&[(5, 9)])),
            (4, ligature_subtable(&[(5, vec![(vec![6], 10)])])),
            (6, chain_subtable()),
        ];
        let lookups: Vec<Vec<u8>> = subtables
            .iter()
            .map(|(ty, s)| {
                let mut l = be(&[*ty, 0, 1, 8]);
                l.extend(s);
                l
            })
            .collect();
        let mut list = be(&[3]);
        let mut at = 2 + 2 * lookups.len();
        for l in &lookups {
            list.extend(be(&[at as u16]));
            at += l.len();
        }
        lookups.iter().for_each(|l| list.extend(l));
        let mut t = vec![0, 1, 0, 0];
        t.extend(be(&[0, 0, 10]));
        t.extend(list);
        t
    }

    fn would(gsub: &[u8], lookup: u16, glyphs: &[u32], zero_context: bool) -> bool {
        let table = Table::new(
            Some(gsub),
            gsub_view::EXTENSION,
            |l, i| gsub_view::subtable(l, i).ok(),
            subst_coverage,
            Some(gsub_view::REVERSE_CHAIN_SINGLE),
        );
        let mut c = WouldApply {
            glyphs,
            zero_context,
            buffer: None,
        };
        lookup_would_substitute(&table, lookup, &mut c)
    }

    #[test]
    fn single_and_ligature() {
        let t = gsub();
        assert!(would(&t, 0, &[5], true));
        assert!(!would(&t, 0, &[5, 6], true));
        assert!(!would(&t, 0, &[6], true));
        assert!(would(&t, 1, &[5, 6], true));
        assert!(!would(&t, 1, &[5, 7], true));
        assert!(!would(&t, 1, &[5], true));
        assert!(!would(&t, 1, &[], true));
        // No such lookup.
        assert!(!would(&t, 3, &[5], true));
    }

    #[test]
    fn chain_context_needs_context_unless_zero() {
        let t = gsub();
        // The backtrack is not looked at, but with zero context a rule
        // that has one does not apply.
        assert!(would(&t, 2, &[5, 6], false));
        assert!(!would(&t, 2, &[5, 6], true));
        assert!(!would(&t, 2, &[5, 7], false));
        assert!(!would(&t, 2, &[5], false));
    }

    #[test]
    fn work_is_charged_to_the_buffer() {
        let t = gsub();
        let table = Table::new(
            Some(t.as_slice()),
            gsub_view::EXTENSION,
            |l, i| gsub_view::subtable(l, i).ok(),
            subst_coverage,
            Some(gsub_view::REVERSE_CHAIN_SINGLE),
        );
        let mut b = Buffer::new();
        b.max_ops = 3;
        let mut c = WouldApply {
            glyphs: &[5, 6],
            zero_context: false,
            buffer: Some(&mut b),
        };
        // One for the lookup and its subtable, one for the ligature set.
        assert!(lookup_would_substitute(&table, 1, &mut c));
        assert_eq!(b.max_ops, 0);
        let mut c = WouldApply {
            glyphs: &[5, 6],
            zero_context: false,
            buffer: Some(&mut b),
        };
        assert!(!lookup_would_substitute(&table, 1, &mut c));
        assert!(!b.successful);
        assert_eq!(b.max_ops, -1);
    }
}
