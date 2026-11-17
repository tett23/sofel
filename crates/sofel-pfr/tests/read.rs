#![allow(clippy::unwrap_used)]
use sofel_pfr::{PfrError, PfrFace};

#[test]
fn rejects_short_and_foreign_data_without_a_position_past_the_header() {
    assert_eq!(PfrFace::read(&[], 0).unwrap_err(), PfrError::OutOfBounds);
    let mut data = [0u8; 58];
    data[..4].copy_from_slice(b"PFR1");
    assert_eq!(PfrFace::read(&data, 0).unwrap_err(), PfrError::InvalidValue);
    assert_eq!(
        PfrFace::read_positioned(&data, 0).unwrap_err(),
        (PfrError::InvalidValue, 0)
    );
}
