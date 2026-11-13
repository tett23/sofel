//! COLR (`src/OT/Color/COLR/COLR.hh`, `colrv1-closure.hh`): the closure
//! of the plan (`_colr_closure`: the layer glyphs of version 0, the
//! glyphs, layers and palette indices the version 1 paints reach) and
//! the subsetting of both versions, without a variation store or a
//! DeltaSetIndexMap (ADR 0022 stage 10).

mod radial;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::colr::Colr;
use sofel_write::{Errors, Serializer, Whence};

use super::Tables;
use crate::error::SubsetError;
use crate::layout::data::{D, bsearch, n16};
use crate::limits;
use crate::plan::Plan;
use crate::set::{INVALID, RangeSet};

/// `VarIdx::NO_VARIATION`.
const NO_VARIATION: u32 = 0xFFFF_FFFF;

/// The color part of the plan (`_glyphset_colred`, `colrv1_layers` and
/// `colr_palettes` of `hb_subset_plan_t`).
#[derive(Clone, Debug, Default)]
pub(crate) struct ColorPlan {
    /// The glyphs after the COLR closure (`_glyphset_colred`).
    pub glyphset_colred: RangeSet,
    /// Old LayerList index to new (`colrv1_layers`).
    pub layers: BTreeMap<u32, u32>,
    /// Old palette entry index to new; 0xFFFF maps to itself
    /// (`colr_palettes`).
    pub palettes: BTreeMap<u32, u32>,
    variation_indices: BTreeMap<u32, u32>,
    variation_store: Vec<u8>,
    index_map: Vec<u8>,
}

/// The COLR table HarfBuzz's sanitizer accepts, as far as the views
/// check it; `None` stands for the empty blob of a rejected table.
fn sanitized(data: &[u8]) -> Option<Colr<'_>> {
    Colr::new(data).ok().filter(|_| !data.is_empty())
}

fn retains_no_v1_paints(colr: &Colr<'_>, glyphset: &RangeSet) -> bool {
    (0..colr.num_base_glyph_paints())
        .filter_map(|i| colr.base_glyph_paint_record(i))
        .all(|(g, _)| !glyphset.contains(u32::from(g)))
}

const TAG: Tag = Tag::new(b"COLR");

fn unsupported_store() -> SubsetError {
    SubsetError::Unsupported {
        tag: TAG,
        reason: "the COLR item variation store (ADR 0022 stage 10)",
    }
}

fn collect_variations(indices: &mut BTreeSet<u32>, base: u32, count: u32) {
    indices.extend(base..base.saturating_add(count));
}

fn plan_store(
    colr: &Colr<'_>,
    glyphset: &RangeSet,
    complete: bool,
    pinned: bool,
    instance: (&Plan, &Font<'_>),
    mut indices: BTreeSet<u32>,
    plan: &mut ColorPlan,
) -> Result<(), SubsetError> {
    if colr.var_store_offset() == 0 && colr.var_index_map_offset() == 0 {
        return Ok(());
    }
    let map = colr
        .var_index_map()
        .map_err(|_| unsupported_store())?
        .filter(|m| !m.is_empty());
    let store = colr.variation_store().map_err(|_| unsupported_store())?;
    if retains_no_v1_paints(colr, glyphset) {
        return Ok(());
    }
    if !complete {
        return Err(unsupported_store());
    }
    if pinned {
        return Ok(());
    }
    for g in glyphset.iter() {
        if let Some(base) = colr.clip_box(g).and_then(|b| b.var_index_base) {
            collect_variations(&mut indices, base, 4);
        }
    }
    if indices.is_empty() {
        return Ok(());
    }
    let rows: BTreeSet<_> = indices
        .iter()
        .map(|&idx| map.map_or(idx, |m| m.map(idx)))
        .filter(|&idx| idx != NO_VARIATION)
        .collect();
    let mut groups: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for idx in rows {
        let outer = u16::try_from(idx >> 16).map_err(|_| unsupported_store())?;
        let inner = idx & 0xffff;
        if !store
            .and_then(|s| s.item_data(outer))
            .is_some_and(|data| inner < u32::from(data.item_count))
        {
            return Err(unsupported_store());
        }
        groups.entry(outer).or_default().push(inner);
    }
    let groups: Vec<_> = groups.into_iter().collect();
    for (major, (outer, rows)) in groups.iter().enumerate() {
        for (minor, inner) in rows.iter().enumerate() {
            let old = (u32::from(*outer) << 16) | inner;
            let new = (u32::try_from(major).map_err(|_| unsupported_store())? << 16)
                | u32::try_from(minor).map_err(|_| unsupported_store())?;
            plan.variation_indices.insert(old, new);
        }
    }
    if let Some(store) = store {
        if instance.0.var.instancing() {
            let mut remap = BTreeMap::new();
            plan.variation_store = super::hvar::instance::subset(
                instance.0,
                instance.1,
                store,
                &groups,
                TAG,
                map.is_some(),
                &mut remap,
            )?;
            if map.is_some() {
                for index in plan.variation_indices.values_mut() {
                    *index = remap.get(index).copied().unwrap_or(NO_VARIATION);
                }
            }
        } else {
            plan.variation_store = super::hvar::subset_store(store, &groups, TAG)?;
        }
    }
    if let Some(map) = map {
        let rows = core::mem::take(&mut plan.variation_indices);
        let mut entries = Vec::new();
        for idx in indices {
            let source = map.map(idx);
            let target = if source == NO_VARIATION {
                source
            } else {
                *rows.get(&source).ok_or_else(unsupported_store)?
            };
            let new = u32::try_from(entries.len()).map_err(|_| unsupported_store())?;
            plan.variation_indices.insert(idx, new);
            entries.push(target);
        }
        // Indices beyond the stored map repeat its last entry.
        if let Some(&last) = entries.last() {
            let suffix = entries.iter().rev().take_while(|&&v| v == last).count();
            entries.truncate(entries.len().saturating_sub(suffix.saturating_sub(1)));
        }
        plan.index_map = super::hvar::encode_index_map(&entries, TAG)?;
    }
    Ok(())
}

/// The layer index range `[first, first + count)` of a base glyph
/// record, cut at the end of the layer records (`sub_array`).
fn layer_range(first: u16, count: u16, num_layers: usize) -> (usize, usize) {
    let first = usize::from(first);
    let n = num_layers.saturating_sub(first).min(usize::from(count));
    (first, n)
}

/// `_colr_closure`: adds the glyphs COLR reaches from `glyphs` and
/// returns the layer and palette index maps.
pub(crate) fn closure(
    data: &[u8],
    glyphs: &mut RangeSet,
    pinned: bool,
    subset_table: bool,
    instance: (&Plan, &Font<'_>),
    num_glyphs: u32,
) -> Result<ColorPlan, SubsetError> {
    let Some(colr) = sanitized(data) else {
        return Ok(ColorPlan::default());
    };
    let t = D(data);
    // closure_glyphs (version 0)
    let num_layers = colr.num_layers();
    if colr.num_base_glyphs() != 0 && num_layers != 0 {
        let mut layer_indices = RangeSet::new();
        for g in glyphs.iter() {
            let Some(r) = colr.find_base_glyph(g) else {
                continue;
            };
            let (first, n) = layer_range(r.first_layer, r.num_layers, num_layers);
            if n != 0 {
                layer_indices.add_range(
                    crate::layout::data::n32(first),
                    crate::layout::data::n32(first.saturating_add(n).saturating_sub(1)),
                );
            }
        }
        let related: Vec<u32> = layer_indices
            .iter()
            .filter_map(|i| colr.layer(i as usize))
            .map(|(g, _)| u32::from(g))
            .collect();
        related.into_iter().for_each(|g| glyphs.add(g));
    }
    // closure_forV1
    let mut layer_indices = RangeSet::new();
    let mut palette_indices = RangeSet::new();
    let mut can_subset_store = true;
    let mut indices = BTreeSet::new();
    if colr.version() >= 1 {
        let mut c = Closure {
            t,
            visited: BTreeSet::new(),
            can_subset_store: true,
            indices: &mut indices,
            pinned,
            glyphs: RangeSet::new(),
            layers: &mut layer_indices,
            palettes: &mut palette_indices,
            nesting_left: limits::MAX_NESTING_LEVEL,
        };
        let list = t.follow(t.u32(14));
        let list_at = t.u32(14) as usize;
        for i in 0..list.u32(0) as usize {
            let rec = 4usize.saturating_add(i.saturating_mul(6));
            if !glyphs.contains(u32::from(list.u16(rec))) {
                continue;
            }
            let off = list.u32(rec.saturating_add(2));
            if off != 0 {
                c.dispatch(list_at.saturating_add(off as usize));
            }
        }
        let visited = c.glyphs;
        glyphs.union(&visited);
        // PaintGlyph may add a glyph with its own base paint to the output.
        // Accept the store only when every retained base paint was inspected.
        can_subset_store = (colr.var_store_offset() == 0 && colr.var_index_map_offset() == 0)
            || c.can_subset_store
                && (0..colr.num_base_glyph_paints())
                    .filter_map(|i| colr.base_glyph_paint_record(i))
                    .all(|(g, at)| !glyphs.contains(u32::from(g)) || c.visited.contains(&at));
    }
    // closure_V0palette_indices
    if colr.num_base_glyphs() != 0 && num_layers != 0 {
        let mut idx = RangeSet::new();
        for r in (0..colr.num_base_glyphs()).filter_map(|i| colr.base_glyph(i)) {
            if !glyphs.contains(u32::from(r.glyph)) {
                continue;
            }
            let (first, n) = layer_range(r.first_layer, r.num_layers, num_layers);
            if n != 0 {
                idx.add_range(
                    crate::layout::data::n32(first),
                    crate::layout::data::n32(first.saturating_add(n).saturating_sub(1)),
                );
            }
        }
        for (_, p) in idx.iter().filter_map(|i| colr.layer(i as usize)) {
            palette_indices.add(u32::from(p));
        }
    }
    let layers = layer_indices.iter().zip(0u32..).collect();
    let mut palettes = BTreeMap::new();
    let mut next = 0u32;
    for p in palette_indices.iter() {
        if p == 0xFFFF {
            palettes.insert(p, p);
        } else {
            palettes.insert(p, next);
            next = next.saturating_add(1);
        }
    }
    glyphs.truncate(num_glyphs);
    let mut plan = ColorPlan {
        layers,
        palettes,
        ..ColorPlan::default()
    };
    if subset_table {
        plan_store(
            &colr,
            glyphs,
            can_subset_store,
            pinned,
            instance,
            indices,
            &mut plan,
        )?;
    }
    Ok(plan)
}

/// Collects glyphs, layers, palette entries and variation indices reached by retained paints.
struct Closure<'a, 'c> {
    t: D<'a>,
    visited: BTreeSet<usize>,
    can_subset_store: bool,
    indices: &'c mut BTreeSet<u32>,
    pinned: bool,
    glyphs: RangeSet,
    layers: &'c mut RangeSet,
    palettes: &'c mut RangeSet,
    nesting_left: u32,
}

impl Closure<'_, '_> {
    /// `Paint::dispatch` with the context's `dispatch`: the paint at `at`
    /// (from the start of the table).
    fn dispatch(&mut self, at: usize) {
        let format = self.t.u8(at);
        if !matches!(format, 1..=33) || self.nesting_left == 0 {
            self.can_subset_store = false;
            return;
        }
        if !self.visited.insert(at) {
            return;
        }
        self.nesting_left = self.nesting_left.saturating_sub(1);
        if let Some((size, true)) = paint_size(format) {
            let count = match format {
                3 | 21 | 25 => 1,
                15 | 17 | 29 => 2,
                23 | 27 => 3,
                9 | 19 | 31 => 4,
                _ => 6,
            };
            self.check_var_index(at.saturating_add(size), count);
        } else if format == 13 {
            let offset = u24(self.t, at.saturating_add(4)) as usize;
            self.check_var_index(at.saturating_add(offset).saturating_add(24), 6);
        }
        self.closurev1(format, at);
        self.nesting_left = self.nesting_left.saturating_add(1);
    }

    /// The paint a 24-bit offset at `field` of the paint at `at` points
    /// to.
    fn child(&mut self, at: usize, field: usize) {
        let off = u24(self.t, at.saturating_add(field));
        if off != 0 {
            self.dispatch(at.saturating_add(off as usize));
        }
    }

    fn check_var_index(&mut self, at: usize, count: u32) {
        if !self.pinned {
            collect_variations(self.indices, self.t.u32(at), count);
        }
    }

    fn color_line(&mut self, at: usize, var: bool) {
        let off = u24(self.t, at.saturating_add(1));
        if off == 0 {
            return;
        }
        let line = self.t.at(at.saturating_add(off as usize));
        let size = if var { 10 } else { 6 };
        for i in 0..usize::from(line.u16(1)) {
            let stop = 3usize.saturating_add(i.saturating_mul(size));
            self.palettes
                .add(u32::from(line.u16(stop.saturating_add(2))));
            if var {
                self.check_var_index(
                    at.saturating_add(off as usize)
                        .saturating_add(stop)
                        .saturating_add(6),
                    2,
                );
            }
        }
    }

    fn closurev1(&mut self, format: u8, at: usize) {
        let p = self.t.at(at);
        match format {
            1 => {
                let ll_at = self.t.u32(18) as usize;
                let ll = self.t.follow(self.t.u32(18));
                let len = ll.u32(0);
                let first = p.u32(2).min(len);
                let count = u32::from(p.u8(1)).min(len.saturating_sub(first));
                if count != 0 && first != INVALID {
                    let last = first
                        .checked_add(count.saturating_sub(1))
                        .filter(|&l| l != INVALID);
                    self.layers
                        .add_range(first, last.unwrap_or(INVALID.saturating_sub(1)));
                }
                for i in first..first.saturating_add(count) {
                    let off = ll.u32(4usize.saturating_add((i as usize).saturating_mul(4)));
                    if off != 0 {
                        self.dispatch(ll_at.saturating_add(off as usize));
                    }
                }
            }
            2 | 3 => self.palettes.add(u32::from(p.u16(1))),
            4..=9 => self.color_line(at, format % 2 == 1),
            10 | 33 => {
                let gid = if format == 10 {
                    u32::from(p.u16(4))
                } else {
                    u24(p, 4)
                };
                self.glyphs.add(gid);
                self.child(at, 1);
            }
            11 => {
                let gid = u32::from(p.u16(1));
                if let Some(paint) = base_glyph_paint(self.t, gid) {
                    self.glyphs.add(gid);
                    if let Some(paint) = paint {
                        self.dispatch(paint);
                    }
                }
            }
            32 => {
                self.child(at, 1);
                self.child(at, 5);
            }
            _ => self.child(at, 1),
        }
    }
}

/// A big-endian 24-bit value.
fn u24(d: D<'_>, at: usize) -> u32 {
    u32::from(d.u8(at)) << 16 | u32::from(d.u16(at.saturating_add(1)))
}

/// `get_base_glyph_paintrecord`: `Some` when `glyph` has a
/// BaseGlyphPaintRecord, with the offset of its paint from the start of
/// the table (`None` for a null offset).
fn base_glyph_paint(t: D<'_>, glyph: u32) -> Option<Option<usize>> {
    let list = t.follow(t.u32(14));
    let i = bsearch(list.u32(0) as usize, |i| {
        glyph.cmp(&u32::from(
            list.u16(4usize.saturating_add(i.saturating_mul(6))),
        ))
    })?;
    let off = list.u32(6usize.saturating_add(i.saturating_mul(6)));
    Some((off != 0).then(|| (t.u32(14) as usize).saturating_add(off as usize)))
}

/// `n` bytes at `at` of `d`, zeros past the end.
fn bytes(d: D<'_>, at: usize, n: usize) -> Vec<u8> {
    (0..n).map(|i| d.u8(at.saturating_add(i))).collect()
}

/// The subsetting context of COLR.
struct Sub<'a, 'p> {
    p: &'p Plan,
    t: D<'a>,
    s: &'p mut Serializer,
    delta: Option<&'p dyn Fn(u32) -> f32>,
    /// Paints nested in the one being written.
    depth: u32,
}

/// The static size of the value of a paint format, and whether the
/// format wraps it in `Variable` (`Paint`'s union).
fn paint_size(format: u8) -> Option<(usize, bool)> {
    let var = matches!(format, 3 | 5 | 7 | 9 | 15..=31) && format % 2 == 1;
    let size = match format {
        1 => 6,
        2 | 3 => 5,
        4..=7 => 16,
        8 | 9 => 12,
        10 | 20 | 21 | 24 | 25 => 6,
        11 => 3,
        12 | 13 | 33 => 7,
        14..=17 | 28 | 29 | 32 => 8,
        18 | 19 | 30 | 31 => 12,
        22 | 23 | 26 | 27 => 10,
        _ => return None,
    };
    Some((size, var))
}

impl Sub<'_, '_> {
    /// Copies `n` source bytes at `at` into the current object (`embed`).
    fn embed(&mut self, at: usize, n: usize) -> usize {
        let b = bytes(self.t, at, n);
        self.s.extend(&b)
    }

    /// `OffsetTo::serialize_subset` of the offset of `width` bytes at
    /// `field` of the current object to the source object at `target`
    /// (`None` for a null offset).
    fn serialize_subset(
        &mut self,
        field: usize,
        width: u8,
        target: Option<usize>,
        f: impl FnOnce(&mut Self, usize) -> bool,
    ) -> bool {
        self.s.write_at(field, &alloc::vec![0; usize::from(width)]);
        let Some(target) = target else {
            return false;
        };
        self.s.push();
        let ret = f(self, target);
        if ret {
            let idx = self.s.pop_pack(true);
            self.s.add_link(field, width, false, idx, Whence::Head, 0);
        } else {
            self.s.pop_discard();
        }
        ret
    }

    /// A 24-bit offset field `field` of the source paint at `at` (copied
    /// at `out` of the current object) to a paint.
    fn paint_offset(&mut self, out: usize, at: usize, field: usize) -> bool {
        let off = u24(self.t, at.saturating_add(field));
        let target = (off != 0).then(|| at.saturating_add(off as usize));
        self.serialize_subset(out.saturating_add(field), 3, target, |c, t| c.paint(t))
    }

    fn palette(&mut self, out: usize, src: u16) -> bool {
        let v = self
            .p
            .color
            .palettes
            .get(&u32::from(src))
            .copied()
            .unwrap_or(INVALID);
        self.s
            .check_assign_u16(out, u64::from(v), Errors::INT_OVERFLOW)
    }

    /// `Variable::subset`'s variation index:
    /// retains the sentinel or rewrites a collected index.
    fn var_index(&mut self, at: usize) -> bool {
        if self.p.var.instancing() && self.p.var.all_axes_pinned() {
            return true;
        }
        let old = self.t.u32(at);
        let new = if old == NO_VARIATION {
            old
        } else if let Some(&new) = self.p.color.variation_indices.get(&old) {
            new
        } else {
            return false;
        };
        self.s.u32(new);
        true
    }

    fn fold_fixed(&mut self, src: usize, out: usize, index: u32, alpha: bool) {
        let Some(delta) = self.delta else { return };
        if index == NO_VARIATION {
            return;
        }
        let base = i16::from_be_bytes(self.t.u16(src).to_be_bytes());
        let sum = f32::from(base) + delta(index);
        let (min, max) = if alpha && self.p.var.all_axes_pinned() {
            (0, 16384)
        } else {
            (i16::MIN, i16::MAX)
        };
        let value = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(sum))
            .clamp(i32::from(min), i32::from(max));
        self.s
            .write_at(out, &i16::try_from(value).unwrap_or(max).to_be_bytes());
    }

    fn fold_coord(&mut self, src: usize, out: usize, adjustment: f64) {
        let base = i16::from_be_bytes(self.t.u16(src).to_be_bytes());
        let value = crate::instance::f64_to_i32(f64::from(base) + adjustment)
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        self.s
            .write_at(out, &i16::try_from(value).unwrap_or_default().to_be_bytes());
    }

    fn fold_paint_fields(&mut self, format: u8, at: usize, out: usize, size: usize) {
        let Some(delta) = self.delta else { return };
        let base = self.t.u32(at.saturating_add(size));
        if base == NO_VARIATION {
            return;
        }
        if format == 3 {
            self.fold_fixed(at.saturating_add(3), out.saturating_add(3), base, true);
            return;
        }
        for (axis, offset) in (4..size).step_by(2).enumerate() {
            let index = base.saturating_add(u32::try_from(axis).unwrap_or(u32::MAX));
            let src = at.saturating_add(offset);
            let dest = out.saturating_add(offset);
            let fixed = match format {
                9 => axis >= 2,
                17 | 19 | 29 | 31 => axis < 2,
                21 | 23 | 25 | 27 => axis == 0,
                _ => false,
            };
            if fixed {
                self.fold_fixed(src, dest, index, false);
            } else if format == 7 && matches!(axis, 2 | 5) {
                let value = crate::instance::f64_to_i32(
                    f64::from(self.t.u16(src)) + sofel_geom::float::hb_roundf32(delta(index)),
                );
                self.s.write_at(
                    dest,
                    &u16::try_from(value.clamp(0, 65535))
                        .unwrap_or_default()
                        .to_be_bytes(),
                );
            } else {
                self.fold_coord(src, dest, sofel_geom::float::hb_roundf32(delta(index)));
            }
        }
    }

    /// `ColorLine::subset`.
    fn color_line(&mut self, at: usize, var: bool) -> bool {
        let line = self.t.at(at);
        self.s.u8(line.u8(0));
        let count = line.u16(1);
        self.s.u16(count);
        let size = if var { 10 } else { 6 };
        for i in 0..usize::from(count) {
            let stop = at.saturating_add(3).saturating_add(i.saturating_mul(size));
            let out = self.embed(stop, 6);
            if !self.palette(out.saturating_add(2), self.t.u16(stop.saturating_add(2))) {
                return false;
            }
            if var {
                let base = self.t.u32(stop.saturating_add(6));
                self.fold_fixed(stop, out, base, false);
                self.fold_fixed(
                    stop.saturating_add(4),
                    out.saturating_add(4),
                    base.saturating_add(1),
                    true,
                );
            }
            if var && !self.var_index(stop.saturating_add(6)) {
                return false;
            }
        }
        true
    }

    /// `Affine2x3::subset` (in `Variable` for `var`).
    fn affine(&mut self, at: usize, var: bool) -> bool {
        let out = self.embed(at, 24);
        let base = self.t.u32(at.saturating_add(24));
        if var
            && base != NO_VARIATION
            && let Some(delta) = self.delta
        {
            for field in 0..6u32 {
                let offset = (field as usize).saturating_mul(4);
                let old = i32::from_be_bytes(self.t.u32(at.saturating_add(offset)).to_be_bytes());
                let sum = f64::from(old) + f64::from(delta(base.saturating_add(field)));
                let value = crate::instance::f64_to_i32(sofel_geom::float::hb_roundf(sum));
                self.s
                    .write_at(out.saturating_add(offset), &value.to_be_bytes());
            }
        }
        !var || self.var_index(at.saturating_add(24))
    }

    /// `Paint::dispatch` with the subset context: the paint at `at` into
    /// the current object.
    fn paint(&mut self, at: usize) -> bool {
        // HarfBuzz's sanitizer neuters the offset to a paint nested
        // deeper than this.
        if self.depth >= limits::MAX_NESTING_LEVEL {
            return false;
        }
        self.depth = self.depth.saturating_add(1);
        let ok = self.paint_at(at);
        self.depth = self.depth.saturating_sub(1);
        ok
    }

    // A static F2DOT14 scale outside [-2, 2) needs a 16.16 transform.
    fn scale_transform(&mut self, at: usize, format: u8, size: usize) -> Option<bool> {
        if !self.p.var.all_axes_pinned() || !matches!(format, 17 | 19 | 21 | 23) {
            return None;
        }
        let delta = self.delta?;
        let base = self.t.u32(at.saturating_add(size));
        if base == NO_VARIATION {
            return None;
        }
        let coord = |field: u32| {
            let offset = at
                .saturating_add(4)
                .saturating_add((field as usize).saturating_mul(2));
            f32::from(i16::from_be_bytes(self.t.u16(offset).to_be_bytes()))
        };
        let scale = |field| {
            crate::instance::f64_to_f32(sofel_geom::float::hb_roundf32(
                coord(field) + delta(base.saturating_add(field)),
            )) / 16384.0
        };
        let uniform = matches!(format, 21 | 23);
        let sx = scale(0);
        let sy = if uniform { sx } else { scale(1) };
        if (-2.0..2.0).contains(&sx) && (-2.0..2.0).contains(&sy) {
            return None;
        }
        let center = |field| {
            coord(field)
                + crate::instance::f64_to_f32(sofel_geom::float::hb_roundf32(delta(
                    base.saturating_add(field),
                )))
        };
        let (cx, cy) = match format {
            19 => (center(2), center(3)),
            23 => (center(1), center(2)),
            _ => (0.0, 0.0),
        };
        let (dx, dy) = (cx * (1.0 - sx), cy * (1.0 - sy));
        let ok = if (-32768.0..32768.0).contains(&dx) && (-32768.0..32768.0).contains(&dy) {
            self.transform_chain(at, &[[sx, sy, dx, dy]])
        } else {
            let outer = [1.0, 1.0, cx / 2.0, cy / 2.0];
            let inner = [1.0, 1.0, -cx / 2.0, -cy / 2.0];
            self.transform_chain(at, &[outer, outer, [sx, sy, 0.0, 0.0], inner, inner])
        };
        Some(ok)
    }

    fn transform_chain(&mut self, at: usize, matrices: &[[f32; 4]]) -> bool {
        let Some((&[sx, sy, dx, dy], rest)) = matrices.split_first() else {
            let offset = u24(self.t, at.saturating_add(1));
            return offset != 0 && self.paint(at.saturating_add(offset as usize));
        };
        self.s.u8(12);
        let child = self.s.u24(0);
        let affine = self.s.u24(0);
        self.s.push();
        for value in [sx, 0.0, 0.0, sy, dx, dy] {
            let fixed =
                crate::instance::f64_to_i32(sofel_geom::float::hb_roundf32(value * 65536.0));
            self.s.extend(&fixed.to_be_bytes());
        }
        let matrix = self.s.pop_pack(true);
        self.s.link24(affine, matrix);
        self.s.push();
        if !self.transform_chain(at, rest) {
            self.s.pop_discard();
            return false;
        }
        let source = self.s.pop_pack(true);
        self.s.link24(child, source);
        true
    }

    /// [`paint`](Self::paint) without the nesting check.
    fn paint_at(&mut self, at: usize) -> bool {
        let format = self.t.u8(at);
        let Some((size, var)) = paint_size(format) else {
            return false;
        };
        if format == 7
            && self.p.var.all_axes_pinned()
            && let Some(result) = self.negative_radial(at)
        {
            return result;
        }
        if let Some(result) = self.scale_transform(at, format, size) {
            return result;
        }
        let out = self.embed(at, size);
        let p = self.t.at(at);
        if var && self.p.var.instancing() {
            self.fold_paint_fields(format, at, out, size);
        }
        if (var || format == 13) && self.p.var.instancing() && self.p.var.all_axes_pinned() {
            self.s.write_at(out, &[format.saturating_sub(1)]);
        }
        let ok = match format {
            1 => {
                let v = if p.u8(1) == 0 {
                    0
                } else {
                    self.p
                        .color
                        .layers
                        .get(&p.u32(2))
                        .copied()
                        .unwrap_or(INVALID)
                };
                self.s
                    .check_assign_u32(out.saturating_add(2), u64::from(v), Errors::INT_OVERFLOW)
            }
            2 | 3 => self.palette(out.saturating_add(1), p.u16(1)),
            4..=9 => {
                let off = u24(p, 1);
                let target = (off != 0).then(|| at.saturating_add(off as usize));
                self.serialize_subset(out.saturating_add(1), 3, target, |c, t| {
                    c.color_line(t, var)
                })
            }
            10 | 33 => {
                let gid = if format == 10 {
                    u32::from(p.u16(4))
                } else {
                    u24(p, 4)
                };
                let new = u64::from(self.p.glyphs.new_gid(gid).unwrap_or(INVALID));
                let fits = if format == 10 {
                    self.s
                        .check_assign_u16(out.saturating_add(4), new, Errors::INT_OVERFLOW)
                } else {
                    self.s
                        .check_assign_u24(out.saturating_add(4), new, Errors::INT_OVERFLOW)
                };
                fits && self.paint_offset(out, at, 1)
            }
            11 => {
                let new = u64::from(
                    self.p
                        .glyphs
                        .new_gid(u32::from(p.u16(1)))
                        .unwrap_or(INVALID),
                );
                self.s
                    .check_assign_u16(out.saturating_add(1), new, Errors::INT_OVERFLOW)
            }
            12 | 13 => {
                let off = u24(p, 4);
                let target = (off != 0).then(|| at.saturating_add(off as usize));
                let transform = self.serialize_subset(out.saturating_add(4), 3, target, |c, t| {
                    c.affine(t, format == 13)
                });
                transform && self.paint_offset(out, at, 1)
            }
            32 => {
                let src = self.paint_offset(out, at, 1);
                let backdrop = self.paint_offset(out, at, 5);
                src || backdrop
            }
            _ => self.paint_offset(out, at, 1),
        };
        ok && (!var || self.var_index(at.saturating_add(size)))
    }

    /// `BaseGlyphList::subset`.
    fn base_glyph_list(&mut self, list_at: usize) -> bool {
        let list = self.t.at(list_at);
        self.s.u32(0);
        let mut len = 0u32;
        for i in 0..list.u32(0) as usize {
            let rec = 4usize.saturating_add(i.saturating_mul(6));
            let gid = u32::from(list.u16(rec));
            if !self.p.color.glyphset_colred.contains(gid) {
                continue;
            }
            let out = self.embed(list_at.saturating_add(rec), 6);
            let new = u64::from(self.p.glyphs.new_gid(gid).unwrap_or(INVALID));
            if !self.s.check_assign_u16(out, new, Errors::INT_OVERFLOW) {
                return false;
            }
            let off = list.u32(rec.saturating_add(2));
            let target = (off != 0).then(|| list_at.saturating_add(off as usize));
            if !self.serialize_subset(out.saturating_add(2), 4, target, |c, t| c.paint(t)) {
                return false;
            }
            len = len.saturating_add(1);
            self.s.set_u32(0, len);
        }
        len != 0
    }

    /// `LayerList::subset`.
    fn layer_list(&mut self, list_at: usize) -> bool {
        let list = self.t.at(list_at);
        self.s.u32(0);
        let mut len = 0u32;
        let mut ret = false;
        let layers: Vec<u32> = self.p.color.layers.keys().copied().collect();
        for i in layers.into_iter().filter(|&i| i < list.u32(0)) {
            let slot = self.s.u32(0);
            len = len.saturating_add(1);
            self.s.set_u32(0, len);
            let off = list.u32(4usize.saturating_add((i as usize).saturating_mul(4)));
            let target = (off != 0).then(|| list_at.saturating_add(off as usize));
            ret |= self.serialize_subset(slot, 4, target, |c, t| c.paint(t));
        }
        ret
    }

    /// `ClipBox::subset`.
    fn clip_box(&mut self, at: usize) -> bool {
        match self.t.u8(at) {
            1 => {
                self.embed(at, 9);
                true
            }
            2 => {
                let out = self.embed(at, 9);
                if self.p.var.instancing() {
                    let base = self.t.u32(at.saturating_add(9));
                    if base != NO_VARIATION
                        && let Some(delta) = self.delta
                    {
                        for field in 0..4u32 {
                            let d = delta(base.saturating_add(field));
                            let d = if field < 2 {
                                sofel_geom::float::floorf(d)
                            } else {
                                sofel_geom::float::ceilf(d)
                            };
                            let offset = 1usize.saturating_add((field as usize).saturating_mul(2));
                            self.fold_coord(
                                at.saturating_add(offset),
                                out.saturating_add(offset),
                                f64::from(d),
                            );
                        }
                    }
                    if self.p.var.all_axes_pinned() {
                        self.s.write_at(out, &[1]);
                        return true;
                    }
                }
                self.var_index(at.saturating_add(9))
            }
            _ => false,
        }
    }

    /// `ClipList::subset`.
    fn clip_list(&mut self, list_at: usize) -> bool {
        let list = self.t.at(list_at);
        self.s.u8(list.u8(0));
        self.s.u32(0);
        let n = list.u32(1) as usize;
        let record = |i: usize| 5usize.saturating_add(i.saturating_mul(7));
        let mut offsets: BTreeMap<u32, u32> = BTreeMap::new();
        for gid in self.p.color.glyphset_colred.iter() {
            let Some(new) = self.p.glyphs.new_gid(gid) else {
                continue;
            };
            let found = bsearch(n, |i| {
                let r = record(i);
                if gid < u32::from(list.u16(r)) {
                    core::cmp::Ordering::Less
                } else if gid <= u32::from(list.u16(r.saturating_add(2))) {
                    core::cmp::Ordering::Equal
                } else {
                    core::cmp::Ordering::Greater
                }
            });
            if let Some(i) = found {
                offsets.insert(new, u24(list, record(i).saturating_add(4)));
            }
        }
        // serialize_clip_records
        let Some((&first, &first_off)) = offsets.iter().next() else {
            return false;
        };
        let mut groups: Vec<(u32, u32, u32)> = Vec::new();
        let (mut start, mut prev, mut prev_off) = (first, first, first_off);
        for (&g, &off) in offsets.iter().skip(1) {
            if g == prev.wrapping_add(1) && off == prev_off {
                prev = g;
                continue;
            }
            groups.push((start, prev, prev_off));
            (start, prev, prev_off) = (g, g, off);
        }
        groups.push((start, prev, prev_off));
        let mut count = 0u32;
        for (start, end, off) in groups {
            let out = self.s.u16((start & 0xFFFF) as u16);
            self.s.u16((end & 0xFFFF) as u16);
            self.s.u24(off);
            let target = (off != 0).then(|| list_at.saturating_add(off as usize));
            if !self.serialize_subset(out.saturating_add(4), 3, target, |c, t| c.clip_box(t)) {
                return false;
            }
            count = count.saturating_add(1);
        }
        self.s
            .check_assign_u32(1, u64::from(count), Errors::INT_OVERFLOW)
    }
}

/// `COLR::subset`.
pub(crate) fn subset(
    plan: &Plan,
    font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let Some(colr) = sanitized(src) else {
        return Err(SubsetError::Failed { tag });
    };
    let t = D(src);
    let version = colr.version();
    let glyphset = &plan.color.glyphset_colred;
    let num_layers = colr.num_layers();
    // base_it and layer_it
    let mut base: Vec<(u32, u16)> = Vec::new();
    let mut layers: Vec<Vec<(u16, u16)>> = Vec::new();
    for new in 0..plan.num_output_glyphs {
        let Some(old) = plan.glyphs.old_gid(new).filter(|&o| glyphset.contains(o)) else {
            continue;
        };
        let Some(r) = colr.find_base_glyph(old) else {
            continue;
        };
        base.push((new, r.num_layers));
        let first = usize::from(r.first_layer);
        if first >= num_layers || first.saturating_add(usize::from(r.num_layers)) > num_layers {
            continue;
        }
        let out: Option<Vec<(u16, u16)>> = (first..first.saturating_add(usize::from(r.num_layers)))
            .map(|i| {
                let (g, p) = colr.layer(i)?;
                let new_g = plan.glyphs.new_gid(u32::from(g))?;
                let new_p = plan
                    .color
                    .palettes
                    .get(&u32::from(p))
                    .copied()
                    .unwrap_or(INVALID);
                Some(((new_g & 0xFFFF) as u16, (new_p & 0xFFFF) as u16))
            })
            .collect();
        if let Some(out) = out {
            layers.push(out);
        }
    }
    let total: u32 = base.iter().map(|b| u32::from(b.1)).sum();
    if total > 0xFFFF {
        return Ok(false);
    }
    if version == 0 && (base.is_empty() || layers.is_empty()) {
        return Ok(false);
    }
    s.zeros(14);
    if version == 0 || retains_no_v1_paints(&colr, glyphset) {
        return Ok(serialize_v0(s, 0, &base, &layers));
    }
    s.zeros(20);
    if !serialize_v0(s, version, &base, &layers) {
        return Ok(false);
    }
    if !plan.color.variation_store.is_empty() {
        s.push();
        s.extend(&plan.color.variation_store);
        let idx = s.pop_pack(true);
        s.link32(30, idx);
    }
    use sofel_font::var::{HbF32, Location, StoreKind};
    let pinned = plan.var.instancing() && !plan.var.n.pinned_at_default;
    let location =
        pinned.then(|| Location::from_normalized_2_14(font, &plan.var.n.normalized_coords));
    let instance = location.as_ref().map(|location| font.instance(location));
    let index_map = colr
        .var_index_map()
        .ok()
        .flatten()
        .filter(|map| !map.is_empty());
    let delta = |index| {
        let mapped = index_map.map_or(index, |map| map.map(index));
        let [a, b, c, d] = mapped.to_be_bytes();
        instance
            .as_ref()
            .and_then(|instance| {
                instance
                    .item_delta::<HbF32>(
                        StoreKind::Colr,
                        u16::from_be_bytes([a, b]),
                        u16::from_be_bytes([c, d]),
                    )
                    .ok()
            })
            .unwrap_or(0.0)
    };
    let mut c = Sub {
        p: plan,
        t,
        s,
        depth: 0,
        delta: pinned.then_some(&delta),
    };
    let off = |at: usize| {
        let o = t.u32(at);
        (o != 0).then_some(o as usize)
    };
    if !c.serialize_subset(14, 4, off(14), |c, at| c.base_glyph_list(at)) {
        return Ok(false);
    }
    c.serialize_subset(18, 4, off(18), |c, at| c.layer_list(at));
    c.serialize_subset(22, 4, off(22), |c, at| c.clip_list(at));
    if !plan.color.index_map.is_empty() {
        s.push();
        s.extend(&plan.color.index_map);
        let idx = s.pop_pack(true);
        s.link32(26, idx);
    }
    Ok(true)
}

/// `COLR::serialize_V0` into the 14-byte header of the current object.
fn serialize_v0(
    s: &mut Serializer,
    version: u16,
    base: &[(u32, u16)],
    layers: &[Vec<(u16, u16)>],
) -> bool {
    if base.len() != layers.len() {
        return false;
    }
    s.set_u16(0, version);
    s.set_u16(2, n16(base.len()));
    if base.is_empty() {
        return true;
    }
    s.push();
    let mut num_layers = 0u16;
    for &(gid, n) in base {
        s.u16((gid & 0xFFFF) as u16);
        s.u16(num_layers);
        s.u16(n);
        num_layers = num_layers.wrapping_add(n);
    }
    let idx = s.pop_pack(true);
    s.link32(4, idx);
    s.set_u16(12, num_layers);
    s.push();
    for &(g, p) in layers.iter().flatten() {
        s.u16(g);
        s.u16(p);
    }
    let idx = s.pop_pack(true);
    s.link32(8, idx);
    true
}
