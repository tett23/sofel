//! The only dfont and sanitize exceptions admitted by ADR 0044.
use sofel_conformance::expect::parse_reasons;

#[derive(Clone, Copy)]
enum Suite {
    Shape,
    Layout,
    Random,
}

#[test]
fn i1_only_the_reviewed_inputs_may_use_the_two_categories() {
    for (suite, list) in [
        (
            Suite::Shape,
            include_str!("../../../conformance/expected-failures.txt"),
        ),
        (
            Suite::Layout,
            include_str!("../../../conformance/expected-failures-layout-map.txt"),
        ),
        (
            Suite::Random,
            include_str!("../../../conformance/expected-failures-shape-random.txt"),
        ),
    ] {
        for (id, reason) in parse_reasons(list) {
            if matches!(reason.as_str(), "dfont" | "sanitize") {
                assert!(permitted(suite, &id, &reason), "{id}: {reason}");
            }
        }
    }
}

#[test]
fn i1_changing_the_line_face_script_suite_or_reason_invalidates_the_exception() {
    assert!(permitted(
        Suite::Shape,
        "in-house/collections.tests:1",
        "dfont"
    ));
    assert!(!permitted(
        Suite::Shape,
        "in-house/collections.tests:2",
        "dfont"
    ));
    assert!(!permitted(
        Suite::Shape,
        "in-house/collections.tests:1",
        "sanitize"
    ));
    let face = "harfbuzz/test/shape/data/in-house/fonts/HarfBust.ttf#0";
    assert!(permitted(Suite::Layout, face, "sanitize"));
    assert!(!permitted(
        Suite::Layout,
        &face.replace("#0", "#1"),
        "sanitize"
    ));
    assert!(permitted(
        Suite::Random,
        &format!("{face}:Latn"),
        "sanitize"
    ));
    assert!(!permitted(
        Suite::Random,
        &format!("{face}:Arab"),
        "sanitize"
    ));
    assert!(!permitted(Suite::Shape, face, "sanitize"));
}

fn permitted(suite: Suite, id: &str, category: &str) -> bool {
    const FONTS: [(&str, &str); 3] = [
        ("HarfBust.ttf#0", "Latn"),
        ("36f8e7f3b4f54efaa6bfd8225893022dc0041915.ttf#0", "Arab"),
        ("ecc344a5fa31c4d69350231bb3b96cc693646089.ttf#0", "Arab"),
    ];
    if matches!(suite, Suite::Shape) {
        return matches!(
            (id, category),
            ("in-house/collections.tests:1", "dfont") | ("in-house/harfbust.tests:4", "sanitize")
        );
    }
    let Some(face) = id.strip_prefix("harfbuzz/test/shape/data/in-house/fonts/") else {
        return false;
    };
    category == "sanitize"
        && FONTS.iter().any(|&(font, script)| match suite {
            Suite::Layout => face == font,
            Suite::Random => face.split_once(':') == Some((font, script)),
            Suite::Shape => false,
        })
}
