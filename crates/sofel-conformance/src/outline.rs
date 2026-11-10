//! Outlines in the text form of `ft-dump --outline` (ADR 0011), for
//! comparing sofel-outline with FreeType glyph by glyph.

use std::collections::BTreeMap;

use sofel_font::Font;
use sofel_font::read::{GlyphId, Tag};
use sofel_geom::{Command, FnPen};
use sofel_outline::{Outlines, Scale, Scratch};

/// The lines of one glyph, with error codes dropped (`error`).
pub type GlyphLines = Vec<String>;

/// A face: its glyph count and the lines of each glyph.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FaceOutlines {
    pub error: bool,
    pub num_glyphs: u32,
    pub glyphs: BTreeMap<u32, GlyphLines>,
}

fn normalize(line: &str) -> String {
    if line.starts_with("error") {
        "error".to_string()
    } else {
        line.to_string()
    }
}

/// Parses `ft-dump --outline` output into faces by index.
pub fn parse(text: &str) -> Result<BTreeMap<u32, FaceOutlines>, String> {
    let mut faces = BTreeMap::new();
    let mut face: Option<(u32, FaceOutlines)> = None;
    let mut glyph: Option<u32> = None;
    for line in text.lines() {
        let mut words = line.split(' ');
        let key = words.next().unwrap_or("");
        let num = |w: Option<&str>| {
            w.and_then(|v| v.parse::<u32>().ok())
                .ok_or_else(|| format!("bad line: {line}"))
        };
        match key {
            "face" => {
                if let Some((i, f)) = face.take() {
                    faces.insert(i, f);
                }
                face = Some((num(words.next())?, FaceOutlines::default()));
                glyph = None;
            }
            "end" => {
                if let Some((i, f)) = face.take() {
                    faces.insert(i, f);
                }
            }
            _ => {
                let (_, f) = face
                    .as_mut()
                    .ok_or_else(|| format!("line before any face: {line}"))?;
                match (key, glyph) {
                    ("num_glyphs", _) => f.num_glyphs = num(words.next())?,
                    ("glyph", _) => {
                        let g = num(words.next())?;
                        f.glyphs.insert(g, Vec::new());
                        glyph = Some(g);
                    }
                    ("error", None) => f.error = true,
                    (_, Some(g)) => f.glyphs.entry(g).or_default().push(normalize(line)),
                    _ => return Err(format!("unexpected line: {line}")),
                }
            }
        }
    }
    Ok(faces)
}

/// The lines sofel produces for one glyph, in the same form.
pub fn sofel_glyph(
    outlines: &Outlines<'_>,
    glyph: u32,
    scale: Scale,
    scratch: &mut Scratch,
) -> GlyphLines {
    let mut lines = Vec::new();
    let r = outlines.draw(
        GlyphId(glyph),
        scale,
        scratch,
        &mut FnPen(|c| {
            lines.push(match c {
                Command::MoveTo(p) => format!("M {} {}", p.x, p.y),
                Command::LineTo(p) => format!("L {} {}", p.x, p.y),
                Command::QuadTo(c, p) => format!("Q {} {} {} {}", c.x, c.y, p.x, p.y),
                Command::CubicTo(a, b, p) => {
                    format!("C {} {} {} {} {} {}", a.x, a.y, b.x, b.y, p.x, p.y)
                }
                Command::Close => "Z".to_string(),
            });
        }),
    );
    match r {
        Ok(info) => {
            lines.push(format!("overlap {}", u8::from(info.overlap)));
            let b = info.cbox;
            lines.push(format!(
                "cbox {} {} {} {}",
                b.x_min, b.y_min, b.x_max, b.y_max
            ));
        }
        Err(_) => lines.push("error".to_string()),
    }
    lines
}

/// Whether sofel draws this face's outlines: from glyf or GLYF, or
/// (for `OTTO` fonts) from CFF or CFF2 (ADR 0021).
pub fn has_glyf(font: &Font<'_>) -> bool {
    if font.sfnt().version() == u32::from_be_bytes(*b"OTTO") {
        return sofel_font::Cff::new(font).is_ok();
    }
    font.table(Tag::new(b"glyf")).is_some() || font.table(Tag::new(b"GLYF")).is_some()
}

/// The `Scale` for an `ft-dump --outline` size.
pub fn scale_for(font: &Font<'_>, size: &str) -> Scale {
    match size.parse::<u16>() {
        Ok(ppem) => Scale::pixels(font, ppem, ppem),
        Err(_) => Scale::Unscaled,
    }
}
