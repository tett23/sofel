#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use sofel_svg::parse_glyph;

#[test]
fn i2_svg_shared_targets_styles_and_forward_references() {
    let svg = br##"<svg viewBox="0 0 40 20"><g fill="red"><g id="glyph1">
        <use href="#later"/><rect width="2" height="2" style="fill:blue;fill-opacity:0.5"/>
        <rect width="1" height="1" fill="context-fill"/></g></g>
        <defs><rect id="later" width="1" height="1"/></defs>
        <rect id="glyph2" width="2" height="2"/></svg>"##;
    let glyph = parse_glyph(svg, 1, 80, [0, 255, 0, 255]).unwrap();
    assert_eq!(glyph.scene.shapes.len(), 3);
    assert_eq!(
        glyph.scene.shapes[0].transform,
        [2.0, 0.0, 0.0, 2.0, 0.0, 20.0]
    );
    for (shape, expected) in
        glyph
            .scene
            .shapes
            .iter()
            .zip([[255, 0, 0, 255], [0, 0, 255, 128], [0, 255, 0, 255]])
    {
        assert_eq!(glyph.scene.sample(shape, 1.0, -21.0, 1.0), expected);
    }
}

#[test]
fn i2_svg_use_cycles_and_script_subtrees_do_not_render() {
    let svg = br##"<svg><g id="glyph1"><use href="#cycle"/><script><rect width="9" height="9"/></script><rect width="1" height="1" fill="red"/></g><g id="cycle"><use href="#cycle"/></g></svg>"##;
    assert_eq!(
        parse_glyph(svg, 1, 1000, [0; 4])
            .unwrap()
            .scene
            .shapes
            .len(),
        1
    );
}

#[test]
fn i2_selected_glyph_keeps_ancestor_visibility_and_clips() {
    for element in ["script", "metadata", "foreignObject"] {
        let document = format!(
            r#"<svg><{element}><rect id="glyph1" width="3" height="2"/></{element}></svg>"#
        );
        assert!(
            parse_glyph(document.as_bytes(), 1, 1000, [0; 4])
                .unwrap()
                .scene
                .shapes
                .is_empty()
        );
    }
    let hidden = br#"<svg><g display="none"><rect id="glyph1" width="3" height="2"/></g></svg>"#;
    assert!(
        parse_glyph(hidden, 1, 1000, [0; 4])
            .unwrap()
            .scene
            .shapes
            .is_empty()
    );
    let clipped=br##"<svg><defs><clipPath id="window"><rect width="1" height="2"/></clipPath></defs><g clip-path="url(#window)"><rect id="glyph1" width="3" height="2"/></g></svg>"##;
    let glyph = parse_glyph(clipped, 1, 1000, [0; 4]).unwrap();
    assert_eq!(glyph.clips[0].len(), 1);
}

#[test]
fn i2_repeated_use_is_bounded_even_without_a_cycle() {
    let mut svg = String::from(
        r##"<svg><g id="glyph1"><use href="#n20"/></g><defs><rect id="n0" width="1" height="1"/>"##,
    );
    for i in 1..=20 {
        svg.push_str(&format!(
            r##"<g id="n{i}"><use href="#n{}"/><use href="#n{}"/></g>"##,
            i - 1,
            i - 1
        ));
    }
    svg.push_str("</defs></svg>");
    assert!(parse_glyph(svg.as_bytes(), 1, 1000, [0; 4]).is_err());
}
