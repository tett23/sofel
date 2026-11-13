use sofel_font::{Font, HbScale, Type1Face, Type1Source};
use sofel_shape::{
    Feature, Scale, SegmentProperties, ShapePlan, SubFont, UnicodeBuffer, shape_with_subfont,
};

#[test]
fn adr0065_i1_attached_afm_changes_advances_in_normal_shaping()
-> Result<(), Box<dyn std::error::Error>> {
    let pfa = include_bytes!("support/type1_triangle.pfa");
    let source = Type1Source::parse_pfa(pfa, 4096)?;
    let mut face = Type1Face::new(&source)?;
    let font = Font::for_shaping(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 0)?;
    let shape = |face: &Type1Face<'_>, features: &[Feature]| {
        let mut b = UnicodeBuffer::new();
        b.push_utf8(b"AA", 0, None);
        b.number_clusters();
        let instance = font.default_instance();
        let plan = ShapePlan::new(
            instance,
            SegmentProperties::default().guess(&b, None),
            features,
        );
        let subfont =
            SubFont::from_type1(Scale::new(HbScale::for_font(&font, 1000, 1000), 0, 0), face);
        shape_with_subfont(instance, &subfont, &plan, b)
            .glyph_positions()
            .iter()
            .map(|p| (p.x_advance, p.x_offset))
            .collect::<Vec<_>>()
    };
    let before = shape(&face, &[]);
    face.attach_afm(
        b"StartFontMetrics 4.1\nStartKernPairs 1\nKPX A A -81\nEndKernPairs\nEndFontMetrics\n",
    )?;
    let width = face.glyph_metrics(1).ok_or("missing width")?.advance_x;
    assert_eq!(before, [(width, 0), (width, 0)]);
    assert_eq!(shape(&face, &[]), [(width - 41, 0), (width - 40, -40)]);
    assert_eq!(
        shape(
            &face,
            &[Feature::parse(b"kern=0").ok_or("invalid feature")?]
        ),
        before
    );
    Ok(())
}
