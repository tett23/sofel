//! `sofel-paint` (ADR 0023): its text, number format and exit status.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
#[allow(dead_code)]
mod build;

use build::*;
use sofel_cli::paint::{format_g, format_g3};
use std::path::PathBuf;
use std::process::Command;

/// Glyph 1 ('A') is a COLRv0 glyph of two layers; glyph 2 is not a color
/// glyph.
fn font_file(name: &str) -> PathBuf {
    let colr = colr_v0(&[(1, 0, 2)], &[(2, 0), (2, 0xFFFF)]);
    let cpal = cpal(1, &[0, 1], &[[0, 0, 255, 255], [255, 0, 0, 128]], None);
    let font = glyph_font_with(
        &[Vec::new(), Vec::new(), Vec::new()],
        false,
        false,
        vec![(b"COLR", colr), (b"CPAL", cpal)],
    );
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-paint-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, font).unwrap();
    p
}

fn run(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_sofel-paint"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
    )
}

#[test]
fn writes_the_layers() {
    let f = font_file("layers");
    let p = f.to_str().unwrap();
    assert_eq!(
        run(&["--glyph", "1", p]),
        (
            0,
            "fill glyph 2 solid 255 0 0 255\nfill glyph 2 solid 0 0 0 255\n".into()
        )
    );
    assert_eq!(
        run(&[
            "--palette",
            "1",
            "--foreground",
            "11223344",
            "--char",
            "U+0041",
            p
        ]),
        (
            0,
            "fill glyph 2 solid 0 0 255 128\nfill glyph 2 solid 17 34 51 68\n".into()
        )
    );
    assert_eq!(run(&["--glyph", "2", p]).0, 1);
    assert_eq!(run(&["--glyph", "1", "/nonexistent.ttf"]).0, 1);
    assert_eq!(run(&["--glyph", "1"]).0, 2);
    assert_eq!(run(&["--foreground", "12", "--glyph", "1", p]).0, 2);
    std::fs::remove_file(&f).ok();
}

#[test]
fn formats_numbers_as_printf() {
    // C's printf("%.3g") and printf("%.6g").
    let cases: [(f64, &str); 12] = [
        (1000.0, "1e+03"),
        (0.977, "0.977"),
        (-0.0, "-0"),
        (1300.0, "1.3e+03"),
        (100.0, "100"),
        (1220.0, "1.22e+03"),
        (999.5, "1e+03"),
        (0.0001, "0.0001"),
        (0.00001234, "1.23e-05"),
        (-224.0, "-224"),
        (1e100, "1e+100"),
        (f64::INFINITY, "inf"),
    ];
    for (v, want) in cases {
        assert_eq!(format_g3(v), want, "{v}");
    }
    assert_eq!(format_g(3.14159265, 6), "3.14159");
    assert_eq!(format_g(123456789.0, 6), "1.23457e+08");
}
