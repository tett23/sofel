//! sofel ADR 0065 I1 and I2 against FreeType 2.15.0 Windows FNT/FON.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use sofel_bitmapfont::winfnt::WinFont;

const FNT: &[u8] = include_bytes!("data/mono.fnt");
const FNT3: &[u8] = include_bytes!("data/mono3.fnt");
const FON: &[u8] = include_bytes!("data/two.fon");

#[test]
fn adr0065_i1_fnt_and_fon_faces_match_freetype() {
    let fnt = WinFont::parse(FNT, 0).unwrap();
    assert_eq!(fnt.num_faces(), 1);
    assert_eq!(fnt.num_glyphs(), 2);
    assert_eq!(fnt.ppem_26_6(), (512, 512));
    assert_eq!(fnt.nominal_glyph(65), Some(1));
    assert_eq!(fnt.nominal_glyph(66), None);
    assert_eq!(fnt.glyph(0).unwrap().bitmap, fnt.glyph(1).unwrap().bitmap);
    assert_eq!(
        fnt.glyph(1).unwrap().bitmap,
        [0x18, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0]
    );
    let fnt3 = WinFont::parse(FNT3, 0).unwrap();
    assert_eq!(fnt3.ppem_26_6(), (512, 512));
    assert_eq!(fnt3.glyph(1).unwrap().bitmap, fnt.glyph(1).unwrap().bitmap);
    let mut high_dpi = FNT.to_vec();
    high_dpi[68..74].copy_from_slice(&[255; 6]);
    assert_eq!(
        WinFont::parse(&high_dpi, 0).unwrap().ppem_26_6(),
        (896, 512)
    );
    let first = WinFont::parse(FON, 0).unwrap();
    let second = WinFont::parse(FON, 1).unwrap();
    assert_eq!(first.num_faces(), 2);
    assert_eq!(second.num_faces(), 2);
    assert_eq!(first.nominal_glyph(65), Some(1));
    assert_eq!(second.nominal_glyph(66), Some(1));
    assert_eq!(second.glyph(1).unwrap().bitmap, [0xff; 8]);
}

#[test]
fn adr0065_i2_fnt_and_fon_bounds() {
    assert!(WinFont::parse(FNT, 1).is_err());
    assert!(WinFont::parse(FON, 2).is_err());
    assert!(WinFont::parse(&FNT[..118], 0).is_err());
    assert!(WinFont::parse(&FON[..300], 1).is_err());
    let mut bad = FNT.to_vec();
    bad[120] = 0xff;
    bad[121] = 0xff;
    assert!(WinFont::parse(&bad, 0).is_err());
    bad = FON.to_vec();
    bad[0x80] = 0xff; // invalid NE resource alignment shift
    assert!(WinFont::parse(&bad, 0).is_err());
    for end in 0..FNT.len() {
        assert!(WinFont::parse(&FNT[..end], 0).is_err(), "FNT prefix {end}");
    }
    for end in 0..FON.len() {
        assert!(WinFont::parse(&FON[..end], 1).is_err(), "FON prefix {end}");
    }
}

#[test]
fn adr0065_i2_ne_resource_alignment_offset() {
    let mut bytes = FON.to_vec();
    bytes[0x80] = 17;
    assert_eq!(WinFont::parse_with_offset(&bytes, 0).unwrap_err(), 0x80);
}
