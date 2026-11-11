//! The subsetting of the structures shared by GSUB and GPOS
//! (`hb-ot-layout-common.hh`, `hb-ot-layout-gsubgpos.hh`): Coverage and
//! ClassDef, Lookup and LookupList, Feature and FeatureList, Script,
//! LangSys and ScriptList, FeatureVariations and the table header
//! (`GSUBGPOS::subset`).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use sofel_font::read::feature_variations::{
    ConditionSet, FeatureTableSubstitution, FeatureVariations,
};
use sofel_write::{Errors, Serializer, Whence};

use super::data::{Cls, Cov, D, lo16, n16, n32};
use super::limits::{MAX_FEATURE_INDICES, MAX_LANGSYS, MAX_LOOKUP_VISIT_COUNT, MAX_SCRIPTS};
use super::{
    Gsubgpos, Kind, LayoutPlan, extension_type, feature_params, feature_params_size, kind,
};
use super::{context, gpos, gsub};
use crate::plan::Plan;
use crate::set::INVALID;

/// The subsetting context (`hb_subset_context_t` with
/// `hb_subset_layout_context_t`).
pub(crate) struct Sub<'p, 's> {
    pub p: &'p Plan,
    pub s: &'s mut Serializer,
    /// Whether the table is GSUB (otherwise GPOS, or a table that uses
    /// only the glyph helpers).
    pub gsub: bool,
    script_count: u32,
    langsys_count: u32,
    feature_index_count: u32,
    lookup_index_count: u32,
    cur_script_index: u32,
    /// What this version cannot write, found while subsetting.
    pub unsupported: Option<&'static str>,
}

impl<'p, 's> Sub<'p, 's> {
    /// A context for `plan` writing to `s`.
    pub fn new(p: &'p Plan, s: &'s mut Serializer, gsub: bool) -> Sub<'p, 's> {
        Sub {
            p,
            s,
            gsub,
            script_count: 0,
            langsys_count: 0,
            feature_index_count: 0,
            lookup_index_count: 0,
            cur_script_index: 0xFFFF,
            unsupported: None,
        }
    }

    /// The plan of the table being written.
    pub fn lp(&self) -> &'p LayoutPlan {
        if self.gsub {
            &self.p.layout.gsub
        } else {
            &self.p.layout.gpos
        }
    }

    /// Old lookup index to new.
    pub fn lookup_map(&self) -> &'p BTreeMap<u32, u32> {
        &self.lp().lookups
    }

    /// Whether `g` is in the GSUB glyph set (`glyphset_gsub`).
    pub fn in_gsub(&self, g: u32) -> bool {
        self.p.layout.glyphset_gsub.has(g)
    }

    /// The new id of a glyph (`glyph_map`), [`INVALID`] when not kept.
    pub fn map(&self, g: u32) -> u32 {
        self.p.glyphs.new_gid(g).unwrap_or(INVALID)
    }

    /// The new id of a glyph of the GSUB glyph set (`map_gsub_glyph`).
    pub fn map_gsub(&self, g: u32) -> u32 {
        self.p
            .layout
            .gsub_map_flat
            .get(g as usize)
            .copied()
            .unwrap_or(INVALID)
    }

    /// Notes something this version cannot write.
    pub fn unsupported(&mut self, reason: &'static str) {
        if self.unsupported.is_none() {
            self.unsupported = Some(reason);
        }
        self.s.err(Errors::OTHER);
    }

    /// `OffsetTo::serialize_subset` (and `serialize_serialize`) for an
    /// offset field of `width` bytes at `at` of the current object: the
    /// child written by `f` in a pushed object, linked when `f` succeeds.
    pub fn serialize_subset(
        &mut self,
        at: usize,
        width: u8,
        f: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        self.s.push();
        let ok = f(self);
        if ok {
            let o = self.s.pop_pack(true);
            self.s.add_link(at, width, false, o, Whence::Head, 0);
        } else {
            self.s.pop_discard();
        }
        ok
    }

    /// [`Self::serialize_subset`] of a source offset that may be null.
    pub fn serialize_subset_opt(
        &mut self,
        at: usize,
        width: u8,
        src: D<'_>,
        f: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        if src.is_null() {
            return false;
        }
        self.serialize_subset(at, width, f)
    }

    /// [`Self::serialize_subset`] without the null check.
    pub fn serialize_serialize(
        &mut self,
        at: usize,
        width: u8,
        f: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        self.serialize_subset(at, width, f)
    }

    /// `OffsetTo::serialize_copy`: the child is linked whatever `f`
    /// returns (an empty child is the null offset).
    pub fn serialize_copy(
        &mut self,
        at: usize,
        width: u8,
        src: D<'_>,
        f: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        if src.is_null() {
            return false;
        }
        self.s.push();
        let ok = f(self);
        let o = self.s.pop_pack(true);
        self.s.add_link(at, width, false, o, Whence::Head, 0);
        ok
    }

    /// `subset_offset_array`: appends an offset of `width` bytes and
    /// writes its child; on failure the offset is removed again.
    pub fn offset_append(&mut self, width: u8, f: impl FnOnce(&mut Self) -> bool) -> bool {
        let snap = self.s.snapshot();
        let at = self.s.zeros(usize::from(width));
        let ok = self.serialize_subset(at, width, f);
        if !ok {
            self.s.revert(snap);
        }
        ok
    }

    /// Adds `delta` to the big-endian `u16` at `at` of the current object.
    pub fn bump_u16(&mut self, at: usize, delta: u16) {
        let cur = D(self.s.current()).u16(at);
        self.s.set_u16(at, cur.wrapping_add(delta));
    }

    // -----------------------------------------------------------------
    // Coverage and ClassDef.

    /// `Coverage::serialize` into the current object.
    pub fn coverage(&mut self, glyphs: &[u32]) -> bool {
        let count = glyphs.len();
        let mut num_ranges = 0usize;
        let mut last = INVALID.wrapping_sub(1);
        let mut max = 0u32;
        let mut unsorted = false;
        for &g in glyphs {
            if last != INVALID.wrapping_sub(1) && g < last {
                unsorted = true;
            }
            if last.wrapping_add(1) != g {
                num_ranges = num_ranges.wrapping_add(1);
            }
            last = g;
            max = max.max(g);
        }
        if max > 0xFFFF {
            self.unsupported("24-bit Coverage tables (ADR 0022 stage 3)");
            return false;
        }
        if !unsorted && count <= num_ranges.saturating_mul(3) {
            self.s.u16(1);
            let at = self.s.u16(0);
            if !self
                .s
                .check_assign_u16(at, count as u64, Errors::ARRAY_OVERFLOW)
            {
                return false;
            }
            glyphs.iter().for_each(|&g| {
                self.s.u16(lo16(g));
            });
            return true;
        }
        self.s.u16(2);
        let at = self.s.u16(0);
        if !self
            .s
            .check_assign_u16(at, num_ranges as u64, Errors::ARRAY_OVERFLOW)
        {
            return false;
        }
        let mut ranges: Vec<(u32, u32, u32)> = Vec::with_capacity(num_ranges);
        let mut last = INVALID.wrapping_sub(1);
        let mut unsorted = false;
        for (count, &g) in glyphs.iter().enumerate() {
            if last.wrapping_add(1) != g {
                if last != INVALID.wrapping_sub(1) && last.wrapping_add(1) > g {
                    unsorted = true;
                }
                ranges.push((g, g, n32(count)));
            }
            if let Some(r) = ranges.last_mut() {
                r.1 = g;
            }
            last = g;
        }
        if unsorted {
            ranges.sort_unstable();
        }
        for (a, z, v) in ranges {
            self.s.u16(lo16(a));
            self.s.u16(lo16(z));
            self.s.u16((v & 0xFFFF) as u16);
        }
        true
    }

    /// `Coverage::subset` into the current object: the covered glyphs of
    /// the GSUB glyph set, renumbered; false when none is left.
    pub fn coverage_subset(&mut self, cov: Cov<'_>) -> bool {
        let num_glyphs = self.p.source_glyphs;
        let glyphs: Vec<u32> = cov
            .glyphs()
            .into_iter()
            .take(num_glyphs as usize)
            .map(|g| self.map_gsub(g))
            .filter(|&g| g != INVALID)
            .collect();
        self.coverage(&glyphs);
        !glyphs.is_empty()
    }

    /// `ClassDef::serialize` of `(glyph, class)` pairs (class 0 left out)
    /// into the current object.
    pub fn class_def(&mut self, pairs: &[(u32, u32)]) -> bool {
        let glyphs: Vec<(u32, u32)> = pairs.iter().copied().filter(|p| p.1 != 0).collect();
        let mut format = 2u16;
        if let Some(&(glyph_min, first_class)) = glyphs.first() {
            let mut glyph_max = glyph_min;
            let mut class_max = 0u32;
            let mut num_ranges = 1u64;
            let mut prev_gid = glyph_min;
            let mut prev_klass = first_class;
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
            let span = u64::from(glyph_max.wrapping_sub(glyph_min)).wrapping_add(1);
            let mut best = u64::MAX;
            if glyph_max <= 0xFFFF && span <= 0xFFFF && class_max <= 0xFFFF {
                let size = 6u64.saturating_add(span.saturating_mul(2));
                if size < best {
                    best = size;
                    format = 1;
                }
            }
            if glyph_max <= 0xFFFF && num_ranges <= 0xFFFF && class_max <= 0xFFFF {
                let size = 4u64.saturating_add(num_ranges.saturating_mul(6));
                if size < best {
                    best = size;
                    format = 2;
                }
            }
            if best == u64::MAX {
                self.unsupported("24-bit ClassDef tables (ADR 0022 stage 3)");
                return false;
            }
        }
        if format == 1 {
            self.s.u16(1);
            let Some(&(glyph_min, _)) = glyphs.first() else {
                self.s.u16(0);
                self.s.u16(0);
                return true;
            };
            let glyph_max = glyphs.iter().map(|p| p.0).fold(0, u32::max);
            let count = glyph_max.wrapping_sub(glyph_min).wrapping_add(1) as usize;
            self.s.u16(lo16(glyph_min));
            self.s.u16(n16(count));
            let at = self.s.zeros(count.saturating_mul(2));
            for &(g, k) in &glyphs {
                let idx = g.wrapping_sub(glyph_min) as usize;
                if idx < count {
                    self.s
                        .set_u16(at.saturating_add(idx.saturating_mul(2)), lo16(k));
                }
            }
            return true;
        }
        self.s.u16(2);
        let count_at = self.s.u16(0);
        let Some(&(first, first_class)) = glyphs.first() else {
            return true;
        };
        let mut ranges: Vec<(u32, u32, u32)> = alloc::vec![(first, first, first_class)];
        let mut prev_gid = first;
        let mut prev_klass = first_class;
        let mut unsorted = false;
        for &(g, k) in glyphs.iter().skip(1) {
            if g != prev_gid.wrapping_add(1) || k != prev_klass {
                if g < prev_gid {
                    unsorted = true;
                }
                if let Some(r) = ranges.last_mut() {
                    r.1 = prev_gid;
                }
                ranges.push((g, g, k));
            }
            prev_klass = k;
            prev_gid = g;
        }
        if let Some(r) = ranges.last_mut() {
            r.1 = prev_gid;
        }
        self.s.set_u16(count_at, n16(ranges.len()));
        if unsorted {
            ranges.sort_unstable();
        }
        for (a, z, v) in ranges {
            self.s.u16(lo16(a));
            self.s.u16(lo16(z));
            self.s.u16(lo16(v));
        }
        true
    }

    /// `ClassDef::subset` into the current object. With `klass_map`, the
    /// classes are renumbered (`ClassDef_remap_and_serialize`).
    pub fn class_def_subset(
        &mut self,
        cd: Cls<'_>,
        klass_map: Option<&mut BTreeMap<u32, u32>>,
        keep_empty: bool,
        use_class_zero: bool,
        filter: Option<Cov<'_>>,
    ) -> bool {
        let gs = &self.p.layout.glyphset_gsub;
        let (mut pairs, klasses) = cd.subset_pairs(
            gs,
            self.p.source_glyphs,
            |g| {
                let n = self.map_gsub(g);
                (n != INVALID).then_some(n)
            },
            |g| filter.is_none_or(|f| f.has(g)),
        );
        let glyph_count = match filter {
            Some(f) => gs.iter().filter(|&g| f.has(g)).count() as u64,
            None => gs.len(),
        };
        let use_class_zero = use_class_zero && glyph_count <= pairs.len() as u64;
        if let Some(map) = klass_map {
            if !use_class_zero {
                map.insert(0, 0);
            }
            let mut idx: u32 = if map.contains_key(&0) { 1 } else { 0 };
            for k in klasses.iter() {
                if map.contains_key(&k) {
                    continue;
                }
                map.insert(k, idx);
                idx = idx.wrapping_add(1);
            }
            pairs
                .iter_mut()
                .for_each(|p| p.1 = map.get(&p.1).copied().unwrap_or(INVALID));
        }
        if !self.class_def(&pairs) {
            return false;
        }
        keep_empty || !pairs.is_empty()
    }

    // -----------------------------------------------------------------
    // Lookups.

    /// `Lookup::subset` of lookup `lookup` into the current object.
    fn lookup(&mut self, lookup: D<'_>) -> bool {
        let ty = lookup.u16(0);
        let flag = lookup.u16(2);
        self.s.u16(ty);
        let flag_at = self.s.u16(flag);
        let count_at = self.s.u16(0);
        let n = usize::from(lookup.u16(4));
        let gs = &self.p.layout.glyphset_gsub;
        for i in 0..n {
            let st = lookup.off16(6usize.saturating_add(i.saturating_mul(2)));
            let Some((k, inner)) = super::resolve(ty, st, self.gsub) else {
                continue;
            };
            if !super::closure::subtable_intersects(k, inner, gs) {
                continue;
            }
            if self.offset_append(2, |s| s.subtable(ty, st)) {
                self.bump_u16(count_at, 1);
            }
        }
        if flag & 0x0010 != 0 {
            let mfs = lookup.u16(6usize.saturating_add(n.saturating_mul(2)));
            match self.p.layout.used_mark_sets.get(&u32::from(mfs)) {
                None => {
                    let new_flag = (flag & !0x0010) | 0x0008;
                    self.s.set_u16(flag_at, new_flag);
                }
                Some(&idx) => {
                    self.s.u16((idx & 0xFFFF) as u16);
                }
            }
        }
        true
    }

    /// A subtable of lookup type `ty` (the subtable dispatch of
    /// `subset`, extensions rewritten as extensions).
    fn subtable(&mut self, ty: u16, st: D<'_>) -> bool {
        if ty == extension_type(self.gsub) {
            if st.u16(0) != 1 {
                return false;
            }
            let inner_ty = st.u16(2);
            self.s.u16(1);
            self.s.u16(inner_ty);
            let at = self.s.u32(0);
            let inner = st.off32(4);
            return self.serialize_subset_opt(at, 4, inner, |s| s.subtable(inner_ty, inner));
        }
        match kind(self.gsub, ty) {
            Kind::SingleSubst => gsub::single(self, st),
            Kind::MultipleSubst => gsub::multiple_or_alternate(self, st, true),
            Kind::AlternateSubst => gsub::multiple_or_alternate(self, st, false),
            Kind::LigatureSubst => gsub::ligature(self, st),
            Kind::ReverseChain => gsub::reverse(self, st),
            Kind::Context => context::context_subset(self, st),
            Kind::ChainContext => context::chain_subset(self, st),
            Kind::SinglePos => gpos::single(self, st),
            Kind::PairPos => gpos::pair(self, st),
            Kind::CursivePos => gpos::cursive(self, st),
            Kind::MarkBasePos => gpos::mark_base(self, st),
            Kind::MarkLigPos => gpos::mark_lig(self, st),
            Kind::MarkMarkPos => gpos::mark_mark(self, st),
            Kind::Unknown => false,
        }
    }

    /// `LookupOffsetList::subset`.
    fn lookup_list(&mut self, list: D<'_>) -> bool {
        let count_at = self.s.u16(0);
        let n = usize::from(list.u16(0));
        let lookups = self.lookup_map();
        for i in 0..n {
            if !lookups.contains_key(&n32(i)) {
                continue;
            }
            let l = list.off16(2usize.saturating_add(i.saturating_mul(2)));
            if l.is_null() {
                continue;
            }
            if self.offset_append(2, |s| s.lookup(l)) {
                self.bump_u16(count_at, 1);
            }
        }
        true
    }

    // -----------------------------------------------------------------
    // Features.

    fn visit_lookup_index(&mut self) -> bool {
        self.lookup_index_count = self.lookup_index_count.saturating_add(1);
        self.lookup_index_count < MAX_LOOKUP_VISIT_COUNT
    }

    /// `IndexArray::serialize`: the indices are appended to the count
    /// field at `count_at`.
    fn index_array(&mut self, count_at: usize, it: impl Iterator<Item = u32>) {
        let mut n = 0u16;
        for v in it {
            if !self.visit_lookup_index() {
                break;
            }
            self.s.u16((v & 0xFFFF) as u16);
            n = n.wrapping_add(1);
        }
        self.s.set_u16(count_at, n);
    }

    /// `Feature::subset` into the current object.
    fn feature(&mut self, f: D<'_>, tag: Option<u32>) -> bool {
        let params_at = self.s.u16(0);
        let count_at = self.s.u16(0);
        if let Some(tag) = tag {
            let params = feature_params(f, tag);
            if let Some(size) = feature_params_size(params, tag)
                && !params.is_null()
            {
                self.serialize_subset(params_at, 2, |s| {
                    s.s.extend(params.slice(0, size));
                    params.0.len() >= size
                });
            }
        }
        let lookups = self.lookup_map();
        let it: Vec<u32> = (0..usize::from(f.u16(2)))
            .filter_map(|i| lookups.get(&u32::from(f.u16_at(4, i))).copied())
            .collect();
        self.index_array(count_at, it.into_iter());
        true
    }

    /// `RecordListOfFeature::subset`.
    fn feature_list(&mut self, t: Gsubgpos<'_>) -> bool {
        let count_at = self.s.u16(0);
        let features = &self.lp().features;
        for i in 0..t.feature_count() {
            if !features.contains_key(&n32(i)) {
                continue;
            }
            let snap = self.s.snapshot();
            let tag = t.feature_tag(i);
            self.s.u32(tag);
            let at = self.s.u16(0);
            // A substitute from the applied FeatureVariations record is
            // written in place of the feature (`Record::subset` with
            // `f_sub`).
            let ok = match self.lp().substitutes.get(&n32(i)) {
                Some(&off) => {
                    let f = t.d.at(off);
                    self.serialize_subset(at, 2, |s| s.feature(f, Some(tag)))
                }
                None => {
                    let f = t.feature(i);
                    self.serialize_subset_opt(at, 2, f, |s| s.feature(f, Some(tag)))
                }
            };
            if ok {
                self.bump_u16(count_at, 1);
            } else {
                self.s.revert(snap);
            }
        }
        true
    }

    // -----------------------------------------------------------------
    // Scripts.

    /// `LangSys::subset` into the current object.
    fn lang_sys(&mut self, l: D<'_>) -> bool {
        let map = &self.lp().features_w_dups;
        self.s.u16(0);
        let req = l.u16(2);
        let new_req = map
            .get(&u32::from(req))
            .map_or(0xFFFF, |&v| (v & 0xFFFF) as u16);
        self.s.u16(new_req);
        let count_at = self.s.u16(0);
        let n = usize::from(l.u16(4));
        self.feature_index_count = self.feature_index_count.saturating_add(n32(n));
        if self.feature_index_count >= MAX_FEATURE_INDICES {
            return false;
        }
        let it: Vec<u32> = (0..n)
            .filter_map(|i| map.get(&u32::from(l.u16_at(6, i))).copied())
            .collect();
        let ret = !it.is_empty();
        self.index_array(count_at, it.into_iter());
        ret
    }

    /// `Script::subset` into the current object.
    fn script(&mut self, sc: D<'_>, tag: u32) -> bool {
        let visit = self.script_count < MAX_SCRIPTS;
        self.script_count = self.script_count.saturating_add(1);
        if !visit {
            return false;
        }
        if !self.p.layout.layout_scripts.contains(tag) {
            return false;
        }
        let default_at = self.s.u16(0);
        let count_at = self.s.u16(0);
        let mut default_lang = false;
        let default = sc.off16(0);
        if !default.is_null() {
            self.s.push();
            let ret = self.lang_sys(default);
            if !ret && tag != u32::from_be_bytes(*b"DFLT") {
                self.s.pop_discard();
            } else {
                let o = self.s.pop_pack(true);
                self.s.add_link(default_at, 2, false, o, Whence::Head, 0);
                default_lang = true;
            }
        }
        let active = self.lp().langsys.get(&self.cur_script_index);
        let mut n = 0u16;
        if let Some(active) = active {
            for i in 0..usize::from(sc.u16(2)) {
                if !active.has(n32(i)) {
                    continue;
                }
                let visit = self.langsys_count < MAX_LANGSYS;
                self.langsys_count = self.langsys_count.saturating_add(1);
                if !visit {
                    continue;
                }
                let rec = 4usize.saturating_add(i.saturating_mul(6));
                let snap = self.s.snapshot();
                self.s.u32(sc.u32(rec));
                let at = self.s.u16(0);
                let l = sc.off16(rec.saturating_add(4));
                if self.serialize_subset_opt(at, 2, l, |s| s.lang_sys(l)) {
                    n = n.wrapping_add(1);
                } else {
                    self.s.revert(snap);
                }
            }
        }
        self.s.set_u16(count_at, n);
        n != 0 || default_lang || self.gsub
    }

    /// `RecordListOfScript::subset`.
    fn script_list(&mut self, t: Gsubgpos<'_>) -> bool {
        let count_at = self.s.u16(0);
        for i in 0..t.script_count() {
            let snap = self.s.snapshot();
            self.cur_script_index = n32(i);
            let tag = t.script_tag(i);
            self.s.u32(tag);
            let at = self.s.u16(0);
            let sc = t.script(i);
            if self.serialize_subset_opt(at, 2, sc, |s| s.script(sc, tag)) {
                self.bump_u16(count_at, 1);
            } else {
                self.s.revert(snap);
            }
        }
        true
    }

    // -----------------------------------------------------------------
    // FeatureVariations (version 1.0, without instancing).

    fn feature_variations(&mut self, t: Gsubgpos<'_>, fv: FeatureVariations<'_>) -> bool {
        if fv.version() != 0x0001_0000 {
            self.unsupported("FeatureVariations 1.1 (ADR 0022 stage 3)");
            return false;
        }
        self.s.u32(fv.version());
        let count_at = self.s.u32(0);
        let map = &self.lp().features_w_dups;
        let keep = (0..fv.len()).rev().find(|&i| {
            let subs = fv.record(i).and_then(|r| r.substitutions().ok().flatten());
            subs.is_some_and(|s| {
                (0..s.len()).any(|j| {
                    s.record(j)
                        .is_some_and(|r| map.contains_key(&u32::from(r.feature_index())))
                })
            })
        });
        let count = keep.map_or(0, |k| k.saturating_add(1));
        let mut out = 0u32;
        for i in 0..count {
            let cond_at = self.s.u32(0);
            let subs_at = self.s.u32(0);
            let Some(record) = fv.record(i) else { continue };
            if let Some(conds) = record.conditions().ok().flatten() {
                self.serialize_subset_opt(cond_at, 4, D(conds.data()), |s| s.condition_set(conds));
            }
            if let Some(subs) = record.substitutions().ok().flatten() {
                self.serialize_subset_opt(subs_at, 4, D(subs.data()), |s| {
                    s.feature_substitution(t, subs)
                });
            }
            out = out.wrapping_add(1);
        }
        self.s.set_u32(count_at, out);
        out != 0
    }

    fn condition_set(&mut self, cs: ConditionSet<'_>) -> bool {
        let count_at = self.s.u16(0);
        for i in 0..cs.len() {
            let Some(c) = cs.condition(i).ok().flatten() else {
                self.unsupported("FeatureVariations conditions other than format 1");
                return false;
            };
            if c.format() != 1 {
                self.unsupported("FeatureVariations conditions other than format 1");
                return false;
            }
            if self.offset_append(4, |s| {
                s.s.extend(D(c.data()).slice(0, 8));
                c.data().len() >= 8
            }) {
                self.bump_u16(count_at, 1);
            }
        }
        D(self.s.current()).u16(count_at) != 0
    }

    fn feature_substitution(&mut self, t: Gsubgpos<'_>, fts: FeatureTableSubstitution<'_>) -> bool {
        self.s.u32(fts.version());
        let count_at = self.s.u16(0);
        let map = &self.lp().features_w_dups;
        for j in 0..fts.len() {
            let Some(record) = fts.record(j) else {
                continue;
            };
            let index = u32::from(record.feature_index());
            let Some(&new) = map.get(&index) else {
                continue;
            };
            let snap = self.s.snapshot();
            self.s.u16((new & 0xFFFF) as u16);
            let at = self.s.u32(0);
            let tag = t.feature_tag(index as usize);
            let f = D(record.feature().ok().flatten().unwrap_or(&[]));
            if self.serialize_subset_opt(at, 4, f, |s| s.feature(f, Some(tag))) {
                self.bump_u16(count_at, 1);
            } else {
                self.s.revert(snap);
            }
        }
        D(self.s.current()).u16(count_at) != 0
    }

    // -----------------------------------------------------------------
    // The table.

    /// `GSUBGPOS::subset` of `t` into the root object.
    pub fn gsubgpos(&mut self, t: Gsubgpos<'_>) -> bool {
        let version = t.version();
        if version >> 16 != 1 {
            return false;
        }
        if version >= 0x0001_0002 {
            self.unsupported("GSUB and GPOS 1.2 (ADR 0022 stage 3)");
            return false;
        }
        self.s.u32(version);
        let script_at = self.s.u16(0);
        let feature_at = self.s.u16(0);
        let lookup_at = self.s.u16(0);
        let ll = t.lookup_list();
        self.serialize_subset_opt(lookup_at, 2, ll, |s| s.lookup_list(ll));
        let fl = t.feature_list();
        self.serialize_subset_opt(feature_at, 2, fl, |s| s.feature_list(t));
        let sl = t.script_list();
        self.serialize_subset_opt(script_at, 2, sl, |s| s.script_list(t));
        if version >= 0x0001_0001 {
            let snap = self.s.snapshot();
            let at = self.s.u32(0);
            let fv = t.feature_variations_view();
            // Pinning every axis drops the feature variations.
            let ret = !self.p.var.all_axes_pinned()
                && fv.is_some_and(|fv| {
                    self.serialize_subset_opt(at, 4, D(fv.data()), |s| s.feature_variations(t, fv))
                });
            if !ret {
                self.s.revert(snap);
                self.s.set_u32(0, 0x0001_0000);
            }
        }
        true
    }
}
