#![allow(clippy::all, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../sofel-shape/tests/support/mod.rs"]
mod support;

use std::path::PathBuf;
use std::process::Command;
use support::build::{Pt, glyf_loca, simple_glyph};
use support::{lookup, pair_pos, simple_layout};

fn font() -> PathBuf {
    static FONT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    FONT.get_or_init(build_font).clone()
}

fn build_font() -> PathBuf {
    let square = simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, 700),
            Pt::On(400, 700),
            Pt::On(400, 0),
        ]],
        false,
        &[],
    );
    let (glyf, loca) = glyf_loca(&[Vec::new(), square.clone(), square], false);
    let gpos = simple_layout(&[b"kern"], vec![lookup(2, 0, vec![pair_pos(1, 2, -50)])]);
    let bytes = support::font(
        3,
        &[(0x41, 1), (0x56, 2)],
        &[1000, 500, 600],
        vec![(b"GPOS", gpos), (b"glyf", glyf), (b"loca", loca)],
    );
    let path = std::env::temp_dir().join(format!("sofel-text-font-{}.ttf", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sofel"))
        .arg("render")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn inv5_text_pixels() {
    let f = font();
    let f = f.to_str().unwrap();
    let empty = run(&["--ppem", "48", "--text", "", f]);
    assert!(
        empty.status.success(),
        "{}",
        String::from_utf8_lossy(&empty.stderr)
    );
    assert_eq!(empty.stdout, b"P5\n1 1\n255\n\xff");
    let one = run(&["--ppem", "10", "--text", "A", f]);
    let mut expected = b"P5\n5 7\n255\n".to_vec();
    for _ in 0..7 {
        expected.extend([0, 0, 0, 0, 255]);
    }
    assert_eq!(one.stdout, expected);
    let space = run(&["--ppem", "10", "--text", " ", f]);
    assert_eq!(
        space.stdout,
        b"P5\n10 1\n255\n\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff"
    );
    let a = run(&["--ppem", "48", "--text", "AV", f]);
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    assert!(a.stdout.starts_with(b"P5\n"));
    assert!(a.stdout.len() > 100);
    let no_liga = run(&["--ppem", "48", "--features=-kern", "--text", "AV", f]);
    assert!(no_liga.status.success());
    assert_ne!(a.stdout, no_liga.stdout);
}

#[test]
fn inv6_text_usage() {
    let f = font();
    let f = f.to_str().unwrap();
    for args in [
        vec!["--ppem", "48", "--text", "A", "--text", "B", f],
        vec!["--ppem", "48", "--text", "A", "--glyph", "1", f],
        vec!["--ppem", "48", "--text", "A", "--color", f],
        vec!["--ppem", "48", "--text", "A", "--palette", "0", f],
        vec!["--ppem", "48", "--text", "A", "--text-file", "x", f],
        vec!["--ppem", "48", "--text", "A\nB", f],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(o.stdout.is_empty());
        assert!(!o.stderr.is_empty());
    }
}

#[test]
fn inv7_text_file_parity() {
    let f = font();
    let f = f.to_str().unwrap();
    let path = std::env::temp_dir().join(format!("sofel-text-{}.txt", std::process::id()));
    let arg = run(&["--ppem", "48", "--text", " office ", f]);
    assert!(arg.status.success());
    for contents in [b" office ".as_slice(), b" office \n", b" office \r\n"] {
        std::fs::write(&path, contents).unwrap();
        let o = run(&["--ppem", "48", "--text-file", path.to_str().unwrap(), f]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(arg.stdout, o.stdout);
    }
    std::fs::write(&path, b"A\nB").unwrap();
    let o = run(&["--ppem", "48", "--text-file", path.to_str().unwrap(), f]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
    std::fs::write(&path, [0xff]).unwrap();
    let o = run(&["--ppem", "48", "--text-file", path.to_str().unwrap(), f]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn inv8_text_file_read_error() {
    let path = format!("/nonexistent/sofel-text-{}.txt", std::process::id());
    let o = run(&[
        "--ppem",
        "48",
        "--text-file",
        &path,
        font().to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    assert!(String::from_utf8_lossy(&o.stderr).contains(&path));
}

#[test]
fn inv3_legacy_render_parity_without_text() {
    let f = font();
    let f = f.to_str().unwrap();
    for args in [
        vec!["--ppem", "10", "--glyph", "1", f],
        vec!["--ppem", "10", "--features=-kern", "--glyph", "1", f],
        vec!["--ppem", "10", "--glyph", "--text", f],
        vec!["--face", "--text", "--ppem", "10", "--glyph", "1", f],
    ] {
        let unified = run(&args);
        let legacy = Command::new(env!("CARGO_BIN_EXE_sofel-render"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(unified.status.code(), legacy.status.code(), "{args:?}");
        assert_eq!(unified.stdout, legacy.stdout, "{args:?}");
        assert_eq!(unified.stderr, legacy.stderr, "{args:?}");
    }
}
