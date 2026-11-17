//! bzip2 members decoded directly (sofel ADR 0065, moved by sofel ADR 0081).

use sofel_bzip2 as bzip2;

#[test]
fn adr0065_i1_bzip2_randomised_and_concatenated_members() {
    // An early-style randomised block, including its first bit flip.
    let member = [
        66, 90, 104, 49, 49, 65, 89, 38, 83, 89, 188, 203, 165, 171, 128, 0, 59, 64, 0, 127, 0, 32,
        0, 80, 134, 0, 77, 84, 52, 61, 17, 155, 17, 202, 26, 212, 140, 17, 200, 142, 196, 122, 35,
        226, 238, 72, 167, 10, 18, 23, 153, 116, 181, 96,
    ];
    let expected: Vec<_> = (0u8..7).cycle().take(830).collect();
    assert_eq!(bzip2::decode(&member, 830), Ok(expected.clone()));
    assert!(bzip2::decode(&member, 829).is_err());
    assert_eq!(
        bzip2::decode(&member.repeat(2), 1660),
        Ok(expected.repeat(2))
    );
}
