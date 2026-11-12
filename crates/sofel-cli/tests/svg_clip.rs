//! sofel ADR 0074 I2: clipping and transformed coverage in paint order.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use sofel_cli::color_raster::render_glyph_scene;
use sofel_raster::{BgraImage, Rasterizer};
use sofel_svg::{ClipElement, ClipPath, GlyphScene, ShapeKind, parse_shapes};

fn bitmap(paths: Vec<Vec<(f32, f32)>>) -> BgraImage {
    let data = br##"<svg><defs><linearGradient id="red"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#ff0000"/></linearGradient></defs><g id="glyph1"><rect x="0" y="0" width="4" height="1" fill="url(#red)"/></g></svg>"##;
    let scene = parse_shapes(data, 1).unwrap();
    let paths: Vec<ClipPath> = paths
        .into_iter()
        .map(|children| {
            children
                .into_iter()
                .map(|(x, width)| {
                    let mut shape = scene.shapes.first().unwrap().clone();
                    shape.kind = ShapeKind::Rect {
                        x: 0.,
                        y: 0.,
                        width,
                        height: 1.,
                    };
                    shape.transform = [1., 0., 0., 1., x, 0.];
                    ClipElement {
                        shape,
                        even_odd: false,
                    }
                })
                .collect()
        })
        .collect();
    let mut image = BgraImage::default();
    assert!(render_glyph_scene(
        &GlyphScene {
            scene,
            clips: vec![paths]
        },
        16,
        16,
        &mut Rasterizer::new(),
        &mut image
    ));
    image
}

#[test]
fn i2_svg_clip_unions_siblings_and_intersects_ancestors() {
    let image = bitmap(vec![vec![(0.5, 0.5), (1., 1.), (3., 1.)], vec![(0., 2.)]]);
    let alpha: Vec<_> = image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|[_, _, _, alpha]| *alpha)
        .collect();
    assert_eq!(alpha, [128, 255, 0, 0]);
}

#[test]
fn i2_svg_clip_unions_adjacent_subpixel_shapes_before_resolving_coverage() {
    let image = bitmap(vec![vec![(1., 0.5), (1.5, 0.5)]]);
    let alpha: Vec<_> = image
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|[_, _, _, alpha]| *alpha)
        .collect();
    assert_eq!(alpha, [0, 255, 0, 0]);
}

#[test]
fn i2_svg_empty_clip_hides_the_entire_shape() {
    let image = bitmap(vec![vec![]]);
    assert!(image.pixels.iter().all(|value| *value == 0));
}
