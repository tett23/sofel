//! FeatureVariations selection for GSUB and GPOS (ADR 0027).

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
                condition_delta((index >> 16) as u16, index as u16)
            };
            f64::from(default) + delta > 0.0
        }),
        3 => (0..condition.children_len().unwrap_or(0)).all(|index| {
            condition
                .child(index)
                .is_ok_and(|child| condition_matches(child, coords, condition_delta, depth - 1))
        }),
        4 => (0..condition.children_len().unwrap_or(0)).any(|index| {
            condition
                .child(index)
                .is_ok_and(|child| condition_matches(child, coords, condition_delta, depth - 1))
        }),
        5 => condition
            .child(0)
            .is_ok_and(|child| !condition_matches(child, coords, condition_delta, depth - 1)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use sofel_font::read::feature_variations::{Condition, FeatureVariations};

    use super::{condition_matches, find_index, substituted_feature};

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
