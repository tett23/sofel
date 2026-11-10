//! Applying `GSUB` subtables.
//!
//! Ported from HarfBuzz 15.1.0 `src/OT/Layout/GSUB/` and HarfRust 0.15.0
//! `src/ot/lookup/gsub.rs`, MIT; see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::read::GlyphId;
use sofel_font::read::gsub::{Ligature, SubstSubtable};
use sofel_font::read::layout::GlyphArray;

use super::apply::{
    ApplyContext, MAX_CONTEXT_LENGTH, MAX_NESTING_LEVEL, MatchFn, Skip, Which, apply_chain_context,
    apply_context, ligate_input, match_backtrack, match_input, match_lookahead,
};
use super::map::MAX_VALUE;
use crate::buffer::gprops;

/// Applies a subtable at the current glyph.
pub(crate) fn apply<'a>(c: &mut ApplyContext<'_, 'a, '_, '_, '_>, s: &SubstSubtable<'a>) -> bool {
    let glyph = c.buffer.cur(0).codepoint;
    let g = GlyphId(glyph);
    match s {
        SubstSubtable::SingleDelta {
            coverage,
            delta,
            mask,
        } => {
            if coverage.get(g).is_none() {
                return false;
            }
            c.replace_glyph(glyph.wrapping_add(*delta) & mask);
            true
        }
        SubstSubtable::SingleList {
            coverage,
            substitutes,
        } => {
            let Some(index) = coverage.get(g) else {
                return false;
            };
            let Some(sub) = usize::try_from(index).ok().and_then(|i| substitutes.get(i)) else {
                return false;
            };
            c.replace_glyph(sub);
            true
        }
        SubstSubtable::Multiple {
            coverage,
            sequences,
        } => {
            let Some(index) = coverage.get(g) else {
                return false;
            };
            let seq = usize::try_from(index)
                .ok()
                .and_then(|i| sequences.get(i).ok().flatten());
            apply_sequence(c, seq)
        }
        SubstSubtable::Alternate {
            coverage,
            alternate_sets,
        } => {
            let Some(index) = coverage.get(g) else {
                return false;
            };
            let set = usize::try_from(index)
                .ok()
                .and_then(|i| alternate_sets.get(i).ok().flatten());
            apply_alternates(c, set)
        }
        SubstSubtable::Ligature {
            coverage,
            ligature_sets,
        } => {
            let Some(index) = coverage.get(g) else {
                return false;
            };
            let Some(set) = usize::try_from(index)
                .ok()
                .and_then(|i| ligature_sets.get(i).ok().flatten())
            else {
                return false;
            };
            let ligs: Vec<Ligature<'a>> = (0..set.len())
                .filter_map(|i| set.get(i).ok().flatten())
                .collect();
            apply_ligature_set(c, &ligs)
        }
        SubstSubtable::Context(ctx) => apply_context(c, ctx),
        SubstSubtable::ChainContext(ctx) => apply_chain_context(c, ctx),
        SubstSubtable::ReverseChainSingle {
            coverage,
            backtrack,
            lookahead,
            substitutes,
        } => {
            let Some(index) = coverage.get(g) else {
                return false;
            };
            if c.nesting_level_left != MAX_NESTING_LEVEL {
                return false;
            }
            let Some(sub) = usize::try_from(index).ok().and_then(|i| substitutes.get(i)) else {
                return false;
            };
            let idx = c.buffer.idx;
            let back = match_backtrack(c, backtrack.len(), MatchFn::Coverage(*backtrack, 0));
            let (start, end, ok) = match back {
                Ok(start) => match match_lookahead(
                    c,
                    lookahead.len(),
                    MatchFn::Coverage(*lookahead, 0),
                    idx.wrapping_add(1),
                ) {
                    Ok(end) => (start, end, true),
                    Err(end) => (start, end, false),
                },
                Err(start) => (start, 0, false),
            };
            if ok {
                c.buffer.unsafe_to_break_from_outbuffer(start, end);
                c.replace_glyph_inplace(sub);
                true
            } else {
                c.buffer.unsafe_to_concat_from_outbuffer(start, end);
                false
            }
        }
    }
}

/// `Sequence::apply`.
fn apply_sequence(c: &mut ApplyContext<'_, '_, '_, '_, '_>, seq: Option<GlyphArray<'_>>) -> bool {
    let count = seq.map_or(0, |s| s.len());
    if count == 1 {
        let g = seq.and_then(|s| s.get(0)).unwrap_or(0);
        c.replace_glyph(g);
        return true;
    }
    if count == 0 {
        c.buffer.delete_glyph();
        return true;
    }
    let cur = *c.buffer.cur(0);
    let klass = if cur.is_ligature() {
        gprops::BASE_GLYPH
    } else {
        0
    };
    let lig_id = cur.lig_id();
    for i in 0..count {
        if lig_id == 0 {
            let comp = u8::try_from(i & 0x0F).unwrap_or(0);
            c.buffer.cur_mut(0).set_lig_props_for_component(comp);
        }
        let g = seq.and_then(|s| s.get(i)).unwrap_or(0);
        c.output_glyph_for_component(g, klass);
    }
    c.buffer.skip_glyph();
    true
}

/// `AlternateSet::apply`.
fn apply_alternates(c: &mut ApplyContext<'_, '_, '_, '_, '_>, set: Option<GlyphArray<'_>>) -> bool {
    let Some(set) = set else { return false };
    let count = u32::try_from(set.len()).unwrap_or(0);
    if count == 0 {
        return false;
    }
    let glyph_mask = c.buffer.cur(0).mask;
    let lookup_mask = c.lookup_mask;
    let shift = lookup_mask.trailing_zeros();
    let mut alt_index = (lookup_mask & glyph_mask).checked_shr(shift).unwrap_or(0);
    if alt_index == MAX_VALUE && c.random {
        let len = c.buffer.len;
        c.buffer.unsafe_to_break(0, len);
        alt_index = (c.random_number().checked_rem(count).unwrap_or(0)).wrapping_add(1);
    }
    if alt_index > count || alt_index == 0 {
        return false;
    }
    let Some(g) = usize::try_from(alt_index.wrapping_sub(1))
        .ok()
        .and_then(|i| set.get(i))
    else {
        return false;
    };
    c.replace_glyph(g);
    true
}

/// `Ligature::apply`.
fn apply_ligature<'a>(c: &mut ApplyContext<'_, 'a, '_, '_, '_>, lig: &Ligature<'a>) -> bool {
    let count = lig.components.len().wrapping_add(1);
    if count == 1 {
        c.replace_glyph(lig.glyph.0);
        return true;
    }
    if count > MAX_CONTEXT_LENGTH {
        return false;
    }
    match match_input(c, count, MatchFn::Glyph(lig.components)) {
        Ok((end, total)) => {
            ligate_input(c, count, end, lig.glyph.0, total);
            true
        }
        Err(end) => {
            let idx = c.buffer.idx;
            c.buffer.unsafe_to_concat(idx, end);
            false
        }
    }
}

/// `LigatureSet::apply`, with HarfBuzz's fast path for more than one
/// ligature.
fn apply_ligature_set<'a>(c: &mut ApplyContext<'_, 'a, '_, '_, '_>, ligs: &[Ligature<'a>]) -> bool {
    let slow = |c: &mut ApplyContext<'_, 'a, '_, '_, '_>| ligs.iter().any(|l| apply_ligature(c, l));
    if ligs.len() <= 1 {
        return slow(c);
    }
    let idx = c.buffer.idx;
    c.iter_reset(Which::Context, idx);
    c.iter_context.set_match(Some(MatchFn::Always));
    let (second, unsafe_to) = match c.iter_next(Which::Context) {
        Ok(()) => {
            let at = c.iter_context.idx;
            let info = c.buffer.info[at];
            if c.may_skip_at(Which::Context, &info) != Skip::No {
                return slow(c);
            }
            (info.codepoint, at.wrapping_add(1))
        }
        Err(_) => return slow(c),
    };
    let mut unsafe_to_concat = false;
    for l in ligs {
        if l.components.is_empty() || l.components.get(0) == Some(second) {
            if apply_ligature(c, l) {
                if unsafe_to_concat {
                    let idx = c.buffer.idx;
                    c.buffer.unsafe_to_concat(idx, unsafe_to);
                }
                return true;
            }
        } else {
            unsafe_to_concat = true;
        }
    }
    if unsafe_to_concat {
        let idx = c.buffer.idx;
        c.buffer.unsafe_to_concat(idx, unsafe_to);
    }
    false
}
