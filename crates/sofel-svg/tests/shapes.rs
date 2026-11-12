#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_svg::{GradientKind, ShapeKind, parse_shapes};

const DOCUMENT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg"><g id="glyph19" transform="matrix(120 0 0 120 37.5 -950)"><defs><linearGradient id="lg1"><stop offset="5%" stop-color="gold" stop-opacity="0.5"/><stop offset="95%" stop-color="red"/></linearGradient><linearGradient id="lg2" gradientTransform="rotate(90)"><stop offset="0.05" stop-color="gold"/><stop offset="0.95" stop-color="red" stop-opacity="0.5"/></linearGradient><radialGradient id="rg1"><stop offset="0.0" stop-color="gold"/><stop offset="0.85" stop-color="red" stop-opacity="0.5"/><stop offset="1.0" stop-color="white" stop-opacity="0.7"/></radialGradient></defs><rect x="1" y="4.5" width="8" height="1" fill="url(#lg1)"/><rect x="4.5" y="1" width="1" height="8" fill="url(#lg2)"/><circle cx="5" cy="5" r="2" fill="url(#rg1)"/></g></svg>"##;

#[test]
fn adr0072_i3_svg_rect_circle_and_gradients() {
    let scene = parse_shapes(DOCUMENT.as_bytes(), 19).unwrap();
    assert_eq!(scene.shapes.len(), 3);
    assert_eq!(scene.gradients.len(), 3);
    assert!(matches!(
        scene.shapes[0].kind,
        ShapeKind::Rect {
            x: 1.0,
            y: 4.5,
            width: 8.0,
            height: 1.0
        }
    ));
    assert!(matches!(
        scene.shapes[2].kind,
        ShapeKind::Circle {
            cx: 5.0,
            cy: 5.0,
            r: 2.0
        }
    ));
    assert_eq!(
        scene.shapes[0].transform,
        [120.0, 0.0, 0.0, 120.0, 37.5, -950.0]
    );
    assert_eq!(scene.shapes[0].gradient, 0);
    assert_eq!(scene.shapes[1].gradient, 1);
    assert_eq!(scene.shapes[2].gradient, 2);
    assert_eq!(scene.gradients[0].stops[0].offset, 0.05);
    assert_eq!(scene.gradients[0].stops[0].rgba, [255, 215, 0, 128]);
    assert_eq!(
        scene.gradients[1].transform,
        [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]
    );
    assert!(matches!(
        scene.gradients[2].kind,
        GradientKind::Radial { .. }
    ));
    assert_eq!(scene.gradients[2].stops[2].rgba, [255, 255, 255, 179]);
}

#[test]
fn adr0072_i3_only_selected_glyph_group() {
    let scene = parse_shapes(DOCUMENT.as_bytes(), 26).unwrap();
    assert!(scene.shapes.is_empty());
}
