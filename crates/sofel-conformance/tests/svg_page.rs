//! sofel ADR 0074, output page observations.
use sofel_conformance::svg_page::{compare, observe};

#[test]
fn i2_svg_observations_preserve_page_geometry_and_paint_order() {
    let reference = r#"<svg width="12" height="15" viewBox="0 0 12 15"><g transform="rotate(10) scale(2)"><path fill="red"/><path fill="blue"/></g></svg>"#;
    let reordered_attrs = r#"<svg viewBox="0 0 12 15" height="15" width="12">
      <g transform="rotate(10) scale(2)"><path fill="red"></path><path fill="blue"/></g>
    </svg>"#;
    assert!(compare(reference, reordered_attrs, "scene-tree").is_ok());
    for (old, new) in [
        ("rotate(10) scale(2)", "scale(2) rotate(10)"),
        (
            r#"fill="red"/><path fill="blue""#,
            r#"fill="blue"/><path fill="red""#,
        ),
    ] {
        assert!(compare(reference, &reference.replace(old, new), "scene-tree").is_err());
    }
    assert!(compare(reference, &reference.replace("12", "13"), "page-size").is_err());
}

#[test]
fn i2_svg_observations_positioned_glyphs_match_harfbuzz() {
    for case in ["empty", "single", "translucent-overlap", "clipped-image"] {
        for name in ["page-size", "scene-tree"] {
            assert_eq!(observe(case, name), Ok(()), "{case}/{name}");
        }
    }
}

#[test]
fn i2_svg_observations_detect_swapped_overlap_and_changed_clip() {
    let overlap = include_str!("../../../conformance/svg-page/translucent-overlap.svg");
    let glyphs: Vec<_> = overlap
        .lines()
        .filter(|line| line.starts_with("<path "))
        .collect();
    let [first, second] = glyphs.as_slice() else {
        unreachable!("two reference glyphs")
    };
    let swapped = overlap.replace(&format!("{first}\n{second}"), &format!("{second}\n{first}"));
    assert!(compare(overlap, &swapped, "scene-tree").is_err());
    let image = include_str!("../../../conformance/svg-page/clipped-image.svg");
    for changed in [
        image.replace("matrix(2,0,0,3,1,2)", "matrix(3,0,0,2,1,2)"),
        image.replace(r#"<rect x="2""#, r#"<rect x="3""#),
        image.replace("iVBORw", "jVBORw"),
    ] {
        assert!(compare(image, &changed, "scene-tree").is_err());
    }
}
