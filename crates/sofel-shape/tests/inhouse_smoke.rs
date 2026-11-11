//! A quick whole-pipeline smoke check against HarfBuzz's `in-house`
//! shaping tests (ignored; the conformance runner of `sofel-conformance`
//! is the real check). It shapes the lines whose options it understands
//! and prints the pass rate per file. Run with
//! `cargo test -p sofel-shape --test inhouse_smoke -- --ignored --nocapture`.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]

use std::path::Path;

use sofel_font::read::GlyphId;
use sofel_font::{Font, HbScale};
use sofel_shape::{
    BufferFlags, ClusterLevel, Direction, Feature, Language, Scale, SegmentProperties,
    SerializeFlags, SerializeFont, SerializeFormat, ShapePlan, UnicodeBuffer, serialize_glyphs,
    shape,
};

struct Names<'a>(&'a Font<'a>);

impl SerializeFont for Names<'_> {
    fn glyph_name(&self, glyph: u32) -> Option<&[u8]> {
        self.0.glyph_name(GlyphId(glyph))
    }
}

/// Shapes one test line; `None` when an option is not understood.
fn run(dir: &Path, line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split(';').collect();
    if parts.len() != 4 {
        return None;
    }
    let (font_path, opts, unicodes, expected) = (parts[0], parts[1], parts[2], parts[3]);
    if expected.trim() == "*" {
        return None;
    }
    let data = std::fs::read(dir.join(font_path)).ok()?;
    let mut flags = SerializeFlags::DEFAULT;
    let mut props = SegmentProperties::default();
    let mut features = Vec::new();
    let mut bflags = BufferFlags::DEFAULT;
    let mut level = ClusterLevel::MonotoneGraphemes;
    let mut size: Option<i32> = None;
    let mut ppem = 0u32;
    let mut face_index = 0u32;
    for o in opts.split_whitespace() {
        let (k, v) = o.split_once('=').unwrap_or((o, ""));
        let v = v.trim_matches('"');
        match k {
            "--no-glyph-names" => flags = flags.union(SerializeFlags::NO_GLYPH_NAMES),
            "--no-positions" => flags = flags.union(SerializeFlags::NO_POSITIONS),
            "--no-clusters" => flags = flags.union(SerializeFlags::NO_CLUSTERS),
            "--no-advances" => flags = flags.union(SerializeFlags::NO_ADVANCES),
            "--ned" => {
                flags = flags
                    .union(SerializeFlags::NO_CLUSTERS)
                    .union(SerializeFlags::NO_ADVANCES)
            }
            "--show-flags" => flags = flags.union(SerializeFlags::GLYPH_FLAGS),
            "--bot" => bflags = bflags.union(BufferFlags::BOT),
            "--eot" => bflags = bflags.union(BufferFlags::EOT),
            "--unsafe-to-concat" => bflags = bflags.union(BufferFlags::PRODUCE_UNSAFE_TO_CONCAT),
            "--preserve-default-ignorables" => {
                bflags = bflags.union(BufferFlags::PRESERVE_DEFAULT_IGNORABLES)
            }
            "--remove-default-ignorables" => {
                bflags = bflags.union(BufferFlags::REMOVE_DEFAULT_IGNORABLES)
            }
            "--direction" => props.direction = Direction::from_bytes(v.as_bytes()),
            "--script" => props.script = sofel_shape::Script::from_iso15924(v.as_bytes()),
            "--language" => props.language = Language::new(v.as_bytes()),
            "--features" => {
                for f in v.split(',') {
                    features.push(Feature::parse(f.as_bytes())?);
                }
            }
            "--cluster-level" => {
                level = match v {
                    "0" => ClusterLevel::MonotoneGraphemes,
                    "1" => ClusterLevel::MonotoneCharacters,
                    "2" => ClusterLevel::Characters,
                    "3" => ClusterLevel::Graphemes,
                    _ => return None,
                }
            }
            "--font-size" => size = Some(v.parse().ok()?),
            "--font-ppem" => ppem = v.parse().ok()?,
            "--face-index" => face_index = v.parse().ok()?,
            "--font-funcs" | "--single-par" => {}
            _ => return None,
        }
    }
    let font = Font::new(&data, face_index).ok()?;
    let cps: Vec<u32> = unicodes
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| u32::from_str_radix(s.trim().trim_start_matches("U+"), 16).unwrap())
        .collect();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_codepoints(&cps, 0, None);
    buffer.set_flags(bflags);
    buffer.set_cluster_level(level);
    let resolved = props.guess(&buffer, None);
    let plan = ShapePlan::new(font.default_instance(), resolved, &features);
    if std::env::var_os("SMOKE_DEFAULT_ONLY").is_some()
        && plan.shaper() != sofel_shape::Shaper::Default
    {
        return None;
    }
    let scale = match size {
        Some(s) => Scale::new(HbScale::for_font(&font, s, s), ppem, ppem),
        None => Scale::new(HbScale::unscaled(&font), ppem, ppem),
    };
    let glyphs = shape(font.default_instance(), &scale, &plan, buffer);
    let mut out = Vec::new();
    serialize_glyphs(
        glyphs.glyph_infos(),
        Some(glyphs.glyph_positions()),
        &Names(&font),
        SerializeFormat::Text,
        flags,
        &mut out,
    );
    if out.is_empty() {
        out.extend_from_slice(b"[]");
    }
    Some((
        String::from_utf8_lossy(&out).into_owned(),
        expected.trim().to_string(),
    ))
}

#[test]
#[ignore]
fn inhouse_smoke() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/testdata/harfbuzz/test/shape/data")
        .join(std::env::var("SMOKE_SUITE").unwrap_or_else(|_| "in-house".into()))
        .join("tests");
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    let (mut total, mut pass, mut skipped) = (0usize, 0usize, 0usize);
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let (mut t, mut p) = (0, 0);
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let r = std::panic::catch_unwind(|| run(f.parent().unwrap(), line));
            match r {
                Ok(Some((got, want))) => {
                    t += 1;
                    if got == want {
                        p += 1;
                    } else if std::env::var_os("SMOKE_VERBOSE").is_some() {
                        println!("FAIL {}: {line}\n  got  {got}", f.display());
                    }
                }
                Ok(None) => skipped += 1,
                Err(_) => {
                    t += 1;
                    println!("PANIC {}: {line}", f.display());
                }
            }
        }
        println!("{p}/{t} {}", f.file_name().unwrap().to_string_lossy());
        total += t;
        pass += p;
    }
    println!("total {pass}/{total} (skipped {skipped})");
}
