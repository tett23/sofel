//! FeatureVariations and minor version 1 lookup variation records.
use super::{children::Budget, lists, target};
use crate::{be, feature_variations::Condition, layout::LayoutTable};

fn condition(value: Condition<'_>, depth: u8, budget: &mut Budget) -> bool {
    depth > 0
        && budget.take(2)
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
                                be::slice_at(v, 2, usize::from(n).saturating_mul(2)).is_some()
                            })
                        })
                    })
                })
        })
}

pub(super) fn inspect(table: &LayoutTable<'_>, budget: &mut Budget) -> bool {
    let var = match table.feature_variations() {
        Ok(Some(v)) => v,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let features = table.feature_list().ok().flatten();
    if !budget.take(var.len() as usize)
        || !(0..var.len()).all(|i| {
            var.record(i).is_some_and(|r| {
                r.conditions().is_ok_and(|set| {
                    set.is_none_or(|set| {
                        budget.take(usize::from(set.len()))
                            && (0..set.len()).all(|n| {
                                set.condition(n)
                                    .is_ok_and(|c| c.is_none_or(|c| condition(c, 64, budget)))
                            })
                    })
                }) && r.substitutions().is_ok_and(|subs| {
                    subs.is_none_or(|s| {
                        s.version() >> 16 == 1
                            && budget.take(usize::from(s.len()))
                            && (0..s.len()).all(|j| {
                                s.record(j).is_some_and(|r| {
                                    let tag = features
                                        .and_then(|f| f.tag(usize::from(r.feature_index())))
                                        .map_or(0, |tag| u32::from_be_bytes(tag.0));
                                    r.feature().is_ok_and(|f| {
                                        f.is_none_or(|f| lists::feature(tag, f, budget))
                                    })
                                })
                            })
                    })
                })
            })
        })
    {
        return false;
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
