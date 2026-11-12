//! sofel ADR 0065 I1: PCF through the ordinary render command.
#![allow(clippy::unwrap_used)]
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const PCF: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono.pcf");
const SWAP: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono_swap.pcf");
const ORDER_LSB: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/order_lsb.pcf");

fn run(extra: &[&str]) -> (i32, Vec<u8>) {
    run_data(PCF, extra)
}

fn run_data(data: &[u8], extra: &[&str]) -> (i32, Vec<u8>) {
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-pcf-{}-{}.pcf",
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
fn adr0065_i1_pcf_lsb_order_and_default_match_freetype() {
    let (status, image) = run_data(ORDER_LSB, &["--ppem", "8", "--char", "U+0041"]);
    assert_eq!(status, 0);
    assert!(image.starts_with(b"P5\n# left 0 top 8\n8 8\n255\n"));
    assert_eq!(
        run_data(ORDER_LSB, &["--ppem", "8", "--glyph", "2"]).1,
        image
    );
    let (status, default) = run_data(ORDER_LSB, &["--ppem", "8", "--glyph", "0"]);
    assert_eq!(status, 0);
    assert_eq!(
        default,
        run_data(ORDER_LSB, &["--ppem", "8", "--char", "U+0042"]).1
    );
}

#[test]
fn adr0065_i1_pcf_char_and_notdef_match_freetype_bitmap() {
    let (status, image) = run(&["--ppem", "8", "--char", "U+0041"]);
    assert_eq!(status, 0);
    let mut expected = b"P5\n# left 0 top 8\n8 8\n255\n".to_vec();
    for row in [0x18u8, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0] {
        for bit in 0..8 {
            expected.push(if row & (0x80 >> bit) != 0 { 0 } else { 255 });
        }
    }
    assert_eq!(image, expected);
    assert_eq!(run(&["--ppem", "8", "--glyph", "0"]).1, image);
    assert_eq!(run_data(SWAP, &["--ppem", "8", "--glyph", "1"]).1, image);
}

#[test]
fn adr0065_i1_pcf_rejects_other_face_strike_and_character() {
    assert_eq!(run(&["--ppem", "8", "--face", "1", "--glyph", "1"]).0, 1);
    assert_eq!(run(&["--ppem", "7", "--glyph", "1"]).0, 1);
    assert_eq!(run(&["--ppem", "8", "--char", "U+0042"]).0, 1);
}
