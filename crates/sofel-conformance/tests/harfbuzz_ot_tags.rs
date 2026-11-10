//! sofel-shape's OpenType script and language tags against HarfBuzz
//! (ADR 0020): every line `hb-dump --ot-tags` writes must match. There
//! is no list of expected failures; one difference fails the test.
//!
//! The one documented difference is the script of a script tag that
//! names no ISO 15924 shaped script: HarfBuzz returns the tag value
//! itself, sofel [`Script::UNKNOWN`] (see `sofel_shape::ot_tag_to_script`).
//! The comparison maps HarfBuzz's value the same way; the language,
//! which carries the script tag, is compared as is.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::collections::BTreeSet;
use std::process::Command;

use sofel_conformance::paths;
use sofel_font::read::Tag;
use sofel_shape::{
    Language, Script, ot_tag_to_language, ot_tag_to_script, ot_tags_from_script_and_language,
    ot_tags_from_script_and_language_max, ot_tags_to_script_and_language,
};

fn hb_dump_ot_tags() -> String {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe)
        .arg("--ot-tags")
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(out.status.success(), "hb-dump --ot-tags failed");
    String::from_utf8(out.stdout).unwrap()
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap_or_else(|e| panic!("{s}: {e}"))
}

fn fmt_tags(v: &[Tag]) -> String {
    if v.is_empty() {
        "-".to_string()
    } else {
        v.iter()
            .map(|t| format!("{:08x}", t.to_u32()))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn fmt_language(l: Option<&Language>) -> String {
    l.map_or("*".to_string(), |l| l.as_str().to_string())
}

/// A script of the `scripts` line, as sofel takes it.
fn script(raw: u32) -> Option<Script> {
    if raw == 0 {
        return None;
    }
    let s = Script::from_iso15924(&raw.to_be_bytes()).unwrap();
    assert_eq!(s.tag(), raw.to_be_bytes(), "{raw:08x}");
    Some(s)
}

/// A script HarfBuzz returns, as sofel gives it.
fn expected_script(raw: u32) -> Option<Script> {
    if raw == 0 {
        return None;
    }
    let b = raw.to_be_bytes();
    Some(
        Script::from_iso15924(&b)
            .filter(|s| s.tag() == b)
            .unwrap_or(Script::UNKNOWN),
    )
}

fn fmt_script(s: Option<Script>) -> String {
    s.map_or("00000000".to_string(), |s| {
        format!("{:08x}", u32::from_be_bytes(s.tag()))
    })
}

struct Report {
    lines: usize,
    failures: Vec<String>,
    failed: usize,
}

impl Report {
    fn check(&mut self, line: &str, want: String, got: String) {
        self.lines += 1;
        if want != got {
            self.failed += 1;
            if self.failures.len() < 40 {
                self.failures
                    .push(format!("{line}\n    harfbuzz {want}\n    sofel    {got}"));
            }
        }
    }
}

/// The `Branch` lines of the committed `tag_table.rs`, in hb-dump's
/// `branch` notation.
fn generated_branches() -> BTreeSet<String> {
    let path = paths::root().join("crates/sofel-shape/src/tag_table.rs");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut out = BTreeSet::new();
    let between = |s: &str, a: &str, b: &str| -> String {
        let i = s.find(a).unwrap() + a.len();
        let j = s[i..].find(b).unwrap() + i;
        s[i..j].to_string()
    };
    for l in text.lines() {
        if let Some(rest) = l.trim().strip_prefix("Branch { first: b'") {
            let first = &rest[..1];
            if rest.contains("Cond::LangMatches(") {
                out.insert(format!(
                    "l {first}{}",
                    between(rest, "LangMatches(b\"", "\"")
                ));
            } else if rest.contains("Cond::Exact(") {
                out.insert(format!("e {first}{}", between(rest, "Exact(b\"", "\"")));
            } else if rest.contains("Cond::PrefixSubtag(") {
                out.insert(format!(
                    "n {first}{}",
                    between(rest, "PrefixSubtag(b\"", "\"")
                ));
                out.insert(format!("s {}", between(rest, "\", b\"", "\"")));
            } else {
                panic!("unknown branch {l}");
            }
        } else if let Some(rest) = l.trim().strip_prefix("(b\"-") {
            out.insert(format!("p -{}", &rest[..rest.find('"').unwrap()]));
        }
    }
    out
}

#[test]
fn ot_tags_agree_with_harfbuzz() {
    let text = hb_dump_ot_tags();
    let mut r = Report {
        lines: 0,
        failures: Vec::new(),
        failed: 0,
    };
    let mut scripts: Vec<Option<Script>> = Vec::new();
    let mut branches = BTreeSet::new();
    let mut counts = [0usize; 5];
    let mut ended = false;
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "scripts" => {
                scripts = f[1].split(',').map(|t| script(hex(t))).collect();
            }
            "branch" => {
                branches.insert(format!("{} {}", f[1], f[2]));
            }
            "f" => {
                counts[0] += 1;
                let lang = Language::new(f[1].as_bytes());
                r.check(line, f[2].to_string(), fmt_language(lang.as_ref()));
                assert_eq!(f.len(), 4 + scripts.len(), "{line}");
                for (i, &s) in scripts.iter().enumerate() {
                    let (st, lt) = ot_tags_from_script_and_language(s, lang.as_ref());
                    let want = format!("{} {}", f[3], f[4 + i]);
                    let got = format!("{} {}", fmt_tags(&lt), fmt_tags(&st));
                    r.check(&format!("{} ({})", f[1], fmt_script(s)), want, got);
                }
            }
            "c" => {
                counts[1] += 1;
                let lang = Language::new(f[1].as_bytes());
                let s = script(hex(f[2]));
                let (sc, lc) = (f[3].parse().unwrap(), f[4].parse().unwrap());
                let (st, lt) = ot_tags_from_script_and_language_max(s, lang.as_ref(), sc, lc);
                r.check(
                    line,
                    format!("{} {}", f[5], f[6]),
                    format!("{} {}", fmt_tags(&st), fmt_tags(&lt)),
                );
            }
            "s" => {
                counts[2] += 1;
                let got = ot_tag_to_script(Tag::from_u32(hex(f[1])));
                r.check(
                    line,
                    fmt_script(expected_script(hex(f[2]))),
                    fmt_script(got),
                );
            }
            "l" => {
                counts[3] += 1;
                let got = ot_tag_to_language(Tag::from_u32(hex(f[1])));
                r.check(line, f[2].to_string(), fmt_language(got.as_ref()));
            }
            "r" => {
                counts[4] += 1;
                let (s, l) = ot_tags_to_script_and_language(
                    Tag::from_u32(hex(f[1])),
                    Some(Tag::from_u32(hex(f[2]))),
                );
                r.check(
                    line,
                    format!("{} {}", fmt_script(expected_script(hex(f[3]))), f[4]),
                    format!("{} {}", fmt_script(s), fmt_language(l.as_ref())),
                );
            }
            "end" => ended = true,
            _ => panic!("unknown line {line}"),
        }
    }
    assert!(ended, "hb-dump --ot-tags stopped early");
    eprintln!(
        "{} comparisons: {} f, {} c, {} s, {} l, {} r lines, {} scripts",
        r.lines,
        counts[0],
        counts[1],
        counts[2],
        counts[3],
        counts[4],
        scripts.len()
    );

    // Every script of sofel-unicode, and the three without characters.
    let mut ours: BTreeSet<Option<Script>> = (0..=0x10FFFF)
        .map(|cp| Some(sofel_unicode::script(cp)))
        .collect();
    ours.extend([
        None,
        Script::from_iso15924(b"Zmth"),
        Script::from_iso15924(b"Hrkt"),
    ]);
    assert_eq!(scripts.iter().copied().collect::<BTreeSet<_>>(), ours);

    // The generator read the same branches HarfBuzz runs.
    assert_eq!(branches, generated_branches());

    assert!(counts[0] > 10_000 && counts[1] > 30_000 && counts[4] > 50_000);
    assert!(
        r.failed == 0,
        "{} of {} differ:\n{}",
        r.failed,
        r.lines,
        r.failures.join("\n")
    );
}
