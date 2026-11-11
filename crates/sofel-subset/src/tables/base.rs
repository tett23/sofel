//! BASE subsetting (`hb-ot-layout-base-table.hh`), including removal of the
//! variation store when every axis is pinned to its default.

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_write::Serializer;

use super::Tables;
use crate::error::SubsetError;
use crate::layout::common::Sub;
use crate::layout::data::D;
use crate::plan::Plan;

fn device_copy(s: &mut Sub<'_, '_>, dev: D<'_>) -> bool {
    if !matches!(dev.u16(4), 1..=3) {
        return false;
    }
    let start = dev.u16(0);
    let end = dev.u16(2);
    let f = dev.u16(4);
    let size = if start > end {
        6
    } else {
        2usize.saturating_mul(
            4usize.saturating_add(usize::from(end.wrapping_sub(start) >> 4u16.wrapping_sub(f))),
        )
    };
    s.s.extend(dev.slice(0, size));
    true
}

/// `BaseCoord::subset`.
fn base_coord(s: &mut Sub<'_, '_>, c: D<'_>) -> bool {
    match c.u16(0) {
        1 => {
            s.s.extend(c.slice(0, 4));
            true
        }
        2 => {
            s.s.extend(c.slice(0, 8));
            let Some(g) = s.p.glyphs.new_gid(u32::from(c.u16(4))) else {
                return false;
            };
            s.s.check_assign_u16(4, u64::from(g), sofel_write::Errors::INT_OVERFLOW)
        }
        3 => {
            s.s.extend(c.slice(0, 4));
            let dev = c.off16(4);
            let at = s.s.u16(0);
            s.serialize_copy(at, 2, dev, |s| device_copy(s, dev));
            true
        }
        _ => false,
    }
}

/// `MinMax::subset`.
fn min_max(s: &mut Sub<'_, '_>, m: D<'_>) -> bool {
    let min = m.off16(0);
    let max = m.off16(2);
    s.s.u16(0);
    s.s.u16(0);
    let count_at = s.s.u16(0);
    let mut ret = s.serialize_subset_opt(0, 2, min, |s| base_coord(s, min));
    ret |= s.serialize_subset_opt(2, 2, max, |s| base_coord(s, max));
    let mut len = 0u16;
    for i in 0..usize::from(m.u16(4)) {
        let rec = 6usize.saturating_add(i.saturating_mul(8));
        let tag = m.u32(rec);
        if !s.p.layout.layout_features.contains(tag) {
            continue;
        }
        let snap = s.s.snapshot();
        s.s.u32(tag);
        let min_at = s.s.u16(0);
        let max_at = s.s.u16(0);
        let fmin = m.off16(rec.saturating_add(4));
        let fmax = m.off16(rec.saturating_add(6));
        let mut ok = s.serialize_subset_opt(min_at, 2, fmin, |s| base_coord(s, fmin));
        ok |= s.serialize_subset_opt(max_at, 2, fmax, |s| base_coord(s, fmax));
        if ok {
            len = len.wrapping_add(1);
        } else {
            s.s.revert(snap);
        }
    }
    if len > 0 {
        ret = true;
    }
    if !ret {
        return false;
    }
    s.s.set_u16(count_at, len);
    true
}

/// `BaseValues::subset`.
fn base_values(s: &mut Sub<'_, '_>, v: D<'_>) -> bool {
    s.s.u16(v.u16(0));
    let count_at = s.s.u16(0);
    let mut ret = false;
    for i in 0..usize::from(v.u16(2)) {
        let c = v.off16(4usize.saturating_add(i.saturating_mul(2)));
        let at = s.s.u16(0);
        s.bump_u16(count_at, 1);
        ret |= s.serialize_subset_opt(at, 2, c, |s| base_coord(s, c));
    }
    ret
}

/// `BaseScript::subset`.
fn base_script(s: &mut Sub<'_, '_>, b: D<'_>) -> bool {
    s.s.u16(0);
    s.s.u16(0);
    let count_at = s.s.u16(0);
    let values = b.off16(0);
    let mut ret = s.serialize_subset_opt(0, 2, values, |s| base_values(s, values));
    let dmm = b.off16(2);
    ret |= s.serialize_subset_opt(2, 2, dmm, |s| min_max(s, dmm));
    let mut len = 0u16;
    for i in 0..usize::from(b.u16(4)) {
        let rec = 6usize.saturating_add(i.saturating_mul(6));
        let snap = s.s.snapshot();
        s.s.u32(b.u32(rec));
        let at = s.s.u16(0);
        let mm = b.off16(rec.saturating_add(4));
        if s.serialize_subset_opt(at, 2, mm, |s| min_max(s, mm)) {
            ret = true;
            len = len.wrapping_add(1);
        } else {
            s.s.revert(snap);
        }
    }
    if !ret {
        return false;
    }
    s.s.set_u16(count_at, len);
    true
}

/// `BaseScriptList::subset`.
fn base_script_list(s: &mut Sub<'_, '_>, l: D<'_>) -> bool {
    let count_at = s.s.u16(0);
    let mut len = 0u16;
    for i in 0..usize::from(l.u16(0)) {
        let rec = 2usize.saturating_add(i.saturating_mul(6));
        let tag = l.u32(rec);
        if !s.p.layout.layout_scripts.contains(tag) {
            continue;
        }
        let snap = s.s.snapshot();
        s.s.u32(tag);
        let at = s.s.u16(0);
        let bs = l.off16(rec.saturating_add(4));
        if s.serialize_subset_opt(at, 2, bs, |s| base_script(s, bs)) {
            len = len.wrapping_add(1);
        } else {
            s.s.revert(snap);
        }
    }
    if len == 0 {
        return false;
    }
    s.s.set_u16(count_at, len);
    true
}

/// `Axis::subset`.
fn axis(s: &mut Sub<'_, '_>, a: D<'_>) -> bool {
    s.s.u16(0);
    s.s.u16(0);
    let tags = a.off16(0);
    let mut ret = s.serialize_copy(0, 2, tags, |s| {
        let n = usize::from(tags.u16(0));
        s.s.extend(tags.slice(0, 2usize.saturating_add(n.saturating_mul(4))));
        true
    });
    let list = a.off16(2);
    ret |= s.serialize_subset_opt(2, 2, list, |s| base_script_list(s, list));
    ret
}

/// `BASE::subset`.
pub(crate) fn subset(
    p: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _out: &mut Tables,
) -> Result<bool, SubsetError> {
    let d = D(src);
    let version = d.u32(0);
    let mut c = Sub::new(p, s, false);
    c.s.u32(version);
    c.s.u16(0);
    c.s.u16(0);
    let mut ret = false;
    if version >= 0x0001_0001 {
        c.s.u32(0);
        if p.var.instancing() && p.var.all_axes_pinned() {
            // BASE::subset_varstore keeps the null offset, even with no Axis.
            ret = true;
        } else if d.u32(8) != 0 {
            return Err(SubsetError::Unsupported {
                tag,
                reason: "the BASE item variation store (ADR 0022 stage 10)",
            });
        }
    }
    let h = d.off16(4);
    ret |= c.serialize_subset_opt(4, 2, h, |s| axis(s, h));
    let v = d.off16(6);
    ret |= c.serialize_subset_opt(6, 2, v, |s| axis(s, v));
    if let Some(reason) = c.unsupported {
        return Err(SubsetError::Unsupported { tag, reason });
    }
    Ok(ret)
}
