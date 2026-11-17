//! sofel ADR 0084 I4: the derived `rand` sample font (Caveat with a
//! `rand` feature, built by `font_gallery::derive::rand_sample`) shapes in
//! sofel as in the pinned HarfBuzz, through `hb-dump --shape` (ADR 0020),
//! run with `LC_ALL=C` so that HarfBuzz's language is none, as sofel's.
//! Needs the gallery fonts (`scripts/fetch-fonts.sh gallery-fonts.lock
//! conformance/gallery-fonts.tsv`) and `hb-dump`.

#![allow(
    clippy::all,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::ffi::OsString;
use std::process::Command;

use sofel_conformance::paths;

#[test]
fn adr0084_i4_derived_rand_font_shapes_as_hb_shape() {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let base_path = paths::testdata().join("extra-fonts/fonts/caveat.ttf");
    paths::require(
        &base_path,
        "Caveat (scripts/fetch-fonts.sh gallery-fonts.lock conformance/gallery-fonts.tsv)",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let sha =
        font_gallery::derive::lock_sha256(&paths::root().join("gallery-fonts.lock"), "caveat")
            .unwrap();
    let derived =
        font_gallery::derive::rand_sample(&std::fs::read(&base_path).unwrap(), &sha).unwrap();
    let dir = std::env::temp_dir().join(format!("sofel-adr0084-i4-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let font = dir.join("sofel-rand-sample.ttf");
    std::fs::write(&font, derived).unwrap();
    for (i, text) in ["aaaa eeee oooo tttt", "", "a"].into_iter().enumerate() {
        // One line per file: the random state starts anew in each buffer.
        let text_path = dir.join(format!("text-{i}.txt"));
        std::fs::write(&text_path, format!("{text}\n")).unwrap();
        let out = Command::new(&exe)
            .args([
                OsString::from("--shape"),
                font.clone().into(),
                text_path.clone().into(),
                "auto".into(),
                "".into(),
            ])
            .env("LC_ALL", "C")
            .output()
            .unwrap();
        assert!(out.status.success(), "hb-dump --shape failed");
        let want = String::from_utf8(out.stdout).unwrap();
        let args: Vec<OsString> = vec![
            font.clone().into(),
            "--text-file".into(),
            text_path.into(),
            "--utf8-clusters".into(),
            "--no-glyph-names".into(),
            "--show-flags".into(),
        ];
        let (mut got, mut err) = (Vec::new(), Vec::new());
        let status = sofel_cli::shape::run_with_input(&args, &mut &b""[..], &mut got, &mut err);
        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&err));
        let got = String::from_utf8(got).unwrap();
        assert_eq!(got, want, "{text:?}");
        let glyphs = want.trim().trim_matches(['[', ']']);
        let count = if glyphs.is_empty() {
            0
        } else {
            glyphs.split('|').count()
        };
        assert_eq!(count, text.chars().count(), "{text:?}: {want}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
