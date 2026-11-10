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
