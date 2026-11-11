//! Applying lookups (ADR 0020, GSUBとGPOSの適用): the apply context, the
//! skipping iterator, input, backtrack and lookahead matching,
//! ligation, nested lookups and the sequence context subtables shared
//! by `GSUB` and `GPOS`.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-layout-gsubgpos.hh` and
//! `src/hb-ot-layout.cc` (`apply_forward`, `apply_backward`,
//! `apply_string`, `hb_ot_map_t::apply`), and HarfRust 0.15.0
//! `src/ot/apply.rs` and `src/ot/lookup/context.rs`, MIT; see
//! THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::read::GlyphId;
use sofel_font::read::layout::{
    ChainRule, ChainRuleSets, ChainedSequenceContext, ClassDef, CoverageArray, GlyphArray,
    LookupRecords, Rule, RuleSets, SequenceContext, lookup_flag,
};

use super::{GPOS, GSUB};
use crate::buffer::{Buffer, Info, Mask, gprops};
use crate::direction::Direction;
use crate::font::ShapeFont;
use crate::plan::ShapePlan;
use sofel_unicode::GeneralCategory;

/// `HB_MAX_NESTING_LEVEL`.
pub(crate) const MAX_NESTING_LEVEL: u32 = 64;
/// `HB_MAX_CONTEXT_LENGTH`.
pub(crate) const MAX_CONTEXT_LENGTH: usize = 64;

/// `LookupFlag::IgnoreFlags`.
const IGNORE_FLAGS: u32 = 0x000E;

/// A match function with its data (`match_func_t` and `match_data`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum MatchFn<'a> {
    /// `match_always`.
    Always,
    /// `match_glyph` against glyph ids.
    Glyph(GlyphArray<'a>),
    /// `match_class` against class values.
    Class(GlyphArray<'a>, ClassDef<'a>),
    /// `match_coverage` against the coverages from an index on.
    Coverage(CoverageArray<'a>, usize),
}

impl MatchFn<'_> {
    /// Whether `info` matches the value at `pos`.
    pub(super) fn matches(&self, info: &Info, pos: usize) -> bool {
        match self {
            MatchFn::Always => true,
            MatchFn::Glyph(a) => a.get(pos) == Some(info.codepoint),
            MatchFn::Class(a, c) => a.get(pos) == Some(c.get(GlyphId(info.codepoint))),
            MatchFn::Coverage(a, start) => {
                matches!(a.get(start.wrapping_add(pos)), Ok(Some(c)) if c.get(GlyphId(info.codepoint)).is_some())
            }
        }
    }
}

/// `may_skip_t`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Skip {
    No,
    Yes,
    Maybe,
}

/// `match_t` of the iterator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Match {
    Match,
    NotMatch,
    Skip,
}

/// The skipping iterator (`skipping_iterator_t` with its `matcher_t`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Iter<'a> {
    pub(crate) idx: usize,
    end: usize,
    pub(crate) lookup_props: u32,
    mask: Mask,
    ignore_zwnj: bool,
    ignore_zwj: bool,
    ignore_hidden: bool,
    per_syllable: bool,
    syllable: u8,
    matcher: Option<MatchFn<'a>>,
    data_pos: usize,
}

impl<'a> Iter<'a> {
    fn empty() -> Iter<'a> {
        Iter {
            idx: 0,
            end: 0,
            lookup_props: 0,
            mask: u32::MAX,
            ignore_zwnj: false,
            ignore_zwj: false,
            ignore_hidden: false,
            per_syllable: false,
            syllable: 0,
            matcher: None,
            data_pos: 0,
        }
    }
    /// `set_match_func` with `set_glyph_data`.
    pub(crate) fn set_match(&mut self, m: Option<MatchFn<'a>>) {
        self.matcher = m;
        self.data_pos = 0;
    }
    /// `set_lookup_props`.
    pub(crate) fn set_lookup_props(&mut self, props: u32) {
        self.lookup_props = props;
    }
}

/// Which iterator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Which {
    /// `iter_input`.
    Input,
    /// `iter_context`.
    Context,
}

/// The apply context (`hb_ot_apply_context_t`).
pub(crate) struct ApplyContext<'p, 'a, 'b, 'f, 'g, 'l, 's> {
    pub(crate) table: usize,
    pub(crate) plan: &'p ShapePlan<'a>,
    pub(crate) font: &'f ShapeFont<'f, 'g, 'l, 's>,
    pub(crate) buffer: &'b mut Buffer,
    pub(crate) direction: Direction,
    pub(crate) lookup_mask: Mask,
    pub(crate) lookup_index: usize,
    pub(crate) lookup_props: u32,
    pub(crate) nesting_level_left: u32,
    pub(crate) has_glyph_classes: bool,
    pub(crate) auto_zwnj: bool,
    pub(crate) auto_zwj: bool,
    pub(crate) per_syllable: bool,
    pub(crate) random: bool,
    pub(crate) last_base: Option<usize>,
    pub(crate) last_base_until: usize,
    pub(crate) match_positions: Vec<usize>,
    pub(crate) iter_input: Iter<'a>,
    pub(crate) iter_context: Iter<'a>,
}

impl<'p, 'a, 'b, 'f, 'g, 'l, 's> ApplyContext<'p, 'a, 'b, 'f, 'g, 'l, 's> {
    pub(crate) fn new(
        table: usize,
        plan: &'p ShapePlan<'a>,
        font: &'f ShapeFont<'f, 'g, 'l, 's>,
        buffer: &'b mut Buffer,
    ) -> ApplyContext<'p, 'a, 'b, 'f, 'g, 'l, 's> {
        let direction = buffer.direction;
        let mut c = ApplyContext {
            table,
            plan,
            font,
            buffer,
            direction,
            lookup_mask: 1,
            lookup_index: usize::MAX,
            lookup_props: 0,
            nesting_level_left: MAX_NESTING_LEVEL,
            has_glyph_classes: plan.layout.gdef.has_glyph_classes(),
            auto_zwnj: true,
            auto_zwj: true,
            per_syllable: false,
            random: false,
            last_base: None,
            last_base_until: 0,
            match_positions: Vec::new(),
            iter_input: Iter::empty(),
            iter_context: Iter::empty(),
        };
        c.init_iters();
        c
    }

    /// `matcher_t::init` and `skipping_iterator_t::init`.
    fn make_iter(&self, context_match: bool) -> Iter<'a> {
        Iter {
            idx: 0,
            end: self.buffer.len,
            lookup_props: self.lookup_props,
            mask: if context_match {
                u32::MAX
            } else {
                self.lookup_mask
            },
            ignore_zwnj: self.table == GPOS || (context_match && self.auto_zwnj),
            ignore_zwj: context_match || self.auto_zwj,
            ignore_hidden: self.table == GPOS,
            per_syllable: self.table == GSUB && self.per_syllable,
            syllable: 0,
            matcher: None,
            data_pos: 0,
        }
    }

    /// `init_iters`.
    pub(crate) fn init_iters(&mut self) {
        self.iter_input = self.make_iter(false);
        self.iter_context = self.make_iter(true);
    }

    /// `set_lookup_mask` without re-initializing the iterators.
    pub(crate) fn set_lookup_mask(&mut self, mask: Mask) {
        self.lookup_mask = mask;
        self.last_base = None;
        self.last_base_until = 0;
    }

    /// `set_lookup_props`.
    pub(crate) fn set_lookup_props(&mut self, props: u32) {
        self.lookup_props = self.plan.layout.gdef.sanitize_lookup_props(props);
        self.init_iters();
    }

    pub(crate) fn iter(&mut self, which: Which) -> &mut Iter<'a> {
        match which {
            Which::Input => &mut self.iter_input,
            Which::Context => &mut self.iter_context,
        }
    }

    /// `random_number`: HarfBuzz's `minstd_rand`.
    pub(crate) fn random_number(&mut self) -> u32 {
        // `uint32_t * int` wraps at 32 bits before the remainder.
        let s = self
            .buffer
            .random_state
            .wrapping_mul(48271)
            .checked_rem(2_147_483_647)
            .unwrap_or(0);
        self.buffer.random_state = s;
        s
    }

    /// `match_properties_mark`.
    fn match_properties_mark(&self, info: &Info, glyph_props: u16, match_props: u32) -> bool {
        if match_props & u32::from(lookup_flag::USE_MARK_FILTERING_SET) != 0 {
            return self
                .plan
                .layout
                .gdef
                .mark_set_covers(match_props >> 16, info.codepoint);
        }
        let mat = u32::from(lookup_flag::MARK_ATTACHMENT_TYPE_MASK);
        if match_props & mat != 0 {
            return match_props & mat == u32::from(glyph_props) & mat;
        }
        true
    }

    /// `check_glyph_property`.
    pub(crate) fn check_glyph_property(&self, info: &Info, match_props: u32) -> bool {
        let glyph_props = info.glyph_props;
        if u32::from(glyph_props) & match_props & IGNORE_FLAGS != 0 {
            return false;
        }
        if glyph_props & gprops::MARK != 0 {
            return self.match_properties_mark(info, glyph_props, match_props);
        }
        true
    }

    /// `matcher_t::may_skip`.
    pub(crate) fn may_skip(&self, it: &Iter<'_>, info: &Info) -> Skip {
        if !self.check_glyph_property(info, it.lookup_props) {
            return Skip::Yes;
        }
        if info.is_default_ignorable()
            && (it.ignore_zwnj || !info.is_zwnj())
            && (it.ignore_zwj || !info.is_zwj())
            && (it.ignore_hidden || !info.is_hidden())
        {
            return Skip::Maybe;
        }
        Skip::No
    }

    /// `skipping_iterator_t::match`.
    fn iter_match(&self, it: &Iter<'_>, info: &Info) -> Match {
        let skip = self.may_skip(it, info);
        if skip == Skip::Yes {
            return Match::Skip;
        }
        // `may_match`.
        let m = if info.mask & it.mask == 0
            || (it.per_syllable && it.syllable != 0 && it.syllable != info.syllable)
        {
            Some(false)
        } else {
            it.matcher.map(|f| f.matches(info, it.data_pos))
        };
        match (m, skip) {
            (Some(true), _) | (None, Skip::No) => Match::Match,
            (_, Skip::No) => Match::NotMatch,
            _ => Match::Skip,
        }
    }

    /// The match of iterator `which` at `info` (`skippy_iter.match`).
    pub(crate) fn match_at(&self, which: Which, info: &Info) -> Match {
        let it = match which {
            Which::Input => &self.iter_input,
            Which::Context => &self.iter_context,
        };
        self.iter_match(it, info)
    }

    /// `may_skip` of iterator `which`.
    pub(crate) fn may_skip_at(&self, which: Which, info: &Info) -> Skip {
        let it = match which {
            Which::Input => &self.iter_input,
            Which::Context => &self.iter_context,
        };
        self.may_skip(it, info)
    }

    /// `reset`: for the forward iterator of `GSUB`.
    pub(crate) fn iter_reset(&mut self, which: Which, start: usize) {
        let len = self.buffer.len;
        let syllable = self.buffer.cur(0).syllable;
        let it = self.iter(which);
        it.idx = start;
        it.end = len;
        it.syllable = syllable;
    }

    /// `reset_back`.
    pub(crate) fn iter_reset_back(&mut self, which: Which, start: usize) {
        let syllable = self.buffer.cur(0).syllable;
        let it = self.iter(which);
        it.idx = start;
        it.syllable = syllable;
    }

    /// `reset_fast`.
    pub(crate) fn iter_reset_fast(&mut self, which: Which, start: usize) {
        self.iter(which).idx = start;
    }

    /// `next`: `Ok(())` when a glyph matched (its index is the iterator's
    /// `idx`), else `Err(unsafe_to)`.
    pub(crate) fn iter_next(&mut self, which: Which) -> Result<(), usize> {
        let mut it = *self.iter(which);
        let mut result = Err(it.end);
        while it.end > 0 && it.idx < it.end.wrapping_sub(1) {
            it.idx = it.idx.wrapping_add(1);
            match self.iter_match(&it, &self.buffer.info[it.idx]) {
                Match::Match => {
                    it.data_pos = it.data_pos.wrapping_add(1);
                    result = Ok(());
                    break;
                }
                Match::NotMatch => {
                    result = Err(it.idx.wrapping_add(1));
                    break;
                }
                Match::Skip => {}
            }
        }
        *self.iter(which) = it;
        result
    }

    /// `prev`: `Ok(())` when a glyph of the output matched, else
    /// `Err(unsafe_from)`.
    pub(crate) fn iter_prev(&mut self, which: Which) -> Result<(), usize> {
        let mut it = *self.iter(which);
        let mut result = Err(0);
        while it.idx > 0 {
            it.idx = it.idx.wrapping_sub(1);
            match self.iter_match(&it, &self.buffer.out_info()[it.idx]) {
                Match::Match => {
                    it.data_pos = it.data_pos.wrapping_add(1);
                    result = Ok(());
                    break;
                }
                Match::NotMatch => {
                    result = Err(it.idx.max(1).wrapping_sub(1));
                    break;
                }
                Match::Skip => {}
            }
        }
        *self.iter(which) = it;
        result
    }

    /// `_set_glyph_class`.
    fn set_glyph_class(&mut self, glyph: u32, class_guess: u16, ligature: bool, component: bool) {
        self.buffer.digest.add(glyph);
        let has_classes = self.has_glyph_classes;
        let gdef_props = if has_classes {
            self.plan.layout.gdef.glyph_props(glyph)
        } else {
            0
        };
        let cur = self.buffer.cur_mut(0);
        let mut props = cur.glyph_props | gprops::SUBSTITUTED;
        if ligature {
            props |= gprops::LIGATED;
            props &= !gprops::MULTIPLIED;
        }
        if component {
            props |= gprops::MULTIPLIED;
        }
        if has_classes {
            cur.glyph_props = (props & gprops::PRESERVE) | gdef_props;
        } else if class_guess != 0 {
            cur.glyph_props = (props & gprops::PRESERVE) | class_guess;
        } else {
            cur.glyph_props = props;
        }
    }

    /// `replace_glyph`.
    pub(crate) fn replace_glyph(&mut self, glyph: u32) {
        self.set_glyph_class(glyph, 0, false, false);
        let _ = self.buffer.replace_glyph(glyph);
    }
    /// `replace_glyph_inplace`.
    pub(crate) fn replace_glyph_inplace(&mut self, glyph: u32) {
        self.set_glyph_class(glyph, 0, false, false);
        self.buffer.cur_mut(0).codepoint = glyph;
    }
    /// `replace_glyph_with_ligature`.
    pub(crate) fn replace_glyph_with_ligature(&mut self, glyph: u32, class_guess: u16) {
        self.set_glyph_class(glyph, class_guess, true, false);
        let _ = self.buffer.replace_glyph(glyph);
    }
    /// `output_glyph_for_component`.
    pub(crate) fn output_glyph_for_component(&mut self, glyph: u32, class_guess: u16) {
        self.set_glyph_class(glyph, class_guess, false, true);
        let _ = self.buffer.output_glyph(glyph);
    }

    /// `match_positions[i]`, growing the array as needed.
    fn set_match_position(&mut self, i: usize, v: usize) {
        if self.match_positions.len() <= i {
            self.match_positions.resize(i.wrapping_add(1), 0);
        }
        if let Some(p) = self.match_positions.get_mut(i) {
            *p = v;
        }
    }
    fn match_position(&self, i: usize) -> usize {
        self.match_positions.get(i).copied().unwrap_or(0)
    }

    /// `recurse`.
    pub(crate) fn recurse(&mut self, lookup_index: u16) -> bool {
        if self.nesting_level_left == 0 {
            self.buffer.successful = false;
            return false;
        }
        self.buffer.max_ops = self.buffer.max_ops.saturating_sub(1);
        if self.buffer.max_ops < 0 {
            self.buffer.successful = false;
            return false;
        }
        self.nesting_level_left = self.nesting_level_left.wrapping_sub(1);
        let ret = self.recurse_func(usize::from(lookup_index));
        self.nesting_level_left = self.nesting_level_left.wrapping_add(1);
        ret
    }

    /// `dispatch_recurse_func<hb_ot_apply_context_t>`.
    fn recurse_func(&mut self, lookup_index: usize) -> bool {
        let plan = self.plan;
        let props = if self.table == GSUB {
            plan.layout.gsub.lookup(lookup_index).map_or(0, |l| l.props)
        } else {
            plan.layout.gpos.lookup(lookup_index).map_or(0, |l| l.props)
        };
        let saved_props = self.lookup_props;
        let saved_index = self.lookup_index;
        self.lookup_index = lookup_index;
        self.set_lookup_props(props);
        let saved_positions = core::mem::take(&mut self.match_positions);
        let ret = accel_apply(self, lookup_index, false);
        self.lookup_index = saved_index;
        self.set_lookup_props(saved_props);
        self.match_positions = saved_positions;
        ret
    }
}

/// `hb_ot_layout_lookup_accelerator_t::apply`: the subtables of lookup
/// `index` in order, the first that applies winning; each is tried only
/// when its digest may have the current glyph, except the only one.
pub(crate) fn accel_apply(
    c: &mut ApplyContext<'_, '_, '_, '_, '_, '_, '_>,
    index: usize,
    _use_cache: bool,
) -> bool {
    let plan = c.plan;
    if c.table == GSUB {
        let Some(accel) = plan.layout.gsub.lookup(index) else {
            return false;
        };
        if let [only] = accel.subtables.as_slice() {
            return super::gsub::apply(c, &only.table);
        }
        accel.subtables.iter().any(|s| {
            s.digest.may_have(c.buffer.cur(0).codepoint) && super::gsub::apply(c, &s.table)
        })
    } else {
        let Some(accel) = plan.layout.gpos.lookup(index) else {
            return false;
        };
        if let [only] = accel.subtables.as_slice() {
            return super::gpos::apply(c, &only.table);
        }
        accel.subtables.iter().any(|s| {
            s.digest.may_have(c.buffer.cur(0).codepoint) && super::gpos::apply(c, &s.table)
        })
    }
}

/// `match_input`: matches the glyphs after the current one against
/// `count - 1` values. `Ok((end, total_component_count))` on a match;
/// `Err(unsafe_to)` otherwise.
pub(crate) fn match_input<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    count: usize,
    matcher: MatchFn<'a>,
) -> Result<(usize, u32), usize> {
    let idx = c.buffer.idx;
    if count <= 1 {
        c.set_match_position(0, idx);
        let comps = u32::from(c.buffer.cur(0).lig_num_comps());
        return Ok((idx.wrapping_add(1), comps));
    }
    if count > MAX_CONTEXT_LENGTH {
        return Err(idx);
    }
    c.iter_reset(Which::Input, idx);
    c.iter_input.set_match(Some(matcher));

    let first = *c.buffer.cur(0);
    let first_lig_id = first.lig_id();
    let first_lig_comp = first.lig_comp();
    #[derive(PartialEq)]
    enum LigBase {
        NotChecked,
        MayNotSkip,
        MaySkip,
    }
    let mut ligbase = LigBase::NotChecked;
    let mut total: u32 = 0;
    for i in 1..count {
        c.iter_next(Which::Input)?;
        let at = c.iter_input.idx;
        c.set_match_position(i, at);
        let this = c.buffer.info[at];
        let this_lig_id = this.lig_id();
        let this_lig_comp = this.lig_comp();
        if first_lig_id != 0 && first_lig_comp != 0 {
            if first_lig_id != this_lig_id || first_lig_comp != this_lig_comp {
                if ligbase == LigBase::NotChecked {
                    let mut found = false;
                    let mut j = c.buffer.out_len;
                    while j > 0 && c.buffer.out_info()[j.wrapping_sub(1)].lig_id() == first_lig_id {
                        if c.buffer.out_info()[j.wrapping_sub(1)].lig_comp() == 0 {
                            j = j.wrapping_sub(1);
                            found = true;
                            break;
                        }
                        j = j.wrapping_sub(1);
                    }
                    let out_j = c.buffer.out_info()[j];
                    ligbase = if found && c.may_skip_at(Which::Input, &out_j) == Skip::Yes {
                        LigBase::MaySkip
                    } else {
                        LigBase::MayNotSkip
                    };
                }
                if ligbase == LigBase::MayNotSkip {
                    return Err(c.buffer.idx);
                }
            }
        } else if this_lig_id != 0 && this_lig_comp != 0 && this_lig_id != first_lig_id {
            return Err(c.buffer.idx);
        }
        total = total.wrapping_add(u32::from(this.lig_num_comps_in_ligation()));
    }
    let end = c.iter_input.idx.wrapping_add(1);
    total = total.wrapping_add(u32::from(c.buffer.cur(0).lig_num_comps()));
    c.set_match_position(0, idx);
    Ok((end, total))
}

/// `ligate_input`.
pub(crate) fn ligate_input(
    c: &mut ApplyContext<'_, '_, '_, '_, '_, '_, '_>,
    count: usize,
    match_end: usize,
    lig_glyph: u32,
    total_component_count: u32,
) {
    let idx = c.buffer.idx;
    c.buffer.merge_clusters(idx, match_end);

    let p0 = c.match_position(0);
    let mut is_base_ligature = c.buffer.info[p0].is_base_glyph();
    let mut is_mark_ligature = c.buffer.info[p0].is_mark();
    for i in 1..count {
        let p = c.match_position(i);
        if !c.buffer.info[p].is_mark() {
            is_base_ligature = false;
            is_mark_ligature = false;
            break;
        }
    }
    let is_ligature = !is_base_ligature && !is_mark_ligature;
    let klass = if is_ligature { gprops::LIGATURE } else { 0 };
    let lig_id = if is_ligature {
        c.buffer.allocate_lig_id()
    } else {
        0
    };
    let mut last_lig_id = c.buffer.cur(0).lig_id();
    let mut last_num_components = c.buffer.cur(0).lig_num_comps();
    let mut components_so_far = last_num_components;

    if is_ligature {
        let total = u8::try_from(total_component_count & 0xFF).unwrap_or(0);
        let cur = c.buffer.cur_mut(0);
        cur.set_lig_props_for_ligature(lig_id, total);
        if cur.general_category() == GeneralCategory::NonspacingMark {
            cur.set_general_category(GeneralCategory::OtherLetter);
        }
    }
    c.replace_glyph_with_ligature(lig_glyph, klass);

    for i in 1..count {
        let target = c.match_position(i);
        while c.buffer.idx < target && c.buffer.successful {
            if is_ligature {
                let mut this_comp = c.buffer.cur(0).lig_comp();
                if this_comp == 0 {
                    this_comp = last_num_components;
                }
                let new_comp = components_so_far
                    .wrapping_sub(last_num_components)
                    .wrapping_add(this_comp.min(last_num_components));
                c.buffer.cur_mut(0).set_lig_props_for_mark(lig_id, new_comp);
            }
            let _ = c.buffer.next_glyph();
        }
        last_lig_id = c.buffer.cur(0).lig_id();
        last_num_components = c.buffer.cur(0).lig_num_comps_in_ligation();
        components_so_far = components_so_far.wrapping_add(last_num_components);
        // Skip the base glyph.
        c.buffer.idx = c.buffer.idx.wrapping_add(1);
    }

    if !is_mark_ligature && last_lig_id != 0 {
        let mut i = c.buffer.idx;
        while i < c.buffer.len {
            let info = c.buffer.info[i];
            if last_lig_id != info.lig_id() {
                break;
            }
            let this_comp = info.lig_comp();
            if this_comp == 0 {
                break;
            }
            let new_comp = components_so_far
                .wrapping_sub(last_num_components)
                .wrapping_add(this_comp.min(last_num_components));
            c.buffer.info[i].set_lig_props_for_mark(lig_id, new_comp);
            i = i.wrapping_add(1);
        }
    }
}

/// `match_backtrack`: `Ok(start)` or `Err(unsafe_from)`.
pub(crate) fn match_backtrack<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    count: usize,
    matcher: MatchFn<'a>,
) -> Result<usize, usize> {
    let bl = c.buffer.backtrack_len();
    if count == 0 {
        return Ok(bl);
    }
    c.iter_reset_back(Which::Context, bl);
    c.iter_context.set_match(Some(matcher));
    for _ in 0..count {
        c.iter_prev(Which::Context)?;
    }
    Ok(c.iter_context.idx)
}

/// `match_lookahead`: `Ok(end)` or `Err(unsafe_to)`.
pub(crate) fn match_lookahead<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    count: usize,
    matcher: MatchFn<'a>,
    start: usize,
) -> Result<usize, usize> {
    if count == 0 {
        return Ok(start);
    }
    c.iter_reset(Which::Context, start.wrapping_sub(1));
    c.iter_context.set_match(Some(matcher));
    for _ in 0..count {
        c.iter_next(Which::Context)?;
    }
    Ok(c.iter_context.idx.wrapping_add(1))
}

/// `apply_lookup`: applies the nested lookups of a matched rule.
pub(crate) fn apply_lookup(
    c: &mut ApplyContext<'_, '_, '_, '_, '_, '_, '_>,
    count: usize,
    records: LookupRecords<'_>,
    match_end: usize,
) {
    let mut count = count;
    let to_i = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
    let to_u = |v: i64| usize::try_from(v).unwrap_or(0);
    let bl = c.buffer.backtrack_len();
    let mut end = to_i(bl)
        .wrapping_add(to_i(match_end))
        .wrapping_sub(to_i(c.buffer.idx));
    let delta0 = to_i(bl).wrapping_sub(to_i(c.buffer.idx));
    for j in 0..count {
        let p = c.match_position(j);
        c.set_match_position(j, to_u(to_i(p).wrapping_add(delta0)));
    }

    for record in records.iter() {
        if !c.buffer.successful {
            break;
        }
        let idx = usize::from(record.sequence_index);
        if idx >= count {
            continue;
        }
        let orig_len = c
            .buffer
            .backtrack_len()
            .wrapping_add(c.buffer.lookahead_len());
        let pos = c.match_position(idx);
        if pos >= orig_len {
            continue;
        }
        if !c.buffer.move_to(pos) {
            break;
        }
        if c.buffer.max_ops <= 0 {
            break;
        }
        if !c.recurse(record.lookup_index) {
            continue;
        }
        let new_len = c
            .buffer
            .backtrack_len()
            .wrapping_add(c.buffer.lookahead_len());
        let mut delta = to_i(new_len).wrapping_sub(to_i(orig_len));
        if delta == 0 {
            continue;
        }
        end = end.wrapping_add(delta);
        let pos = to_i(c.match_position(idx));
        if end < pos {
            delta = delta.wrapping_add(pos.wrapping_sub(end));
            end = pos;
        }
        let mut next = to_i(idx).wrapping_add(1);
        let count_i = to_i(count);
        if delta > 0 {
            if to_u(delta).wrapping_add(count) > MAX_CONTEXT_LENGTH {
                break;
            }
            if c.match_positions.len() < to_u(count_i.wrapping_add(delta)) {
                c.match_positions
                    .resize(to_u(count_i.wrapping_add(delta)), 0);
            }
        } else {
            delta = delta.max(next.wrapping_sub(count_i));
            next = next.wrapping_sub(delta);
        }
        // Shift!
        let from = to_u(next);
        let to = to_u(next.wrapping_add(delta));
        let n = to_u(count_i.wrapping_sub(next));
        if from <= c.match_positions.len() && to.wrapping_add(n) <= c.match_positions.len() {
            let src_end = from.wrapping_add(n).min(c.match_positions.len());
            c.match_positions.copy_within(from..src_end, to);
        }
        next = next.wrapping_add(delta);
        count = to_u(count_i.wrapping_add(delta));
        let mut j = idx.wrapping_add(1);
        while to_i(j) < next {
            let prev = c.match_position(j.wrapping_sub(1));
            c.set_match_position(j, prev.wrapping_add(1));
            j = j.wrapping_add(1);
        }
        let mut k = to_u(next);
        while k < count {
            let p = c.match_position(k);
            c.set_match_position(k, to_u(to_i(p).wrapping_add(delta)));
            k = k.wrapping_add(1);
        }
    }
    let _ = c.buffer.move_to(to_u(end.max(0)));
}

/// The match functions of a context: glyphs or classes.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RuleMatch<'a> {
    Glyphs,
    Classes {
        backtrack: ClassDef<'a>,
        input: ClassDef<'a>,
        lookahead: ClassDef<'a>,
    },
}

impl<'a> RuleMatch<'a> {
    fn backtrack(&self, values: GlyphArray<'a>) -> MatchFn<'a> {
        match *self {
            RuleMatch::Glyphs => MatchFn::Glyph(values),
            RuleMatch::Classes { backtrack, .. } => MatchFn::Class(values, backtrack),
        }
    }
    pub(super) fn input(&self, values: GlyphArray<'a>) -> MatchFn<'a> {
        match *self {
            RuleMatch::Glyphs => MatchFn::Glyph(values),
            RuleMatch::Classes { input, .. } => MatchFn::Class(values, input),
        }
    }
    fn lookahead(&self, values: GlyphArray<'a>) -> MatchFn<'a> {
        match *self {
            RuleMatch::Glyphs => MatchFn::Glyph(values),
            RuleMatch::Classes { lookahead, .. } => MatchFn::Class(values, lookahead),
        }
    }
    /// Whether `info` matches value `v` of the input (`match_glyph` or
    /// `match_class`).
    fn input_value(&self, info: &Info, v: u32) -> bool {
        match self {
            RuleMatch::Glyphs => info.codepoint == v,
            RuleMatch::Classes { input, .. } => input.get(GlyphId(info.codepoint)) == v,
        }
    }
    fn lookahead_value(&self, info: &Info, v: u32) -> bool {
        match self {
            RuleMatch::Glyphs => info.codepoint == v,
            RuleMatch::Classes { lookahead, .. } => lookahead.get(GlyphId(info.codepoint)) == v,
        }
    }
}

/// `context_apply_lookup`.
fn context_apply_lookup<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    count: usize,
    input: MatchFn<'a>,
    records: LookupRecords<'_>,
) -> bool {
    if count > MAX_CONTEXT_LENGTH {
        return false;
    }
    match match_input(c, count, input) {
        Ok((end, _)) => {
            let idx = c.buffer.idx;
            c.buffer.unsafe_to_break(idx, end);
            apply_lookup(c, count, records, end);
            true
        }
        Err(end) => {
            let idx = c.buffer.idx;
            c.buffer.unsafe_to_concat(idx, end);
            false
        }
    }
}

/// `Rule::apply`.
fn rule_apply<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    r: &Rule<'a>,
    m: RuleMatch<'a>,
) -> bool {
    context_apply_lookup(
        c,
        r.input.len().wrapping_add(1),
        m.input(r.input),
        r.lookups,
    )
}

/// The skippy iterator's first two glyphs for the rule set fast path:
/// `None` to take the slow path, else `(first, unsafe_to1, second,
/// unsafe_to2)`; `first` is `None` when no glyph follows.
fn fast_path_glyphs(
    c: &mut ApplyContext<'_, '_, '_, '_, '_, '_, '_>,
) -> Option<(Option<Info>, usize, Option<Info>, usize)> {
    let idx = c.buffer.idx;
    c.iter_reset(Which::Context, idx);
    c.iter_context.set_match(Some(MatchFn::Always));
    let first = match c.iter_next(Which::Context) {
        Ok(()) => {
            let at = c.iter_context.idx;
            let info = c.buffer.info[at];
            if c.may_skip_at(Which::Context, &info) != Skip::No {
                return None;
            }
            (Some(info), at.wrapping_add(1))
        }
        Err(unsafe_to) => (None, unsafe_to),
    };
    if first.0.is_none() {
        return Some((None, first.1, None, 0));
    }
    let second = match c.iter_next(Which::Context) {
        Ok(()) => {
            let at = c.iter_context.idx;
            let info = c.buffer.info[at];
            if c.may_skip_at(Which::Context, &info) != Skip::No {
                return None;
            }
            (Some(info), at.wrapping_add(1))
        }
        Err(_) => (None, 0),
    };
    Some((first.0, first.1, second.0, second.1))
}

/// `RuleSet::apply`, with HarfBuzz's fast path for more than four rules.
fn rule_set_apply<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    sets: &RuleSets<'a>,
    index: usize,
    m: RuleMatch<'a>,
) -> bool {
    let Ok(Some(set)) = sets.get(index) else {
        return false;
    };
    let rules: Vec<Rule<'a>> = (0..set.len())
        .filter_map(|i| set.get(i).ok().flatten())
        .collect();
    let slow = |c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>| {
        rules.iter().any(|r| rule_apply(c, r, m))
    };
    if set.len() <= 4 {
        return slow(c);
    }
    let Some((first, unsafe_to1, second, unsafe_to2)) = fast_path_glyphs(c) else {
        return slow(c);
    };
    let Some(first) = first else {
        let mut unsafe_to_concat = false;
        let mut ret = false;
        for r in &rules {
            if r.input.len().wrapping_add(1) <= 1 {
                if rule_apply(c, r, m) {
                    ret = true;
                    break;
                }
            } else {
                unsafe_to_concat = true;
            }
        }
        if unsafe_to_concat {
            let idx = c.buffer.idx;
            c.buffer.unsafe_to_concat(idx, unsafe_to1);
        }
        return ret;
    };
    let mut unsafe_to: Option<usize> = None;
    let mut i = 0usize;
    while i < rules.len() {
        let Some(r) = rules.get(i) else { break };
        let first_ok =
            r.input.is_empty() || r.input.get(0).is_some_and(|v| m.input_value(&first, v));
        if first_ok {
            let second_ok = match &second {
                None => true,
                Some(s) => r.input.len() < 2 || r.input.get(1).is_some_and(|v| m.input_value(s, v)),
            };
            if second_ok {
                if rule_apply(c, r, m) {
                    if let Some(u) = unsafe_to {
                        let idx = c.buffer.idx;
                        c.buffer.unsafe_to_concat(idx, u);
                    }
                    return true;
                }
            } else {
                unsafe_to = Some(unsafe_to2);
            }
        } else {
            if unsafe_to.is_none() {
                unsafe_to = Some(unsafe_to1);
            }
            // Skip ahead to the next possible first glyph match.
            let v0 = r.input.get(0);
            while let Some(r2) = rules.get(i.wrapping_add(1)) {
                if r2.input.is_empty() || r2.input.get(0) != v0 {
                    break;
                }
                i = i.wrapping_add(1);
            }
        }
        i = i.wrapping_add(1);
    }
    if let Some(u) = unsafe_to {
        let idx = c.buffer.idx;
        c.buffer.unsafe_to_concat(idx, u);
    }
    false
}

/// `ContextFormat1`..`ContextFormat5` and the coverage formats 3 and 6.
pub(crate) fn apply_context<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    s: &SequenceContext<'a>,
) -> bool {
    let glyph = GlyphId(c.buffer.cur(0).codepoint);
    match s {
        SequenceContext::Glyphs {
            coverage,
            rule_sets,
        } => {
            let Some(index) = coverage.get(glyph) else {
                return false;
            };
            rule_set_apply(
                c,
                rule_sets,
                usize::try_from(index).unwrap_or(usize::MAX),
                RuleMatch::Glyphs,
            )
        }
        SequenceContext::Classes {
            coverage,
            class_def,
            rule_sets,
        } => {
            if coverage.get(glyph).is_none() {
                return false;
            }
            let class = class_def.get(glyph);
            let m = RuleMatch::Classes {
                backtrack: *class_def,
                input: *class_def,
                lookahead: *class_def,
            };
            rule_set_apply(
                c,
                rule_sets,
                usize::try_from(class).unwrap_or(usize::MAX),
                m,
            )
        }
        SequenceContext::Coverages { coverages, lookups } => {
            let first = coverages.get(0).ok().flatten();
            if first.and_then(|f| f.get(glyph)).is_none() {
                return false;
            }
            context_apply_lookup(
                c,
                coverages.len(),
                MatchFn::Coverage(*coverages, 1),
                *lookups,
            )
        }
    }
}

/// `chain_context_apply_lookup`.
fn chain_context_apply_lookup<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    backtrack: (usize, MatchFn<'a>),
    input: (usize, MatchFn<'a>),
    lookahead: (usize, MatchFn<'a>),
    records: LookupRecords<'_>,
) -> bool {
    if input.0 > MAX_CONTEXT_LENGTH {
        return false;
    }
    let idx = c.buffer.idx;
    let mut end_index = idx;
    let matched = match match_input(c, input.0, input.1) {
        Ok((match_end, _)) => match match_lookahead(c, lookahead.0, lookahead.1, match_end) {
            Ok(e) => {
                end_index = e;
                Some(match_end)
            }
            Err(e) => {
                end_index = e;
                None
            }
        },
        Err(_) => None,
    };
    let Some(match_end) = matched else {
        let idx = c.buffer.idx;
        c.buffer.unsafe_to_concat(idx, end_index);
        return false;
    };
    let start_index = match match_backtrack(c, backtrack.0, backtrack.1) {
        Ok(s) => s,
        Err(s) => {
            c.buffer.unsafe_to_concat_from_outbuffer(s, end_index);
            return false;
        }
    };
    c.buffer
        .unsafe_to_break_from_outbuffer(start_index, end_index);
    apply_lookup(c, input.0, records, match_end);
    true
}

/// `ChainRule::apply`.
fn chain_rule_apply<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    r: &ChainRule<'a>,
    m: RuleMatch<'a>,
) -> bool {
    chain_context_apply_lookup(
        c,
        (r.backtrack.len(), m.backtrack(r.backtrack)),
        (r.input.len().wrapping_add(1), m.input(r.input)),
        (r.lookahead.len(), m.lookahead(r.lookahead)),
        r.lookups,
    )
}

/// `ChainRuleSet::apply`, with HarfBuzz's fast path for more than four
/// rules.
fn chain_rule_set_apply<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    sets: &ChainRuleSets<'a>,
    index: usize,
    m: RuleMatch<'a>,
) -> bool {
    let Ok(Some(set)) = sets.get(index) else {
        return false;
    };
    let rules: Vec<ChainRule<'a>> = (0..set.len())
        .filter_map(|i| set.get(i).ok().flatten())
        .collect();
    let slow = |c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>| {
        rules.iter().any(|r| chain_rule_apply(c, r, m))
    };
    if set.len() <= 4 {
        return slow(c);
    }
    let Some((first, unsafe_to1, second, unsafe_to2)) = fast_path_glyphs(c) else {
        return slow(c);
    };
    let Some(first) = first else {
        let mut unsafe_to_concat = false;
        let mut ret = false;
        for r in &rules {
            if r.input.is_empty() && r.lookahead.is_empty() {
                if chain_rule_apply(c, r, m) {
                    ret = true;
                    break;
                }
            } else {
                unsafe_to_concat = true;
            }
        }
        if unsafe_to_concat {
            let idx = c.buffer.idx;
            c.buffer.unsafe_to_concat(idx, unsafe_to1);
        }
        return ret;
    };
    let mut unsafe_to: Option<usize> = None;
    let mut i = 0usize;
    while i < rules.len() {
        let Some(r) = rules.get(i) else { break };
        let len_p1 = r.input.len().wrapping_add(1);
        let first_ok = if len_p1 > 1 {
            r.input.get(0).is_some_and(|v| m.input_value(&first, v))
        } else {
            r.lookahead.is_empty()
                || r.lookahead
                    .get(0)
                    .is_some_and(|v| m.lookahead_value(&first, v))
        };
        if first_ok {
            let second_ok = match &second {
                None => true,
                Some(s) => {
                    if len_p1 > 2 {
                        r.input.get(1).is_some_and(|v| m.input_value(s, v))
                    } else {
                        let k = 2usize.wrapping_sub(len_p1);
                        r.lookahead.len() <= k
                            || r.lookahead.get(k).is_some_and(|v| m.lookahead_value(s, v))
                    }
                }
            };
            if second_ok {
                if chain_rule_apply(c, r, m) {
                    if let Some(u) = unsafe_to {
                        let idx = c.buffer.idx;
                        c.buffer.unsafe_to_concat(idx, u);
                    }
                    return true;
                }
            } else {
                unsafe_to = Some(unsafe_to2);
            }
        } else {
            if unsafe_to.is_none() {
                unsafe_to = Some(unsafe_to1);
            }
            if len_p1 > 1 {
                let v0 = r.input.get(0);
                while let Some(r2) = rules.get(i.wrapping_add(1)) {
                    if r2.input.is_empty() || r2.input.get(0) != v0 {
                        break;
                    }
                    i = i.wrapping_add(1);
                }
            }
        }
        i = i.wrapping_add(1);
    }
    if let Some(u) = unsafe_to {
        let idx = c.buffer.idx;
        c.buffer.unsafe_to_concat(idx, u);
    }
    false
}

/// `ChainContextFormat1`..`ChainContextFormat5`.
pub(crate) fn apply_chain_context<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    s: &ChainedSequenceContext<'a>,
) -> bool {
    let glyph = GlyphId(c.buffer.cur(0).codepoint);
    match s {
        ChainedSequenceContext::Glyphs {
            coverage,
            rule_sets,
        } => {
            let Some(index) = coverage.get(glyph) else {
                return false;
            };
            chain_rule_set_apply(
                c,
                rule_sets,
                usize::try_from(index).unwrap_or(usize::MAX),
                RuleMatch::Glyphs,
            )
        }
        ChainedSequenceContext::Classes {
            coverage,
            backtrack_class_def,
            input_class_def,
            lookahead_class_def,
            rule_sets,
        } => {
            if coverage.get(glyph).is_none() {
                return false;
            }
            let class = input_class_def.get(glyph);
            let m = RuleMatch::Classes {
                backtrack: *backtrack_class_def,
                input: *input_class_def,
                lookahead: *lookahead_class_def,
            };
            chain_rule_set_apply(
                c,
                rule_sets,
                usize::try_from(class).unwrap_or(usize::MAX),
                m,
            )
        }
        ChainedSequenceContext::Coverages {
            backtrack,
            input,
            lookahead,
            lookups,
        } => {
            let first = input.get(0).ok().flatten();
            if first.and_then(|f| f.get(glyph)).is_none() {
                return false;
            }
            chain_context_apply_lookup(
                c,
                (backtrack.len(), MatchFn::Coverage(*backtrack, 0)),
                (input.len(), MatchFn::Coverage(*input, 1)),
                (lookahead.len(), MatchFn::Coverage(*lookahead, 0)),
                *lookups,
            )
        }
    }
}

/// `apply_forward`.
fn apply_forward(c: &mut ApplyContext<'_, '_, '_, '_, '_, '_, '_>, index: usize) -> bool {
    let plan = c.plan;
    let digest = if c.table == GSUB {
        plan.layout.gsub.lookup(index).map(|l| l.digest)
    } else {
        plan.layout.gpos.lookup(index).map(|l| l.digest)
    };
    let Some(digest) = digest else { return false };
    let mut ret = false;
    while c.buffer.successful {
        let mut j = c.buffer.idx;
        while j < c.buffer.len {
            let info = &c.buffer.info[j];
            if digest.may_have(info.codepoint)
                && info.mask & c.lookup_mask != 0
                && c.check_glyph_property(info, c.lookup_props)
            {
                break;
            }
            j = j.wrapping_add(1);
        }
        if j > c.buffer.idx && !c.buffer.next_glyphs(j.wrapping_sub(c.buffer.idx)) {
            break;
        }
        if c.buffer.idx >= c.buffer.len {
            break;
        }
        if accel_apply(c, index, true) {
            ret = true;
        } else {
            let _ = c.buffer.next_glyph();
        }
    }
    ret
}

/// `apply_backward`.
fn apply_backward(c: &mut ApplyContext<'_, '_, '_, '_, '_, '_, '_>, index: usize) -> bool {
    let plan = c.plan;
    let Some(digest) = plan.layout.gsub.lookup(index).map(|l| l.digest) else {
        return false;
    };
    let mut ret = false;
    loop {
        let cur = *c.buffer.cur(0);
        if digest.may_have(cur.codepoint)
            && cur.mask & c.lookup_mask != 0
            && c.check_glyph_property(&cur, c.lookup_props)
        {
            ret |= accel_apply(c, index, false);
        }
        if c.buffer.idx == 0 {
            break;
        }
        c.buffer.idx = c.buffer.idx.wrapping_sub(1);
    }
    ret
}

/// `apply_string`.
fn apply_string(c: &mut ApplyContext<'_, '_, '_, '_, '_, '_, '_>, index: usize) -> bool {
    if c.buffer.len == 0 || c.lookup_mask == 0 {
        return false;
    }
    let plan = c.plan;
    let (props, reverse) = if c.table == GSUB {
        plan.layout
            .gsub
            .lookup(index)
            .map_or((0, false), |l| (l.props, l.reverse))
    } else {
        plan.layout
            .gpos
            .lookup(index)
            .map_or((0, false), |l| (l.props, false))
    };
    c.set_lookup_props(props);
    if !reverse {
        let in_place = c.table == GPOS;
        if !in_place {
            c.buffer.clear_output();
        }
        c.buffer.idx = 0;
        let ret = apply_forward(c, index);
        if !in_place {
            c.buffer.sync();
        }
        ret
    } else {
        c.buffer.idx = c.buffer.len.wrapping_sub(1);
        let ret = apply_backward(c, index);
        c.buffer.idx = 0;
        ret
    }
}

/// `hb_ot_layout_substitute_lookup` for a lookup a shaper built, not in
/// the font (the Arabic fallback lookups of ADR 0024): its `props`, its
/// subtables with their digests, and the digest of all of them. The
/// context applies it as HarfBuzz's with an empty `GDEF`
/// (`has_glyph_classes` cleared by the caller).
pub(crate) fn apply_synthesized_gsub<'a>(
    c: &mut ApplyContext<'_, 'a, '_, '_, '_, '_, '_>,
    props: u32,
    subtables: &[(
        sofel_font::read::gsub::SubstSubtable<'a>,
        crate::digest::Digest,
    )],
    digest: &crate::digest::Digest,
) -> bool {
    if c.buffer.len == 0 || c.lookup_mask == 0 {
        return false;
    }
    c.set_lookup_props(props);
    c.buffer.clear_output();
    c.buffer.idx = 0;
    let mut ret = false;
    while c.buffer.successful {
        let mut j = c.buffer.idx;
        while j < c.buffer.len {
            let info = &c.buffer.info[j];
            if digest.may_have(info.codepoint)
                && info.mask & c.lookup_mask != 0
                && c.check_glyph_property(info, c.lookup_props)
            {
                break;
            }
            j = j.wrapping_add(1);
        }
        if j > c.buffer.idx && !c.buffer.next_glyphs(j.wrapping_sub(c.buffer.idx)) {
            break;
        }
        if c.buffer.idx >= c.buffer.len {
            break;
        }
        let applied = match subtables {
            [(only, _)] => super::gsub::apply(c, only),
            _ => subtables
                .iter()
                .any(|(s, d)| d.may_have(c.buffer.cur(0).codepoint) && super::gsub::apply(c, s)),
        };
        if applied {
            ret = true;
        } else {
            let _ = c.buffer.next_glyph();
        }
    }
    c.buffer.sync();
    ret
}

/// `hb_ot_map_t::apply`: the lookups of every stage of `table`, with the
/// pauses between stages.
pub(crate) fn apply_map(
    table: usize,
    plan: &ShapePlan<'_>,
    font: &ShapeFont<'_, '_, '_, '_>,
    buffer: &mut Buffer,
) {
    let (Some(stages), Some(lookups)) = (plan.map.stages.get(table), plan.map.lookups.get(table))
    else {
        return;
    };
    let mut i = 0usize;
    for stage in stages {
        {
            let mut c = ApplyContext::new(table, plan, font, buffer);
            while i < stage.last_lookup {
                let Some(lookup) = lookups.get(i) else { break };
                i = i.wrapping_add(1);
                let index = usize::from(lookup.index);
                let digest = if table == GSUB {
                    plan.layout.gsub.lookup(index).map(|l| l.digest)
                } else {
                    plan.layout.gpos.lookup(index).map(|l| l.digest)
                };
                let Some(digest) = digest else { continue };
                if !digest.may_intersect(&c.buffer.digest) {
                    continue;
                }
                c.lookup_index = index;
                c.set_lookup_mask(lookup.mask);
                c.auto_zwj = lookup.auto_zwj;
                c.auto_zwnj = lookup.auto_zwnj;
                c.random = lookup.random;
                c.per_syllable = lookup.per_syllable;
                apply_string(&mut c, index);
            }
        }
        if let Some(pause) = stage.pause
            && pause(plan, font, buffer)
        {
            buffer.update_digest();
        }
    }
}
