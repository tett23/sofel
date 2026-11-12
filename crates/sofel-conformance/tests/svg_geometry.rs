//! Geometry probes for sofel ADR 0074.
use sofel_conformance::svg_geometry::reference_bounds;

#[test]
fn i2_geometry_reference_bounds_use_transformed_ink() {
    for (case, bounds) in [
        ("path-line", (4, 4, 11, 11)),
        ("path-curve", (4, 3, 15, 11)),
        ("rect", (4, 4, 15, 11)),
        ("circle", (4, 4, 11, 11)),
        ("nested-transform", (8, 4, 15, 7)),
    ] {
        assert_eq!(reference_bounds(case), Ok(bounds), "{case}");
    }
    assert!(reference_bounds("unknown").is_err());
}
