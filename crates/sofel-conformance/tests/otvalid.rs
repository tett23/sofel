//! sofel's GDEF, GSUB and GPOS validators against FreeType's otvalid
//! (ADR 0020, ADR 0008): whenever `FT_OpenType_Validate` rejects a
//! table of one of HarfBuzz's test fonts, sofel's validator must report
//! an `Error` for that table. Items are `<font>#<face>:otvalid:<tag>`;
//! failures not listed in conformance/expected-failures-otvalid.txt, and
//! listed items that pass, fail the test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::process::Command;

use sofel_conformance::{expect, fonts, paths};
use sofel_read::sfnt::FontFile;
use sofel_read::{Severity, Tag, validate};

const LIST: &str = "expected-failures-otvalid.txt";

/// `(face, tag, FreeType error)` for every table in the output of
/// `ft-dump --otvalid`.
fn parse(out: &str) -> Vec<(u32, String, i32)> {
    let mut face = 0;
    let mut tables = Vec::new();
    for line in out.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["face", i] => face = i.parse().unwrap(),
            ["otvalid", tag, error] => tables.push((face, tag.to_string(), error.parse().unwrap())),
            _ => {}
        }
    }
    tables
}

#[test]
fn parses_ft_dump_output() {
    let out = "face 0\notvalid GSUB 0\notvalid GDEF 8\nend\nface 1\nerror 3\nend\n";
    assert_eq!(
        parse(out),
        vec![(0, "GSUB".to_string(), 0), (0, "GDEF".to_string(), 8)]
    );
}

/// Whether sofel reports an `Error` for `tag` in face `face`.
fn sofel_rejects(bytes: &[u8], face: u32, tag: &str) -> bool {
    let tag = Tag(tag.as_bytes().try_into().unwrap());
    let mut error = false;
    let mut report = |d: sofel_read::Diagnostic| {
        error |= d.table == tag && d.severity == Severity::Error;
    };
    match FontFile::new(bytes) {
        Ok(FontFile::Sfnt(s)) => validate::sfnt(&s, &mut report),
        Ok(FontFile::Collection(c)) => {
            if let Ok(s) = c.font(face) {
                validate::sfnt(&s, &mut report);
            }
        }
        Err(_) => {}
    }
    error
}

#[test]
fn rejects_what_freetype_rejects() {
    let testdata = paths::testdata();
    let ft_dump = paths::ft_dump();
    paths::require(&testdata.join("harfbuzz"), "the HarfBuzz test data")
        .unwrap_or_else(|e| panic!("{e}"));
    paths::require(&ft_dump, "ft-dump").unwrap_or_else(|e| panic!("{e}"));

    let all = fonts::harfbuzz_fonts(&testdata);
    assert!(all.len() > 600, "found only {} fonts", all.len());
    let mut items = Vec::new();
    let mut tables = 0;
    for font in &all {
        let out = Command::new(&ft_dump)
            .arg("--otvalid")
            .arg(font)
            .output()
            .unwrap();
        if !out.status.success() {
            continue;
        }
        let out = String::from_utf8(out.stdout).unwrap();
        assert!(
            out.lines().next().is_some_and(|l| l.starts_with("face")),
            "ft-dump does not support --otvalid; rebuild it with tools/ft-dump/build.sh"
        );
        let bytes = std::fs::read(font).unwrap();
        for (face, tag, error) in parse(&out) {
            tables += 1;
            if error == 0 {
                continue;
            }
            let id = format!("{}#{face}:otvalid:{tag}", fonts::id(font, &testdata));
            items.push((id, sofel_rejects(&bytes, face, &tag)));
        }
    }
    eprintln!(
        "{} fonts, {tables} tables, FreeType rejects {}, sofel agrees on {}",
        all.len(),
        items.len(),
        items.iter().filter(|i| i.1).count()
    );
    assert!(tables > 1000, "only {tables} tables validated");

    let list = std::fs::read_to_string(paths::conformance_file(LIST)).unwrap_or_default();
    let expected = expect::parse(&list);
    let problems = expect::judge(items.iter().map(|(id, ok)| (id.as_str(), *ok)), &expected);
    assert!(problems.is_empty(), "{}", expect::report(&problems, LIST));
}
