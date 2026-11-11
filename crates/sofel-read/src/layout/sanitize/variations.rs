//! FeatureVariations and minor version 1 lookup variation records.
use super::{children::Budget, lists, target};
use crate::{
    be,
    feature_variations::{Condition, ConditionSet, FeatureTableSubstitution},
    layout::{FeatureList, LayoutTable},
};

fn condition(value: Condition<'_>, depth: u8, budget: &mut Budget) -> bool {
    depth > 0
        && budget.take(if matches!(value.format(), 3 | 4) {
            usize::from(value.children_len().unwrap_or(0)).saturating_mul(3)
        } else {
            0
        })
        && (0..value.children_len().unwrap_or(0)).all(|i| {
            value.child(i).is_ok_and(|child| {
                child.is_none_or(|child| condition(child, depth.saturating_sub(1), budget))
            })
        })
}

fn records(data: &[u8], at: usize, stride: usize) -> Option<&[u8]> {
    let count = usize::try_from(be::u32_at(data, at)?).ok()?;
    let start = at.checked_add(4)?;
    be::slice_at(data, start, count.checked_mul(stride)?)
}

fn lookup_conditions(data: &[u8], budget: &mut Budget) -> bool {
    be::u16_at(data, 0) == Some(1)
        && records(data, 6, 8).is_some_and(|rows| {
            budget.take(rows.len())
                && rows.as_chunks::<8>().0.iter().all(|r| {
                    be::u32_at(r, 0).is_some_and(|off| {
                        target(data, off, |v| {
                            Condition::new(v).is_ok_and(|c| condition(c, 64, budget))
                        })
                    }) && be::u32_at(r, 4).is_some_and(|off| {
                        target(data, off, |v| {
                            be::u16_at(v, 0).is_some_and(|n| {
                                be::slice_at(v, 2, usize::from(n).saturating_mul(2))
                                    .is_some_and(|v| budget.take(v.len()))
                            })
                        })
                    })
                })
        })
}

fn conditions(set: ConditionSet<'_>, budget: &mut Budget) -> bool {
    budget.take(usize::from(set.len()).saturating_mul(4))
        && (0..set.len()).all(|i| {
            set.condition(i)
                .is_ok_and(|c| c.is_none_or(|c| condition(c, 64, budget)))
        })
}

fn substitutes(
    table: FeatureTableSubstitution<'_>,
    features: Option<FeatureList<'_>>,
    budget: &mut Budget,
) -> bool {
    if table.version() >> 16 != 1 || !budget.take(usize::from(table.len()).saturating_mul(6)) {
        return false;
    }
    (0..table.len()).all(|i| {
        let Some(record) = table.record(i) else {
            return false;
        };
        let tag = features
            .and_then(|f| f.tag(usize::from(record.feature_index())))
            .map_or(0, |t| t.to_u32());
        record
            .feature()
            .is_ok_and(|f| f.is_none_or(|f| lists::feature(tag, f, budget)))
    })
}

pub(super) fn inspect(table: &LayoutTable<'_>, budget: &mut Budget) -> bool {
    let var = match table.feature_variations() {
        Ok(Some(v)) => v,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let features = table.feature_list().ok().flatten();
    if !budget.take((var.len() as usize).saturating_mul(8)) {
        return false;
    }
    for i in 0..var.len() {
        let Some(record) = var.record(i) else {
            return false;
        };
        if !record
            .conditions()
            .is_ok_and(|set| set.is_none_or(|set| conditions(set, budget)))
            || !record
                .substitutions()
                .is_ok_and(|table| table.is_none_or(|table| substitutes(table, features, budget)))
        {
            return false;
        }
    }
    if var.version() == 0x10000 {
        return true;
    }
    let Some(start) = (var.len() as usize)
        .checked_mul(8)
        .and_then(|v| v.checked_add(8))
    else {
        return false;
    };
    records(var.data(), start, 6).is_some_and(|rows| {
        budget.take(rows.len())
            && rows.as_chunks::<6>().0.iter().all(|r| {
                be::u32_at(r, 2)
                    .is_some_and(|off| target(var.data(), off, |t| lookup_conditions(t, budget)))
            })
    })
}
