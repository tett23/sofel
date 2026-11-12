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
