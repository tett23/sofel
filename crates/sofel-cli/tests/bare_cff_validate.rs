//! sofel ADR 0065 I2: the validator reports bare CFF damage.
#![allow(clippy::unwrap_used)]

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const TWO_FACES: &[u8] = &[
    1, 0, 4, 4, 0, 2, 1, 1, 2, 3, b'A', b'B', 0, 2, 1, 1, 3, 7, 167, 17, 167, 17, 181, 13, 0, 0, 0,
    0, 0, 1, 1, 1, 2, 14,
];

const MISSING_SECOND_CHARSTRINGS: &[u8] = &[
    1, 0, 4, 4, 0, 2, 1, 1, 2, 3, b'A', b'B', 0, 2, 1, 1, 3, 5, 165, 17, 181, 13, 0, 0, 0, 0, 0, 1,
    1, 1, 2, 14,
];

fn run(data: &[u8]) -> (i32, String) {
    let path = std::env::temp_dir().join(format!(
        "sofel-adr0065-bare-cff-validate-{}-{}.cff",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, data).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_sofel-validate"))
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

#[test]
fn adr0065_i1_bare_cff_valid_file_has_no_errors() {
    assert_eq!(run(TWO_FACES).0, 0);
}

#[test]
fn adr0065_i2_bare_cff_reports_missing_second_charstrings() {
    let (status, output) = run(MISSING_SECOND_CHARSTRINGS);
    assert_eq!(status, 1);
    assert!(output.contains("\"offset\":20"));
    assert!(output.contains("missing_char_strings"));
}
