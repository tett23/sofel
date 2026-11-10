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

/// The square font of `font_file` with a 12-ppem strike of `depth` bits
/// for glyph 1: a 3×2 image (small metrics, byte-aligned).
fn bitmap_font(name: &str, depth: u8, pixels: &[u8]) -> PathBuf {
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
    let image = Buf::new().bytes(&[2, 3, 1, 2, 4]).bytes(pixels).done();
    let data = Buf::new().u16(2).u16(0).bytes(&image).done();
    let loc = cblc(
        2,
        &[SizeSpec {
            ppem_x: 12,
            ppem_y: 12,
            bit_depth: depth,
            start_glyph: 1,
            end_glyph: 1,
            subtables: vec![(1, 1, index_offsets(1, 1, 4, &[0, image.len() as u32]))],
            ..SizeSpec::default()
        }],
    );
    let font = glyph_font_with(
        &[Vec::new(), square, Vec::new()],
        true,
        false,
        vec![(b"EBDT", data), (b"EBLC", loc)],
    );
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-render-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, font).unwrap();
    p
}

#[test]
fn bitmaps_win_at_their_size() {
    // 1 bit: rows 101 and 010.
    let f = bitmap_font("mono", 1, &[0b1010_0000, 0b0100_0000]);
    let path = f.to_str().unwrap();
    let (code, out, err) = run(&["--ppem", "12", "--glyph", "1", path]);
    assert_eq!(code, 0, "{err}");
    let header = b"P5\n# left 1 top 2\n3 2\n255\n";
    assert_eq!(&out[..header.len()], header);
    assert_eq!(&out[header.len()..], &[0, 255, 0, 255, 0, 255]);
    // --no-bitmap and other sizes draw the outline.
    let (_, out, _) = run(&["--ppem", "12", "--no-bitmap", "--glyph", "1", path]);
    assert!(out.starts_with(b"P5\n# left 0 top 12\n12 12\n"));
    let (_, out, _) = run(&["--ppem", "13", "--glyph", "1", path]);
    assert!(out.starts_with(b"P5\n# left 0 top 13\n"));
    let _ = std::fs::remove_file(f);

    // 32 bits: premultiplied BGRA written as PAM, not premultiplied.
    let px = [
        0, 0, 128, 128, 255, 0, 0, 255, 0, 0, 0, 0, 0, 64, 0, 128, 1, 2, 3, 255, 0, 0, 0, 0,
    ];
    let f = bitmap_font("bgra", 32, &px);
    let (code, out, err) = run(&["--ppem", "12", "--glyph", "1", f.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    let header =
        b"P7\n# left 1 top 2\nWIDTH 3\nHEIGHT 2\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n";
    assert_eq!(&out[..header.len()], header);
    assert_eq!(
        &out[header.len()..],
        &[
            255, 0, 0, 128, 0, 0, 255, 255, 0, 0, 0, 0, 0, 128, 0, 128, 3, 2, 1, 255, 0, 0, 0, 0
        ]
    );
    let _ = std::fs::remove_file(f);
}
