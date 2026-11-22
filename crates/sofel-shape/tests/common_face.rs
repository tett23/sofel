#![allow(clippy::unwrap_used)]

use sofel_font::{Face, FaceData, read::GlyphId};
use sofel_shape::{Feature, SegmentProperties, UnicodeBuffer, shape_face};

#[path = "../../sofel-cli/tests/support/pfr.rs"]
#[allow(dead_code)]
mod pfr_fixture;

#[test]
fn adr0100_i5_shape_metrics_and_kern_setting() {
    let data = FaceData::new(include_bytes!("support/type1_triangle.pfa")).unwrap();
    let mut opened = data.open(0).unwrap();
    fn shape(face: Face<'_>) -> sofel_shape::GlyphBuffer {
        let mut input = UnicodeBuffer::new();
        input.push_utf8(b"AA", 0, None);
        input.number_clusters();
        let props = SegmentProperties::default().guess(&input, None);
        shape_face(face, 20, None, props, &[], input).unwrap()
    }
    let before = shape(opened.as_face());
    let original = opened.as_face().h_advance(GlyphId(1), 20, None).unwrap();
    assert_eq!(
        before
            .glyph_positions()
            .iter()
            .map(|p| p.x_advance)
            .collect::<Vec<_>>(),
        [original, original]
    );
    opened
        .attach_afm(
            b"StartFontMetrics 4.1\nStartKernPairs 1\nKPX A A -80\nEndKernPairs\nEndFontMetrics\n",
        )
        .unwrap();
    let after = shape(opened.as_face());
    assert_eq!(
        after
            .glyph_positions()
            .iter()
            .map(|p| p.x_advance)
            .sum::<i32>(),
        original * 2 - 102
    );
    let mut input = UnicodeBuffer::new();
    input.push_utf8(b"AA", 0, None);
    input.number_clusters();
    let props = SegmentProperties::default().guess(&input, None);
    let disabled = shape_face(
        opened.as_face(),
        20,
        None,
        props,
        &[Feature::parse(b"kern=0").unwrap()],
        input,
    )
    .unwrap();
    assert_eq!(
        disabled
            .glyph_positions()
            .iter()
            .map(|p| p.x_advance)
            .collect::<Vec<_>>(),
        [original, original]
    );
}

#[test]
fn adr0100_i5_pfr_nonzero_pair_shapes_with_kern_setting() {
    let bytes = pfr_fixture::with_kerning();
    let data = FaceData::new(&bytes).unwrap();
    let opened = data.open(0).unwrap();
    let face = opened.as_face();
    let advance = face.h_advance(GlyphId(1), 20, None).unwrap();
    let (kern, _) = face.kerning(GlyphId(1), GlyphId(1), 20, None).unwrap();
    assert_ne!(kern, 0);
    let shape = |features: &[Feature]| {
        let mut input = UnicodeBuffer::new();
        input.push_utf8(b"AA", 0, None);
        input.number_clusters();
        let props = SegmentProperties::default().guess(&input, None);
        shape_face(face, 20, None, props, features, input).unwrap()
    };
    let with_kern = shape(&[]);
    assert_eq!(with_kern.glyph_positions().len(), 2);
    assert_eq!(
        with_kern
            .glyph_positions()
            .iter()
            .map(|p| p.x_advance)
            .sum::<i32>(),
        advance * 2 + kern
    );
    let disabled = shape(&[Feature::parse(b"kern=0").unwrap()]);
    assert_eq!(
        disabled
            .glyph_positions()
            .iter()
            .map(|p| p.x_advance)
            .sum::<i32>(),
        advance * 2
    );
}
