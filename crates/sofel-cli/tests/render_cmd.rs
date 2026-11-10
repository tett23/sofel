#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::*;
use std::path::PathBuf;
use std::process::Command;

fn font_file(name: &str) -> PathBuf {
    // Glyph 1 is a 1000-unit square: exactly 12 pixels at 12 ppem; 'A'
    // maps to it. Glyph 2 is empty.
    let square = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 1000),
            Pt::On(1000, 1000),
            Pt::On(1000, 0),
        ]],
        false,
        &[],
    );
    let font = glyph_font(&[Vec::new(), square, Vec::new()], true, false);
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-render-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, font).unwrap();
    p
}

fn run(args: &[&str]) -> (i32, Vec<u8>, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
        .args(args)
        .output()
        .unwrap();
    (
        o.status.code().unwrap(),
        o.stdout,
        String::from_utf8(o.stderr).unwrap(),
    )
}

#[test]
fn renders_a_glyph_as_pgm() {
    let f = font_file("glyph");
    let (code, out, err) = run(&["--ppem", "12", "--glyph", "1", f.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    let header = b"P5\n# left 0 top 12\n12 12\n255\n";
    assert_eq!(&out[..header.len()], header);
    // Black ink on white: full coverage is 0.
    assert_eq!(&out[header.len()..], &[0u8; 144][..]);
    let _ = std::fs::remove_file(f);
}

#[test]
fn characters_go_through_the_cmap() {
    let f = font_file("char");
    let by_glyph = run(&["--ppem", "12", "--glyph", "1", f.to_str().unwrap()]).1;
    let (code, by_char, _) = run(&["--ppem", "12", "--char", "U+0041", f.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(by_char, by_glyph);
    let (code, _, err) = run(&["--ppem", "12", "--char", "U+0042", f.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(err.contains("U+0042"), "{err}");
    let _ = std::fs::remove_file(f);
}

#[test]
fn errors() {
    let f = font_file("errors");
    let path = f.to_str().unwrap();
    // An empty glyph has nothing to show.
    assert_eq!(run(&["--ppem", "12", "--glyph", "2", path]).0, 1);
    // Out of range glyph.
    assert_eq!(run(&["--ppem", "12", "--glyph", "9", path]).0, 1);
    // Usage errors.
    assert_eq!(run(&["--glyph", "1", path]).0, 2);
    assert_eq!(run(&["--ppem", "12", path]).0, 2);
    assert_eq!(
        run(&["--ppem", "12", "--glyph", "1", "--char", "U+0041", path]).0,
        2
    );
    assert_eq!(run(&["--ppem", "x", "--glyph", "1", path]).0, 2);
    assert_eq!(run(&["--ppem", "12", "--glyph", "1"]).0, 2);
    assert_eq!(
        run(&["--ppem", "12", "--glyph", "1", "--face", "3", path]).0,
        1
    );
    let (code, out, _) = run(&["--help"]);
    assert_eq!(code, 0);
    assert!(String::from_utf8(out).unwrap().contains("--ppem"));
    let _ = std::fs::remove_file(f);
}
