#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_conformance::expect::{Problem, judge, parse};

#[test]
fn parses_identifiers_and_comments() {
    let list = parse("# header\n\na#0:head  # why\n  b#0:hmtx\n#c#0:maxp\n");
    assert_eq!(
        list.into_iter().collect::<Vec<_>>(),
        vec!["a#0:head", "b#0:hmtx"]
    );
}

#[test]
fn unlisted_failures_and_listed_passes_both_fail() {
    let expected = parse("listed-fails\nlisted-passes\nnever-ran\n");
    let results = [
        ("passes", true),
        ("fails", false),
        ("listed-fails", false),
        ("listed-passes", true),
    ];
    let problems = judge(results, &expected);
    assert_eq!(
        problems,
        vec![
            Problem::Regressed("fails".into()),
            Problem::Fixed("listed-passes".into()),
            Problem::Unknown("never-ran".into()),
        ]
    );
}

#[test]
fn nothing_to_report() {
    let expected = parse("x\n");
    assert!(judge([("x", false), ("y", true)], &expected).is_empty());
}
