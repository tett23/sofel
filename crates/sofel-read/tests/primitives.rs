#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use sofel_read::be;
use sofel_read::{F2Dot14, Fixed, GlyphId, LongDateTime, Tag};

#[test]
fn reads_each_width_at_the_boundary() {
    let d = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    assert_eq!(be::u8_at(&d, 7), Some(0x08));
    assert_eq!(be::i8_at(&[0xFF], 0), Some(-1));
    assert_eq!(be::u16_at(&d, 6), Some(0x0708));
    assert_eq!(be::i16_at(&[0xFF, 0xFE], 0), Some(-2));
    assert_eq!(be::u24_at(&d, 5), Some(0x06_0708));
    assert_eq!(be::u32_at(&d, 4), Some(0x0506_0708));
    assert_eq!(be::i32_at(&[0xFF, 0xFF, 0xFF, 0xFE], 0), Some(-2));
    assert_eq!(be::i64_at(&d, 0), Some(0x0102_0304_0506_0708));
}

#[test]
fn returns_none_when_one_byte_is_missing() {
    let d = [0u8; 8];
    assert_eq!(be::u8_at(&d, 8), None);
    assert_eq!(be::u16_at(&d, 7), None);
    assert_eq!(be::u24_at(&d, 6), None);
    assert_eq!(be::u32_at(&d, 5), None);
    assert_eq!(be::i64_at(&d, 1), None);
}

#[test]
fn returns_none_near_usize_max() {
    let d = [0u8; 8];
    assert_eq!(be::u16_at(&d, usize::MAX), None);
    assert_eq!(be::u32_at(&d, usize::MAX - 1), None);
    assert_eq!(be::slice_at(&d, usize::MAX, 2), None);
    assert_eq!(be::slice_at(&d, 2, usize::MAX), None);
}

#[test]
fn slices() {
    let d = [1u8, 2, 3, 4];
    assert_eq!(be::slice_at(&d, 1, 2), Some(&[2u8, 3][..]));
    assert_eq!(be::slice_at(&d, 4, 0), Some(&[][..]));
    assert_eq!(be::slice_at(&d, 3, 2), None);
}

#[test]
fn tags() {
    let t = Tag::new(b"head");
    assert_eq!(t.to_u32(), 0x6865_6164);
    assert_eq!(Tag::from_u32(0x6865_6164), t);
    assert_eq!(format!("{t}"), "head");
    assert_eq!(format!("{}", Tag::new(b"OS/2")), "OS/2");
    assert_eq!(format!("{}", Tag::new(&[0x00, 0x41, 0xFF, 0x20])), "?A? ");
    assert!(Tag::new(b"cmap") < Tag::new(b"head"));
}

#[test]
fn raw_value_types() {
    assert_eq!(GlyphId::from(7u16), GlyphId(7));
    assert_eq!(Fixed(0x0001_8000).0, 0x0001_8000);
    assert_eq!(F2Dot14(-16384).0, -16384);
    assert_eq!(LongDateTime(1).0, 1);
}
