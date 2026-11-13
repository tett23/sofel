pub fn triangle() -> Vec<u8> {
    let mut data = vec![0; 124];
    data[..4].copy_from_slice(b"PFR0");
    for (offset, width, value) in [
        (4, 2, 4u32),
        (6, 2, 0x0d0a),
        (8, 2, 58),
        (10, 2, 7),
        (12, 2, 58),
        (14, 2, 18),
        (16, 3, 18),
        (19, 3, 65),
        (22, 2, 37),
        (24, 3, 37),
        (27, 3, 83),
        (30, 2, 2),
        (32, 3, 4),
        (35, 3, 120),
        (52, 2, 1),
        (56, 2, 1),
        (58, 2, 1),
        (60, 2, 18),
        (62, 3, 65),
        (65, 3, 65536),
        (74, 3, 65536),
        (78, 2, 37),
        (80, 3, 83),
        (85, 2, 1000),
        (87, 2, 1000),
        (98, 2, 500),
        (110, 2, 1),
        (112, 1, 65),
        (113, 1, 2),
        (114, 2, 1),
    ] {
        data[offset..offset + width].copy_from_slice(&value.to_be_bytes()[4 - width..]);
    }
    let program = [0, 0x4f, 0x1e, 80, 0x1b, 60, 0];
    data.truncate(121);
    data.extend_from_slice(&program);
    data[34] = 8;
    data[113] = 7;
    data
}

pub fn bitmap_only() -> Vec<u8> {
    let mut data = triangle();
    data.truncate(120);
    data[113] = 2;
    data[97] = 128;
    data.splice(
        100..100,
        [1, 13, 1, 0, 0, 4, 0, 1, 12, 12, 0, 0, 4, 0, 0, 1],
    );
    let program = [4, 0, 0x32, 0xa8];
    data.extend([65, 4, 0, 3, 0, 0, 0]);
    data.extend_from_slice(&program);
    for (index, value) in [(23, 53), (26, 53), (79, 53), (34, 7), (37, 140)] {
        data[index] = value;
    }
    data
}
