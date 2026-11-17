//! Glyph hints shared by the writing systems of the automatic hinter.
//!
//! Ported from FreeType 2.15.0's `src/autofit/afhints.c` and
//! `src/autofit/afhints.h`. Copyright (C) 2003-2026 by David Turner,
//! Robert Wilhelm, and Werner Lemberg. Used under the FreeType License
//! (FTL); see THIRD_PARTY_NOTICES.md.
//!
//! FreeType links points, segments and edges with pointers; here they are
//! indices into the arrays of [`GlyphHints`]. Every write that FreeType
//! makes to a field, including the reuse of the `u` and `v` fields of the
//! points for unrelated purposes, is kept in the same order, since later
//! steps read whatever the earlier ones left.

// Point, segment and edge indices come from the loader's validated
// contours; the arithmetic is FreeType's on `FT_Pos` (64-bit `long`), whose
// values stay far from the limits for 16-bit font units.
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

use super::calc::{corner_is_flat, div_fix, mul_fix};

pub(super) const DIR_NONE: i8 = 4;
pub(super) const DIR_RIGHT: i8 = 1;
pub(super) const DIR_LEFT: i8 = -1;
pub(super) const DIR_UP: i8 = 2;
pub(super) const DIR_DOWN: i8 = -2;

pub(super) const FLAG_CONIC: u16 = 1 << 0;
pub(super) const FLAG_CUBIC: u16 = 1 << 1;
pub(super) const FLAG_CONTROL: u16 = FLAG_CONIC | FLAG_CUBIC;
pub(super) const FLAG_TOUCH_X: u16 = 1 << 2;
pub(super) const FLAG_TOUCH_Y: u16 = 1 << 3;
pub(super) const FLAG_WEAK_INTERPOLATION: u16 = 1 << 4;
pub(super) const FLAG_NEAR: u16 = 1 << 5;
pub(super) const FLAG_IGNORE: u16 = 1 << 6;

pub(super) const EDGE_NORMAL: u8 = 0;
pub(super) const EDGE_ROUND: u8 = 1 << 0;
pub(super) const EDGE_SERIF: u8 = 1 << 1;
pub(super) const EDGE_DONE: u8 = 1 << 2;
pub(super) const EDGE_NEUTRAL: u8 = 1 << 3;
pub(super) const EDGE_NO_BLUE: u8 = 1 << 4;

pub(super) const SCALER_FLAG_NO_HORIZONTAL: u32 = 1;
pub(super) const SCALER_FLAG_NO_VERTICAL: u32 = 2;
pub(super) const SCALER_FLAG_NO_ADVANCE: u32 = 4;

/// `AF_DIMENSION_HORZ`: x coordinates, vertical segments and edges.
pub(super) const HORZ: usize = 0;
/// `AF_DIMENSION_VERT`: y coordinates, horizontal segments and edges.
pub(super) const VERT: usize = 1;

/// `AF_WidthRec`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Width {
    pub org: i64,
    pub cur: i64,
    pub fit: i64,
}

/// `AF_PointRec`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct HintPoint {
    pub flags: u16,
    pub in_dir: i8,
    pub out_dir: i8,
    pub ox: i64,
    pub oy: i64,
    /// `FT_Short` in FreeType.
    pub fx: i64,
    /// `FT_Short` in FreeType.
    pub fy: i64,
    pub x: i64,
    pub y: i64,
    pub u: i64,
    pub v: i64,
    pub next: usize,
    pub prev: usize,
}

/// `AF_SegmentRec`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Segment {
    pub flags: u8,
    pub dir: i8,
    pub pos: i16,
    pub delta: i16,
    pub min_coord: i16,
    pub max_coord: i16,
    pub height: i16,
    pub edge: Option<usize>,
    pub edge_next: usize,
    pub link: Option<usize>,
    pub serif: Option<usize>,
    pub score: i64,
    pub len: i64,
    pub first: usize,
    pub last: usize,
}

impl Segment {
    /// `seg0` of `af_latin_hints_compute_segments`.
    pub(super) fn new(dir: i8, point: usize) -> Segment {
        Segment {
            flags: EDGE_NORMAL,
            dir,
            pos: 0,
            delta: 0,
            min_coord: 0,
            max_coord: 0,
            height: 0,
            edge: None,
            edge_next: 0,
            link: None,
            serif: None,
            score: 32000,
            len: 0,
            first: point,
            last: point,
        }
    }
}

/// `AF_EdgeRec`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Edge {
    pub fpos: i16,
    pub opos: i64,
    pub pos: i64,
    pub flags: u8,
    pub dir: i8,
    pub scale: i64,
    /// The fitted position of the blue zone width this edge is aligned to
    /// (`blue_edge->fit`).
    pub blue_edge: Option<i64>,
    pub link: Option<usize>,
    pub serif: Option<usize>,
    pub first: usize,
    pub last: usize,
}

/// `AF_AxisHintsRec`.
#[derive(Clone, Debug, Default)]
pub(super) struct Axis {
    pub segments: Vec<Segment>,
    pub edges: Vec<Edge>,
    pub major_dir: i8,
}

/// `AF_GlyphHintsRec`.
#[derive(Clone, Debug, Default)]
pub(super) struct GlyphHints {
    pub x_scale: i64,
    pub x_delta: i64,
    pub y_scale: i64,
    pub y_delta: i64,
    pub points: Vec<HintPoint>,
    /// The first point of each contour.
    pub contours: Vec<usize>,
    pub contour_y_minima: Vec<i64>,
    pub contour_y_maxima: Vec<i64>,
    pub axis: [Axis; 2],
    pub scaler_flags: u32,
    pub other_flags: u32,
    /// `hints->metrics->scaler.face->units_per_EM`.
    pub units_per_em: i64,
}

/// An outline in the form `af_glyph_hints_reload` reads: FreeType
/// coordinates, tags, and inclusive contour ends.
pub(super) struct OutlineRef<'a> {
    pub x: &'a [i64],
    pub y: &'a [i64],
    pub tags: &'a [u8],
    pub ends: &'a [usize],
}

/// `af_sort_pos`.
pub(super) fn sort_pos(table: &mut [i64]) {
    for i in 1..table.len() {
        let mut j = i;
        while j > 0 {
            if table[j] >= table[j - 1] {
                break;
            }
            table.swap(j, j - 1);
            j -= 1;
        }
    }
}

/// `af_sort_and_quantize_widths`; returns the new count.
pub(super) fn sort_and_quantize_widths(table: &mut [Width], count: usize, threshold: i64) -> usize {
    // FreeType returns 1 for both 0 and 1 widths.
    if count <= 1 {
        return 1;
    }
    let table = &mut table[..count];
    for i in 1..count {
        let mut j = i;
        while j > 0 {
            if table[j].org >= table[j - 1].org {
                break;
            }
            table.swap(j, j - 1);
            j -= 1;
        }
    }
    let mut cur_idx = 0;
    let mut cur_val = table[cur_idx].org;
    let mut i = 1;
    while i < count {
        if table[i].org - cur_val > threshold || i == count - 1 {
            let mut sum = 0;
            if table[i].org - cur_val <= threshold && i == count - 1 {
                i += 1;
            }
            let mut j = cur_idx;
            while j < i {
                sum += table[j].org;
                table[j].org = 0;
                j += 1;
            }
            table[cur_idx].org = sum / j as i64;
            if i < count - 1 {
                cur_idx = i + 1;
                cur_val = table[cur_idx].org;
            }
        }
        i += 1;
    }
    let mut cur_idx = 1;
    for i in 1..count {
        if table[i].org != 0 {
            table[cur_idx] = table[i];
            cur_idx += 1;
        }
    }
    cur_idx
}

/// `af_direction_compute`.
pub(super) fn direction_compute(dx: i64, dy: i64) -> i8 {
    let (dir, ll, ss) = if dy >= dx {
        if dy >= -dx {
            (DIR_UP, dy, dx)
        } else {
            (DIR_LEFT, -dx, dy)
        }
    } else if dy >= -dx {
        (DIR_RIGHT, dx, dy)
    } else {
        (DIR_DOWN, -dy, dx)
    };
    if ll <= 14 * ss.abs() { DIR_NONE } else { dir }
}

impl Axis {
    /// `af_axis_hints_new_edge` with the edge zeroed: inserts a new edge
    /// sorted by `fpos` and returns its index.
    pub(super) fn new_edge(&mut self, fpos: i64, dir: i8, top_to_bottom: bool) -> usize {
        let mut at = self.edges.len();
        while at > 0 {
            let prev = i64::from(self.edges[at - 1].fpos);
            if if top_to_bottom {
                prev > fpos
            } else {
                prev < fpos
            } {
                break;
            }
            if prev == fpos && dir == self.major_dir {
                break;
            }
            at -= 1;
        }
        self.edges.insert(at, Edge::default());
        at
    }
}

impl GlyphHints {
    /// `af_glyph_hints_reload`.
    pub(super) fn reload(&mut self, outline: &OutlineRef<'_>, postscript: bool) {
        let n = outline.x.len();
        for axis in &mut self.axis {
            axis.segments.clear();
            axis.edges.clear();
        }
        self.points.clear();
        self.points.resize(n, HintPoint::default());
        self.contours.clear();
        self.contour_y_minima.clear();
        self.contour_y_maxima.clear();
        let nc = outline.ends.len();
        self.contour_y_minima.resize(nc, 0);
        self.contour_y_maxima.resize(nc, 0);

        self.axis[HORZ].major_dir = DIR_UP;
        self.axis[VERT].major_dir = DIR_LEFT;
        if postscript {
            self.axis[HORZ].major_dir = DIR_DOWN;
            self.axis[VERT].major_dir = DIR_RIGHT;
        }
        if n == 0 {
            return;
        }
        let near_limit = 20 * self.units_per_em / 2048;
        let (x_scale, y_scale, x_delta, y_delta) =
            (self.x_scale, self.y_scale, self.x_delta, self.y_delta);
        let mut endpoint = outline.ends[0];
        let mut end = endpoint;
        let mut prev = end;
        let mut contour_index = 0;
        for i in 0..n {
            let vx = outline.x[i];
            let vy = outline.y[i];
            {
                let point = &mut self.points[i];
                point.in_dir = DIR_NONE;
                point.out_dir = DIR_NONE;
                point.fx = i64::from(vx as i16);
                point.fy = i64::from(vy as i16);
                point.ox = mul_fix(vx, x_scale) + x_delta;
                point.x = point.ox;
                point.oy = mul_fix(vy, y_scale) + y_delta;
                point.y = point.oy;
            }
            self.points[end].fx = i64::from(outline.x[endpoint] as i16);
            self.points[end].fy = i64::from(outline.y[endpoint] as i16);
            self.points[i].flags = match outline.tags[i] & 3 {
                0 => FLAG_CONIC,
                2 => FLAG_CUBIC,
                _ => 0,
            };
            let out_x = self.points[i].fx - self.points[prev].fx;
            let out_y = self.points[i].fy - self.points[prev].fy;
            if out_x.abs() + out_y.abs() < near_limit {
                self.points[prev].flags |= FLAG_NEAR;
            }
            self.points[i].prev = prev;
            self.points[prev].next = i;
            prev = i;
            if i == end {
                contour_index += 1;
                if contour_index < nc {
                    endpoint = outline.ends[contour_index];
                    end = endpoint;
                    prev = end;
                }
            }
        }

        let mut idx = 0;
        for &e in outline.ends {
            self.contours.push(idx);
            idx = e + 1;
        }

        let near_limit2 = 2 * near_limit - 1;
        for c in 0..nc {
            let first = self.contours[c];
            let mut point = first;
            let mut prev = self.points[first].prev;
            while prev != first {
                let out_x = self.points[point].fx - self.points[prev].fx;
                let out_y = self.points[point].fy - self.points[prev].fy;
                if out_x.abs() + out_y.abs() >= near_limit2 {
                    break;
                }
                point = prev;
                prev = self.points[prev].prev;
            }
            let first = point;
            let mut curr = first;
            self.points[curr].u = first as i64 - curr as i64;
            self.points[first].v = -self.points[curr].u;
            let mut out_x = 0;
            let mut out_y = 0;
            let mut next = first;
            loop {
                let point = next;
                next = self.points[point].next;
                out_x += self.points[next].fx - self.points[point].fx;
                out_y += self.points[next].fy - self.points[point].fy;
                if out_x.abs() + out_y.abs() < near_limit {
                    self.points[next].flags |= FLAG_WEAK_INTERPOLATION;
                    if next == first {
                        break;
                    }
                    continue;
                }
                self.points[curr].u = next as i64 - curr as i64;
                self.points[next].v = -self.points[curr].u;
                let out_dir = direction_compute(out_x, out_y);
                self.points[curr].out_dir = out_dir;
                curr = self.points[curr].next;
                while curr != next {
                    self.points[curr].in_dir = out_dir;
                    self.points[curr].out_dir = out_dir;
                    curr = self.points[curr].next;
                }
                self.points[next].in_dir = out_dir;
                self.points[curr].u = first as i64 - curr as i64;
                self.points[first].v = -self.points[curr].u;
                out_x = 0;
                out_y = 0;
                if next == first {
                    break;
                }
            }
        }

        for p in 0..n {
            if self.points[p].flags & FLAG_WEAK_INTERPOLATION != 0 {
                continue;
            }
            if self.points[p].in_dir == DIR_NONE && self.points[p].out_dir == DIR_NONE {
                let next_u = (p as i64 + self.points[p].u) as usize;
                let prev_v = (p as i64 + self.points[p].v) as usize;
                let in_x = self.points[p].fx - self.points[prev_v].fx;
                let in_y = self.points[p].fy - self.points[prev_v].fy;
                let out_x = self.points[next_u].fx - self.points[p].fx;
                let out_y = self.points[next_u].fy - self.points[p].fy;
                if (in_x ^ out_x) >= 0 && (in_y ^ out_y) >= 0 {
                    self.points[p].flags |= FLAG_WEAK_INTERPOLATION;
                    self.points[prev_v].u = next_u as i64 - prev_v as i64;
                    self.points[next_u].v = -self.points[prev_v].u;
                }
            }
        }

        for p in 0..n {
            let point = self.points[p];
            if point.flags & FLAG_WEAK_INTERPOLATION != 0 {
                continue;
            }
            let weak = if point.flags & FLAG_CONTROL != 0 {
                true
            } else if point.out_dir == point.in_dir {
                if point.out_dir != DIR_NONE {
                    true
                } else {
                    let next_u = (p as i64 + point.u) as usize;
                    let prev_v = (p as i64 + point.v) as usize;
                    let prev = self.points[prev_v];
                    let next = self.points[next_u];
                    if corner_is_flat(
                        point.fx - prev.fx,
                        point.fy - prev.fy,
                        next.fx - point.fx,
                        next.fy - point.fy,
                    ) {
                        self.points[prev_v].u = next_u as i64 - prev_v as i64;
                        self.points[next_u].v = -self.points[prev_v].u;
                        true
                    } else {
                        false
                    }
                }
            } else {
                point.in_dir == -point.out_dir
            };
            if weak {
                self.points[p].flags |= FLAG_WEAK_INTERPOLATION;
            }
        }
    }

    /// `af_glyph_hints_save`: the hinted coordinates and FreeType's tags.
    pub(super) fn save(&self, x: &mut [i64], y: &mut [i64], tags: &mut [u8]) {
        for (i, point) in self.points.iter().enumerate() {
            x[i] = point.x;
            y[i] = point.y;
            tags[i] = if point.flags & FLAG_CONIC != 0 {
                0
            } else if point.flags & FLAG_CUBIC != 0 {
                2
            } else {
                1
            };
        }
    }

    /// `af_glyph_hints_align_edge_points`.
    pub(super) fn align_edge_points(&mut self, dim: usize) {
        for s in 0..self.axis[dim].segments.len() {
            let seg = self.axis[dim].segments[s];
            let Some(e) = seg.edge else {
                continue;
            };
            let pos = self.axis[dim].edges[e].pos;
            let mut point = seg.first;
            loop {
                if dim == HORZ {
                    self.points[point].x = pos;
                    self.points[point].flags |= FLAG_TOUCH_X;
                } else {
                    self.points[point].y = pos;
                    self.points[point].flags |= FLAG_TOUCH_Y;
                }
                if point == seg.last {
                    break;
                }
                point = self.points[point].next;
            }
        }
    }

    /// `af_glyph_hints_align_strong_points`.
    pub(super) fn align_strong_points(&mut self, dim: usize) {
        let touch_flag = if dim == HORZ {
            FLAG_TOUCH_X
        } else {
            FLAG_TOUCH_Y
        };
        let num_edges = self.axis[dim].edges.len();
        if num_edges == 0 {
            return;
        }
        for p in 0..self.points.len() {
            let point = self.points[p];
            if point.flags & touch_flag != 0 || point.flags & FLAG_WEAK_INTERPOLATION != 0 {
                continue;
            }
            let (mut u, ou) = if dim == VERT {
                (point.fy, point.oy)
            } else {
                (point.fx, point.ox)
            };
            let fu = u;
            let edges = &mut self.axis[dim].edges;
            'store: {
                let first = edges[0];
                if i64::from(first.fpos) - u >= 0 {
                    u = first.pos - (first.opos - ou);
                    break 'store;
                }
                let last = edges[num_edges - 1];
                if u - i64::from(last.fpos) >= 0 {
                    u = last.pos + (ou - last.opos);
                    break 'store;
                }
                let mut min = 0;
                let mut max = num_edges;
                if max <= 8 {
                    let mut nn = 0;
                    while nn < max {
                        if i64::from(edges[nn].fpos) >= u {
                            break;
                        }
                        nn += 1;
                    }
                    if i64::from(edges[nn].fpos) == u {
                        u = edges[nn].pos;
                        break 'store;
                    }
                    min = nn;
                } else {
                    while min < max {
                        let mid = (max + min) >> 1;
                        let fpos = i64::from(edges[mid].fpos);
                        if u < fpos {
                            max = mid;
                        } else if u > fpos {
                            min = mid + 1;
                        } else {
                            u = edges[mid].pos;
                            break 'store;
                        }
                    }
                }
                let before = min - 1;
                let after = min;
                if edges[before].scale == 0 {
                    edges[before].scale = div_fix(
                        edges[after].pos - edges[before].pos,
                        i64::from(edges[after].fpos) - i64::from(edges[before].fpos),
                    );
                }
                u = edges[before].pos
                    + mul_fix(fu - i64::from(edges[before].fpos), edges[before].scale);
            }
            if dim == HORZ {
                self.points[p].x = u;
            } else {
                self.points[p].y = u;
            }
            self.points[p].flags |= touch_flag;
        }
    }

    /// `af_iup_shift`.
    fn iup_shift(&mut self, p1: usize, p2: usize, r: usize) {
        let delta = self.points[r].u - self.points[r].v;
        if delta == 0 {
            return;
        }
        for p in p1..r {
            self.points[p].u = self.points[p].v + delta;
        }
        for p in r + 1..=p2 {
            self.points[p].u = self.points[p].v + delta;
        }
    }

    /// `af_iup_interp`.
    fn iup_interp(&mut self, p1: usize, p2: usize, ref1: usize, ref2: usize) {
        if p1 > p2 {
            return;
        }
        let (ref1, ref2) = if self.points[ref1].v > self.points[ref2].v {
            (ref2, ref1)
        } else {
            (ref1, ref2)
        };
        let v1 = self.points[ref1].v;
        let v2 = self.points[ref2].v;
        let u1 = self.points[ref1].u;
        let u2 = self.points[ref2].u;
        let d1 = u1 - v1;
        let d2 = u2 - v2;
        if u1 == u2 || v1 == v2 {
            for p in p1..=p2 {
                let mut u = self.points[p].v;
                if u <= v1 {
                    u += d1;
                } else if u >= v2 {
                    u += d2;
                } else {
                    u = u1;
                }
                self.points[p].u = u;
            }
        } else {
            let scale = div_fix(u2 - u1, v2 - v1);
            for p in p1..=p2 {
                let mut u = self.points[p].v;
                if u <= v1 {
                    u += d1;
                } else if u >= v2 {
                    u += d2;
                } else {
                    u = u1 + mul_fix(u - v1, scale);
                }
                self.points[p].u = u;
            }
        }
    }

    /// `af_glyph_hints_align_weak_points`.
    pub(super) fn align_weak_points(&mut self, dim: usize) {
        let touch_flag = if dim == HORZ {
            for point in &mut self.points {
                point.u = point.x;
                point.v = point.ox;
            }
            FLAG_TOUCH_X
        } else {
            for point in &mut self.points {
                point.u = point.y;
                point.v = point.oy;
            }
            FLAG_TOUCH_Y
        };
        for c in 0..self.contours.len() {
            let mut point = self.contours[c];
            let end_point = self.points[point].prev;
            let first_point = point;
            'contour: {
                loop {
                    if point > end_point {
                        break 'contour;
                    }
                    if self.points[point].flags & touch_flag != 0 {
                        break;
                    }
                    point += 1;
                }
                let first_touched = point;
                let last_touched;
                'scan: loop {
                    while point < end_point && self.points[point + 1].flags & touch_flag != 0 {
                        point += 1;
                    }
                    let lt = point;
                    point += 1;
                    loop {
                        if point > end_point {
                            last_touched = lt;
                            break 'scan;
                        }
                        if self.points[point].flags & touch_flag != 0 {
                            break;
                        }
                        point += 1;
                    }
                    self.iup_interp(lt + 1, point - 1, lt, point);
                }
                if last_touched == first_touched {
                    self.iup_shift(first_point, end_point, first_touched);
                } else {
                    if last_touched < end_point {
                        self.iup_interp(last_touched + 1, end_point, last_touched, first_touched);
                    }
                    if first_touched > 0 {
                        self.iup_interp(
                            first_point,
                            first_touched - 1,
                            last_touched,
                            first_touched,
                        );
                    }
                }
            }
        }
        if dim == HORZ {
            for point in &mut self.points {
                point.x = point.u;
            }
        } else {
            for point in &mut self.points {
                point.y = point.u;
            }
        }
    }
}
