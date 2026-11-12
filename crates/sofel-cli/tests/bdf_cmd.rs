//! sofel ADR 0065 I1: a BDF face through the ordinary render command.
#![allow(clippy::unwrap_used)]
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const BDF: &str = include_str!("../../sofel-bitmapfont/tests/data/mono.bdf");
const GRAY2: &str = include_str!("../../sofel-bitmapfont/tests/data/gray2.bdf");

fn run(extra: &[&str]) -> (i32, Vec<u8>) {
    run_data(BDF, extra)
}

fn run_data(data: &str, extra: &[&str]) -> (i32, Vec<u8>) {
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-bdf-{}-{}.bdf",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, data).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
        .args(extra)
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    (result.status.code().unwrap(), result.stdout)
}

#[test]
fn adr0065_i1_bdf_gray2_matches_freetype_bitmap() {
    let (status, image) = run_data(GRAY2, &["--ppem", "8", "--glyph", "1"]);
    assert_eq!(status, 0);
    let mut expected = b"P5\n# left 0 top 2\n4 2\n255\n".to_vec();
    expected.extend_from_slice(&[255, 170, 85, 0, 0, 85, 170, 255]);
    assert_eq!(image, expected);
}

#[test]
fn adr0065_i1_bdf_gray4_and_gray8_match_freetype_bitmap() {
    for (bits, bdf, expected_pixels) in [
        (
            4,
            include_str!("../../sofel-bitmapfont/tests/data/gray4.bdf"),
            &[238, 0, 85, 255][..],
        ),
        (
            8,
            include_str!("../../sofel-bitmapfont/tests/data/gray8.bdf"),
            &[255, 0, 127, 191][..],
        ),
    ] {
        let (status, image) = run_data(bdf, &["--ppem", "8", "--glyph", "1"]);
        assert_eq!(status, 0, "{bits} bit");
        let mut expected = b"P5\n# left 0 top 1\n4 1\n255\n".to_vec();
        expected.extend_from_slice(expected_pixels);
        assert_eq!(image, expected, "{bits} bit");
    }
}

#[test]
fn adr0065_i1_bdf_char_and_notdef_match_freetype_bitmap() {
    let (status, image) = run(&["--ppem", "8", "--char", "U+0041"]);
    assert_eq!(status, 0);
    let header = b"P5\n# left 0 top 8\n8 8\n255\n";
    assert!(image.starts_with(header));
    let mut expected = header.to_vec();
    for row in [0x18u8, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0] {
        for bit in 0..8 {
            expected.push(if row & (0x80 >> bit) != 0 { 0 } else { 255 });
        }
    }
    assert_eq!(image, expected);
    let (status, notdef) = run(&["--ppem", "8", "--glyph", "0"]);
    assert_eq!(status, 0);
    assert_eq!(notdef, image);
}

#[test]
fn adr0065_i1_bdf_bounds_and_strike() {
    assert_eq!(run(&["--ppem", "8", "--face", "1", "--glyph", "1"]).0, 1);
    assert_eq!(run(&["--ppem", "7", "--glyph", "1"]).0, 1);
    assert_eq!(run(&["--ppem", "8", "--char", "U+0042"]).0, 1);
    let at_75_dpi = BDF.replace("SIZE 8 72 72", "SIZE 8 75 75");
    assert_eq!(run_data(&at_75_dpi, &["--ppem", "8", "--glyph", "1"]).0, 0);
    assert_eq!(run_data(&at_75_dpi, &["--ppem", "9", "--glyph", "1"]).0, 1);
}
