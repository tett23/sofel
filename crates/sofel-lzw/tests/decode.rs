//! UNIX compress streams decoded directly (sofel ADR 0065, moved by sofel ADR 0081).

use sofel_lzw as lzw;

#[test]
fn i2_lzw_checks_codes_and_output_limit() {
    assert!(lzw::decode(&[31, 157, 0x90, 0x41], 7).is_err());
    assert!(lzw::decode(&[31, 157, 0x90, 0xff, 1], 7).is_err());
    assert!(lzw::decode(&[31, 157, 0x90, 0x41, 0x84, 4, 0x1c, 8], 6).is_err());
}
