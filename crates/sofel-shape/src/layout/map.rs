//! The feature map of a plan (`hb_ot_map_t`, `hb_ot_map_builder_t`):
//! which features a font has for the chosen script and language, the
//! mask bits each gets, and the lookups of each stage of `GSUB` and
//! `GPOS` in the order they are applied.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-ot-map.hh` and `src/hb-ot-map.cc`,
//! MIT; see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::read::Tag;

use super::{GPOS, GSUB, Layout};
use crate::buffer::{Buffer, Mask};
use crate::font::ShapeFont;
use crate::plan::ShapePlan;

/// `HB_OT_MAP_MAX_BITS`.
pub(crate) const MAX_BITS: u32 = 8;
/// `HB_OT_MAP_MAX_VALUE`.
pub(crate) const MAX_VALUE: u32 = (1 << MAX_BITS) - 1;

/// `hb_ot_map_feature_flags_t`.
pub(crate) mod flags {
    pub(crate) const NONE: u32 = 0x0000;
    pub(crate) const GLOBAL: u32 = 0x0001;
    pub(crate) const HAS_FALLBACK: u32 = 0x0002;
    pub(crate) const MANUAL_ZWNJ: u32 = 0x0004;
    pub(crate) const MANUAL_ZWJ: u32 = 0x0008;
    pub(crate) const MANUAL_JOINERS: u32 = MANUAL_ZWNJ | MANUAL_ZWJ;
    pub(crate) const GLOBAL_MANUAL_JOINERS: u32 = GLOBAL | MANUAL_JOINERS;
    pub(crate) const GLOBAL_HAS_FALLBACK: u32 = GLOBAL | HAS_FALLBACK;
    pub(crate) const GLOBAL_SEARCH: u32 = 0x0010;
    pub(crate) const RANDOM: u32 = 0x0020;
    pub(crate) const PER_SYLLABLE: u32 = 0x0040;
}

/// A pause between stages (`pause_func_t`): returns whether glyphs
/// may have been added, so that the buffer digest is refreshed.
pub(crate) type PauseFn = fn(&ShapePlan<'_>, &ShapeFont<'_, '_>, &mut Buffer) -> bool;

/// `feature_map_t`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FeatureMap {
    pub(crate) tag: Tag,
    pub(crate) index: [Option<u16>; 2],
    pub(crate) stage: [usize; 2],
    pub(crate) shift: u32,
    pub(crate) mask: Mask,
    pub(crate) one_mask: Mask,
    pub(crate) needs_fallback: bool,
    pub(crate) auto_zwnj: bool,
    pub(crate) auto_zwj: bool,
    pub(crate) random: bool,
    pub(crate) per_syllable: bool,
}

/// `lookup_map_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LookupMap {
    pub(crate) index: u16,
    pub(crate) auto_zwnj: bool,
    pub(crate) auto_zwj: bool,
    pub(crate) random: bool,
    pub(crate) per_syllable: bool,
    pub(crate) mask: Mask,
    pub(crate) feature_tag: Tag,
}

/// `stage_map_t`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StageMap {
    /// The number of lookups up to the end of this stage.
    pub(crate) last_lookup: usize,
    pub(crate) pause: Option<PauseFn>,
}

/// `hb_ot_map_t`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Map {
    pub(crate) global_mask: Mask,
    /// Sorted by tag.
    pub(crate) features: Vec<FeatureMap>,
    pub(crate) lookups: [Vec<LookupMap>; 2],
    pub(crate) stages: [Vec<StageMap>; 2],
    pub(crate) chosen_script: [Option<Tag>; 2],
    pub(crate) found_script: [bool; 2],
}

impl Map {
    fn feature(&self, tag: Tag) -> Option<&FeatureMap> {
        self.features
            .binary_search_by(|f| f.tag.cmp(&tag))
            .ok()
            .and_then(|i| self.features.get(i))
    }
    /// `get_mask`: the mask and the shift of a feature.
    pub(crate) fn mask(&self, tag: Tag) -> (Mask, u32) {
        self.feature(tag).map_or((0, 0), |f| (f.mask, f.shift))
    }
    /// `get_1_mask`.
    pub(crate) fn one_mask(&self, tag: Tag) -> Mask {
        self.feature(tag).map_or(0, |f| f.one_mask)
    }
    /// `needs_fallback`.
    pub(crate) fn needs_fallback(&self, tag: Tag) -> bool {
        self.feature(tag).is_some_and(|f| f.needs_fallback)
    }
    /// `get_feature_index`.
    pub(crate) fn feature_index(&self, table: usize, tag: Tag) -> Option<u16> {
        self.feature(tag)
            .and_then(|f| f.index.get(table).copied().flatten())
    }
    /// `get_feature_stage`.
    pub(crate) fn feature_stage(&self, table: usize, tag: Tag) -> Option<usize> {
        self.feature(tag).and_then(|f| f.stage.get(table).copied())
    }
}

/// `feature_info_t`.
#[derive(Clone, Copy, Debug)]
struct FeatureInfo {
    tag: Tag,
    seq: usize,
    max_value: u32,
    flags: u32,
    default_value: u32,
    stage: [usize; 2],
}

/// `stage_info_t`.
#[derive(Clone, Copy, Debug)]
struct StageInfo {
    index: usize,
    pause: Option<PauseFn>,
}

/// `hb_ot_map_builder_t`.
#[derive(Debug)]
pub(crate) struct MapBuilder<'l, 'a> {
    layout: &'l Layout<'a>,
    pub(crate) is_simple: bool,
    pub(crate) chosen_script: [Option<Tag>; 2],
    pub(crate) found_script: [bool; 2],
    pub(crate) script_index: [Option<usize>; 2],
    pub(crate) language_index: [Option<usize>; 2],
    current_stage: [usize; 2],
    feature_infos: Vec<FeatureInfo>,
    stages: [Vec<StageInfo>; 2],
}

/// `bits_needed` for a value: `hb_bit_storage`.
fn bit_storage(v: u32) -> u32 {
    32u32.saturating_sub(v.leading_zeros())
}

impl<'l, 'a> MapBuilder<'l, 'a> {
    /// The builder for the script and language tags (in preference
    /// order) of the segment.
    pub(crate) fn new(
        layout: &'l Layout<'a>,
        script_tags: &[Tag],
        language_tags: &[Tag],
    ) -> MapBuilder<'l, 'a> {
        let (found0, script0, chosen0) = layout.gsub.select_script(script_tags);
        let (found1, script1, chosen1) = layout.gpos.select_script(script_tags);
        MapBuilder {
            layout,
            is_simple: false,
            chosen_script: [chosen0, chosen1],
            found_script: [found0, found1],
            script_index: [script0, script1],
            language_index: [
                layout.gsub.select_language(script0, language_tags),
                layout.gpos.select_language(script1, language_tags),
            ],
            current_stage: [0, 0],
            feature_infos: Vec::new(),
            stages: [Vec::new(), Vec::new()],
        }
    }

    /// `add_feature`.
    pub(crate) fn add_feature(&mut self, tag: Tag, flags: u32, value: u32) {
        if tag.0 == [0; 4] {
            return;
        }
        let seq = self.feature_infos.len().wrapping_add(1);
        self.feature_infos.push(FeatureInfo {
            tag,
            seq,
            max_value: value,
            flags,
            default_value: if flags & flags::GLOBAL != 0 { value } else { 0 },
            stage: self.current_stage,
        });
    }
    /// `enable_feature`.
    pub(crate) fn enable_feature(&mut self, tag: Tag, flags: u32, value: u32) {
        self.add_feature(tag, flags::GLOBAL | flags, value);
    }
    /// `disable_feature`.
    pub(crate) fn disable_feature(&mut self, tag: Tag) {
        self.add_feature(tag, flags::GLOBAL, 0);
    }
    /// `add_gsub_pause`.
    pub(crate) fn add_gsub_pause(&mut self, pause: Option<PauseFn>) {
        self.add_pause(GSUB, pause);
    }
    /// `add_gpos_pause`.
    pub(crate) fn add_gpos_pause(&mut self, pause: Option<PauseFn>) {
        self.add_pause(GPOS, pause);
    }
    fn add_pause(&mut self, table: usize, pause: Option<PauseFn>) {
        let Some(cur) = self.current_stage.get_mut(table) else {
            return;
        };
        let index = *cur;
        *cur = index.wrapping_add(1);
        if let Some(s) = self.stages.get_mut(table) {
            s.push(StageInfo { index, pause });
        }
    }

    /// `has_feature`.
    pub(crate) fn has_feature(&self, tag: Tag) -> bool {
        self.layout
            .gsub
            .language_find_feature(self.script_index[0], self.language_index[0], tag)
            .is_some()
            || self
                .layout
                .gpos
                .language_find_feature(self.script_index[1], self.language_index[1], tag)
                .is_some()
    }

    fn table_required(&self, table: usize) -> (Option<u16>, Option<Tag>) {
        if table == GSUB {
            let i = self
                .layout
                .gsub
                .required_feature(self.script_index[0], self.language_index[0]);
            (i, i.and_then(|i| self.layout.gsub.feature_tag(i)))
        } else {
            let i = self
                .layout
                .gpos
                .required_feature(self.script_index[1], self.language_index[1]);
            (i, i.and_then(|i| self.layout.gpos.feature_tag(i)))
        }
    }

    fn feature_lookups(&self, table: usize, index: u16) -> Vec<u16> {
        if table == GSUB {
            self.layout.gsub.feature_lookups(index)
        } else {
            self.layout.gpos.feature_lookups(index)
        }
    }

    fn lookup_count(&self, table: usize) -> usize {
        if table == GSUB {
            self.layout.gsub.lookup_count()
        } else {
            self.layout.gpos.lookup_count()
        }
    }

    /// `add_lookups`.
    #[allow(clippy::too_many_arguments)]
    fn add_lookups(
        &self,
        m: &mut Map,
        table: usize,
        feature_index: Option<u16>,
        mask: Mask,
        auto_zwnj: bool,
        auto_zwj: bool,
        random: bool,
        per_syllable: bool,
        feature_tag: Tag,
    ) {
        let Some(index) = feature_index else { return };
        let count = self.lookup_count(table);
        let Some(lookups) = m.lookups.get_mut(table) else {
            return;
        };
        for l in self.feature_lookups(table, index) {
            if usize::from(l) >= count {
                continue;
            }
            lookups.push(LookupMap {
                index: l,
                auto_zwnj,
                auto_zwj,
                random,
                per_syllable,
                mask,
                feature_tag,
            });
        }
    }

    /// `compile`.
    pub(crate) fn compile(mut self) -> Map {
        let global_bit_shift: u32 = 31;
        let global_bit_mask: Mask = 1 << global_bit_shift;
        let mut m = Map {
            global_mask: global_bit_mask,
            chosen_script: self.chosen_script,
            found_script: self.found_script,
            ..Map::default()
        };
        let required = [self.table_required(GSUB), self.table_required(GPOS)];
        let mut required_stage = [0usize; 2];

        // Sort features and merge duplicates.
        if !self.is_simple {
            self.feature_infos.sort_by(|a, b| a.tag.cmp(&b.tag).then(a.seq.cmp(&b.seq)));
        }
        let mut merged: Vec<FeatureInfo> = Vec::with_capacity(self.feature_infos.len());
        for f in &self.feature_infos {
            match merged.last_mut() {
                Some(j) if j.tag == f.tag => {
                    if f.flags & flags::GLOBAL != 0 {
                        j.flags |= flags::GLOBAL;
                        j.max_value = f.max_value;
                        j.default_value = f.default_value;
                    } else {
                        j.flags &= !flags::GLOBAL;
                        j.max_value = j.max_value.max(f.max_value);
                    }
                    j.flags |= f.flags & flags::HAS_FALLBACK;
                    j.stage[0] = j.stage[0].min(f.stage[0]);
                    j.stage[1] = j.stage[1].min(f.stage[1]);
                }
                _ => merged.push(*f),
            }
        }

        let feature_indices = [
            self.layout
                .gsub
                .features_map(self.script_index[0], self.language_index[0]),
            self.layout
                .gpos
                .features_map(self.script_index[1], self.language_index[1]),
        ];

        // Allocate bits now; the low bits hold the glyph flags.
        let mut next_bit: u32 = 3 + 1;
        for info in &merged {
            let global_one = info.flags & flags::GLOBAL != 0 && info.max_value == 1;
            let bits_needed = if global_one {
                0
            } else {
                MAX_BITS.min(bit_storage(info.max_value))
            };
            if info.max_value == 0 || next_bit.saturating_add(bits_needed) >= global_bit_shift {
                continue;
            }
            let mut found = false;
            let mut feature_index = [None, None];
            for t in [GSUB, GPOS] {
                if required[t].1 == Some(info.tag) {
                    required_stage[t] = info.stage[t];
                }
                feature_index[t] = feature_indices[t].get(&info.tag).copied();
                found |= feature_index[t].is_some();
            }
            if !found && info.flags & flags::GLOBAL_SEARCH != 0 {
                feature_index[GSUB] = self.layout.gsub.find_feature(info.tag);
                feature_index[GPOS] = self.layout.gpos.find_feature(info.tag);
                found = feature_index[GSUB].is_some() || feature_index[GPOS].is_some();
            }
            if !found && info.flags & flags::HAS_FALLBACK == 0 {
                continue;
            }
            let (shift, mask) = if global_one {
                (global_bit_shift, global_bit_mask)
            } else {
                let mask = (1u32 << next_bit.saturating_add(bits_needed))
                    .wrapping_sub(1u32 << next_bit);
                (next_bit, mask)
            };
            if !global_one {
                next_bit = next_bit.saturating_add(bits_needed);
                m.global_mask |= info.default_value.wrapping_shl(shift) & mask;
            }
            m.features.push(FeatureMap {
                tag: info.tag,
                index: feature_index,
                stage: info.stage,
                shift,
                mask,
                one_mask: (1u32 << shift) & mask,
                needs_fallback: !found,
                auto_zwnj: info.flags & flags::MANUAL_ZWNJ == 0,
                auto_zwj: info.flags & flags::MANUAL_ZWJ == 0,
                random: info.flags & flags::RANDOM != 0,
                per_syllable: info.flags & flags::PER_SYLLABLE != 0,
            });
        }
        if self.is_simple {
            m.features.sort_by(|a, b| a.tag.cmp(&b.tag));
        }

        self.add_gsub_pause(None);
        self.add_gpos_pause(None);

        for t in [GSUB, GPOS] {
            let mut stage_index = 0usize;
            let mut last_num_lookups = 0usize;
            for stage in 0..self.current_stage[t] {
                if let (Some(index), true) = (required[t].0, required_stage[t] == stage) {
                    self.add_lookups(
                        &mut m,
                        t,
                        Some(index),
                        global_bit_mask,
                        true,
                        true,
                        false,
                        false,
                        Tag(*b"    "),
                    );
                }
                let features = m.features.clone();
                for f in features.iter().filter(|f| f.stage[t] == stage) {
                    self.add_lookups(
                        &mut m,
                        t,
                        f.index[t],
                        f.mask,
                        f.auto_zwnj,
                        f.auto_zwj,
                        f.random,
                        f.per_syllable,
                        f.tag,
                    );
                }
                // Sort lookups and merge duplicates.
                if let Some(lookups) = m.lookups.get_mut(t) {
                    if last_num_lookups.wrapping_add(1) < lookups.len() {
                        let tail = lookups.split_off(last_num_lookups);
                        let mut tail = tail;
                        tail.sort_by_key(|l| l.index);
                        for l in tail {
                            let can_merge = lookups.len() > last_num_lookups;
                            match lookups.last_mut() {
                                Some(j) if can_merge && j.index == l.index => {
                                    j.mask |= l.mask;
                                    j.auto_zwnj &= l.auto_zwnj;
                                    j.auto_zwj &= l.auto_zwj;
                                }
                                _ => lookups.push(l),
                            }
                        }
                    }
                    last_num_lookups = lookups.len();
                }
                if let Some(s) = self.stages[t].get(stage_index)
                    && s.index == stage
                {
                    if let Some(stages) = m.stages.get_mut(t) {
                        stages.push(StageMap {
                            last_lookup: last_num_lookups,
                            pause: s.pause,
                        });
                    }
                    stage_index = stage_index.wrapping_add(1);
                }
            }
        }
        m
    }
}
