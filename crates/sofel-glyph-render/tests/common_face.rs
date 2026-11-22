#![allow(clippy::unwrap_used)]

use sofel_font::{FaceData, read::GlyphId};
use sofel_glyph_render::render_face;
use sofel_outline::Scratch;
use sofel_raster::Rasterizer;

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;
#[path = "../../sofel-cli/tests/support/pfr.rs"]
mod pfr_fixture;

#[test]
fn adr0100_i1_i2_fixed_face_renders_selected_pixels() {
    let data = FaceData::new(include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf")).unwrap();
    let opened = data.open(0).unwrap();
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let image = render_face(
        opened.as_face(),
        GlyphId(1),
        8,
        Some(0),
        &mut scratch,
        &mut raster,
    )
    .unwrap();
    assert_eq!((image.width, image.rows), (8, 8));
    assert_eq!(
        image.pixels.get(..8),
        Some(&[0, 0, 0, 255, 255, 0, 0, 0][..])
    );
}

#[test]
fn adr0100_i4_bitmap_only_pfr_chooses_strike_without_explicit_index() {
    let mut bytes = pfr_fixture::bitmap_only();
    *bytes.get_mut(131).unwrap() = 0;
    let data = FaceData::new(&bytes).unwrap();
    let opened = data.open(0).unwrap();
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let image = render_face(
        opened.as_face(),
        GlyphId(1),
        12,
        None,
        &mut scratch,
        &mut raster,
    )
    .unwrap();
    assert_eq!((image.width, image.rows), (3, 2));
    assert_eq!(image.pixels, [0, 255, 0, 255, 0, 255]);
    assert!(
        render_face(
            opened.as_face(),
            GlyphId(1),
            13,
            None,
            &mut scratch,
            &mut raster
        )
        .is_err()
    );
}

#[test]
fn adr0100_i1_i2_sfnt_fixed_strike_renders_embedded_pixels() {
    let image = build::Buf::new()
        .bytes(&[2, 3, 1, 2, 4])
        .bytes(&[0xe0, 0xa0])
        .done();
    let sizes = [build::SizeSpec {
        ppem_x: 16,
        ppem_y: 16,
        start_glyph: 1,
        end_glyph: 1,
        subtables: vec![(
            1,
            1,
            build::index_offsets(1, 1, 4, &[0, u32::try_from(image.len()).unwrap()]),
        )],
        ..build::SizeSpec::default()
    }];
    let bytes = build::glyph_font_with(
        &[Vec::new(), Vec::new()],
        false,
        false,
        vec![
            (b"EBLC", build::cblc(2, &sizes)),
            (
                b"EBDT",
                build::Buf::new().u16(2).u16(0).bytes(&image).done(),
            ),
        ],
    );
    let data = FaceData::new(&bytes).unwrap();
    let opened = data.open(0).unwrap();
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let rendered = render_face(
        opened.as_face(),
        GlyphId(1),
        16,
        Some(0),
        &mut scratch,
        &mut raster,
    )
    .unwrap();
    assert_eq!((rendered.width, rendered.rows), (3, 2));
    assert_eq!(rendered.pixels, [255, 255, 255, 255, 0, 255]);

    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let wrapper = format!(
        "%!PS-TrueTypeFont-1.0\n/FontType 42 def\n/Encoding StandardEncoding def\n/CharStrings 2 dict dup begin\n/.notdef 0 def\n/A 1 def\nend readonly def\n/sfnts [<{hex}>] def\n"
    );
    let data = FaceData::new(wrapper.as_bytes()).unwrap();
    let opened = data.open(0).unwrap();
    let rendered = render_face(
        opened.as_face(),
        GlyphId(1),
        16,
        Some(0),
        &mut scratch,
        &mut raster,
    )
    .unwrap();
    assert_eq!((rendered.width, rendered.rows), (3, 2));
    assert_eq!(rendered.pixels, [255, 255, 255, 255, 0, 255]);
}
