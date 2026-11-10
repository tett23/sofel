//! The buffers (ADR 0020, 入力のバッファ, 出力, データ構造):
//! [`UnicodeBuffer`] goes into shaping, [`GlyphBuffer`] comes out, and
//! both wrap the working [`Buffer`] of HarfBuzz's `hb_buffer_t` so that
//! the memory is handed back and forth.
//!
//! An item of the working buffer is an [`Info`] with named fields
//! instead of HarfBuzz's `var1` and `var2` unions; positions are a
//! separate array of [`Pos`], which also hold the attachment chains of
//! `GPOS`. As in HarfBuzz, substitutions write to an output array that
//! is the input array itself until the output grows past the input
//! (`have_separate_output`).
//!
//! The arrays are [`Items`]: indexing past the end reads a zero item and
//! writes to a scratch item instead of panicking, as HarfBuzz's `Null`
//! and `Crap` objects do. Code paths that would index past the end are
//! bugs or exhausted limits; [`Buffer::successful`] is cleared where
//! HarfBuzz clears it.
//!
//! Ported from HarfBuzz 15.1.0 `src/hb-buffer.hh`, `src/hb-buffer.cc`
//! and `src/hb-ot-layout.hh`, and HarfRust 0.15.0 `src/buffer.rs`, both
//! MIT; see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;
use core::ops::{Index, IndexMut};

use sofel_unicode::{GeneralCategory, Script};

use crate::digest::Digest;
use crate::direction::Direction;
use crate::glyph::{GlyphFlags, GlyphInfo, GlyphPosition};
use crate::unicode;
use crate::utf::{self, Utf};

/// A mask of feature bits (`hb_mask_t`).
pub(crate) type Mask = u32;

/// Flags of a buffer (`hb_buffer_flags_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct BufferFlags(pub u32);

impl BufferFlags {
    /// No flags (`HB_BUFFER_FLAG_DEFAULT`).
    pub const DEFAULT: BufferFlags = BufferFlags(0);
    /// The text starts the paragraph (`HB_BUFFER_FLAG_BOT`): a mark at
    /// the start gets a dotted circle.
    pub const BOT: BufferFlags = BufferFlags(0x1);
    /// The text ends the paragraph (`HB_BUFFER_FLAG_EOT`).
    pub const EOT: BufferFlags = BufferFlags(0x2);
    /// Keep default-ignorable characters as glyphs
    /// (`HB_BUFFER_FLAG_PRESERVE_DEFAULT_IGNORABLES`).
    pub const PRESERVE_DEFAULT_IGNORABLES: BufferFlags = BufferFlags(0x4);
    /// Remove default-ignorable characters instead of showing them as
    /// the invisible glyph (`HB_BUFFER_FLAG_REMOVE_DEFAULT_IGNORABLES`).
    pub const REMOVE_DEFAULT_IGNORABLES: BufferFlags = BufferFlags(0x8);
    /// Never insert a dotted circle
    /// (`HB_BUFFER_FLAG_DO_NOT_INSERT_DOTTED_CIRCLE`).
    pub const DO_NOT_INSERT_DOTTED_CIRCLE: BufferFlags = BufferFlags(0x10);
    /// Set [`GlyphFlags::UNSAFE_TO_CONCAT`]
    /// (`HB_BUFFER_FLAG_PRODUCE_UNSAFE_TO_CONCAT`).
    pub const PRODUCE_UNSAFE_TO_CONCAT: BufferFlags = BufferFlags(0x40);
    /// Set [`GlyphFlags::SAFE_TO_INSERT_TATWEEL`]
    /// (`HB_BUFFER_FLAG_PRODUCE_SAFE_TO_INSERT_TATWEEL`).
    pub const PRODUCE_SAFE_TO_INSERT_TATWEEL: BufferFlags = BufferFlags(0x80);

    /// Whether every flag of `other` is set.
    pub const fn contains(self, other: BufferFlags) -> bool {
        self.0 & other.0 == other.0
    }
    /// The union.
    pub const fn union(self, other: BufferFlags) -> BufferFlags {
        BufferFlags(self.0 | other.0)
    }
}

/// How characters are grouped into clusters
/// (`hb_buffer_cluster_level_t`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ClusterLevel {
    /// Graphemes, with monotone cluster values
    /// (`HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES`, the default).
    #[default]
    MonotoneGraphemes,
    /// Characters, with marks merged into their base and monotone
    /// cluster values (`HB_BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS`).
    MonotoneCharacters,
    /// Characters, without merging (`HB_BUFFER_CLUSTER_LEVEL_CHARACTERS`).
    Characters,
    /// Graphemes, with cluster values that need not be monotone
    /// (`HB_BUFFER_CLUSTER_LEVEL_GRAPHEMES`).
    Graphemes,
}

#[allow(dead_code)] // Some items serve the shapers of ADR 0024.
impl ClusterLevel {
    /// `HB_BUFFER_CLUSTER_LEVEL_IS_MONOTONE`.
    pub(crate) fn is_monotone(self) -> bool {
        matches!(
            self,
            ClusterLevel::MonotoneGraphemes | ClusterLevel::MonotoneCharacters
        )
    }
    /// `HB_BUFFER_CLUSTER_LEVEL_IS_GRAPHEMES`.
    pub(crate) fn is_graphemes(self) -> bool {
        matches!(
            self,
            ClusterLevel::MonotoneGraphemes | ClusterLevel::Graphemes
        )
    }
    /// `HB_BUFFER_CLUSTER_LEVEL_IS_CHARACTERS`.
    pub(crate) fn is_characters(self) -> bool {
        !self.is_graphemes()
    }
}

/// The number of context characters kept on each side
/// (`hb_buffer_t::CONTEXT_LENGTH`).
pub(crate) const CONTEXT_LENGTH: usize = 5;
/// The replacement character for ill-formed text
/// (`HB_BUFFER_REPLACEMENT_CODEPOINT_DEFAULT`).
pub const REPLACEMENT_CHARACTER: u32 = 0xFFFD;

/// `HB_BUFFER_MAX_LEN_FACTOR`.
pub const MAX_LEN_FACTOR: usize = 256;
/// `HB_BUFFER_MAX_LEN_MIN`.
pub const MAX_LEN_MIN: usize = 65536;
/// `HB_BUFFER_MAX_LEN_DEFAULT`.
pub const MAX_LEN_DEFAULT: usize = 0x3FFF_FFFF;
/// `HB_BUFFER_MAX_OPS_FACTOR`.
pub const MAX_OPS_FACTOR: i32 = 4096;
/// `HB_BUFFER_MAX_OPS_MIN`.
pub const MAX_OPS_MIN: i32 = 65536;
/// `HB_BUFFER_MAX_OPS_DEFAULT`.
pub const MAX_OPS_DEFAULT: i32 = 0x1FFF_FFFF;

/// `unicode_props` bits (`hb_unicode_props_flags_t`).
pub(crate) mod uprops {
    pub(crate) const GEN_CAT: u16 = 0x001F;
    pub(crate) const IGNORABLE: u16 = 0x0020;
    pub(crate) const HIDDEN: u16 = 0x0040;
    pub(crate) const CONTINUATION: u16 = 0x0080;
    pub(crate) const CF_ZWJ: u16 = 0x0100;
    pub(crate) const CF_ZWNJ: u16 = 0x0200;
    pub(crate) const CF_VS: u16 = 0x0400;
}

/// `glyph_props` bits (`hb_ot_layout_glyph_props_flags_t`).
pub(crate) mod gprops {
    pub(crate) const BASE_GLYPH: u16 = 0x02;
    pub(crate) const LIGATURE: u16 = 0x04;
    pub(crate) const MARK: u16 = 0x08;
    #[allow(dead_code)]
    pub(crate) const CLASS_MASK: u16 = BASE_GLYPH | LIGATURE | MARK;
    pub(crate) const SUBSTITUTED: u16 = 0x10;
    pub(crate) const LIGATED: u16 = 0x20;
    pub(crate) const MULTIPLIED: u16 = 0x40;
    pub(crate) const PRESERVE: u16 = SUBSTITUTED | LIGATED | MULTIPLIED;
}

/// `scratch_flags` bits (`hb_buffer_scratch_flags_t`).
pub(crate) mod scratch {
    pub(crate) const HAS_FRACTION_SLASH: u32 = 0x0000_0001;
    pub(crate) const HAS_DEFAULT_IGNORABLES: u32 = 0x0000_0002;
    pub(crate) const HAS_SPACE_FALLBACK: u32 = 0x0000_0004;
    pub(crate) const HAS_GPOS_ATTACHMENT: u32 = 0x0000_0008;
    pub(crate) const HAS_CGJ: u32 = 0x0000_0010;
    pub(crate) const HAS_VARIATION_SELECTOR_FALLBACK: u32 = 0x0000_0040;
    pub(crate) const HAS_CONTINUATIONS: u32 = 0x0000_0080;
}

/// `IS_LIG_BASE` of `lig_props`.
const IS_LIG_BASE: u8 = 0x10;

/// An item of the working buffer: a character, later a glyph, with the
/// per-item state of the shaping steps (HarfBuzz's `hb_glyph_info_t`
/// with its `var1` and `var2` unions split into named fields).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Info {
    /// The character, and from the end of normalization on the glyph.
    pub(crate) codepoint: u32,
    /// The feature mask; the low bits are the [`GlyphFlags`].
    pub(crate) mask: Mask,
    /// The cluster value.
    pub(crate) cluster: u32,
    /// The glyph normalization chose (`normalizer_glyph_index`).
    pub(crate) glyph_index: u32,
    /// General category, ignorability, continuation and the combining
    /// class or format flags (`unicode_props`).
    pub(crate) unicode_props: u16,
    /// The GDEF class and substitution marks (`glyph_props`).
    pub(crate) glyph_props: u16,
    /// Ligature id and component (`lig_props`).
    pub(crate) lig_props: u8,
    /// The syllable serial of a shaper (`syllable`).
    pub(crate) syllable: u8,
    /// A shaper's character category (ADR 0024).
    pub(crate) shaper_category: u8,
    /// A shaper's character position (ADR 0024).
    pub(crate) shaper_position: u8,
}

/// The position of an item, with the attachment chain of `GPOS`
/// (HarfBuzz's `hb_glyph_position_t` with its `var`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Pos {
    pub(crate) x_advance: i32,
    pub(crate) y_advance: i32,
    pub(crate) x_offset: i32,
    pub(crate) y_offset: i32,
    /// The glyph this one attaches to, relative to it; 0 for none.
    pub(crate) attach_chain: i16,
    /// The attachment type (`ATTACH_TYPE_*` of `GPOS`).
    pub(crate) attach_type: u8,
}

#[allow(dead_code)] // Some items serve the shapers of ADR 0024.
impl Info {
    /// The general category (`_hb_glyph_info_get_general_category`).
    pub(crate) fn general_category(&self) -> GeneralCategory {
        let v = self.unicode_props & uprops::GEN_CAT;
        unicode::category_from_u8(u8::try_from(v).unwrap_or(2))
    }
    /// `_hb_glyph_info_set_general_category`: clears the top byte.
    pub(crate) fn set_general_category(&mut self, gc: GeneralCategory) {
        self.unicode_props = u16::from(gc as u8) | (self.unicode_props & (0xFF & !uprops::GEN_CAT));
    }
    /// `_hb_glyph_info_is_unicode_mark`.
    pub(crate) fn is_unicode_mark(&self) -> bool {
        unicode::is_mark_category(self.general_category())
    }
    /// `_hb_glyph_info_set_modified_combining_class`.
    pub(crate) fn set_modified_combining_class(&mut self, class: u8) {
        if !self.is_unicode_mark() {
            return;
        }
        self.unicode_props = (u16::from(class) << 8) | (self.unicode_props & 0xFF);
    }
    /// `_hb_glyph_info_get_modified_combining_class`.
    pub(crate) fn modified_combining_class(&self) -> u8 {
        if self.is_unicode_mark() {
            self.unicode_props.to_be_bytes()[0]
        } else {
            0
        }
    }
    /// `_hb_glyph_info_is_unicode_space`.
    pub(crate) fn is_unicode_space(&self) -> bool {
        self.general_category() == GeneralCategory::SpaceSeparator
    }
    /// `_hb_glyph_info_set_unicode_space_fallback_type`.
    pub(crate) fn set_space_fallback(&mut self, s: unicode::SpaceFallback) {
        if !self.is_unicode_space() {
            return;
        }
        self.unicode_props = (u16::from(s as u8) << 8) | (self.unicode_props & 0xFF);
    }
    /// `_hb_glyph_info_get_unicode_space_fallback_type`.
    pub(crate) fn space_fallback(&self) -> unicode::SpaceFallback {
        if self.is_unicode_space() {
            unicode::SpaceFallback::from_u8(self.unicode_props.to_be_bytes()[0])
        } else {
            unicode::SpaceFallback::NotSpace
        }
    }
    /// `_hb_glyph_info_is_variation_selector`.
    pub(crate) fn is_variation_selector(&self) -> bool {
        self.general_category() == GeneralCategory::Format
            && self.unicode_props & uprops::CF_VS != 0
    }
    /// `_hb_glyph_info_set_variation_selector`.
    pub(crate) fn set_variation_selector(&mut self, customize: bool) {
        if customize {
            self.set_general_category(GeneralCategory::Format);
            self.unicode_props |= uprops::CF_VS;
        } else {
            self.set_general_category(GeneralCategory::NonspacingMark);
        }
    }
    /// `_hb_glyph_info_is_default_ignorable`.
    pub(crate) fn is_default_ignorable(&self) -> bool {
        self.unicode_props & uprops::IGNORABLE != 0 && !self.substituted()
    }
    /// `_hb_glyph_info_clear_default_ignorable`.
    pub(crate) fn clear_default_ignorable(&mut self) {
        self.unicode_props &= !uprops::IGNORABLE;
    }
    /// `_hb_glyph_info_is_hidden`.
    pub(crate) fn is_hidden(&self) -> bool {
        self.unicode_props & uprops::HIDDEN != 0
    }
    /// `_hb_glyph_info_unhide`.
    pub(crate) fn unhide(&mut self) {
        self.unicode_props &= !uprops::HIDDEN;
    }
    /// `_hb_glyph_info_is_continuation`.
    pub(crate) fn is_continuation(&self) -> bool {
        self.unicode_props & uprops::CONTINUATION != 0
    }
    /// `_hb_glyph_info_clear_continuation`.
    pub(crate) fn clear_continuation(&mut self) {
        self.unicode_props &= !uprops::CONTINUATION;
    }
    /// `_hb_glyph_info_is_unicode_format`.
    pub(crate) fn is_unicode_format(&self) -> bool {
        self.general_category() == GeneralCategory::Format
    }
    /// `_hb_glyph_info_is_zwnj`.
    pub(crate) fn is_zwnj(&self) -> bool {
        self.is_unicode_format() && self.unicode_props & uprops::CF_ZWNJ != 0
    }
    /// `_hb_glyph_info_is_zwj`.
    pub(crate) fn is_zwj(&self) -> bool {
        self.is_unicode_format() && self.unicode_props & uprops::CF_ZWJ != 0
    }

    /// `_hb_glyph_info_set_lig_props_for_ligature`.
    pub(crate) fn set_lig_props_for_ligature(&mut self, lig_id: u8, num_comps: u8) {
        self.lig_props = (lig_id << 5) | IS_LIG_BASE | (num_comps & 0x0F);
    }
    /// `_hb_glyph_info_set_lig_props_for_mark`.
    pub(crate) fn set_lig_props_for_mark(&mut self, lig_id: u8, comp: u8) {
        self.lig_props = (lig_id << 5) | (comp & 0x0F);
    }
    /// `_hb_glyph_info_set_lig_props_for_component`.
    pub(crate) fn set_lig_props_for_component(&mut self, comp: u8) {
        self.set_lig_props_for_mark(0, comp);
    }
    /// `_hb_glyph_info_get_lig_id`.
    pub(crate) fn lig_id(&self) -> u8 {
        self.lig_props >> 5
    }
    /// `_hb_glyph_info_ligated_internal`.
    fn ligated_internal(&self) -> bool {
        self.lig_props & IS_LIG_BASE != 0
    }
    /// `_hb_glyph_info_get_lig_comp`.
    pub(crate) fn lig_comp(&self) -> u8 {
        if self.ligated_internal() {
            0
        } else {
            self.lig_props & 0x0F
        }
    }
    /// `_hb_glyph_info_get_lig_num_comps`.
    pub(crate) fn lig_num_comps(&self) -> u8 {
        if self.glyph_props & gprops::LIGATURE != 0 && self.ligated_internal() {
            self.lig_props & 0x0F
        } else {
            1
        }
    }
    /// `_hb_glyph_info_get_lig_num_comps_in_ligation`.
    pub(crate) fn lig_num_comps_in_ligation(&self) -> u8 {
        if self.multiplied() && self.lig_comp() != 0 {
            0
        } else {
            self.lig_num_comps()
        }
    }
    /// `_hb_glyph_info_is_base_glyph`.
    pub(crate) fn is_base_glyph(&self) -> bool {
        self.glyph_props & gprops::BASE_GLYPH != 0
    }
    /// `_hb_glyph_info_is_ligature`.
    pub(crate) fn is_ligature(&self) -> bool {
        self.glyph_props & gprops::LIGATURE != 0
    }
    /// `_hb_glyph_info_is_mark`.
    pub(crate) fn is_mark(&self) -> bool {
        self.glyph_props & gprops::MARK != 0
    }
    /// `_hb_glyph_info_substituted`.
    pub(crate) fn substituted(&self) -> bool {
        self.glyph_props & gprops::SUBSTITUTED != 0
    }
    /// `_hb_glyph_info_ligated`.
    pub(crate) fn ligated(&self) -> bool {
        self.glyph_props & gprops::LIGATED != 0
    }
    /// `_hb_glyph_info_multiplied`.
    pub(crate) fn multiplied(&self) -> bool {
        self.glyph_props & gprops::MULTIPLIED != 0
    }
    /// `_hb_glyph_info_ligated_and_didnt_multiply`.
    pub(crate) fn ligated_and_didnt_multiply(&self) -> bool {
        self.ligated() && !self.multiplied()
    }
    /// `_hb_glyph_info_clear_ligated_and_multiplied`.
    pub(crate) fn clear_ligated_and_multiplied(&mut self) {
        self.glyph_props &= !(gprops::LIGATED | gprops::MULTIPLIED);
    }
    /// `_hb_glyph_info_clear_substituted`.
    pub(crate) fn clear_substituted(&mut self) {
        self.glyph_props &= !gprops::SUBSTITUTED;
    }
}

/// A growable array that reads a zero item and writes to a scratch item
/// past its end instead of panicking (HarfBuzz's `Null` and `Crap`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Items<T> {
    pub(crate) v: Vec<T>,
    null: T,
    crap: T,
}

impl<T: Copy + Default> Items<T> {
    pub(crate) fn new() -> Items<T> {
        Items {
            v: Vec::new(),
            null: T::default(),
            crap: T::default(),
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.v.len()
    }
    /// Items `start..end`, clamped to the array.
    pub(crate) fn range(&self, start: usize, end: usize) -> &[T] {
        let end = end.min(self.v.len());
        self.v.get(start.min(end)..end).unwrap_or_default()
    }
    /// Items `start..end`, clamped to the array.
    pub(crate) fn range_mut(&mut self, start: usize, end: usize) -> &mut [T] {
        let end = end.min(self.v.len());
        self.v.get_mut(start.min(end)..end).unwrap_or_default()
    }
}

impl<T> Index<usize> for Items<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        self.v.get(i).unwrap_or(&self.null)
    }
}

impl<T> IndexMut<usize> for Items<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        match self.v.get_mut(i) {
            Some(x) => x,
            None => &mut self.crap,
        }
    }
}

/// The working buffer (`hb_buffer_t`).
#[derive(Clone, Debug)]
pub(crate) struct Buffer {
    pub(crate) flags: BufferFlags,
    pub(crate) cluster_level: ClusterLevel,
    pub(crate) replacement: u32,
    pub(crate) invisible: u32,
    pub(crate) not_found: u32,
    pub(crate) not_found_variation_selector: Option<u32>,
    pub(crate) random_state: u32,

    /// The direction while shaping.
    pub(crate) direction: Direction,
    /// The script while shaping; `None` is `HB_SCRIPT_INVALID`.
    pub(crate) script: Option<Script>,

    pub(crate) successful: bool,
    pub(crate) have_output: bool,
    pub(crate) have_separate_output: bool,
    pub(crate) have_positions: bool,

    pub(crate) idx: usize,
    pub(crate) len: usize,
    pub(crate) out_len: usize,

    pub(crate) info: Items<Info>,
    /// The output array while it is separate from `info`.
    pub(crate) out: Items<Info>,
    pub(crate) pos: Items<Pos>,

    /// Pre-context (outward, index 0 nearest) and post-context.
    pub(crate) context: [[u32; CONTEXT_LENGTH]; 2],
    pub(crate) context_len: [usize; 2],

    pub(crate) serial: u8,
    pub(crate) scratch_flags: u32,
    pub(crate) max_len: usize,
    pub(crate) max_ops: i32,
    /// The digest of the glyphs in the buffer (`digest`).
    pub(crate) digest: Digest,
}

impl Default for Buffer {
    fn default() -> Buffer {
        Buffer::new()
    }
}

#[allow(dead_code)] // Some items serve the shapers of ADR 0024.
impl Buffer {
    pub(crate) fn new() -> Buffer {
        Buffer {
            flags: BufferFlags::DEFAULT,
            cluster_level: ClusterLevel::MonotoneGraphemes,
            replacement: REPLACEMENT_CHARACTER,
            invisible: 0,
            not_found: 0,
            not_found_variation_selector: None,
            random_state: 1,
            direction: Direction::Ltr,
            script: None,
            successful: true,
            have_output: false,
            have_separate_output: false,
            have_positions: false,
            idx: 0,
            len: 0,
            out_len: 0,
            info: Items::new(),
            out: Items::new(),
            pos: Items::new(),
            context: [[0; CONTEXT_LENGTH]; 2],
            context_len: [0; 2],
            serial: 0,
            scratch_flags: 0,
            max_len: MAX_LEN_DEFAULT,
            max_ops: MAX_OPS_DEFAULT,
            digest: Digest::default(),
        }
    }

    /// `hb_buffer_t::clear`: the contents go, the settings stay.
    pub(crate) fn clear(&mut self) {
        self.direction = Direction::Ltr;
        self.script = None;
        self.successful = true;
        self.have_output = false;
        self.have_separate_output = false;
        self.have_positions = false;
        self.idx = 0;
        self.len = 0;
        self.out_len = 0;
        self.info.v.clear();
        self.out.v.clear();
        self.pos.v.clear();
        self.context = [[0; CONTEXT_LENGTH]; 2];
        self.context_len = [0; 2];
        self.serial = 0;
        self.random_state = 1;
        self.scratch_flags = 0;
    }

    /// The items `0..len`.
    pub(crate) fn infos(&self) -> &[Info] {
        self.info.range(0, self.len)
    }
    /// The items `0..len`.
    pub(crate) fn infos_mut(&mut self) -> &mut [Info] {
        self.info.range_mut(0, self.len)
    }
    /// The items and positions `0..len`.
    pub(crate) fn infos_and_pos_mut(&mut self) -> (&mut [Info], &mut [Pos]) {
        let len = self.len;
        (self.info.range_mut(0, len), self.pos.range_mut(0, len))
    }
    /// The positions `0..len`.
    pub(crate) fn positions_mut(&mut self) -> &mut [Pos] {
        let len = self.len;
        self.pos.range_mut(0, len)
    }

    /// The output array.
    pub(crate) fn out_info(&self) -> &Items<Info> {
        if self.have_separate_output {
            &self.out
        } else {
            &self.info
        }
    }
    /// The output array.
    pub(crate) fn out_info_mut(&mut self) -> &mut Items<Info> {
        if self.have_separate_output {
            &mut self.out
        } else {
            &mut self.info
        }
    }

    /// The current item (`cur`).
    pub(crate) fn cur(&self, i: usize) -> &Info {
        &self.info[self.idx.wrapping_add(i)]
    }
    /// The current item (`cur`).
    pub(crate) fn cur_mut(&mut self, i: usize) -> &mut Info {
        let at = self.idx.wrapping_add(i);
        &mut self.info[at]
    }
    /// The current position (`cur_pos`).
    pub(crate) fn cur_pos_mut(&mut self) -> &mut Pos {
        let at = self.idx;
        &mut self.pos[at]
    }
    /// The last output item (`prev`).
    pub(crate) fn prev(&self) -> &Info {
        &self.out_info()[self.out_len.wrapping_sub(1)]
    }
    /// The last output item (`prev`).
    pub(crate) fn prev_mut(&mut self) -> &mut Info {
        let at = self.out_len.wrapping_sub(1);
        &mut self.out_info_mut()[at]
    }

    /// `backtrack_len`.
    pub(crate) fn backtrack_len(&self) -> usize {
        if self.have_output {
            self.out_len
        } else {
            self.idx
        }
    }
    /// `lookahead_len`.
    pub(crate) fn lookahead_len(&self) -> usize {
        self.len.saturating_sub(self.idx)
    }

    /// `next_serial`.
    pub(crate) fn next_serial(&mut self) -> u8 {
        self.serial = self.serial.wrapping_add(1);
        if self.serial == 0 {
            self.serial = 1;
        }
        self.serial
    }

    /// `_hb_allocate_lig_id`.
    pub(crate) fn allocate_lig_id(&mut self) -> u8 {
        loop {
            let id = self.next_serial() & 0x07;
            if id != 0 {
                return id;
            }
        }
    }

    /// `hb_buffer_t::add`.
    pub(crate) fn add(&mut self, codepoint: u32, cluster: u32) {
        if !self.ensure(self.len.wrapping_add(1)) {
            return;
        }
        let at = self.len;
        self.info[at] = Info {
            codepoint,
            cluster,
            ..Info::default()
        };
        self.len = at.wrapping_add(1);
    }

    /// `hb_buffer_t::add_info`.
    pub(crate) fn add_info(&mut self, info: Info) {
        if !self.ensure(self.len.wrapping_add(1)) {
            return;
        }
        let at = self.len;
        self.info[at] = info;
        self.len = at.wrapping_add(1);
    }

    /// Adds text as `hb_buffer_add_utf` does.
    pub(crate) fn add_utf<U: Utf>(
        &mut self,
        text: &[U::Unit],
        item_offset: usize,
        item_len: Option<usize>,
    ) {
        let text_len = text.len();
        let item_offset = item_offset.min(text_len);
        let item_len = item_len
            .unwrap_or(usize::MAX)
            .min(text_len.wrapping_sub(item_offset));
        if item_len > 0x7FFF_FFFF / 8 {
            return;
        }
        let unit = core::mem::size_of::<U::Unit>();
        if !self.ensure(self.len.wrapping_add(item_len.wrapping_mul(unit) / 4)) {
            return;
        }
        let replacement = self.replacement;
        if self.len == 0 && item_offset > 0 {
            self.context_len[0] = 0;
            let mut prev = item_offset;
            while prev > 0 && self.context_len[0] < CONTEXT_LENGTH {
                let Some((u, n)) = text.get(..prev).and_then(|t| U::prev(t, replacement)) else {
                    break;
                };
                prev = prev.saturating_sub(n);
                let k = self.context_len[0];
                if let Some(slot) = self.context[0].get_mut(k) {
                    *slot = u;
                }
                self.context_len[0] = k.wrapping_add(1);
            }
        }
        let end = item_offset.wrapping_add(item_len);
        let mut next = item_offset;
        while next < end {
            let Some((u, n)) = text.get(next..end).and_then(|t| U::next(t, replacement)) else {
                break;
            };
            self.add(u, u32::try_from(next).unwrap_or(u32::MAX));
            next = next.wrapping_add(n);
        }
        self.context_len[1] = 0;
        while next < text_len && self.context_len[1] < CONTEXT_LENGTH {
            let Some((u, n)) = text.get(next..).and_then(|t| U::next(t, replacement)) else {
                break;
            };
            let k = self.context_len[1];
            if let Some(slot) = self.context[1].get_mut(k) {
                *slot = u;
            }
            self.context_len[1] = k.wrapping_add(1);
            next = next.wrapping_add(n);
        }
    }

    /// `ensure`: room for `size` items.
    #[must_use]
    pub(crate) fn ensure(&mut self, size: usize) -> bool {
        if size == 0 || size < self.info.len() {
            return true;
        }
        self.enlarge(size)
    }

    /// `enlarge`: grows the arrays as HarfBuzz does, failing past
    /// `max_len`.
    #[must_use]
    fn enlarge(&mut self, size: usize) -> bool {
        if size > self.max_len {
            self.successful = false;
            return false;
        }
        if !self.successful {
            return false;
        }
        let mut allocated = self.info.len();
        while size >= allocated {
            allocated = allocated.saturating_add((allocated >> 1).saturating_add(32));
        }
        self.info.v.resize(allocated, Info::default());
        self.pos.v.resize(allocated, Pos::default());
        if self.have_separate_output {
            self.out.v.resize(allocated, Info::default());
        }
        true
    }

    /// `make_room_for`.
    #[must_use]
    pub(crate) fn make_room_for(&mut self, num_in: usize, num_out: usize) -> bool {
        if !self.ensure(self.out_len.wrapping_add(num_out)) {
            return false;
        }
        if !self.have_separate_output
            && self.out_len.wrapping_add(num_out) > self.idx.wrapping_add(num_in)
        {
            self.have_separate_output = true;
            let n = self.info.len();
            self.out.v.clear();
            self.out.v.resize(n, Info::default());
            let out_len = self.out_len;
            let (src, dst) = (self.info.range(0, out_len), self.out.range_mut(0, out_len));
            dst.copy_from_slice(src.get(..dst.len()).unwrap_or_default());
        }
        true
    }

    /// `shift_forward`.
    #[must_use]
    fn shift_forward(&mut self, count: usize) -> bool {
        if !self.ensure(self.len.wrapping_add(count)) {
            return false;
        }
        self.charge(self.len.wrapping_sub(self.idx));
        if self.max_ops < 0 {
            self.successful = false;
            return false;
        }
        let (idx, len) = (self.idx, self.len);
        let end = len.wrapping_add(count);
        if idx <= len && end <= self.info.v.len() {
            self.info.v.copy_within(idx..len, idx.wrapping_add(count));
        }
        if idx.wrapping_add(count) > len {
            for item in self.info.range_mut(len, idx.wrapping_add(count)) {
                *item = Info::default();
            }
        }
        self.len = end;
        self.idx = idx.wrapping_add(count);
        true
    }

    /// Lowers `max_ops` by `n`.
    pub(crate) fn charge(&mut self, n: usize) {
        let n = i32::try_from(n).unwrap_or(i32::MAX);
        self.max_ops = self.max_ops.saturating_sub(n);
    }

    /// `clear_output`.
    pub(crate) fn clear_output(&mut self) {
        self.have_output = true;
        self.have_positions = false;
        self.idx = 0;
        self.out_len = 0;
        self.have_separate_output = false;
    }

    /// `clear_positions`.
    pub(crate) fn clear_positions(&mut self) {
        self.have_output = false;
        self.have_positions = true;
        self.out_len = 0;
        self.have_separate_output = false;
        let len = self.len;
        for p in self.pos.range_mut(0, len) {
            *p = Pos::default();
        }
    }

    /// `sync`.
    pub(crate) fn sync(&mut self) -> bool {
        let ok = self.successful && self.next_glyphs(self.len.wrapping_sub(self.idx));
        if ok {
            if self.have_separate_output {
                core::mem::swap(&mut self.info, &mut self.out);
            }
            self.len = self.out_len;
        }
        self.have_output = false;
        self.have_separate_output = false;
        self.out_len = 0;
        self.idx = 0;
        ok
    }

    /// `next_glyph`.
    pub(crate) fn next_glyph(&mut self) -> bool {
        if self.have_output {
            if self.have_separate_output || self.out_len != self.idx {
                if !self.ensure(self.out_len.wrapping_add(1)) {
                    return false;
                }
                let item = self.info[self.idx];
                let at = self.out_len;
                self.out_info_mut()[at] = item;
            }
            self.out_len = self.out_len.wrapping_add(1);
        }
        self.idx = self.idx.wrapping_add(1);
        true
    }

    /// `next_glyphs`.
    pub(crate) fn next_glyphs(&mut self, n: usize) -> bool {
        if self.have_output {
            if self.have_separate_output || self.out_len != self.idx {
                if !self.ensure(self.out_len.wrapping_add(n)) {
                    return false;
                }
                self.copy_to_out(self.idx, self.out_len, n);
            }
            self.out_len = self.out_len.wrapping_add(n);
        }
        self.idx = self.idx.wrapping_add(n);
        true
    }

    /// Copies `info[src..src + n]` to `out_info[dst..dst + n]` (memmove).
    fn copy_to_out(&mut self, src: usize, dst: usize, n: usize) {
        if self.have_separate_output {
            let from = self.info.range(src, src.wrapping_add(n));
            let to = self.out.range_mut(dst, dst.wrapping_add(n));
            let k = from.len().min(to.len());
            if let (Some(t), Some(f)) = (to.get_mut(..k), from.get(..k)) {
                t.copy_from_slice(f);
            }
        } else {
            let len = self.info.v.len();
            let end = src.wrapping_add(n).min(len);
            if src < end && dst.wrapping_add(end.wrapping_sub(src)) <= len {
                self.info.v.copy_within(src..end, dst);
            }
        }
    }

    /// Copies `out_info[src..src + n]` to `info[dst..dst + n]` (memmove).
    fn copy_from_out(&mut self, src: usize, dst: usize, n: usize) {
        if self.have_separate_output {
            let from = self.out.range(src, src.wrapping_add(n));
            let to = self.info.range_mut(dst, dst.wrapping_add(n));
            let k = from.len().min(to.len());
            if let (Some(t), Some(f)) = (to.get_mut(..k), from.get(..k)) {
                t.copy_from_slice(f);
            }
        } else {
            let len = self.info.v.len();
            let end = src.wrapping_add(n).min(len);
            if src < end && dst.wrapping_add(end.wrapping_sub(src)) <= len {
                self.info.v.copy_within(src..end, dst);
            }
        }
    }

    /// `skip_glyph`.
    pub(crate) fn skip_glyph(&mut self) {
        self.idx = self.idx.wrapping_add(1);
    }

    /// `replace_glyphs`.
    pub(crate) fn replace_glyphs(&mut self, num_in: usize, num_out: usize, glyphs: &[u32]) -> bool {
        if !self.make_room_for(num_in, num_out) {
            return false;
        }
        self.merge_clusters(self.idx, self.idx.wrapping_add(num_in));
        let orig = if self.idx < self.len {
            *self.cur(0)
        } else {
            *self.prev()
        };
        let out_len = self.out_len;
        for (k, &g) in glyphs.iter().take(num_out).enumerate() {
            let item = &mut self.out_info_mut()[out_len.wrapping_add(k)];
            *item = orig;
            item.codepoint = g;
        }
        self.idx = self.idx.wrapping_add(num_in);
        self.out_len = out_len.wrapping_add(num_out);
        true
    }

    /// `replace_glyph`.
    pub(crate) fn replace_glyph(&mut self, glyph: u32) -> bool {
        self.replace_glyphs(1, 1, &[glyph])
    }

    /// `output_glyph`.
    pub(crate) fn output_glyph(&mut self, glyph: u32) -> bool {
        self.replace_glyphs(0, 1, &[glyph])
    }

    /// `output_info`.
    pub(crate) fn output_info(&mut self, info: Info) -> bool {
        if !self.make_room_for(0, 1) {
            return false;
        }
        let at = self.out_len;
        self.out_info_mut()[at] = info;
        self.out_len = at.wrapping_add(1);
        true
    }

    /// `copy_glyph`.
    pub(crate) fn copy_glyph(&mut self) -> bool {
        let item = *self.cur(0);
        self.output_info(item)
    }

    /// `move_to`: `i` is an output index.
    pub(crate) fn move_to(&mut self, i: usize) -> bool {
        if !self.have_output {
            self.idx = i;
            return true;
        }
        if !self.successful {
            return false;
        }
        if self.out_len < i {
            let count = i.wrapping_sub(self.out_len);
            if !self.make_room_for(count, count) {
                return false;
            }
            self.charge(count);
            if self.max_ops < 0 {
                self.successful = false;
                return false;
            }
            self.copy_to_out(self.idx, self.out_len, count);
            self.idx = self.idx.wrapping_add(count);
            self.out_len = self.out_len.wrapping_add(count);
        } else if self.out_len > i {
            let count = self.out_len.wrapping_sub(i);
            if self.idx < count && !self.shift_forward(count.wrapping_sub(self.idx)) {
                return false;
            }
            self.charge(count);
            if self.max_ops < 0 {
                self.successful = false;
                return false;
            }
            self.idx = self.idx.wrapping_sub(count);
            self.out_len = self.out_len.wrapping_sub(count);
            self.copy_from_out(self.out_len, self.idx, count);
        }
        true
    }

    /// `sync_so_far`.
    pub(crate) fn sync_so_far(&mut self) -> isize {
        let had_output = self.have_output;
        let out_i = self.out_len;
        let i = self.idx;
        let old_idx = self.idx;
        self.idx = if self.sync() { out_i } else { i };
        if had_output {
            self.have_output = true;
            self.out_len = self.idx;
        }
        isize::try_from(self.idx)
            .unwrap_or(0)
            .wrapping_sub(isize::try_from(old_idx).unwrap_or(0))
    }

    /// `set_masks`.
    pub(crate) fn set_masks(&mut self, value: Mask, mask: Mask, start: u32, end: u32) {
        if mask == 0 {
            return;
        }
        let not_mask = !mask;
        let value = value & mask;
        self.charge(self.len);
        if self.max_ops < 0 {
            self.successful = false;
        }
        let global = start == 0 && end == u32::MAX;
        for info in self.infos_mut() {
            if global || (start <= info.cluster && info.cluster < end) {
                info.mask = (info.mask & not_mask) | value;
            }
        }
    }

    /// `update_digest`.
    pub(crate) fn update_digest(&mut self) {
        let mut d = Digest::default();
        for info in self.infos() {
            d.add(info.codepoint);
        }
        self.digest = d;
    }

    /// `clear_glyph_flags`.
    pub(crate) fn clear_glyph_flags(&mut self) {
        for info in self.infos_mut() {
            info.mask &= !GlyphFlags::DEFINED.0;
        }
    }

    /// `add_masks`.
    pub(crate) fn add_masks(&mut self, mask: Mask) {
        for info in self.infos_mut() {
            info.mask |= mask;
        }
    }

    /// `set_cluster`.
    pub(crate) fn set_cluster(info: &mut Info, cluster: u32, mask: Mask) {
        if info.cluster != cluster {
            info.mask = (info.mask & !GlyphFlags::DEFINED.0) | (mask & GlyphFlags::DEFINED.0);
        }
        info.cluster = cluster;
    }

    /// `merge_clusters`.
    pub(crate) fn merge_clusters(&mut self, start: usize, end: usize) {
        if end.wrapping_sub(start) < 2 || end < start {
            return;
        }
        if !self.cluster_level.is_monotone() {
            self.unsafe_to_break(start, end);
            return;
        }
        self.merge_clusters_impl(start, end);
    }

    /// `merge_grapheme_clusters`.
    pub(crate) fn merge_grapheme_clusters(&mut self, start: usize, end: usize) {
        if end.wrapping_sub(start) < 2 || end < start {
            return;
        }
        if !self.cluster_level.is_graphemes() {
            self.unsafe_to_break(start, end);
            return;
        }
        self.merge_clusters_impl(start, end);
    }

    fn merge_clusters_impl(&mut self, mut start: usize, mut end: usize) {
        let mut cluster = self.info[start].cluster;
        for i in start.wrapping_add(1)..end {
            cluster = cluster.min(self.info[i].cluster);
        }
        if self.successful {
            if cluster != self.info[end.wrapping_sub(1)].cluster {
                while end < self.len
                    && self.info[end.wrapping_sub(1)].cluster == self.info[end].cluster
                {
                    end = end.wrapping_add(1);
                }
            }
            if cluster != self.info[start].cluster {
                while self.idx < start
                    && self.info[start.wrapping_sub(1)].cluster == self.info[start].cluster
                {
                    start = start.wrapping_sub(1);
                }
            }
            if self.idx == start && self.info[start].cluster != cluster {
                let target = self.info[start].cluster;
                let mut i = self.out_len;
                while i > 0 && self.out_info()[i.wrapping_sub(1)].cluster == target {
                    let at = i.wrapping_sub(1);
                    Buffer::set_cluster(&mut self.out_info_mut()[at], cluster, 0);
                    i = at;
                }
                self.charge(self.out_len.wrapping_sub(i));
            }
        }
        self.charge(end.wrapping_sub(start));
        if self.max_ops < 0 {
            self.successful = false;
        }
        for i in start..end {
            Buffer::set_cluster(&mut self.info[i], cluster, 0);
        }
    }

    /// `merge_out_clusters`.
    pub(crate) fn merge_out_clusters(&mut self, start: usize, end: usize) {
        if end.wrapping_sub(start) < 2 || end < start {
            return;
        }
        if !self.cluster_level.is_monotone() {
            return;
        }
        self.merge_out_clusters_impl(start, end);
    }

    /// `merge_out_grapheme_clusters`.
    pub(crate) fn merge_out_grapheme_clusters(&mut self, start: usize, end: usize) {
        if end.wrapping_sub(start) < 2 || end < start {
            return;
        }
        if !self.cluster_level.is_graphemes() {
            return;
        }
        self.merge_out_clusters_impl(start, end);
    }

    fn merge_out_clusters_impl(&mut self, mut start: usize, mut end: usize) {
        let mut cluster = self.out_info()[start].cluster;
        for i in start.wrapping_add(1)..end {
            cluster = cluster.min(self.out_info()[i].cluster);
        }
        if self.successful {
            if cluster != self.out_info()[start].cluster {
                while start > 0
                    && self.out_info()[start.wrapping_sub(1)].cluster
                        == self.out_info()[start].cluster
                {
                    start = start.wrapping_sub(1);
                }
            }
            if cluster != self.out_info()[end.wrapping_sub(1)].cluster {
                while end < self.out_len
                    && self.out_info()[end.wrapping_sub(1)].cluster == self.out_info()[end].cluster
                {
                    end = end.wrapping_add(1);
                }
            }
            if end == self.out_len && self.out_info()[end.wrapping_sub(1)].cluster != cluster {
                let target = self.out_info()[end.wrapping_sub(1)].cluster;
                let mut i = self.idx;
                while i < self.len && self.info[i].cluster == target {
                    Buffer::set_cluster(&mut self.info[i], cluster, 0);
                    i = i.wrapping_add(1);
                }
                self.charge(i.wrapping_sub(self.idx));
            }
        }
        self.charge(end.wrapping_sub(start));
        if self.max_ops < 0 {
            self.successful = false;
        }
        for i in start..end {
            Buffer::set_cluster(&mut self.out_info_mut()[i], cluster, 0);
        }
    }

    /// `delete_glyph`.
    pub(crate) fn delete_glyph(&mut self) {
        let cluster = self.info[self.idx].cluster;
        let survives = (self.idx.wrapping_add(1) < self.len
            && cluster == self.info[self.idx.wrapping_add(1)].cluster)
            || (self.out_len > 0 && cluster == self.prev().cluster);
        if !survives {
            if self.out_len > 0 {
                if cluster < self.prev().cluster && self.successful {
                    let mask = self.info[self.idx].mask;
                    let old = self.prev().cluster;
                    let mut i = self.out_len;
                    while i > 0 && self.out_info()[i.wrapping_sub(1)].cluster == old {
                        let at = i.wrapping_sub(1);
                        Buffer::set_cluster(&mut self.out_info_mut()[at], cluster, mask);
                        i = at;
                    }
                    self.charge(self.out_len.wrapping_sub(i));
                    if self.max_ops < 0 {
                        self.successful = false;
                    }
                }
            } else if self.idx.wrapping_add(1) < self.len {
                self.merge_clusters(self.idx, self.idx.wrapping_add(2));
            }
        }
        self.skip_glyph();
    }

    /// `delete_glyphs_inplace`.
    pub(crate) fn delete_glyphs_inplace(&mut self, filter: impl Fn(&Info) -> bool) {
        let mut j = 0usize;
        let count = self.len;
        for i in 0..count {
            if filter(&self.info[i]) {
                let cluster = self.info[i].cluster;
                if i.wrapping_add(1) < count && cluster == self.info[i.wrapping_add(1)].cluster {
                    continue;
                }
                if j > 0 {
                    if cluster < self.info[j.wrapping_sub(1)].cluster && self.successful {
                        let mask = self.info[i].mask;
                        let old = self.info[j.wrapping_sub(1)].cluster;
                        let mut k = j;
                        while k > 0 && self.info[k.wrapping_sub(1)].cluster == old {
                            let at = k.wrapping_sub(1);
                            Buffer::set_cluster(&mut self.info[at], cluster, mask);
                            k = at;
                        }
                        self.charge(j.wrapping_sub(k));
                        if self.max_ops < 0 {
                            self.successful = false;
                        }
                    }
                    continue;
                }
                if i.wrapping_add(1) < count {
                    self.merge_clusters(i, i.wrapping_add(2));
                }
                continue;
            }
            if j != i {
                self.info[j] = self.info[i];
                self.pos[j] = self.pos[i];
            }
            j = j.wrapping_add(1);
        }
        self.len = j;
    }

    /// `reverse_range`.
    pub(crate) fn reverse_range(&mut self, start: usize, end: usize) {
        self.info.range_mut(start, end).reverse();
        if self.have_positions {
            self.pos.range_mut(start, end).reverse();
        }
    }

    /// `reverse`.
    pub(crate) fn reverse(&mut self) {
        self.reverse_range(0, self.len);
    }

    /// `reverse_groups`.
    pub(crate) fn reverse_groups(&mut self, group: impl Fn(&Info, &Info) -> bool, merge: bool) {
        if self.len == 0 {
            return;
        }
        let mut start = 0usize;
        let mut i = 1usize;
        while i < self.len {
            if !group(&self.info[i.wrapping_sub(1)], &self.info[i]) {
                if merge {
                    self.merge_clusters(start, i);
                }
                self.reverse_range(start, i);
                start = i;
            }
            i = i.wrapping_add(1);
        }
        if merge {
            self.merge_clusters(start, i);
        }
        self.reverse_range(start, i);
        self.reverse();
    }

    /// `reverse_clusters`.
    pub(crate) fn reverse_clusters(&mut self) {
        self.reverse_groups(|a, b| a.cluster == b.cluster, false);
    }

    /// `group_end`.
    pub(crate) fn group_end(
        &self,
        mut start: usize,
        group: impl Fn(&Info, &Info) -> bool,
    ) -> usize {
        start = start.wrapping_add(1);
        while start < self.len && group(&self.info[start.wrapping_sub(1)], &self.info[start]) {
            start = start.wrapping_add(1);
        }
        start
    }

    /// The groups of `foreach_group` as `(start, end)` pairs, collected.
    pub(crate) fn groups(&self, group: impl Fn(&Info, &Info) -> bool) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        let mut start = 0usize;
        while start < self.len {
            let end = self.group_end(start, &group);
            v.push((start, end));
            start = end;
        }
        v
    }

    /// `sort`: an insertion sort of `start..end` that merges the
    /// clusters it moves items across.
    pub(crate) fn sort(&mut self, start: usize, end: usize, cmp: impl Fn(&Info, &Info) -> i32) {
        for i in start.wrapping_add(1)..end {
            let mut j = i;
            while j > start && cmp(&self.info[j.wrapping_sub(1)], &self.info[i]) > 0 {
                j = j.wrapping_sub(1);
            }
            if i == j {
                continue;
            }
            self.merge_clusters(j, i.wrapping_add(1));
            self.info.range_mut(j, i.wrapping_add(1)).rotate_right(1);
        }
    }

    /// `_set_glyph_flags`.
    fn set_glyph_flags(
        &mut self,
        mask: Mask,
        start: usize,
        end: Option<usize>,
        interior: bool,
        from_out_buffer: bool,
    ) {
        if let Some(e) = end
            && e.wrapping_sub(start) > 255
        {
            return;
        }
        let end = end.unwrap_or(usize::MAX).min(self.len);
        if interior && !from_out_buffer && end.wrapping_sub(start) < 2 {
            return;
        }
        if !from_out_buffer || !self.have_output {
            if !interior {
                for info in self.info.range_mut(start, end) {
                    info.mask |= mask;
                }
            } else {
                let cluster = self.find_min_cluster(false, start, end, u32::MAX);
                self.infos_set_glyph_flags(false, start, end, cluster, mask);
            }
        } else if !interior {
            let out_len = self.out_len;
            for info in self.out_info_mut().range_mut(start, out_len) {
                info.mask |= mask;
            }
            let idx = self.idx;
            for info in self.info.range_mut(idx, end) {
                info.mask |= mask;
            }
        } else {
            let cluster = self.find_min_cluster(false, self.idx, end, u32::MAX);
            let cluster = self.find_min_cluster(true, start, self.out_len, cluster);
            self.infos_set_glyph_flags(true, start, self.out_len, cluster, mask);
            self.infos_set_glyph_flags(false, self.idx, end, cluster, mask);
        }
    }

    fn array(&self, out: bool) -> &Items<Info> {
        if out { self.out_info() } else { &self.info }
    }
    fn array_mut(&mut self, out: bool) -> &mut Items<Info> {
        if out {
            self.out_info_mut()
        } else {
            &mut self.info
        }
    }

    /// `_infos_find_min_cluster`.
    fn find_min_cluster(&self, out: bool, start: usize, end: usize, cluster: u32) -> u32 {
        if start >= end {
            return cluster;
        }
        let a = self.array(out);
        if self.cluster_level == ClusterLevel::Characters {
            return a
                .range(start, end)
                .iter()
                .fold(cluster, |c, i| c.min(i.cluster));
        }
        cluster.min(a[start].cluster.min(a[end.wrapping_sub(1)].cluster))
    }

    /// `_infos_set_glyph_flags`.
    fn infos_set_glyph_flags(
        &mut self,
        out: bool,
        start: usize,
        end: usize,
        cluster: u32,
        mask: Mask,
    ) {
        if start >= end {
            return;
        }
        self.charge(end.wrapping_sub(start));
        if self.max_ops < 0 {
            self.successful = false;
        }
        let level = self.cluster_level;
        let a = self.array_mut(out);
        let first = a[start].cluster;
        let last = a[end.wrapping_sub(1)].cluster;
        if level == ClusterLevel::Characters || (cluster != first && cluster != last) {
            for info in a.range_mut(start, end) {
                if cluster != info.cluster {
                    info.mask |= mask;
                }
            }
            return;
        }
        if cluster == first {
            let mut i = end;
            while start < i && a[i.wrapping_sub(1)].cluster != first {
                a[i.wrapping_sub(1)].mask |= mask;
                i = i.wrapping_sub(1);
            }
        } else {
            let mut i = start;
            while i < end && a[i].cluster != last {
                a[i].mask |= mask;
                i = i.wrapping_add(1);
            }
        }
    }

    /// `unsafe_to_break`.
    pub(crate) fn unsafe_to_break(&mut self, start: usize, end: usize) {
        self.set_glyph_flags(
            GlyphFlags::UNSAFE_TO_BREAK.0 | GlyphFlags::UNSAFE_TO_CONCAT.0,
            start,
            Some(end),
            true,
            false,
        );
    }

    /// `unsafe_to_break` of the whole buffer.
    pub(crate) fn unsafe_to_break_all(&mut self) {
        self.set_glyph_flags(
            GlyphFlags::UNSAFE_TO_BREAK.0 | GlyphFlags::UNSAFE_TO_CONCAT.0,
            0,
            None,
            true,
            false,
        );
    }

    /// `safe_to_insert_tatweel`.
    pub(crate) fn safe_to_insert_tatweel(&mut self, start: usize, end: usize) {
        if !self
            .flags
            .contains(BufferFlags::PRODUCE_SAFE_TO_INSERT_TATWEEL)
        {
            self.unsafe_to_break(start, end);
            return;
        }
        self.set_glyph_flags(
            GlyphFlags::SAFE_TO_INSERT_TATWEEL.0,
            start,
            Some(end),
            true,
            false,
        );
    }

    /// `unsafe_to_concat`.
    pub(crate) fn unsafe_to_concat(&mut self, start: usize, end: usize) {
        if !self.flags.contains(BufferFlags::PRODUCE_UNSAFE_TO_CONCAT) {
            return;
        }
        self.set_glyph_flags(
            GlyphFlags::UNSAFE_TO_CONCAT.0,
            start,
            Some(end),
            false,
            false,
        );
    }

    /// `unsafe_to_break_from_outbuffer`.
    pub(crate) fn unsafe_to_break_from_outbuffer(&mut self, start: usize, end: usize) {
        self.set_glyph_flags(
            GlyphFlags::UNSAFE_TO_BREAK.0 | GlyphFlags::UNSAFE_TO_CONCAT.0,
            start,
            Some(end),
            true,
            true,
        );
    }

    /// `unsafe_to_concat_from_outbuffer`.
    pub(crate) fn unsafe_to_concat_from_outbuffer(&mut self, start: usize, end: usize) {
        if !self.flags.contains(BufferFlags::PRODUCE_UNSAFE_TO_CONCAT) {
            return;
        }
        self.set_glyph_flags(
            GlyphFlags::UNSAFE_TO_CONCAT.0,
            start,
            Some(end),
            false,
            true,
        );
    }

    /// `_hb_next_syllable`.
    pub(crate) fn next_syllable(&self, start: usize) -> usize {
        let count = start.wrapping_add(self.len.saturating_sub(start).min(MAX_SYLLABLE_LENGTH));
        let syllable = self.info[start].syllable;
        let mut i = start.wrapping_add(1);
        while i < count && self.info[i].syllable == syllable {
            i = i.wrapping_add(1);
        }
        i
    }

    /// `enter`: sets the limits for a shaping call.
    pub(crate) fn enter(&mut self) {
        self.serial = 0;
        self.scratch_flags = 0;
        let len = u32::try_from(self.len).ok();
        if let Some(mul) = len.and_then(|l| l.checked_mul(256)) {
            let limit = usize::try_from(mul).unwrap_or(usize::MAX).max(MAX_LEN_MIN);
            self.max_len = if self.max_len < MAX_LEN_DEFAULT {
                self.max_len.min(limit)
            } else {
                limit
            };
        }
        if let Some(mul) = len.and_then(|l| l.checked_mul(4096)) {
            // `(int) hb_max (mul, HB_BUFFER_MAX_OPS_MIN)`.
            let limit = i32::from_ne_bytes(mul.max(65536).to_ne_bytes());
            self.max_ops = if self.max_ops < MAX_OPS_DEFAULT {
                self.max_ops.min(limit)
            } else {
                limit
            };
        }
    }

    /// `leave`.
    pub(crate) fn leave(&mut self) {
        self.max_len = MAX_LEN_DEFAULT;
        self.max_ops = MAX_OPS_DEFAULT;
        self.serial = 0;
    }
}

/// `HB_MAX_SYLLABLE_LENGTH`.
#[allow(dead_code)] // Some items serve the shapers of ADR 0024.
pub(crate) const MAX_SYLLABLE_LENGTH: usize = 64;

/// Text to shape (`hb_buffer_t` holding Unicode): characters with their
/// cluster values, the context around them, and the settings of the
/// buffer.
#[derive(Clone, Debug, Default)]
pub struct UnicodeBuffer {
    pub(crate) buf: Buffer,
}

impl UnicodeBuffer {
    /// An empty buffer with the default settings (`hb_buffer_create`).
    pub fn new() -> UnicodeBuffer {
        UnicodeBuffer { buf: Buffer::new() }
    }

    /// Removes the characters and the context, keeping the settings
    /// (`hb_buffer_clear_contents`). The random state goes back to 1.
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// The number of characters.
    pub fn len(&self) -> usize {
        self.buf.len
    }
    /// Whether there are no characters.
    pub fn is_empty(&self) -> bool {
        self.buf.len == 0
    }
    /// The code points.
    pub fn codepoints(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.buf.infos().iter().map(|i| i.codepoint)
    }
    /// The cluster values.
    pub fn clusters(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.buf.infos().iter().map(|i| i.cluster)
    }
    /// The pre-context, nearest character first.
    pub fn pre_context(&self) -> &[u32] {
        self.buf.context[0]
            .get(..self.buf.context_len[0])
            .unwrap_or_default()
    }
    /// The post-context, nearest character first.
    pub fn post_context(&self) -> &[u32] {
        self.buf.context[1]
            .get(..self.buf.context_len[1])
            .unwrap_or_default()
    }

    /// Adds one character with a cluster value (`hb_buffer_add`).
    pub fn push(&mut self, codepoint: u32, cluster: u32) {
        self.buf.add(codepoint, cluster);
    }

    /// Adds UTF-8 text (`hb_buffer_add_utf8`): the characters of
    /// `text[item_offset..item_offset + item_len]` (to the end for
    /// `None`; both clamped to the text) with their byte positions in
    /// `text` as cluster values. Up to five characters before the item
    /// become the pre-context when the buffer is empty, and up to five
    /// after it the post-context. Each ill-formed byte becomes the
    /// replacement character (`src/hb-utf.hh`).
    pub fn push_utf8(&mut self, text: &[u8], item_offset: usize, item_len: Option<usize>) {
        self.buf.add_utf::<utf::Utf8>(text, item_offset, item_len);
    }
    /// Adds UTF-16 text (`hb_buffer_add_utf16`), as [`Self::push_utf8`]
    /// with code unit positions; lone surrogates become the replacement
    /// character.
    pub fn push_utf16(&mut self, text: &[u16], item_offset: usize, item_len: Option<usize>) {
        self.buf.add_utf::<utf::Utf16>(text, item_offset, item_len);
    }
    /// Adds UTF-32 text (`hb_buffer_add_utf32`), as [`Self::push_utf8`]
    /// with indices; surrogates and values past U+10FFFF become the
    /// replacement character.
    pub fn push_utf32(&mut self, text: &[u32], item_offset: usize, item_len: Option<usize>) {
        self.buf.add_utf::<utf::Utf32>(text, item_offset, item_len);
    }
    /// Adds code points as they are (`hb_buffer_add_codepoints`), as
    /// [`Self::push_utf32`] without the replacement.
    pub fn push_codepoints(&mut self, text: &[u32], item_offset: usize, item_len: Option<usize>) {
        self.buf
            .add_utf::<utf::Codepoints>(text, item_offset, item_len);
    }
    /// Adds Latin-1 text (`hb_buffer_add_latin1`), each byte one
    /// character.
    pub fn push_latin1(&mut self, text: &[u8], item_offset: usize, item_len: Option<usize>) {
        self.buf.add_utf::<utf::Latin1>(text, item_offset, item_len);
    }

    /// The flags.
    pub fn flags(&self) -> BufferFlags {
        self.buf.flags
    }
    /// Sets the flags (`hb_buffer_set_flags`).
    pub fn set_flags(&mut self, flags: BufferFlags) {
        self.buf.flags = flags;
    }
    /// The cluster level.
    pub fn cluster_level(&self) -> ClusterLevel {
        self.buf.cluster_level
    }
    /// Sets the cluster level (`hb_buffer_set_cluster_level`).
    pub fn set_cluster_level(&mut self, level: ClusterLevel) {
        self.buf.cluster_level = level;
    }
    /// The replacement character for ill-formed text.
    pub fn replacement_codepoint(&self) -> u32 {
        self.buf.replacement
    }
    /// Sets the replacement character for ill-formed text
    /// (`hb_buffer_set_replacement_codepoint`); U+FFFD by default.
    pub fn set_replacement_codepoint(&mut self, c: u32) {
        self.buf.replacement = c;
    }
    /// The glyph for default-ignorable characters; 0 for the font's
    /// space glyph.
    pub fn invisible_glyph(&self) -> u32 {
        self.buf.invisible
    }
    /// Sets the glyph that replaces default-ignorable characters
    /// (`hb_buffer_set_invisible_glyph`); 0 uses the font's space glyph.
    pub fn set_invisible_glyph(&mut self, glyph: u32) {
        self.buf.invisible = glyph;
    }
    /// The glyph for characters the font does not have.
    pub fn not_found_glyph(&self) -> u32 {
        self.buf.not_found
    }
    /// Sets the glyph for characters the font does not have
    /// (`hb_buffer_set_not_found_glyph`); 0 by default.
    pub fn set_not_found_glyph(&mut self, glyph: u32) {
        self.buf.not_found = glyph;
    }
    /// The glyph for variation selectors the font does not support.
    pub fn not_found_variation_selector_glyph(&self) -> Option<u32> {
        self.buf.not_found_variation_selector
    }
    /// Sets the glyph for variation selectors the font has no variant
    /// for (`hb_buffer_set_not_found_variation_selector_glyph`); `None`
    /// (the default) treats them as default-ignorable.
    pub fn set_not_found_variation_selector_glyph(&mut self, glyph: Option<u32>) {
        self.buf.not_found_variation_selector = glyph;
    }
    /// The state of the random numbers of the `rand` feature.
    pub fn random_state(&self) -> u32 {
        self.buf.random_state
    }
    /// Sets the state of the random numbers of the `rand` feature
    /// (`hb_buffer_set_random_state`); 1 after [`Self::new`] and
    /// [`Self::clear`]. HarfBuzz's generator is `state × 48271 mod
    /// 2³¹ − 1`.
    pub fn set_random_state(&mut self, state: u32) {
        self.buf.random_state = state;
    }
}

/// The result of shaping: glyphs with their cluster values, flags and
/// positions, in visual order (`hb_buffer_t` holding glyphs).
#[derive(Clone, Debug)]
pub struct GlyphBuffer {
    pub(crate) infos: Vec<GlyphInfo>,
    pub(crate) positions: Vec<GlyphPosition>,
    pub(crate) direction: Direction,
    /// The working buffer, emptied, kept for [`GlyphBuffer::into_unicode`].
    pub(crate) spare: Buffer,
}

impl GlyphBuffer {
    /// A glyph buffer from its parts, as deserialization reads them;
    /// the shorter of `infos` and `positions` is padded with zeros.
    pub fn from_parts(
        direction: Direction,
        mut infos: Vec<GlyphInfo>,
        mut positions: Vec<GlyphPosition>,
    ) -> GlyphBuffer {
        let n = infos.len().max(positions.len());
        infos.resize(n, GlyphInfo::default());
        positions.resize(n, GlyphPosition::default());
        GlyphBuffer {
            infos,
            positions,
            direction,
            spare: Buffer::new(),
        }
    }
    /// The glyphs.
    pub fn glyph_infos(&self) -> &[GlyphInfo] {
        &self.infos
    }
    /// The positions, one per glyph.
    pub fn glyph_positions(&self) -> &[GlyphPosition] {
        &self.positions
    }
    /// The number of glyphs.
    pub fn len(&self) -> usize {
        self.infos.len()
    }
    /// Whether there are no glyphs.
    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }
    /// The direction the glyphs were shaped in.
    pub fn direction(&self) -> Direction {
        self.direction
    }
    /// An empty [`UnicodeBuffer`] with the settings of the buffer that
    /// was shaped and the memory of this one (`hb_buffer_clear_contents`
    /// on the same `hb_buffer_t`).
    pub fn into_unicode(self) -> UnicodeBuffer {
        let mut buf = self.spare;
        buf.clear();
        UnicodeBuffer { buf }
    }

    /// Reorders the glyphs of each cluster into a canonical order and
    /// moves the cluster's advance onto one glyph, so that buffers that
    /// differ only in that compare equal (`hb_buffer_normalize_glyphs`).
    pub fn normalize_glyphs(&mut self) {
        let backward = self.direction.is_backward();
        let n = self.infos.len().min(self.positions.len());
        let mut start = 0usize;
        while start < n {
            let cluster = self.infos.get(start).map_or(0, |i| i.cluster);
            let mut end = start.wrapping_add(1);
            while end < n && self.infos.get(end).is_some_and(|i| i.cluster == cluster) {
                end = end.wrapping_add(1);
            }
            normalize_cluster(
                self.infos.get_mut(start..end).unwrap_or_default(),
                self.positions.get_mut(start..end).unwrap_or_default(),
                backward,
            );
            start = end;
        }
    }
}

/// `normalize_glyphs_cluster`.
fn normalize_cluster(infos: &mut [GlyphInfo], pos: &mut [GlyphPosition], backward: bool) {
    let (total_x, total_y) = pos.iter().fold((0i32, 0i32), |(x, y), p| {
        (x.saturating_add(p.x_advance), y.saturating_add(p.y_advance))
    });
    let (mut x, mut y) = (0i32, 0i32);
    for p in pos.iter_mut() {
        p.x_offset = p.x_offset.saturating_add(x);
        p.y_offset = p.y_offset.saturating_add(y);
        x = x.saturating_add(p.x_advance);
        y = y.saturating_add(p.y_advance);
        p.x_advance = 0;
        p.y_advance = 0;
    }
    let n = infos.len();
    if backward {
        if let Some(last) = pos.last_mut() {
            last.x_advance = total_x;
            last.y_advance = total_y;
        }
        stable_sort(
            infos.get_mut(..n.saturating_sub(1)).unwrap_or_default(),
            pos.get_mut(..n.saturating_sub(1)).unwrap_or_default(),
        );
    } else {
        if let Some(first) = pos.first_mut() {
            first.x_advance = first.x_advance.saturating_add(total_x);
            first.y_advance = first.y_advance.saturating_add(total_y);
        }
        for p in pos.iter_mut().skip(1) {
            p.x_offset = p.x_offset.saturating_sub(total_x);
            p.y_offset = p.y_offset.saturating_sub(total_y);
        }
        stable_sort(
            infos.get_mut(1..).unwrap_or_default(),
            pos.get_mut(1..).unwrap_or_default(),
        );
    }
}

/// `hb_stable_sort` with `compare_info_codepoint` (`(int) b - (int) a`),
/// moving the positions along.
fn stable_sort(infos: &mut [GlyphInfo], pos: &mut [GlyphPosition]) {
    let cmp = |a: &GlyphInfo, b: &GlyphInfo| {
        i32::from_ne_bytes(b.glyph.to_ne_bytes())
            .wrapping_sub(i32::from_ne_bytes(a.glyph.to_ne_bytes()))
    };
    for i in 1..infos.len() {
        let Some(&item) = infos.get(i) else { break };
        let mut j = i;
        while j > 0
            && infos
                .get(j.wrapping_sub(1))
                .is_some_and(|p| cmp(p, &item) > 0)
        {
            j = j.wrapping_sub(1);
        }
        if i == j {
            continue;
        }
        if let Some(s) = infos.get_mut(j..=i) {
            s.rotate_right(1);
        }
        if let Some(s) = pos.get_mut(j..=i) {
            s.rotate_right(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Info, Pos};

    /// The measured sizes (ADR 0020, データ構造): HarfBuzz's 20 bytes plus
    /// the normalizer's glyph index, which HarfBuzz overlays on
    /// `glyph_props`, `lig_props` and `syllable`.
    #[test]
    fn item_sizes() {
        assert_eq!(core::mem::size_of::<Info>(), 24);
        assert_eq!(core::mem::size_of::<Pos>(), 20);
    }
}
