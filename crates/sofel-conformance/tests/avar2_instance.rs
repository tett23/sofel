//! avar version 2 partial axis instancing (sofel ADR 0061).
#![allow(
    clippy::all,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic
)]

use sofel_conformance::paths;
use sofel_font::Font;
use sofel_read::Tag;
use sofel_subset::{Input, SetKind, subset};
use std::process::Command;

#[test]
fn adr0061_i1_passthrough_hvar_with_axis_pin_matches_harfbuzz() {
    compare_passthrough_table(b"HVAR");
}

#[test]
fn adr0061_i1_passthrough_gdef_with_axis_pin_matches_harfbuzz() {
    compare_passthrough_table(b"GDEF");
}

#[test]
fn adr0061_i1_passthrough_gpos_with_axis_pin_matches_harfbuzz() {
    compare_passthrough_table(b"GPOS");
}

fn compare_passthrough_table(tag: &[u8; 4]) {
    let name = std::str::from_utf8(tag).unwrap();
    let path = paths::testdata().join("harfbuzz/test/subset/data/fonts/KalniaGlaze.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff))
        .map_set(SetKind::NoSubsetTables, |s| s.add(Tag::new(tag).to_u32()))
        .with_axis_pinned(Tag::new(b"wght"), 600.0, &font)
        .unwrap();
    let actual = subset(&font, &input).unwrap();
    let output = std::env::temp_dir().join(format!(
        "sofel-adr0061-passthrough-{name}-{}.ttf",
        std::process::id()
    ));
    assert!(
        Command::new(paths::hb_dump())
            .arg("--subset")
            .arg(format!("--font-file={}", path.display()))
            .arg("--unicodes=*")
            .arg(format!("--no-subset-tables+={name}"))
            .arg("--variations=wght=600")
            .arg(format!("--output-file={}", output.display()))
            .status()
            .unwrap()
            .success()
    );
    let expected = std::fs::read(&output).unwrap();
    std::fs::remove_file(output).unwrap();
    if actual != expected {
        let got = Font::new(&actual, 0).unwrap();
        let want = Font::new(&expected, 0).unwrap();
        let differences = want
            .sfnt()
            .records()
            .filter_map(|record| {
                let a = got.table(record.tag).unwrap_or(&[]);
                let b = want.table(record.tag).unwrap_or(&[]);
                (a != b).then(|| {
                    (
                        record.tag,
                        a.len(),
                        b.len(),
                        a.iter()
                            .zip(b)
                            .position(|(x, y)| x != y)
                            .unwrap_or(a.len().min(b.len())),
                    )
                })
            })
            .collect::<Vec<_>>();
        panic!("{name}: {differences:?}");
    }
}

#[test]
fn adr0061_i1_avar2_passthrough_tables_match_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/TestAvar2Instance.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    for tag in [b"HVAR", b"MVAR", b"gvar", b"avar", b"fvar"] {
        for (min, default, max) in [(600.0, 600.0, 600.0), (200.0, 400.0, 700.0)] {
            let name = std::str::from_utf8(tag).unwrap();
            let input = Input::new()
                .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff))
                .map_set(SetKind::NoSubsetTables, |s| s.add(Tag::new(tag).to_u32()))
                .with_axis_range(
                    Tag::new(b"wght"),
                    Some(min),
                    Some(max),
                    Some(default),
                    &font,
                )
                .unwrap();
            let actual = subset(&font, &input).unwrap();
            let output = std::env::temp_dir().join(format!(
                "sofel-adr0061-avar2-passthrough-{name}-{min}-{default}-{max}-{}.ttf",
                std::process::id()
            ));
            assert!(
                Command::new(paths::hb_dump())
                    .arg("--subset")
                    .arg(format!("--font-file={}", path.display()))
                    .arg("--unicodes=*")
                    .arg(format!("--no-subset-tables+={name}"))
                    .arg(format!("--variations=wght={min}:{default}:{max}"))
                    .arg(format!("--output-file={}", output.display()))
                    .status()
                    .unwrap()
                    .success()
            );
            let expected = std::fs::read(&output).unwrap();
            std::fs::remove_file(output).unwrap();
            if actual != expected {
                let got = Font::new(&actual, 0).unwrap();
                let want = Font::new(&expected, 0).unwrap();
                let differences = want
                    .sfnt()
                    .records()
                    .filter_map(|record| {
                        let a = got.table(record.tag).unwrap_or(&[]);
                        let b = want.table(record.tag).unwrap_or(&[]);
                        (a != b).then(|| {
                            (
                                record.tag,
                                a.len(),
                                b.len(),
                                a.iter()
                                    .zip(b)
                                    .position(|(x, y)| x != y)
                                    .unwrap_or(a.len().min(b.len())),
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                panic!("{name} {min}:{default}:{max}: {differences:?}");
            }
        }
    }
}

#[test]
fn adr0061_i1_avar2_passthrough_multiple_tables_and_axes_match_harfbuzz() {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/TestAvar2Instance.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff))
        .map_set(SetKind::NoSubsetTables, |s| {
            for tag in [b"HVAR", b"MVAR", b"gvar"] {
                s.add(Tag::new(tag).to_u32());
            }
        })
        .with_axis_range(
            Tag::new(b"wght"),
            Some(200.0),
            Some(700.0),
            Some(400.0),
            &font,
        )
        .unwrap()
        .with_axis_pinned(Tag::new(b"XOPQ"), 88.0, &font)
        .unwrap();
    let actual = subset(&font, &input).unwrap();
    let output = std::env::temp_dir().join(format!(
        "sofel-adr0061-avar2-passthrough-mixed-{}.ttf",
        std::process::id()
    ));
    assert!(
        Command::new(paths::hb_dump())
            .arg("--subset")
            .arg(format!("--font-file={}", path.display()))
            .arg("--unicodes=*")
            .arg("--no-subset-tables+=HVAR,MVAR,gvar")
            .arg("--variations=wght=200:400:700,XOPQ=88")
            .arg(format!("--output-file={}", output.display()))
            .status()
            .unwrap()
            .success()
    );
    let expected = std::fs::read(&output).unwrap();
    std::fs::remove_file(output).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn adr0061_i1_avar2_passthrough_layout_tables_match_harfbuzz() {
    let path =
        paths::testdata().join("harfbuzz/test/subset/data/fonts/RobotoDelta-NullVarStore-VF.ttf");
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    for tag in [b"GPOS", b"GDEF"] {
        let name = std::str::from_utf8(tag).unwrap();
        let input = Input::new()
            .map_set(SetKind::Unicodes, |s| s.add_range(0, 0x10ffff))
            .map_set(SetKind::NoSubsetTables, |s| s.add(Tag::new(tag).to_u32()))
            .with_axis_range(
                Tag::new(b"wght"),
                Some(300.0),
                Some(600.0),
                Some(400.0),
                &font,
            )
            .unwrap();
        let actual = subset(&font, &input).unwrap();
        let output = std::env::temp_dir().join(format!(
            "sofel-adr0061-avar2-passthrough-layout-{name}-{}.ttf",
            std::process::id()
        ));
        assert!(
            Command::new(paths::hb_dump())
                .arg("--subset")
                .arg(format!("--font-file={}", path.display()))
                .arg("--unicodes=*")
                .arg(format!("--no-subset-tables+={name}"))
                .arg("--variations=wght=300:400:600")
                .arg(format!("--output-file={}", output.display()))
                .status()
                .unwrap()
                .success()
        );
        let expected = std::fs::read(&output).unwrap();
        std::fs::remove_file(output).unwrap();
        if actual != expected {
            let got = Font::new(&actual, 0).unwrap();
            let want = Font::new(&expected, 0).unwrap();
            let differences = want
                .sfnt()
                .records()
                .filter_map(|record| {
                    let a = got.table(record.tag).unwrap_or(&[]);
                    let b = want.table(record.tag).unwrap_or(&[]);
                    (a != b).then(|| {
                        (
                            record.tag,
                            a.len(),
                            b.len(),
                            a.iter()
                                .zip(b)
                                .position(|(x, y)| x != y)
                                .unwrap_or(a.len().min(b.len())),
                        )
                    })
                })
                .collect::<Vec<_>>();
            panic!("{name}: {differences:?}");
        }
    }
}

#[test]
fn adr0061_i1_avar2_restrict_same_default_matches_harfbuzz() {
    compare(b"wght", 200.0, 400.0, 700.0);
}

#[test]
fn adr0061_i1_avar2_restrict_moved_default_matches_harfbuzz() {
    compare(b"wght", 190.0, 550.0, 900.0);
}

#[test]
fn adr0061_i1_avar2_restrict_moved_default_other_side_matches_harfbuzz() {
    compare(b"wght", 100.0, 250.0, 500.0);
}

#[test]
fn adr0061_i1_avar2_restrict_axis_without_varidx_matches_harfbuzz() {
    compare(b"wdth", 60.0, 120.0, 200.0);
}

#[test]
fn adr0061_i1_avar2_restrict_shared_row_matches_harfbuzz() {
    compare(b"XOPQ", 20.0, 120.0, 160.0);
}

#[test]
fn adr0061_i1_avar2_pin_shared_row_matches_harfbuzz() {
    compare(b"XOPQ", 120.0, 120.0, 120.0);
}

#[test]
fn adr0061_i1_avar2_pin_driven_axis_matches_harfbuzz() {
    compare(b"wght", 600.0, 600.0, 600.0);
}

#[test]
fn adr0061_i1_avar2_pin_shared_row_at_default_matches_harfbuzz() {
    compare(b"XOPQ", 88.0, 88.0, 88.0);
}

#[test]
fn adr0061_i1_avar2_pin_self_contained_axis_matches_harfbuzz() {
    compare(b"opsz", 40.0, 40.0, 40.0);
}

#[test]
fn adr0061_i1_avar2_restrict_steep_shared_axis_matches_harfbuzz() {
    compare(b"XOPQ", 10.0, 140.0, 160.0);
}

#[test]
fn adr0061_i1_avar2_restrict_steep_no_varidx_matches_harfbuzz() {
    compare(b"opsz", 8.0, 120.0, 144.0);
}

#[test]
fn adr0061_i1_avar2_restrict_culls_unreachable_regions() {
    compare(b"wght", 300.0, 400.0, 500.0);
}

#[test]
fn adr0061_i1_avar2_two_shared_ranges_match_harfbuzz() {
    compare_axes(&[(b"GRAD", -100.0, 0.0, 100.0), (b"XOPQ", 30.0, 88.0, 140.0)]);
}

#[test]
fn adr0061_i1_avar2_self_contained_pin_plus_range_match_harfbuzz() {
    compare_axes(&[
        (b"wdth", 150.0, 150.0, 150.0),
        (b"wght", 250.0, 400.0, 700.0),
    ]);
}

#[test]
fn adr0061_i1_avar2_moved_default_plus_self_contained_pin_matches_harfbuzz() {
    compare_axes(&[
        (b"wght", 190.0, 550.0, 900.0),
        (b"wdth", 150.0, 150.0, 150.0),
    ]);
}

#[test]
fn adr0061_i1_avar2_restricts_cff2_hvar_and_gdef_like_harfbuzz() {
    let source = paths::testdata().join("harfbuzz/test/subset/data/fonts/AdobeVFPrototype.otf");
    let bytes = std::fs::read(source).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    for has_store in [false, true] {
        let mut avar = font.table(Tag::new(b"avar")).unwrap().to_vec();
        avar[..2].copy_from_slice(&2u16.to_be_bytes());
        avar.extend(0u32.to_be_bytes()); // No explicit DeltaSetIndexMap.
        if has_store {
            avar.extend(u32::try_from(avar.len() + 4).unwrap().to_be_bytes());
            avar.extend(1u16.to_be_bytes()); // ItemVariationStore format 1.
            avar.extend(12u32.to_be_bytes()); // RegionList offset.
            avar.extend(1u16.to_be_bytes()); // One ItemVariationData.
            avar.extend(28u32.to_be_bytes()); // ItemVariationData offset.
            avar.extend(2u16.to_be_bytes()); // Two axes.
            avar.extend(1u16.to_be_bytes()); // One region.
            for axis in [[0i16, 16384, 16384], [0, 0, 0]] {
                for value in axis {
                    avar.extend(value.to_be_bytes());
                }
            }
            for value in [2u16, 1, 1, 0, 1024, 0] {
                avar.extend(value.to_be_bytes());
            }
        } else {
            avar.extend(0u32.to_be_bytes());
        }
        let tables = font
            .sfnt()
            .records()
            .map(|r| {
                (
                    r.tag,
                    if r.tag == Tag::new(b"avar") {
                        avar.clone()
                    } else {
                        r.data(&bytes).unwrap().to_vec()
                    },
                )
            })
            .collect::<Vec<_>>();
        let refs = tables
            .iter()
            .map(|(tag, bytes)| (*tag, bytes.as_slice()))
            .collect::<Vec<_>>();
        let modified = sofel_write::build_sfnt(&refs).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sofel-adr61-cff2-avar2-{has_store}-{}.otf",
            std::process::id()
        ));
        std::fs::write(&path, modified).unwrap();
        for ranges in [
            vec![(b"wght", 300.0, 500.0, 700.0)],
            vec![(b"wght", 600.0, 600.0, 600.0)],
            vec![(b"CNTR", 50.0, 50.0, 50.0)],
            vec![(b"wght", 300.0, 500.0, 700.0), (b"CNTR", 50.0, 50.0, 50.0)],
            vec![(b"wght", 600.0, 600.0, 600.0), (b"CNTR", 50.0, 50.0, 50.0)],
            vec![(b"wght", 200.0, 200.0, 200.0), (b"CNTR", 0.0, 0.0, 0.0)],
            vec![
                (b"wght", 900.0, 900.0, 900.0),
                (b"CNTR", 100.0, 100.0, 100.0),
            ],
        ] {
            compare_path(&path, &ranges);
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn adr0061_i1_avar2_restricts_variable_colr_like_harfbuzz() {
    let source = paths::testdata().join("harfbuzz/test/subset/data/fonts/KalniaGlaze.ttf");
    let bytes = std::fs::read(source).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let mut avar = font.table(Tag::new(b"avar")).unwrap().to_vec();
    avar[..2].copy_from_slice(&2u16.to_be_bytes());
    avar.extend([0; 8]);
    let tables = font
        .sfnt()
        .records()
        .map(|r| {
            (
                r.tag,
                if r.tag == Tag::new(b"avar") {
                    avar.clone()
                } else {
                    r.data(&bytes).unwrap().to_vec()
                },
            )
        })
        .collect::<Vec<_>>();
    let refs = tables
        .iter()
        .map(|(tag, bytes)| (*tag, bytes.as_slice()))
        .collect::<Vec<_>>();
    let modified = sofel_write::build_sfnt(&refs).unwrap();
    let path =
        std::env::temp_dir().join(format!("sofel-adr61-colr-avar2-{}.ttf", std::process::id()));
    std::fs::write(&path, modified).unwrap();
    for ranges in [
        vec![(b"wght", 200.0, 400.0, 600.0)],
        vec![(b"wght", 400.0, 400.0, 600.0)],
        vec![(b"wdth", 112.0, 112.0, 112.0)],
        vec![
            (b"wght", 200.0, 400.0, 600.0),
            (b"wdth", 112.0, 112.0, 112.0),
        ],
        vec![
            (b"wght", 400.0, 400.0, 400.0),
            (b"wdth", 112.0, 112.0, 112.0),
        ],
    ] {
        compare_path(&path, &ranges);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0061_i1_avar2_restricts_variable_base_like_harfbuzz() {
    let source = paths::testdata().join("harfbuzz/test/subset/data/fonts/NotoSerifHK-subset.ttf");
    let bytes = std::fs::read(source).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let mut avar = font.table(Tag::new(b"avar")).unwrap().to_vec();
    avar[..2].copy_from_slice(&2u16.to_be_bytes());
    avar.extend([0; 8]);
    let tables = font
        .sfnt()
        .records()
        .map(|r| {
            (
                r.tag,
                if r.tag == Tag::new(b"avar") {
                    avar.clone()
                } else {
                    r.data(&bytes).unwrap().to_vec()
                },
            )
        })
        .collect::<Vec<_>>();
    let refs = tables
        .iter()
        .map(|(tag, bytes)| (*tag, bytes.as_slice()))
        .collect::<Vec<_>>();
    let modified = sofel_write::build_sfnt(&refs).unwrap();
    let path =
        std::env::temp_dir().join(format!("sofel-adr61-base-avar2-{}.ttf", std::process::id()));
    std::fs::write(&path, modified).unwrap();
    for ranges in [
        vec![(b"wght", 200.0, 200.0, 400.0)],
        vec![(b"wght", 400.0, 400.0, 900.0)],
    ] {
        compare_path(&path, &ranges);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0061_i1_avar2_partial_pin_with_empty_base_store_matches_harfbuzz() {
    let source = paths::testdata().join("harfbuzz/test/subset/data/fonts/KalniaGlaze.ttf");
    let bytes = std::fs::read(source).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let mut avar = font.table(Tag::new(b"avar")).unwrap().to_vec();
    avar[..2].copy_from_slice(&2u16.to_be_bytes());
    avar.extend([0; 8]);
    let adobe = std::fs::read(
        paths::testdata().join("harfbuzz/test/subset/data/fonts/AdobeVFPrototype.otf"),
    )
    .unwrap();
    let adobe_font = Font::new(&adobe, 0).unwrap();
    let old_base = adobe_font.table(Tag::new(b"BASE")).unwrap();
    let mut base = vec![0, 1, 0, 1, 0, 12, 0, 0];
    let store_at = u32::try_from(old_base.len() + 4).unwrap();
    base.extend(store_at.to_be_bytes());
    base.extend(&old_base[8..]);
    base.extend([0, 1, 0, 0, 0, 8, 0, 0, 0, 2, 0, 0]);
    let tables = font
        .sfnt()
        .records()
        .map(|r| {
            (
                r.tag,
                if r.tag == Tag::new(b"avar") {
                    avar.clone()
                } else {
                    r.data(&bytes).unwrap().to_vec()
                },
            )
        })
        .chain(std::iter::once((Tag::new(b"BASE"), base)))
        .collect::<Vec<_>>();
    let refs = tables
        .iter()
        .map(|(tag, bytes)| (*tag, bytes.as_slice()))
        .collect::<Vec<_>>();
    let modified = sofel_write::build_sfnt(&refs).unwrap();
    let path =
        std::env::temp_dir().join(format!("sofel-adr61-base-store-{}.ttf", std::process::id()));
    std::fs::write(&path, modified).unwrap();
    for ranges in [
        vec![
            (b"wght", 200.0, 400.0, 600.0),
            (b"wdth", 112.0, 112.0, 112.0),
        ],
        vec![(b"wght", 200.0, 400.0, 600.0)],
    ] {
        compare_path(&path, &ranges);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0061_i1_identity_avar2_partial_pin_keeps_variable_tables() {
    let source = paths::testdata().join("harfbuzz/test/subset/data/fonts/KalniaGlaze.ttf");
    let bytes = std::fs::read(source).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let mut avar = vec![0, 2, 0, 0, 0, 0, 0, 2];
    for _ in 0..2 {
        avar.extend(3u16.to_be_bytes());
        for coord in [-16384i16, 0, 16384] {
            avar.extend(coord.to_be_bytes());
            avar.extend(coord.to_be_bytes());
        }
    }
    avar.extend([0; 8]);
    let tables = font
        .sfnt()
        .records()
        .map(|r| {
            (
                r.tag,
                if r.tag == Tag::new(b"avar") {
                    avar.clone()
                } else {
                    r.data(&bytes).unwrap().to_vec()
                },
            )
        })
        .collect::<Vec<_>>();
    let refs = tables
        .iter()
        .map(|(tag, bytes)| (*tag, bytes.as_slice()))
        .collect::<Vec<_>>();
    let modified = sofel_write::build_sfnt(&refs).unwrap();
    let path = std::env::temp_dir().join(format!(
        "sofel-adr61-identity-avar2-{}.ttf",
        std::process::id()
    ));
    std::fs::write(&path, modified).unwrap();
    for ranges in [
        vec![(b"wght", 200.0, 200.0, 200.0)],
        vec![(b"wdth", 112.0, 112.0, 112.0)],
        vec![(b"wght", 200.0, 400.0, 600.0)],
    ] {
        compare_path(&path, &ranges);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0061_i1_math_variation_references_match_harfbuzz() {
    let source = paths::testdata().join("harfbuzz/test/api/fonts/math-variations.ttf");
    for ranges in [
        vec![(b"wght", 0.5, 0.5, 0.5)],
        vec![(b"wdth", 0.5, 0.5, 0.5)],
        vec![(b"wght", 0.0, 0.5, 1.0)],
        vec![(b"wght", 0.5, 0.5, 0.5), (b"wdth", 0.5, 0.5, 0.5)],
    ] {
        compare_path(&source, &ranges);
    }
}

#[test]
fn adr0061_i1_avar2_math_variation_references_match_harfbuzz() {
    let source = paths::testdata().join("harfbuzz/test/api/fonts/math-variations.ttf");
    let bytes = std::fs::read(source).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let mut avar = vec![0, 2, 0, 0, 0, 0, 0, 2];
    for _ in 0..2 {
        avar.extend(3u16.to_be_bytes());
        for coord in [-16384i16, 0, 16384] {
            avar.extend(coord.to_be_bytes());
            avar.extend(coord.to_be_bytes());
        }
    }
    avar.extend([0; 8]);
    let mut tables = font
        .sfnt()
        .records()
        .map(|r| (r.tag, r.data(&bytes).unwrap().to_vec()))
        .collect::<Vec<_>>();
    tables.push((Tag::new(b"avar"), avar));
    let refs = tables
        .iter()
        .map(|(tag, bytes)| (*tag, bytes.as_slice()))
        .collect::<Vec<_>>();
    let modified = sofel_write::build_sfnt(&refs).unwrap();
    let path =
        std::env::temp_dir().join(format!("sofel-adr61-math-avar2-{}.ttf", std::process::id()));
    std::fs::write(&path, modified).unwrap();
    for ranges in [
        vec![(b"wght", 0.0, 0.5, 1.0)],
        vec![(b"wght", 0.5, 0.5, 0.5)],
        vec![(b"wght", 0.0, 0.5, 1.0), (b"wdth", 0.5, 0.5, 0.5)],
        vec![(b"wght", 0.5, 0.5, 0.5), (b"wdth", 0.5, 0.5, 0.5)],
    ] {
        compare_path(&path, &ranges);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn adr0061_i1_avar2_shared_default_and_self_contained_pin_match_harfbuzz() {
    compare_axes(&[(b"XOPQ", 88.0, 88.0, 88.0), (b"wdth", 100.0, 100.0, 100.0)]);
}

#[test]
fn adr0061_i1_avar2_empty_store_matches_harfbuzz() {
    compare_store_variant(false, &[(b"wght", 300.0, 400.0, 500.0)]);
}

#[test]
fn adr0061_i1_avar2_null_store_matches_harfbuzz() {
    compare_store_variant(true, &[(b"wght", 300.0, 400.0, 500.0)]);
}

#[test]
fn adr0061_i1_avar2_null_store_self_contained_pin_matches_harfbuzz() {
    compare_store_variant(true, &[(b"opsz", 40.0, 40.0, 40.0)]);
}

#[test]
fn adr0022_i1_avar2_gsub_feature_variations_11_matches_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GSUB-lookup-avar2.ttf");
    compare_path(&path, &[(b"TEST", -0.5, 0.0, 0.5)]);
}

#[test]
fn adr0022_i1_avar2_gpos_feature_variations_11_matches_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GPOS-lookup-avar2.ttf");
    compare_path(&path, &[(b"TEST", -0.5, 0.0, 0.5)]);
}

#[test]
fn adr0022_i1_gsub_feature_variations_11_without_axis_change_matches_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GSUB-lookup-avar2.ttf");
    compare_path(&path, &[]);
}

#[test]
fn adr0022_i1_gpos_feature_variations_11_without_axis_change_matches_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GPOS-lookup-avar2.ttf");
    compare_path(&path, &[]);
}

#[test]
fn adr0022_i1_gsub_feature_variations_11_moved_default_matches_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GSUB-lookup-avar2.ttf");
    compare_path(&path, &[(b"TEST", -0.75, -0.25, 0.5)]);
}

#[test]
fn adr0022_i1_gpos_feature_variations_11_moved_default_matches_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GPOS-lookup-avar2.ttf");
    compare_path(&path, &[(b"TEST", -0.75, -0.25, 0.5)]);
}

#[test]
fn adr0022_i1_gsub_feature_variations_11_two_ranges_match_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GSUB-lookup-avar2.ttf");
    compare_path(
        &path,
        &[(b"TEST", -0.5, 0.0, 0.5), (b"DUMY", -0.75, 0.25, 1.0)],
    );
}

#[test]
fn adr0022_i1_gpos_feature_variations_11_two_ranges_match_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GPOS-lookup-avar2.ttf");
    compare_path(
        &path,
        &[(b"TEST", -0.5, 0.0, 0.5), (b"DUMY", -0.75, 0.25, 1.0)],
    );
}

#[test]
fn adr0061_i1_gsub_feature_variations_11_one_pin_match_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GSUB-lookup-avar2.ttf");
    compare_path(
        &path,
        &[(b"TEST", -0.5, 0.0, 0.5), (b"DUMY", 0.5, 0.5, 0.5)],
    );
}

#[test]
fn adr0061_i1_gpos_feature_variations_11_one_pin_match_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GPOS-lookup-avar2.ttf");
    compare_path(
        &path,
        &[(b"TEST", -0.5, 0.0, 0.5), (b"DUMY", 0.5, 0.5, 0.5)],
    );
}

#[test]
fn adr0061_i1_gsub_feature_variations_11_all_pinned_match_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GSUB-lookup-avar2.ttf");
    compare_path(
        &path,
        &[(b"TEST", 0.5, 0.5, 0.5), (b"DUMY", -0.5, -0.5, -0.5)],
    );
}

#[test]
fn adr0061_i1_gpos_feature_variations_11_all_pinned_match_harfbuzz() {
    let path = paths::testdata()
        .join("harfbuzz/test/api/fonts/feature-variation-precision-GPOS-lookup-avar2.ttf");
    compare_path(
        &path,
        &[(b"TEST", 0.5, 0.5, 0.5), (b"DUMY", -0.5, -0.5, -0.5)],
    );
}

fn compare_store_variant(null: bool, ranges: &[(&[u8; 4], f32, f32, f32)]) {
    let source = paths::testdata().join("harfbuzz/test/api/fonts/TestAvar2Instance.ttf");
    let bytes = std::fs::read(source).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let tag = Tag::new(b"avar");
    let mut avar = font.table(tag).unwrap().to_vec();
    let mut at = 8usize;
    for _ in 0..u16::from_be_bytes([avar[6], avar[7]]) {
        let count = usize::from(u16::from_be_bytes([avar[at], avar[at + 1]]));
        at += 2 + count * 4;
    }
    let store_offset = u32::from_be_bytes(avar[at + 4..at + 8].try_into().unwrap()) as usize;
    avar[at..at + 4].copy_from_slice(&0u32.to_be_bytes());
    if null {
        avar[at + 4..at + 8].copy_from_slice(&0u32.to_be_bytes());
    } else {
        avar[store_offset + 6..store_offset + 8].copy_from_slice(&0u16.to_be_bytes());
    }
    let tables = font
        .sfnt()
        .records()
        .map(|r| {
            (
                r.tag,
                if r.tag == tag {
                    avar.clone()
                } else {
                    font.table(r.tag).unwrap().to_vec()
                },
            )
        })
        .collect::<Vec<_>>();
    let refs = tables
        .iter()
        .map(|(t, b)| (*t, b.as_slice()))
        .collect::<Vec<_>>();
    let modified = sofel_write::build_sfnt(&refs).unwrap();
    let suffix = ranges
        .iter()
        .map(|(axis, min, default, max)| {
            format!(
                "{}-{min}-{default}-{max}",
                std::str::from_utf8(*axis).unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join("-");
    let path = std::env::temp_dir().join(format!(
        "sofel-avar2-store-{null}-{}-{suffix}.ttf",
        std::process::id()
    ));
    std::fs::write(&path, modified).unwrap();
    compare_path(&path, ranges);
    std::fs::remove_file(path).unwrap();
}

fn compare(axis: &[u8; 4], min: f32, default: f32, max: f32) {
    compare_axes(&[(axis, min, default, max)]);
}

fn compare_axes(ranges: &[(&[u8; 4], f32, f32, f32)]) {
    let path = paths::testdata().join("harfbuzz/test/api/fonts/TestAvar2Instance.ttf");
    compare_path(&path, ranges);
}

fn compare_path(path: &std::path::Path, ranges: &[(&[u8; 4], f32, f32, f32)]) {
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let mut input = Input::new().map_set(SetKind::Unicodes, |set| set.add_range(0, 0x10ffff));
    for &(axis, min, default, max) in ranges {
        input = input
            .with_axis_range(Tag::new(axis), Some(min), Some(max), Some(default), &font)
            .unwrap();
    }
    let actual = subset(&font, &input).unwrap();
    let variations = ranges
        .iter()
        .map(|(axis, min, default, max)| {
            format!(
                "{}={min}:{default}:{max}",
                std::str::from_utf8(*axis).unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let output = std::env::temp_dir().join(format!(
        "sofel-avar2-partial-{}-{}-{}.ttf",
        std::process::id(),
        path.file_stem().unwrap().to_string_lossy(),
        variations.replace(['=', ':', ','], "-")
    ));
    assert!(
        Command::new(paths::hb_dump())
            .arg("--subset")
            .arg(format!("--font-file={}", path.display()))
            .arg("--unicodes=*")
            .arg(format!("--variations={variations}"))
            .arg(format!("--output-file={}", output.display()))
            .status()
            .unwrap()
            .success()
    );
    let expected = std::fs::read(&output).unwrap();
    std::fs::remove_file(output).unwrap();
    if actual != expected {
        let got = Font::new(&actual, 0).unwrap();
        let want = Font::new(&expected, 0).unwrap();
        for record in want.sfnt().records() {
            let a = got.table(record.tag).unwrap_or(&[]);
            let b = want.table(record.tag).unwrap_or(&[]);
            if a != b {
                let first = a
                    .iter()
                    .zip(b)
                    .position(|(x, y)| x != y)
                    .unwrap_or(a.len().min(b.len()));
                panic!(
                    "first differing table {:?}, offset {first}, lengths {} / {}",
                    record.tag,
                    a.len(),
                    b.len()
                );
            }
        }
        panic!("sfnt bytes differ outside table payloads");
    }
}
