//! sofel ADR 0065 I1: FNT and FON through the ordinary render command.
#![allow(clippy::unwrap_used)]
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const FNT: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono.fnt");
const FNT3: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/mono3.fnt");
const FON: &[u8] = include_bytes!("../../sofel-bitmapfont/tests/data/two.fon");

fn run(data: &[u8], extra: &[&str]) -> (i32, Vec<u8>) {
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-winfnt-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, data).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
        .args(extra)
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    (output.status.code().unwrap(), output.stdout)
}

#[test]
fn adr0065_i1_fnt_and_fon_render_match_freetype_bitmap() {
    let (status, image) = run(FNT, &["--ppem", "8", "--char", "U+0041"]);
    assert_eq!(status, 0);
    let mut expected = b"P5\n# left 0 top 8\n8 8\n255\n".to_vec();
    for row in [0x18u8, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0] {
        for bit in 0..8 {
            expected.push(if row & (0x80 >> bit) != 0 { 0 } else { 255 });
        }
    }
    assert_eq!(image, expected);
    assert_eq!(run(FNT, &["--ppem", "8", "--glyph", "0"]).1, image);
    assert_eq!(run(FNT3, &["--ppem", "8", "--glyph", "1"]).1, image);
    assert_eq!(
        run(FON, &["--ppem", "8", "--face", "0", "--glyph", "1"]).1,
        image
    );
    let (status, second) = run(FON, &["--ppem", "8", "--face", "1", "--char", "U+0042"]);
    assert_eq!(status, 0);
    let mut solid = b"P5\n# left 0 top 8\n8 8\n255\n".to_vec();
    solid.extend_from_slice(&[0; 64]);
    assert_eq!(second, solid);
}

#[test]
fn adr0065_i1_fnt_and_fon_reject_bounds_and_missing_strike() {
    assert_eq!(
        run(FNT, &["--ppem", "8", "--face", "1", "--glyph", "1"]).0,
        1
    );
    assert_eq!(
        run(FON, &["--ppem", "8", "--face", "2", "--glyph", "1"]).0,
        1
    );
    assert_eq!(
        run(FON, &["--ppem", "7", "--face", "1", "--glyph", "1"]).0,
        1
    );
    assert_eq!(
        run(FON, &["--ppem", "8", "--face", "1", "--char", "U+0041"]).0,
        1
    );
    assert_eq!(
        run(
            FON,
            &["--ppem", "8", "--face", "1", "--no-bitmap", "--glyph", "1"]
        )
        .0,
        1
    );
}
