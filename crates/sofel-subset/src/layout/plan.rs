//! The layout part of the plan (`hb-subset-plan-layout.cc`): the
//! features, scripts and lookups to keep, the GSUB glyph closure, the
//! deduplication of features, the language systems and the name ids of
//! feature parameters.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use sofel_font::read::feature_variations::FeatureVariations;

use super::closure::{closure_lookups, substitute_closure};
use super::data::{D, bsearch, n32};
use super::limits::{MAX_FEATURE_INDICES, MAX_LANGSYS, MAX_LANGSYS_FEATURE_COUNT, MAX_SCRIPTS};
use super::{Gsubgpos, LayoutPlan, feature_params};
use crate::bits::BitSet;
use crate::set::{INVALID, RangeSet};

/// `_filter_tag_list`: the tags in `filter`, without zero and duplicate
/// tags; and whether a tag was left out for not being in `filter`.
fn filter_tags(tags: &[u32], filter: &RangeSet) -> (Vec<u32>, bool) {
    let mut out = Vec::new();
    let mut removed = false;
    let mut seen: Vec<u32> = Vec::new();
    for &t in tags {
        if t == 0 {
            continue;
        }
        if seen.contains(&t) {
            continue;
        }
        if !filter.contains(t) {
            removed = true;
            continue;
        }
        seen.push(t);
        out.push(t);
    }
    (out, removed)
}

/// The required feature index of a LangSys; HarfBuzz's Null LangSys has
/// none (0xFFFF).
fn required(l: D<'_>) -> u16 {
    if l.is_null() { 0xFFFF } else { l.u16(2) }
}

/// `hb_collect_features_context_t` and `hb_ot_layout_collect_features`.
struct Collect<'a> {
    t: Gsubgpos<'a>,
    filter: Option<BitSet>,
    out: BitSet,
    visited_script: BitSet,
    visited_langsys: BitSet,
    script_count: u32,
    langsys_count: u32,
    feature_index_count: u32,
}

impl<'a> Collect<'a> {
    fn visited_script(&mut self, s: D<'a>) -> bool {
        if s.u16(0) == 0 && s.u16(2) == 0 {
            return true;
        }
        let over = self.script_count > MAX_SCRIPTS;
        self.script_count = self.script_count.saturating_add(1);
        if over {
            return true;
        }
        let delta = n32(self.t.delta(s));
        if self.visited_script.has(delta) {
            return true;
        }
        self.visited_script.add(delta);
        false
    }

    fn visited_langsys(&mut self, l: D<'a>) -> bool {
        if required(l) == 0xFFFF && l.u16(4) == 0 {
            return true;
        }
        let over = self.langsys_count > MAX_LANGSYS;
        self.langsys_count = self.langsys_count.saturating_add(1);
        if over {
            return true;
        }
        let delta = n32(self.t.delta(l));
        if self.visited_langsys.has(delta) {
            return true;
        }
        self.visited_langsys.add(delta);
        false
    }

    fn visited_feature_indices(&mut self, n: u32) -> bool {
        self.feature_index_count = self.feature_index_count.saturating_add(n);
        self.feature_index_count > MAX_FEATURE_INDICES
    }

    fn langsys(&mut self, l: D<'a>) {
        if self.visited_langsys(l) {
            return;
        }
        let n = usize::from(l.u16(4));
        match &mut self.filter {
            None => {
                let req = required(l);
                if req != 0xFFFF && !self.visited_feature_indices(1) {
                    self.out.add(u32::from(req));
                }
                if !self.visited_feature_indices(n32(n)) {
                    (0..n).for_each(|i| self.out.add(u32::from(l.u16_at(6, i))));
                }
            }
            Some(filter) => {
                if filter.is_empty() {
                    return;
                }
                for i in 0..n {
                    let f = u32::from(l.u16_at(6, i));
                    if !filter.has(f) {
                        continue;
                    }
                    self.out.add(f);
                    filter.remove(f);
                }
            }
        }
    }

    fn script(&mut self, s: D<'a>) {
        if self.visited_script(s) {
            return;
        }
        let default = s.off16(0);
        if !default.is_null() {
            self.langsys(default);
        }
        for i in 0..usize::from(s.u16(2)) {
            self.langsys(s.off16(4usize.saturating_add(i.saturating_mul(6)).saturating_add(4)));
        }
    }
}

/// `find_script_index`.
fn find_script(t: Gsubgpos<'_>, tag: u32) -> Option<usize> {
    bsearch(t.script_count(), |i| tag.cmp(&t.script_tag(i)))
}

/// The lookup indices of feature `f`.
fn feature_lookups(f: D<'_>) -> impl Iterator<Item = u32> + '_ {
    (0..usize::from(f.u16(2))).map(move |i| u32::from(f.u16_at(4, i)))
}

/// The substitution records of FeatureVariations version 1.x:
/// `(feature index, substitute feature)`, per record.
fn substitutions(fv: Option<FeatureVariations<'_>>) -> Vec<Vec<(u32, D<'_>)>> {
    let Some(fv) = fv else {
        return Vec::new();
    };
    (0..fv.len())
        .map(|i| {
            let subs = fv.record(i).and_then(|r| r.substitutions().ok().flatten());
            let Some(subs) = subs else {
                return Vec::new();
            };
            (0..subs.len())
                .filter_map(|j| {
                    let record = subs.record(j)?;
                    Some((
                        u32::from(record.feature_index()),
                        D(record.feature().ok().flatten().unwrap_or(&[])),
                    ))
                })
                .collect()
        })
        .collect()
}

/// The FeatureVariations record that applies when every axis is pinned
/// to its requested location (`collect_feature_substitutes_with_variations` with
/// `all_axes_pinned`): the first record whose conditions all hold at the
/// normalized location. Only conditions of format 1 (axis ranges) are
/// evaluated by this version.
pub(crate) fn applied_record(
    fv: Option<FeatureVariations<'_>>,
    coords: &[i32],
) -> Result<Option<usize>, &'static str> {
    let Some(fv) = fv else {
        return Ok(None);
    };
    if fv.version() != 0x0001_0000 {
        return Err("pinning the axes of FeatureVariations 1.1 (deferred by ADR 0022)");
    }
    for i in 0..fv.len() {
        let conditions = fv.record(i).and_then(|r| r.conditions().ok().flatten());
        let mut applies = true;
        if let Some(conditions) = conditions {
            for j in 0..conditions.len() {
                let condition = conditions.condition(j).ok().flatten();
                let Some((axis, min, max)) = condition.and_then(|c| c.axis_range()) else {
                    return Err(
                        "pinning FeatureVariations conditions other than format 1 (deferred by ADR 0022)",
                    );
                };
                if coords
                    .get(usize::from(axis))
                    .is_none_or(|&coord| coord < i32::from(min) || coord > i32::from(max))
                {
                    applies = false;
                    break;
                }
            }
        }
        if applies {
            return Ok(usize::try_from(i).ok());
        }
    }
    Ok(None)
}

/// The substitutes of `features` from the record that applies at the
/// pinned location: feature index to the offset of the substitute.
pub(crate) fn pinned_substitutes(
    t: Gsubgpos<'_>,
    features: &BitSet,
    coords: &[i32],
) -> Result<BTreeMap<u32, usize>, &'static str> {
    let fv = t.feature_variations_view();
    let mut out = BTreeMap::new();
    if let Some(i) = applied_record(fv, coords)? {
        let subs = fv
            .and_then(|f| f.record(u32::try_from(i).ok()?))
            .and_then(|r| r.substitutions().ok().flatten());
        if let Some(subs) = subs {
            for j in 0..subs.len() {
                let Some(record) = subs.record(j) else {
                    continue;
                };
                let fi = u32::from(record.feature_index());
                if features.has(fi) {
                    out.insert(
                        fi,
                        t.delta(D(record.feature().ok().flatten().unwrap_or(&[]))),
                    );
                }
            }
        }
    }
    Ok(out)
}

/// The axes pinned for a layout plan: `None` without pinning, else the
/// normalized 2.14 coordinates in fvar axis order.
pub(crate) type Pinned<'a> = Option<&'a [i32]>;

/// `_closure_glyphs_lookups_features`: what to keep of GSUB or GPOS;
/// for GSUB also grows `glyphs` by the substitution closure. With every
/// axis pinned, FeatureVariations is evaluated there and
/// its substitutes replace the features.
pub(crate) fn closure_glyphs_lookups_features(
    t: Gsubgpos<'_>,
    num_glyphs: u32,
    no_layout_closure: bool,
    layout_features: &RangeSet,
    layout_scripts: &RangeSet,
    glyphs: &mut BitSet,
    pinned: Pinned<'_>,
) -> LayoutPlan {
    // _collect_layout_indices
    let feature_tags: Vec<u32> = (0..t.feature_count()).map(|i| t.feature_tag(i)).collect();
    let (features, removed_features) = filter_tags(&feature_tags, layout_features);
    let script_tags: Vec<u32> = (0..t.script_count()).map(|i| t.script_tag(i)).collect();
    let (scripts, removed_scripts) = filter_tags(&script_tags, layout_scripts);

    let filter = removed_features.then(|| {
        let set: Vec<u32> = features.clone();
        BitSet::from_values(
            (0..t.feature_count())
                .filter(|&i| set.contains(&t.feature_tag(i)))
                .map(n32),
        )
    });
    let mut c = Collect {
        t,
        filter,
        out: BitSet::new(),
        visited_script: BitSet::new(),
        visited_langsys: BitSet::new(),
        script_count: 0,
        langsys_count: 0,
        feature_index_count: 0,
    };
    if removed_scripts {
        for &tag in &scripts {
            if let Some(i) = find_script(t, tag) {
                c.script(t.script(i));
            }
        }
    } else {
        for i in 0..t.script_count() {
            c.script(t.script(i));
        }
    }
    let mut feature_indices = c.out;
    let mut lookup_indices = BitSet::new();
    let (substitutes, unsupported) = match pinned {
        Some(axes) => match pinned_substitutes(t, &feature_indices, axes) {
            Ok(m) => (m, None),
            Err(e) => (BTreeMap::new(), Some(e)),
        },
        None => (BTreeMap::new(), None),
    };
    // The feature, or its substitute.
    let feature = |i: u32| {
        substitutes
            .get(&i)
            .map_or_else(|| t.feature(i as usize), |&off| t.d.at(off))
    };
    for f in feature_indices.iter() {
        feature_lookups(feature(f)).for_each(|l| lookup_indices.add(l));
    }
    // Pinning drops FeatureVariations: its lookups are not collected.
    let subs = if pinned.is_some() {
        Vec::new()
    } else {
        substitutions(t.feature_variations_view())
    };
    for record in &subs {
        for &(fi, f) in record {
            if feature_indices.has(fi) {
                feature_lookups(f).for_each(|l| lookup_indices.add(l));
            }
        }
    }

    if t.gsub && !no_layout_closure {
        *glyphs = substitute_closure(t, num_glyphs, &lookup_indices, core::mem::take(glyphs));
    }
    closure_lookups(t, glyphs, &mut lookup_indices);
    let lookups: BTreeMap<u32, u32> = lookup_indices.iter().zip(0u32..).collect();

    // prune_features
    let mut alternate = BitSet::new();
    for record in &subs {
        for &(fi, f) in record {
            if feature_lookups(f).any(|l| lookups.contains_key(&l)) {
                alternate.add(fi);
            }
        }
    }
    for i in feature_indices.iter().collect::<Vec<_>>() {
        let tag = t.feature_tag(i as usize);
        if tag == u32::from_be_bytes(*b"pref") {
            continue;
        }
        let f = feature(i);
        if !feature_params(f, tag).is_null() && tag == u32::from_be_bytes(*b"size") {
            continue;
        }
        if !feature_lookups(f).any(|l| lookups.contains_key(&l)) && !alternate.has(i) {
            feature_indices.remove(i);
        }
    }

    // _GSUBGPOS_find_duplicate_features
    let mut duplicates: BTreeMap<u32, u32> = BTreeMap::new();
    if !feature_indices.is_empty() {
        let mut variable = BitSet::new();
        subs.iter().flatten().for_each(|&(fi, _)| variable.add(fi));
        let mut unique: BTreeMap<u32, BitSet> = BTreeMap::new();
        for i in feature_indices.iter() {
            let tag = t.feature_tag(i as usize);
            if tag == INVALID {
                continue;
            }
            let Some(same) = unique.get_mut(&tag) else {
                unique.insert(tag, BitSet::from_values([i]));
                duplicates.insert(i, i);
                continue;
            };
            let mut found = None;
            for other in same.iter() {
                if variable.has(i) || variable.has(other) {
                    continue;
                }
                let f = feature(i);
                let of = feature(other);
                if !equal_params(f, of, tag) {
                    continue;
                }
                let a = feature_lookups(f).filter(|l| lookups.contains_key(l));
                let b = feature_lookups(of).filter(|l| lookups.contains_key(l));
                if a.eq(b) {
                    found = Some(other);
                    break;
                }
            }
            match found {
                Some(o) => {
                    duplicates.insert(i, o);
                }
                None => {
                    same.add(i);
                    duplicates.insert(i, i);
                }
            }
        }
    }

    // prune_langsys
    let (langsys, kept) = prune_langsys(t, &duplicates, layout_scripts);

    // remap_feature_indices
    let mut features_map = BTreeMap::new();
    let mut with_dups = BTreeMap::new();
    let mut next = 0u32;
    for f in kept.iter() {
        let target = duplicates.get(&f).copied().unwrap_or(INVALID);
        match features_map.get(&target) {
            Some(&new) => {
                with_dups.insert(f, new);
            }
            None => {
                features_map.insert(f, next);
                with_dups.insert(f, next);
                next = next.wrapping_add(1);
            }
        }
    }
    LayoutPlan {
        fixed_conditions: pinned.is_some(),
        universal_record: None,
        lookups,
        features: features_map,
        features_w_dups: with_dups,
        langsys,
        substitutes,
        unsupported,
    }
}

/// `_features_have_equal_params`.
fn equal_params(a: D<'_>, b: D<'_>, tag: u32) -> bool {
    let pa = feature_params(a, tag);
    let pb = feature_params(b, tag);
    if pa.is_null() != pb.is_null() {
        return false;
    }
    if pa.is_null() {
        return true;
    }
    if tag == u32::from_be_bytes(*b"size") {
        return pa.slice(0, 10) == pb.slice(0, 10);
    }
    if tag & 0xFFFF_0000 == u32::from_be_bytes(*b"ss\0\0") {
        return pa.slice(0, 4) == pb.slice(0, 4);
    }
    if tag & 0xFFFF_0000 == u32::from_be_bytes(*b"cv\0\0") {
        let size = |p: D<'_>| 14usize.saturating_add(usize::from(p.u16(12)).saturating_mul(3));
        return size(pa) == size(pb) && pa.slice(0, size(pa)) == pb.slice(0, size(pb));
    }
    true
}

/// `LangSys::compare` through the duplicate map.
fn langsys_equal(a: D<'_>, b: D<'_>, map: &BTreeMap<u32, u32>) -> bool {
    if required(a) != required(b) {
        return false;
    }
    let it = |l: D<'_>| {
        (0..usize::from(l.u16(4)))
            .filter_map(move |i| map.get(&u32::from(l.u16_at(6, i))).copied())
            .collect::<Vec<u32>>()
    };
    it(a) == it(b)
}

/// `LangSys::collect_features` of `prune_langsys`.
fn langsys_features(l: D<'_>, map: &BTreeMap<u32, u32>, out: &mut BitSet) {
    let req = required(l);
    let n = usize::from(l.u16(4));
    if req == 0xFFFF && n == 0 {
        return;
    }
    if req != 0xFFFF && map.contains_key(&u32::from(req)) {
        out.add(u32::from(req));
    }
    (0..n)
        .map(|i| u32::from(l.u16_at(6, i)))
        .filter(|f| map.contains_key(f))
        .for_each(|f| out.add(f));
}

/// `GSUBGPOS::prune_langsys`: the kept language systems of each script
/// and the features they use.
fn prune_langsys(
    t: Gsubgpos<'_>,
    duplicates: &BTreeMap<u32, u32>,
    layout_scripts: &RangeSet,
) -> (BTreeMap<u32, BitSet>, BitSet) {
    let mut map: BTreeMap<u32, BitSet> = BTreeMap::new();
    let mut features = BitSet::new();
    let mut script_count = 0u32;
    let mut feature_count = 0u32;
    let mut visit_langsys = |n: u16| {
        feature_count = feature_count.saturating_add(u32::from(n));
        feature_count < MAX_LANGSYS_FEATURE_COUNT
    };
    for si in 0..t.script_count() {
        if !layout_scripts.contains(t.script_tag(si)) {
            continue;
        }
        let s = t.script(si);
        let default = s.off16(0);
        let n = usize::from(s.u16(2));
        if default.is_null() && n == 0 {
            continue;
        }
        let visit = script_count < MAX_SCRIPTS;
        script_count = script_count.saturating_add(1);
        if !visit {
            continue;
        }
        let entry = map.entry(n32(si)).or_default();
        let lang = |i: usize| s.off16(4usize.saturating_add(i.saturating_mul(6)).saturating_add(4));
        if !default.is_null() {
            if visit_langsys(default.u16(4)) {
                langsys_features(default, duplicates, &mut features);
            }
            for i in 0..n {
                let l = lang(i);
                if !visit_langsys(l.u16(4)) {
                    continue;
                }
                if langsys_equal(l, default, duplicates) {
                    continue;
                }
                langsys_features(l, duplicates, &mut features);
                entry.add(n32(i));
            }
        } else {
            for i in 0..n {
                let l = lang(i);
                if !visit_langsys(l.u16(4)) {
                    continue;
                }
                langsys_features(l, duplicates, &mut features);
                entry.add(n32(i));
            }
        }
    }
    (map, features)
}

/// The name ids of the feature parameters of the kept features
/// (`GSUBGPOS::collect_name_ids`).
pub(crate) fn collect_name_ids(
    t: Gsubgpos<'_>,
    lp: &LayoutPlan,
    pinned: bool,
    names: &mut RangeSet,
) {
    let features = &lp.features;
    let add = |f: D<'_>, tag: u32, names: &mut RangeSet| {
        let p = feature_params(f, tag);
        if p.is_null() {
            return;
        }
        if tag == u32::from_be_bytes(*b"size") {
            names.add(u32::from(p.u16(4)));
        } else if tag & 0xFFFF_0000 == u32::from_be_bytes(*b"ss\0\0") {
            names.add(u32::from(p.u16(2)));
        } else if tag & 0xFFFF_0000 == u32::from_be_bytes(*b"cv\0\0") {
            [p.u16(2), p.u16(4), p.u16(6)]
                .into_iter()
                .filter(|&n| n != 0)
                .for_each(|n| names.add(u32::from(n)));
            let first = p.u16(10);
            let count = p.u16(8);
            if first == 0 || count == 0 || count >= 0x7FFF {
                return;
            }
            let last = u32::from(first)
                .wrapping_add(u32::from(count))
                .wrapping_sub(1);
            names.add_range(u32::from(first), last);
        }
    };
    for i in 0..t.feature_count() {
        if !features.contains_key(&n32(i)) {
            continue;
        }
        let f = lp
            .substitutes
            .get(&n32(i))
            .map_or_else(|| t.feature(i), |&off| t.d.at(off));
        add(f, t.feature_tag(i), names);
    }
    if pinned {
        return;
    }
    for record in substitutions(t.feature_variations_view()) {
        for (fi, f) in record {
            if features.contains_key(&fi) {
                add(f, t.feature_tag(fi as usize), names);
            }
        }
    }
}
