//! sofel ADR 0065 I1 and I2 against FreeType 2.15.0 PCF behavior.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
use sofel_bitmapfont::pcf::PcfFont;

const ORDER: &[u8] = include_bytes!("data/order_lsb.pcf");
const MONO: &[u8] = include_bytes!("data/mono.pcf");

#[test]
fn adr0065_i1_pcf_encoding_order_and_default() {
    let font = PcfFont::parse(ORDER).unwrap();
    assert_eq!(font.num_glyphs(), 3);
    assert_eq!(font.nominal_glyph(65), Some(2));
    assert_eq!(font.nominal_glyph(66), Some(1));
    assert_eq!(font.glyph(0).unwrap().bitmap[0], 0xff);
    assert_eq!(font.glyph(1).unwrap().bitmap[0], 0xff);
    assert_eq!(font.glyph(2).unwrap().bitmap[0], 0x18);
    assert_eq!(font.glyph(1).unwrap().advance, 8);
    assert_eq!(font.ppem_26_6(), (510, 510));
}

#[test]
fn adr0065_i2_pcf_rejects_truncation_and_bad_offsets() {
    assert!(PcfFont::parse(&ORDER[..100]).is_err());
    let mut bad = ORDER.to_vec();
    // The table directory's bitmap offset must remain in the file.
    let record = (0..9).map(|i| 8 + 16 * i).find(|at| bad[*at] == 8).unwrap();
    bad[record + 12..record + 16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(PcfFont::parse(&bad).is_err());
    assert!(PcfFont::parse(b"\x01fcp").is_err());
}

#[test]
fn adr0065_i2_pcf_zero_width_does_not_panic() {
    let mut data = MONO.to_vec();
    let record = (0..9)
        .map(|i| 8 + 16 * i)
        .find(|at| data[*at] == 4)
        .unwrap();
    let offset = u32::from_le_bytes(data[record + 12..record + 16].try_into().unwrap()) as usize;
    // The fixture uses compressed metrics; equal left and right bearings mean no row bytes.
    data[offset + 7] = data[offset + 6];
    let font = PcfFont::parse(&data).unwrap();
    assert_eq!(font.glyph(1).unwrap().width, 0);
}
