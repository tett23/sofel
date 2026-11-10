//! The subsetting of GSUB's own subtables (`OT/Layout/GSUB/`): single,
//! multiple, alternate, ligature and reverse chaining substitution.

use alloc::vec::Vec;

use super::closure::{ligature_intersects, reverse_parts};
use super::common::Sub;
use super::data::{Cov, D, lo16, n16};
use crate::bits::BitSet;

/// `SingleSubst::serialize` of `(glyph, substitute)` pairs into the
/// current object.
fn single_serialize(s: &mut Sub<'_, '_>, pairs: &[(u32, u32)]) {
    let Some(&(g0, s0)) = pairs.first() else {
        s.s.u16(2);
        let cov_at = s.s.u16(0);
        s.s.u16(0);
        s.serialize_serialize(cov_at, 2, |s| s.coverage(&[]));
        return;
    };
    if pairs.iter().any(|p| p.1 > 0xFFFF) {
        s.unsupported("24-bit SingleSubst (ADR 0022 stage 3)");
        return;
    }
    let delta = s0.wrapping_sub(g0) & 0xFFFF;
    let same = pairs
        .iter()
        .skip(1)
        .all(|p| p.1.wrapping_sub(p.0) & 0xFFFF == delta);
    let glyphs: Vec<u32> = pairs.iter().map(|p| p.0).collect();
    if same {
        s.s.u16(1);
        let cov_at = s.s.u16(0);
        s.s.u16(delta as u16);
        s.serialize_serialize(cov_at, 2, |s| s.coverage(&glyphs));
    } else {
        s.s.u16(2);
        let cov_at = s.s.u16(0);
        s.s.u16(n16(pairs.len()));
        pairs.iter().for_each(|p| {
            s.s.u16(lo16(p.1));
        });
        s.serialize_serialize(cov_at, 2, |s| s.coverage(&glyphs));
    }
}

/// SingleSubst formats 1 and 2.
pub(crate) fn single(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let cov = Cov(st.off16(2));
    let gs = &s.p.layout.glyphset_gsub;
    let pairs: Vec<(u32, u32)> = match st.u16(0) {
        1 => {
            let d = u32::from(st.u16(4));
            let mut inter = BitSet::new();
            cov.intersect_set(gs, &mut inter);
            inter
                .iter()
                .map(|g| (g, g.wrapping_add(d) & 0xFFFF))
                .filter(|p| gs.has(p.1))
                .map(|(a, b)| (s.map(a), s.map(b)))
                .collect()
        }
        2 => {
            let n = usize::from(st.u16(4));
            cov.glyphs()
                .into_iter()
                .take(n)
                .enumerate()
                .map(|(i, g)| (g, u32::from(st.u16_at(6, i))))
                .filter(|p| gs.has(p.0) && gs.has(p.1))
                .map(|(a, b)| (s.map(a), s.map(b)))
                .collect()
        }
        _ => return false,
    };
    single_serialize(s, &pairs);
    !pairs.is_empty()
}

/// MultipleSubst (`multiple`) and AlternateSubst format 1.
pub(crate) fn multiple_or_alternate(s: &mut Sub<'_, '_>, st: D<'_>, multiple: bool) -> bool {
    if st.u16(0) != 1 {
        return false;
    }
    s.s.u16(1);
    let cov_at = s.s.u16(0);
    let count_at = s.s.u16(0);
    let cov = Cov(st.off16(2));
    let n = usize::from(st.u16(4));
    let mut new_cov = Vec::new();
    for (i, g) in cov.glyphs().into_iter().take(n).enumerate() {
        if !s.in_gsub(g) {
            continue;
        }
        let set = st.off16(6usize.saturating_add(i.saturating_mul(2)));
        if set.is_null() {
            continue;
        }
        let ok = s.offset_append(2, |s| {
            let m = usize::from(set.u16(0));
            let glyphs: Vec<u32> = (0..m).map(|j| u32::from(set.u16_at(2, j))).collect();
            let out: Vec<u32> = if multiple {
                if !glyphs.iter().all(|&g| s.in_gsub(g)) {
                    return false;
                }
                glyphs.iter().map(|&g| s.map(g)).collect()
            } else {
                glyphs
                    .iter()
                    .filter(|&&g| s.in_gsub(g))
                    .map(|&g| s.map(g))
                    .collect()
            };
            s.s.u16(n16(out.len()));
            out.iter().for_each(|&g| {
                s.s.u16((g & 0xFFFF) as u16);
            });
            multiple || !out.is_empty()
        });
        if ok {
            s.bump_u16(count_at, 1);
            new_cov.push(s.map(g));
        }
    }
    s.serialize_serialize(cov_at, 2, |s| s.coverage(&new_cov));
    !new_cov.is_empty()
}

/// `LigatureSet::intersects_lig_glyph`.
fn set_intersects_lig_glyph(set: D<'_>, glyphs: &BitSet) -> bool {
    (0..usize::from(set.u16(0))).any(|j| {
        let lig = set.off16(2usize.saturating_add(j.saturating_mul(2)));
        glyphs.has(u32::from(lig.u16(0))) && ligature_intersects(lig, glyphs)
    })
}

/// LigatureSubst format 1.
pub(crate) fn ligature(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    if st.u16(0) != 1 {
        return false;
    }
    s.s.u16(1);
    let cov_at = s.s.u16(0);
    let count_at = s.s.u16(0);
    let cov = Cov(st.off16(2));
    let n = usize::from(st.u16(4));
    let set_at = |i: usize| st.off16(6usize.saturating_add(i.saturating_mul(2)));
    let pairs: Vec<(usize, u32)> = cov.glyphs().into_iter().take(n).enumerate().collect();
    let mut new_cov = BitSet::new();
    for &(i, g) in &pairs {
        if s.in_gsub(g) && set_intersects_lig_glyph(set_at(i), &s.p.layout.glyphset_gsub) {
            new_cov.add(g);
        }
    }
    let mapped: Vec<u32> = new_cov.iter().map(|g| s.map(g)).collect();
    s.s.push();
    if !s.coverage(&mapped) {
        s.s.pop_discard();
        return false;
    }
    let coverage_idx = s.s.pop_pack(true);
    s.s.link16(cov_at, coverage_idx);
    for &(i, g) in &pairs {
        if !new_cov.has(g) {
            continue;
        }
        let set = set_at(i);
        if set.is_null() {
            continue;
        }
        if s.offset_append(2, |s| ligature_set(s, set, coverage_idx)) {
            s.bump_u16(count_at, 1);
        }
    }
    !new_cov.is_empty()
}

/// `LigatureSet::subset`.
fn ligature_set(s: &mut Sub<'_, '_>, set: D<'_>, coverage_idx: u32) -> bool {
    let count_at = s.s.u16(0);
    let mut n = 0u16;
    for j in 0..usize::from(set.u16(0)) {
        let lig = set.off16(2usize.saturating_add(j.saturating_mul(2)));
        if lig.is_null() {
            continue;
        }
        if s.offset_append(2, |s| {
            let gs = &s.p.layout.glyphset_gsub;
            if !ligature_intersects(lig, gs) || !gs.has(u32::from(lig.u16(0))) {
                return false;
            }
            s.s.add_virtual_link(coverage_idx);
            let comps = usize::from(lig.u16(2)).saturating_sub(1);
            s.s.u16((s.map(u32::from(lig.u16(0))) & 0xFFFF) as u16);
            s.s.u16(n16(comps.wrapping_add(1)));
            for k in 0..comps {
                let g = s.map(u32::from(lig.u16_at(4, k)));
                s.s.u16((g & 0xFFFF) as u16);
            }
            true
        }) {
            n = n.wrapping_add(1);
        }
    }
    s.s.set_u16(count_at, n);
    if n != 0 {
        s.s.add_virtual_link(coverage_idx);
    }
    n != 0
}

/// ReverseChainSingleSubst format 1.
pub(crate) fn reverse(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    if st.u16(0) != 1 {
        return false;
    }
    let cov = Cov(st.off16(2));
    let (backtrack, lookahead, subst) = reverse_parts(st);
    let n = usize::from(subst.u16(0));
    let gs = &s.p.layout.glyphset_gsub;
    let pairs: Vec<(u32, u32)> = cov
        .glyphs()
        .into_iter()
        .take(n)
        .enumerate()
        .map(|(i, g)| (g, u32::from(subst.u16_at(2, i))))
        .filter(|p| gs.has(p.0) && gs.has(p.1))
        .map(|(a, b)| (s.map(a), s.map(b)))
        .collect();
    if pairs.is_empty() {
        return false;
    }
    s.s.u16(1);
    let cov_at = s.s.u16(0);
    for arr in [backtrack, lookahead] {
        let count_at = s.s.u16(0);
        for i in 0..usize::from(arr.u16(0)) {
            let c = st.follow(u32::from(arr.u16_at(2, i)));
            let at = s.s.u16(0);
            s.bump_u16(count_at, 1);
            if !s.serialize_subset_opt(at, 2, c, |s| s.coverage_subset(Cov(c))) {
                return false;
            }
        }
    }
    s.s.u16(n16(pairs.len()));
    pairs.iter().for_each(|p| {
        s.s.u16((p.1 & 0xFFFF) as u16);
    });
    let glyphs: Vec<u32> = pairs.iter().map(|p| p.0).collect();
    s.serialize_serialize(cov_at, 2, |s| s.coverage(&glyphs));
    true
}
