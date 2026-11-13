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
#[path = "../../sofel-type1/tests/support/type1_triangle.rs"]
mod type1_fixture;

use build::*;
use sofel_font::{Cff, Font, HbCff2};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let d = base.join(format!("sofel-validate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[&std::ffi::OsStr]) -> Out {
    let o = Command::new(env!("CARGO_BIN_EXE_sofel-validate"))
        .args(args)
        .output()
        .unwrap();
    Out {
        code: o.status.code().unwrap(),
        stdout: String::from_utf8(o.stdout).unwrap(),
        stderr: String::from_utf8(o.stderr).unwrap(),
    }
}

fn os(p: &Path) -> &std::ffi::OsStr {
    p.as_os_str()
}

/// A font with a Warning (bad magic) and an Error (indexToLocFormat 2) in head.
fn broken_font() -> Vec<u8> {
    let mut tables = sample_tables();
    for (t, d) in tables.iter_mut() {
        if *t == b"head" {
            *d = head(HeadSpec {
                magic: 1,
                index_to_loc_format: 2,
                ..Default::default()
            });
        }
    }
    sfnt(&tables)
}

fn warning_font() -> Vec<u8> {
    let mut tables = sample_tables();
    for (t, d) in tables.iter_mut() {
        if *t == b"head" {
            *d = head(HeadSpec {
                magic: 1,
                ..Default::default()
            });
        }
    }
    sfnt(&tables)
}

fn cff2_fdselect_font(format: u8, sentinel: u32) -> Vec<u8> {
    let fd_select = match format {
        0 => vec![0, 0, 1, 1],
        3 => vec![3, 0, 2, 0, 0, 0, 0, 1, 1, 0, 3],
        4 => {
            let mut bytes = vec![4];
            bytes.extend(2u32.to_be_bytes());
            bytes.extend(0u32.to_be_bytes());
            bytes.extend(0u16.to_be_bytes());
            bytes.extend(1u32.to_be_bytes());
            bytes.extend(1u16.to_be_bytes());
            bytes.extend(sentinel.to_be_bytes());
            bytes
        }
        _ => unreachable!(),
    };
    otto_font(
        3,
        vec![(
            b"CFF2",
            cff2(&Cff2Spec {
                charstrings: vec![
                    Vec::new(),
                    charstring(&[Cs::I(0), Cs::Op(22)]),
                    charstring(&[Cs::I(0), Cs::Op(22)]),
                ],
                fds: vec![(Vec::new(), Vec::new()); 2],
                fd_select,
                ..Default::default()
            }),
        )],
    )
}

#[test]
fn inv2_validate_fdselect_exit() {
    let dir = Dir::new("cff2-fdselect");
    for (format, sentinel, code) in [(4, 3, 0), (4, 4, 1), (0, 3, 0), (3, 3, 0)] {
        let bytes = cff2_fdselect_font(format, sentinel);
        let path = dir.file(&format!("format-{format}-{sentinel}.otf"), &bytes);
        let out = run(&[std::ffi::OsStr::new("--json"), os(&path)]);
        assert_eq!(out.code, code, "{}", out.stdout);
        assert_eq!(
            out.stdout.contains("fd_select_format4"),
            format == 4,
            "{}",
            out.stdout
        );
    }
}

#[test]
fn inv3_equivalent_fdselect() {
    for format in [0, 3, 4] {
        let bytes = cff2_fdselect_font(format, 3);
        let font = Font::new(&bytes, 0).unwrap();
        let cff = HbCff2::new(&font).unwrap();
        assert_eq!([0, 1, 2].map(|g| cff.fd(g)), [0, 1, 1]);
        assert_eq!(Cff::new(&font).is_ok(), format != 4);
    }
}

#[test]
fn valid_font_prints_nothing() {
    let d = Dir::new("valid");
    let p = d.file("ok.ttf", &sample_font());
    let o = run(&[os(&p)]);
    assert_eq!((o.code, o.stdout.as_str(), o.stderr.as_str()), (0, "", ""));
}

#[test]
fn errors_and_warnings_in_text() {
    let d = Dir::new("text");
    let p = d.file("broken.ttf", &broken_font());
    let o = run(&[os(&p)]);
    assert_eq!(o.code, 1);
    let lines: Vec<&str> = o.stdout.lines().collect();
    let path = p.display().to_string();
    assert_eq!(lines.len(), 2, "{}", o.stdout);
    assert!(
        lines[0].starts_with(&format!("{path}: head+0xc: warning: ")),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].starts_with(&format!("{path}: head+0x32: error: ")),
        "{}",
        lines[1]
    );
}

#[test]
fn warnings_only_exit_zero() {
    let d = Dir::new("warn");
    let p = d.file("warn.ttf", &warning_font());
    let o = run(&[os(&p)]);
    assert_eq!(o.code, 0);
    assert_eq!(o.stdout.lines().count(), 1);
}

#[test]
fn collections_number_their_fonts() {
    let d = Dir::new("ttc");
    let mut second = sample_tables();
    for (t, data) in second.iter_mut() {
        if *t == b"head" {
            *data = head(HeadSpec {
                magic: 1,
                ..Default::default()
            });
        }
    }
    let p = d.file("c.ttc", &ttc(true, &[sample_tables(), second]));
    let o = run(&[os(&p)]);
    assert_eq!(o.code, 0);
    let path = p.display().to_string();
    assert_eq!(o.stdout.lines().count(), 1, "{}", o.stdout);
    assert!(
        o.stdout
            .starts_with(&format!("{path}#1: head+0xc: warning: ")),
        "{}",
        o.stdout
    );
}

#[test]
fn unreadable_files_exit_two_but_others_are_checked() {
    let d = Dir::new("missing");
    let p = d.file("warn.ttf", &warning_font());
    let missing = d.0.join("missing.ttf");
    for json in [false, true] {
        let mut args = vec![os(&missing), os(&p)];
        if json {
            args.insert(0, std::ffi::OsStr::new("--json"));
        }
        let o = run(&args);
        assert_eq!(o.code, 2);
        assert!(o.stderr.contains("missing.ttf"), "{}", o.stderr);
        assert_eq!(o.stdout.lines().count(), 1);
    }
}

#[test]
fn not_a_font_is_an_error() {
    let d = Dir::new("notfont");
    let p = d.file("x.ttf", b"hello, world");
    let o = run(&[os(&p)]);
    assert_eq!(o.code, 1);
    assert!(o.stdout.contains(": sfnt+0x0: error: "), "{}", o.stdout);
}

#[test]
fn usage_errors() {
    assert_eq!(run(&[]).code, 2);
    assert_eq!(run(&[std::ffi::OsStr::new("--bogus")]).code, 2);
    let h = run(&[std::ffi::OsStr::new("--help")]);
    assert_eq!(h.code, 0);
    for word in [
        "--json", "path", "font", "table", "offset", "severity", "kind", "message", "details",
    ] {
        assert!(h.stdout.contains(word), "help lacks {word}");
    }
}

#[test]
fn exit_codes_do_not_depend_on_json() {
    let d = Dir::new("codes");
    for (name, font) in [
        ("a.ttf", sample_font()),
        ("b.ttf", warning_font()),
        ("c.ttf", broken_font()),
    ] {
        let p = d.file(name, &font);
        let text = run(&[os(&p)]).code;
        let json = run(&[std::ffi::OsStr::new("--json"), os(&p)]).code;
        assert_eq!(text, json, "{name}");
    }
}

#[test]
fn json_lines() {
    let d = Dir::new("json");
    // A name with a quote, a backslash, a newline, a control character
    // and non-ASCII text, to check the escaping.
    let name = "font \"q\" \\ \n\u{1}フォント.ttf";
    let p = d.file(name, &broken_font());
    let o = run(&[std::ffi::OsStr::new("--json"), os(&p)]);
    assert_eq!(o.code, 1);
    let lines: Vec<&str> = o.stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{}", o.stdout);
    let first = parse(lines[0]);
    let expect_path = p.display().to_string();
    assert_eq!(first["path"], J::Str(expect_path));
    assert_eq!(first["font"], J::Null);
    assert_eq!(first["table"], J::Str("head".into()));
    assert_eq!(first["offset"], J::Num(12.0));
    assert_eq!(first["severity"], J::Str("warning".into()));
    assert_eq!(first["kind"], J::Str("bad_magic_number".into()));
    assert!(matches!(&first["message"], J::Str(m) if !m.is_empty()));
    let J::Obj(details) = &first["details"] else {
        panic!("details is not an object")
    };
    assert_eq!(details["expected"], J::Num(f64::from(0x5F0F_3CF5u32)));
    assert_eq!(details["found"], J::Num(1.0));
    let second = parse(lines[1]);
    assert_eq!(second["severity"], J::Str("error".into()));
    assert_eq!(second["kind"], J::Str("invalid_index_to_loc_format".into()));
    assert!(!lines[0].contains('\n'));
}

#[test]
fn json_fonts_in_collections() {
    let d = Dir::new("jsonttc");
    let mut second = sample_tables();
    for (t, data) in second.iter_mut() {
        if *t == b"head" {
            *data = head(HeadSpec {
                magic: 1,
                ..Default::default()
            });
        }
    }
    let p = d.file("c.ttc", &ttc(false, &[sample_tables(), second]));
    let o = run(&[std::ffi::OsStr::new("--json"), os(&p)]);
    let v = parse(o.stdout.lines().next().unwrap());
    assert_eq!(v["font"], J::Num(1.0));
}

// ------------------------------------------------------------ a small JSON parser

#[derive(Debug, PartialEq, Clone)]
enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(BTreeMap<String, J>),
}

impl std::ops::Index<&str> for J {
    type Output = J;
    fn index(&self, k: &str) -> &J {
        match self {
            J::Obj(m) => m.get(k).unwrap_or_else(|| panic!("no key {k}")),
            _ => panic!("not an object"),
        }
    }
}

fn parse(s: &str) -> BTreeMap<String, J> {
    let mut p = P {
        s: s.as_bytes(),
        i: 0,
    };
    let v = p.value();
    p.ws();
    assert_eq!(p.i, p.s.len(), "trailing data in {s}");
    match v {
        J::Obj(m) => m,
        _ => panic!("not an object: {s}"),
    }
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && b" \t\r\n".contains(&self.s[self.i]) {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) {
        self.ws();
        assert_eq!(
            self.s.get(self.i),
            Some(&c),
            "expected {} at {}",
            c as char,
            self.i
        );
        self.i += 1;
    }
    fn value(&mut self) -> J {
        self.ws();
        match self.s[self.i] {
            b'{' => {
                self.i += 1;
                let mut m = BTreeMap::new();
                self.ws();
                if self.s[self.i] == b'}' {
                    self.i += 1;
                    return J::Obj(m);
                }
                loop {
                    self.ws();
                    let J::Str(k) = self.string() else {
                        unreachable!()
                    };
                    self.eat(b':');
                    let v = self.value();
                    assert!(m.insert(k.clone(), v).is_none(), "duplicate key {k}");
                    self.ws();
                    if self.s[self.i] == b',' {
                        self.i += 1;
                    } else {
                        self.eat(b'}');
                        return J::Obj(m);
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.s[self.i] == b']' {
                    self.i += 1;
                    return J::Arr(a);
                }
                loop {
                    a.push(self.value());
                    self.ws();
                    if self.s[self.i] == b',' {
                        self.i += 1;
                    } else {
                        self.eat(b']');
                        return J::Arr(a);
                    }
                }
            }
            b'"' => self.string(),
            b'n' => self.word("null", J::Null),
            b't' => self.word("true", J::Bool(true)),
            b'f' => self.word("false", J::Bool(false)),
            _ => {
                let start = self.i;
                while self.i < self.s.len() && b"-+.eE0123456789".contains(&self.s[self.i]) {
                    self.i += 1;
                }
                J::Num(
                    std::str::from_utf8(&self.s[start..self.i])
                        .unwrap()
                        .parse()
                        .unwrap(),
                )
            }
        }
    }
    fn word(&mut self, w: &str, v: J) -> J {
        assert!(self.s[self.i..].starts_with(w.as_bytes()));
        self.i += w.len();
        v
    }
    fn string(&mut self) -> J {
        assert_eq!(self.s[self.i], b'"');
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let c = self.s[self.i];
            self.i += 1;
            match c {
                b'"' => return J::Str(String::from_utf8(out).unwrap()),
                b'\\' => {
                    let e = self.s[self.i];
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let hex = std::str::from_utf8(&self.s[self.i..self.i + 4]).unwrap();
                            self.i += 4;
                            let ch = char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap();
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => panic!("bad escape"),
                    }
                }
                c if c < 0x20 => panic!("unescaped control character {c:#x}"),
                c => out.push(c),
            }
        }
    }
}

fn damaged_layout_font() -> Vec<u8> {
    let mut tables = sample_tables();
    tables.push((b"GSUB", vec![0, 1, 0, 0, 0, 255, 0, 254, 0, 0]));
    tables.push((b"GPOS", vec![0, 1, 0, 0, 0, 253, 0, 0, 0, 0]));
    tables.sort_by_key(|(tag, _)| **tag);
    sfnt(&tables)
}

#[test]
fn inv2_layout_discard_diagnostic() {
    let d = Dir::new("layout-discard");
    let p = d.file("damaged.ttf", &damaged_layout_font());
    for output in [run(&[os(&p)]), run(&["--json".as_ref(), os(&p)])] {
        for tag in ["GSUB", "GPOS"] {
            let msg = format!("shaping ignores the {tag} table after sanitization failed");
            assert_eq!(output.stdout.matches(&msg).count(), 1, "{}", output.stdout);
        }
    }
}

#[test]
fn inv3_original_layout_diagnostics() {
    let d = Dir::new("original-layout");
    let p = d.file("damaged.ttf", &damaged_layout_font());
    let out = run(&["--json".as_ref(), os(&p)]);
    assert_eq!(out.code, 1);
    assert!(
        out.stdout.contains("offset_out_of_bounds"),
        "{}",
        out.stdout
    );
}

#[test]
fn inv4_all_layout_damage() {
    let d = Dir::new("layout-damage");
    let p = d.file("damaged.ttf", &damaged_layout_font());
    let out = run(&["--json".as_ref(), os(&p)]);
    assert!(out.stdout.contains("\"damage_count\":3"), "{}", out.stdout);
    assert!(out.stdout.contains("\"complete\":false"));
    let summary = parse(out.stdout.lines().last().unwrap());
    assert_eq!(summary["kind"], J::Str("layout_damage_summary".into()));
    assert_eq!(summary["damage_count"], J::Num(3.0));
    assert_eq!(summary["complete"], J::Bool(false));
    let J::Arr(locations) = &summary["locations"] else {
        panic!("locations is not an array")
    };
    assert_eq!(locations.len(), 3);
    let J::Arr(unchecked) = &summary["unchecked"] else {
        panic!("unchecked is not an array")
    };
    assert_eq!(unchecked.len(), 3);
    for location in locations {
        let J::Obj(fields) = location else {
            panic!("location is not an object")
        };
        assert!(matches!(&fields["table"], J::Str(t) if t == "GSUB" || t == "GPOS"));
        assert!(matches!(&fields["reasons"], J::Arr(r) if !r.is_empty()));
    }
    let out = run(&[os(&p)]);
    assert!(out.stdout.contains("at least 3"), "{}", out.stdout);
}

#[test]
fn inv4_layout_damage_is_per_face() {
    let mut damaged = sample_tables();
    damaged.push((b"GSUB", vec![0, 1, 0, 0, 0, 255, 0, 0, 0, 0]));
    damaged.sort_by_key(|(tag, _)| **tag);
    let d = Dir::new("layout-damage-faces");
    let p = d.file("faces.ttc", &ttc(false, &[sample_tables(), damaged]));
    let out = run(&["--json".as_ref(), os(&p)]);
    let summary = out
        .stdout
        .lines()
        .map(parse)
        .find(|line| line["kind"] == J::Str("layout_damage_summary".into()))
        .unwrap();
    assert_eq!(summary["font"], J::Num(1.0));
    assert_eq!(summary["damage_count"], J::Num(1.0));
}

#[test]
fn inv4_layout_damage_total_counts_two_faces() {
    let mut damaged = sample_tables();
    damaged.push((b"GSUB", vec![0, 1, 0, 0, 0, 255, 0, 0, 0, 0]));
    damaged.sort_by_key(|(tag, _)| **tag);
    let d = Dir::new("layout-damage-total");
    let face = sfnt(&damaged);
    let resources = dfont(&[(b"sfnt", vec![(12, face.clone()), (3, face)])]);
    let collection = ttc(false, &[damaged.clone(), damaged]);
    for p in [
        d.file("faces.ttc", &collection),
        d.file("faces.dfont", &resources),
    ] {
        let out = run(&["--json".as_ref(), os(&p)]);
        let total = parse(out.stdout.lines().last().unwrap());
        assert_eq!(total["kind"], J::Str("layout_damage_total".into()));
        assert_eq!(total["damage_count"], J::Num(2.0));
        assert_eq!(total["complete"], J::Bool(false));
        let J::Arr(locations) = &total["locations"] else {
            panic!("locations is not an array")
        };
        assert_eq!(locations.len(), 2);
        let font_ids: Vec<_> = locations
            .iter()
            .map(|location| {
                let J::Obj(fields) = location else {
                    panic!("location is not an object")
                };
                fields["font"].clone()
            })
            .collect();
        assert_eq!(font_ids, [J::Num(0.0), J::Num(1.0)]);
        let text = run(&[os(&p)]);
        assert!(
            text.stdout.lines().last().unwrap().contains("at least 2"),
            "{}",
            text.stdout
        );
    }
}

#[test]
fn adr0065_i2_type1_source_reports_file_offsets() {
    let dir = Dir::new("type1-source-offset");
    let (pfa, pfb) = type1_fixture::sample();
    for (name, data) in [("triangle.pfa", pfa), ("triangle.pfb", pfb)] {
        let file = dir.file(name, &data);
        let checked = run(&[file.as_os_str()]);
        assert_eq!(checked.code, 0, "{}", checked.stdout);
    }
    let bad_hex = b"%!PS-AdobeFont-1.0\ncurrentfile eexec\n 000Z\ncleartomark\n";
    let short_section = b"\x80\x01\x08\x00\x00\x00ab";
    for (name, bytes, tag, at) in [
        (
            "bad.pfa",
            bad_hex.as_slice(),
            "PFA ",
            bad_hex.iter().position(|b| *b == b'Z').unwrap(),
        ),
        (
            "bad.pfb",
            short_section.as_slice(),
            "PFB ",
            short_section.len(),
        ),
    ] {
        let file = dir.file(name, bytes);
        let out = run(&["--json".as_ref(), file.as_os_str()]);
        assert_eq!(out.code, 1);
        assert!(
            out.stdout.contains(&format!("\"table\":\"{tag}\"")),
            "{}",
            out.stdout
        );
        assert!(
            out.stdout.contains(&format!("\"offset\":{at}")),
            "{}",
            out.stdout
        );
    }
}
