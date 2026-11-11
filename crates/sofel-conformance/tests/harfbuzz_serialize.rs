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
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

/// Write each byte once without an escaped copy of the entire corpus.
fn write_corpus<B: AsRef<[u8]>>(
    writer: &mut impl Write,
    corpus: impl IntoIterator<Item = B>,
) -> io::Result<()> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for s in corpus {
        let s = s.as_ref();
        let mut start = 0;
        for (i, &c) in s.iter().enumerate() {
            if !(c > 0x20 && c < 0x7f && c != b'\\') {
                writer.write_all(&s[start..i])?;
                writer.write_all(&[b'\\', b'x', HEX[(c >> 4) as usize], HEX[(c & 15) as usize]])?;
                start = i + 1;
            }
        }
        writer.write_all(&s[start..])?;
        writer.write_all(b"\n")?;
    }
    writer.flush()
}

/// The font as serialization asks HarfBuzz's `hb_font_t` at the default
/// scale: names (`post`, then CFF), boxes, glyphs by name and cmap.
struct SofelFont<'a, 'b> {
    font: Font<'a>,
    names: Option<GlyphNames<'a, 'b>>,
    extents: HbExtents<'a, 'static, 'static>,
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

fn hb_dump(mode: &str, font: &Path, file: &Path, sofel: &dyn SerializeFont, results: &mut Results) {
    let exe = paths::hb_dump();
    paths::require(&exe, "hb-dump").unwrap_or_else(|e| panic!("{e}"));
    let mut command = Command::new(exe);
    command.arg(mode).arg(font).arg(file).env("LC_ALL", "C");
    compare_command(command, sofel, results).unwrap();
}

fn compare_command(
    mut command: Command,
    font: &dyn SerializeFont,
    results: &mut Results,
) -> io::Result<()> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let output = child.stdout.take().unwrap();
    let compared = compare_reader(BufReader::new(output), font, results);
    if compared.is_err() {
        // Closing the pipe prevents a blocked writer; reap it on errors too.
        let _ = child.kill();
    }
    let status = child.wait()?;
    compared?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "reference process failed: {status}"
        )));
    }
    Ok(())
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
    corpus_count: usize,
    metadata: [usize; 3],
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

fn compare_reader(
    mut reader: impl BufRead,
    font: &dyn SerializeFont,
    results: &mut Results,
) -> io::Result<()> {
    let mut cps: Vec<u32> = Vec::new();
    let mut clusters: Vec<u32> = Vec::new();
    let mut infos: Vec<GlyphInfo> = Vec::new();
    let mut positions: Vec<GlyphPosition> = Vec::new();
    let mut last = Vec::new();
    let mut line = String::new();
    let mut buffer = String::new();
    let mut pending: Option<(bool, bool)> = None;
    let mut corpus_next: Option<usize> = None;
    let order = [(false, false), (false, true), (true, false), (true, true)];
    loop {
        buffer.clear();
        if reader.read_line(&mut buffer)? == 0 {
            break;
        }
        if buffer.ends_with('\n') {
            buffer.pop();
            if buffer.ends_with('\r') {
                buffer.pop();
            }
        }
        let l = buffer.as_str();
        let f: Vec<&str> = l.split(' ').collect();
        if let Some(expected) = pending {
            if f.len() < 4
                || f[0] != "deserialize"
                || (f[1] == "unicode", f[2] == "json") != expected
            {
                return Err(io::Error::other("missing or out-of-order deserialize line"));
            }
        } else if f[0] == "deserialize" {
            return Err(io::Error::other("extra deserialize line"));
        }
        match f[0] {
            "line" => {
                results.metadata[0] += 1;
                line = l.to_string();
            }
            "unicode" => {
                results.metadata[1] += 1;
                (cps, clusters) = f[1..]
                    .iter()
                    .map(|x| {
                        let (a, b) = x.split_once(',').unwrap();
                        (a.parse::<u32>().unwrap(), b.parse::<u32>().unwrap())
                    })
                    .unzip();
            }
            "glyphs" => {
                results.metadata[2] += 1;
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
                pending = Some((f[1] == "unicode", f[2] == "json"));
            }
            "corpus" => {
                last = unescape(f[1]);
                results.corpus_count += 1;
                corpus_next = Some(0);
                pending = Some(order[0]);
            }
            "deserialize" => {
                let format = format_of(f[2]);
                let hb = l.splitn(4, ' ').nth(3).unwrap();
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
                if let Some(i) = corpus_next {
                    corpus_next = (i + 1 < order.len()).then_some(i + 1);
                    pending = corpus_next.map(|next| order[next]);
                } else {
                    pending = None;
                }
            }
            _ => return Err(io::Error::other(format!("unknown line {l}"))),
        }
    }
    if pending.is_some() {
        return Err(io::Error::other("truncated reference output"));
    }
    Ok(())
}

fn check_counts(results: &Results, text_lines: usize, corpus_entries: usize) -> io::Result<()> {
    if results.metadata != [text_lines; 3] {
        return Err(io::Error::other(
            "missing or extra line/unicode/glyph metadata",
        ));
    }
    for (kind, count) in [
        ("serialize-glyphs", text_lines * 64),
        ("serialize-glyphs-extents", text_lines * 64),
        ("serialize-unicode", text_lines * 128),
        ("deserialize-glyphs", text_lines * 128 + corpus_entries * 2),
        ("deserialize-unicode", text_lines * 128 + corpus_entries * 2),
    ] {
        if results.counts.get(kind).map_or(0, |c| c.0) != count {
            return Err(io::Error::other(format!(
                "missing or extra {kind} comparisons"
            )));
        }
    }
    if results.corpus_count != corpus_entries {
        return Err(io::Error::other("missing or extra corpus entries"));
    }
    Ok(())
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
        let mut d = Vec::with_capacity(s.len() - 1);
        d.extend_from_slice(&s[..i]);
        d.extend_from_slice(&s[i + 1..]);
        out.push(d);
        for c in b" ,|]0-\"\\{x#<=" {
            let mut d = Vec::with_capacity(s.len() + 1);
            d.extend_from_slice(&s[..i]);
            d.push(*c);
            d.extend_from_slice(&s[i..]);
            out.push(d);
        }
    }
}

const DELIMITERS: &[u8] = b" ,|]0-\"\\{x#<=";

fn variant_layout(n: usize, mutate: bool) -> Option<(usize, usize)> {
    let prefixes = n.checked_mul(n.saturating_sub(1))?.checked_div(2)?;
    if !mutate {
        return Some((n, prefixes));
    }
    let count = n.checked_add(n.checked_mul(DELIMITERS.len() + 1)?)?;
    let removed = n.checked_mul(n.saturating_sub(1))?;
    let inserted = n
        .checked_mul(n.checked_add(1)?)?
        .checked_mul(DELIMITERS.len())?;
    Some((count, prefixes.checked_add(removed)?.checked_add(inserted)?))
}

/// Bytes are written once; only ranges move during sorting and deduplication.
struct Corpus {
    bytes: Vec<u8>,
    ranges: Vec<std::ops::Range<usize>>,
}

impl Corpus {
    fn new(extra: &[&[u8]], seeds: &[(&[u8], bool)]) -> Self {
        let (count, size) = extra
            .iter()
            .map(|s| Some((1usize, s.len())))
            .chain(
                seeds
                    .iter()
                    .map(|(s, mutate)| variant_layout(s.len(), *mutate)),
            )
            .try_fold((0usize, 0usize), |(count, size), item| {
                let (n, bytes) = item?;
                Some((count.checked_add(n)?, size.checked_add(bytes)?))
            })
            .expect("corpus size overflow");
        let mut corpus = Self {
            bytes: Vec::with_capacity(size),
            ranges: Vec::with_capacity(count),
        };
        for s in extra {
            corpus.push(&[s]);
        }
        for &(s, mutate) in seeds {
            for n in 0..s.len() {
                corpus.push(&[&s[..n]]);
            }
            if mutate {
                for i in 0..s.len() {
                    corpus.push(&[&s[..i], &s[i + 1..]]);
                    for &c in DELIMITERS {
                        corpus.push(&[&s[..i], &[c], &s[i..]]);
                    }
                }
            }
        }
        debug_assert_eq!(corpus.bytes.len(), size);
        debug_assert_eq!(corpus.ranges.len(), count);
        corpus
    }

    fn push(&mut self, parts: &[&[u8]]) {
        let start = self.bytes.len();
        for part in parts {
            self.bytes.extend_from_slice(part);
        }
        self.ranges.push(start..self.bytes.len());
    }

    fn sort_dedup(&mut self) {
        let bytes = &self.bytes;
        self.ranges
            .sort_unstable_by(|a, b| bytes[a.clone()].cmp(&bytes[b.clone()]));
        self.ranges
            .dedup_by(|a, b| bytes[a.clone()] == bytes[b.clone()]);
    }

    fn iter(&self) -> impl Iterator<Item = &[u8]> {
        self.ranges.iter().map(|r| &self.bytes[r.clone()])
    }
}

fn temp_file(name: &str, corpus: Corpus) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-{name}-{}", std::process::id()));
    write_corpus(
        &mut BufWriter::new(std::fs::File::create(&p).unwrap()),
        corpus.iter(),
    )
    .unwrap();
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
            extents: HbExtents::new(font.default_instance()),
        };
        let mut results = Results::default();
        hb_dump("--serialize", &path, &text_file, &sofel, &mut results);
        // All prefixes of the 24 fixed test_buffer_inputs in hb-dump.cc.
        check_counts(
            &results,
            std::fs::read_to_string(&text_file).unwrap().lines().count(),
            529,
        )
        .unwrap();

        // The corpus, on the first font (glyph names from post) and on
        // the CFF one.
        if !corpus_done || rel.ends_with(".otf") {
            let written = std::mem::take(&mut results.written);
            // Per line: 64 flags x 2 formats, unicode then glyphs.
            let seeds: Vec<_> = written
                .iter()
                .enumerate()
                .filter_map(|(k, s)| {
                    let line = k / 256;
                    let flags = k % 64;
                    [0, 0x18, 0x21, 0x3F]
                        .contains(&flags)
                        .then_some((s.as_slice(), line < 3 && flags != 0x21))
                })
                .collect();
            let mut corpus = Corpus::new(EXTRA_CORPUS, &seeds);
            corpus.sort_dedup();
            let count = corpus.ranges.len();
            let file = temp_file("corpus.txt", corpus);
            let mut r = Results::default();
            hb_dump("--deserialize", &path, &file, &sofel, &mut r);
            std::fs::remove_file(&file).ok();
            check_counts(&r, 0, count).unwrap();
            eprintln!("{rel}: corpus of {count} strings");
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

#[test]
fn adr0035_i1_i2_streams_before_reading_the_rest() {
    use std::io::{BufReader, Cursor, Read};
    struct Fails;
    impl Read for Fails {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected read error"))
        }
    }
    let input = b"corpus =[]\ndeserialize glyphs text 1 2 0\n";
    let mut results = Results::default();
    let error = compare_reader(
        BufReader::with_capacity(1, Cursor::new(input).chain(Fails)),
        &sofel_shape::EmptyFont,
        &mut results,
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected read error"));
    // The deliberately different reading must already be recorded.
    assert_eq!(results.counts["deserialize-glyphs"], (1, 1));
    let mut empty = Results::default();
    compare_reader(Cursor::new(b""), &sofel_shape::EmptyFont, &mut empty).unwrap();
    assert!(empty.counts.is_empty());
}

#[test]
fn adr0035_i3_rejects_missing_extra_and_failed_reference_output() {
    use std::io::Cursor;
    let mut results = Results::default();
    assert!(
        compare_reader(
            Cursor::new(b"corpus =[]\n"),
            &sofel_shape::EmptyFont,
            &mut results
        )
        .is_err()
    );
    assert!(
        compare_reader(
            Cursor::new(b"deserialize glyphs text 1 2 0\n"),
            &sofel_shape::EmptyFont,
            &mut Results::default()
        )
        .is_err()
    );
    let mut command = Command::new("sh");
    command.args(["-c", "exit 7"]);
    assert!(compare_command(command, &sofel_shape::EmptyFont, &mut Results::default()).is_err());
    assert!(check_counts(&Results::default(), 0, 1).is_err());
}

#[test]
fn adr0035_i4_corpus_bytes_preserve_empty_and_non_utf8_inputs() {
    let corpus = vec![Vec::new(), b"a b\n\\".to_vec(), vec![0, 0xff, 0x7f]];
    let mut bytes = Vec::new();
    write_corpus(&mut bytes, corpus).unwrap();
    assert_eq!(bytes, b"\na\\x20b\\x0a\\x5c\n\\x00\\xff\\x7f\n");
    let mut empty = Vec::new();
    write_corpus(&mut empty, Vec::<Vec<u8>>::new()).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn adr0035_i6_variants_keep_bytes_and_order_with_exact_capacity() {
    fn previous(s: &[u8], mutate: bool) -> Vec<Vec<u8>> {
        let mut out: Vec<_> = (0..s.len()).map(|n| s[..n].to_vec()).collect();
        if mutate {
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
        out
    }
    for input in [&b""[..], &b"x"[..], &b"a b"[..], &[0, 0xff, b'\\'][..]] {
        for mutate in [false, true] {
            let mut actual = Vec::new();
            variants(input, mutate, &mut actual);
            assert_eq!(actual, previous(input, mutate));
            assert!(actual.iter().all(|v| v.capacity() == v.len()));
        }
    }
}

#[test]
fn adr0035_i3_rejects_extra_metadata_and_complete_blocks() {
    use std::io::Cursor;
    for text in ["line 0 =x\n", "unicode\n", "glyphs\n"] {
        let mut results = Results::default();
        compare_reader(Cursor::new(text), &sofel_shape::EmptyFont, &mut results).unwrap();
        assert!(check_counts(&results, 0, 0).is_err());
    }
    let text = "corpus =[]\ndeserialize glyphs text 0 0 0\ndeserialize glyphs json 0 0 0\ndeserialize unicode text 0 0 0\ndeserialize unicode json 0 0 0\n";
    let mut results = Results::default();
    compare_reader(Cursor::new(text), &sofel_shape::EmptyFont, &mut results).unwrap();
    assert!(check_counts(&results, 0, 0).is_err());
    assert!(check_counts(&results, 0, 1).is_ok());
    assert!(check_counts(&results, 0, 2).is_err());
}

#[test]
fn adr0035_i7_arena_keeps_exact_bytes_order_and_checked_layout() {
    for input in [&b""[..], &b"x"[..], &b"a b"[..], &[0, 0xff, b'\\'][..]] {
        for mutate in [false, true] {
            let mut expected = vec![b"extra".to_vec()];
            variants(input, mutate, &mut expected);
            let mut corpus = Corpus::new(&[b"extra"], &[(input, mutate)]);
            assert_eq!(
                corpus.iter().map(<[u8]>::to_vec).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(corpus.bytes.len(), corpus.bytes.capacity());
            assert_eq!(corpus.ranges.len(), corpus.ranges.capacity());
            corpus.sort_dedup();
            expected.sort();
            expected.dedup();
            assert_eq!(
                corpus.iter().map(<[u8]>::to_vec).collect::<Vec<_>>(),
                expected
            );
        }
    }
    assert_eq!(variant_layout(0, true), Some((0, 0)));
    assert_eq!(variant_layout(usize::MAX, false), None);
    assert_eq!(variant_layout(usize::MAX, true), None);
}

#[test]
fn adr0035_i1_reader_chunk_boundaries_keep_all_results() {
    use std::io::{BufReader, Cursor};
    let text = b"line 0 =a\nunicode 65,0\nserialize unicode text 0 =<U+0041=0>\ndeserialize unicode text 1 10 1 65,0\nglyphs 1,0,0,0,0,0,0\nserialize glyphs text 0 =[a=0+0]\ndeserialize glyphs text 1 7 1 1,0,0,0,0,0,0\n";
    let mut expected = Results::default();
    compare_reader(Cursor::new(text), &sofel_shape::EmptyFont, &mut expected).unwrap();
    assert!(!expected.counts.is_empty());
    // EmptyFont deliberately differs from the reference's glyph names.
    assert!(!expected.details.is_empty());
    assert!(!expected.written.is_empty());
    for capacity in [1, 2, 7, 8192] {
        let mut actual = Results::default();
        compare_reader(
            BufReader::with_capacity(capacity, Cursor::new(text)),
            &sofel_shape::EmptyFont,
            &mut actual,
        )
        .unwrap();
        assert_eq!(actual.counts, expected.counts);
        assert_eq!(actual.details, expected.details);
        assert_eq!(actual.written, expected.written);
        assert_eq!(actual.corpus_count, expected.corpus_count);
        assert_eq!(actual.metadata, expected.metadata);
    }
}
