//! The CJK writing system of the automatic hinter, which the Indic
//! writing system also uses.
//!
//! Ported from FreeType 2.15.0's `src/autofit/afcjk.c`,
//! `src/autofit/afcjk.h` and `src/autofit/afindic.c`. Copyright (C)
//! 2006-2026 by David Turner, Robert Wilhelm, and Werner Lemberg. Used
//! under the FreeType License (FTL); see THIRD_PARTY_NOTICES.md.

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

use super::calc::{div_fix, mul_div, mul_fix, pix_floor, pix_round};
use super::hints::{
    EDGE_DONE, EDGE_ROUND, FLAG_TOUCH_X, FLAG_TOUCH_Y, GlyphHints, HORZ, OutlineRef,
    SCALER_FLAG_NO_ADVANCE, SCALER_FLAG_NO_HORIZONTAL, SCALER_FLAG_NO_VERTICAL, VERT, Width,
    sort_pos,
};
use super::latin::{
    HINTS_HORZ_SNAP, HINTS_MONO, HINTS_STEM_ADJUST, HINTS_VERT_SNAP, blue_string, check_digits,
    compute_segments as latin_compute_segments, init_widths, link_edges, snap_width,
};
use super::tables::{BLUE_STRING_MAX, BLUE_STRINGSETS, STYLES};
use super::{Face, RenderMode, Scaler, next_cluster};

const BLUE_PROPERTY_CJK_TOP: u16 = 1 << 0;
const BLUE_PROPERTY_CJK_HORIZ: u16 = 1 << 1;

const BLUE_ACTIVE: u32 = 1 << 0;
const BLUE_TOP: u32 = 1 << 1;

/// `AF_CJKBlueRec`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Blue {
    pub reference: Width,
    pub shoot: Width,
    pub flags: u32,
}

/// `AF_CJKAxisRec`.
#[derive(Clone, Debug, Default)]
pub(super) struct CjkAxis {
    pub scale: i64,
    pub delta: i64,
    /// FreeType never scales these widths: `cur` stays 0.
    pub widths: Vec<Width>,
    pub edge_distance_threshold: i64,
    pub standard_width: i64,
    pub blues: Vec<Blue>,
}

/// `AF_CJKMetricsRec`.
#[derive(Clone, Debug, Default)]
pub(super) struct CjkMetrics {
    pub units_per_em: i64,
    pub axis: [CjkAxis; 2],
    pub digits_have_same_width: bool,
    pub scaler: Scaler,
}

/// `af_cjk_metrics_init_blues`.
fn init_blues(
    face: &Face<'_, '_, '_, '_>,
    metrics: &mut CjkMetrics,
    style: usize,
    blue_stringset: usize,
) {
    let mut set = blue_stringset;
    while BLUE_STRINGSETS[set].string != BLUE_STRING_MAX {
        let bs = BLUE_STRINGSETS[set];
        set += 1;
        let is_horiz = bs.properties & BLUE_PROPERTY_CJK_HORIZ != 0;
        let is_top = bs.properties & BLUE_PROPERTY_CJK_TOP != 0;
        let axis = &mut metrics.axis[if is_horiz { HORZ } else { VERT }];
        let mut fills = Vec::new();
        let mut flats = Vec::new();
        let mut fill = true;
        let mut p = blue_string(bs.string);
        while !p.is_empty() {
            while p.first() == Some(&b' ') {
                p = &p[1..];
            }
            if p.first() == Some(&b'|') {
                fill = false;
                p = &p[1..];
                continue;
            }
            let (rest, num_idx, glyph_index) = next_cluster(face, style, p);
            p = rest;
            if num_idx > 1 {
                continue;
            }
            if glyph_index == 0 {
                continue;
            }
            let Some(outline) = face.load_unscaled(glyph_index) else {
                continue;
            };
            if outline.x.len() <= 2 {
                continue;
            }
            let mut best_point: i64 = -1;
            let mut best_pos: i64 = 0;
            let mut last: i64 = -1;
            for &end in &outline.ends {
                let first = last + 1;
                last = end as i64;
                if last <= first {
                    continue;
                }
                let coords = if is_horiz { &outline.x } else { &outline.y };
                for pp in first..=last {
                    let c = coords[pp as usize];
                    let better = if is_top { c > best_pos } else { c < best_pos };
                    if best_point < 0 || better {
                        best_point = pp;
                        best_pos = c;
                    }
                }
            }
            if fill {
                fills.push(best_pos);
            } else {
                flats.push(best_pos);
            }
        }
        if flats.is_empty() && fills.is_empty() {
            continue;
        }
        sort_pos(&mut fills);
        sort_pos(&mut flats);
        let (mut reference, mut shoot) = if flats.is_empty() {
            let v = fills[fills.len() / 2];
            (v, v)
        } else if fills.is_empty() {
            let v = flats[flats.len() / 2];
            (v, v)
        } else {
            (fills[fills.len() / 2], flats[flats.len() / 2])
        };
        if shoot != reference {
            let under_ref = shoot < reference;
            if is_top ^ under_ref {
                let mean = (shoot + reference) / 2;
                reference = mean;
                shoot = mean;
            }
        }
        let mut blue = Blue::default();
        blue.reference.org = reference;
        blue.shoot.org = shoot;
        if is_top {
            blue.flags |= BLUE_TOP;
        }
        axis.blues.push(blue);
    }
}

/// `af_cjk_metrics_init` (`cjk`) or `af_indic_metrics_init`.
pub(super) fn metrics_init(face: &Face<'_, '_, '_, '_>, style: usize, cjk: bool) -> CjkMetrics {
    let mut metrics = CjkMetrics {
        units_per_em: face.units_per_em,
        ..CjkMetrics::default()
    };
    if face.has_unicode() {
        let script = usize::from(STYLES[style].script);
        let mut axes = [(Vec::new(), 0, 0), (Vec::new(), 0, 0)];
        init_widths(face, style, script, metrics.units_per_em, &mut axes);
        for (axis, (widths, threshold, standard)) in metrics.axis.iter_mut().zip(axes) {
            axis.widths = widths;
            axis.edge_distance_threshold = threshold;
            axis.standard_width = standard;
        }
        if cjk {
            init_blues(
                face,
                &mut metrics,
                style,
                usize::from(STYLES[style].blue_stringset),
            );
        }
        metrics.digits_have_same_width = check_digits(face, style);
    }
    metrics
}

/// `af_cjk_metrics_scale_dim`.
fn scale_dim(metrics: &mut CjkMetrics, scaler: &Scaler, dim: usize) {
    let (scale, delta) = if dim == HORZ {
        (scaler.x_scale, scaler.x_delta)
    } else {
        (scaler.y_scale, scaler.y_delta)
    };
    let axis = &mut metrics.axis[dim];
    axis.scale = scale;
    axis.delta = delta;
    for blue in &mut axis.blues {
        blue.reference.cur = mul_fix(blue.reference.org, scale) + delta;
        blue.reference.fit = blue.reference.cur;
        blue.shoot.cur = mul_fix(blue.shoot.org, scale) + delta;
        blue.shoot.fit = blue.shoot.cur;
        blue.flags &= !BLUE_ACTIVE;
        let dist = mul_fix(blue.reference.org - blue.shoot.org, scale);
        if (-48..=48).contains(&dist) {
            blue.reference.fit = pix_round(blue.reference.cur);
            let delta1 = div_fix(blue.reference.fit, scale) - blue.shoot.org;
            let mut delta2 = mul_fix(delta1.abs(), scale);
            delta2 = if delta2 < 32 { 0 } else { pix_round(delta2) };
            if delta1 < 0 {
                delta2 = -delta2;
            }
            blue.shoot.fit = blue.reference.fit - delta2;
            blue.flags |= BLUE_ACTIVE;
        }
    }
}

/// `af_cjk_metrics_scale`.
pub(super) fn metrics_scale(metrics: &mut CjkMetrics, scaler: &Scaler) {
    metrics.scaler = *scaler;
    scale_dim(metrics, scaler, HORZ);
    scale_dim(metrics, scaler, VERT);
}

/// `af_cjk_hints_compute_segments`. Its pass that marks segments without
/// successive on-curve points as round walks the segments counted before
/// `af_latin_hints_compute_segments` runs, which `af_glyph_hints_reload`
/// has set to none, so it changes nothing.
fn compute_segments(hints: &mut GlyphHints, dim: usize, units_per_em: i64) {
    latin_compute_segments(hints, dim, units_per_em);
}

/// `af_cjk_hints_link_segments`.
fn link_segments(hints: &mut GlyphHints, dim: usize, units_per_em: i64) {
    let major_dir = hints.axis[dim].major_dir;
    let len_threshold = super::latin::constant(units_per_em, 8);
    let scale = if dim == HORZ {
        hints.x_scale
    } else {
        hints.y_scale
    };
    let dist_threshold = div_fix(64 * 3, scale);
    let segments = &mut hints.axis[dim].segments;
    let n = segments.len();
    for s1 in 0..n {
        if segments[s1].dir != major_dir {
            continue;
        }
        for s2 in 0..n {
            if s2 == s1 || segments[s1].dir + segments[s2].dir != 0 {
                continue;
            }
            let dist = i64::from(segments[s2].pos) - i64::from(segments[s1].pos);
            if dist < 0 {
                continue;
            }
            let min = i64::from(segments[s1].min_coord).max(i64::from(segments[s2].min_coord));
            let max = i64::from(segments[s1].max_coord).min(i64::from(segments[s2].max_coord));
            let len = max - min;
            if len >= len_threshold {
                let seg1 = &segments[s1];
                if dist * 8 < seg1.score * 9 && (dist * 8 < seg1.score * 7 || seg1.len < len) {
                    let seg1 = &mut segments[s1];
                    seg1.score = dist;
                    seg1.len = len;
                    seg1.link = Some(s2);
                }
                let seg2 = &segments[s2];
                if dist * 8 < seg2.score * 9 && (dist * 8 < seg2.score * 7 || seg2.len < len) {
                    let seg2 = &mut segments[s2];
                    seg2.score = dist;
                    seg2.len = len;
                    seg2.link = Some(s1);
                }
            }
        }
    }
    for s1 in 0..n {
        let Some(link1) = segments[s1].link else {
            continue;
        };
        if segments[link1].link != Some(s1) || segments[link1].pos <= segments[s1].pos {
            continue;
        }
        if segments[s1].score >= dist_threshold {
            continue;
        }
        for s2 in 0..n {
            if segments[s2].pos > segments[s1].pos || s1 == s2 {
                continue;
            }
            let Some(link2) = segments[s2].link else {
                continue;
            };
            if segments[link2].link != Some(s2) || segments[link2].pos < segments[link1].pos {
                continue;
            }
            if segments[s1].pos == segments[s2].pos && segments[link1].pos == segments[link2].pos {
                continue;
            }
            if segments[s2].score <= segments[s1].score
                || segments[s1].score * 4 <= segments[s2].score
            {
                continue;
            }
            if segments[s1].len >= segments[s2].len * 3 {
                for seg in segments.iter_mut() {
                    let link = seg.link;
                    if link == Some(s2) {
                        seg.link = None;
                        seg.serif = Some(link1);
                    } else if link == Some(link2) {
                        seg.link = None;
                        seg.serif = Some(s1);
                    }
                }
            } else {
                segments[s1].link = None;
                segments[link1].link = None;
                break;
            }
        }
    }
    for s1 in 0..n {
        if let Some(s2) = segments[s1].link
            && segments[s2].link != Some(s1)
        {
            segments[s1].link = None;
            if segments[s2].score < dist_threshold || segments[s1].score < segments[s2].score * 4 {
                segments[s1].serif = segments[s2].link;
            }
        }
    }
}

/// `af_cjk_hints_compute_edges`.
fn compute_edges(hints: &mut GlyphHints, dim: usize, laxis: &CjkAxis) {
    let scale = if dim == HORZ {
        hints.x_scale
    } else {
        hints.y_scale
    };
    let mut edge_distance_threshold = mul_fix(laxis.edge_distance_threshold, scale);
    edge_distance_threshold = if edge_distance_threshold > 64 / 4 {
        div_fix(64 / 4, scale)
    } else {
        laxis.edge_distance_threshold
    };
    let axis = &mut hints.axis[dim];
    axis.edges.clear();
    for s in 0..axis.segments.len() {
        let seg = axis.segments[s];
        let mut found = None;
        let mut best: i64 = 0xFFFF;
        for e in 0..axis.edges.len() {
            let edge = axis.edges[e];
            if edge.dir != seg.dir {
                continue;
            }
            let dist = (i64::from(seg.pos) - i64::from(edge.fpos)).abs();
            if dist < edge_distance_threshold && dist < best {
                if let Some(link) = seg.link {
                    let mut seg1 = edge.first;
                    let mut dist2 = 0;
                    loop {
                        if let Some(link1) = axis.segments[seg1].link {
                            dist2 = (i64::from(axis.segments[link].pos)
                                - i64::from(axis.segments[link1].pos))
                            .abs();
                            if dist2 >= edge_distance_threshold {
                                break;
                            }
                        }
                        seg1 = axis.segments[seg1].edge_next;
                        if seg1 == edge.first {
                            break;
                        }
                    }
                    if dist2 >= edge_distance_threshold {
                        continue;
                    }
                }
                best = dist;
                found = Some(e);
            }
        }
        match found {
            None => {
                let e = axis.new_edge(i64::from(seg.pos), seg.dir, false);
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
    link_edges(axis, false);
}

/// `af_cjk_hints_compute_blue_edges`.
fn compute_blue_edges(hints: &mut GlyphHints, metrics: &CjkMetrics, dim: usize) {
    let cjk = &metrics.axis[dim];
    let scale = cjk.scale;
    let mut best_dist0 = mul_fix(metrics.units_per_em / 40, scale);
    if best_dist0 > 64 / 2 {
        best_dist0 = 64 / 2;
    }
    let axis = &mut hints.axis[dim];
    let major_dir = axis.major_dir;
    for edge in &mut axis.edges {
        let mut best_blue = None;
        let mut best_dist = best_dist0;
        for blue in &cjk.blues {
            if blue.flags & BLUE_ACTIVE == 0 {
                continue;
            }
            let is_top_right_blue = blue.flags & BLUE_TOP != 0;
            let is_major_dir = edge.dir == major_dir;
            if is_top_right_blue ^ is_major_dir {
                let fpos = i64::from(edge.fpos);
                let compare = if (fpos - blue.reference.org).abs() > (fpos - blue.shoot.org).abs() {
                    blue.shoot
                } else {
                    blue.reference
                };
                let dist = mul_fix((fpos - compare.org).abs(), scale);
                if dist < best_dist {
                    best_dist = dist;
                    best_blue = Some(compare.fit);
                }
            }
        }
        if best_blue.is_some() {
            edge.blue_edge = best_blue;
        }
    }
}

/// `af_cjk_hints_init`.
pub(super) fn hints_init(hints: &mut GlyphHints, metrics: &CjkMetrics) {
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
    hints.scaler_flags |= SCALER_FLAG_NO_ADVANCE;
    hints.other_flags = other_flags;
}

/// `af_cjk_compute_stem_width`.
fn compute_stem_width(hints: &GlyphHints, metrics: &CjkMetrics, dim: usize, width: i64) -> i64 {
    let axis = &metrics.axis[dim];
    let vertical = dim == VERT;
    if hints.other_flags & HINTS_STEM_ADJUST == 0 {
        return width;
    }
    let mut dist = width.abs();
    let sign = width < 0;
    'done: {
        if (vertical && hints.other_flags & HINTS_VERT_SNAP == 0)
            || (!vertical && hints.other_flags & HINTS_HORZ_SNAP == 0)
        {
            if let Some(standard) = axis.widths.first()
                && (dist - standard.cur).abs() < 40
            {
                dist = standard.cur;
                if dist < 48 {
                    dist = 48;
                }
                break 'done;
            }
            if dist < 54 {
                dist += (54 - dist) / 2;
            } else if dist < 3 * 64 {
                let delta = dist & 63;
                dist &= -64;
                if delta < 10 {
                    dist += delta;
                } else if delta < 22 {
                    dist += 10;
                } else if delta < 42 {
                    dist += delta;
                } else if delta < 54 {
                    dist += 54;
                } else {
                    dist += delta;
                }
            }
        } else {
            dist = snap_width(&axis.widths, dist);
            if vertical {
                dist = if dist >= 64 { (dist + 16) & !63 } else { 64 };
            } else if hints.other_flags & HINTS_MONO != 0 {
                dist = if dist < 64 { 64 } else { (dist + 32) & !63 };
            } else if dist < 48 {
                dist = (dist + 64) >> 1;
            } else if dist < 128 {
                dist = (dist + 22) & !63;
            } else {
                dist = (dist + 32) & !63;
            }
        }
    }
    if sign { -dist } else { dist }
}

/// `af_cjk_align_linked_edge`.
fn align_linked_edge(
    hints: &mut GlyphHints,
    metrics: &CjkMetrics,
    dim: usize,
    base: usize,
    stem: usize,
) {
    let edges = &hints.axis[dim].edges;
    let dist = edges[stem].opos - edges[base].opos;
    let fitted = compute_stem_width(hints, metrics, dim, dist);
    let base_pos = hints.axis[dim].edges[base].pos;
    hints.axis[dim].edges[stem].pos = base_pos + fitted;
}

const LIGHT_MODE_MAX_HORZ_GAP: i64 = 9;
const LIGHT_MODE_MAX_VERT_GAP: i64 = 15;
const LIGHT_MODE_MAX_DELTA_ABS: i64 = 14;

/// `af_hint_normal_stem`.
fn hint_normal_stem(
    hints: &mut GlyphHints,
    metrics: &CjkMetrics,
    e: usize,
    e2: usize,
    anchor: i64,
    dim: usize,
) -> i64 {
    let stem_adjust = hints.other_flags & HINTS_STEM_ADJUST != 0;
    let (edge, edge2) = (hints.axis[dim].edges[e], hints.axis[dim].edges[e2]);
    let mut threshold = 64;
    if !stem_adjust {
        threshold = if edge.flags & EDGE_ROUND != 0 && edge2.flags & EDGE_ROUND != 0 {
            if dim == VERT {
                64 - LIGHT_MODE_MAX_HORZ_GAP
            } else {
                64 - LIGHT_MODE_MAX_VERT_GAP
            }
        } else if dim == VERT {
            64 - LIGHT_MODE_MAX_HORZ_GAP / 3
        } else {
            64 - LIGHT_MODE_MAX_VERT_GAP / 3
        };
    }
    let org_len = edge2.opos - edge.opos;
    let cur_len = compute_stem_width(hints, metrics, dim, org_len);
    let org_center = (edge.opos + edge2.opos) / 2 + anchor;
    let mut cur_pos1 = org_center - cur_len / 2;
    let cur_pos2 = cur_pos1 + cur_len;
    let mut d_off1 = cur_pos1 - pix_floor(cur_pos1);
    let mut d_off2 = cur_pos2 - pix_floor(cur_pos2);
    let mut u_off1 = 64 - d_off1;
    let mut u_off2 = 64 - d_off2;
    let mut delta = 0;
    'exit: {
        if d_off1 == 0 || d_off2 == 0 {
            break 'exit;
        }
        if cur_len <= threshold {
            if d_off2 < cur_len {
                delta = if u_off1 <= d_off2 { u_off1 } else { -d_off2 };
            }
            break 'exit;
        }
        if threshold < 64
            && (d_off1 >= threshold
                || u_off1 >= threshold
                || d_off2 >= threshold
                || u_off2 >= threshold)
        {
            break 'exit;
        }
        let mut offset = cur_len & 63;
        if offset < 32 {
            if u_off1 <= offset || d_off2 <= offset {
                break 'exit;
            }
        } else {
            offset = 64 - threshold;
        }
        d_off1 = threshold - u_off1;
        u_off1 -= offset;
        u_off2 = threshold - d_off2;
        d_off2 -= offset;
        if d_off1 <= u_off1 {
            u_off1 = -d_off1;
        }
        if d_off2 <= u_off2 {
            u_off2 = -d_off2;
        }
        delta = if u_off1.abs() <= u_off2.abs() {
            u_off1
        } else {
            u_off2
        };
    }
    if !stem_adjust {
        delta = delta.clamp(-LIGHT_MODE_MAX_DELTA_ABS, LIGHT_MODE_MAX_DELTA_ABS);
    }
    cur_pos1 += delta;
    let edges = &mut hints.axis[dim].edges;
    if edge.opos < edge2.opos {
        edges[e].pos = cur_pos1;
        edges[e2].pos = cur_pos1 + cur_len;
    } else {
        edges[e].pos = cur_pos1 + cur_len;
        edges[e2].pos = cur_pos1;
    }
    delta
}

/// `af_cjk_hint_edges`.
fn hint_edges(hints: &mut GlyphHints, metrics: &CjkMetrics, dim: usize) {
    let n_edges = hints.axis[dim].edges.len();
    let mut anchor: Option<usize> = None;
    let mut delta = 0;
    let mut skipped = 0;
    let mut has_last_stem = false;
    let mut last_stem_pos = 0;

    for e in 0..n_edges {
        let edges = &hints.axis[dim].edges;
        if edges[e].flags & EDGE_DONE != 0 {
            continue;
        }
        let mut blue = edges[e].blue_edge;
        let mut edge1 = None;
        let mut edge2 = edges[e].link;
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
        let edges = &mut hints.axis[dim].edges;
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

    for e in 0..n_edges {
        let edge = hints.axis[dim].edges[e];
        if edge.flags & EDGE_DONE != 0 {
            continue;
        }
        let Some(e2) = edge.link else {
            skipped += 1;
            continue;
        };
        let edge2 = hints.axis[dim].edges[e2];
        if has_last_stem && (edge.pos < last_stem_pos + 64 || edge2.pos < last_stem_pos + 64) {
            skipped += 1;
            continue;
        }
        if edge2.blue_edge.is_some() {
            align_linked_edge(hints, metrics, dim, e2, e);
            hints.axis[dim].edges[e].flags |= EDGE_DONE;
            continue;
        }
        if e2 < e {
            align_linked_edge(hints, metrics, dim, e2, e);
            hints.axis[dim].edges[e].flags |= EDGE_DONE;
            has_last_stem = true;
            last_stem_pos = hints.axis[dim].edges[e].pos;
            continue;
        }
        if dim != VERT && anchor.is_none() {
            delta = hint_normal_stem(hints, metrics, e, e2, 0, HORZ);
        } else {
            hint_normal_stem(hints, metrics, e, e2, delta, dim);
        }
        anchor = Some(e);
        let edges = &mut hints.axis[dim].edges;
        edges[e].flags |= EDGE_DONE;
        edges[e2].flags |= EDGE_DONE;
        has_last_stem = true;
        last_stem_pos = edges[e2].pos;
    }

    if dim == HORZ && (n_edges == 6 || n_edges == 12) {
        let (edge1, edge2, edge3) = if n_edges == 6 { (0, 2, 4) } else { (1, 5, 9) };
        let edges = &mut hints.axis[dim].edges;
        let dist1 = edges[edge2].opos - edges[edge1].opos;
        let dist2 = edges[edge3].opos - edges[edge2].opos;
        let span = (dist1 - dist2).abs();
        if edges[edge1].link == Some(edge1 + 1)
            && edges[edge2].link == Some(edge2 + 1)
            && edges[edge3].link == Some(edge3 + 1)
            && span < 8
        {
            let d = edges[edge3].pos - (2 * edges[edge2].pos - edges[edge1].pos);
            edges[edge3].pos -= d;
            if let Some(l) = edges[edge3].link {
                edges[l].pos -= d;
            }
            if n_edges == 12 {
                edges[8].pos -= d;
                edges[11].pos -= d;
            }
            edges[edge3].flags |= EDGE_DONE;
            if let Some(l) = edges[edge3].link {
                edges[l].flags |= EDGE_DONE;
            }
        }
    }

    if skipped == 0 {
        return;
    }
    let edges = &mut hints.axis[dim].edges;
    for e in 0..n_edges {
        if edges[e].flags & EDGE_DONE != 0 {
            continue;
        }
        if let Some(serif) = edges[e].serif {
            let base = edges[serif];
            edges[e].pos = base.pos + (edges[e].opos - base.opos);
            edges[e].flags |= EDGE_DONE;
            skipped -= 1;
        }
    }
    if skipped == 0 {
        return;
    }
    for e in 0..n_edges {
        if edges[e].flags & EDGE_DONE != 0 {
            continue;
        }
        let before = (0..e).rev().find(|&b| edges[b].flags & EDGE_DONE != 0);
        let after = (e + 1..n_edges).find(|&a| edges[a].flags & EDGE_DONE != 0);
        match (before, after) {
            (None, None) => {}
            (None, Some(a)) => {
                let base = edges[a];
                edges[e].pos = base.pos + (edges[e].opos - base.opos);
            }
            (Some(b), None) => {
                let base = edges[b];
                edges[e].pos = base.pos + (edges[e].opos - base.opos);
            }
            (Some(b), Some(a)) => {
                let (before, after) = (edges[b], edges[a]);
                edges[e].pos = if after.fpos == before.fpos {
                    before.pos
                } else {
                    before.pos
                        + mul_div(
                            i64::from(edges[e].fpos) - i64::from(before.fpos),
                            after.pos - before.pos,
                            i64::from(after.fpos) - i64::from(before.fpos),
                        )
                };
            }
        }
    }
}

/// `af_cjk_align_edge_points`.
fn align_edge_points(hints: &mut GlyphHints, dim: usize) {
    let snapping = (dim == HORZ && hints.other_flags & HINTS_HORZ_SNAP != 0)
        || (dim == VERT && hints.other_flags & HINTS_VERT_SNAP != 0);
    for e in 0..hints.axis[dim].edges.len() {
        let edge = hints.axis[dim].edges[e];
        let mut s = edge.first;
        loop {
            let seg = hints.axis[dim].segments[s];
            let mut point = seg.first;
            loop {
                let p = &mut hints.points[point];
                if dim == HORZ {
                    p.x = if snapping {
                        edge.pos
                    } else {
                        p.x + (edge.pos - edge.opos)
                    };
                    p.flags |= FLAG_TOUCH_X;
                } else {
                    p.y = if snapping {
                        edge.pos
                    } else {
                        p.y + (edge.pos - edge.opos)
                    };
                    p.flags |= FLAG_TOUCH_Y;
                }
                if point == seg.last {
                    break;
                }
                point = hints.points[point].next;
            }
            s = seg.edge_next;
            if s == edge.first {
                break;
            }
        }
    }
}

/// `af_cjk_hints_apply`.
pub(super) fn hints_apply(
    hints: &mut GlyphHints,
    outline: &OutlineRef<'_>,
    postscript: bool,
    metrics: &CjkMetrics,
) {
    hints.reload(outline, postscript);
    let upem = metrics.units_per_em;
    if hints.scaler_flags & SCALER_FLAG_NO_HORIZONTAL == 0 {
        compute_segments(hints, HORZ, upem);
        link_segments(hints, HORZ, upem);
        compute_edges(hints, HORZ, &metrics.axis[HORZ]);
        compute_blue_edges(hints, metrics, HORZ);
    }
    if hints.scaler_flags & SCALER_FLAG_NO_VERTICAL == 0 {
        compute_segments(hints, VERT, upem);
        link_segments(hints, VERT, upem);
        compute_edges(hints, VERT, &metrics.axis[VERT]);
        compute_blue_edges(hints, metrics, VERT);
    }
    for dim in [HORZ, VERT] {
        let flag = if dim == HORZ {
            SCALER_FLAG_NO_HORIZONTAL
        } else {
            SCALER_FLAG_NO_VERTICAL
        };
        if hints.scaler_flags & flag == 0 {
            hint_edges(hints, metrics, dim);
            align_edge_points(hints, dim);
            hints.align_strong_points(dim);
            hints.align_weak_points(dim);
        }
    }
}
