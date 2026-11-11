//! FreeType's single and range advance queries on independent fresh faces
//! (sofel ADR 0030).

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use sofel_conformance::paths;
use std::fmt::Write;
use std::process::Command;

#[test]
fn adr0030_advances_report_both_queries_for_every_glyph() {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap();
    let path =
        std::env::temp_dir().join(format!("sofel-adr0030-advances-{}.ttf", std::process::id()));
    std::fs::write(&path, sfnt(&sample_tables_with_glyf())).unwrap();
    let output = Command::new(exe)
        .arg("--advances")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout).unwrap();
    let mut expected = String::from("face 0\nnum_glyphs 28\n");
    for direction in ["h", "v"] {
        for query in ["single", "batch"] {
            for glyph in 0..28 {
                let value = if direction == "h" {
                    500 + glyph.min(19)
                } else {
                    1000
                };
                writeln!(expected, "advance {direction} {query} {glyph} 0 {value}").unwrap();
            }
        }
    }
    expected.push_str("end\n");
    assert_eq!(actual, expected);
}
