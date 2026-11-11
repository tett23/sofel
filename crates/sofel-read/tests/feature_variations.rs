//! FeatureVariations format 1–5 views and diagnostics (ADR 0027).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_read::feature_variations::{self, Condition, FeatureVariations};
use sofel_read::layout::LayoutTable;
use sofel_read::{Kind, ReadError, Severity, Tag};

#[test]
fn adr0027_feature_variations_record_and_axis_range() {
    let data = [
        0, 1, 0, 0, // version 1.0
        0, 0, 0, 1, // one record
        0, 0, 0, 16, // condition set at 16
        0, 0, 0, 0, // no substitutions
        0, 1, 0, 0, 0, 6, // one condition, at 22
        0, 1, 0, 2, 0xc0, 0, 0x40, 0, // axis 2, [-1, 1]
    ];
    let fv = FeatureVariations::new(&data).unwrap();
    assert_eq!(fv.version(), 0x0001_0000);
    assert_eq!(fv.len(), 1);
    let record = fv.record(0).unwrap();
    let set = record.conditions().unwrap().unwrap();
    assert_eq!(set.len(), 1);
    assert_eq!(
        set.condition(0).unwrap().unwrap().axis_range(),
        Some((2, -16384, 16384))
    );
    assert!(record.substitutions().unwrap().is_none());
    assert!(fv.record(1).is_none());
}

#[test]
fn adr0027_feature_variations_boolean_and_value_conditions() {
    let value = Condition::new(&[0, 2, 0xff, 0xff, 0, 1, 0, 2]).unwrap();
    assert_eq!(value.value(), Some((-1, 0x0001_0002)));

    for format in [3u8, 4] {
        let mut bytes = vec![0, format, 1, 0, 0, 6];
        bytes.extend_from_slice(&[0, 1, 0, 0, 0xc0, 0, 0x40, 0]);
        let c = Condition::new(&bytes).unwrap();
        assert_eq!(c.format(), u16::from(format));
        assert_eq!(c.children_len(), Some(1));
        assert_eq!(
            c.child(0).unwrap().unwrap().axis_range(),
            Some((0, -16384, 16384))
        );
    }
    let negate = Condition::new(&[0, 5, 0, 0, 5, 0, 1, 0, 0, 0xc0, 0, 0x40, 0]).unwrap();
    assert_eq!(negate.child(0).unwrap().unwrap().format(), 1);
    assert!(negate.child(1).unwrap().is_none());
}

#[test]
fn adr0027_feature_variations_truncation_and_unknown_format() {
    assert_eq!(
        FeatureVariations::new(&[0, 1, 0, 0, 0, 0, 0, 1]),
        Err(ReadError::Truncated {
            needed: 16,
            available: 8
        })
    );
    assert_eq!(
        Condition::new(&[0, 1, 0]),
        Err(ReadError::Truncated {
            needed: 8,
            available: 3
        })
    );
    assert_eq!(
        Condition::new(&[0, 3, 1, 0, 0]),
        Err(ReadError::Truncated {
            needed: 6,
            available: 5
        })
    );
    assert_eq!(Condition::new(&[0, 99]).unwrap().format(), 99);
}

#[test]
fn adr0027_feature_variations_validator_diagnoses_axis_and_unknown_format() {
    let mut data = vec![
        0, 1, 0, 0, 0, 0, 0, 1, // version and record count
        0, 0, 0, 16, 0, 0, 0, 0, // condition set and null substitution
        0, 2, 0, 0, 0, 10, 0, 0, 0, 18, // two condition offsets
        0, 1, 0, 2, 0xc0, 0, 0x40, 0, // axis index 2
        0, 99, // unknown condition format
    ];
    let mut diagnostics = Vec::new();
    feature_variations::validate(&data, Tag(*b"GSUB"), 14, Some(2), None, None, &mut |d| {
        diagnostics.push(d)
    });
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|d| d.kind
        == Kind::ConditionAxisOutOfRange { axis: 2, count: 2 }
        && d.severity == Severity::Error
        && d.offset == 42));
    assert!(
        diagnostics
            .iter()
            .any(|d| d.kind == Kind::UnknownConditionFormat { found: 99 }
                && d.severity == Severity::Warning
                && d.offset == 48)
    );

    data[11] = 255;
    diagnostics.clear();
    feature_variations::validate(&data, Tag(*b"GSUB"), 0, Some(2), None, None, &mut |d| {
        diagnostics.push(d)
    });
    assert!(
        diagnostics
            .iter()
            .any(|d| matches!(d.kind, Kind::OffsetOutOfBounds { .. }))
    );
}

#[test]
fn adr0027_feature_variations_substitution_feature_and_layout_accessor() {
    let fv = [
        0, 1, 0, 0, 0, 0, 0, 1, // FeatureVariations version and count
        0, 0, 0, 0, 0, 0, 0, 16, // null conditions, substitution at 16
        0, 1, 0, 0, 0, 1, // substitution version and count
        0, 7, 0, 0, 0, 12, // feature index 7, offset 12
        0, 0, 0, 2, 0, 3, 0, 4, // replacement Feature
    ];
    let mut gsub = vec![0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 14];
    gsub.extend_from_slice(&fv);
    let table = LayoutTable::new(&gsub).unwrap();
    let variations = table.feature_variations().unwrap().unwrap();
    let subs = variations
        .record(0)
        .unwrap()
        .substitutions()
        .unwrap()
        .unwrap();
    assert_eq!(subs.version(), 0x0001_0000);
    assert_eq!(subs.len(), 1);
    let record = subs.record(0).unwrap();
    assert_eq!(record.feature_index(), 7);
    assert_eq!(
        record.feature().unwrap().unwrap().get(..8),
        Some(&fv[28..36])
    );
}

#[test]
fn adr0046_i1_lookup_variation_views_and_version_boundary() {
    let mut data = vec![0, 1, 0, 1, 0, 0, 0, 0];
    data.extend_from_slice(&[0, 0, 0, 1]); // one lookup variation record
    data.extend_from_slice(&[0, 3, 0, 0, 0, 18]); // feature 3, table at 18
    data.extend_from_slice(&[0, 1, 0, 0, 0, 1, 0, 0, 0, 1]); // v1.0, flag 1, one condition
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 18]); // null condition, indices at 18
    data.extend_from_slice(&[0, 2, 0, 4, 0, 5]);
    let variations = FeatureVariations::new(&data).unwrap();
    assert_eq!(variations.lookup_record_count(), 1);
    let record = variations.lookup_record(0).unwrap();
    assert_eq!(record.feature_index(), 3);
    let table = record.feature_lookups().unwrap().unwrap();
    assert_eq!(table.flags(), 1);
    assert_eq!(table.len(), 1);
    let condition = table.record(0).unwrap();
    assert!(condition.condition().unwrap().is_none());
    let indices = condition.lookup_indices().unwrap().unwrap();
    assert_eq!(indices.len(), 2);
    assert_eq!(indices.get(0), Some(4));
    assert_eq!(indices.get(1), Some(5));
    assert_eq!(
        FeatureVariations::new(&[0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0])
            .unwrap()
            .lookup_record_count(),
        0
    );
    assert!(FeatureVariations::new(&data[..17]).is_err());

    data[3] = 0;
    assert_eq!(
        FeatureVariations::new(&data).unwrap().lookup_record_count(),
        0
    );
    data[3] = 1;
    data.pop();
    assert!(
        FeatureVariations::new(&data)
            .unwrap()
            .lookup_record(0)
            .unwrap()
            .feature_lookups()
            .unwrap()
            .unwrap()
            .record(0)
            .unwrap()
            .lookup_indices()
            .is_err()
    );
    data.truncate(11);
    assert!(FeatureVariations::new(&data).is_err());
}

#[test]
fn adr0046_i1_validator_reports_lookup_index_boundary() {
    let mut data = vec![0, 1, 0, 1, 0, 0, 0, 0];
    data.extend_from_slice(&[0, 0, 0, 1, 0, 3, 0, 0, 0, 18]);
    data.extend_from_slice(&[0, 1, 0, 0, 0, 1, 0, 0, 0, 1]);
    data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 18]);
    data.extend_from_slice(&[0, 2, 0, 4, 0, 5]);
    let mut found = Vec::new();
    feature_variations::validate(&data, Tag(*b"GSUB"), 0, None, Some(3), Some(5), &mut |d| {
        found.push(d)
    });
    assert!(
        found
            .iter()
            .any(|d| d.kind == Kind::FeatureIndexOutOfRange { index: 3, count: 3 })
    );
    assert!(
        found
            .iter()
            .any(|d| d.kind == Kind::LookupIndexOutOfRange { index: 5, count: 5 })
    );
    assert!(
        !found
            .iter()
            .any(|d| d.kind == Kind::LookupIndexOutOfRange { index: 4, count: 5 })
    );
}
