//! The Latin writing system of the automatic hinter.
//!
//! Ported from FreeType 2.15.0's `src/autofit/aflatin.c` and
//! `src/autofit/aflatin.h`, with the vertical separation adjustments of
//! `src/autofit/afadjust.c`. Copyright (C) 2003-2026 by David Turner,
//! Robert Wilhelm, and Werner Lemberg. Used under the FreeType License
//! (FTL); see THIRD_PARTY_NOTICES.md.

// Point, segment, edge and blue indices stay inside their arrays as in
// FreeType; the arithmetic is FreeType's on 64-bit `FT_Pos` values that
// come from 16-bit font units.
// The casts are FreeType's: `FT_Short` fields truncate, and point and
// contour indices travel through signed `FT_Int` and `FT_Pos` values.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use alloc::vec::Vec;

use super::calc::{div_fix, mul_div, mul_fix, pix_round};
use super::hints::{
    DIR_NONE, EDGE_DONE, EDGE_NEUTRAL, EDGE_NO_BLUE, EDGE_NORMAL, EDGE_ROUND, EDGE_SERIF,
    FLAG_CONTROL, FLAG_IGNORE, FLAG_TOUCH_Y, GlyphHints, HORZ, OutlineRef,
    SCALER_FLAG_NO_HORIZONTAL, Segment, VERT, Width, sort_and_quantize_widths, sort_pos,
};
use super::tables::{ADJUSTMENTS, BLUE_STRING_MAX, BLUE_STRINGS, BLUE_STRINGSETS, SCRIPTS, STYLES};
use super::{Face, RenderMode, Scaler, next_cluster};

const MAX_WIDTHS: usize = 16;

const BLUE_PROPERTY_LATIN_TOP: u16 = 1 << 0;
const BLUE_PROPERTY_LATIN_SUB_TOP: u16 = 1 << 1;
const BLUE_PROPERTY_LATIN_NEUTRAL: u16 = 1 << 2;
const BLUE_PROPERTY_LATIN_X_HEIGHT: u16 = 1 << 3;
const BLUE_PROPERTY_LATIN_LONG: u16 = 1 << 4;
const BLUE_PROPERTY_LATIN_CAPITAL_BOTTOM: u16 = 1 << 5;
const BLUE_PROPERTY_LATIN_SMALL_BOTTOM: u16 = 1 << 6;

const BLUE_ACTIVE: u32 = 1 << 0;
const BLUE_TOP: u32 = 1 << 1;
const BLUE_SUB_TOP: u32 = 1 << 2;
const BLUE_NEUTRAL: u32 = 1 << 3;
const BLUE_ADJUSTMENT: u32 = 1 << 4;
const BLUE_BOTTOM: u32 = 1 << 5;
const BLUE_BOTTOM_SMALL: u32 = 1 << 6;

pub(super) const HINTS_HORZ_SNAP: u32 = 1 << 0;
pub(super) const HINTS_VERT_SNAP: u32 = 1 << 1;
pub(super) const HINTS_STEM_ADJUST: u32 = 1 << 2;
pub(super) const HINTS_MONO: u32 = 1 << 3;

const ADJUST_UP: u32 = 0x01;
const ADJUST_DOWN: u32 = 0x02;
const ADJUST_UP2: u32 = 0x04;
const ADJUST_DOWN2: u32 = 0x08;
const ADJUST_TILDE_TOP: u32 = 0x10;
const ADJUST_TILDE_BOTTOM: u32 = 0x20;
const ADJUST_TILDE_TOP2: u32 = 0x40;
const ADJUST_TILDE_BOTTOM2: u32 = 0x80;
const IGNORE_CAPITAL_TOP: u32 = 0x100;
const IGNORE_CAPITAL_BOTTOM: u32 = 0x200;
const IGNORE_SMALL_TOP: u32 = 0x400;
const IGNORE_SMALL_BOTTOM: u32 = 0x800;
const ADJUST_NO_HEIGHT_CHECK: u32 = 0x1000;

/// `AF_LATIN_CONSTANT`.
pub(super) fn constant(units_per_em: i64, c: i64) -> i64 {
    c * units_per_em / 2048
}

/// `AF_LatinBlueRec`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Blue {
    pub reference: Width,
    pub shoot: Width,
    pub ascender: i64,
    pub descender: i64,
    pub flags: u32,
}

/// `AF_LatinAxisRec`.
#[derive(Clone, Debug, Default)]
pub(super) struct LatinAxis {
    pub scale: i64,
    pub delta: i64,
    pub widths: Vec<Width>,
    pub edge_distance_threshold: i64,
    pub standard_width: i64,
    pub extra_light: bool,
    pub blues: Vec<Blue>,
}

/// `AF_LatinMetricsRec`.
#[derive(Clone, Debug, Default)]
pub(super) struct LatinMetrics {
    pub units_per_em: i64,
    pub axis: [LatinAxis; 2],
    pub digits_have_same_width: bool,
    /// The reverse character map: glyph and code point, sorted by glyph.
    pub reverse_charmap: Vec<(u32, u32)>,
    pub scaler: Scaler,
}

/// `af_latin_metrics_init_widths` (also used by the CJK writing system).
pub(super) fn init_widths(
    face: &Face<'_, '_, '_, '_>,
    script: usize,
    units_per_em: i64,
    axes: &mut [(Vec<Width>, i64, i64); 2],
) {
    let mut found = 0;
    let mut p = SCRIPTS[script].standard;
    while !p.is_empty() {
        while p.first() == Some(&b' ') {
            p = &p[1..];
        }
        let (rest, count, glyph) = next_cluster(face, p);
        p = rest;
        if count > 1 {
            continue;
        }
        found = glyph;
        if found != 0 {
            break;
        }
    }
    for (widths, _, _) in axes.iter_mut() {
        widths.clear();
    }
    'widths: {
        if found == 0 {
            break 'widths;
        }
        let Some(outline) = face.load_unscaled(found) else {
            break 'widths;
        };
        if outline.x.is_empty() {
            break 'widths;
        }
        let mut hints = GlyphHints {
            x_scale: 0x10000,
            y_scale: 0x10000,
            units_per_em: face.units_per_em,
            ..GlyphHints::default()
        };
        hints.reload(&outline.as_ref(), outline.postscript());
        for (dim, (widths, _, _)) in axes.iter_mut().enumerate() {
            compute_segments(&mut hints, dim, units_per_em);
            link_segments(&mut hints, &[], dim, units_per_em);
            let segments = &hints.axis[dim].segments;
            let mut table = [Width::default(); MAX_WIDTHS];
            let mut num_widths = 0;
            for (i, seg) in segments.iter().enumerate() {
                if let Some(link) = seg.link
                    && segments[link].link == Some(i)
                    && link > i
                {
                    let dist = (i64::from(seg.pos) - i64::from(segments[link].pos)).abs();
                    if num_widths < MAX_WIDTHS {
                        table[num_widths].org = dist;
                        num_widths += 1;
                    }
                }
            }
            let count = sort_and_quantize_widths(&mut table, num_widths, units_per_em / 100);
            widths.extend_from_slice(&table[..count]);
        }
    }
    for (widths, edge_distance_threshold, standard_width) in axes.iter_mut() {
        let stdw = widths
            .first()
            .map_or_else(|| constant(units_per_em, 50), |w| w.org);
        *edge_distance_threshold = stdw / 5;
        *standard_width = stdw;
    }
}

fn is_top_or_sub_top(flags: u32) -> bool {
    flags & (BLUE_TOP | BLUE_SUB_TOP) != 0
}

/// `af_latin_metrics_init_blues`; false when no blue zone is found.
fn init_blues(
    face: &Face<'_, '_, '_, '_>,
    metrics: &mut LatinMetrics,
    blue_stringset: usize,
) -> bool {
    let flat_threshold = metrics.units_per_em / 14;
    let axis = &mut metrics.axis[VERT];
    let mut set = blue_stringset;
    while BLUE_STRINGSETS[set].string != BLUE_STRING_MAX {
        let bs = BLUE_STRINGSETS[set];
        set += 1;
        let props = bs.properties;
        let is_top = props & BLUE_PROPERTY_LATIN_TOP != 0;
        let is_sub_top = props & BLUE_PROPERTY_LATIN_SUB_TOP != 0;
        let is_neutral = props & BLUE_PROPERTY_LATIN_NEUTRAL != 0;
        let is_long = props & BLUE_PROPERTY_LATIN_LONG != 0;
        let mut p = blue_string(bs.string);
        let mut flats = Vec::new();
        let mut rounds = Vec::new();
        let mut ascender: i64 = 0;
        let mut descender: i64 = 0;
        while !p.is_empty() {
            while p.first() == Some(&b' ') {
                p = &p[1..];
            }
            let (rest, num_idx, glyph_index) = next_cluster(face, p);
            p = rest;
            if num_idx == 0 {
                continue;
            }
            let mut best_y_extremum = if is_top { i64::MIN } else { i64::MAX };
            let mut best_round = false;
            for _ in 0..num_idx {
                let y_offset = 0;
                if glyph_index == 0 {
                    continue;
                }
                let Some(outline) = face.load_unscaled(glyph_index) else {
                    continue;
                };
                if outline.x.len() <= 2 {
                    continue;
                }
                let px = &outline.x;
                let py = &outline.y;
                let tags = &outline.tags;
                let on = |i: usize| tags[i] & 3 == 1;
                let mut best_point: i64 = -1;
                let mut best_contour_first: i64 = -1;
                let mut best_contour_last: i64 = -1;
                let mut best_y: i64 = 0;
                let mut last: i64 = -1;
                for &end in &outline.ends {
                    let first = last + 1;
                    last = end as i64;
                    if last <= first {
                        continue;
                    }
                    if is_top || is_sub_top {
                        for pp in first..=last {
                            let y = py[pp as usize];
                            if best_point < 0 || y > best_y {
                                best_point = pp;
                                best_y = y;
                                ascender = ascender.max(best_y + y_offset);
                            } else {
                                descender = descender.min(y + y_offset);
                            }
                        }
                    } else {
                        for pp in first..=last {
                            let y = py[pp as usize];
                            if best_point < 0 || y < best_y {
                                best_point = pp;
                                best_y = y;
                                descender = descender.min(best_y + y_offset);
                            } else {
                                ascender = ascender.max(y + y_offset);
                            }
                        }
                    }
                    if best_point > best_contour_last {
                        best_contour_first = first;
                        best_contour_last = last;
                    }
                }
                let mut round = false;
                if best_point >= 0 {
                    let bp = best_point as usize;
                    let bcf = best_contour_first as usize;
                    let bcl = best_contour_last as usize;
                    let best_x = px[bp];
                    let mut best_segment_first = bp;
                    let mut best_segment_last = bp;
                    let (mut best_on_point_first, mut best_on_point_last): (i64, i64) = if on(bp) {
                        (bp as i64, bp as i64)
                    } else {
                        (-1, -1)
                    };
                    let mut prev = bp;
                    let mut next = prev;
                    loop {
                        prev = if prev > bcf { prev - 1 } else { bcl };
                        let dist = (py[prev] - best_y).abs();
                        if dist > 5 && (px[prev] - best_x).abs() <= 20 * dist {
                            break;
                        }
                        best_segment_first = prev;
                        if on(prev) {
                            best_on_point_first = prev as i64;
                            if best_on_point_last < 0 {
                                best_on_point_last = prev as i64;
                            }
                        }
                        if prev == bp {
                            break;
                        }
                    }
                    loop {
                        next = if next < bcl { next + 1 } else { bcf };
                        let dist = (py[next] - best_y).abs();
                        if dist > 5 && (px[next] - best_x).abs() <= 20 * dist {
                            break;
                        }
                        best_segment_last = next;
                        if on(next) {
                            best_on_point_last = next as i64;
                            if best_on_point_first < 0 {
                                best_on_point_first = next as i64;
                            }
                        }
                        if next == bp {
                            break;
                        }
                    }
                    let mut skip_glyph = false;
                    if is_long {
                        let length_threshold = metrics.units_per_em / 25;
                        let dist = (px[best_segment_last] - px[best_segment_first]).abs();
                        if dist < length_threshold
                            && best_segment_last as i64 - best_segment_first as i64 + 2
                                <= best_contour_last - best_contour_first
                        {
                            let height_threshold = metrics.units_per_em / 4;
                            let mut p_first: i64 = 0;
                            let mut p_last: i64 = 0;
                            let mut prev = bp;
                            loop {
                                prev = if prev > bcf { prev - 1 } else { bcl };
                                if px[prev] != best_x {
                                    break;
                                }
                                if prev == bp {
                                    break;
                                }
                            }
                            if prev == bp {
                                skip_glyph = true;
                            } else {
                                let left2right = px[prev] < px[bp];
                                let mut first = best_segment_last;
                                let mut last = first;
                                let mut hit = false;
                                'segment: loop {
                                    'body: {
                                        if !hit {
                                            first = last;
                                            if on(first) {
                                                p_first = first as i64;
                                                p_last = first as i64;
                                            } else {
                                                p_first = -1;
                                                p_last = -1;
                                            }
                                            hit = true;
                                        }
                                        last = if last < bcl { last + 1 } else { bcf };
                                        if (best_y - py[first]).abs() > height_threshold {
                                            hit = false;
                                            break 'body;
                                        }
                                        let dist = (py[last] - py[first]).abs();
                                        if dist > 5 && (px[last] - px[first]).abs() <= 20 * dist {
                                            hit = false;
                                            break 'body;
                                        }
                                        if on(last) {
                                            p_last = last as i64;
                                            if p_first < 0 {
                                                p_first = last as i64;
                                            }
                                        }
                                        let l2r = px[first] < px[last];
                                        let d = (px[last] - px[first]).abs();
                                        if l2r == left2right && d >= length_threshold {
                                            loop {
                                                last = if last < bcl { last + 1 } else { bcf };
                                                let d = (py[last] - py[first]).abs();
                                                // FreeType tests `next` and `dist` here.
                                                if d > 5
                                                    && (px[next] - px[first]).abs() <= 20 * dist
                                                {
                                                    last = if last > bcf { last - 1 } else { bcl };
                                                    break;
                                                }
                                                p_last = last as i64;
                                                if on(last) {
                                                    p_last = last as i64;
                                                    if p_first < 0 {
                                                        p_first = last as i64;
                                                    }
                                                }
                                                if last == best_segment_first {
                                                    break;
                                                }
                                            }
                                            best_y = py[first];
                                            best_segment_first = first;
                                            best_segment_last = last;
                                            best_on_point_first = p_first;
                                            best_on_point_last = p_last;
                                            break 'segment;
                                        }
                                    }
                                    if last == best_segment_first {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if skip_glyph {
                        continue;
                    }
                    best_y += y_offset;
                    round = if best_on_point_first >= 0
                        && best_on_point_last >= 0
                        && (px[best_on_point_last as usize] - px[best_on_point_first as usize])
                            .abs()
                            > flat_threshold
                    {
                        false
                    } else {
                        !on(best_segment_first) || !on(best_segment_last)
                    };
                    if round && is_neutral {
                        continue;
                    }
                }
                if is_top {
                    if best_y > best_y_extremum {
                        best_y_extremum = best_y;
                        best_round = round;
                    }
                } else if best_y < best_y_extremum {
                    best_y_extremum = best_y;
                    best_round = round;
                }
            }
            if !(best_y_extremum == i64::MIN || best_y_extremum == i64::MAX) {
                if best_round {
                    rounds.push(best_y_extremum);
                } else {
                    flats.push(best_y_extremum);
                }
            }
        }
        if flats.is_empty() && rounds.is_empty() {
            continue;
        }
        sort_pos(&mut rounds);
        sort_pos(&mut flats);
        let mut blue = Blue::default();
        let (reference, shoot) = if flats.is_empty() {
            let v = rounds[rounds.len() / 2];
            (v, v)
        } else if rounds.is_empty() {
            let v = flats[flats.len() / 2];
            (v, v)
        } else {
            (flats[flats.len() / 2], rounds[rounds.len() / 2])
        };
        blue.reference.org = reference;
        blue.shoot.org = shoot;
        if shoot != reference {
            let over_ref = shoot > reference;
            if (is_top || is_sub_top) ^ over_ref {
                let mean = (shoot + reference) / 2;
                blue.reference.org = mean;
                blue.shoot.org = mean;
            }
        }
        blue.ascender = ascender;
        blue.descender = descender;
        blue.flags = 0;
        if is_top {
            blue.flags |= BLUE_TOP;
        }
        if is_sub_top {
            blue.flags |= BLUE_SUB_TOP;
        }
        if is_neutral {
            blue.flags |= BLUE_NEUTRAL;
        }
        if props & BLUE_PROPERTY_LATIN_CAPITAL_BOTTOM != 0 {
            blue.flags |= BLUE_BOTTOM;
        }
        if props & BLUE_PROPERTY_LATIN_SMALL_BOTTOM != 0 {
            blue.flags |= BLUE_BOTTOM_SMALL;
        }
        if props & BLUE_PROPERTY_LATIN_X_HEIGHT != 0 {
            blue.flags |= BLUE_ADJUSTMENT;
        }
        axis.blues.push(blue);
    }
    if axis.blues.is_empty() {
        return false;
    }
    let count = axis.blues.len();
    let mut sorted: Vec<usize> = (0..count).collect();
    let bottom = |b: &Blue| {
        if is_top_or_sub_top(b.flags) {
            b.reference.org
        } else {
            b.shoot.org
        }
    };
    for i in 1..count {
        let mut j = i;
        while j > 0 {
            let a = bottom(&axis.blues[sorted[j - 1]]);
            let b = bottom(&axis.blues[sorted[j]]);
            if b >= a {
                break;
            }
            sorted.swap(j, j - 1);
            j -= 1;
        }
    }
    for i in 0..count - 1 {
        let b_index = sorted[i + 1];
        let b = if is_top_or_sub_top(axis.blues[b_index].flags) {
            axis.blues[b_index].shoot.org
        } else {
            axis.blues[b_index].reference.org
        };
        let a_blue = &mut axis.blues[sorted[i]];
        let a = if is_top_or_sub_top(a_blue.flags) {
            &mut a_blue.shoot.org
        } else {
            &mut a_blue.reference.org
        };
        if *a > b {
            *a = b;
        }
    }
    true
}

/// The NUL-terminated blue string at an `AF_Blue_String` offset.
pub(super) fn blue_string(offset: u16) -> &'static [u8] {
    let s = &BLUE_STRINGS[usize::from(offset)..];
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    &s[..end]
}

/// `af_latin_metrics_check_digits` (also used by CJK).
pub(super) fn check_digits(face: &Face<'_, '_, '_, '_>) -> bool {
    let mut started = false;
    let mut old_advance = 0;
    let mut p: &[u8] = b"0 1 2 3 4 5 6 7 8 9";
    while !p.is_empty() {
        let (rest, num_idx, glyph_index) = next_cluster(face, p);
        p = rest;
        if num_idx > 1 {
            continue;
        }
        if glyph_index == 0 {
            continue;
        }
        let advance = face.get_advance(glyph_index);
        if started {
            if advance != old_advance {
                return false;
            }
        } else {
            old_advance = advance;
            started = true;
        }
    }
    true
}

/// `af_adjustment_database_lookup`, with FreeType's binary search over a
/// table that is not fully sorted.
fn adjustment_lookup(codepoint: u32) -> u32 {
    let mut low: i64 = 0;
    let mut high: i64 = ADJUSTMENTS.len() as i64 - 1;
    while high >= low {
        let mid = (low + high) / 2;
        let (mid_codepoint, flags) = ADJUSTMENTS[mid as usize];
        if mid_codepoint < codepoint {
            low = mid + 1;
        } else if mid_codepoint > codepoint {
            high = mid - 1;
        } else {
            return flags;
        }
    }
    0
}

/// `af_reverse_character_map_new` without HarfBuzz: the inverted cmap of
/// the code points in the adjustment database, later entries winning.
fn reverse_character_map(face: &Face<'_, '_, '_, '_>) -> Vec<(u32, u32)> {
    let mut map: Vec<(u32, u32)> = Vec::new();
    for &(codepoint, _) in ADJUSTMENTS {
        let glyph = face.char_index(codepoint);
        if glyph == 0 {
            continue;
        }
        match map.binary_search_by_key(&glyph, |&(g, _)| g) {
            Ok(i) => map[i].1 = codepoint,
            Err(i) => map.insert(i, (glyph, codepoint)),
        }
    }
    map
}

/// `af_latin_metrics_init`; `None` for FreeType's internal error -1 (no
/// blue zones), after which the caller moves the style's glyphs to the
/// dummy style.
pub(super) fn metrics_init(face: &Face<'_, '_, '_, '_>, style: usize) -> Option<LatinMetrics> {
    let mut metrics = LatinMetrics {
        units_per_em: face.units_per_em,
        ..LatinMetrics::default()
    };
    if face.has_unicode() {
        let script = usize::from(STYLES[style].script);
        let mut axes = [(Vec::new(), 0, 0), (Vec::new(), 0, 0)];
        init_widths(face, script, metrics.units_per_em, &mut axes);
        for (axis, (widths, threshold, standard)) in metrics.axis.iter_mut().zip(axes) {
            axis.widths = widths;
            axis.edge_distance_threshold = threshold;
            axis.standard_width = standard;
            axis.extra_light = false;
        }
        if !init_blues(
            face,
            &mut metrics,
            usize::from(STYLES[style].blue_stringset),
        ) {
            return None;
        }
        metrics.digits_have_same_width = check_digits(face);
    }
    metrics.reverse_charmap = reverse_character_map(face);
    Some(metrics)
}

/// `af_latin_metrics_scale_dim`.
fn scale_dim(metrics: &mut LatinMetrics, scaler: &Scaler, dim: usize) {
    let (mut scale, delta) = if dim == HORZ {
        (scaler.x_scale, scaler.x_delta)
    } else {
        (scaler.y_scale, scaler.y_delta)
    };
    if let Some(blue) = metrics.axis[VERT]
        .blues
        .iter()
        .find(|b| b.flags & BLUE_ADJUSTMENT != 0)
    {
        let scaled = mul_fix(blue.shoot.org, scale);
        let threshold = 40;
        let fitted = (scaled + threshold) & !63;
        if scaled != fitted && dim == VERT {
            let new_scale = mul_div(scale, fitted, scaled);
            let mut max_height = metrics.units_per_em;
            for b in &metrics.axis[VERT].blues {
                max_height = max_height.max(b.ascender);
                max_height = max_height.max(-b.descender);
            }
            let dist = mul_fix(max_height, new_scale - scale);
            if -128 < dist && dist < 128 {
                scale = new_scale;
            }
        }
    }
    let axis = &mut metrics.axis[dim];
    axis.scale = scale;
    axis.delta = delta;
    if dim == HORZ {
        metrics.scaler.x_scale = scale;
        metrics.scaler.x_delta = delta;
    } else {
        metrics.scaler.y_scale = scale;
        metrics.scaler.y_delta = delta;
    }
    let axis = &mut metrics.axis[dim];
    for width in &mut axis.widths {
        width.cur = mul_fix(width.org, scale);
        width.fit = width.cur;
    }
    axis.extra_light = mul_fix(axis.standard_width, scale) < 32 + 8;
    if dim == VERT {
        for blue in &mut axis.blues {
            blue.reference.cur = mul_fix(blue.reference.org, scale) + delta;
            blue.reference.fit = blue.reference.cur;
            blue.shoot.cur = mul_fix(blue.shoot.org, scale) + delta;
            blue.shoot.fit = blue.shoot.cur;
            blue.flags &= !BLUE_ACTIVE;
            let dist = mul_fix(blue.reference.org - blue.shoot.org, scale);
            if (-48..=48).contains(&dist) {
                let mut delta2 = dist.abs();
                delta2 = if delta2 < 32 {
                    0
                } else if delta2 < 48 {
                    32
                } else {
                    64
                };
                if dist < 0 {
                    delta2 = -delta2;
                }
                blue.reference.fit = pix_round(blue.reference.cur);
                blue.shoot.fit = blue.reference.fit - delta2;
                blue.flags |= BLUE_ACTIVE;
            }
        }
        for nn in 0..axis.blues.len() {
            let blue = axis.blues[nn];
            if blue.flags & BLUE_SUB_TOP == 0 || blue.flags & BLUE_ACTIVE == 0 {
                continue;
            }
            for i in 0..axis.blues.len() {
                let b = axis.blues[i];
                if b.flags & BLUE_SUB_TOP != 0 || b.flags & BLUE_ACTIVE == 0 {
                    continue;
                }
                if b.reference.fit <= blue.shoot.fit && b.shoot.fit >= blue.reference.fit {
                    axis.blues[nn].flags &= !BLUE_ACTIVE;
                    break;
                }
            }
        }
    }
}

/// `af_latin_metrics_scale`.
pub(super) fn metrics_scale(metrics: &mut LatinMetrics, scaler: &Scaler) {
    metrics.scaler.render_mode = scaler.render_mode;
    metrics.scaler.flags = scaler.flags;
    metrics.scaler.x_ppem = scaler.x_ppem;
    scale_dim(metrics, scaler, HORZ);
    scale_dim(metrics, scaler, VERT);
}

/// `af_latin_hints_compute_segments` (also used by CJK).
pub(super) fn compute_segments(hints: &mut GlyphHints, dim: usize, units_per_em: i64) {
    let flat_threshold = units_per_em / 14;
    let major_dir = hints.axis[dim].major_dir.abs();
    let mut segment_dir = major_dir;
    hints.axis[dim].segments.clear();
    if dim == HORZ {
        for point in &mut hints.points {
            point.u = point.fx;
            point.v = point.fy;
        }
    } else {
        for point in &mut hints.points {
            point.u = point.fy;
            point.v = point.fx;
        }
    }
    let points = &hints.points;
    let segments = &mut hints.axis[dim].segments;
    for &contour in &hints.contours {
        let mut point = contour;
        let mut last = points[point].prev;
        let mut on_edge = false;
        let mut min_pos: i64 = 32000;
        let mut max_pos: i64 = -32000;
        let mut min_coord: i64 = 32000;
        let mut max_coord: i64 = -32000;
        let mut min_flags: u16 = 0;
        let mut max_flags: u16 = 0;
        let mut min_on_coord: i64 = 32000;
        let mut max_on_coord: i64 = -32000;
        let mut segment: Option<usize> = None;
        let mut prev_segment: Option<usize> = None;
        let mut prev_min_pos = min_pos;
        let mut prev_max_pos = max_pos;
        let mut prev_min_coord = min_coord;
        let mut prev_max_coord = max_coord;
        let mut prev_min_flags = min_flags;
        let mut prev_max_flags = max_flags;
        let mut prev_min_on_coord = min_on_coord;
        let mut prev_max_on_coord = max_on_coord;

        if points[point].flags & FLAG_IGNORE == 0
            && points[last].out_dir.abs() == major_dir
            && points[point].out_dir.abs() == major_dir
        {
            last = point;
            loop {
                point = points[point].prev;
                if points[point].out_dir.abs() != major_dir {
                    point = points[point].next;
                    break;
                }
                if point == last {
                    break;
                }
            }
        }
        last = point;
        let mut passed = false;
        loop {
            if on_edge {
                let pt = points[point];
                let u = pt.u;
                if u < min_pos {
                    min_pos = u;
                }
                if u > max_pos {
                    max_pos = u;
                }
                let v = pt.v;
                if v < min_coord {
                    min_coord = v;
                    min_flags = pt.flags;
                }
                if v > max_coord {
                    max_coord = v;
                    max_flags = pt.flags;
                }
                if pt.flags & FLAG_CONTROL == 0 {
                    if v < min_on_coord {
                        min_on_coord = v;
                    }
                    if v > max_on_coord {
                        max_on_coord = v;
                    }
                }
                if pt.flags & FLAG_IGNORE != 0 || pt.out_dir != segment_dir || point == last {
                    let s = segment.unwrap_or(0);
                    let same = match prev_segment {
                        Some(ps) => segments[s].first == segments[ps].last,
                        None => false,
                    };
                    if pt.flags & FLAG_IGNORE != 0 || !same {
                        let seg = &mut segments[s];
                        seg.last = point;
                        seg.pos = ((min_pos + max_pos) >> 1) as i16;
                        seg.delta = ((max_pos - min_pos) >> 1) as i16;
                        if (min_flags | max_flags) & FLAG_CONTROL != 0
                            && (max_on_coord - min_on_coord) < flat_threshold
                        {
                            seg.flags |= EDGE_ROUND;
                        }
                        seg.min_coord = min_coord as i16;
                        seg.max_coord = max_coord as i16;
                        seg.height = (i32::from(seg.max_coord) - i32::from(seg.min_coord)) as i16;
                        prev_segment = Some(s);
                        prev_min_pos = min_pos;
                        prev_max_pos = max_pos;
                        prev_min_coord = min_coord;
                        prev_max_coord = max_coord;
                        prev_min_flags = min_flags;
                        prev_max_flags = max_flags;
                        prev_min_on_coord = min_on_coord;
                        prev_max_on_coord = max_on_coord;
                    } else {
                        let ps = prev_segment.unwrap_or(0);
                        if points[segments[ps].last].in_dir == pt.in_dir {
                            if prev_min_pos < min_pos {
                                min_pos = prev_min_pos;
                            }
                            if prev_max_pos > max_pos {
                                max_pos = prev_max_pos;
                            }
                            if prev_min_coord < min_coord {
                                min_coord = prev_min_coord;
                                min_flags = prev_min_flags;
                            }
                            if prev_max_coord > max_coord {
                                max_coord = prev_max_coord;
                                max_flags = prev_max_flags;
                            }
                            if prev_min_on_coord < min_on_coord {
                                min_on_coord = prev_min_on_coord;
                            }
                            if prev_max_on_coord > max_on_coord {
                                max_on_coord = prev_max_on_coord;
                            }
                            let prev = &mut segments[ps];
                            prev.last = point;
                            prev.pos = ((min_pos + max_pos) >> 1) as i16;
                            prev.delta = ((max_pos - min_pos) >> 1) as i16;
                            if (min_flags | max_flags) & FLAG_CONTROL != 0
                                && (max_on_coord - min_on_coord) < flat_threshold
                            {
                                prev.flags |= EDGE_ROUND;
                            } else {
                                prev.flags &= !EDGE_ROUND;
                            }
                            prev.min_coord = min_coord as i16;
                            prev.max_coord = max_coord as i16;
                            prev.height =
                                (i32::from(prev.max_coord) - i32::from(prev.min_coord)) as i16;
                        } else if (prev_max_coord - prev_min_coord).abs()
                            > (max_coord - min_coord).abs()
                        {
                            if min_pos < prev_min_pos {
                                prev_min_pos = min_pos;
                            }
                            if max_pos > prev_max_pos {
                                prev_max_pos = max_pos;
                            }
                            let prev = &mut segments[ps];
                            prev.last = point;
                            prev.pos = ((prev_min_pos + prev_max_pos) >> 1) as i16;
                            prev.delta = ((prev_max_pos - prev_min_pos) >> 1) as i16;
                        } else {
                            if prev_min_pos < min_pos {
                                min_pos = prev_min_pos;
                            }
                            if prev_max_pos > max_pos {
                                max_pos = prev_max_pos;
                            }
                            let mut seg = segments[s];
                            seg.last = point;
                            seg.pos = ((min_pos + max_pos) >> 1) as i16;
                            seg.delta = ((max_pos - min_pos) >> 1) as i16;
                            if (min_flags | max_flags) & FLAG_CONTROL != 0
                                && (max_on_coord - min_on_coord) < flat_threshold
                            {
                                seg.flags |= EDGE_ROUND;
                            }
                            seg.min_coord = min_coord as i16;
                            seg.max_coord = max_coord as i16;
                            seg.height =
                                (i32::from(seg.max_coord) - i32::from(seg.min_coord)) as i16;
                            segments[s] = seg;
                            segments[ps] = seg;
                            prev_min_pos = min_pos;
                            prev_max_pos = max_pos;
                            prev_min_coord = min_coord;
                            prev_max_coord = max_coord;
                            prev_min_flags = min_flags;
                            prev_max_flags = max_flags;
                            prev_min_on_coord = min_on_coord;
                            prev_max_on_coord = max_on_coord;
                        }
                        segments.pop();
                    }
                    on_edge = false;
                    segment = None;
                }
            }
            if point == last {
                if passed {
                    break;
                }
                passed = true;
            }
            let pt = points[point];
            if pt.flags & FLAG_IGNORE == 0
                && !on_edge
                && (pt.out_dir.abs() == major_dir || point == pt.prev)
            {
                if segments.len() > 1000 {
                    segments.clear();
                    return;
                }
                segment_dir = pt.out_dir;
                let s = segments.len();
                segments.push(Segment::new(segment_dir, point));
                if prev_segment.is_some() {
                    prev_segment = Some(s - 1);
                }
                min_pos = pt.u;
                max_pos = pt.u;
                min_coord = pt.v;
                max_coord = pt.v;
                min_flags = pt.flags;
                max_flags = pt.flags;
                if pt.flags & FLAG_CONTROL != 0 {
                    min_on_coord = 32000;
                    max_on_coord = -32000;
                } else {
                    min_on_coord = pt.v;
                    max_on_coord = pt.v;
                }
                on_edge = true;
                segment = Some(s);
                if point == pt.prev {
                    let seg = &mut segments[s];
                    seg.pos = min_pos as i16;
                    if pt.flags & FLAG_CONTROL != 0 {
                        seg.flags |= EDGE_ROUND;
                    }
                    seg.min_coord = pt.v as i16;
                    seg.max_coord = pt.v as i16;
                    seg.height = 0;
                    on_edge = false;
                    segment = None;
                }
            }
            point = points[point].next;
        }
    }
    for seg in segments.iter_mut() {
        let first = points[seg.first];
        let last = points[seg.last];
        let (first_v, last_v) = (first.v, last.v);
        if first_v < last_v {
            let p = points[first.prev];
            if p.v < first_v {
                seg.height = (i64::from(seg.height) + ((first_v - p.v) >> 1)) as i16;
            }
            let p = points[last.next];
            if p.v > last_v {
                seg.height = (i64::from(seg.height) + ((p.v - last_v) >> 1)) as i16;
            }
        } else {
            let p = points[first.prev];
            if p.v > first_v {
                seg.height = (i64::from(seg.height) + ((p.v - first_v) >> 1)) as i16;
            }
            let p = points[last.next];
            if p.v < last_v {
                seg.height = (i64::from(seg.height) + ((last_v - p.v) >> 1)) as i16;
            }
        }
    }
}

/// `af_latin_hints_link_segments` (also used by CJK for the widths).
pub(super) fn link_segments(
    hints: &mut GlyphHints,
    widths: &[Width],
    dim: usize,
    units_per_em: i64,
) {
    let major_dir = hints.axis[dim].major_dir;
    let segments = &mut hints.axis[dim].segments;
    let max_width = widths.last().map_or(0, |w| w.org);
    let mut len_threshold = constant(units_per_em, 8);
    if len_threshold == 0 {
        len_threshold = 1;
    }
    let len_score = constant(units_per_em, 6000);
    let dist_score = 3000;
    for s1 in 0..segments.len() {
        if segments[s1].dir != major_dir {
            continue;
        }
        for s2 in 0..segments.len() {
            let seg1 = segments[s1];
            let seg2 = segments[s2];
            let pos1 = i64::from(seg1.pos);
            let pos2 = i64::from(seg2.pos);
            if seg1.dir + seg2.dir == 0 && pos2 > pos1 {
                let min = i64::from(seg1.min_coord).max(i64::from(seg2.min_coord));
                let max = i64::from(seg1.max_coord).min(i64::from(seg2.max_coord));
                let len = max - min;
                if len >= len_threshold {
                    let dist = pos2 - pos1;
                    let dist_demerit = if max_width != 0 {
                        let delta = (dist << 10) / max_width - (1 << 10);
                        if delta > 10000 {
                            32000
                        } else if delta > 0 {
                            delta * delta / dist_score
                        } else {
                            0
                        }
                    } else {
                        dist
                    };
                    let score = dist_demerit + len_score / len;
                    if score < segments[s1].score {
                        segments[s1].score = score;
                        segments[s1].link = Some(s2);
                    }
                    if score < segments[s2].score {
                        segments[s2].score = score;
                        segments[s2].link = Some(s1);
                    }
                }
            }
        }
    }
    for s1 in 0..segments.len() {
        if let Some(s2) = segments[s1].link
            && segments[s2].link != Some(s1)
        {
            segments[s1].link = None;
            segments[s1].serif = segments[s2].link;
        }
    }
}

/// `af_latin_hints_compute_edges`.
fn compute_edges(
    hints: &mut GlyphHints,
    dim: usize,
    laxis: &LatinAxis,
    top_to_bottom_hinting: bool,
) {
    let scale = if dim == HORZ {
        hints.x_scale
    } else {
        hints.y_scale
    };
    let top_to_bottom = dim == VERT && top_to_bottom_hinting;
    let segment_length_threshold = if dim == HORZ {
        div_fix(64, hints.y_scale)
    } else {
        0
    };
    let segment_width_threshold = div_fix(32, scale);
    let mut edge_distance_threshold = mul_fix(laxis.edge_distance_threshold, scale);
    if edge_distance_threshold > 64 / 4 {
        edge_distance_threshold = 64 / 4;
    }
    let edge_distance_threshold = div_fix(edge_distance_threshold, scale);
    let axis = &mut hints.axis[dim];
    axis.edges.clear();
    for s in 0..axis.segments.len() {
        let seg = axis.segments[s];
        if i64::from(seg.height) < segment_length_threshold
            || i64::from(seg.delta) > segment_width_threshold
            || seg.dir == DIR_NONE
        {
            continue;
        }
        if seg.serif.is_some() && 2 * i64::from(seg.height) < 3 * segment_length_threshold {
            continue;
        }
        let found = axis.edges.iter().position(|edge| {
            (i64::from(seg.pos) - i64::from(edge.fpos)).abs() < edge_distance_threshold
                && edge.dir == seg.dir
        });
        match found {
            None => {
                let e = axis.new_edge(i64::from(seg.pos), seg.dir, top_to_bottom);
                let edge = &mut axis.edges[e];
                edge.first = s;
                edge.last = s;
                edge.dir = seg.dir;
                edge.fpos = seg.pos;
                edge.opos = mul_fix(i64::from(seg.pos), scale);
                edge.pos = edge.opos;
                axis.segments[s].edge_next = s;
            }
            Some(e) => {
                let (first, last) = (axis.edges[e].first, axis.edges[e].last);
                axis.segments[s].edge_next = first;
                axis.segments[last].edge_next = s;
                axis.edges[e].last = s;
            }
        }
    }
    for s in 0..axis.segments.len() {
        let seg = axis.segments[s];
        if seg.dir != DIR_NONE {
            continue;
        }
        let found = axis.edges.iter().position(|edge| {
            (i64::from(seg.pos) - i64::from(edge.fpos)).abs() < edge_distance_threshold
        });
        if let Some(e) = found {
            let (first, last) = (axis.edges[e].first, axis.edges[e].last);
            axis.segments[s].edge_next = first;
            axis.segments[last].edge_next = s;
            axis.edges[e].last = s;
        }
    }
    link_edges(axis, true);
}

/// The second half of `af_latin_hints_compute_edges` and
/// `af_cjk_hints_compute_edges`: segment edges, edge links and flags.
/// `latin` selects the Latin variant of the serif test and segment
/// distance.
pub(super) fn link_edges(axis: &mut super::hints::Axis, latin: bool) {
    for e in 0..axis.edges.len() {
        let first = axis.edges[e].first;
        let mut s = first;
        loop {
            axis.segments[s].edge = Some(e);
            s = axis.segments[s].edge_next;
            if s == first {
                break;
            }
        }
    }
    for e in 0..axis.edges.len() {
        let mut is_round = 0;
        let mut is_straight = 0;
        let first = axis.edges[e].first;
        let mut s = first;
        loop {
            let seg = axis.segments[s];
            if seg.flags & EDGE_ROUND != 0 {
                is_round += 1;
            } else {
                is_straight += 1;
            }
            let is_serif = match seg.serif {
                Some(serif) => {
                    let serif_edge = axis.segments[serif].edge;
                    if latin {
                        serif_edge.is_some() && serif_edge != Some(e)
                    } else {
                        serif_edge != Some(e)
                    }
                }
                None => false,
            };
            let linked = match seg.link {
                Some(link) => !latin || axis.segments[link].edge.is_some(),
                None => false,
            };
            if linked || is_serif {
                let (seg2, mut edge2) = if is_serif {
                    (seg.serif.unwrap_or(0), axis.edges[e].serif)
                } else {
                    (seg.link.unwrap_or(0), axis.edges[e].link)
                };
                if let Some(e2) = edge2 {
                    let edge_delta =
                        (i64::from(axis.edges[e].fpos) - i64::from(axis.edges[e2].fpos)).abs();
                    let seg_delta = (i64::from(seg.pos) - i64::from(axis.segments[seg2].pos)).abs();
                    if seg_delta < edge_delta {
                        edge2 = axis.segments[seg2].edge;
                    }
                } else {
                    edge2 = axis.segments[seg2].edge;
                }
                if is_serif {
                    axis.edges[e].serif = edge2;
                    if let Some(e2) = edge2 {
                        axis.edges[e2].flags |= EDGE_SERIF;
                    }
                } else {
                    axis.edges[e].link = edge2;
                }
            }
            s = seg.edge_next;
            if s == first {
                break;
            }
        }
        let edge = &mut axis.edges[e];
        edge.flags = EDGE_NORMAL;
        if is_round > 0 && is_round >= is_straight {
            edge.flags |= EDGE_ROUND;
        }
        if edge.serif.is_some() && edge.link.is_some() {
            edge.serif = None;
        }
    }
}

/// `af_latin_hints_detect_features`.
fn detect_features(
    hints: &mut GlyphHints,
    metrics: &LatinMetrics,
    dim: usize,
    top_to_bottom: bool,
) {
    compute_segments(hints, dim, metrics.units_per_em);
    link_segments(hints, &metrics.axis[dim].widths, dim, metrics.units_per_em);
    compute_edges(hints, dim, &metrics.axis[dim], top_to_bottom);
}

/// `af_latin_hints_compute_blue_edges`.
fn compute_blue_edges(hints: &mut GlyphHints, metrics: &LatinMetrics) {
    let latin = &metrics.axis[VERT];
    let scale = latin.scale;
    let axis = &mut hints.axis[VERT];
    let major_dir = axis.major_dir;
    for edge in &mut axis.edges {
        if edge.flags & EDGE_NO_BLUE != 0 {
            continue;
        }
        let mut best_blue: Option<i64> = None;
        let mut best_blue_is_neutral = false;
        let mut best_dist = mul_fix(metrics.units_per_em / 40, scale);
        if best_dist > 64 / 2 {
            best_dist = 64 / 2;
        }
        for blue in &latin.blues {
            if blue.flags & BLUE_ACTIVE == 0 {
                continue;
            }
            let is_top_blue = is_top_or_sub_top(blue.flags);
            let is_neutral_blue = blue.flags & BLUE_NEUTRAL != 0;
            let is_major_dir = edge.dir == major_dir;
            if is_top_blue ^ is_major_dir || is_neutral_blue {
                let fpos = i64::from(edge.fpos);
                let dist = mul_fix((fpos - blue.reference.org).abs(), scale);
                if dist < best_dist {
                    best_dist = dist;
                    best_blue = Some(blue.reference.fit);
                    best_blue_is_neutral = is_neutral_blue;
                }
                if edge.flags & EDGE_ROUND != 0 && dist != 0 && !is_neutral_blue {
                    let is_under_ref = fpos < blue.reference.org;
                    if is_top_blue ^ is_under_ref {
                        let dist = mul_fix((fpos - blue.shoot.org).abs(), scale);
                        if dist < best_dist {
                            best_dist = dist;
                            best_blue = Some(blue.shoot.fit);
                            best_blue_is_neutral = is_neutral_blue;
                        }
                    }
                }
            }
        }
        if let Some(fit) = best_blue {
            edge.blue_edge = Some(fit);
            if best_blue_is_neutral {
                edge.flags |= EDGE_NEUTRAL;
            }
        }
    }
}

/// `af_latin_hints_init`.
pub(super) fn hints_init(hints: &mut GlyphHints, metrics: &LatinMetrics, italic: bool) {
    hints.scaler_flags = metrics.scaler.flags;
    hints.x_scale = metrics.axis[HORZ].scale;
    hints.x_delta = metrics.axis[HORZ].delta;
    hints.y_scale = metrics.axis[VERT].scale;
    hints.y_delta = metrics.axis[VERT].delta;
    let mode = metrics.scaler.render_mode;
    let mut other_flags = 0;
    if matches!(mode, RenderMode::Mono | RenderMode::Lcd) {
        other_flags |= HINTS_HORZ_SNAP;
    }
    if matches!(mode, RenderMode::Mono | RenderMode::LcdV) {
        other_flags |= HINTS_VERT_SNAP;
    }
    if !matches!(mode, RenderMode::Light | RenderMode::Lcd) {
        other_flags |= HINTS_STEM_ADJUST;
    }
    if mode == RenderMode::Mono {
        other_flags |= HINTS_MONO;
    }
    if matches!(mode, RenderMode::Light | RenderMode::Lcd) || italic {
        hints.scaler_flags |= SCALER_FLAG_NO_HORIZONTAL;
    }
    hints.other_flags = other_flags;
}

/// `af_latin_snap_width` (also used by CJK as `af_cjk_snap_width`).
pub(super) fn snap_width(widths: &[Width], width: i64) -> i64 {
    let mut best = 64 + 32 + 2;
    let mut reference = width;
    for w in widths {
        let dist = (width - w.cur).abs();
        if dist < best {
            best = dist;
            reference = w.cur;
        }
    }
    let scaled = pix_round(reference);
    if width >= reference {
        if width < scaled + 48 {
            return reference;
        }
    } else if width > scaled - 48 {
        return reference;
    }
    width
}

/// `af_latin_compute_stem_width`.
fn compute_stem_width(
    hints: &GlyphHints,
    metrics: &LatinMetrics,
    dim: usize,
    width: i64,
    base_delta: i64,
    base_flags: u8,
    stem_flags: u8,
) -> i64 {
    let axis = &metrics.axis[dim];
    let vertical = dim == VERT;
    if hints.other_flags & HINTS_STEM_ADJUST == 0 || axis.extra_light {
        return width;
    }
    let mut dist = width.abs();
    let sign = width < 0;
    'done: {
        if (vertical && hints.other_flags & HINTS_VERT_SNAP == 0)
            || (!vertical && hints.other_flags & HINTS_HORZ_SNAP == 0)
        {
            if stem_flags & EDGE_SERIF != 0 && vertical && dist < 3 * 64 {
                break 'done;
            } else if base_flags & EDGE_ROUND != 0 {
                if dist < 80 {
                    dist = 64;
                }
            } else if dist < 56 {
                dist = 56;
            }
            if let Some(standard) = axis.widths.first() {
                let delta = (dist - standard.cur).abs();
                if delta < 40 {
                    dist = standard.cur;
                    if dist < 48 {
                        dist = 48;
                    }
                    break 'done;
                }
                if dist < 3 * 64 {
                    let delta = dist & 63;
                    dist &= -64;
                    if delta < 10 {
                        dist += delta;
                    } else if delta < 32 {
                        dist += 10;
                    } else if delta < 54 {
                        dist += 54;
                    } else {
                        dist += delta;
                    }
                } else {
                    let mut bdelta = 0;
                    if (width > 0 && base_delta > 0) || (width < 0 && base_delta < 0) {
                        let ppem = i64::from(metrics.scaler.x_ppem);
                        if ppem < 10 {
                            bdelta = base_delta;
                        } else if ppem < 30 {
                            bdelta = (base_delta * (30 - ppem)) / 20;
                        }
                        bdelta = bdelta.abs();
                    }
                    dist = (dist - bdelta + 32) & !63;
                }
            }
        } else {
            let org_dist = dist;
            dist = snap_width(&axis.widths, dist);
            if vertical {
                dist = if dist >= 64 { (dist + 16) & !63 } else { 64 };
            } else if hints.other_flags & HINTS_MONO != 0 {
                dist = if dist < 64 { 64 } else { (dist + 32) & !63 };
            } else if dist < 48 {
                dist = (dist + 64) >> 1;
            } else if dist < 128 {
                dist = (dist + 22) & !63;
                let delta = (dist - org_dist).abs();
                if delta >= 16 {
                    dist = org_dist;
                    if dist < 48 {
                        dist = (dist + 64) >> 1;
                    }
                }
            } else {
                dist = (dist + 32) & !63;
            }
        }
    }
    if sign { -dist } else { dist }
}

/// `af_latin_align_linked_edge`.
fn align_linked_edge(
    hints: &mut GlyphHints,
    metrics: &LatinMetrics,
    dim: usize,
    base: usize,
    stem: usize,
) {
    let edges = &hints.axis[dim].edges;
    let (b, s) = (edges[base], edges[stem]);
    let dist = s.opos - b.opos;
    let base_delta = b.pos - b.opos;
    let fitted = compute_stem_width(hints, metrics, dim, dist, base_delta, b.flags, s.flags);
    hints.axis[dim].edges[stem].pos = b.pos + fitted;
}

/// The extrema of the `v` values of the first and last points of an
/// edge's first and last segments.
fn edge_v_range(hints: &GlyphHints, dim: usize, e: usize) -> (i64, i64) {
    let axis = &hints.axis[dim];
    let edge = axis.edges[e];
    let v = |s: usize| {
        let seg = axis.segments[s];
        (hints.points[seg.first].v, hints.points[seg.last].v)
    };
    let (a, b) = v(edge.first);
    let (c, d) = v(edge.last);
    (a.min(b).min(c.min(d)), a.max(b).max(c.max(d)))
}

/// `af_latin_hint_edges`.
fn hint_edges(
    hints: &mut GlyphHints,
    metrics: &LatinMetrics,
    dim: usize,
    top_to_bottom_hinting: bool,
) {
    let n_edges = hints.axis[dim].edges.len();
    let mut anchor: Option<usize> = None;
    let mut has_non_stem_edges = false;
    let top_to_bottom = dim == VERT && top_to_bottom_hinting;

    if dim == VERT {
        for e in 0..n_edges {
            let edges = &mut hints.axis[dim].edges;
            if edges[e].flags & EDGE_DONE != 0 {
                continue;
            }
            let mut edge2 = edges[e].link;
            if let Some(e2) = edge2
                && edges[e].blue_edge.is_some()
                && edges[e2].blue_edge.is_some()
            {
                let neutral = edges[e].flags & EDGE_NEUTRAL;
                let neutral2 = edges[e2].flags & EDGE_NEUTRAL;
                if neutral2 != 0 {
                    edges[e2].blue_edge = None;
                    edges[e2].flags &= !EDGE_NEUTRAL;
                } else if neutral != 0 {
                    edges[e].blue_edge = None;
                    edges[e].flags &= !EDGE_NEUTRAL;
                }
            }
            let mut blue = edges[e].blue_edge;
            let mut edge1 = None;
            if blue.is_some() {
                edge1 = Some(e);
            } else if let Some(e2) = edge2
                && edges[e2].blue_edge.is_some()
            {
                blue = edges[e2].blue_edge;
                edge1 = Some(e2);
                edge2 = Some(e);
            }
            let (Some(edge1), Some(fit)) = (edge1, blue) else {
                continue;
            };
            edges[edge1].pos = fit;
            edges[edge1].flags |= EDGE_DONE;
            if let Some(e2) = edge2
                && edges[e2].blue_edge.is_none()
            {
                align_linked_edge(hints, metrics, dim, edge1, e2);
                hints.axis[dim].edges[e2].flags |= EDGE_DONE;
            }
            if anchor.is_none() {
                anchor = Some(e);
            }
        }
    }

    for e in 0..n_edges {
        let edge = hints.axis[dim].edges[e];
        if edge.flags & EDGE_DONE != 0 {
            continue;
        }
        let Some(e2) = edge.link else {
            has_non_stem_edges = true;
            continue;
        };
        let edge2 = hints.axis[dim].edges[e2];
        if edge2.blue_edge.is_some() {
            align_linked_edge(hints, metrics, dim, e2, e);
            hints.axis[dim].edges[e].flags |= EDGE_DONE;
            continue;
        }
        match anchor {
            None => {
                let org_len = edge2.opos - edge.opos;
                let cur_len =
                    compute_stem_width(hints, metrics, dim, org_len, 0, edge.flags, edge2.flags);
                let (u_off, d_off) = if cur_len <= 64 { (32, 32) } else { (38, 26) };
                let edges = &mut hints.axis[dim].edges;
                if cur_len < 96 {
                    let org_center = edge.opos + (org_len >> 1);
                    let mut cur_pos1 = pix_round(org_center);
                    let error1 = (org_center - (cur_pos1 - u_off)).abs();
                    let error2 = (org_center - (cur_pos1 + d_off)).abs();
                    if error1 < error2 {
                        cur_pos1 -= u_off;
                    } else {
                        cur_pos1 += d_off;
                    }
                    edges[e].pos = cur_pos1 - cur_len / 2;
                    edges[e2].pos = edges[e].pos + cur_len;
                } else {
                    edges[e].pos = pix_round(edge.opos);
                }
                anchor = Some(e);
                edges[e].flags |= EDGE_DONE;
                align_linked_edge(hints, metrics, dim, e, e2);
            }
            Some(a) => {
                let anchor_edge = hints.axis[dim].edges[a];
                let org_pos = anchor_edge.pos + (edge.opos - anchor_edge.opos);
                let org_len = edge2.opos - edge.opos;
                let org_center = org_pos + (org_len >> 1);
                let cur_len =
                    compute_stem_width(hints, metrics, dim, org_len, 0, edge.flags, edge2.flags);
                let edges = &mut hints.axis[dim].edges;
                if edge2.flags & EDGE_DONE != 0 {
                    edges[e].pos = edge2.pos - cur_len;
                } else if cur_len < 96 {
                    let mut cur_pos1 = pix_round(org_center);
                    let (u_off, d_off) = if cur_len <= 64 { (32, 32) } else { (38, 26) };
                    let delta1 = (org_center - (cur_pos1 - u_off)).abs();
                    let delta2 = (org_center - (cur_pos1 + d_off)).abs();
                    if delta1 < delta2 {
                        cur_pos1 -= u_off;
                    } else {
                        cur_pos1 += d_off;
                    }
                    edges[e].pos = cur_pos1 - cur_len / 2;
                    edges[e2].pos = cur_pos1 + cur_len / 2;
                } else {
                    let cur_pos1 = pix_round(org_pos);
                    let delta1 = (cur_pos1 + (cur_len >> 1) - org_center).abs();
                    let cur_pos2 = pix_round(org_pos + org_len) - cur_len;
                    let delta2 = (cur_pos2 + (cur_len >> 1) - org_center).abs();
                    edges[e].pos = if delta1 < delta2 { cur_pos1 } else { cur_pos2 };
                    edges[e2].pos = edges[e].pos + cur_len;
                }
                edges[e].flags |= EDGE_DONE;
                edges[e2].flags |= EDGE_DONE;
                if e > 0 {
                    let pos = edges[e].pos;
                    let prev = edges[e - 1].pos;
                    if if top_to_bottom {
                        pos > prev
                    } else {
                        pos < prev
                    } && let Some(l) = edges[e].link
                        && (edges[l].pos - prev).abs() > 16
                    {
                        edges[e].pos = prev;
                    }
                }
            }
        }
    }

    if dim == HORZ && (n_edges == 6 || n_edges == 12) {
        let (edge1, edge2, edge3) = if n_edges == 6 { (0, 2, 4) } else { (1, 5, 9) };
        let edges = &mut hints.axis[dim].edges;
        let dist1 = edges[edge2].opos - edges[edge1].opos;
        let dist2 = edges[edge3].opos - edges[edge2].opos;
        let span = (dist1 - dist2).abs();
        if span < 8 {
            let delta = edges[edge3].pos - (2 * edges[edge2].pos - edges[edge1].pos);
            edges[edge3].pos -= delta;
            if let Some(l) = edges[edge3].link {
                edges[l].pos -= delta;
            }
            if n_edges == 12 {
                edges[8].pos -= delta;
                edges[11].pos -= delta;
            }
            edges[edge3].flags |= EDGE_DONE;
            if let Some(l) = edges[edge3].link {
                edges[l].flags |= EDGE_DONE;
            }
        }
    }

    if has_non_stem_edges || anchor.is_none() {
        for e in 0..n_edges {
            let edge = hints.axis[dim].edges[e];
            if edge.flags & EDGE_DONE != 0 {
                continue;
            }
            let mut delta = 1000;
            if let Some(serif) = edge.serif {
                delta = hints.axis[dim].edges[serif].opos - edge.opos;
                let (top, bottom) = if delta < 0 {
                    delta = -delta;
                    (e, serif)
                } else {
                    (serif, e)
                };
                if delta < 64 + 32 {
                    let (top_min, top_max) = edge_v_range(hints, dim, top);
                    let (bottom_min, bottom_max) = edge_v_range(hints, dim, bottom);
                    let min_pos = top_min.min(bottom_min);
                    let max_pos = top_max.max(bottom_max);
                    let mut x = bottom + 1;
                    while x < top {
                        let (e_min, e_max) = edge_v_range(hints, dim, x);
                        if !((e_min < min_pos && e_max < min_pos)
                            || (e_min > max_pos && e_max > max_pos))
                        {
                            delta = 1000;
                            break;
                        }
                        x += 1;
                    }
                    if delta == 1000 {
                        continue;
                    }
                }
            }
            let edges = &mut hints.axis[dim].edges;
            if delta < 64 + 16 {
                let base = edges[edge.serif.unwrap_or(e)];
                edges[e].pos = base.pos + (edge.opos - base.opos);
            } else if let Some(a) = anchor {
                let mut before = None;
                let mut b = e;
                while b > 0 {
                    b -= 1;
                    if edges[b].flags & EDGE_DONE != 0 {
                        before = Some(b);
                        break;
                    }
                }
                let after = (e + 1..n_edges).find(|&x| edges[x].flags & EDGE_DONE != 0);
                if let (Some(b), Some(af)) = (before, after) {
                    let (before, after) = (edges[b], edges[af]);
                    edges[e].pos = if after.opos == before.opos {
                        before.pos
                    } else {
                        before.pos
                            + mul_div(
                                edge.opos - before.opos,
                                after.pos - before.pos,
                                after.opos - before.opos,
                            )
                    };
                } else {
                    edges[e].pos = edges[a].pos + ((edge.opos - edges[a].opos + 16) & !31);
                }
            } else {
                edges[e].pos = pix_round(edge.opos);
                anchor = Some(e);
            }
            edges[e].flags |= EDGE_DONE;
            if e > 0 {
                let pos = edges[e].pos;
                let prev = edges[e - 1].pos;
                if if top_to_bottom {
                    pos > prev
                } else {
                    pos < prev
                } && let Some(l) = edges[e].link
                    && (edges[l].pos - prev).abs() > 16
                {
                    edges[e].pos = prev;
                }
            }
            if e + 1 < n_edges && edges[e + 1].flags & EDGE_DONE != 0 {
                let pos = edges[e].pos;
                let next = edges[e + 1].pos;
                if if top_to_bottom {
                    pos < next
                } else {
                    pos > next
                } && let Some(l) = edges[e].link
                    && e > 0
                    && (edges[l].pos - edges[e - 1].pos).abs() > 16
                {
                    edges[e].pos = next;
                }
            }
        }
    }
}

// ---- vertical separation adjustments (aflatin.c, FT_COMPONENT afadjust) ----

fn move_contour_vertically(hints: &mut GlyphHints, contour: usize, movement: i64) {
    let first = hints.contours[contour];
    let mut p = first;
    loop {
        hints.points[p].y = hints.points[p].y.wrapping_add(movement);
        p = hints.points[p].next;
        if p == first {
            break;
        }
    }
}

fn move_contours_up(hints: &mut GlyphHints, limit: i64, delta: i64) {
    for c in 0..hints.contours.len() {
        let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
        if min_y < max_y && min_y > limit {
            move_contour_vertically(hints, c, delta);
        }
    }
}

fn move_contours_down(hints: &mut GlyphHints, limit: i64, delta: i64) {
    for c in 0..hints.contours.len() {
        let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
        if min_y < max_y && max_y < limit {
            move_contour_vertically(hints, c, -delta);
        }
    }
}

fn compute_vertical_extrema(hints: &mut GlyphHints) {
    for c in 0..hints.contours.len() {
        let mut min_y = i64::MAX;
        let mut max_y = i64::MIN;
        let first = hints.contours[c];
        if hints.points[hints.points[first].next].next != first {
            let mut p = first;
            loop {
                min_y = min_y.min(hints.points[p].y);
                max_y = max_y.max(hints.points[p].y);
                p = hints.points[p].next;
                if p == first {
                    break;
                }
            }
        }
        hints.contour_y_minima[c] = min_y;
        hints.contour_y_maxima[c] = max_y;
    }
}

fn find_highest_contour(hints: &GlyphHints) -> usize {
    let mut highest = 0;
    let mut highest_min_y = i64::MAX;
    let mut highest_max_y = i64::MIN;
    for c in 0..hints.contours.len() {
        let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
        if max_y > highest_max_y || (max_y == highest_max_y && min_y > highest_min_y) {
            highest_min_y = min_y;
            highest_max_y = max_y;
            highest = c;
        }
    }
    highest
}

fn find_second_highest_contour(hints: &GlyphHints) -> usize {
    if hints.contours.len() < 3 {
        return 0;
    }
    let highest = find_highest_contour(hints);
    let highest_min_y = hints.contour_y_minima[highest];
    let mut second = 0;
    let mut second_max_y = i64::MIN;
    for c in 0..hints.contours.len() {
        if c == highest {
            continue;
        }
        let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
        if max_y > second_max_y && min_y < highest_min_y {
            second_max_y = max_y;
            second = c;
        }
    }
    second
}

fn find_lowest_contour(hints: &GlyphHints) -> usize {
    let mut lowest = 0;
    let mut lowest_min_y = i64::MAX;
    let mut lowest_max_y = i64::MIN;
    for c in 0..hints.contours.len() {
        let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
        if min_y < lowest_min_y || (min_y == lowest_min_y && max_y < lowest_max_y) {
            lowest_min_y = min_y;
            lowest_max_y = max_y;
            lowest = c;
        }
    }
    lowest
}

fn find_second_lowest_contour(hints: &GlyphHints) -> usize {
    if hints.contours.len() < 3 {
        return 0;
    }
    let lowest = find_lowest_contour(hints);
    let lowest_max_y = hints.contour_y_maxima[lowest];
    let mut second = 0;
    let mut second_min_y = i64::MAX;
    for c in 0..hints.contours.len() {
        if c == lowest {
            continue;
        }
        let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
        if min_y < second_min_y && max_y > lowest_max_y {
            second_min_y = min_y;
            second = c;
        }
    }
    second
}

fn prevent_blue_alignment(hints: &mut GlyphHints, pos: i64, top: bool) {
    for edge in &mut hints.axis[VERT].edges {
        if if top { edge.pos > pos } else { edge.pos < pos } {
            edge.flags |= EDGE_NO_BLUE;
        }
    }
}

/// `af_latin_get_base_glyph_blues`.
fn base_glyph_blues(metrics: &LatinMetrics, is_capital: bool) -> (Option<Blue>, Option<Blue>) {
    let blues = &metrics.axis[VERT].blues;
    let top_flag = if is_capital {
        BLUE_TOP
    } else {
        BLUE_ADJUSTMENT
    } | BLUE_ACTIVE;
    let bottom_flag = if is_capital {
        BLUE_BOTTOM
    } else {
        BLUE_BOTTOM_SMALL
    } | BLUE_ACTIVE;
    (
        blues
            .iter()
            .find(|b| b.flags & top_flag == top_flag)
            .copied(),
        blues
            .iter()
            .find(|b| b.flags & bottom_flag == bottom_flag)
            .copied(),
    )
}

fn touch_contour(hints: &mut GlyphHints, contour: usize) {
    let first = hints.contours[contour];
    let mut p = first;
    loop {
        p = hints.points[p].next;
        hints.points[p].flags |= FLAG_IGNORE;
        if hints.points[p].flags & FLAG_CONTROL == 0 {
            hints.points[p].flags |= FLAG_TOUCH_Y;
        }
        if p == first {
            break;
        }
    }
}

fn touch_top_contours(hints: &mut GlyphHints, limit_contour: usize) {
    let limit = hints.contour_y_minima[limit_contour];
    for c in 0..hints.contours.len() {
        let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
        if min_y < max_y && min_y >= limit {
            touch_contour(hints, c);
        }
    }
}

fn touch_bottom_contours(hints: &mut GlyphHints, limit_contour: usize) {
    let limit = hints.contour_y_minima[limit_contour];
    for c in 0..hints.contours.len() {
        let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
        if min_y < max_y && max_y <= limit {
            touch_contour(hints, c);
        }
    }
}

/// The measurement loop shared by `af_latin_stretch_top_tilde` and
/// `af_latin_stretch_bottom_tilde`.
fn tilde_min_measurement(hints: &GlyphHints, contour: usize, min_y: i64, max_y: i64) -> i64 {
    let height = max_y.wrapping_sub(min_y);
    let extremum_threshold = height / 8;
    let mut min_measurement = i64::MAX;
    let mut taken = false;
    let first = hints.contours[contour];
    let mut p = first;
    loop {
        p = hints.points[p].next;
        let pt = hints.points[p];
        let prev = hints.points[pt.prev];
        let next = hints.points[pt.next];
        if pt.flags & FLAG_CONTROL == 0
            && prev.y == pt.y
            && next.y == pt.y
            && pt.y != min_y
            && pt.y != max_y
            && prev.flags & FLAG_CONTROL != 0
            && next.flags & FLAG_CONTROL != 0
        {
            let mut prev_on = pt.prev;
            let mut next_on = pt.next;
            while hints.points[prev_on].flags & FLAG_CONTROL != 0 {
                prev_on = hints.points[prev_on].prev;
            }
            while hints.points[next_on].flags & FLAG_CONTROL != 0 {
                next_on = hints.points[next_on].next;
            }
            let (py, ny) = (hints.points[prev_on].y, hints.points[next_on].y);
            let measurement = if ny > pt.y && py > pt.y {
                Some(pt.y.wrapping_sub(min_y))
            } else if ny < pt.y && py < pt.y {
                Some(max_y.wrapping_sub(pt.y))
            } else {
                None
            };
            if let Some(m) = measurement
                && m >= extremum_threshold
                && (!taken || m < min_measurement)
            {
                taken = true;
                min_measurement = m;
            }
        }
        if p == first {
            break;
        }
    }
    if taken { min_measurement } else { 0 }
}

fn stretch_tilde(hints: &mut GlyphHints, contour: usize, top: bool) -> i64 {
    let min_y = hints.contour_y_minima[contour];
    let max_y = hints.contour_y_maxima[contour];
    if min_y == max_y {
        return 0;
    }
    let height = max_y.wrapping_sub(min_y);
    let min_measurement = tilde_min_measurement(hints, contour, min_y, max_y);
    if top {
        touch_top_contours(hints, contour);
    } else {
        touch_bottom_contours(hints, contour);
    }
    let target_height = min_measurement + 64;
    if height >= target_height {
        return 0;
    }
    let first = hints.contours[contour];
    let mut p = first;
    let anchor = if top { min_y } else { max_y };
    loop {
        p = hints.points[p].next;
        let y = hints.points[p].y;
        hints.points[p].y =
            (y.wrapping_sub(anchor).wrapping_mul(target_height) / height).wrapping_add(anchor);
        if p == first {
            break;
        }
    }
    target_height - height
}

fn align_tilde(hints: &mut GlyphHints, contour: usize, top: bool) -> i64 {
    let first = hints.contours[contour];
    let mut min_y = hints.points[first].y;
    let mut max_y = min_y;
    let mut p = first;
    loop {
        p = hints.points[p].next;
        min_y = min_y.min(hints.points[p].y);
        max_y = max_y.max(hints.points[p].y);
        if p == first {
            break;
        }
    }
    let height = max_y.wrapping_sub(min_y);
    let mut delta = if top {
        pix_round(min_y).wrapping_sub(min_y)
    } else {
        pix_round(max_y).wrapping_sub(max_y)
    };
    if height < 64 * 3 {
        let centering = (pix_round(height) - height) / 2;
        if top {
            delta += centering;
        } else {
            delta -= centering;
        }
    }
    let first_contour = contour;
    move_contour_vertically(hints, first_contour, delta);
    delta
}

fn contour_horizontal_overlap(hints: &GlyphHints, index: usize) -> bool {
    let mut contour_max_x = i64::MIN;
    let mut contour_min_x = i64::MAX;
    let mut others_max_x = i64::MIN;
    let mut others_min_x = i64::MAX;
    for c in 0..hints.contours.len() {
        let first = hints.contours[c];
        if hints.points[hints.points[first].next].next == first {
            continue;
        }
        let mut p = first;
        loop {
            p = hints.points[p].next;
            let x = hints.points[p].x;
            if c == index {
                contour_min_x = contour_min_x.min(x);
                contour_max_x = contour_max_x.max(x);
            } else {
                others_min_x = others_min_x.min(x);
                others_max_x = others_max_x.max(x);
            }
            if p == first {
                break;
            }
        }
    }
    (others_min_x <= contour_max_x && contour_max_x <= others_max_x)
        || (others_min_x <= contour_min_x && contour_min_x <= others_max_x)
        || (contour_max_x >= others_max_x && contour_min_x <= others_min_x)
}

fn adjustment_type(metrics: &LatinMetrics, glyph: u32) -> u32 {
    metrics
        .reverse_charmap
        .binary_search_by_key(&glyph, |&(g, _)| g)
        .map_or(0, |i| adjustment_lookup(metrics.reverse_charmap[i].1))
}

/// `af_glyph_hints_apply_vertical_separation_adjustments`.
fn apply_vertical_separation_adjustments(
    hints: &mut GlyphHints,
    dim: usize,
    adj_type: u32,
    accent_height_limit: i64,
) {
    if dim != VERT {
        return;
    }
    let adjust_top = adj_type & ADJUST_UP != 0;
    let adjust_below_top = adj_type & ADJUST_UP2 != 0;
    let adjust_bottom = adj_type & ADJUST_DOWN != 0;
    let adjust_above_bottom = adj_type & ADJUST_DOWN2 != 0;
    let n = hints.contours.len();
    if ((adjust_top || adjust_bottom) && n >= 2)
        || ((adjust_below_top || adjust_above_bottom) && n >= 3)
    {
        compute_vertical_extrema(hints);
    }
    if (adjust_top && n >= 2) || (adjust_below_top && n >= 3) {
        let is_top_tilde = adj_type & ADJUST_TILDE_TOP != 0;
        let is_below_top_tilde = adj_type & ADJUST_TILDE_TOP2 != 0;
        let high_contour = if adjust_below_top {
            find_second_highest_contour(hints)
        } else {
            find_highest_contour(hints)
        };
        if !contour_horizontal_overlap(hints, high_contour) {
            return;
        }
        let high_min_y = hints.contour_y_minima[high_contour];
        let high_max_y = hints.contour_y_maxima[high_contour];
        let high_height = high_max_y.wrapping_sub(high_min_y);
        if high_height > accent_height_limit {
            return;
        }
        let mut min_distance = 64;
        for c in 0..n {
            if c == high_contour {
                continue;
            }
            let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
            let distance = high_min_y.wrapping_sub(max_y);
            if distance < 64 && distance < min_distance && min_y < high_min_y {
                min_distance = distance;
            }
        }
        let adjustment_amount = 64 - min_distance;
        let mut centering_adjustment = 0;
        if is_top_tilde || is_below_top_tilde {
            let tilde_contour = if adjust_top || is_below_top_tilde {
                high_contour
            } else {
                find_highest_contour(hints)
            };
            let tilde_height = hints.contour_y_maxima[tilde_contour]
                .wrapping_sub(hints.contour_y_minima[tilde_contour]);
            let mut pos = high_min_y.wrapping_add(adjustment_amount);
            if adjust_below_top && is_top_tilde {
                pos = pos.wrapping_add(high_height);
            }
            if pos % 64 == 0 && tilde_height < 3 * 64 {
                centering_adjustment = (pix_round(tilde_height) - tilde_height) / 2;
            }
        }
        let calculated_amount =
            if (adjust_top && is_top_tilde) || (adjust_below_top && is_below_top_tilde) {
                adjustment_amount + centering_adjustment
            } else {
                adjustment_amount
            };
        if calculated_amount != 0
            && calculated_amount >= -2
            && (calculated_amount <= 66 || adjustment_amount <= 66)
        {
            let height_delta = high_height / 8;
            let min_y_limit = high_min_y.wrapping_sub(height_delta);
            move_contours_up(hints, min_y_limit, adjustment_amount);
            if adjust_below_top && is_top_tilde {
                move_contours_up(
                    hints,
                    min_y_limit.wrapping_add(high_height),
                    centering_adjustment,
                );
            }
        }
    }
    if (adjust_bottom && n >= 2) || (adjust_above_bottom && n >= 3) {
        let is_bottom_tilde = adj_type & ADJUST_TILDE_BOTTOM != 0;
        let is_above_bottom_tilde = adj_type & ADJUST_TILDE_BOTTOM2 != 0;
        let low_contour = if adjust_above_bottom {
            find_second_lowest_contour(hints)
        } else {
            find_lowest_contour(hints)
        };
        if !contour_horizontal_overlap(hints, low_contour) {
            return;
        }
        let low_min_y = hints.contour_y_minima[low_contour];
        let low_max_y = hints.contour_y_maxima[low_contour];
        let low_height = low_max_y.wrapping_sub(low_min_y);
        if low_height > accent_height_limit {
            return;
        }
        let mut min_distance = 64;
        for c in 0..n {
            if c == low_contour {
                continue;
            }
            let (min_y, max_y) = (hints.contour_y_minima[c], hints.contour_y_maxima[c]);
            let distance = min_y.wrapping_sub(low_max_y);
            if distance < 64 && distance < min_distance && max_y > low_max_y {
                min_distance = distance;
            }
        }
        let adjustment_amount = 64 - min_distance;
        let mut centering_adjustment = 0;
        if is_bottom_tilde || is_above_bottom_tilde {
            let tilde_contour = if adjust_bottom || is_above_bottom_tilde {
                low_contour
            } else {
                find_lowest_contour(hints)
            };
            let tilde_height = hints.contour_y_maxima[tilde_contour]
                .wrapping_sub(hints.contour_y_minima[tilde_contour]);
            let mut pos = low_max_y.wrapping_sub(adjustment_amount);
            if adjust_above_bottom && is_bottom_tilde {
                pos = pos.wrapping_sub(low_height);
            }
            if pos % 64 == 0 && tilde_height < 3 * 64 {
                centering_adjustment = (pix_round(tilde_height) - tilde_height) / 2;
            }
        }
        let calculated_amount = if (adjust_bottom && is_bottom_tilde)
            || (adjust_above_bottom && is_above_bottom_tilde)
        {
            adjustment_amount + centering_adjustment
        } else {
            adjustment_amount
        };
        if calculated_amount != 0
            && calculated_amount >= -2
            && (calculated_amount <= 66 || adjustment_amount <= 66)
        {
            let height_delta = low_height / 8;
            let max_y_limit = low_max_y.wrapping_add(height_delta);
            move_contours_down(hints, max_y_limit, adjustment_amount);
            if adjust_above_bottom && is_bottom_tilde {
                move_contours_down(
                    hints,
                    max_y_limit.wrapping_sub(low_height),
                    centering_adjustment,
                );
            }
        }
    }
}

/// `af_latin_hints_apply`.
pub(super) fn hints_apply(
    glyph: u32,
    nonbase: bool,
    hints: &mut GlyphHints,
    outline: &OutlineRef<'_>,
    postscript: bool,
    metrics: &LatinMetrics,
    top_to_bottom: bool,
) {
    hints.reload(outline, postscript);
    let mut accent_height_limit = 0;
    let mut adj_type = 0;
    if hints.scaler_flags & SCALER_FLAG_NO_HORIZONTAL == 0 {
        detect_features(hints, metrics, HORZ, top_to_bottom);
    }
    if hints.scaler_flags & super::hints::SCALER_FLAG_NO_VERTICAL == 0 {
        adj_type = adjustment_type(metrics, glyph);
        let have_flags = adj_type != 0;
        let is_top_tilde = adj_type & ADJUST_TILDE_TOP != 0;
        let is_bottom_tilde = adj_type & ADJUST_TILDE_BOTTOM != 0;
        let is_below_top_tilde = adj_type & ADJUST_TILDE_TOP2 != 0;
        let is_above_bottom_tilde = adj_type & ADJUST_TILDE_BOTTOM2 != 0;
        let ignore_capital_top = adj_type & IGNORE_CAPITAL_TOP != 0;
        let ignore_capital_bottom = adj_type & IGNORE_CAPITAL_BOTTOM != 0;
        let ignore_small_top = adj_type & IGNORE_SMALL_TOP != 0;
        let ignore_small_bottom = adj_type & IGNORE_SMALL_BOTTOM != 0;
        let do_height_check = adj_type & ADJUST_NO_HEIGHT_CHECK == 0;

        if is_top_tilde || is_bottom_tilde || is_below_top_tilde || is_above_bottom_tilde {
            compute_vertical_extrema(hints);
        }
        if is_below_top_tilde {
            let contour = find_second_highest_contour(hints);
            let mut y_offset = stretch_tilde(hints, contour, true);
            y_offset += align_tilde(hints, contour, true);
            let limit = hints.contour_y_minima[contour];
            move_contours_up(hints, limit, y_offset);
        }
        if is_above_bottom_tilde {
            let contour = find_second_lowest_contour(hints);
            let mut y_offset = stretch_tilde(hints, contour, false);
            y_offset -= align_tilde(hints, contour, false);
            let limit = hints.contour_y_maxima[contour];
            move_contours_down(hints, limit, y_offset);
        }
        if is_top_tilde {
            let contour = find_highest_contour(hints);
            stretch_tilde(hints, contour, true);
            align_tilde(hints, contour, true);
        }
        if is_bottom_tilde {
            let contour = find_lowest_contour(hints);
            stretch_tilde(hints, contour, false);
            align_tilde(hints, contour, false);
        }

        detect_features(hints, metrics, VERT, top_to_bottom);

        let (mut capital_top, mut capital_bottom) = (None, None);
        let (mut small_top, mut small_bottom) = (None, None);
        if have_flags {
            (capital_top, capital_bottom) = base_glyph_blues(metrics, true);
            (small_top, small_bottom) = base_glyph_blues(metrics, false);
            if do_height_check {
                accent_height_limit = if let (Some(t), Some(b)) = (small_top, small_bottom) {
                    2 * (t.shoot.cur - b.shoot.cur) / 3
                } else if let (Some(t), Some(b)) = (capital_top, capital_bottom) {
                    (t.shoot.cur - b.shoot.cur) / 2
                } else {
                    mul_fix(metrics.units_per_em * 4 / 10, metrics.scaler.y_scale)
                };
            }
        }
        let ignore = |hints: &mut GlyphHints, top: &Blue, bottom: &Blue, at_top: bool| {
            let base_glyph_height = top.shoot.cur - bottom.shoot.cur;
            if at_top {
                let limit = top.shoot.cur + base_glyph_height / 7 + 16;
                prevent_blue_alignment(hints, limit, true);
            } else {
                let limit = bottom.shoot.cur - base_glyph_height / 7 - 16;
                prevent_blue_alignment(hints, limit, false);
            }
        };
        if let (Some(t), Some(b)) = (capital_top, capital_bottom) {
            if ignore_capital_top {
                ignore(hints, &t, &b, true);
            }
            if ignore_capital_bottom {
                ignore(hints, &t, &b, false);
            }
        }
        if let (Some(t), Some(b)) = (small_top, small_bottom) {
            if ignore_small_top {
                ignore(hints, &t, &b, true);
            }
            if ignore_small_bottom {
                ignore(hints, &t, &b, false);
            }
        }
        if !nonbase {
            compute_blue_edges(hints, metrics);
        }
    }
    for dim in [HORZ, VERT] {
        let enabled = if dim == HORZ {
            hints.scaler_flags & SCALER_FLAG_NO_HORIZONTAL == 0
        } else {
            hints.scaler_flags & super::hints::SCALER_FLAG_NO_VERTICAL == 0
        };
        if enabled {
            hint_edges(hints, metrics, dim, top_to_bottom);
            hints.align_edge_points(dim);
            hints.align_strong_points(dim);
            hints.align_weak_points(dim);
            apply_vertical_separation_adjustments(hints, dim, adj_type, accent_height_limit);
        }
    }
}

/// The standard widths for stem darkening (`af_latin_get_standard_widths`).
#[allow(dead_code)]
pub(super) fn standard_widths(metrics: &LatinMetrics) -> (i64, i64) {
    (
        metrics.axis[VERT].standard_width,
        metrics.axis[HORZ].standard_width,
    )
}
