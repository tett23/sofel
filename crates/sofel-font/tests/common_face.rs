#![allow(clippy::unwrap_used)]

use sofel_font::{FaceData, read::GlyphId};

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

const BDF: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf");
const WIDTH_500: &[u8] = &[
    1, 0, 4, 4, 0, 1, 1, 1, 2, b'A', 0, 1, 1, 1, 3, 160, 17, 0, 0, 0, 0, 0, 1, 1, 1, 6, 248, 136,
    139, 22, 14,
];

#[test]
fn adr0100_i1_i2_common_face_bitmap_mapping_metrics_and_pixels() {
    let data = FaceData::new(BDF).unwrap();
    assert_eq!(data.face_count().unwrap(), 1);
    let opened = data.open(0).unwrap();
    let face = opened.as_face();
    assert_eq!(face.glyph_count(), 2);
    assert_eq!(face.nominal_glyph(65), Some(GlyphId(1)));
    assert_eq!(face.strike_count(), 1);
    assert_eq!(face.strike(0), Some((512, 512)));
    assert_eq!(face.h_advance(GlyphId(1), 8, None), Ok(512));
    assert_eq!(face.h_advance(GlyphId(1), 8, Some(0)), Ok(512));
    assert!(face.h_advance(GlyphId(1), 7, None).is_err());
    assert!(face.h_advance(GlyphId(1), 8, Some(1)).is_err());
    assert_eq!(
        face.bitmap(GlyphId(1)).and_then(|g| g.bitmap.first()),
        Some(&0x18)
    );
    assert_eq!(face.kerning(GlyphId(1), GlyphId(1), 8, None), Ok((0, 0)));
}

#[test]
fn adr0100_i1_i2_common_face_bare_cff_mapping_and_metrics() {
    let data = FaceData::new(WIDTH_500).unwrap();
    let opened = data.open(0).unwrap();
    let face = opened.as_face();
    assert_eq!(face.glyph_count(), 1);
    assert_eq!(face.h_advance(GlyphId(0), 20, None), Ok(640));
    assert!(face.bitmap(GlyphId(0)).is_none());
}

#[test]
fn adr0100_i2_sfnt_kern_pair_and_missing_pair() {
    let mut tables = build::sample_tables_with_glyf();
    let kern = build::Buf::new()
        .u16(0)
        .u16(1)
        .u16(0)
        .u16(20)
        .u16(1)
        .u16(1)
        .u16(6)
        .u16(0)
        .u16(0)
        .u16(1)
        .u16(2)
        .i16(-80)
        .done();
    tables.push((b"kern", kern));
    tables.sort_by_key(|(tag, _)| **tag);
    let data = FaceData::new(&build::sfnt(&tables)).unwrap();
    let opened = data.open(0).unwrap();
    let face = opened.as_face();
    assert_eq!(
        face.kerning(GlyphId(1), GlyphId(2), 20, None),
        Ok((-102, 0))
    );
    assert_eq!(face.kerning(GlyphId(2), GlyphId(1), 20, None), Ok((0, 0)));
}

#[test]
fn adr0100_i3_metrics_attachment_changes_only_opened_type1() {
    let data = FaceData::new(include_bytes!(
        "../../sofel-shape/tests/support/type1_triangle.pfa"
    ))
    .unwrap();
    let mut opened = data.open(0).unwrap();
    let original = opened.as_face().kerning(GlyphId(1), GlyphId(1), 20, None);
    assert_eq!(original, Ok((0, 0)));
    opened
        .attach_afm(
            b"StartFontMetrics 4.1\nStartKernPairs 1\nKPX A A -80\nEndKernPairs\nEndFontMetrics\n",
        )
        .unwrap();
    assert_eq!(
        opened.as_face().kerning(GlyphId(1), GlyphId(1), 20, None),
        Ok((-102, 0))
    );
    let fresh = data.open(0).unwrap();
    assert_eq!(
        fresh.as_face().kerning(GlyphId(1), GlyphId(1), 20, None),
        original
    );
}

#[test]
fn adr0100_i4_size_upem_and_conversion_boundaries() {
    let data = FaceData::new(WIDTH_500).unwrap();
    let opened = data.open(0).unwrap();
    let face = opened.as_face();
    assert!(face.h_advance(GlyphId(0), 0, None).is_err());
    assert!(
        face.h_advance(GlyphId(face.glyph_count()), 20, None)
            .is_err()
    );
    assert!(face.h_advance(GlyphId(0), u16::MAX, None).is_ok());

    for (upem, ppem) in [(0u16, 20u16), (1u16, u16::MAX)] {
        let mut tables = build::sample_tables_with_glyf();
        let head = tables.iter_mut().find(|(tag, _)| *tag == b"head").unwrap();
        head.1
            .get_mut(18..20)
            .unwrap()
            .copy_from_slice(&upem.to_be_bytes());
        let bytes = build::sfnt(&tables);
        let data = FaceData::new(&bytes).unwrap();
        let opened = data.open(0).unwrap();
        assert!(opened.as_face().h_advance(GlyphId(0), ppem, None).is_err());
    }
}
