#![allow(clippy::unwrap_used)]
use sofel_font::Font;
use sofel_font::read::GlyphId;
const BDF: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf");
const PCF: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono.pcf");
const FNT: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono.fnt");
const FON: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/two.fon");

#[test]
fn i1_font_driver_all_fixed_faces_match_freetype() {
    for (bytes, index, faces, strike, code, first) in [
        (BDF, 0, 1, 512, 65, 0x18),
        (PCF, 0, 1, 510, 65, 0x18),
        (FNT, 0, 1, 512, 65, 0x18),
        (FON, 0, 2, 512, 65, 0x18),
        (FON, 1, 2, 512, 66, 0xff),
    ] {
        let face = Font::from_bitmap(bytes, index).unwrap();
        assert_eq!(face.face_count(), faces);
        assert_eq!(face.glyph_count(), 2);
        assert_eq!(face.ppem_26_6(), (strike, strike));
        assert_eq!(face.nominal_glyph(code), Some(GlyphId(1)));
        assert_eq!(face.nominal_glyph(0x10000), None);
        let glyph = face.glyph(GlyphId(1)).unwrap();
        assert_eq!(
            (
                glyph.advance,
                glyph.left,
                glyph.top,
                glyph.width,
                glyph.height
            ),
            (8, 0, 8, 8, 8)
        );
        assert_eq!(glyph.bitmap.first(), Some(&first));
        assert!(face.glyph(GlyphId(2)).is_none());
        assert!(Font::from_bitmap(bytes, faces).is_err());
    }
}
#[test]
fn i2_decode_bounds_fixed_face_rejects_truncated_inputs() {
    for data in [BDF, PCF, FNT, FON] {
        assert!(Font::from_bitmap(data.get(..16).unwrap(), 0).is_err());
    }
}

/// A PE32 resource section contains type, name, then language directories.
/// Both language entries refer to the same embedded FNT.
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn pe_font() -> Vec<u8> {
    let mut data = vec![0; 640];
    data[..2].copy_from_slice(b"MZ");
    data[64..68].copy_from_slice(b"PE\0\0");
    let size = u32::try_from(FNT.len()).unwrap();
    for (offset, width, value) in [
        (60, 4, 64u32),
        (68, 2, 0x14c),
        (70, 2, 1),
        (84, 2, 224),
        (88, 2, 267),
        (180, 4, 16),
        (200, 4, 4096),
        (204, 4, 128 + size),
        (324, 4, 4096),
        (328, 4, 128 + size),
        (332, 4, 512),
        (526, 2, 1),
        (528, 4, 8),
        (532, 4, 0x80000018),
        (550, 2, 1),
        (552, 4, 1),
        (556, 4, 0x80000030),
        (574, 2, 2),
        (576, 4, 1033),
        (580, 4, 80),
        (584, 4, 1041),
        (588, 4, 96),
        (592, 4, 4224),
        (596, 4, size),
        (608, 4, 4224),
        (612, 4, size),
    ] {
        data[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
    }
    data.extend_from_slice(FNT);
    data
}

#[test]
fn i1_font_driver_pe_fon_indexes_resource_languages() {
    let data = pe_font();
    for i in 0..2 {
        let face = Font::from_bitmap(&data, i).unwrap();
        assert_eq!(face.face_count(), 2);
        assert_eq!(face.nominal_glyph(65), Some(GlyphId(1)));
        let glyph = face.glyph(GlyphId(1)).unwrap();
        assert_eq!((glyph.advance, glyph.width, glyph.height), (8, 8, 8));
        assert_eq!(glyph.bitmap.first(), Some(&0x18));
    }
    assert!(Font::from_bitmap(&data, 2).is_err());
}
#[test]
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
fn i2_decode_bounds_pe_rejects_invalid_resource_addresses() {
    for (at, value) in [
        (332, u32::MAX),
        (532, 0x8000ffff),
        (556, 48),
        (580, 0x80000050),
        (592, 4095),
        (596, u32::MAX),
    ] {
        let mut data = pe_font();
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(Font::from_bitmap(&data, 0).is_err(), "offset {at}");
    }
}
