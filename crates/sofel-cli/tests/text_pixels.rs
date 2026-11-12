#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

#[path = "../../sofel-shape/tests/support/mod.rs"]
mod support;

use std::path::PathBuf;
use std::process::Command;
use support::build::{Pt, glyf_loca, simple_glyph};
use support::{gdef, ligature_subst, lookup, mark_base, simple_layout};

fn rectangle(width: i16, height: i16) -> Vec<u8> {
    simple_glyph(
        &[&[
            Pt::On(0, 0),
            Pt::On(0, height),
            Pt::On(width, height),
            Pt::On(width, 0),
        ]],
        false,
        &[],
    )
}

fn font() -> PathBuf {
    static FONT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    FONT.get_or_init(build_font).clone()
}

fn build_font() -> PathBuf {
    // One font isolates the chosen GSUB, GPOS and RTL paths from installed fonts.
    // Units per em are 1000, so 10 ppem makes 100 font units equal one pixel.
    let glyphs = [
        Vec::new(),
        rectangle(300, 700), // f
        rectangle(200, 700), // i
        rectangle(500, 700), // fi
        rectangle(400, 700), // A
        rectangle(100, 200), // acute
        rectangle(300, 700), // alef
        rectangle(200, 700), // bet
    ];
    let (glyf, loca) = glyf_loca(&glyphs, false);
    let gsub = simple_layout(
        &[b"liga"],
        vec![lookup(4, 0, vec![ligature_subst(1, &[(&[2], 3)])])],
    );
    let gpos = simple_layout(
        &[b"mark"],
        vec![lookup(
            4,
            0,
            vec![mark_base(&[(5, 0, 0)], &[(4, 200, 700)])],
        )],
    );
    let data = support::font(
        8,
        &[
            (0x41, 4),
            (0x66, 1),
            (0x69, 2),
            (0x0301, 5),
            (0x05D0, 6),
            (0x05D1, 7),
        ],
        &[1000, 400, 300, 600, 500, 0, 400, 300],
        vec![
            (b"GDEF", gdef(&[(4, 1), (5, 3)])),
            (b"GSUB", gsub),
            (b"GPOS", gpos),
            (b"glyf", glyf),
            (b"loca", loca),
        ],
    );
    let path = std::env::temp_dir().join(format!("sofel-pixels-{}.ttf", std::process::id()));
    std::fs::write(&path, data).unwrap();
    path
}

fn render(font: &str, text: &str, extra: &[&str]) -> Vec<u8> {
    let result = Command::new(env!("CARGO_BIN_EXE_sofel"))
        .args(["render", "--ppem", "10", "--text", text])
        .args(extra)
        .arg(font)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

fn expected(lines: &[&str]) -> Vec<u8> {
    let width = lines[0].len();
    let height = lines.len();
    let mut bytes = format!("P5\n{width} {height}\n255\n").into_bytes();
    for line in lines {
        assert_eq!(line.len(), width);
        bytes.extend(line.bytes().map(|pixel| match pixel {
            b'#' => 0,
            b'.' => 255,
            _ => panic!("bad expected pixel"),
        }));
    }
    bytes
}

#[test]
fn adr0071_inv5_ligature_and_disabled_feature_pixels() {
    let path = font();
    let path = path.to_str().unwrap();
    assert_eq!(
        render(path, "fi", &[]),
        expected(&[
            "#####.", "#####.", "#####.", "#####.", "#####.", "#####.", "#####."
        ])
    );
    assert_eq!(
        render(path, "fi", &["--features=-liga"]),
        expected(&[
            "###.##.", "###.##.", "###.##.", "###.##.", "###.##.", "###.##.", "###.##."
        ])
    );
}

#[test]
fn adr0071_inv5_mark_overlap_pixels() {
    let path = font();
    let path = path.to_str().unwrap();
    assert_eq!(
        render(path, "A\u{0301}", &[]),
        expected(&[
            "..#..", "..#..", "####.", "####.", "####.", "####.", "####.", "####.", "####."
        ])
    );
}

#[test]
fn adr0071_inv5_right_to_left_and_spaces() {
    let path = font();
    let path = path.to_str().unwrap();
    assert_eq!(
        render(path, "אב", &[]),
        expected(&[
            "##.###.", "##.###.", "##.###.", "##.###.", "##.###.", "##.###.", "##.###."
        ])
    );
    assert_eq!(
        render(path, " A ", &[]),
        expected(&[
            "..........####...........",
            "..........####...........",
            "..........####...........",
            "..........####...........",
            "..........####...........",
            "..........####...........",
            "..........####..........."
        ])
    );
}
