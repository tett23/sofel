//! sofel ADR 0065 I1: a bare CFF face through the ordinary render command.
#![allow(clippy::unwrap_used)]

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const BARE_CFF: &[u8] = &[
    1, 0, 4, 4, 0, 1, 1, 1, 2, b'A', 0, 1, 1, 1, 3, 160, 17, 0, 0, 0, 0, 0, 1, 1, 1, 11, 239, 239,
    21, 239, 139, 5, 139, 239, 5, 14,
];

fn run(extra: &[&str]) -> (i32, Vec<u8>) {
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-bare-cff-{}-{}.cff",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, BARE_CFF).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
        .args(extra)
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    (result.status.code().unwrap(), result.stdout)
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
