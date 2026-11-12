//! ADR 0061 I1: gvar sources with no glyph deltas still support axis instancing.

#![allow(clippy::arithmetic_side_effects, clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{sample_tables, sfnt};
use sofel_conformance::paths;
use sofel_font::Font;
use sofel_font::read::Tag;
use sofel_subset::{Flags, Input, SetKind, subset};
use std::process::Command;

fn inert_gvar_font() -> Vec<u8> {
    let mut tables = sample_tables();
    let glyph_count = Font::new(&sfnt(&tables), 0).unwrap().num_glyphs();
    let mut fvar = vec![0, 1, 0, 0];
    fvar.extend(16u16.to_be_bytes());
    fvar.extend(2u16.to_be_bytes());
    fvar.extend(1u16.to_be_bytes());
    fvar.extend(20u16.to_be_bytes());
    fvar.extend(0u16.to_be_bytes());
    fvar.extend(8u16.to_be_bytes());
    fvar.extend(b"wght");
    for value in [100i32, 400, 900] {
        fvar.extend((value << 16).to_be_bytes());
    }
    fvar.extend(0u16.to_be_bytes());
    fvar.extend(256u16.to_be_bytes());
    let mut gvar = vec![0, 1, 0, 0];
    gvar.extend(1u16.to_be_bytes());
    gvar.extend(0u16.to_be_bytes());
    gvar.extend(0u32.to_be_bytes());
    gvar.extend(u16::try_from(glyph_count).unwrap().to_be_bytes());
    gvar.extend(0u16.to_be_bytes());
    gvar.extend((20 + (glyph_count + 1) * 2).to_be_bytes());
    gvar.extend(vec![0; usize::try_from((glyph_count + 1) * 2).unwrap()]);
    tables.push((b"fvar", fvar));
    tables.push((b"gvar", gvar));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn adr0061_i1_inert_gvar_nondefault_pin_and_range_match_harfbuzz() {
    let source = std::env::temp_dir().join(format!("sofel-inert-gvar-{}.ttf", std::process::id()));
    let output =
        std::env::temp_dir().join(format!("sofel-inert-gvar-out-{}.ttf", std::process::id()));
    let data = inert_gvar_font();
    std::fs::write(&source, &data).unwrap();
    let font = Font::new(&data, 0).unwrap();
    for retain in [false, true] {
        let base = Input::new()
            .with_flags(if retain {
                Flags::RETAIN_GIDS
            } else {
                Flags::DEFAULT
            })
            .map_set(SetKind::Glyphs, |set| set.add(1));
        for (variation, input) in [
            (
                "wght=650",
                base.with_axis_pinned(Tag::new(b"wght"), 650.0, &font)
                    .unwrap(),
            ),
            (
                "wght=200:650:800",
                base.with_axis_range(
                    Tag::new(b"wght"),
                    Some(200.0),
                    Some(800.0),
                    Some(650.0),
                    &font,
                )
                .unwrap(),
            ),
        ] {
            let ours = subset(&font, &input).unwrap();
            let mut reference = Command::new(paths::hb_dump());
            reference
                .arg("--subset")
                .arg(format!("--font-file={}", source.display()))
                .arg("--gids=1")
                .arg(format!("--variations={variation}"))
                .arg(format!("--output-file={}", output.display()));
            if retain {
                reference.arg("--retain-gids");
            }
            assert!(reference.status().unwrap().success());
            assert_eq!(
                ours,
                std::fs::read(&output).unwrap(),
                "{variation}, retain={retain}"
            );
        }
    }
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(output);
}
