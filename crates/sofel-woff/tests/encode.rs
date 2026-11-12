#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use sofel_read::Tag;
use sofel_woff::{Format, encode};
use sofel_write::build_sfnt_ordered;

fn font() -> Vec<u8> {
    build_sfnt_ordered(
        &[
            (Tag::new(b"head"), &[0; 54]),
            (Tag::new(b"Xfoo"), &[42; 513]),
            (Tag::new(b"DSIG"), &[1, 2, 3]),
        ],
        &[Tag::new(b"head")],
    )
    .unwrap()
}

// gzip reads the same RFC 1951 bitstream under an RFC 1952 wrapper.
fn inflate_with_gzip(zlib: &[u8], expected: &[u8]) -> Vec<u8> {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut gzip = vec![31, 139, 8, 0, 0, 0, 0, 0, 0, 255];
    gzip.extend(&zlib[2..zlib.len() - 4]);
    let mut crc = u32::MAX;
    for &b in expected {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & (crc & 1).wrapping_neg());
        }
    }
    gzip.extend((!crc).to_le_bytes());
    gzip.extend(u32::try_from(expected.len()).unwrap().to_le_bytes());
    let mut child = Command::new("gzip")
        .arg("-dc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&gzip).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    result.stdout
}

#[test]
fn i1_webfont_roundtrip_preserves_woff_physical_order() {
    let input = font();
    let encoded = encode(&input, Format::Woff).unwrap();
    assert_eq!(&encoded[4..8], &input[..4]);
    assert_eq!(&encoded[12..14], &input[4..6]);
    let mut decoded = input[..12].to_vec();
    decoded.resize(60, 0);
    let mut records: Vec<_> = encoded[44..104].as_chunks::<20>().0.iter().collect();
    records.sort_by_key(|r| &r[4..8]);
    let mut directory = Vec::new();
    for r in records {
        let word = |at| u32::from_be_bytes(r[at..at + 4].try_into().unwrap()) as usize;
        let data = &encoded[word(4)..word(4) + word(8)];
        let mut record = Vec::from(&r[..4]);
        record.extend(&r[16..20]);
        record.extend(u32::try_from(decoded.len()).unwrap().to_be_bytes());
        record.extend(&r[12..16]);
        directory.push(record);
        if word(8) < word(12) {
            let expected = &input[decoded.len()..decoded.len() + word(12)];
            decoded.extend(inflate_with_gzip(data, expected));
        } else {
            decoded.extend(data);
        }
        decoded.resize(decoded.len().next_multiple_of(4), 0);
    }
    directory.sort_by_key(|r| <[u8; 4]>::try_from(&r[..4]).unwrap());
    decoded[12..60].copy_from_slice(&directory.concat());
    assert_eq!(decoded, input);
}

#[test]
fn i2_webfont_directory_drops_dsig_only_for_woff2() {
    let data = encode(&font(), Format::Woff2).unwrap();
    assert_eq!(&data[..4], b"wOF2");
    assert_eq!(&data[12..14], &2u16.to_be_bytes());
    assert_eq!(&data[28..48], &[0; 20]);
}

#[test]
fn i3_webfont_error_bad_checksums_and_extra_bytes() {
    for format in [Format::Woff, Format::Woff2] {
        let mut invalid = font();
        invalid[8] ^= 1;
        for data in [&[][..], b"ttcf", &invalid] {
            assert!(encode(data, format).is_err());
        }
    }
    let mut trailing = font();
    trailing.extend([0; 4]);
    assert!(encode(&trailing, Format::Woff).is_err());
}

#[test]
fn i3_webfont_error_keeps_existing_and_absent_output() {
    use std::{
        fs,
        process::{Command, Stdio},
    };
    let dir = std::env::temp_dir().join(format!("sofel-woff-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("result");
    for format in ["woff", "woff2"] {
        for exists in [false, true] {
            if exists {
                fs::write(&path, b"original").unwrap();
            }
            let result = Command::new(env!("CARGO_BIN_EXE_sofel-woff"))
                .args(["--format", format, "-", "-o"])
                .arg(&path)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
            if exists {
                assert_eq!(fs::read(&path).unwrap(), b"original");
                fs::remove_file(&path).unwrap();
            } else {
                assert!(!path.exists());
            }
        }
        let out = Command::new(env!("CARGO_BIN_EXE_sofel-woff"))
            .args(["--format", format, "-", "-o", "-"])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
    }
    fs::remove_dir(dir).unwrap();
}

#[test]
fn i2_webfont_directory_null_transforms_and_signature_flags() {
    for signed in [false, true] {
        for already_flagged in [false, true] {
            let mut head = [0u8; 54];
            if already_flagged {
                head[16] = 8;
            }
            let mut tables = vec![
                (Tag::new(b"head"), head.as_slice()),
                (Tag::new(b"loca"), &[0, 0][..]),
                (Tag::new(b"glyf"), &[0, 0, 0][..]),
                (Tag::new(b"ZZZZ"), &[0x99][..]),
            ];
            if signed {
                tables.push((Tag::new(b"DSIG"), &[0x55]));
            }
            let font = build_sfnt_ordered(&tables, &[Tag::new(b"loca")]).unwrap();
            let woff = encode(&font, Format::Woff).unwrap();
            assert_eq!(
                woff[12..14],
                u16::try_from(tables.len()).unwrap().to_be_bytes()
            );
            let woff2 = encode(&font, Format::Woff2).unwrap();
            assert_eq!(&woff2[12..14], &4u16.to_be_bytes());
            assert_eq!(
                &woff2[48..60],
                &[63, b'Z', b'Z', b'Z', b'Z', 1, 202, 3, 1, 54, 203, 2]
            );
            // One RFC 7932 uncompressed meta-block, then a final empty block.
            let stream = &woff2[60..];
            let length_minus_one =
                (u32::from_le_bytes([stream[0], stream[1], stream[2], 0]) >> 4) & 0xffff;
            assert_eq!(length_minus_one, 59);
            assert_eq!(stream[2] & 16, 16);
            let payload = &stream[3..63];
            assert_eq!(payload[0], 0x99);
            assert_eq!(payload[4 + 16], 8);
            assert_eq!(&payload[4 + 8..4 + 12], &[0; 4]);
            assert_eq!(stream[63], 3);
        }
    }
}
