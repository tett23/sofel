//! ADR 0027 task 2: reference variation queries precede value evaluation.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use sofel_conformance::{expect, fonts, paths};
use sofel_font::{
    Font,
    var::{Location, Variation},
};
use sofel_read::Tag;
use std::path::Path;
use std::process::Command;

fn source_serif() -> std::path::PathBuf {
    paths::testdata().join("harfbuzz/test/subset/data/fonts/SourceSerifVariable-Roman.ttf")
}

fn variable_colr() -> std::path::PathBuf {
    paths::testdata().join("harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf")
}

fn dump(executable: &Path, args: &[&str], font: &Path) -> String {
    paths::require(executable, "variation reference").unwrap();
    let output = Command::new(executable)
        .args(args)
        .arg(font)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}: {}",
        executable.display(),
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn row<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    text.lines()
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("missing {prefix}: {text}"))
        .split_ascii_whitespace()
        .collect()
}

fn face_block(text: &str, index: u32) -> Option<&str> {
    let marker = format!("face {index}\n");
    let start = text.find(&marker)?;
    let after = &text[start + marker.len()..];
    let end = after.find("\nend\n")?;
    Some(&after[..end])
}

fn variable_font_paths(data: &Path) -> Vec<std::path::PathBuf> {
    let mut paths = fonts::harfbuzz_fonts(data);
    for dir in ["harfbuzz/test/api/fonts", "harfbuzz/perf/fonts"] {
        if let Ok(entries) = std::fs::read_dir(data.join(dir)) {
            paths.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_file()),
            );
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

#[test]
fn adr0027_i2_variable_corpus_axis_queries() {
    let data = paths::testdata();
    let mut checked = 0;
    let mut ft_unavailable = 0;
    let mut ft_missing_view = 0;
    let mut named_style_results = Vec::new();
    for path in variable_font_paths(&data) {
        let bytes = std::fs::read(&path).unwrap();
        let face_count = if bytes.starts_with(b"ttcf") && bytes.len() >= 12 {
            u32::from_be_bytes(bytes[8..12].try_into().unwrap()).min(1024)
        } else {
            1
        };
        let mut variable_faces = Vec::new();
        for index in 0..face_count {
            if let Ok(font) = Font::new(&bytes, index)
                && font.table(Tag::new(b"fvar")).is_some()
            {
                variable_faces.push((
                    index,
                    font.var_axes().count(),
                    font.named_instances().count(),
                    font.ft_mm_var().map(|v| {
                        (
                            v.axes().count(),
                            v.named_styles().count(),
                            v.default_named_instance().unwrap_or(0),
                        )
                    }),
                ));
            }
        }
        if variable_faces.is_empty() {
            continue;
        }
        let hb = dump(&paths::hb_dump(), &["--var"], &path);
        let ft = Command::new(paths::ft_dump())
            .arg("--var")
            .arg(&path)
            .output()
            .unwrap();
        let ft_text = String::from_utf8(ft.stdout).unwrap();
        for (index, hb_count, hb_named, ft_count) in variable_faces {
            let font = Font::new(&bytes, index).unwrap();
            let hb_face =
                face_block(&hb, index).unwrap_or_else(|| panic!("{}#{index}", path.display()));
            assert_eq!(
                row(hb_face, "axis_count ")[1].parse::<usize>().unwrap(),
                hb_count,
                "{}#{index}",
                path.display()
            );
            assert_eq!(
                row(hb_face, "named_count ")[1].parse::<usize>().unwrap(),
                hb_named,
                "{}#{index}",
                path.display()
            );
            for axis in font.var_axes() {
                let fields = row(hb_face, &format!("axis {} ", axis.index));
                assert_eq!(fields[2], format!("{:08x}", axis.tag.to_u32()));
                assert_eq!(fields[3].parse::<u16>().unwrap(), axis.name_id);
                assert_eq!(fields[4].parse::<u32>().unwrap(), u32::from(axis.hidden));
                for (field, value) in fields[5..8].iter().zip([axis.min, axis.default, axis.max]) {
                    assert_eq!(u32::from_str_radix(field, 16).unwrap(), value.to_bits());
                }
            }
            for (named_index, named_style) in font.named_instances().enumerate() {
                let fields = row(hb_face, &format!("named {named_index} "));
                assert_eq!(fields.len(), hb_count + 4);
                assert_eq!(
                    fields[2].parse::<u16>().unwrap(),
                    named_style.subfamily_name_id
                );
                assert_eq!(
                    fields[3].parse::<u16>().unwrap(),
                    named_style.postscript_name_id.unwrap_or(0xFFFF)
                );
                for (field, value) in fields[4..].iter().zip(named_style.design_coords()) {
                    assert_eq!(u32::from_str_radix(field, 16).unwrap(), value.to_bits());
                }
            }
            if let Some((axes, named, default)) = ft_count {
                if ft.status.success() {
                    let ft_face = face_block(&ft_text, index)
                        .unwrap_or_else(|| panic!("{}#{index}", path.display()));
                    assert_eq!(
                        row(ft_face, "axis_count ")[1].parse::<usize>().unwrap(),
                        axes,
                        "{}#{index}",
                        path.display()
                    );
                    let ft_view = font.ft_mm_var().unwrap();
                    for (axis_index, axis) in ft_view.axes().enumerate() {
                        let fields = row(ft_face, &format!("axis {axis_index} "));
                        assert_eq!(
                            u32::from_str_radix(fields[2], 16).unwrap(),
                            axis.tag.to_u32()
                        );
                        assert_eq!(fields[3].parse::<u16>().unwrap(), axis.name_id);
                        assert_eq!(fields[4].parse::<u16>().unwrap(), axis.flags);
                        for (field, value) in
                            fields[5..8].iter().zip([axis.min, axis.default, axis.max])
                        {
                            assert_eq!(field.parse::<i32>().unwrap(), value.0);
                        }
                    }
                    for (named_index, named_style) in ft_view.named_styles().enumerate() {
                        let fields = row(ft_face, &format!("named {named_index} "));
                        assert_eq!(fields.len(), axes + 4);
                        assert_eq!(fields[2].parse::<u16>().unwrap(), named_style.strid);
                        assert_eq!(fields[3].parse::<u16>().unwrap(), named_style.psid);
                        for (field, value) in fields[4..].iter().zip(named_style.design_coords()) {
                            assert_eq!(field.parse::<i32>().unwrap(), value.0);
                        }
                    }
                    let actual_named = row(ft_face, "named_count ")[1].parse::<usize>().unwrap();
                    let actual_default = row(ft_face, "default_named ")[1].parse::<u16>().unwrap();
                    let id = format!(
                        "{}#{index}:var:ft-named-style",
                        path.strip_prefix(&data).unwrap().display()
                    );
                    let passed = (actual_named, actual_default) == (named, default);
                    if !passed {
                        assert_eq!(
                            (actual_named, actual_default, named, default),
                            (1, 1, 0, 0),
                            "unexpected difference in {id}"
                        );
                        assert!(
                            row(ft_face, "named 0 ")
                                .iter()
                                .skip(2)
                                .all(|value| value.parse::<i32>() == Ok(0)),
                            "expected a zero-entry synthesized style in {id}"
                        );
                    }
                    named_style_results.push((id, passed));
                } else {
                    ft_unavailable += 1;
                }
            } else {
                ft_missing_view += 1;
            }
            checked += 1;
        }
    }
    eprintln!(
        "{checked} variable faces queried; {ft_unavailable} FreeType inputs unavailable; {ft_missing_view} FreeType views unavailable"
    );
    let expected = expect::parse(include_str!(
        "../../../conformance/expected-failures-freetype.txt"
    ))
    .into_iter()
    .filter(|id| id.ends_with(":var:ft-named-style"))
    .collect();
    let problems = expect::judge(
        named_style_results
            .iter()
            .map(|(id, passed)| (id.as_str(), *passed)),
        &expected,
    );
    assert!(
        problems.is_empty(),
        "{}",
        expect::report(&problems, "expected-failures-freetype.txt")
    );
    assert!(checked > 20, "only {checked} variable faces");
    assert_eq!(ft_unavailable, 0, "FreeType reference input unavailable");
    assert_eq!(ft_missing_view, 0, "FreeType view unavailable");
}

#[test]
fn adr0027_i2_harfbuzz_var_queries_match_fvar() {
    let path = source_serif();
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let axes: Vec<_> = font.var_axes().collect();
    assert!(!axes.is_empty());
    let text = dump(&paths::hb_dump(), &["--var"], &path);
    assert!(text.contains(&format!("axis_count {}\n", axes.len())));
    assert!(text.contains(&format!("named_count {}\n", font.named_instances().count())));
    for axis in axes {
        let tag = axis.tag.to_u32();
        let fields = row(&text, &format!("axis {} ", axis.index));
        assert_eq!(fields[2], format!("{tag:08x}"));
        assert_eq!(fields[3].parse::<u16>().unwrap(), axis.name_id);
        assert_eq!(fields[4].parse::<u32>().unwrap(), u32::from(axis.hidden));
        for (field, expected) in [axis.min, axis.default, axis.max]
            .into_iter()
            .zip(&fields[5..8])
        {
            assert_eq!(u32::from_str_radix(expected, 16).unwrap(), field.to_bits());
        }
    }
    assert!(text.contains("coords_normalized"));
    assert!(text.contains("coords_design"));
}

#[test]
fn adr0027_i2_freetype_var_queries_match_fvar() {
    let path = source_serif();
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let view = font.ft_mm_var().unwrap();
    let text = dump(&paths::ft_dump(), &["--var"], &path);
    assert!(text.contains(&format!("axis_count {}\n", view.axes().count())));
    for (index, axis) in view.axes().enumerate() {
        let fields = row(&text, &format!("axis {index} "));
        assert_eq!(
            u32::from_str_radix(fields[2], 16).unwrap(),
            axis.tag.to_u32()
        );
        assert_eq!(fields[3].parse::<u16>().unwrap(), axis.name_id);
        assert_eq!(fields[4].parse::<u16>().unwrap(), axis.flags);
        for (value, field) in [axis.min.0, axis.default.0, axis.max.0]
            .into_iter()
            .zip(&fields[5..8])
        {
            assert_eq!(field.parse::<i32>().unwrap(), value);
        }
    }
    assert!(text.contains(&format!("named_count {}\n", view.named_styles().count())));
    let default = view.default_named_instance().unwrap_or(0);
    assert_eq!(
        row(&text, "default_named ")[1].parse::<u16>().unwrap(),
        default
    );
    assert!(text.contains("coords_blend"));
    assert!(text.contains("coords_design"));
    assert!(text.contains("ps_name "));
}

#[test]
fn adr0027_i2_named_and_overridden_design_coordinates() {
    let path = source_serif();
    let bytes = std::fs::read(&path).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let variation = Variation {
        tag: Tag::new(b"wght"),
        value: 900.0,
    };
    for (args, location) in [
        (
            vec!["--named-instance=0"],
            Location::named_instance(&font, 0),
        ),
        (
            vec!["--named-instance=0", "--variations=wght=900"],
            Location::new(&font, Some(0), &[variation]),
        ),
        (
            vec!["--named-instance=0", "--variations=wght=389.34425354003906"],
            Location::new(
                &font,
                Some(0),
                &[Variation {
                    value: 389.34425,
                    ..variation
                }],
            ),
        ),
        (
            vec!["--named-instance=0", "--variations=wght=100"],
            Location::new(
                &font,
                Some(0),
                &[Variation {
                    value: 100.0,
                    ..variation
                }],
            ),
        ),
    ] {
        let mut options = vec!["--var"];
        options.extend_from_slice(&args);
        assert_location(&path, 0, &font, &location, &options);
    }
}

#[test]
fn adr0027_i2_variation_options_reach_existing_reference_modes() {
    let path = source_serif();
    let hb_default = dump(&paths::hb_dump(), &["--metrics"], &path);
    let hb_heavy = dump(
        &paths::hb_dump(),
        &["--metrics", "--variations=wght=900"],
        &path,
    );
    assert_ne!(hb_default, hb_heavy, "HarfBuzz metrics ignored variations");
    let ft_default = dump(&paths::ft_dump(), &["--outline", "unscaled"], &path);
    let ft_heavy = dump(
        &paths::ft_dump(),
        &["--outline", "unscaled", "--variations=wght=900"],
        &path,
    );
    assert_ne!(ft_default, ft_heavy, "FreeType outlines ignored variations");
}

#[test]
fn adr0027_i2_normalize_samples_cover_257_intervals_and_outside() {
    for path in [source_serif(), variable_colr()] {
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        for executable in [paths::hb_dump(), paths::ft_dump()] {
            let text = dump(&executable, &["--var-normalize"], &path);
            let is_hb = executable == paths::hb_dump();
            assert_normalize_face(&font, &text, is_hb, &path);
            assert!(text.contains("sample 0 0 "));
            assert!(text.contains("sample 0 256 "));
            assert!(text.contains("sample 0 outside_min "));
            assert!(text.contains("sample 0 outside_max "));
            assert!(text.contains("sample 0 half_min "));
            assert!(text.contains("sample 0 half_max "));
        }
    }
}

fn assert_normalize_face(font: &Font<'_>, text: &str, is_hb: bool, path: &Path) -> usize {
    let defaults: Vec<_> = font.var_axes().map(|axis| axis.default).collect();
    let mut samples = 0;
    for line in text.lines().filter(|line| line.starts_with("sample ")) {
        let parts: Vec<_> = line.split_ascii_whitespace().collect();
        let axis: usize = parts[1].parse().unwrap();
        let design = if is_hb {
            f32::from_bits(u32::from_str_radix(parts[3], 16).unwrap())
        } else {
            parts[3].parse::<i32>().unwrap() as f32 / 65536.0
        };
        let value: i32 = parts[4].parse().unwrap();
        let mut coordinates = defaults.clone();
        coordinates[axis] = design;
        let location = Location::from_design(font, &coordinates);
        let ours = if is_hb {
            location.coords_normalized()[axis]
        } else {
            location.ft_coords()[axis].0
        };
        assert_eq!(ours, value, "{}: {line}", path.display());
        samples += 1;
    }
    assert_eq!(
        samples,
        defaults.len() * 261,
        "{}: {samples}",
        path.display()
    );
    samples
}

#[test]
fn adr0027_i2_variable_corpus_normalization_samples() {
    let data = paths::testdata();
    let mut faces = 0;
    let mut samples = 0;
    for path in variable_font_paths(&data) {
        let bytes = std::fs::read(&path).unwrap();
        let face_count = if bytes.starts_with(b"ttcf") && bytes.len() >= 12 {
            u32::from_be_bytes(bytes[8..12].try_into().unwrap()).min(1024)
        } else {
            1
        };
        let indices: Vec<_> = (0..face_count)
            .filter(|index| {
                Font::new(&bytes, *index)
                    .ok()
                    .is_some_and(|font| font.table(Tag::new(b"fvar")).is_some())
            })
            .collect();
        if indices.is_empty() {
            continue;
        }
        let hb = dump(&paths::hb_dump(), &["--var-normalize"], &path);
        let ft = dump(&paths::ft_dump(), &["--var-normalize"], &path);
        for index in indices {
            let font = Font::new(&bytes, index).unwrap();
            for (is_hb, text) in [(true, &hb), (false, &ft)] {
                let face = face_block(text, index).unwrap();
                samples += assert_normalize_face(&font, face, is_hb, &path);
            }
            faces += 1;
        }
    }
    eprintln!("{faces} variable faces, {samples} normalization samples");
    assert_eq!(faces, 166);
}

#[path = "support/variation_locations.rs"]
mod variation_locations;

#[test]
fn adr0030_i01_unrounded_mvar_reference_at_corpus_positions() {
    use sofel_read::{mvar::Mvar, sfnt::FontFile};
    let mut values = 0;
    let mut fractional = 0;
    for path in variable_font_paths(&paths::testdata()) {
        let bytes = std::fs::read(&path).unwrap();
        let faces = match FontFile::new(&bytes) {
            Ok(FontFile::Collection(c)) => c.num_fonts(),
            Ok(FontFile::Sfnt(_)) => 1,
            Err(_) => continue,
        };
        for index in 0..faces {
            let Ok(font) = Font::new(&bytes, index) else {
                continue;
            };
            let Some(mvar) = font
                .table(Tag::new(b"MVAR"))
                .and_then(|d| Mvar::new(d).ok())
            else {
                continue;
            };
            for pos in variation_locations::positions(&font) {
                let extra = pos.args();
                let mut args = vec!["--metrics-variation"];
                args.extend(extra.iter().map(String::as_str));
                let reference = dump(&paths::hb_dump(), &args, &path);
                let location = pos.location(&font);
                let instance = font.instance(&location);
                let scale = sofel_font::HbScale::for_font(&font, 768, -1536);
                let section = reference
                    .split(&format!("face {index}\n"))
                    .nth(1)
                    .unwrap()
                    .split("end\n")
                    .next()
                    .unwrap();
                let mut records = 0;
                for (i, line) in section.lines().enumerate() {
                    let record = mvar.record(i).unwrap();
                    let fields: Vec<_> = line.split_whitespace().collect();
                    let tag = record.tag;
                    let raw = instance.metrics_variation(tag);
                    assert_eq!(
                        fields,
                        [
                            format!("{:08x}", u32::from_be_bytes(tag.0)),
                            format!("{:08x}", raw.to_bits()),
                            instance.metrics_x_variation(&scale, tag).to_string(),
                            instance.metrics_y_variation(&scale, tag).to_string()
                        ],
                        "{} {} {:?}",
                        path.display(),
                        pos.id,
                        tag
                    );
                    fractional += usize::from(raw.fract() != 0.0);
                    records += 1;
                }
                assert_eq!(records, section.lines().count());
                assert_eq!(records, mvar.record_count());
                values += records;
            }
        }
    }
    assert!(values > 0 && fractional > 0);
    eprintln!("{values} MVAR values, {fractional} fractional");
}

#[test]
fn adr0027_i02_design_normalized_and_postscript_at_all_positions() {
    use sofel_read::sfnt::FontFile;
    let mut tested = 0;
    for path in variable_font_paths(&paths::testdata()) {
        let bytes = std::fs::read(&path).unwrap();
        let faces = match FontFile::new(&bytes) {
            Ok(FontFile::Collection(c)) => c.num_fonts(),
            Ok(FontFile::Sfnt(_)) => 1,
            Err(_) => continue,
        };
        for index in 0..faces {
            let Ok(font) = Font::new(&bytes, index) else {
                continue;
            };
            if font.var_axes().next().is_none() {
                continue;
            }
            for pos in variation_locations::positions(&font) {
                let loc = pos.location(&font);
                let extra = pos.args();
                let mut args = vec!["--var"];
                args.extend(extra.iter().map(String::as_str));
                assert_location(&path, index, &font, &loc, &args);
                tested += 1;
            }
        }
    }
    eprintln!("checked {tested} face-locations");
    assert!(tested > 0);
}

fn assert_location(path: &Path, index: u32, font: &Font<'_>, loc: &Location<'_>, args: &[&str]) {
    let instance = font.instance(loc);
    let size = instance.postscript_name(&mut []).unwrap_or(0);
    let mut name = vec![0; size];
    instance.postscript_name(&mut name);
    let hex = if name.is_empty() {
        "-".to_string()
    } else {
        name.iter().map(|b| format!("{b:02x}")).collect()
    };
    let fixed = |v: &[sofel_read::Fixed]| v.iter().map(|x| x.0.to_string()).collect::<Vec<_>>();
    let hb_fields = [
        (
            "coords_normalized",
            loc.coords_normalized()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        ),
        (
            "coords_design",
            loc.coords_design()
                .iter()
                .map(|x| format!("{:08x}", x.to_bits()))
                .collect(),
        ),
    ];
    let ft_fields = [
        ("coords_blend", fixed(loc.ft_coords())),
        ("coords_design", fixed(loc.ft_design_coords())),
        ("ps_name", vec![hex]),
    ];
    for (tool, fields) in [
        (paths::hb_dump(), hb_fields.as_slice()),
        (paths::ft_dump(), ft_fields.as_slice()),
    ] {
        let reference = dump(&tool, args, path);
        let face = face_block(&reference, index).unwrap();
        for (field, actual) in fields {
            let expected = row(face, field);
            assert_eq!(
                actual,
                &expected[1..],
                "{} face {index} {args:?} {field}",
                path.display()
            );
        }
    }
}

fn delta_indices(font: &Font<'_>) -> Vec<(sofel_font::var::StoreKind, u16, u16)> {
    use sofel_font::var::StoreKind;
    use sofel_read::{gdef::Gdef, hvar::Hvar, mvar::Mvar};
    let mut indices = Vec::new();
    if let Some(mvar) = font
        .table(Tag::new(b"MVAR"))
        .and_then(|bytes| Mvar::new(bytes).ok())
    {
        for record in (0..mvar.record_count()).filter_map(|i| mvar.record(i)) {
            indices.push((StoreKind::Mvar, record.outer, record.inner));
        }
    }
    if let Some(map) = font
        .table(Tag::new(b"HVAR"))
        .and_then(|bytes| Hvar::new(bytes).ok()?.advance_map().ok())
    {
        for glyph in 0..font.num_glyphs() {
            let index = map.map_or(glyph, |map| map.map(glyph));
            let [a, b, c, d] = index.to_be_bytes();
            indices.push((
                StoreKind::Hvar,
                u16::from_be_bytes([a, b]),
                u16::from_be_bytes([c, d]),
            ));
        }
    }
    if let Some(store) = font
        .table(Tag::new(b"GDEF"))
        .and_then(|bytes| Gdef::new(bytes).ok()?.variation_store().ok().flatten())
    {
        for outer in 0..store.data_count() {
            if let Some(data) = store.item_data(outer) {
                indices.extend((0..data.item_count).map(|inner| (StoreKind::Gdef, outer, inner)));
            }
        }
    }
    indices
}

#[test]
fn adr0029_corpus_delta_digest() {
    use sofel_font::var::{Ft, HbF32};
    use sofel_read::sfnt::FontFile;
    let mut digest = 0xcbf2_9ce4_8422_2325u64;
    let mut rows = 0u64;
    for path in variable_font_paths(&paths::testdata()) {
        let bytes = std::fs::read(path).unwrap();
        let faces = match FontFile::new(&bytes) {
            Ok(FontFile::Collection(c)) => c.num_fonts(),
            Ok(FontFile::Sfnt(_)) => 1,
            Err(_) => continue,
        };
        for index in 0..faces {
            let Ok(font) = Font::new(&bytes, index) else {
                continue;
            };
            if font.var_axes().next().is_none() {
                continue;
            }
            let indices = delta_indices(&font);
            for pos in variation_locations::positions(&font) {
                let location = pos.location(&font);
                let instance = font.instance(&location);
                for &(kind, outer, inner) in &indices {
                    let hb = instance.item_delta::<HbF32>(kind, outer, inner).unwrap();
                    let bits = if hb.is_nan() {
                        0x7fc0_0000
                    } else {
                        hb.to_bits()
                    };
                    let ft = instance.item_delta::<Ft>(kind, outer, inner).unwrap();
                    for byte in bits.to_le_bytes().into_iter().chain(ft.to_le_bytes()) {
                        digest = (digest ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
                    }
                    rows += 1;
                }
            }
        }
    }
    assert!(rows > 0);
    assert_eq!(
        format!("{digest:016x} {rows}"),
        include_str!("../../../conformance/var-deltas.digest").trim()
    );
}
