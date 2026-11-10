//! sofel's metrics against HarfBuzz (ADR 0016): `hb-dump` on a font with
//! known values, and on every face of HarfBuzz's test fonts. Failures
//! not listed in conformance/expected-failures-harfbuzz.txt, and listed
//! items that pass, fail the test.

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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use build::*;
use sofel_conformance::compare::Item;
use sofel_conformance::{expect, fonts, hb, paths};
use sofel_font::{Font, FontError};

const LIST: &str = "expected-failures-harfbuzz.txt";

fn hb_dump(font: &Path) -> Option<String> {
    hb_dump_mode("--metrics", font)
}

fn hb_dump_mode(mode: &str, font: &Path) -> Option<String> {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe).arg(mode).arg(font).output().unwrap();
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).unwrap())
}

fn temp_font(name: &str, bytes: &[u8]) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-hb-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn hb_dump_writes_known_metrics() {
    // sample_tables: upem 1000, hhea 800/-200/90, advances 500 + gid.
    let mut tables = sample_tables();
    tables.push((b"vhea", hhea(1)));
    tables.push((b"vmtx", hmtx(&[(1100, 0)], &[0; 27])));
    tables.push((b"VORG", vorg(1, 880, &[(2, 900)])));
    tables.sort_by_key(|(t, _)| **t);
    let path = temp_font("known", &sfnt(&tables));
    let text = hb_dump(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let faces = hb::parse(&text).unwrap();
    assert_eq!(faces.len(), 1);
    let f = &faces[0];
    assert_eq!((f.num_glyphs, f.upem), (28, 1000));
    assert_eq!(f.scales.len(), 6);
    let em = &f.scales[0];
    assert_eq!((em.x_scale, em.y_scale), (1000, 1000));
    assert_eq!(em.h_extents, [800, -200, 90]);
    assert_eq!(em.v_extents, [800, -200, 90]);
    assert!(em.metrics.contains(&("hasc".to_string(), true, 800, 800)));
    assert!(em.metrics.contains(&("xhgt".to_string(), false, 0, 500)));
    assert_eq!(em.glyphs.len(), 28);
    assert_eq!(em.glyphs[2], (502, -1100, 251, 900, None));
    assert_eq!(em.glyphs[3], (503, -1100, 251, 880, None));
    let half = &f.scales[2];
    assert_eq!((half.x_scale, half.y_scale), (500, 500));
    assert_eq!(half.h_extents, [400, -100, 45]);
    assert_eq!(half.glyphs[2], (251, -550, 125, 450, None));
}

#[test]
fn agrees_with_harfbuzz() {
    let testdata = paths::testdata();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    let fonts = fonts::harfbuzz_fonts(&testdata);
    assert!(fonts.len() > 600, "found only {} fonts", fonts.len());

    let mut items: Vec<Item> = Vec::new();
    for path in &fonts {
        let id = fonts::id(path, &testdata);
        let Some(text) = hb_dump(path) else { continue };
        let faces = hb::parse(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        let bytes = std::fs::read(path).unwrap();
        for face in &faces {
            let font = match Font::new(&bytes, face.index) {
                Ok(f) => f,
                // HarfBuzz reads faces without head; sofel does not open them.
                Err(FontError::NoHead | FontError::Read(_) | FontError::IndexOutOfRange) => {
                    continue;
                }
            };
            let ours = hb::sofel_face(&font, face);
            items.extend(hb::compare(&id, face, &ours));
        }
        if let Some(text) = hb_dump_mode("--cmap", path) {
            let faces = hb::parse_cmap(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
            for face in &faces {
                if let Ok(font) = Font::new(&bytes, face.index) {
                    items.push(hb::compare_cmap(&id, face, &font));
                }
            }
        }
        items.extend(baseline_items(path, &id, &bytes));
        let Some(text) = hb_dump_mode("--names", path) else {
            continue;
        };
        let faces = hb::parse_names(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
        for face in &faces {
            let Ok(font) = Font::new(&bytes, face.index) else {
                continue;
            };
            let ours = hb::sofel_names(&font, face);
            items.extend(hb::compare_names(&id, face, &ours));
        }
    }

    let base_fonts = fonts::harfbuzz_base_fonts(&testdata);
    assert!(
        base_fonts.len() >= 3,
        "found only {} BASE fonts",
        base_fonts.len()
    );
    for path in &base_fonts {
        let id = fonts::id(path, &testdata);
        let bytes = std::fs::read(path).unwrap();
        items.extend(baseline_items(path, &id, &bytes));
    }

    let mut kinds: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for i in &items {
        let kind = i.id.rsplit_once(":hb:").map_or("", |(_, k)| k);
        let e = kinds.entry(kind.to_string()).or_default();
        e.0 += 1;
        e.1 += usize::from(!i.ok);
    }
    eprintln!("{} fonts, {} items", fonts.len(), items.len());
    for (k, (n, failed)) in &kinds {
        eprintln!("  {k}: {n} items, {failed} failed");
    }

    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let expected = expect::parse(&list);
    let problems = expect::judge(items.iter().map(|i| (i.id.as_str(), i.ok)), &expected);
    if !problems.is_empty() {
        let mut msg = expect::report(&problems, LIST);
        for p in &problems {
            if let expect::Problem::Regressed(id) = p
                && let Some(i) = items.iter().find(|i| &i.id == id)
            {
                msg.push_str(&format!("  {}: {}\n", i.id, i.detail));
            }
        }
        let failed = items.iter().filter(|i| !i.ok).count();
        panic!("{msg}\n{} items, {failed} failed", items.len());
    }
}

/// The items of `hb-dump --baselines` for every face of a font
/// (ADR 0017).
fn baseline_items(path: &Path, id: &str, bytes: &[u8]) -> Vec<Item> {
    let Some(text) = hb_dump_mode("--baselines", path) else {
        return Vec::new();
    };
    let faces = hb::parse_baselines(&text).unwrap_or_else(|e| panic!("{id}: {e}"));
    faces
        .iter()
        .filter_map(|face| {
            let font = Font::new(bytes, face.index).ok()?;
            Some(hb::compare_baselines(
                id,
                face,
                &hb::sofel_baselines(&font, face),
            ))
        })
        .flatten()
        .collect()
}

fn tag(t: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*t)
}

#[test]
fn hb_dump_writes_known_baselines() {
    // sample_tables: upem 1000, hhea 800/-200/90, no OS/2, glyphs for
    // A..Z only (no glyph boxes).
    let h = AxisSpec {
        tags: Some(vec![*b"ideo", *b"romn"]),
        scripts: Some(vec![
            (
                *b"DFLT",
                Some(ScriptSpec {
                    values: Some((1, vec![Some(Coord::F1(-120)), Some(Coord::F1(0))])),
                    ..Default::default()
                }),
            ),
            (
                *b"latn",
                Some(ScriptSpec {
                    default_min_max: Some(MinMaxSpec {
                        min: Some(Coord::F1(-300)),
                        max: Some(Coord::F1(1100)),
                        features: vec![],
                    }),
                    lang_sys: vec![(
                        *b"ENG ",
                        Some(MinMaxSpec {
                            min: None,
                            max: Some(Coord::F1(1200)),
                            features: vec![],
                        }),
                    )],
                    ..Default::default()
                }),
            ),
        ]),
    };
    let mut tables = sample_tables();
    tables.push((b"BASE", base(Some(&h), None)));
    tables.sort_by_key(|(t, _)| **t);
    let path = temp_font("baselines", &sfnt(&tables));
    let text = hb_dump_mode("--baselines", &path).unwrap();
    std::fs::remove_file(&path).ok();
    let faces = hb::parse_baselines(&text).unwrap();
    assert_eq!(faces.len(), 1);
    let f = &faces[0];
    assert_eq!(f.upem, 1000);
    assert_eq!(f.scales.len(), 6);
    let em = &f.scales[0];
    let b = |d: char, s: &[u8; 4], t: &[u8; 4]| {
        *em.baselines
            .iter()
            .find(|x| x.0 == d && x.1 == tag(s) && x.2 == tag(t))
            .unwrap()
    };
    let (h, v) = ('h', 'v');
    assert_eq!(
        b(h, b"hani", b"ideo"),
        (h, tag(b"hani"), tag(b"ideo"), true, -120, -120)
    );
    assert_eq!(
        b(h, b"hani", b"idtp"),
        (h, tag(b"hani"), tag(b"idtp"), false, 0, 880)
    );
    assert_eq!(
        b(h, b"hani", b"Idce"),
        (h, tag(b"hani"), tag(b"Idce"), false, 0, 380)
    );
    assert_eq!(
        b(h, b"latn", b"romn"),
        (h, tag(b"latn"), tag(b"romn"), false, 0, 0)
    );
    assert_eq!(
        b(h, b"deva", b"hang"),
        (h, tag(b"deva"), tag(b"hang"), false, 0, 600)
    );
    assert_eq!(
        b(v, b"hani", b"Idce"),
        (v, tag(b"hani"), tag(b"Idce"), false, 0, 0)
    );
    let e = |s: &[u8; 4], l: &[u8; 4]| {
        *em.extents
            .iter()
            .find(|x| x.0 == 'h' && x.1 == tag(s) && x.2 == tag(l))
            .unwrap()
    };
    assert_eq!(
        e(b"latn", b"dflt"),
        ('h', tag(b"latn"), tag(b"dflt"), true, [1100, -300, 0])
    );
    assert_eq!(
        e(b"latn", b"ENG "),
        ('h', tag(b"latn"), tag(b"ENG "), true, [1200, 0, 0])
    );
    assert_eq!(
        e(b"hani", b"dflt"),
        ('h', tag(b"hani"), tag(b"dflt"), false, [800, -200, 90])
    );
    // The language system tags of the table are listed.
    assert!(em.extents.iter().any(|x| x.2 == tag(b"ENG ")));
}

#[test]
fn hb_dump_rejects_bases_like_sofel() {
    // HarfBuzz's sanitizer rejects the whole table for a bad offset and
    // for running out of its operation budget (64 per byte): 600 script
    // records sharing one BaseScript with 600 language systems check
    // 600 * 3600 bytes, more than 64 times the table's ~7 kB.
    let langs: Vec<([u8; 4], Option<MinMaxSpec>)> = (0..600u32)
        .map(|i| {
            let t = (0x4141_4141 + i).to_be_bytes();
            (t, None)
        })
        .collect();
    let shared = ScriptSpec {
        values: Some((0, vec![Some(Coord::F1(5))])),
        lang_sys: langs,
        ..Default::default()
    };
    let few = AxisSpec {
        tags: Some(vec![*b"romn"]),
        scripts: Some(vec![(*b"DFLT", Some(shared.clone()))]),
    };
    let mut cases = vec![(base(Some(&few), None), true)];
    let mut bad = base(Some(&few), None);
    bad[6..8].copy_from_slice(&0xFFF0u16.to_be_bytes());
    cases.push((bad, false));
    // Point 600 script records at the first one's BaseScript.
    let one = base(Some(&few), None);
    let axis = 8usize;
    let list = axis + u16::from_be_bytes([one[axis + 2], one[axis + 3]]) as usize;
    let script = list + u16::from_be_bytes([one[list + 6], one[list + 7]]) as usize;
    let mut many = one[..list].to_vec();
    let records = 600usize;
    let new_script_off = 2 + 6 * records;
    many.extend_from_slice(&(records as u16).to_be_bytes());
    for i in 0..records {
        many.extend_from_slice(&(0x4141_4141u32 + i as u32).to_be_bytes());
        many.extend_from_slice(&(new_script_off as u16).to_be_bytes());
    }
    many.extend_from_slice(&one[script..]);
    // The BaseScript's own offsets are relative to it, so it moves as is.
    cases.push((many, false));
    for (table, kept) in cases {
        let mut tables = sample_tables();
        tables.push((b"BASE", table));
        tables.sort_by_key(|(t, _)| **t);
        let bytes = sfnt(&tables);
        let path = temp_font("reject", &bytes);
        let text = hb_dump_mode("--baselines", &path).unwrap();
        std::fs::remove_file(&path).ok();
        let face = &hb::parse_baselines(&text).unwrap()[0];
        let found = face.scales[0]
            .baselines
            .iter()
            .any(|x| x.2 == tag(b"romn") && x.3);
        assert_eq!(found, kept);
        let font = Font::new(&bytes, 0).unwrap();
        let ours = hb::sofel_baselines(&font, face);
        assert!(
            hb::compare_baselines("reject", face, &ours)
                .iter()
                .all(|i| i.ok)
        );
    }
}

#[test]
fn hb_dump_writes_known_names() {
    let ps = utf16be("Sofel");
    let mut tables = sample_tables();
    tables.push((
        b"name",
        name(0, &[(1, 0, 0, 1, b"Mac"), (3, 1, 0x409, 1, &ps)], &[]),
    ));
    tables.push((b"post", post2(&[0, 36, 258, 258], &["alpha"])));
    tables.sort_by_key(|(t, _)| **t);
    let path = temp_font("names", &sfnt(&tables));
    let text = hb_dump_mode("--names", &path).unwrap();
    std::fs::remove_file(&path).ok();
    let f = &hb::parse_names(&text).unwrap()[0];
    assert_eq!(f.names, vec![(1, "en".to_string())]);
    assert!(f.utf.contains(&(
        "utf8".to_string(),
        1,
        "-".to_string(),
        6,
        5,
        5,
        "536f66656c".to_string()
    )));
    assert!(f.utf.contains(&(
        "utf8".to_string(),
        1,
        "en".to_string(),
        4,
        5,
        3,
        "536f66".to_string()
    )));
    assert_eq!(f.glyph_names[2], (2, true, "616c706861".to_string()));
    assert_eq!(f.glyph_names[4], (4, false, "-".to_string()));
    let from = f.from_names.iter().find(|x| x.0 == 3).unwrap();
    assert!(from.1 && (from.2 == 2 || from.2 == 3));
}
