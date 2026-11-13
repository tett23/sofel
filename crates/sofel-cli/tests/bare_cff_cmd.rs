//! sofel ADR 0065 I1: a bare CFF face through the ordinary render command.
#![allow(clippy::unwrap_used)]

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const BARE_CFF: &[u8] = &[
    1, 0, 4, 4, 0, 1, 1, 1, 2, b'A', 0, 1, 1, 1, 3, 160, 17, 0, 0, 0, 0, 0, 1, 1, 1, 11, 239, 239,
    21, 239, 139, 5, 139, 239, 5, 14,
];

const TEXT_CFF: &[u8] = &[
    1, 0, 4, 4, 0, 1, 1, 1, 2, b'A', 0, 1, 1, 1, 3, 160, 17, 0, 0, 0, 0, 0, 2, 1, 1, 2, 14, 14,
    248, 136, 239, 239, 21, 239, 139, 5, 139, 239, 5, 14,
];

fn run(extra: &[&str]) -> (i32, Vec<u8>) {
    run_data(BARE_CFF, extra)
}

fn run_data(data: &[u8], extra: &[&str]) -> (i32, Vec<u8>) {
    run_data_as(data, extra, false)
}

fn run_data_as(data: &[u8], extra: &[&str], unified: bool) -> (i32, Vec<u8>) {
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-bare-cff-{}-{}.cff",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, data).unwrap();
    let mut command = Command::new(if unified {
        env!("CARGO_BIN_EXE_sofel")
    } else {
        env!("CARGO_BIN_EXE_sofel-render")
    });
    if unified {
        command.arg("render");
    }
    let result = command.args(extra).arg(&path).output().unwrap();
    std::fs::remove_file(path).unwrap();
    (result.status.code().unwrap(), result.stdout)
}

#[test]
fn adr0065_i1_bare_cff_shapes_and_renders_two_glyphs() {
    let (status, image) = run_data_as(TEXT_CFF, &["--ppem", "100", "--text", "  "], true);
    assert_eq!(status, 0);
    let mut expected = b"P5\n100 10\n255\n".to_vec();
    for row in 0..10 {
        for x in 0..100 {
            let value = if (10..20).contains(&x) || (60..70).contains(&x) {
                let local = x % 50 - 10;
                if local < 9 - row {
                    255
                } else if local == 9 - row {
                    127
                } else {
                    0
                }
            } else {
                255
            };
            expected.push(value);
        }
    }
    assert_eq!(image, expected);
}

#[test]
fn adr0065_i1_bare_cff_renders_reference_triangle() {
    let (status, image) = run(&["--ppem", "100", "--glyph", "0"]);
    assert_eq!(status, 0);
    let mut expected = b"P5\n# left 10 top 20\n10 10\n255\n".to_vec();
    for row in 0..10 {
        expected.extend(std::iter::repeat_n(255, 9 - row));
        expected.push(127);
        expected.extend(std::iter::repeat_n(0, row));
    }
    assert_eq!(image, expected);
}

#[test]
fn adr0065_i2_bare_cff_rejects_out_of_range_face() {
    assert_eq!(run(&["--ppem", "100", "--face", "1", "--glyph", "0"]).0, 1);
}

#[test]
fn adr0065_i1_cef_matches_reference_character_mapping_and_outline() {
    // CEF has no head. FreeType's CFF driver synthesizes its Unicode mapping
    // from glyph names even when this wrapper contains a cmap mapping A to 1.
    let mut data =
        b"OTTO\x00\x02\x00\x20\x00\x01\x00\x00CFF \x00\x00\x00\x00\x00\x00\x00\x2c".to_vec();
    data.extend_from_slice(&u32::try_from(TEXT_CFF.len()).unwrap().to_be_bytes());
    data.extend_from_slice(b"cmap\x00\x00\x00\x00\x00\x00\x00\x54\x00\x00\x00\x18");
    data.extend_from_slice(TEXT_CFF);
    data.resize(84, 0);
    data.extend_from_slice(&[
        0, 0, 0, 1, 0, 3, 0, 1, 0, 0, 0, 12, 0, 6, 0, 12, 0, 0, 0, 65, 0, 1, 0, 1,
    ]);
    let args = ["--ppem", "32", "--char", "U+0020"];
    let expected = run_data(TEXT_CFF, &args);
    assert_eq!(expected.0, 0);
    assert_eq!(run_data_as(&data, &args, true), expected);
    assert_ne!(run_data(&data, &["--ppem", "32", "--char", "U+0041"]).0, 0);
}
