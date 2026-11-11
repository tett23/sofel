//! FeatureVariations selection for GSUB and GPOS (ADR 0027).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use sofel_font::read::feature_variations::{Condition, ConditionSet, FeatureVariations};
use sofel_font::read::layout::Feature;

/// The replacement Feature table for an index in the selected record.
pub(crate) fn substituted_feature<'a>(
    variations: FeatureVariations<'a>,
    variation_index: u32,
    feature_index: u16,
) -> Option<Feature<'a>> {
    let substitutions = variations.record(variation_index)?.substitutions().ok()??;
    (0..substitutions.len()).find_map(|index| {
        let record = substitutions.record(index)?;
        (record.feature_index() == feature_index).then(|| {
            record
                .feature()
                .ok()
                .flatten()
                .and_then(|bytes| Feature::new(bytes).ok())
        })?
    })
}

/// The first matching variation record, as `FeatureVariations::find_index`.
pub(crate) fn find_index(
    variations: FeatureVariations<'_>,
    coords: &[i32],
    condition_delta: impl Fn(u16, u16) -> f64,
) -> Option<u32> {
    (0..variations.len()).find(|&index| {
        variations
            .record(index)
            .and_then(|record| record.conditions().ok())
            .is_some_and(|conditions| {
                conditions.is_none_or(|set| condition_set_matches(set, coords, &condition_delta))
            })
    })
}

/// Sorted, distinct offsets of true, non-null lookup conditions.
pub(crate) fn collect_lookup_conditions(
    variations: FeatureVariations<'_>,
    coords: &[i32],
    condition_delta: &impl Fn(u16, u16) -> f64,
) -> Vec<u32> {
    let mut evaluated = BTreeMap::<u32, bool>::new();
    let mut true_offsets = BTreeSet::new();
    for i in 0..variations.lookup_record_count() {
        let Some(table) = variations
            .lookup_record(i)
            .and_then(|record| record.feature_lookups().ok().flatten())
        else {
            continue;
        };
        let table_at = variations.data().len().saturating_sub(table.data().len());
        for j in 0..table.len() {
            let Some(record) = table.record(j) else {
                continue;
            };
            let local = record.condition_offset();
            if local == 0 {
                continue;
            }
            let Some(offset) = table_at
                .checked_add(usize::try_from(local).unwrap_or(usize::MAX))
                .and_then(|value| u32::try_from(value).ok())
            else {
                continue;
            };
            let matches = *evaluated.entry(offset).or_insert_with(|| {
                record.condition().is_ok_and(|condition| {
                    condition_matches(condition, coords, condition_delta, 64)
                })
            });
            if matches {
                true_offsets.insert(offset);
            }
        }
    }
    true_offsets.into_iter().collect()
}

/// The first LookupVariationRecord for a feature, when present.
pub(crate) fn lookup_variation_lookups(
    variations: FeatureVariations<'_>,
    feature_index: u16,
    true_conditions: &[u32],
) -> Option<(bool, Vec<u16>)> {
    let record = (0..variations.lookup_record_count())
        .filter_map(|i| variations.lookup_record(i))
        .find(|record| record.feature_index() == feature_index)?;
    let Some(table) = record.feature_lookups().ok().flatten() else {
        return Some((false, Vec::new()));
    };
    let table_at = variations.data().len().saturating_sub(table.data().len());
    let mut lookups = Vec::new();
    for i in 0..table.len() {
        let Some(condition) = table.record(i) else {
            continue;
        };
        let local = condition.condition_offset();
        let applies = if local == 0 {
            true
        } else {
            table_at
                .checked_add(usize::try_from(local).unwrap_or(usize::MAX))
                .and_then(|value| u32::try_from(value).ok())
                .is_some_and(|offset| true_conditions.binary_search(&offset).is_ok())
        };
        if applies && let Ok(Some(indices)) = condition.lookup_indices() {
            lookups.extend((0..indices.len()).filter_map(|j| indices.get(j)));
        }
    }
    lookups.sort_unstable();
    lookups.dedup();
    Some((table.flags() & 1 != 0, lookups))
}

fn condition_set_matches(
    set: ConditionSet<'_>,
    coords: &[i32],
    condition_delta: &impl Fn(u16, u16) -> f64,
) -> bool {
    (0..set.len()).all(|index| {
        set.condition(index)
            .is_ok_and(|condition| condition_matches(condition, coords, condition_delta, 64))
    })
}

/// A null offset is HarfBuzz's true condition (format zero).
fn condition_matches(
    condition: Option<Condition<'_>>,
    coords: &[i32],
    condition_delta: &impl Fn(u16, u16) -> f64,
    depth: u8,
) -> bool {
    if depth == 0 {
        return false;
    }
    let Some(condition) = condition else {
        return true;
    };
    match condition.format() {
        0 => true,
        1 => condition
            .axis_range()
            .is_some_and(|(axis, minimum, maximum)| {
                let coord = coords.get(usize::from(axis)).copied().unwrap_or(0);
                i32::from(minimum) <= coord && coord <= i32::from(maximum)
            }),
        2 => condition.value().is_some_and(|(default, index)| {
            let delta = if index == u32::MAX {
                0.0
            } else {
                condition_delta(
                    u16::try_from(index >> 16).unwrap_or(0),
                    u16::try_from(index & 0xffff).unwrap_or(0),
                )
            };
            f64::from(default) + delta > 0.0
        }),
        3 => (0..condition.children_len().unwrap_or(0)).all(|index| {
            condition.child(index).is_ok_and(|child| {
                condition_matches(child, coords, condition_delta, depth.saturating_sub(1))
            })
        }),
        4 => (0..condition.children_len().unwrap_or(0)).any(|index| {
            condition.child(index).is_ok_and(|child| {
                condition_matches(child, coords, condition_delta, depth.saturating_sub(1))
            })
        }),
        5 => condition.child(0).is_ok_and(|child| {
            !condition_matches(child, coords, condition_delta, depth.saturating_sub(1))
        }),
        _ => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use alloc::vec;
    use sofel_font::read::feature_variations::{Condition, FeatureVariations};

    use super::{
        collect_lookup_conditions, condition_matches, find_index, lookup_variation_lookups,
        substituted_feature,
    };

    #[test]
    fn adr0046_i4_first_record_null_union_and_dedup() {
        let mut data = vec![0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2];
        data.extend_from_slice(&[0, 3, 0, 0, 0, 24, 0, 3, 0, 0, 0, 0]);
        data.extend_from_slice(&[0, 1, 0, 0, 0, 1, 0, 0, 0, 2]);
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 26]);
        data.extend_from_slice(&[0, 0, 0, 32, 0, 0, 0, 40]);
        data.extend_from_slice(&[0, 2, 0, 4, 0, 4]);
        data.extend_from_slice(&[0, 1, 0, 0, 0x20, 0, 0x40, 0]);
        data.extend_from_slice(&[0, 2, 0, 2, 0, 5]);
        let fv = FeatureVariations::new(&data).unwrap();
        let false_state = collect_lookup_conditions(fv, &[0], &|_, _| 0.0);
        let true_state = collect_lookup_conditions(fv, &[0x3000], &|_, _| 0.0);
        assert_eq!(false_state, vec![]);
        assert_eq!(true_state, vec![56]);
        assert_eq!(
            lookup_variation_lookups(fv, 3, &false_state),
            Some((true, vec![4]))
        );
        assert_eq!(
            lookup_variation_lookups(fv, 3, &true_state),
            Some((true, vec![2, 4, 5]))
        );
        assert_eq!(lookup_variation_lookups(fv, 4, &true_state), None);
    }

    #[test]
    fn adr0046_i2_i3_shared_condition_and_null_state() {
        let mut data = vec![0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2];
        data.extend_from_slice(&[0, 1, 0, 0, 0, 24, 0, 2, 0, 0, 0, 24]);
        data.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 2]);
        data.extend_from_slice(&[0, 0, 0, 26, 0, 0, 0, 42]);
        data.extend_from_slice(&[0, 0, 0, 26, 0, 0, 0, 46]);
        data.extend_from_slice(&[0, 1, 0, 0, 0x20, 0, 0x40, 0]);
        data.extend_from_slice(&[0, 1, 0, 1]);
        data.extend_from_slice(&[0, 1, 0, 2]);
        let fv = FeatureVariations::new(&data).unwrap();
        let at_zero = collect_lookup_conditions(fv, &[0], &|_, _| 0.0);
        let at_middle = collect_lookup_conditions(fv, &[0x3000], &|_, _| 0.0);
        assert!(at_zero.is_empty());
        assert_eq!(at_middle, vec![50]);
    }

    #[test]
    fn adr0027_i_plan_axis_range_boundaries_and_first_record() {
        let mut data = vec![0, 1, 0, 0, 0, 0, 0, 2];
        data.extend_from_slice(&[0, 0, 0, 24, 0, 0, 0, 0]);
        data.extend_from_slice(&[0, 0, 0, 40, 0, 0, 0, 0]);
        data.extend_from_slice(&[0, 1, 0, 0, 0, 6]);
        data.extend_from_slice(&[0, 1, 0, 0, 0x20, 0, 0x30, 0]);
        data.extend_from_slice(&[0, 0]);
        data.extend_from_slice(&[0, 1, 0, 0, 0, 6]);
        data.extend_from_slice(&[0, 1, 0, 0, 0x30, 0, 0x40, 0]);
        let variations = FeatureVariations::new(&data).unwrap();
        let delta = |_: u16, _: u16| 0.0;
        assert_eq!(find_index(variations, &[0x2000], delta), Some(0));
        assert_eq!(find_index(variations, &[0x3000], delta), Some(0));
        assert_eq!(find_index(variations, &[0x4000], delta), Some(1));
        assert_eq!(find_index(variations, &[0x4001], delta), None);
        assert_eq!(find_index(variations, &[], delta), None);
    }

    #[test]
    fn adr0027_i_plan_boolean_value_and_null_conditions() {
        let value = Condition::new(&[0, 2, 0, 1, 0, 1, 0, 2]).unwrap();
        assert!(condition_matches(
            Some(value),
            &[],
            &|outer, inner| {
                assert_eq!((outer, inner), (1, 2));
                0.0
            },
            64
        ));
        assert!(!condition_matches(Some(value), &[], &|_, _| -1.0, 64));
        assert!(condition_matches(None, &[], &|_, _| 0.0, 64));
        let and_empty = Condition::new(&[0, 3, 0]).unwrap();
        let or_empty = Condition::new(&[0, 4, 0]).unwrap();
        let negated_null = Condition::new(&[0, 5, 0, 0, 0]).unwrap();
        assert!(condition_matches(Some(and_empty), &[], &|_, _| 0.0, 64));
        assert!(!condition_matches(Some(or_empty), &[], &|_, _| 0.0, 64));
        assert!(!condition_matches(Some(negated_null), &[], &|_, _| 0.0, 64));
    }

    #[test]
    fn adr0027_i_plan_nested_boolean_conditions_and_depth() {
        let mut bytes = vec![0, 3, 2, 0, 0, 9, 0, 0, 17];
        bytes.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0x20, 0]);
        bytes.extend_from_slice(&[0, 2, 0, 1, 0xff, 0xff, 0xff, 0xff]);
        let and = Condition::new(&bytes).unwrap();
        assert!(condition_matches(Some(and), &[0x2000], &|_, _| 0.0, 64));
        assert!(!condition_matches(Some(and), &[0x2001], &|_, _| 0.0, 64));
        assert!(!condition_matches(Some(and), &[0x2000], &|_, _| 0.0, 1));
        bytes[1] = 4;
        let or = Condition::new(&bytes).unwrap();
        assert!(condition_matches(Some(or), &[0x2001], &|_, _| 0.0, 64));
        let negate = Condition::new(&[0, 5, 0, 0, 5, 0, 1, 0, 0, 0, 0, 0x20, 0]).unwrap();
        assert!(!condition_matches(Some(negate), &[0x2000], &|_, _| 0.0, 64));
        assert!(condition_matches(Some(negate), &[0x2001], &|_, _| 0.0, 64));
    }

    #[test]
    fn adr0027_i_plan_substitution_uses_feature_index() {
        let mut data = vec![0, 1, 0, 0, 0, 0, 0, 1];
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 16]);
        data.extend_from_slice(&[0, 1, 0, 0, 0, 1, 0, 3, 0, 0, 0, 12]);
        data.extend_from_slice(&[0, 0, 0, 1, 0, 7]);
        let variations = FeatureVariations::new(&data).unwrap();
        let feature = substituted_feature(variations, 0, 3).unwrap();
        assert_eq!(feature.lookup_index(0), Some(7));
        assert!(substituted_feature(variations, 0, 2).is_none());
        assert!(substituted_feature(variations, 1, 3).is_none());
    }
}
