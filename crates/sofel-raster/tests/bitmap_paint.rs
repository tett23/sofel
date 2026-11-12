//! Bilinear interpolation uses premultiplied BGRA, including transparent neighbors (sofel ADR 0066).
use sofel_geom::{GlyphExtents, Transform};
use sofel_raster::BgraImage;

#[test]
fn i2_transformed_image_interpolates_four_neighbors_and_respects_clip() {
    let src = BgraImage {
        width: 2,
        rows: 2,
        pixels: vec![0, 0, 0, 0, 0, 0, 128, 128, 0, 255, 0, 255, 64, 0, 0, 64],
        ..Default::default()
    };
    let mut dst = BgraImage {
        width: 3,
        rows: 1,
        left: 4,
        top: 6,
        pixels: vec![0; 12],
    };
    let bounds = GlyphExtents {
        x_bearing: 0,
        y_bearing: 0,
        width: 2,
        height: -2,
    };
    let transform = Transform {
        xx: 2.,
        yy: 2.,
        xy: 2.,
        dx: 4.,
        dy: 6.,
        ..Transform::IDENTITY
    };
    // The first destination pixel maps to source (0.5, 0.5), the second to (1, 0.5).
    assert_eq!(
        dst.paint_bitmap(&src, bounds, transform, &[255, 0, 255]),
        Some(())
    );
    assert_eq!(dst.pixels, [16, 64, 32, 112, 0, 0, 0, 0, 0, 0, 0, 0]);
}
