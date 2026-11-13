//! avar version 2 partial axis instancing (sofel ADR 0061).

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_read::Tag;
use sofel_subset::{Input, SetKind, subset};
use std::process::Command;

#[test]
fn adr0061_i1_avar2_restrict_same_default_matches_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/TestAvar2Instance.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Unicodes, |set| set.add_range(0, 0x10ffff))
        .with_axis_range(
            Tag::new(b"wght"),
            Some(200.0),
            Some(700.0),
            Some(400.0),
            &font,
        )
        .unwrap();
    let actual = subset(&font, &input).unwrap();
    let output =
        std::env::temp_dir().join(format!("sofel-avar2-partial-{}.ttf", std::process::id()));
    assert!(
        Command::new(paths::hb_dump())
            .arg("--subset")
            .arg(format!("--font-file={}", path.display()))
            .args(["--unicodes=*", "--variations=wght=200:400:700"])
            .arg(format!("--output-file={}", output.display()))
            .status()
            .unwrap()
            .success()
    );
    let expected = std::fs::read(&output).unwrap();
    std::fs::remove_file(output).unwrap();
    assert_eq!(actual, expected);
}
