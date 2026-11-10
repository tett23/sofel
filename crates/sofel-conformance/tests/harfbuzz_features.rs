//! sofel's feature strings against HarfBuzz (ADR 0020, 機能の文字列):
//! `hb-dump --features` parses every string of the set below with
//! `hb_feature_from_string` and writes it back with
//! `hb_feature_to_string`; `Feature::parse` and `Feature::format` must
//! agree on every one. The set is the direct product of tags, prefixes,
//! ranges, values and surrounding whitespace, then extra cases; hb-dump
//! builds it from the same tables, and the strings it writes are checked
//! against these. There is no list of expected failures.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::process::Command;

use sofel_conformance::paths;
use sofel_shape::Feature;

const TAGS: [&str; 10] = [
    "kern",
    "aalt",
    "a",
    "abc",
    "abcde",
    "'liga'",
    "\"liga\"",
    "'abc'",
    "\"abcde\"",
    "'liga\"",
];
const PREFIXES: [&str; 3] = ["", "+", "-"];
const RANGES: [&str; 9] = ["", "[]", "[:]", "[3]", "[3:]", "[:5]", "[3:5]", "[3", "[x]"];
const VALUES: [&str; 10] = [
    "", "=0", "=2", " on", " off", " 2", "=", "=on", " xyz", "=-1",
];
const SPACES: [(&str, &str); 4] = [("", ""), (" ", ""), ("", " "), ("\t ", " \n")];
const EXTRAS: &[&[u8]] = &[
    b"abcd",
    b"abcd=1",
    b"+abcd",
    b"abcd=0",
    b"-abcd",
    b"abcd=2",
    b"+abcd=2",
    b"-abcd=2",
    b"\"abcd\" on",
    b"\"abcd\" off",
    b"\"abcd\" 1",
    b"\"abcd\" 0",
    b"\"abcd\" 2",
    b"abcd[0]",
    b"abcd[1]",
    b"abcd[1]=1",
    b"abcd[1]=2",
    b"abcd[1]=0",
    b"abcd[]",
    b"abcd[:]",
    b"abcd[1:]",
    b"abcd[:1]",
    b"abcd[1:3]",
    b"abcd[1:2]=1",
    b"abcd[1:4]=2",
    b"aalt[3:5]=2",
    b"kern\t",
    b"kern\t=2",
    b"kern[ 3 : 5 ]",
    b"kern[3;5]",
    b"kern[-1]",
    b"kern[-1:]",
    b"kern[4294967295]",
    b"kern[2147483647]",
    b"kern[2147483648]",
    b"kern=2147483647",
    b"kern=2147483648",
    b"kern=-2147483648",
    b"kern=+2",
    b"kern = 2",
    b"kern=0x10",
    b"kern=010",
    b"kern  ON",
    b"kern OfF",
    b"kern onx",
    b"kern=off",
    b"kern=ON",
    b"-+kern",
    b"+-kern",
    b"--kern",
    b"",
    b" ",
    b"=",
    b"[]",
    b"-",
    b"+",
    b"'kern' 'x'",
    b"'ke\"n'",
    b"\"kern'",
    b"kern'",
    b"k=rn",
    b"k[rn",
    b"'k rn'",
    b"'k[rn'",
    b"'k=rn'",
    b"k:rn",
    b"k]rn",
    b"k\trn",
    b"\xc3\xa9t\xc3",
    b"\xc3\xa9t\xc3\xa9",
    b"\xff\xfe",
    b"\x0b kern \x0c",
    b"kern[3:5]=2 ",
    b"kern[3:5] =2",
    b"kern [3:5]",
    b"kern[3:5]x",
    b"kern[3:5",
    b"kern[3:5]]",
    b"kern[:]=3",
    b"kern[0:]",
    b"kern[0:1]",
    b"kern[5:3]",
    b"kern[3:3]",
    b"kern==2",
    b"kern=2=3",
    b"kern 2 3",
    b"kern on off",
    b"'kern'on",
    b"'kern'=2",
    b"\"kern\"[1:2]",
    b"''",
    b"\"\"",
    b"'",
    b"\"",
    b"kern'x",
    b"liga=4294967295",
    b"liga=-0",
    b"liga[+3]",
    b"liga[ -2 ]",
    b"liga[3:-1]",
];

/// The strings, in hb-dump's order.
fn strings() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for t in TAGS {
        for p in PREFIXES {
            for r in RANGES {
                for v in VALUES {
                    for (lead, trail) in SPACES {
                        out.push(format!("{lead}{p}{t}{r}{v}{trail}").into_bytes());
                    }
                }
            }
        }
    }
    out.extend(EXTRAS.iter().map(|s| s.to_vec()));
    out
}

/// Undoes hb-dump's escaping (after the `=`).
fn unescape(s: &str) -> Vec<u8> {
    let s = s
        .strip_prefix('=')
        .expect("an escaped string starts with =");
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && b.get(i + 1) == Some(&b'x') {
            out.push(u8::from_str_radix(&s[i + 2..i + 4], 16).unwrap());
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// What hb-dump writes for one string: the input, and the feature with
/// its string when it parses.
#[derive(Debug, PartialEq)]
struct Line {
    input: Vec<u8>,
    parsed: Option<(u32, u32, u32, u32, Vec<u8>)>,
}

fn parse(text: &str) -> Vec<Line> {
    text.lines()
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            assert_eq!(f[0], "feature", "{l}");
            let input = unescape(f[1]);
            let parsed = match f[2] {
                "0" => None,
                _ => Some((
                    u32::from_str_radix(f[3], 16).unwrap(),
                    f[4].parse().unwrap(),
                    f[5].parse().unwrap(),
                    f[6].parse().unwrap(),
                    unescape(f[7]),
                )),
            };
            Line { input, parsed }
        })
        .collect()
}

fn ours(input: &[u8]) -> Line {
    Line {
        input: input.to_vec(),
        parsed: Feature::parse(input)
            .map(|f| (f.tag.to_u32(), f.value, f.start, f.end, f.format())),
    }
}

#[test]
fn features_agree_with_harfbuzz() {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe)
        .arg("--features")
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(out.status.success(), "hb-dump --features failed");
    let hb = parse(&String::from_utf8(out.stdout).unwrap());
    let inputs = strings();
    assert_eq!(hb.len(), inputs.len());
    assert_eq!(
        hb.iter().map(|l| l.input.clone()).collect::<Vec<_>>(),
        inputs,
        "hb-dump's strings differ from this test's"
    );
    let parsed = hb.iter().filter(|l| l.parsed.is_some()).count();
    eprintln!("{} strings, {} parse", hb.len(), parsed);
    // Both outcomes are well represented.
    assert!(parsed > 1000 && hb.len() - parsed > 1000);
    let failed: Vec<String> = hb
        .iter()
        .filter_map(|l| {
            let o = ours(&l.input);
            (o != *l).then(|| {
                format!(
                    "{:?}: HarfBuzz {:?}, sofel {:?}",
                    String::from_utf8_lossy(&l.input),
                    l.parsed,
                    o.parsed
                )
            })
        })
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
