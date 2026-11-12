#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use sofel_read::trak::Trak;
fn table() -> Vec<u8> {
    let mut b = vec![
        0, 1, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 1, 0, 2, 0, 0, 0, 28, 0, 0, 0, 0, 0, 0, 0, 36,
    ];
    b.extend((8i32 << 16).to_be_bytes());
    b.extend((16i32 << 16).to_be_bytes());
    b.extend(60i16.to_be_bytes());
    b.extend((-20i16).to_be_bytes());
    b
}
#[test]
fn i1_aat_shape_trak_interpolates_size_and_clamps_ends() {
    let bytes = table();
    let t = Trak::new(&bytes).unwrap();
    for (ptem, delta) in [
        (1., 60.),
        (8., 60.),
        (10., 40.),
        (12., 20.),
        (16., -20.),
        (24., -20.),
    ] {
        assert_eq!(t.tracking(true, ptem), delta);
    }
    assert_eq!(t.tracking(false, 12.), 0.);
}
#[test]
fn i2_aat_state_trak_rejects_incomplete_values() {
    let bytes = table();
    for end in 0..bytes.len() {
        assert!(Trak::new(&bytes[..end]).is_none());
    }
}
