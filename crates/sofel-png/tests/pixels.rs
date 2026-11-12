#![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]
use sofel_png::decode;
#[test]
fn i1_decode_all_color_types_depths_and_interlace() {
    check(
        0,
        1,
        &[128],
        &[255, 255, 255, 255],
        &[120, 156, 99, 104, 0, 0, 0, 130, 0, 129],
    );
    check(
        0,
        2,
        &[192],
        &[255, 255, 255, 255],
        &[120, 156, 99, 56, 0, 0, 0, 194, 0, 193],
    );
    check(
        0,
        4,
        &[240],
        &[255, 255, 255, 255],
        &[120, 156, 99, 248, 0, 0, 0, 242, 0, 241],
    );
    check(
        0,
        8,
        &[255],
        &[255, 255, 255, 255],
        &[120, 156, 99, 248, 15, 0, 1, 1, 1, 0],
    );
    check(
        0,
        16,
        &[255, 255],
        &[255, 255, 255, 255],
        &[120, 156, 99, 248, 255, 31, 0, 3, 0, 1, 255],
    );
    check(
        2,
        8,
        &[255, 0, 0],
        &[0, 0, 255, 255],
        &[120, 156, 99, 248, 207, 192, 0, 0, 3, 1, 1, 0],
    );
    check(
        2,
        16,
        &[255, 255, 0, 0, 0, 0],
        &[0, 0, 255, 255],
        &[120, 156, 99, 248, 255, 159, 1, 8, 0, 10, 252, 1, 255],
    );
    check(
        3,
        1,
        &[128],
        &[32, 16, 8, 128],
        &[120, 156, 99, 104, 0, 0, 0, 130, 0, 129],
    );
    check(
        3,
        2,
        &[64],
        &[32, 16, 8, 128],
        &[120, 156, 99, 112, 0, 0, 0, 66, 0, 65],
    );
    check(
        3,
        4,
        &[16],
        &[32, 16, 8, 128],
        &[120, 156, 99, 16, 0, 0, 0, 18, 0, 17],
    );
    check(
        3,
        8,
        &[1],
        &[32, 16, 8, 128],
        &[120, 156, 99, 96, 4, 0, 0, 3, 0, 2],
    );
    check(
        4,
        8,
        &[255, 127],
        &[127, 127, 127, 127],
        &[120, 156, 99, 248, 95, 15, 0, 2, 128, 1, 127],
    );
    check(
        4,
        16,
        &[255, 255, 127, 255],
        &[127, 127, 127, 127],
        &[120, 156, 99, 248, 255, 191, 254, 63, 0, 8, 251, 3, 125],
    );
    check(
        6,
        8,
        &[255, 0, 0, 127],
        &[0, 0, 127, 127],
        &[120, 156, 99, 248, 207, 192, 80, 15, 0, 4, 128, 1, 127],
    );
    check(
        6,
        16,
        &[255, 255, 0, 0, 0, 0, 127, 255],
        &[0, 0, 127, 127],
        &[
            120, 156, 99, 248, 255, 159, 1, 8, 234, 255, 3, 0, 16, 247, 3, 125,
        ],
    );
}
fn chunk(png: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    png.extend(u32::try_from(body.len()).unwrap().to_be_bytes());
    png.extend(tag);
    png.extend(body);
    png.extend(sofel_inflate::crc32(&[tag.as_slice(), body].concat()).to_be_bytes());
}
fn check(color: u8, depth: u8, _raw: &[u8], expected: &[u8], encoded: &[u8]) {
    for interlace in [0, 1] {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(
            &mut png,
            b"IHDR",
            &[0, 0, 0, 1, 0, 0, 0, 1, depth, color, 0, 0, interlace],
        );
        if color == 3 {
            chunk(&mut png, b"PLTE", &[0, 0, 0, 16, 32, 64]);
            chunk(&mut png, b"tRNS", &[255, 128]);
        }
        chunk(&mut png, b"IDAT", encoded);
        chunk(&mut png, b"IEND", &[]);
        let result = decode(&png, 1).unwrap();
        assert_eq!(result.bgra, expected);
        assert_eq!((result.width, result.height), (1, 1));
        assert!(decode(&png, 0).is_err());
        let last = png.last_mut().unwrap();
        *last ^= 1;
        assert!(decode(&png, 1).is_err());
    }
}

#[test]
fn i1_adam7_passes_reset_filters_and_place_every_pixel() {
    let image = decode(include_bytes!("data/adam7-rgba.png"), 81).unwrap();
    let expected: Vec<_> = (0u8..9)
        .flat_map(|y| (0u8..9).flat_map(move |x| [(x + y) * 13, y * 19, x * 17, 255]))
        .collect();
    assert_eq!(image.bgra, expected);
}
