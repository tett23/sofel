#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_conformance::compare::compare;
use sofel_conformance::dump::{Charmap, Face};

fn cm(
    platform: u16,
    encoding: u16,
    maps: &[(u32, u32)],
    readable: bool,
    validator_error: bool,
) -> Charmap {
    Charmap {
        platform,
        encoding,
        format: 4,
        maps: maps.to_vec(),
        readable,
        validator_error,
    }
}

fn face(charmaps: Vec<Charmap>) -> Face {
    Face {
        index: 0,
        num_glyphs: Some(10),
        charmaps,
        ..Face::default()
    }
}

fn results(ft: Face, ours: Face) -> Vec<(String, bool)> {
    compare("f", &[ft], &[ours])
        .into_iter()
        .map(|i| (i.id, i.ok))
        .collect()
}

#[test]
fn dropped_subtables_need_a_validator_error() {
    let ft = face(vec![]);
    let ours = face(vec![cm(3, 1, &[], false, true)]);
    assert!(results(ft.clone(), ours).contains(&("f#0:cmap-validate:3:1".into(), true)));
    let ours = face(vec![cm(3, 1, &[], true, false)]);
    assert!(results(ft, ours).contains(&("f#0:cmap-validate:3:1".into(), false)));
}

#[test]
fn mappings_beyond_num_glyphs_are_left_out() {
    let ft = face(vec![cm(3, 1, &[(65, 1)], true, false)]);
    let ours = face(vec![cm(3, 1, &[(65, 1), (66, 10)], true, false)]);
    assert!(results(ft, ours).contains(&("f#0:cmap:3:1".into(), true)));
}

#[test]
fn duplicate_records_are_numbered() {
    let ft = face(vec![
        cm(3, 1, &[(65, 1)], true, false),
        cm(3, 1, &[(66, 2)], true, false),
    ]);
    let ours = face(vec![
        cm(3, 1, &[(65, 1)], true, false),
        cm(3, 1, &[(66, 3)], true, false),
    ]);
    let r = results(ft, ours);
    assert!(r.contains(&("f#0:cmap:3:1".into(), true)));
    assert!(r.contains(&("f#0:cmap:3:1:1".into(), false)));
}

#[test]
fn synthesized_adobe_charmaps_are_skipped() {
    let ft = face(vec![cm(7, 0, &[(65, 1)], true, false)]);
    let r = results(ft, face(vec![]));
    assert!(!r.iter().any(|(id, _)| id.contains("cmap")), "{r:?}");
}

#[test]
fn maxp_gets_freetypes_adjustments() {
    let kv = |k: &str, v: &str| (k.to_string(), v.to_string());
    let mut ft = face(vec![]);
    ft.maxp = vec![
        kv("max_function_defs", "64"),
        kv("max_twilight_points", "65531"),
    ];
    let mut ours = face(vec![]);
    ours.maxp = vec![
        kv("max_function_defs", "3"),
        kv("max_twilight_points", "65535"),
    ];
    assert!(results(ft, ours).contains(&("f#0:maxp".into(), true)));
}

#[test]
fn open_is_checked_one_way() {
    let ft_fails = Face {
        error: Some("2".into()),
        ..face(vec![])
    };
    assert!(results(ft_fails, face(vec![])).is_empty());
    let ours_fails = Face {
        error: Some("bad".into()),
        ..face(vec![])
    };
    assert_eq!(
        results(face(vec![]), ours_fails),
        vec![("f#0:open".into(), false)]
    );
}
