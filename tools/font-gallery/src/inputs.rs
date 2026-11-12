//! Fixed samples, face enumeration, and renderer/shape invocation.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::pixels::{self, Fixed26Dot6, Glyph, Image};

#[derive(Clone, Debug)]
pub enum SoloInput {
    Scalar(&'static str),
    Glyph(u32),
}
impl SoloInput {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Scalar(_) => "char",
            Self::Glyph(_) => "glyph",
        }
    }
    pub fn value(&self) -> String {
        match self {
            Self::Scalar(s) => (*s).into(),
            Self::Glyph(n) => n.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Sample {
    pub id: String,
    pub face: u32,
    pub path: PathBuf,
    pub ppem: u32,
    pub solo: SoloInput,
    pub text: &'static str,
    pub lang: &'static str,
    pub color: bool,
    pub unsupported_empty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Drawn,
    Empty,
    Grayscale,
}

#[derive(Clone, Debug)]
pub struct Rendered {
    pub solo: Option<Image>,
    pub glyphs: Vec<Glyph>,
    pub glyph_ids: Vec<u32>,
    pub status: Status,
}

fn choice(id: &str) -> Option<(SoloInput, &'static str, &'static str)> {
    Some(match id {
        "arabic" => (SoloInput::Scalar("U+0628"), "مرحبا", "ar"),
        "devanagari" => (SoloInput::Scalar("U+0915"), "नमस्ते", "hi"),
        "khmer" => (SoloInput::Scalar("U+1780"), "សួស្តី", "km"),
        "myanmar" => (SoloInput::Scalar("U+1000"), "မင်္ဂလာပါ", "my"),
        "balinese" => (SoloInput::Scalar("U+1B13"), "ᬩᬮᬶ", "ban"),
        "hebrew" => (SoloInput::Scalar("U+05E9"), "שלום", "he"),
        "source-serif" => (SoloInput::Scalar("U+0067"), "office & affix", "en"),
        "stix-math" => (SoloInput::Scalar("U+222B"), "∫₀¹ x² dx", "en"),
        "source-han" => (SoloInput::Scalar("U+9AA8"), "山水風月", "und"),
        "amstelvar" => (SoloInput::Scalar("U+0067"), "office & affix", "en"),
        "color-cff" | "color-sbix" | "color-svg" => (SoloInput::Glyph(19), "simple_linear", "en"),
        "hb-variable-colr" => (SoloInput::Glyph(92), "\u{f0502}\u{f0503}", "und"),
        _ => return None,
    })
}

pub fn cases(
    repo: &Path,
    manifest: &Path,
    font_dir: &Path,
    include_variable: bool,
) -> Result<Vec<Sample>, String> {
    let text = fs::read_to_string(manifest)
        .map_err(|_| format!("missing font manifest: {}", manifest.display()))?;
    let mut rows = Vec::new();
    for row in text
        .lines()
        .filter(|x| !x.is_empty() && !x.starts_with('#'))
    {
        let fields: Vec<_> = row.split('\t').collect();
        if fields.len() < 4 {
            return Err(format!("invalid font manifest row: {row}"));
        }
        rows.push((fields[0].to_string(), font_dir.join(fields[3])));
    }
    if rows.is_empty() {
        return Err("empty font manifest".into());
    }
    if include_variable {
        rows.push((
            "hb-variable-colr".into(),
            repo.join(
                "target/testdata/harfbuzz/test/api/fonts/test_glyphs-glyf_colr_1_variable.ttf",
            ),
        ));
    }
    let mut samples = Vec::new();
    for (id, path) in rows {
        let (solo, text, lang) = choice(&id).ok_or_else(|| format!("no sample for {id}"))?;
        let bytes = fs::read(&path).map_err(|_| format!("missing font: {}", path.display()))?;
        let faces = if bytes.starts_with(b"ttcf") {
            bytes
                .get(8..12)
                .map(|d| u32::from_be_bytes(d.try_into().unwrap()))
                .unwrap_or(0)
        } else {
            1
        };
        if !(1..=1024).contains(&faces) {
            return Err(format!("invalid face count: {}", path.display()));
        }
        for face in 0..faces {
            samples.push(Sample {
                id: id.clone(),
                face,
                path: path.clone(),
                ppem: if id == "color-sbix" { 109 } else { 48 },
                solo: solo.clone(),
                text,
                lang,
                color: id.starts_with("color-") || id == "hb-variable-colr",
                unsupported_empty: id == "color-cff",
            });
        }
    }
    Ok(samples)
}

fn run(bin: &Path, args: &[String]) -> Result<Output, String> {
    Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| e.to_string())
}
fn error_text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

fn check_empty(out: &Output, glyph: u32, sample: &Sample) -> Result<(), String> {
    let expected = format!("sofel-render: glyph {glyph} is empty\n");
    if !out.status.success() && out.stdout.is_empty() && out.stderr == expected.as_bytes() {
        Ok(())
    } else {
        Err(format!(
            "{}/{} expected {expected:?}, got {:?}",
            sample.id,
            sample.face,
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeRecord {
    pub g: u32,
    pub cl: usize,
    pub dx: Fixed26Dot6,
    pub dy: Fixed26Dot6,
    pub ax: Fixed26Dot6,
    pub ay: Fixed26Dot6,
}

struct JsonCursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> JsonCursor<'a> {
    fn peek(&mut self) -> Option<u8> {
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
        self.bytes.get(self.at).copied()
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        if self.peek() != Some(expected) {
            return Err("invalid shape JSON".into());
        }
        self.at += 1;
        Ok(())
    }

    fn key(&mut self) -> Result<&'a str, String> {
        self.expect(b'"')?;
        let start = self.at;
        while let Some(&byte) = self.bytes.get(self.at) {
            if byte == b'"' {
                let key = std::str::from_utf8(&self.bytes[start..self.at])
                    .map_err(|_| "invalid shape JSON")?;
                self.at += 1;
                return Ok(key);
            }
            if byte == b'\\' || byte < 0x20 {
                return Err("invalid shape JSON".into());
            }
            self.at += 1;
        }
        Err("invalid shape JSON".into())
    }

    fn integer(&mut self) -> Result<i32, String> {
        self.peek();
        let start = self.at;
        if self.bytes.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        let digits = self.at;
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        if self.at == digits {
            return Err("invalid shape JSON number".into());
        }
        std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| "invalid shape JSON number".to_string())?
            .parse()
            .map_err(|_| "invalid shape JSON number".into())
    }
}

pub fn shape_records(bytes: &[u8]) -> Result<Vec<ShapeRecord>, String> {
    std::str::from_utf8(bytes).map_err(|_| "invalid shape JSON UTF-8")?;
    let mut cursor = JsonCursor { bytes, at: 0 };
    cursor.expect(b'[')?;
    let mut out = Vec::new();
    if cursor.peek() != Some(b']') {
        loop {
            cursor.expect(b'{')?;
            let (mut g, mut cl, mut dx, mut dy, mut ax, mut ay) =
                (None, None, None, None, None, None);
            loop {
                let key = cursor.key()?;
                cursor.expect(b':')?;
                let value = cursor.integer()?;
                let field = match key {
                    "g" => &mut g,
                    "cl" => &mut cl,
                    "dx" => &mut dx,
                    "dy" => &mut dy,
                    "ax" => &mut ax,
                    "ay" => &mut ay,
                    _ => return Err("unexpected shape JSON field".into()),
                };
                if field.replace(value).is_some() {
                    return Err("duplicate shape JSON field".into());
                }
                match cursor.peek() {
                    Some(b',') => cursor.at += 1,
                    Some(b'}') => {
                        cursor.at += 1;
                        break;
                    }
                    _ => return Err("invalid shape JSON".into()),
                }
            }
            out.push(ShapeRecord {
                g: g.ok_or("missing g")?.try_into().map_err(|_| "invalid g")?,
                cl: cl
                    .ok_or("missing cl")?
                    .try_into()
                    .map_err(|_| "invalid cl")?,
                dx: Fixed26Dot6(dx.ok_or("missing dx")?),
                dy: Fixed26Dot6(dy.ok_or("missing dy")?),
                ax: Fixed26Dot6(ax.ok_or("missing ax")?),
                ay: Fixed26Dot6(ay.ok_or("missing ay")?),
            });
            match cursor.peek() {
                Some(b',') => cursor.at += 1,
                Some(b']') => break,
                _ => return Err("invalid shape JSON".into()),
            }
        }
    }
    cursor.expect(b']')?;
    if cursor.peek().is_some() {
        return Err("invalid shape JSON".into());
    }
    Ok(out)
}

pub fn render_case(
    sample: &Sample,
    render_bin: &Path,
    shape_bin: &Path,
) -> Result<Rendered, String> {
    let path = sample.path.to_string_lossy().to_string();
    let mut base = vec![
        "--ppem".into(),
        sample.ppem.to_string(),
        "--face".into(),
        sample.face.to_string(),
    ];
    if sample.color {
        base.push("--color".into());
    }
    let mut args = base.clone();
    args.extend([
        format!("--{}", sample.solo.kind()),
        sample.solo.value(),
        path.clone(),
    ]);
    let solo = run(render_bin, &args)?;
    if sample.unsupported_empty {
        check_empty(&solo, 19, sample)?;
    } else if !solo.status.success() {
        return Err(format!(
            "{}/{} solo: {}",
            sample.id,
            sample.face,
            error_text(&solo)
        ));
    }
    let shape_args = vec![
        "-O".into(),
        "json".into(),
        "--no-glyph-names".into(),
        format!("--language={}", sample.lang),
        format!("--face-index={}", sample.face),
        format!("--font-size={}", sample.ppem * 64),
        format!("--font-ppem={}", sample.ppem),
        path.clone(),
        sample.text.into(),
    ];
    let shaped = run(shape_bin, &shape_args)?;
    if !shaped.status.success() {
        return Err(format!(
            "{}/{} shape: {}",
            sample.id,
            sample.face,
            error_text(&shaped)
        ));
    }
    let records = shape_records(&shaped.stdout)
        .map_err(|e| format!("{}/{} shape JSON: {e}", sample.id, sample.face))?;
    if records.is_empty() {
        return Err(format!("{}/{} empty shape", sample.id, sample.face));
    }
    if records.iter().any(|r| r.g == 0) {
        return Err(format!("{}/{} missing glyph", sample.id, sample.face));
    }
    if sample.unsupported_empty && records.iter().map(|r| r.g).collect::<Vec<_>>() != [26] {
        return Err(format!(
            "{}/{} unexpected color glyphs",
            sample.id, sample.face
        ));
    }
    let chars: Vec<_> = sample.text.chars().collect();
    let mut glyphs = Vec::new();
    let mut glyph_ids = Vec::new();
    for record in records {
        glyph_ids.push(record.g);
        let mut args = base.clone();
        args.extend(["--glyph".into(), record.g.to_string(), path.clone()]);
        let output = run(render_bin, &args)?;
        let image = if sample.unsupported_empty {
            check_empty(&output, record.g, sample)?;
            None
        } else if output.status.success() {
            Some(
                pixels::parse(&output.stdout).map_err(|e| {
                    format!("{}/{} glyph {}: {e}", sample.id, sample.face, record.g)
                })?,
            )
        } else if chars.get(record.cl) == Some(&' ')
            && output.stderr == format!("sofel-render: glyph {} is empty\n", record.g).as_bytes()
        {
            None
        } else {
            return Err(format!(
                "{}/{} glyph {}: {}",
                sample.id,
                sample.face,
                record.g,
                error_text(&output)
            ));
        };
        glyphs.push(Glyph {
            image,
            dx: record.dx,
            dy: record.dy,
            ax: record.ax,
            ay: record.ay,
        });
    }
    let status = if sample.unsupported_empty {
        Status::Empty
    } else if sample.id == "hb-variable-colr" {
        if !solo.stdout.starts_with(b"P5\n")
            || glyphs
                .iter()
                .any(|g| g.image.as_ref().is_none_or(|im| !im.pgm))
        {
            return Err(
                "variable COLR render mode changed; review its registered limitation".into(),
            );
        }
        Status::Grayscale
    } else {
        Status::Drawn
    };
    Ok(Rendered {
        solo: if sample.unsupported_empty {
            None
        } else {
            Some(pixels::parse(&solo.stdout)?)
        },
        glyphs,
        glyph_ids,
        status,
    })
}
