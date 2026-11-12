//! sofel ADR 0065 I1: a BDF face through the ordinary render command.
#![allow(clippy::unwrap_used)]
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const BDF: &str = "STARTFONT 2.1\nFONT -misc-test-medium-r-normal--8-80-72-72-c-80-iso10646-1\nSIZE 8 72 72\nFONTBOUNDINGBOX 8 8 0 0\nSTARTPROPERTIES 2\nFONT_ASCENT 8\nFONT_DESCENT 0\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nSWIDTH 1000 0\nDWIDTH 8 0\nBBX 8 8 0 0\nBITMAP\n18\n24\n42\n7E\n42\n42\n42\n00\nENDCHAR\nENDFONT\n";
const GRAY2: &str = "STARTFONT 2.1\nFONT -misc-test-medium-r-normal--8-80-72-72-c-80-iso10646-1\nSIZE 8 72 72 2\nFONTBOUNDINGBOX 4 2 0 0\nSTARTPROPERTIES 2\nFONT_ASCENT 2\nFONT_DESCENT 0\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nSWIDTH 1000 0\nDWIDTH 4 0\nBBX 4 2 0 0\nBITMAP\n1B\nE4\nENDCHAR\nENDFONT\n";

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
    for (bits, row, expected_pixels) in [
        (4, "1FA0", &[238, 0, 85, 255][..]),
        (8, "00FF8040", &[255, 0, 127, 191][..]),
    ] {
        let bdf = format!(
            "STARTFONT 2.1\nFONT -misc-test-medium-r-normal--8-80-72-72-c-80-iso10646-1\nSIZE 8 72 72 {bits}\nFONTBOUNDINGBOX 4 1 0 0\nSTARTPROPERTIES 2\nFONT_ASCENT 1\nFONT_DESCENT 0\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nSWIDTH 1000 0\nDWIDTH 4 0\nBBX 4 1 0 0\nBITMAP\n{row}\nENDCHAR\nENDFONT\n"
        );
        let (status, image) = run_data(&bdf, &["--ppem", "8", "--glyph", "1"]);
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
