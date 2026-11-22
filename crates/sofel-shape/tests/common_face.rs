#![allow(clippy::unwrap_used)]

use sofel_font::{Face, FaceData, read::GlyphId};
use sofel_shape::{Feature, SegmentProperties, UnicodeBuffer, shape_face};

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
