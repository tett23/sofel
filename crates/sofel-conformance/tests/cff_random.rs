//! CFF `random` against FreeType and the validator over the test fonts
//! (ADR 0082): the value sequence of `random` against `ft-dump --calc`
//! (I3), the validation of every CFF face against the output of the
//! commit before ADR 0082 (I5), and the fonts of Golem's investigation
//! with the expected diagnostics of `conformance/cff-random-golem.tsv`
//! (A2).

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

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use build::*;
use sofel_conformance::{fonts, paths};
use sofel_read::Tag;
use sofel_read::cff::charstring::{self, Context, PathSink, Work};
use sofel_read::cff::dict::{NoBlend, TopDict};
use sofel_read::cff::{Cff1, FtBlend, Index, Version};
use sofel_read::sfnt::Sfnt;

use Cs::{F, I, Op};

const RANDOM: u16 = 0x0C17;

// ------------------------------------------------------------------ I3

/// The x of every line end, in 16.16 (the decoder's scale is 1).
#[derive(Default)]
struct Xs(Vec<i32>);

impl PathSink for Xs {
    fn move_to(&mut self) {}
    fn line_to(&mut self, _: (i32, i32), to: (i32, i32)) {
        self.0.push(to.0);
    }
    fn cubic_to(&mut self, _: (i32, i32), _: (i32, i32), _: (i32, i32), _: (i32, i32)) {}
}

/// The first `n` values sofel's decoder pushes for `random` from the
/// state `seed`: the glyph draws a line of each value along x.
fn sofel_values(seed: u32, n: usize) -> Vec<i32> {
    let mut items = vec![I(0), I(0), Op(21)];
    for _ in 0..n {
        items.extend([Op(RANDOM), I(0), Op(5)]);
    }
    items.push(Op(14));
    let glyph = charstring(&items);
    let empty = cff_index(&[], 1, false);
    let none = Index::new(&empty, Version::Cff).unwrap();
    let blend = FtBlend::new(None, &[]);
    let ctx = Context {
        version: Version::Cff,
        char_strings: none,
        charset: None,
        global_subrs: none,
        global_subrs_sorted: true,
        local_subrs: none,
        local_subrs_sorted: true,
        charstring_type: 2,
        default_width_x: 0,
        nominal_width_x: 0,
        max_stack: 48,
        has_variations: false,
        vsindex: 0,
        blend: &blend,
        random_seed: seed,
        scale: 0x1_0000,
    };
    let mut xs = Xs::default();
    charstring::decode(&glyph, &ctx, &mut Work::default(), &mut xs).unwrap();
    let mut prev = 0i32;
    xs.0.iter()
        .take(n)
        .map(|&x| {
            let v = x.wrapping_sub(prev);
            prev = x;
            v
        })
        .collect()
}

#[test]
fn adr0082_i3_random_values_match_freetype() {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    let seeds: [u32; 6] = [987_654_321, 1, 12345, 0xFFFF, 0x7FFF_FFFF, 0x8000_0000];
    let n = 1000;
    let mut child = Command::new(&exe)
        .arg("--calc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input: String = seeds.iter().map(|s| format!("random {s} {n}\n")).collect();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), seeds.len());
    for (seed, line) in seeds.iter().zip(lines) {
        let ft: Vec<i32> = line.split(' ').map(|v| v.parse().unwrap()).collect();
        assert_eq!(ft.len(), n);
        assert!(ft.iter().all(|&v| (1..=0x1_0000).contains(&v)));
        assert_eq!(sofel_values(*seed, n), ft, "seed {seed}");
    }
}

// ------------------------------------------------------------------ I5

fn validate_json(path: &Path) -> String {
    let args = vec![OsString::from("--json"), path.as_os_str().to_owned()];
    let (mut out, mut err) = (Vec::new(), Vec::new());
    sofel_cli::validate::run(&args, &mut out, &mut err);
    String::from_utf8(out).unwrap()
}

/// Whether a face of the sfnt or collection `bytes` has a CFF or CFF2
/// table.
fn has_cff(bytes: &[u8]) -> bool {
    let tables =
        |s: Sfnt<'_>| s.table(Tag::new(b"CFF ")).is_some() || s.table(Tag::new(b"CFF2")).is_some();
    if bytes.starts_with(b"ttcf") {
        let Ok(c) = sofel_read::sfnt::Collection::new(bytes) else {
            return false;
        };
        (0..c.num_fonts()).any(|i| c.font(i).is_ok_and(tables))
    } else {
        Sfnt::new(bytes).is_ok_and(tables)
    }
}

#[test]
fn adr0082_i5_cff_faces_validate_as_before() {
    let testdata = paths::testdata();
    paths::require(&testdata, "the test data").unwrap_or_else(|e| panic!("{e}"));
    let prefix = format!("{}/", testdata.display());
    let mut now = String::new();
    let mut count = 0;
    for font in fonts::harfbuzz_fonts(&testdata) {
        if !has_cff(&std::fs::read(&font).unwrap()) {
            continue;
        }
        count += 1;
        for line in validate_json(&font).lines() {
            if line.contains("\"kind\":\"cff_random_") {
                continue;
            }
            now.push_str(&line.replace(&prefix, ""));
            now.push('\n');
        }
    }
    assert_eq!(count, 297, "the number of fonts with CFF or CFF2 tables");
    let before =
        std::fs::read_to_string(paths::conformance_file("cff-validate-before-adr0082.jsonl"))
            .unwrap();
    let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), now.lines().collect());
    let differ: Vec<String> = a
        .iter()
        .zip(&b)
        .filter(|(x, y)| x != y)
        .take(5)
        .map(|(x, y)| format!("before: {x}\nnow:    {y}"))
        .collect();
    assert!(
        differ.is_empty() && a.len() == b.len(),
        "{} lines before, {} now\n{}",
        a.len(),
        b.len(),
        differ.join("\n")
    );
}

// ------------------------------------------------------------------ A2

/// FNV-1a 64 of `bytes`, to check that a rebuilt font is Golem's.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

/// Golem's `src/patch.py`: overwrites the charstring of the first glyph
/// from `gid` long enough for `prog` with `prog`, padded with `endchar`.
/// Returns the font and the glyph.
fn patch(font: &[u8], gid: u32, prog: &[u8]) -> (Vec<u8>, u32) {
    let table = Sfnt::new(font).unwrap().table(Tag::new(b"CFF ")).unwrap();
    let base = table.as_ptr() as usize - font.as_ptr() as usize;
    let cff = Cff1::new(table).unwrap();
    let top = TopDict::parse(cff.top_dicts().get(0).unwrap(), Version::Cff).unwrap();
    let at = top.char_strings as usize;
    let cs = Index::new(&table[at..], Version::Cff).unwrap();
    let glyph = (gid..cs.len())
        .find(|&g| cs.get(g).unwrap().len() >= prog.len())
        .unwrap();
    let start = base + at + cs.data_offset() + cs.offset(glyph).unwrap() as usize - 1;
    let len = cs.get(glyph).unwrap().len();
    let mut out = font.to_vec();
    out[start..start + len].fill(14);
    out[start..start + prog.len()].copy_from_slice(prog);
    (out, glyph)
}

/// `random 1000 mul random 1000 mul rmoveto 500 0 rlineto 0 500 rlineto
/// endchar`, the program of Golem's `*-random.otf`.
fn coordinates() -> Vec<u8> {
    vec![
        12, 23, 28, 3, 0xE8, 12, 24, 12, 23, 28, 3, 0xE8, 12, 24, 21, 28, 1, 0xF4, 139, 5, 139, 28,
        1, 0xF4, 5, 14,
    ]
}

/// The local subroutines of the Top DICT's Private DICT.
fn local_subrs(font: &[u8]) -> u32 {
    let table = Sfnt::new(font).unwrap().table(Tag::new(b"CFF ")).unwrap();
    let cff = Cff1::new(table).unwrap();
    let top = TopDict::parse(cff.top_dicts().get(0).unwrap(), Version::Cff).unwrap();
    let p = top.private_offset as usize;
    let private = sofel_read::cff::dict::PrivateDict::parse(
        &table[p..p + top.private_size as usize],
        Version::Cff,
        &NoBlend,
    )
    .unwrap();
    Index::new(&table[p + private.subrs as usize..], Version::Cff)
        .unwrap()
        .len()
}

/// The fonts of A2: Golem's three `*-random.otf` that the test data can
/// rebuild, and Source Sans Pro's glyph 44 with the programs of Golem's
/// review of its ADR 0390 and of that ADR's boundary table.
fn a2_fonts(testdata: &Path) -> Vec<(String, Vec<u8>, u32)> {
    let sans =
        std::fs::read(testdata.join("harfbuzz/test/api/fonts/SourceSansPro-Regular.otf")).unwrap();
    let han = std::fs::read(
        testdata.join("harfbuzz/test/subset/data/fonts/SourceHanSans-Regular_subset.otf"),
    )
    .unwrap();
    let mut out = Vec::new();
    let (s, g) = patch(&sans, 44, &coordinates());
    assert_eq!(fnv(&s), 0xefea_3714_9fe5_2249, "sans-random.otf");
    out.push(("sans-random".to_string(), s, g));
    let (h, g) = patch(&han, 5, &coordinates());
    assert_eq!(fnv(&h), 0x9b44_ef00_6717_8828, "han-random.otf");
    out.push(("han-random".to_string(), h, g));

    // The largest subroutine number of Source Sans Pro.
    let n = local_subrs(&sans) as i32 - 1 - sofel_read::cff::subr_bias(local_subrs(&sans));
    let simple = [
        I(1000),
        Op(0x0C18),
        I(0),
        Op(21),
        I(500),
        I(0),
        Op(5),
        Op(14),
    ];
    let with_simple = |head: &[Cs]| {
        let mut v = head.to_vec();
        v.extend(simple.iter().cloned());
        charstring(&v)
    };
    let programs: Vec<(&str, Vec<u8>)> = vec![
        ("simple", with_simple(&[Op(RANDOM)])),
        (
            "callsubr",
            charstring(&[Op(RANDOM), I(1), Op(0x0C18), Op(10), Op(14)]),
        ),
        ("intcall", charstring(&[I(-107), Op(10), Op(14)])),
        (
            "ifelse",
            charstring(&[
                I(-107),
                I(-107),
                Op(RANDOM),
                Op(RANDOM),
                Op(0x0C16),
                Op(10),
                Op(14),
            ]),
        ),
        (
            "getrand",
            with_simple(&[Op(RANDOM), Op(0x0C15), Op(0x0C12), Op(RANDOM)]),
        ),
        (
            "idxrand",
            with_simple(&[
                I(0),
                Op(RANDOM),
                Op(0x0C1D),
                Op(0x0C12),
                Op(0x0C12),
                Op(RANDOM),
            ]),
        ),
        (
            "div0",
            with_simple(&[I(1), I(0), Op(0x0C0C), Op(0x0C12), Op(RANDOM)]),
        ),
        (
            "sqrtneg",
            with_simple(&[I(-1), Op(0x0C1A), Op(0x0C12), Op(RANDOM)]),
        ),
        (
            "random-k-mul-callsubr-n",
            charstring(&[Op(RANDOM), I(n), Op(0x0C18), Op(10), Op(14)]),
        ),
        (
            "random-k-mul-callsubr-n+1",
            charstring(&[Op(RANDOM), I(n + 1), Op(0x0C18), Op(10), Op(14)]),
        ),
        (
            "one-over-random-callsubr",
            charstring(&[I(1), Op(RANDOM), Op(0x0C0C), Op(10), Op(14)]),
        ),
        (
            "divide-by-random-minus-half",
            with_simple(&[
                I(1),
                Op(RANDOM),
                F(0x8000),
                Op(0x0C0B),
                Op(0x0C0C),
                Op(0x0C12),
                Op(RANDOM),
            ]),
        ),
        (
            "sqrt-random-minus-half",
            with_simple(&[
                Op(RANDOM),
                F(0x8000),
                Op(0x0C0B),
                Op(0x0C1A),
                Op(0x0C12),
                Op(RANDOM),
            ]),
        ),
        (
            "ifelse-one-sided-call",
            charstring(&[
                I(n + 1),
                I(n),
                Op(RANDOM),
                F(0x8000),
                Op(0x0C16),
                Op(10),
                Op(14),
            ]),
        ),
    ];
    for (name, prog) in programs {
        let (f, g) = patch(&sans, 44, &prog);
        assert_eq!(g, 44, "{name} fits glyph 44");
        out.push((name.to_string(), f, g));
    }
    out
}

/// `font glyph kind` of each sofel diagnostic of the interval check, and
/// `font - -` for a font without one.
fn a2_lines(testdata: &Path) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!("sofel-a2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut out = Vec::new();
    for (name, bytes, glyph) in a2_fonts(testdata) {
        let p: PathBuf = dir.join(format!("{name}.otf"));
        std::fs::write(&p, &bytes).unwrap();
        let json = validate_json(&p);
        let found: Vec<String> = json
            .lines()
            .filter_map(|l| {
                let k = l.split("\"kind\":\"").nth(1)?.split('"').next()?;
                k.starts_with("cff_random_").then(|| {
                    let g = l
                        .split("\"glyph\":")
                        .nth(1)
                        .unwrap()
                        .split([',', '}'])
                        .next()
                        .unwrap();
                    format!("{name}\t{g}\t{k}")
                })
            })
            .collect();
        if found.is_empty() {
            out.push(format!("{name}\t{glyph}\t-"));
        }
        out.extend(found);
    }
    std::fs::remove_dir_all(&dir).ok();
    out
}

#[test]
fn adr0082_a2_golem_fonts() {
    let testdata = paths::testdata();
    paths::require(&testdata, "the test data").unwrap_or_else(|e| panic!("{e}"));
    let expected: Vec<String> =
        std::fs::read_to_string(paths::conformance_file("cff-random-golem.tsv"))
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .map(|l| l.split('\t').take(3).collect::<Vec<_>>().join("\t"))
            .collect();
    assert_eq!(a2_lines(&testdata), expected);
}
