#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use sofel_inflate::{deflate, gzip, zlib};
#[test]
fn i1_i3_decode_block_types_with_output_limits() {
    let message = b"OpenType\n".repeat(1300);
    let mut stored = vec![1];
    let len = u16::try_from(message.len()).unwrap();
    stored.extend(len.to_le_bytes());
    stored.extend((!len).to_le_bytes());
    stored.extend(&message);
    assert_eq!(deflate(&stored, message.len()).unwrap(), message);
    assert!(deflate(&stored, message.len() - 1).is_err());
    assert_eq!(
        deflate(
            &[
                243, 47, 72, 205, 11, 169, 44, 72, 229, 242, 31, 101, 140, 50, 70, 25, 163, 140,
                81, 198, 40, 99, 148, 49, 202, 24, 101, 140, 50, 70, 25, 163, 140, 81, 198, 40, 99,
                148, 49, 202, 24, 101, 140, 50, 70, 25, 163, 140, 81, 198, 40, 99, 148, 49, 202,
                24, 101, 140, 50, 70, 25, 163, 140, 81, 198, 40, 99, 148, 49, 202, 24, 101, 140,
                50, 70, 25, 163, 140, 81, 198, 40, 99, 148, 49, 202, 24, 101, 140, 50, 70, 25, 163,
                140, 81, 6, 181, 25, 0
            ],
            message.len()
        )
        .unwrap(),
        message
    );
    assert!(
        deflate(
            &[
                243, 47, 72, 205, 11, 169, 44, 72, 229, 242, 31, 101, 140, 50, 70, 25, 163, 140,
                81, 198, 40, 99, 148, 49, 202, 24, 101, 140, 50, 70, 25, 163, 140, 81, 198, 40, 99,
                148, 49, 202, 24, 101, 140, 50, 70, 25, 163, 140, 81, 198, 40, 99, 148, 49, 202,
                24, 101, 140, 50, 70, 25, 163, 140, 81, 198, 40, 99, 148, 49, 202, 24, 101, 140,
                50, 70, 25, 163, 140, 81, 198, 40, 99, 148, 49, 202, 24, 101, 140, 50, 70, 25, 163,
                140, 81, 6, 181, 25, 0
            ],
            message.len() - 1
        )
        .is_err()
    );
    assert_eq!(
        deflate(
            &[
                237, 198, 33, 17, 0, 64, 8, 0, 48, 255, 229, 48, 95, 1, 203, 97, 105, 79, 9, 228,
                166, 22, 157, 245, 167, 243, 133, 136, 136, 136, 136, 136, 136, 136, 136, 136, 136,
                136, 136, 136, 136, 136, 136, 136, 136, 136, 136, 136, 136, 92, 103, 1
            ],
            message.len()
        )
        .unwrap(),
        message
    );
    assert!(
        deflate(
            &[
                237, 198, 33, 17, 0, 64, 8, 0, 48, 255, 229, 48, 95, 1, 203, 97, 105, 79, 9, 228,
                166, 22, 157, 245, 167, 243, 133, 136, 136, 136, 136, 136, 136, 136, 136, 136, 136,
                136, 136, 136, 136, 136, 136, 136, 136, 136, 136, 136, 136, 92, 103, 1
            ],
            message.len() - 1
        )
        .is_err()
    );
}
#[test]
fn i1_i3_wrappers_verify_trailers_and_gzip_members() {
    let good = &[120, 156, 11, 14, 115, 7, 0, 1, 239, 0, 241];
    assert_eq!(zlib(good, 3).unwrap(), b"SVG");
    let mut corrupt = good.to_vec();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(zlib(&corrupt, 3).is_err());
    let good = &[
        31, 139, 8, 0, 0, 0, 0, 0, 0, 255, 11, 14, 115, 7, 0, 26, 103, 226, 149, 3, 0, 0, 0,
    ];
    assert_eq!(gzip(good, 3).unwrap(), b"SVG");
    assert_eq!(
        gzip(&[good.as_slice(), good.as_slice()].concat(), 6).unwrap(),
        b"SVGSVG"
    );
    let mut bad = good.to_vec();
    bad[15] ^= 1;
    assert!(gzip(&bad, 3).is_err());
}
