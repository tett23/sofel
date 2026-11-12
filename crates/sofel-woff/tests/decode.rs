#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
use sofel_read::Tag;
use sofel_woff::{Format, decode_woff, encode};
use sofel_write::build_sfnt_ordered;

#[test]
fn i1_font_driver_woff_restores_directory_data_and_physical_order() {
    let input = build_sfnt_ordered(
        &[
            (Tag::new(b"head"), &[0; 54]),
            (Tag::new(b"abcd"), &[1, 2, 3]),
            (Tag::new(b"xyz "), &[42; 500]),
        ],
        &[Tag::new(b"head")],
    )
    .unwrap();
    let packed = encode(&input, Format::Woff).unwrap();
    assert_eq!(decode_woff(&packed, input.len()).unwrap(), input);
    assert!(decode_woff(&packed, input.len() - 1).is_err());
}
#[test]
fn i2_bounded_input_rejects_short_or_overlapping_woff_records() {
    let input = build_sfnt_ordered(
        &[
            (Tag::new(b"head"), &[0; 54]),
            (Tag::new(b"test"), &[42; 100]),
        ],
        &[],
    )
    .unwrap();
    let packed = encode(&input, Format::Woff).unwrap();
    assert!(decode_woff(&packed[..packed.len() - 1], input.len()).is_err());
    for (at, value) in [
        (48, 0),
        (52, 1000),
        (56, u32::MAX),
        (16, u32::MAX),
        (60, 0xdeadbeef),
    ] {
        let mut damaged = packed.clone();
        damaged[at..at + 4].copy_from_slice(&u32::to_be_bytes(value));
        assert!(decode_woff(&damaged, input.len()).is_err());
    }
}
