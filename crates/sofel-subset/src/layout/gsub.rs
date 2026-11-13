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
    let (at, width) = single_header(s.s, pairs);
    let glyphs: Vec<u32> = pairs.iter().map(|p| p.0).collect();
    s.serialize_serialize(at, width, |s| s.coverage(&glyphs));
}

/// Writes the substitution operands and returns the Coverage offset field and its width.
fn single_header(s: &mut sofel_write::Serializer, pairs: &[(u32, u32)]) -> (usize, u8) {
    use sofel_write::Errors;
    let wide = pairs.iter().any(|p| p.1 > 0xFFFF);
    let mask = if wide { 0xFF_FFFF } else { 0xFFFF };
    let delta = pairs
        .first()
        .map(|p| p.1.wrapping_sub(p.0) & mask)
        .filter(|&d| pairs.iter().all(|p| p.1.wrapping_sub(p.0) & mask == d));
    s.u16(match (wide, delta.is_some()) {
        (false, true) => 1,
        (false, false) => 2,
        (true, true) => 3,
        (true, false) => 4,
    });
    let field = if wide { (s.u32(0), 4) } else { (s.u16(0), 2) };
    if pairs.iter().any(|p| p.1 > 0xFF_FFFF) {
        s.err(Errors::INT_OVERFLOW);
    } else if let Some(d) = delta {
        if wide {
            s.u24(d);
        } else {
            s.u16(lo16(d));
        }
    } else if wide {
        let at = s.u24(0);
        s.check_assign_u24(at, pairs.len() as u64, Errors::ARRAY_OVERFLOW);
        for &(_, gid) in pairs {
            s.u24(gid);
        }
    } else {
        let at = s.u16(0);
        s.check_assign_u16(at, pairs.len() as u64, Errors::ARRAY_OVERFLOW);
        for &(_, gid) in pairs {
            s.u16(lo16(gid));
        }
    }
    field
}

/// SingleSubst formats 1 through 4.
pub(crate) fn single(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let sub = super::data::SingleSubst(st);
    let cov = sub.coverage();
    let gs = &s.p.layout.glyphset_gsub;
    let pairs: Vec<(u32, u32)> = match st.u16(0) {
        1 | 3 => {
            let d = sub.operand(0);
            let mut inter = BitSet::new();
            cov.intersect_set(gs, &mut inter);
            inter
                .iter()
                .map(|g| (g, g.wrapping_add(d) & sub.mask()))
                .filter(|p| gs.has(p.1))
                .map(|(a, b)| (s.map(a), s.map(b)))
                .collect()
        }
        2 | 4 => {
            let n = sub.operand(0) as usize;
            cov.glyphs()
                .into_iter()
                .take(n)
                .enumerate()
                .map(|(i, g)| (g, sub.operand(i.saturating_add(1))))
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
    let Some(sets) = super::data::SequenceSets::new(st) else {
        return false;
    };
    let width = sets.width;
    let cov_width: u8 = if width == 3 { 4 } else { 2 };
    s.s.u16(st.u16(0));
    let cov_at = s.s.zeros(usize::from(cov_width));
    let count_at = s.s.zeros(usize::from(width));
    let mut new_cov = Vec::new();
    for (i, g) in sets
        .coverage()
        .glyphs()
        .into_iter()
        .take(sets.count())
        .enumerate()
    {
        if !s.in_gsub(g) {
            continue;
        }
        let Some(seq) = sets.sequence(i) else {
            continue;
        };
        let ok = s.offset_append(width, |s| {
            let glyphs: Vec<u32> = seq.collect();
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
            if width == 3 {
                if !multiple && out.is_empty() {
                    return false;
                }
                s.s.u24(super::data::n32(out.len()));
                for g in out {
                    s.s.u24(g);
                }
                return true;
            }
            s.s.u16(n16(out.len()));
            for &g in &out {
                let at = s.s.u16(0);
                s.s.check_assign_u16(at, u64::from(g), sofel_write::Errors::INT_OVERFLOW);
            }
            multiple || !out.is_empty()
        });
        if ok {
            new_cov.push(s.map(g));
        }
    }
    if width == 3 {
        s.s.set_u24(count_at, super::data::n32(new_cov.len()));
    } else {
        s.s.set_u16(count_at, n16(new_cov.len()));
    }
    s.serialize_serialize(cov_at, cov_width, |s| s.coverage(&new_cov));
    !new_cov.is_empty()
}

/// LigatureSubst formats 1 and 2.
pub(crate) fn ligature(s: &mut Sub<'_, '_>, st: D<'_>) -> bool {
    let Some(sets) = super::data::SequenceSets::new(st) else {
        return false;
    };
    let width = sets.width;
    s.s.u16(st.u16(0));
    let cov_at = s.s.zeros(if width == 3 { 4 } else { 2 });
    let count_at = s.s.zeros(usize::from(width));
    let cov = sets.coverage();
    let n = sets.count();
    let pairs: Vec<(usize, u32)> = cov.glyphs().into_iter().take(n).enumerate().collect();
    let mut new_cov = BitSet::new();
    for &(i, g) in &pairs {
        if s.in_gsub(g)
            && super::data::ligatures(sets.set(i), width).any(|lig| {
                s.in_gsub(lig.uint(0, width))
                    && ligature_intersects(lig, width, &s.p.layout.glyphset_gsub)
            })
        {
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
    if width == 3 {
        s.s.link32(cov_at, coverage_idx);
    } else {
        s.s.link16(cov_at, coverage_idx);
    }
    let mut count = 0u32;
    for &(i, g) in &pairs {
        if !new_cov.has(g) {
            continue;
        }
        let set = sets.set(i);
        if set.is_null() {
            continue;
        }
        if s.offset_append(width, |s| ligature_set(s, set, width, coverage_idx)) {
            count = count.saturating_add(1);
        }
    }
    if width == 3 {
        s.s.set_u24(count_at, count);
    } else {
        s.s.set_u16(count_at, lo16(count));
    }
    !new_cov.is_empty()
}

/// `LigatureSet::subset`.
fn ligature_set(s: &mut Sub<'_, '_>, set: D<'_>, width: u8, coverage_idx: u32) -> bool {
    let count_at = s.s.u16(0);
    let mut n = 0u16;
    for lig in super::data::ligatures(set, width) {
        if s.offset_append(width, |s| {
            if !ligature_intersects(lig, width, &s.p.layout.glyphset_gsub)
                || !s.in_gsub(lig.uint(0, width))
            {
                return false;
            }
            s.s.add_virtual_link(coverage_idx);
            let write_gid = |s: &mut Sub<'_, '_>, old| {
                let g = s.map(old);
                if width == 3 {
                    s.s.u24(g);
                } else {
                    let at = s.s.u16(0);
                    s.s.check_assign_u16(at, u64::from(g), sofel_write::Errors::INT_OVERFLOW);
                }
            };
            write_gid(s, lig.uint(0, width));
            s.s.u16(lig.u16(usize::from(width)).max(1));
            for g in super::data::components(lig, width) {
                write_gid(s, g);
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
    if !matches!(st.u16(0), 1 | 2) {
        return false;
    }
    let (cov, width, contexts, subst) = reverse_parts(st);
    let n = subst.uint(0, width) as usize;
    let gs = &s.p.layout.glyphset_gsub;
    let pairs: Vec<(u32, u32)> = cov
        .glyphs()
        .into_iter()
        .take(n)
        .enumerate()
        .map(|(i, g)| {
            (
                g,
                subst.uint(
                    i.saturating_add(1).saturating_mul(usize::from(width)),
                    width,
                ),
            )
        })
        .filter(|p| gs.has(p.0) && gs.has(p.1))
        .map(|(a, b)| (s.map(a), s.map(b)))
        .collect();
    if pairs.is_empty() {
        return false;
    }
    s.s.u16(st.u16(0));
    let cov_width = if width == 3 { 4 } else { 2 };
    let cov_at = s.s.zeros(usize::from(cov_width));
    for arr in contexts {
        let count_at = s.s.u16(0);
        for i in 0..usize::from(arr.u16(0)) {
            let c = st.follow(arr.uint(
                2usize.saturating_add(i.saturating_mul(usize::from(width))),
                width,
            ));
            let at = s.s.zeros(usize::from(width));
            s.bump_u16(count_at, 1);
            if !s.serialize_subset_opt(at, width, c, |s| s.coverage_subset(Cov(c))) {
                return false;
            }
        }
    }
    if width == 3 {
        s.s.u24(super::data::n32(pairs.len()));
        for &(_, g) in &pairs {
            s.s.u24(g);
        }
    } else {
        s.s.u16(n16(pairs.len()));
        for &(_, g) in &pairs {
            let at = s.s.u16(0);
            s.s.check_assign_u16(at, u64::from(g), sofel_write::Errors::INT_OVERFLOW);
        }
    }
    let glyphs: Vec<u32> = pairs.iter().map(|p| p.0).collect();
    s.serialize_serialize(cov_at, cov_width, |s| s.coverage(&glyphs));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use sofel_write::Serializer;

    #[test]
    fn adr0022_single_substitution_writes_24_bit_operands() {
        for (pairs, expected) in [
            (
                &[(1, 65536), (2, 65537)][..],
                &[0, 3, 0, 0, 0, 0, 0, 255, 255][..],
            ),
            (
                &[(1, 65536), (2, 16777215)][..],
                &[0, 4, 0, 0, 0, 0, 0, 0, 2, 1, 0, 0, 255, 255, 255][..],
            ),
        ] {
            let mut serializer = Serializer::new();
            assert_eq!(single_header(&mut serializer, pairs), (2, 4));
            assert_eq!(serializer.current(), expected);
        }
    }
}
