//! The hinting state of FreeType's Adobe engine (sofel ADR 0064): stem
//! hints, hint masks, blue zones (`src/psaux/psblues.c`) and hint maps
//! (`src/psaux/pshints.c`). Only horizontal stems move points, in y;
//! vertical stems are only counted. Stem darkening is off, as in
//! FreeType's default (`no-stem-darkening` is true), so the darkening
//! amounts are zero.
//!
//! Ported from FreeType 2.15.0. Copyright 2007-2014 Adobe Systems
//! Incorporated, and Copyright (C) 1996-2026 by David Turner, Robert
//! Wilhelm, and Werner Lemberg. Used under the FreeType License (FTL)
//! with the Adobe patent license; see THIRD_PARTY_NOTICES.md.

use super::dict::PrivateDict;
use super::ft;

/// `CF2_MAX_HINTS`: the most horizontal and vertical stems together.
pub(crate) const MAX_HINTS: usize = 96;
/// `CF2_MAX_HINT_EDGES`.
pub(crate) const MAX_EDGES: usize = MAX_HINTS * 2;
/// `CF2_MIN_COUNTER`: half a pixel.
const MIN_COUNTER: i32 = 0x8000;
const ONE: i32 = 0x1_0000;
/// `CF2_ICF_Top` and `CF2_ICF_Bottom`.
const ICF_TOP: i32 = 880 << 16;
const ICF_BOTTOM: i32 = -120 * 65536;

const GHOST_BOTTOM: u8 = 0x1;
const GHOST_TOP: u8 = 0x2;
const PAIR_BOTTOM: u8 = 0x4;
const PAIR_TOP: u8 = 0x8;
const LOCKED: u8 = 0x10;
const SYNTHETIC: u8 = 0x20;

fn mul_fix(a: i32, b: i32) -> i32 {
    ft::to_i32(ft::mul_fix(i64::from(a), i64::from(b)))
}

fn div_fix(a: i32, b: i32) -> i32 {
    ft::to_i32(ft::div_fix(i64::from(a), i64::from(b)))
}

/// `cf2_intToFixed` of an `FT_Pos`: the low 32 bits shifted.
fn int_to_fixed(v: i64) -> i32 {
    (ft::low32(v.cast_unsigned()) << 16).cast_signed()
}

/// `cf2_fixedRound`.
fn round(x: i32) -> i32 {
    (x.cast_unsigned().wrapping_add(0x8000) & 0xFFFF_0000).cast_signed()
}

/// `cf2_fixedFraction`.
fn fraction(x: i32) -> i32 {
    x & 0xFFFF
}

fn abs(x: i32) -> i32 {
    if x < 0 { x.wrapping_neg() } else { x }
}

/// One edge of a hint map (`CF2_HintRec`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Hint {
    flags: u8,
    index: u8,
    cs: i32,
    ds: i32,
    scale: i32,
}

impl Hint {
    fn is_valid(&self) -> bool {
        self.flags != 0
    }

    fn is_pair(&self) -> bool {
        self.flags & (PAIR_BOTTOM | PAIR_TOP) != 0
    }

    fn is_pair_top(&self) -> bool {
        self.flags & PAIR_TOP != 0
    }

    fn is_top(&self) -> bool {
        self.flags & (PAIR_TOP | GHOST_TOP) != 0
    }

    fn is_bottom(&self) -> bool {
        self.flags & (PAIR_BOTTOM | GHOST_BOTTOM) != 0
    }

    fn is_locked(&self) -> bool {
        self.flags & LOCKED != 0
    }

    fn is_synthetic(&self) -> bool {
        self.flags & SYNTHETIC != 0
    }

    /// `cf2_hint_init` without darkening.
    fn new(stem: &StemHint, index: usize, origin: i32, scale: i32, bottom: bool) -> Hint {
        let width = stem.max.wrapping_sub(stem.min);
        let (flags, cs) = if width == -21 * 65536 {
            if bottom {
                (GHOST_BOTTOM, stem.max)
            } else {
                (0, 0)
            }
        } else if width == -20 * 65536 {
            if bottom {
                (0, 0)
            } else {
                (GHOST_TOP, stem.min)
            }
        } else if width < 0 {
            if bottom {
                (PAIR_BOTTOM, stem.max)
            } else {
                (PAIR_TOP, stem.min)
            }
        } else if bottom {
            (PAIR_BOTTOM, stem.min)
        } else {
            (PAIR_TOP, stem.max)
        };
        let mut hint = Hint {
            flags,
            index: u8::try_from(index).unwrap_or(u8::MAX),
            cs: cs.wrapping_add(origin),
            ds: 0,
            scale,
        };
        if hint.flags != 0 && stem.used {
            hint.ds = if hint.is_top() {
                stem.max_ds
            } else {
                stem.min_ds
            };
            hint.flags |= LOCKED;
        } else {
            hint.ds = mul_fix(hint.cs, scale);
        }
        hint
    }
}

/// A stem hint (`CF2_StemHintRec`): character space edges and, once a
/// hint map used it, the device space positions it got.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct StemHint {
    used: bool,
    min: i32,
    max: i32,
    min_ds: i32,
    max_ds: i32,
}

impl StemHint {
    /// A stem from `min` to `max` in character space, not used yet.
    pub(crate) fn new(min: i32, max: i32) -> StemHint {
        StemHint {
            used: false,
            min,
            max,
            min_ds: 0,
            max_ds: 0,
        }
    }
}

/// A hint mask (`CF2_HintMaskRec`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct HintMask {
    /// `isValid`.
    pub(crate) valid: bool,
    /// `isNew`.
    pub(crate) new: bool,
    /// `bitCount`.
    pub(crate) bits: usize,
    mask: [u8; MAX_HINTS / 8],
}

/// More than [`MAX_HINTS`] stems.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct TooManyHints;

impl HintMask {
    /// `cf2_hintmask_setCounts`.
    pub(crate) fn set_counts(&mut self, bits: usize) -> Result<(), TooManyHints> {
        if bits > MAX_HINTS {
            return Err(TooManyHints);
        }
        self.bits = bits;
        self.valid = true;
        self.new = true;
        Ok(())
    }

    fn bytes(&self) -> usize {
        self.bits.saturating_add(7) / 8
    }

    /// `cf2_hintmask_read` after the counts: the mask bytes.
    pub(crate) fn read(&mut self, bits: usize, data: &[u8]) -> Result<(), TooManyHints> {
        self.set_counts(bits)?;
        let n = self.bytes();
        for (slot, byte) in self.mask.iter_mut().zip(data).take(n) {
            *slot = *byte;
        }
        Ok(())
    }

    /// `cf2_hintmask_setAll`.
    fn set_all(&mut self, bits: usize) -> Result<(), TooManyHints> {
        self.set_counts(bits)?;
        let n = self.bytes();
        for slot in self.mask.iter_mut().take(n) {
            *slot = 0xFF;
        }
        let unused = (1u32 << (bits.wrapping_neg() & 7)).wrapping_sub(1);
        if let Some(last) = n.checked_sub(1).and_then(|i| self.mask.get_mut(i)) {
            *last &= !u8::try_from(unused).unwrap_or(0);
        }
        Ok(())
    }

    fn bit(&self, i: usize) -> bool {
        self.mask
            .get(i / 8)
            .is_some_and(|b| b & (0x80 >> (i & 7)) != 0)
    }

    fn clear_bit(&mut self, i: usize) {
        if let Some(b) = self.mask.get_mut(i / 8) {
            *b &= !(0x80 >> (i & 7));
        }
    }
}

/// A blue zone (`CF2_BlueRec`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Zone {
    cs_bottom: i32,
    cs_top: i32,
    cs_flat: i32,
    ds_flat: i32,
    bottom: bool,
}

/// What FreeType's Adobe engine hints a glyph with (sofel ADR 0064): the
/// scales from character space to 16.16 device pixels (FreeType's
/// `(x_scale + 32) / 64`) and the blue zones of the glyph's Private DICT
/// at the vertical scale. Make it with [`Hinting::new`] and pass it to
/// [`super::charstring::decode_hinted`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hinting {
    blues: Blues,
    x_scale: i32,
    y_scale: i32,
}

impl Hinting {
    /// The hinting of glyphs whose Private DICT is `private` at the
    /// scales `x_scale` and `y_scale` (`cf2_font_setup` and
    /// `cf2_blues_init`, without stem darkening).
    pub fn new(private: &PrivateDict, x_scale: i32, y_scale: i32) -> Hinting {
        Hinting {
            blues: Blues::new(private, y_scale),
            x_scale,
            y_scale,
        }
    }

    pub(crate) fn scales(&self) -> (i32, i32) {
        (self.x_scale, self.y_scale)
    }

    pub(crate) fn blues(&self) -> &Blues {
        &self.blues
    }
}

/// The blue zones of a Private DICT at a scale (`CF2_BluesRec`), as
/// `cf2_blues_init` computes them without darkening.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Blues {
    count: usize,
    suppress_overshoot: bool,
    em_box: bool,
    blue_shift: i32,
    blue_fuzz: i32,
    em_top: Hint,
    em_bottom: Hint,
    zones: [Zone; 12],
}

impl Blues {
    /// `cf2_blues_init` for `private` at the vertical scale `scale` (the
    /// 16.16 factor from character space to 16.16 device pixels).
    fn new(private: &PrivateDict, scale: i32) -> Blues {
        let mut blues = Blues {
            count: 0,
            suppress_overshoot: false,
            em_box: false,
            blue_shift: int_to_fixed(private.blue_shift),
            blue_fuzz: int_to_fixed(private.blue_fuzz),
            em_top: Hint::default(),
            em_bottom: Hint::default(),
            zones: [Zone::default(); 12],
        };
        let mut blue_scale = ft::to_i32(ft::div_fix(private.blue_scale, 1000 << 16));
        let value = |values: &[i64], i: usize| ft::to_i32(values.get(i).copied().unwrap_or(0));
        let blue_values = &private.blue_values.values[..];
        let other_blues = &private.other_blues.values[..];
        let family_blues = &private.family_blues.values[..];
        let family_other = &private.family_other_blues.values[..];
        let n_blue = usize::from(private.blue_values.len);
        let n_other = usize::from(private.other_blues.len);
        let n_family = usize::from(private.family_blues.len);
        let n_family_other = usize::from(private.family_other_blues.len);

        if private.language_group == 1
            && (n_blue == 0
                || (n_blue == 4
                    && value(blue_values, 0) < ICF_BOTTOM
                    && value(blue_values, 1) < ICF_BOTTOM
                    && value(blue_values, 2) > ICF_TOP
                    && value(blue_values, 3) > ICF_TOP))
        {
            let cs = ICF_BOTTOM.wrapping_sub(1);
            blues.em_bottom = Hint {
                flags: GHOST_BOTTOM | LOCKED | SYNTHETIC,
                index: 0,
                cs,
                ds: round(mul_fix(cs, scale)).wrapping_sub(MIN_COUNTER),
                scale,
            };
            let cs = ICF_TOP.wrapping_add(1);
            blues.em_top = Hint {
                flags: GHOST_TOP | LOCKED | SYNTHETIC,
                index: 0,
                cs,
                ds: round(mul_fix(cs, scale)).wrapping_add(MIN_COUNTER),
                scale,
            };
            blues.em_box = true;
            return blues;
        }

        let mut max_zone_height = 0i32;
        let mut push = |blues: &mut Blues, bottom_edge: i32, top_edge: i32, bottom: bool| {
            let height = top_edge.wrapping_sub(bottom_edge);
            if height < 0 {
                return;
            }
            max_zone_height = max_zone_height.max(height);
            if let Some(zone) = blues.zones.get_mut(blues.count) {
                *zone = Zone {
                    cs_bottom: bottom_edge,
                    cs_top: top_edge,
                    cs_flat: if bottom { top_edge } else { bottom_edge },
                    ds_flat: 0,
                    bottom,
                };
                blues.count = blues.count.saturating_add(1);
            }
        };
        for i in (0..n_blue).step_by(2) {
            push(
                &mut blues,
                value(blue_values, i),
                value(blue_values, i.saturating_add(1)),
                i == 0,
            );
        }
        for i in (0..n_other).step_by(2) {
            push(
                &mut blues,
                value(other_blues, i),
                value(other_blues, i.saturating_add(1)),
                true,
            );
        }

        // FamilyBlues: the nearest flat edge within one device pixel.
        let units_per_pixel = div_fix(ONE, scale);
        let count = blues.count;
        for zone in blues.zones.iter_mut().take(count) {
            let flat = zone.cs_flat;
            let mut min_diff = i32::MAX;
            if zone.bottom {
                for j in (0..n_family_other).step_by(2) {
                    let family = value(family_other, j.saturating_add(1));
                    let diff = abs(flat.wrapping_sub(family));
                    if diff < min_diff && diff < units_per_pixel {
                        zone.cs_flat = family;
                        min_diff = diff;
                        if diff == 0 {
                            break;
                        }
                    }
                }
                if n_family >= 2 {
                    let family = value(family_blues, 1);
                    let diff = abs(flat.wrapping_sub(family));
                    if diff < min_diff && diff < units_per_pixel {
                        zone.cs_flat = family;
                    }
                }
            } else {
                for j in (2..n_family).step_by(2) {
                    let family = value(family_blues, j);
                    let diff = abs(flat.wrapping_sub(family));
                    if diff < min_diff && diff < units_per_pixel {
                        zone.cs_flat = family;
                        min_diff = diff;
                        if diff == 0 {
                            break;
                        }
                    }
                }
            }
        }

        if max_zone_height > 0 {
            let limit = div_fix(ONE, max_zone_height);
            if blue_scale > limit {
                blue_scale = limit;
            }
        }
        let mut boost = 0;
        if scale < blue_scale {
            blues.suppress_overshoot = true;
            boost = 39322_i32.wrapping_sub(ft::to_i32(ft::mul_div(
                39322,
                i64::from(scale),
                i64::from(blue_scale),
            )));
            if boost > 0x7FFF {
                boost = 0x7FFF;
            }
        }
        for zone in blues.zones.iter_mut().take(count) {
            let ds = mul_fix(zone.cs_flat, scale);
            zone.ds_flat = round(if zone.bottom {
                ds.wrapping_sub(boost)
            } else {
                ds.wrapping_add(boost)
            });
        }
        blues
    }

    /// `cf2_blues_capture`.
    fn capture(&self, bottom: &mut Hint, top: &mut Hint) -> bool {
        let fuzz = self.blue_fuzz;
        let mut captured = None;
        for zone in self.zones.iter().take(self.count) {
            if zone.bottom
                && bottom.is_bottom()
                && zone.cs_bottom.wrapping_sub(fuzz) <= bottom.cs
                && bottom.cs <= zone.cs_top.wrapping_add(fuzz)
            {
                let new = if self.suppress_overshoot {
                    zone.ds_flat
                } else if zone.cs_top.wrapping_sub(bottom.cs) >= self.blue_shift {
                    round(bottom.ds).min(zone.ds_flat.wrapping_sub(ONE))
                } else {
                    round(bottom.ds)
                };
                captured = Some(new.wrapping_sub(bottom.ds));
                break;
            }
            if !zone.bottom
                && top.is_top()
                && zone.cs_bottom.wrapping_sub(fuzz) <= top.cs
                && top.cs <= zone.cs_top.wrapping_add(fuzz)
            {
                let new = if self.suppress_overshoot {
                    zone.ds_flat
                } else if top.cs.wrapping_sub(zone.cs_bottom) >= self.blue_shift {
                    round(top.ds).max(zone.ds_flat.wrapping_add(ONE))
                } else {
                    round(top.ds)
                };
                captured = Some(new.wrapping_sub(top.ds));
                break;
            }
        }
        let Some(moved) = captured else {
            return false;
        };
        for edge in [bottom, top] {
            if edge.is_valid() {
                edge.ds = edge.ds.wrapping_add(moved);
                edge.flags |= LOCKED;
            }
        }
        true
    }
}

/// A hint map (`CF2_HintMapRec`): edges sorted by character space
/// coordinate, mapping y to device space piecewise linearly. It holds at
/// most `N` edges: [`MAX_EDGES`] for hinting, 0 without, where only its
/// validity is used.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct HintMap<const N: usize> {
    /// `isValid`.
    pub(crate) valid: bool,
    scale: i32,
    count: usize,
    edges: [Hint; N],
}

impl<const N: usize> HintMap<N> {
    /// `cf2_hintmap_init`: empty and invalid, at `scale`.
    pub(crate) fn new(scale: i32) -> HintMap<N> {
        HintMap {
            valid: false,
            scale,
            count: 0,
            edges: [Hint::default(); N],
        }
    }

    /// Empties the map and makes it invalid at `scale`, as a fresh
    /// `cf2_hintmap_init`.
    pub(crate) fn reset(&mut self, scale: i32) {
        self.valid = false;
        self.scale = scale;
        self.count = 0;
    }

    fn edge(&self, i: usize) -> Hint {
        self.edges.get(i).copied().unwrap_or_default()
    }

    fn ds(&self, i: usize) -> i32 {
        self.edges.get(i).map_or(0, |e| e.ds)
    }

    fn cs(&self, i: usize) -> i32 {
        self.edges.get(i).map_or(0, |e| e.cs)
    }

    /// `cf2_hintmap_map`. The linear search from `lastIndex` always ends
    /// at the highest edge at or below `cs`, so it is not kept.
    pub(crate) fn map(&self, cs: i32) -> i32 {
        if self.count == 0 {
            return mul_fix(cs, self.scale);
        }
        let i = self
            .edges
            .iter()
            .take(self.count)
            .skip(1)
            .take_while(|e| cs >= e.cs)
            .count();
        let edge = self.edge(i);
        if i == 0 && cs < edge.cs {
            mul_fix(cs.wrapping_sub(edge.cs), self.scale).wrapping_add(edge.ds)
        } else {
            mul_fix(cs.wrapping_sub(edge.cs), edge.scale).wrapping_add(edge.ds)
        }
    }

    /// `cf2_hintmap_adjustHints`.
    fn adjust(&mut self) {
        let mut moves = [(0usize, 0i32); MAX_EDGES];
        let mut n_moves = 0usize;
        let count = self.count;
        let mut i = 0usize;
        while i < count {
            let is_pair = self.edge(i).is_pair();
            let j = if is_pair { i.saturating_add(1) } else { i };
            let ds_i = self.ds(i);
            let ds_j = self.ds(j);
            if !self.edge(i).is_locked() {
                let frac_down = fraction(ds_i);
                let frac_up = fraction(ds_j);
                let down_move_down = frac_down.wrapping_neg();
                let up_move_down = frac_up.wrapping_neg();
                let down_move_up = if frac_down == 0 {
                    0
                } else {
                    ONE.wrapping_sub(frac_down)
                };
                let up_move_up = if frac_up == 0 {
                    0
                } else {
                    ONE.wrapping_sub(frac_up)
                };
                let move_up = down_move_up.min(up_move_up);
                let move_down = down_move_down.max(up_move_down);
                let top = j.saturating_add(1) >= count;
                let room_up = top
                    || self.ds(j.saturating_add(1))
                        >= ds_j.wrapping_add(move_up.wrapping_add(MIN_COUNTER));
                let room_down = i == 0
                    || self.ds(i.saturating_sub(1))
                        <= ds_i.wrapping_add(move_down.wrapping_sub(MIN_COUNTER));
                let (mv, save) = if room_up {
                    if room_down {
                        (
                            if move_down.wrapping_neg() < move_up {
                                move_down
                            } else {
                                move_up
                            },
                            false,
                        )
                    } else {
                        (move_up, false)
                    }
                } else if room_down {
                    (move_down, move_up < move_down.wrapping_neg())
                } else {
                    (0, true)
                };
                if save
                    && !top
                    && !self.edge(j.saturating_add(1)).is_locked()
                    && let Some(slot) = moves.get_mut(n_moves)
                {
                    *slot = (j, move_up.wrapping_sub(mv));
                    n_moves = n_moves.saturating_add(1);
                }
                if let Some(e) = self.edges.get_mut(i) {
                    e.ds = ds_i.wrapping_add(mv);
                }
                if is_pair && let Some(e) = self.edges.get_mut(j) {
                    e.ds = ds_j.wrapping_add(mv);
                }
            }
            if i > 0 {
                self.set_scale(i);
            }
            if is_pair {
                self.set_scale(j);
                i = i.saturating_add(1);
            }
            i = i.saturating_add(1);
        }
        for &(j, move_up) in moves.iter().take(n_moves).rev() {
            if self.ds(j.saturating_add(1))
                >= self.ds(j).wrapping_add(move_up.wrapping_add(MIN_COUNTER))
            {
                if let Some(e) = self.edges.get_mut(j) {
                    e.ds = e.ds.wrapping_add(move_up);
                }
                if self.edge(j).is_pair()
                    && let Some(e) = j.checked_sub(1).and_then(|k| self.edges.get_mut(k))
                {
                    e.ds = e.ds.wrapping_add(move_up);
                }
            }
        }
    }

    /// The scale of edge `i - 1` from its distance to edge `i`.
    fn set_scale(&mut self, i: usize) {
        let Some(k) = i.checked_sub(1) else {
            return;
        };
        let (lower, upper) = (self.edge(k), self.edge(i));
        if upper.cs != lower.cs
            && let Some(e) = self.edges.get_mut(k)
        {
            e.scale = div_fix(
                upper.ds.wrapping_sub(lower.ds),
                upper.cs.wrapping_sub(lower.cs),
            );
        }
    }

    /// `cf2_hintmap_insertHint`.
    fn insert(&mut self, initial: Option<&HintMap<N>>, bottom: Hint, top: Hint) {
        let (mut first, mut second, is_pair) = if !bottom.is_valid() {
            (top, top, false)
        } else if !top.is_valid() {
            (bottom, top, false)
        } else {
            (bottom, top, true)
        };
        if is_pair && top.cs < bottom.cs {
            return;
        }
        let at = self
            .edges
            .iter()
            .take(self.count)
            .position(|e| e.cs >= first.cs)
            .unwrap_or(self.count);
        if at < self.count {
            let next = self.edge(at);
            if next.cs == first.cs || (is_pair && next.cs <= second.cs) || next.is_pair_top() {
                return;
            }
        }
        if let Some(initial) = initial.filter(|m| m.valid)
            && !first.is_locked()
        {
            if is_pair {
                let half = second.cs.wrapping_sub(first.cs) / 2;
                let mid = initial.map(first.cs.wrapping_add(half));
                let half_width = mul_fix(half, self.scale);
                first.ds = mid.wrapping_sub(half_width);
                second.ds = mid.wrapping_add(half_width);
            } else {
                first.ds = initial.map(first.cs);
            }
        }
        if at > 0 && first.ds < self.ds(at.saturating_sub(1)) {
            return;
        }
        if at < self.count {
            let edge = if is_pair { second.ds } else { first.ds };
            if edge > self.ds(at) {
                return;
            }
        }
        let added = if is_pair { 2 } else { 1 };
        let new_count = self.count.saturating_add(added);
        if new_count > N.min(MAX_EDGES) {
            return;
        }
        if let Some(tail) = self.edges.get_mut(at..new_count) {
            tail.rotate_right(added);
        }
        if let Some(e) = self.edges.get_mut(at) {
            *e = first;
        }
        if is_pair && let Some(e) = self.edges.get_mut(at.saturating_add(1)) {
            *e = second;
        }
        self.count = new_count;
    }
}

/// What builds a hint map: the stems of the glyph, their counts, the
/// blue zones, and whether the map is hinted at all.
pub(crate) struct Stems<'a> {
    /// The horizontal stems (at most [`MAX_HINTS`] kept).
    pub(crate) h: &'a mut [StemHint],
    /// The number of horizontal stems.
    pub(crate) h_count: usize,
    /// The number of vertical stems.
    pub(crate) v_count: usize,
    /// The blue zones when hinting, `None` without.
    pub(crate) blues: Option<&'a Blues>,
}

/// `cf2_hintmap_build` of `map` from `mask`, building `initial` first if
/// it is not valid.
pub(crate) fn build<const N: usize>(
    map: &mut HintMap<N>,
    initial: &mut HintMap<N>,
    stems: &mut Stems<'_>,
    mask: &mut HintMask,
    origin: i32,
) -> Result<(), TooManyHints> {
    if !initial.valid {
        let mut temp = HintMask::default();
        build_one(initial, None, stems, &mut temp, origin, true)?;
    }
    build_one(map, Some(initial), stems, mask, origin, false)
}

fn build_one<const N: usize>(
    map: &mut HintMap<N>,
    initial: Option<&HintMap<N>>,
    stems: &mut Stems<'_>,
    mask: &mut HintMask,
    origin: i32,
    initial_map: bool,
) -> Result<(), TooManyHints> {
    if !mask.valid {
        mask.set_all(stems.h_count.saturating_add(stems.v_count))?;
    }
    map.count = 0;
    let bit_count = stems.h_count;
    if bit_count > mask.bits {
        return Ok(());
    }
    if let Some(blues) = stems.blues {
        let mut temp = *mask;
        if blues.em_box {
            map.insert(initial, blues.em_bottom, Hint::default());
            map.insert(initial, Hint::default(), blues.em_top);
        }
        let scale = map.scale;
        let hint = |stems: &Stems<'_>, i: usize, bottom: bool| {
            let stem = stems.h.get(i).copied().unwrap_or_default();
            Hint::new(&stem, i, origin, scale, bottom)
        };
        for i in 0..bit_count {
            if temp.bit(i) {
                let mut bottom = hint(stems, i, true);
                let mut top = hint(stems, i, false);
                if bottom.is_locked() || top.is_locked() || blues.capture(&mut bottom, &mut top) {
                    map.insert(initial, bottom, top);
                    temp.clear_bit(i);
                }
            }
        }
        if initial_map {
            if map.count == 0 || map.cs(0) > 0 || map.cs(map.count.saturating_sub(1)) < 0 {
                let edge = Hint {
                    flags: GHOST_BOTTOM | LOCKED | SYNTHETIC,
                    index: 0,
                    cs: 0,
                    ds: 0,
                    scale,
                };
                map.insert(initial, edge, Hint::default());
            }
        } else {
            for i in 0..bit_count {
                if temp.bit(i) {
                    let bottom = hint(stems, i, true);
                    let top = hint(stems, i, false);
                    map.insert(initial, bottom, top);
                }
            }
        }
        map.adjust();
        if !initial_map {
            for edge in map.edges.iter().take(map.count) {
                if edge.is_synthetic() {
                    continue;
                }
                if let Some(stem) = stems.h.get_mut(usize::from(edge.index)) {
                    if edge.is_top() {
                        stem.max_ds = edge.ds;
                    } else {
                        stem.min_ds = edge.ds;
                    }
                    stem.used = true;
                }
            }
        }
    }
    map.valid = true;
    mask.new = false;
    Ok(())
}
