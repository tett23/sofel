//! HarfBuzz byte equality for VARC records without auxiliary data.

#![allow(clippy::all)]

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_subset::{Flags, Input, SetKind, subset};
use std::process::Command;

#[test]
fn adr0067_i2_short_cff2_reassigned_and_retained_gids() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/varc-short-cff2.otf");
    let data = std::fs::read(&path).unwrap();
    let font = Font::new(&data, 0).unwrap();
    for retain in [false, true] {
        let flags = if retain {
            Flags::RETAIN_GIDS
        } else {
            Flags::DEFAULT
        };
        let input = Input::new()
            .with_flags(flags)
            .map_set(SetKind::Glyphs, |glyphs| glyphs.add(3));
        let ours = subset(&font, &input).unwrap();
        let reference =
            std::env::temp_dir().join(format!("sofel-varc-{retain}-{}-hb.otf", std::process::id()));
        let mut command = Command::new(paths::hb_dump());
        command
            .arg("--subset")
            .arg(format!("--font-file={}", path.display()))
            .arg("--gids=3")
            .arg(format!("--output-file={}", reference.display()));
        if retain {
            command.arg("--retain-gids");
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        let expected = std::fs::read(&reference).unwrap();
        let _ = std::fs::remove_file(&reference);
        assert_eq!(ours, expected, "retain={retain}");
    }
}
