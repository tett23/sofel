//! `sofel-shape`'s options and output (ADR 0020, テスト設計 `sofel-shape`):
//! the strings hb-shape writes for a known result, and exit status 2
//! for the options sofel-shape does not support.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "../../sofel-shape/tests/support/mod.rs"]
mod support;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use support::build::{Pt, glyf_loca, post2, simple_glyph};
use support::{lookup, pair_pos, simple_layout};

/// Glyph 1 `A` (advance 500, box 0..400 × 0..700), glyph 2 `V`
/// (advance 600), glyph 3 `B.alt` (advance 700); `kern` moves `A`
/// before `V` by -50. With `names`, `post` names the glyphs.
fn font_file(name: &str, names: bool) -> PathBuf {
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
    let (glyf, loca) = glyf_loca(&[Vec::new(), square, Vec::new(), Vec::new()], false);
    let gpos = simple_layout(&[b"kern"], vec![lookup(2, 0, vec![pair_pos(1, 2, -50)])]);
    let mut extra = vec![(b"GPOS", gpos), (b"glyf", glyf), (b"loca", loca)];
    if names {
        // .notdef, A and V are standard Macintosh names.
        extra.push((b"post", post2(&[0, 36, 57, 258], &["B.alt"])));
    }
    let font = support::font(
        4,
        &[(0x41, 1), (0x56, 2), (0x42, 3), (0xE9, 3)],
        &[1000, 500, 600, 700],
        extra,
    );
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-shape-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, font).unwrap();
    p
}

/// Runs the library function as the conformance tests do.
fn shape(font: &PathBuf, args: &[&str]) -> (i32, String, String) {
    let mut all: Vec<OsString> = vec![font.into()];
    all.extend(args.iter().map(OsString::from));
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = sofel_cli::shape::run_with_input(&all, &mut &b""[..], &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn adr0063_i2_synthetic_vertical_shape_matches_harfbuzz() {
    let dir = std::env::var_os("SOFEL_TESTDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata"));
    let font = dir.join("harfbuzz/test/shape/data/in-house/fonts/NotoSans-VF.abc.ttf");
    if !font.exists() {
        assert!(std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_none());
        return;
    }
    for (option, expected) in [
        (
            "--font-slant=0.5",
            "[gid1=0@-280,-948+0,-1362|gid2=1@-307,-1056+0,-1362|gid3=2@-240,-949+0,-1362]",
        ),
        (
            "--font-bold=0.1",
            "[gid1=0@-430,-1148+0,-1362|gid2=1@-457,-1256+0,-1362|gid3=2@-390,-1149+0,-1362]",
        ),
        (
            "--font-grade=0.1",
            "[gid1=0@-280,-1048+0,-1462|gid2=1@-307,-1156+0,-1462|gid3=2@-240,-1049+0,-1462]",
        ),
    ] {
        let (code, out, err) = shape(&font, &[option, "--direction=ttb", "--unicodes=61,62,63"]);
        assert_eq!(code, 0, "{option}: {err}");
        assert_eq!(out.trim(), expected, "{option}");
    }
    let (code, out, err) = shape(
        &font,
        &[
            "--font-bold=0.1",
            "--direction=ttb",
            "--show-extents",
            "--unicodes=61,62,63",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        out.trim(),
        "[gid1=0@-430,-1148+0,-1362<46,645,534,-655>|gid2=1@-457,-1256+0,-1362<85,860,575,-870>|gid3=2@-390,-1149+0,-1362<55,646,504,-656>]"
    );
}

fn ok(font: &PathBuf, args: &[&str]) -> String {
    let (code, out, err) = shape(font, args);
    assert_eq!(code, 0, "{err}");
    out
}

#[test]
fn writes_names_clusters_and_positions() {
    let f = font_file("plain", true);
    assert_eq!(ok(&f, &["AVB"]), "[A=0+450|V=1+600|B.alt=2+700]\n");
    assert_eq!(
        ok(&f, &["--no-glyph-names", "AVB"]),
        "[1=0+450|2=1+600|3=2+700]\n"
    );
    assert_eq!(ok(&f, &["--no-positions", "AVB"]), "[A=0|V=1|B.alt=2]\n");
    assert_eq!(
        ok(&f, &["--no-clusters", "AVB"]),
        "[A+450|V+600|B.alt+700]\n"
    );
}

#[test]
fn glyphs_without_names_are_gids() {
    let f = font_file("nameless", false);
    assert_eq!(ok(&f, &["AV"]), "[gid1=0+450|gid2=1+600]\n");
}

#[test]
fn no_advances_sums_the_advances_into_the_offsets() {
    let f = font_file("advances", true);
    assert_eq!(
        ok(&f, &["--no-advances", "AVB"]),
        "[A=0|V=1@450,0|B.alt=2@1050,0]\n"
    );
    assert_eq!(ok(&f, &["--ned", "AVB"]), "[A|V@450,0|B.alt@1050,0]\n");
}

#[test]
fn shows_flags_and_extents() {
    let f = font_file("flags", true);
    // The kerned pair is unsafe to break before V.
    assert_eq!(
        ok(&f, &["--show-flags", "AVB"]),
        "[A=0+450|V=1+600#1|B.alt=2+700]\n"
    );
    assert_eq!(
        ok(&f, &["--show-flags", "--unsafe-to-concat", "AV"]),
        "[A=0+450|V=1+600#3]\n"
    );
    // V is an empty glyph: an empty box.
    assert_eq!(
        ok(&f, &["--show-extents", "AV"]),
        "[A=0+450<0,700,400,-700>|V=1+600<0,0,0,0>]\n"
    );
    assert_eq!(
        ok(&f, &["--show-extents", "--font-size=2000", "A"]),
        "[A=0+1000<0,1400,800,-1400>]\n"
    );
}

#[test]
fn writes_json() {
    let f = font_file("json", true);
    assert_eq!(
        ok(&f, &["-O", "json", "--show-flags", "AV"]),
        "[{\"g\":\"A\",\"cl\":0,\"dx\":0,\"dy\":0,\"ax\":450,\"ay\":0},\
         {\"g\":\"V\",\"cl\":1,\"dx\":0,\"dy\":0,\"ax\":600,\"ay\":0,\"fl\":1}]\n"
    );
    assert_eq!(
        ok(&f, &["--output-format=JSON", "--ned", "B"]),
        "[{\"g\":\"B.alt\",\"dx\":0,\"dy\":0}]\n"
    );
}

#[test]
fn clusters_are_character_indices_unless_utf8_clusters() {
    let f = font_file("clusters", true);
    assert_eq!(ok(&f, &["--no-positions", "éA"]), "[B.alt=0|A=1]\n");
    assert_eq!(
        ok(&f, &["--no-positions", "--utf8-clusters", "éA"]),
        "[B.alt=0|A=2]\n"
    );
}

#[test]
fn reads_unicodes_lines_and_text_files() {
    let f = font_file("lines", true);
    assert_eq!(
        ok(&f, &["--unicodes", "U+0041,U+0056", "--ned"]),
        "[A|V@450,0]\n"
    );
    assert_eq!(ok(&f, &["-u", "41 56", "--ned"]), "[A|V@450,0]\n");
    // A line per line of text; an empty line writes an empty line.
    assert_eq!(ok(&f, &["--ned", "--text=A\n\nV\n"]), "[A]\n\n[V]\n");
    assert_eq!(
        ok(&f, &["--ned", "--show-line-num", "--text=A\nV"]),
        "1: [A]\n2: [V]\n"
    );
    assert_eq!(
        ok(&f, &["--ned", "--show-text", "--show-unicode", "A"]),
        "(A)\n<U+0041=0>\n[A]\n"
    );
    let text = f.with_extension("txt");
    std::fs::write(&text, "AV\nB\n").unwrap();
    assert_eq!(
        ok(
            &f,
            &["--no-positions", "--text-file", text.to_str().unwrap()]
        ),
        "[A=0|V=1]\n[B.alt=0]\n"
    );
    // Standard input when no text is given.
    let mut out = Vec::new();
    let code = sofel_cli::shape::run_with_input(
        &[f.clone().into(), "--ned".into()],
        &mut &b"V\n"[..],
        &mut out,
        &mut Vec::new(),
    );
    assert_eq!(
        (code, String::from_utf8(out).unwrap()),
        (0, "[V]\n".to_string())
    );
}

#[test]
fn features_scale_shapers_and_verify() {
    let f = font_file("features", true);
    assert_eq!(ok(&f, &["--features=-kern", "AV"]), "[A=0+500|V=1+600]\n");
    // Both glyphs of the pair must be in the range.
    assert_eq!(
        ok(&f, &["--features=\"-kern,kern[1:]\"", "AV"]),
        "[A=0+500|V=1+600]\n"
    );
    assert_eq!(
        ok(&f, &["--features=-kern kern[:1]", "AV"]),
        "[A=0+500|V=1+600]\n"
    );
    assert_eq!(
        ok(&f, &["--features=-kern kern[:2]", "AV"]),
        "[A=0+450|V=1+600]\n"
    );
    assert_eq!(ok(&f, &["--font-size=2000", "AV"]), "[A=0+900|V=1+1200]\n");
    assert_eq!(ok(&f, &["--shaper=fallback", "AV"]), "[A=0+500|V=1+600]\n");
    assert_eq!(
        ok(&f, &["--shapers=ot,fallback", "AV"]),
        "[A=0+450|V=1+600]\n"
    );
    assert_eq!(
        ok(
            &f,
            &["--verify", "--unsafe-to-concat", "--direction=rtl", "AVB"]
        ),
        "[B.alt=2+700|V=1+600|A=0+500]\n"
    );
    assert_eq!(
        ok(&f, &["--font-funcs=ft", "--face-loader=ot", "A"]),
        "[A=0+500]\n"
    );
}

#[test]
fn rewrites_glyph_strings() {
    let f = font_file("glyphs", true);
    // Advances from the font when the string has no positions.
    assert_eq!(ok(&f, &["--glyphs", "[A=0|V=1]"]), "[A=0+500|V=1+600]\n");
    assert_eq!(
        ok(
            &f,
            &["--glyphs", "--no-glyph-names", "[A=0+10|gid3=1@5,0+20]"]
        ),
        "[1=0+10|3=1@5,0+20]\n"
    );
    // The closing bracket may be left out.
    assert_eq!(ok(&f, &["--glyphs", "--ned", "[V"]), "[V]\n");
}

#[test]
fn unsupported_options_and_bad_usage_exit_with_2() {
    let f = font_file("usage", true);
    for args in [
        &["--variations=wght=bad", "A"][..],
        &["--named-instance=65536", "A"],
        &["--font-ptem=bad", "A"],
        &["--font-slant=bad", "A"],
        &["--font-bold=bad", "A"],
        &["--font-grade=bad", "A"],
        &["--shaper=coretext", "A"],
        &["--shapers=ot,graphite2", "A"],
        &["--no-such-option", "A"],
        &["--cluster-level=4", "A"],
        &["--invisible-glyph=x", "A"],
        &["--text=A", "--unicodes=41"],
        &["A", "B"],
    ] {
        let (code, out, err) = shape(&f, args);
        assert_eq!(code, 2, "{args:?}: {out} {err}");
        assert!(out.is_empty() && !err.is_empty(), "{args:?}");
    }
    assert_eq!(ok(&f, &["--variations=wght=500", "A"]), "[A=0+500]\n");
    assert_eq!(ok(&f, &["--named-instance=1", "A"]), "[A=0+500]\n");
    let (code, _, _) = shape(&PathBuf::from("/nonexistent/font.ttf"), &["A"]);
    assert_eq!(code, 1);
}

#[test]
fn the_binary_runs() {
    let f = font_file("binary", true);
    let o = Command::new(env!("CARGO_BIN_EXE_sofel-shape"))
        .arg(&f)
        .arg("--ned")
        .arg("AV")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(String::from_utf8(o.stdout).unwrap(), "[A|V@450,0]\n");
    let o = Command::new(env!("CARGO_BIN_EXE_sofel-shape"))
        .arg("--variations=wght=1")
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
}
