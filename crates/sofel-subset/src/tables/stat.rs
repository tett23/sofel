//! STAT with axes set (`src/hb-ot-stat-table.hh`): the axis values
//! outside the axis ranges are dropped.

use alloc::collections::BTreeMap;

use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_font::read::stat::{AxisValue, Stat};
use sofel_write::{Serializer, Whence};

use crate::error::SubsetError;
use crate::input::AxisRange;
use crate::instance::fixed_to_f32;
use crate::plan::Plan;
use crate::tables::Tables;

/// `axis_value_is_outside_axis_range`.
fn outside(tag: Tag, value: f32, user: &BTreeMap<Tag, AxisRange>) -> bool {
    user.get(&tag)
        .is_some_and(|r| value < r.min || value > r.max)
}

/// `AxisValue::keep_axis_value`: whether every value of `v` is inside
/// the axis ranges of `user` (an unknown format is never kept).
pub(crate) fn keep_axis_value(
    stat: &Stat<'_>,
    v: &AxisValue<'_>,
    user: &BTreeMap<Tag, AxisRange>,
) -> bool {
    if matches!(v, AxisValue::Unknown { .. }) {
        return false;
    }
    !v.axis_values().any(|(axis, value)| {
        // An axis index out of range reads HarfBuzz's Null record: tag 0.
        let tag = stat
            .design_axis(usize::from(axis))
            .map_or(Tag([0; 4]), |a| a.tag);
        outside(tag, fixed_to_f32(value), user)
    })
}

/// `STAT::subset`.
pub(crate) fn subset(
    plan: &Plan,
    _font: &Font<'_>,
    tag: Tag,
    src: &[u8],
    s: &mut Serializer,
    _tables: &mut Tables,
) -> Result<bool, SubsetError> {
    let stat = Stat::new(src).map_err(|_| SubsetError::Malformed { tag })?;
    let user = plan.var.user();
    s.extend(src.get(..20).unwrap_or(&[]));
    for i in 0..stat.design_axis_count() {
        if let Some(a) = stat.design_axis(i) {
            s.extend(&a.tag.0);
            s.u16(a.name_id);
            s.u16(a.ordering);
        }
    }
    if stat.design_axis_count() > 0 {
        s.set_u32(8, 20);
    }
    // offsetToAxisValueOffsets.serialize_subset: the offset array object,
    // its axis values pushed and linked to it.
    s.set_u32(14, 0);
    s.push();
    let mut count = 0u16;
    for i in 0..stat.axis_value_count() {
        if stat.axis_value_offset(i).unwrap_or(0) == 0 {
            continue;
        }
        let snap = s.snapshot();
        let at = s.u16(0);
        s.push();
        let kept = stat
            .axis_value(i)
            .filter(|(v, _)| keep_axis_value(&stat, v, user))
            .map(|(_, bytes)| s.extend(bytes))
            .is_some();
        if kept {
            let o = s.pop_pack(true);
            s.add_link(at, 2, false, o, Whence::Head, 0);
            count = count.wrapping_add(1);
        } else {
            s.pop_discard();
            s.revert(snap);
        }
    }
    if count != 0 {
        let o = s.pop_pack(true);
        s.add_link(14, 4, false, o, Whence::Head, 0);
    } else {
        s.pop_discard();
    }
    s.set_u16(12, count);
    Ok(true)
}
