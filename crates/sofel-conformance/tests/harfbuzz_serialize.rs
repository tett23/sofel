//! sofel's buffer serialization against HarfBuzz (ADR 0020, 直列化).
//!
//! `hb-dump --serialize FONT conformance/serialize-text.txt` shapes each
//! line with HarfBuzz and writes the characters, the shaped glyphs (raw:
//! glyph, cluster, mask, advances, offsets) and their serialization in
//! text and JSON for every combination of flags, each followed by
//! HarfBuzz's reading of that string; then its reading of the inputs of
//! `test/api/test-buffer.c` and of every prefix of them. This test reads
//! the raw results into `GlyphInfo` and `GlyphPosition` (so it does not
//! depend on sofel's shaper), writes them with sofel's functions and
//! compares byte for byte, and reads every string with sofel's readers
//! and compares the result, the end and the items.
//!
//! `hb-dump --deserialize` then reads a corpus this test builds: every
//! prefix of some of the strings, the strings with a byte deleted or a
//! delimiter inserted at every position, and hand-written cases, so that
//! the hand-written parsers meet the Ragel machines on input they reject.
//!
//! The fonts: glyf with `post` names (and COLR), glyf without names, CFF
//! with charset names, and glyf fonts with mark positioning. The items
//! are `<font>:<kind>`, the writing with GLYPH_EXTENTS being a kind of
//! its own (`serialize-glyphs-extents`); items expected to fail are
//! listed in conformance/expected-failures-serialize.txt.

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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use sofel_conformance::{expect, paths};
use sofel_font::read::GlyphId;
use sofel_font::{Font, GlyphExtents, GlyphNames};
use sofel_outline::HbExtents;
use sofel_shape::{
    GlyphFlags, GlyphInfo, GlyphPosition, SerializeFlags, SerializeFont, SerializeFormat,
    deserialize_glyphs, deserialize_unicode, serialize_glyphs, serialize_unicode,
};

const FONTS: [&str; 5] = [
    "harfbuzz/test/api/fonts/adwaita.ttf",
    "harfbuzz/test/api/fonts/Roboto-Regular.ac.ttf",
    "harfbuzz/test/api/fonts/SourceSansPro-Regular.otf",
    "harfbuzz/test/api/fonts/OpenSans-Regular.ttf",
    "harfbuzz/test/api/fonts/NotoNastaliqUrdu-Regular.ttf",
];

const LIST: &str = "expected-failures-serialize.txt";

/// Inputs HarfBuzz's tests do not have: spaces, brackets, escapes,
/// numbers at their limits, mixed content.
const EXTRA_CORPUS: &[&[u8]] = &[
    b"[{\"g\":1}, {\"g\":2}]",
    b"[{\"g\":1} ,  {\"g\":2} ]  ",
    b" [ { \"g\" : 1 , \"cl\" : 3 }",
    b"{\"g\":1}]{\"g\":2},",
    b"[{\"g\":\"gid5\",\"xb\":-3,\"yb\":0,\"w\":01,\"h\":-0}]",
    b"[{\"g\":\"a\\\"b\\\\c\"}]",
    b"[{\"g\":\"a\\x\"}]",
    b"[{\"g\":\"\"}]",
    b"[{\"g\":4294967295,\"cl\":4294967295,\"dx\":-2147483648,\"dy\":2147483647,\"fl\":4294967295}]",
    b"[{\"g\":4294967296}]",
    b"[{\"dx\":-2147483649}]",
    b"[{\"fl\":-1}]",
    b"[{\"g\":-1}]",
    b"[{\"u\":1,\"g\":2}]",
    b"[{\"g\":2,\"u\":1}]",
    b"[{\"cl\":1}]",
    b"[{}]",
    b"[{\"gg\":1}]",
    b"[{\"c\":1}]",
    b"[{\"g\"1}]",
    b"[[{\"g\":1}]",
    b"[\t{\"g\":1}\n]\x0b\x0c\r",
    b"[1=0|x",
    b"[[a|]",
    b"[|]",
    b"[=0|]",
    b"[]",
    b"]",
    b"[",
    b"[a]b]c|d",
    b"[1,2]",
    b"[1|,]",
    b"[gid1=0@1,2+3,4#7<1,2,3,4>|uni0041]",
    b"[gid1<1,2,3,4>|gid2=1<-1,-2,-3,-4>]",
    b"[gid1=0<1,2,3>]",
    b"[gid1=0+1,2,3]",
    b"[gid1=0@1+2]",
    b"[gid1=0#01]",
    b"[gid1=0#4294967296]",
    b"[gid1=-1]",
    b"[gid1@-0,-0+-0,-0]",
    b"[gid1@-2147483648,2147483647+-2147483649]",
    b"[a\\|b|c]",
    b"[a\\#b]",
    b"[a\\]b]",
    b"[\\\\]",
    b"[ 12|+5]",
    b"[-1|gid-1|gid+1|uni41|uni0x41|uni|gid]",
    b"[\xc3\xa9|\xff]",
    b"<U+41|x",
    b"<U+41|U+42",
    b"<U+41|U+42|",
    b"<U+41=01|",
    b"<u41>U+42>",
    b"<U+>",
    b"<U+0x41>",
    b"<U+FFFFFFFF=4294967295|U+100000000>",
    b"<U+41=0|U+42=1",
    b"U+41|U+42",
    b"<<U+41>",
    b"<U++41>",
    b"[{\"u\":65,\"cl\":0,\"dx\":1,\"fl\":3}]",
];

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

/// hb-dump's escaping, without the `=`.
fn escape(s: &[u8]) -> String {
    s.iter()
        .map(|&c| {
            if c > 0x20 && c < 0x7F && c != b'\\' {
                char::from(c).to_string()
            } else {
                format!("\\x{c:02x}")
            }
        })
        .collect()
}

/// The font as serialization asks HarfBuzz's `hb_font_t` at the default
/// scale: names (`post`, then CFF), boxes, glyphs by name and cmap.
struct SofelFont<'a, 'b> {
    font: Font<'a>,
    names: Option<GlyphNames<'a, 'b>>,
    extents: HbExtents<'a>,
}

impl SerializeFont for SofelFont<'_, '_> {
    fn glyph_name(&self, glyph: u32) -> Option<&[u8]> {
        self.names.as_ref()?.glyph_name(GlyphId(glyph))
    }
    fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        self.extents.glyph_extents(GlyphId(glyph))
    }
    fn glyph_from_name(&self, name: &[u8]) -> Option<u32> {
        self.names.as_ref()?.glyph_from_name(name).map(|g| g.0)
    }
    fn nominal_glyph(&self, c: u32) -> Option<u32> {
        self.font.nominal_glyph(c).map(|g| g.0)
    }
}

fn hb_dump(mode: &str, font: &Path, file: &Path) -> String {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(exe)
        .arg(mode)
        .arg(font)
        .arg(file)
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "hb-dump {mode} {} failed",
        font.display()
    );
    String::from_utf8(out.stdout).unwrap()
}

fn format_of(s: &str) -> SerializeFormat {
    match s {
        "text" => SerializeFormat::Text,
        "json" => SerializeFormat::Json,
        _ => panic!("unknown format {s}"),
    }
}

/// `<ok> <end> <len> <items>` as hb-dump writes a reading.
fn reading(r: Result<(), sofel_shape::DeserializeError>, len: usize, items: Vec<String>) -> String {
    let (ok, end) = match r {
        Ok(()) => (1, len),
        Err(e) => (0, e.consumed),
    };
    let mut s = format!("{ok} {end} {}", items.len());
    for i in items {
        s.push(' ');
        s.push_str(&i);
    }
    s
}

fn read_glyphs(text: &[u8], format: SerializeFormat, font: &dyn SerializeFont) -> String {
    let (mut infos, mut positions) = (Vec::new(), Vec::new());
    let r = deserialize_glyphs(text, format, font, &mut infos, &mut positions);
    let items = infos
        .iter()
        .zip(&positions)
        .map(|(i, p)| {
            format!(
                "{},{},{},{},{},{},{}",
                i.glyph, i.cluster, i.flags.0, p.x_advance, p.y_advance, p.x_offset, p.y_offset
            )
        })
        .collect();
    reading(r, text.len(), items)
}

fn read_unicode(text: &[u8], format: SerializeFormat) -> String {
    let (mut cps, mut clusters) = (Vec::new(), Vec::new());
    let r = deserialize_unicode(text, format, &mut cps, &mut clusters);
    let items = cps
        .iter()
        .zip(&clusters)
        .map(|(c, cl)| format!("{c},{cl}"))
        .collect();
    reading(r, text.len(), items)
}

/// The comparisons of one hb-dump output: (item kind, ok) for each line
/// compared, and the details of the differences.
#[derive(Default)]
struct Results {
    counts: BTreeMap<String, (usize, usize)>,
    details: Vec<String>,
    /// The strings HarfBuzz wrote, for the corpus.
    written: Vec<Vec<u8>>,
}

impl Results {
    fn record(&mut self, kind: &str, ok: bool, detail: impl FnOnce() -> String) {
        let c = self.counts.entry(kind.to_string()).or_default();
        c.0 += 1;
        if !ok {
            c.1 += 1;
            if self.details.len() < 40 {
                self.details.push(format!("{kind}: {}", detail()));
            }
        }
    }
}

fn compare(text: &str, font: &dyn SerializeFont, results: &mut Results) {
    let mut cps: Vec<u32> = Vec::new();
    let mut clusters: Vec<u32> = Vec::new();
    let mut infos: Vec<GlyphInfo> = Vec::new();
    let mut positions: Vec<GlyphPosition> = Vec::new();
    let mut last = Vec::new();
    let mut line = String::new();
    for l in text.lines() {
        let f: Vec<&str> = l.split(' ').collect();
        match f[0] {
            "line" => line = l.to_string(),
            "unicode" => {
                (cps, clusters) = f[1..]
                    .iter()
                    .map(|x| {
                        let (a, b) = x.split_once(',').unwrap();
                        (a.parse::<u32>().unwrap(), b.parse::<u32>().unwrap())
                    })
                    .unzip();
            }
            "glyphs" => {
                (infos, positions) = f[1..]
                    .iter()
                    .map(|x| {
                        let v: Vec<i64> = x.split(',').map(|n| n.parse().unwrap()).collect();
                        (
                            GlyphInfo {
                                glyph: v[0] as u32,
                                cluster: v[1] as u32,
                                flags: GlyphFlags(v[2] as u32),
                            },
                            GlyphPosition {
                                x_advance: v[3] as i32,
                                y_advance: v[4] as i32,
                                x_offset: v[5] as i32,
                                y_offset: v[6] as i32,
                            },
                        )
                    })
                    .unzip();
            }
            "serialize" => {
                let format = format_of(f[2]);
                let flags = SerializeFlags(f[3].parse().unwrap());
                let hb = unescape(f[4]);
                let mut ours = Vec::new();
                if f[1] == "unicode" {
                    serialize_unicode(&cps, &clusters, format, flags, &mut ours);
                } else {
                    serialize_glyphs(&infos, Some(&positions), font, format, flags, &mut ours);
                }
                // The boxes apart, so that a font whose boxes are
                // expected to differ is still checked without them.
                let kind = if f[1] == "glyphs" && flags.contains(SerializeFlags::GLYPH_EXTENTS) {
                    "serialize-glyphs-extents".to_string()
                } else {
                    format!("serialize-{}", f[1])
                };
                results.record(&kind, ours == hb, || {
                    format!(
                        "{line}: {} {}: HarfBuzz {:?}, sofel {:?}",
                        f[2],
                        f[3],
                        String::from_utf8_lossy(&hb),
                        String::from_utf8_lossy(&ours)
                    )
                });
                results.written.push(hb.clone());
                last = hb;
            }
            "corpus" => last = unescape(f[1]),
            "deserialize" => {
                let format = format_of(f[2]);
                let hb = f[3..].join(" ");
                let ours = if f[1] == "unicode" {
                    read_unicode(&last, format)
                } else {
                    read_glyphs(&last, format, font)
                };
                results.record(&format!("deserialize-{}", f[1]), ours == hb, || {
                    format!(
                        "{:?} as {}: HarfBuzz {hb}, sofel {ours}",
                        String::from_utf8_lossy(&last),
                        f[2]
                    )
                });
            }
            _ => panic!("unknown line {l}"),
        }
    }
}

/// Every prefix of `s`; and with each byte deleted and each delimiter
/// inserted at every position when `mutate`.
fn variants(s: &[u8], mutate: bool, out: &mut Vec<Vec<u8>>) {
    for n in 0..s.len() {
        out.push(s[..n].to_vec());
    }
    if !mutate {
        return;
    }
    for i in 0..s.len() {
        let mut d = s.to_vec();
        d.remove(i);
        out.push(d);
        for c in b" ,|]0-\"\\{x#<=" {
            let mut d = s.to_vec();
            d.insert(i, *c);
            out.push(d);
        }
    }
}

fn temp_file(name: &str, contents: &[u8]) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-{name}-{}", std::process::id()));
    std::fs::write(&p, contents).unwrap();
    p
}

#[test]
fn serialization_agrees_with_harfbuzz() {
    let text_file = paths::conformance_file("serialize-text.txt");
    let expected = expect::parse(&std::fs::read_to_string(paths::conformance_file(LIST)).unwrap());
    let mut all = Vec::new();
    let mut details = Vec::new();
    let mut corpus_done = false;
    for rel in FONTS {
        let path = paths::testdata().join(rel);
        paths::require(&path, "the HarfBuzz test fonts").unwrap_or_else(|e| panic!("{e}"));
        let bytes = std::fs::read(&path).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let mut buf = vec![0u32; font.glyph_names_len()];
        let sofel = SofelFont {
            font,
            names: font.glyph_names(&mut buf),
            extents: HbExtents::new(&font),
        };
        let mut results = Results::default();
        compare(
            &hb_dump("--serialize", &path, &text_file),
            &sofel,
            &mut results,
        );

        // The corpus, on the first font (glyph names from post) and on
        // the CFF one.
        if !corpus_done || rel.ends_with(".otf") {
            let mut corpus: Vec<Vec<u8>> = EXTRA_CORPUS.iter().map(|s| s.to_vec()).collect();
            let written = std::mem::take(&mut results.written);
            // Per line: 64 flags x 2 formats, unicode then glyphs.
            for (k, s) in written.iter().enumerate() {
                let line = k / 256;
                let flags = k % 64;
                if [0, 0x18, 0x21, 0x3F].contains(&flags) {
                    variants(s, line < 3 && flags != 0x21, &mut corpus);
                }
            }
            corpus.sort();
            corpus.dedup();
            let lines: String = corpus.iter().map(|s| escape(s) + "\n").collect();
            let file = temp_file("corpus.txt", lines.as_bytes());
            let out = hb_dump("--deserialize", &path, &file);
            std::fs::remove_file(&file).ok();
            let mut r = Results::default();
            compare(&out, &sofel, &mut r);
            eprintln!("{rel}: corpus of {} strings", corpus.len());
            for (kind, c) in r.counts {
                let e = results.counts.entry(format!("corpus-{kind}")).or_default();
                e.0 += c.0;
                e.1 += c.1;
            }
            details.extend(r.details);
            corpus_done = true;
        }
        for (kind, (n, failed)) in &results.counts {
            eprintln!("  {rel}:{kind}: {n} compared, {failed} differ");
            all.push((format!("{rel}:{kind}"), *failed == 0));
        }
        details.extend(results.details);
    }
    let problems = expect::judge(all.iter().map(|(id, ok)| (id.as_str(), *ok)), &expected);
    assert!(
        problems.is_empty(),
        "{}\n{}",
        expect::report(&problems, LIST),
        details.join("\n")
    );
}
