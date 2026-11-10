//! The CID queries and the PostScript font information of `ft-dump
//! --cid` and `ft-dump --ps-info` (ADR 0021), and sofel's lines in the
//! same form, with FreeType's error codes reduced to `E`.

use std::collections::BTreeMap;

use sofel_font::read::GlyphId;
use sofel_font::{Cff, Font};

/// The lines of each face that FreeType could open, by face index.
pub fn parse(text: &str) -> BTreeMap<u32, Vec<String>> {
    let mut faces = BTreeMap::new();
    let mut cur: Option<(u32, Vec<String>, bool)> = None;
    for line in text.lines() {
        let mut w = line.split(' ');
        match w.next() {
            Some("face") => {
                cur = w
                    .next()
                    .and_then(|v| v.parse().ok())
                    .map(|i| (i, Vec::new(), false));
            }
            Some("end") => {
                if let Some((i, lines, error)) = cur.take()
                    && !error
                {
                    faces.insert(i, lines);
                }
            }
            Some("error") => {
                if let Some(c) = cur.as_mut() {
                    c.2 = true;
                }
            }
            Some(key) => {
                if let Some(c) = cur.as_mut() {
                    c.1.push(normalize(key, line));
                }
            }
            None => {}
        }
    }
    faces
}

/// The error code (the second word, or the third for `cid`) as `E` when
/// it is not 0; a failed `ros`/`ps_font_info` keeps no values.
fn normalize(key: &str, line: &str) -> String {
    if !matches!(key, "is_cid" | "ros" | "cid" | "ps_font_info") {
        return line.to_string();
    }
    let w: Vec<&str> = line.split(' ').collect();
    let at = if key == "cid" { 2 } else { 1 };
    match w.get(at) {
        Some(&"0") | None => line.to_string(),
        Some(_) if key == "ros" || key == "ps_font_info" => format!("{key} E"),
        Some(_) => {
            let mut v: Vec<String> = w.iter().map(|s| s.to_string()).collect();
            v[at] = "E".to_string();
            v.join(" ")
        }
    }
}

fn hex(s: Option<&[u8]>) -> String {
    match s {
        None => "-".to_string(),
        Some([]) => "\"\"".to_string(),
        Some(b) => b.iter().map(|c| format!("{c:02x}")).collect(),
    }
}

fn cut(s: Option<&[u8]>) -> Option<&[u8]> {
    s.map(|s| s.split(|&b| b == 0).next().unwrap_or_default())
}

/// sofel's `--cid` lines for a face; `num_glyphs` is FreeType's.
pub fn sofel_cid(font: &Font<'_>, num_glyphs: u32) -> Vec<String> {
    let cff = Cff::new(font).ok();
    let mut out = Vec::new();
    match &cff {
        Some(c) => out.push(format!("is_cid 0 {}", u8::from(c.is_cid_keyed()))),
        None => out.push("is_cid E 0".to_string()),
    }
    match cff.as_ref().map(|c| c.ros()) {
        Some(Ok(r)) => out.push(format!(
            "ros 0 {} {} {}",
            hex(cut(r.registry)),
            hex(cut(r.ordering)),
            r.supplement
        )),
        _ => out.push("ros E".to_string()),
    }
    for g in 0..=num_glyphs {
        match cff.as_ref().map(|c| c.cid(GlyphId(g))) {
            Some(Ok(cid)) => out.push(format!("cid {g} 0 {cid}")),
            _ => out.push(format!("cid {g} E 0")),
        }
    }
    out
}

/// sofel's `--ps-info` lines for a face.
pub fn sofel_ps_info(font: &Font<'_>) -> Vec<String> {
    let cff = Cff::new(font).ok();
    let mut out = vec![format!(
        "has_ps_glyph_names {}",
        u8::from(cff.as_ref().is_some_and(|c| c.has_glyph_names()))
    )];
    match cff.as_ref().map(|c| c.ps_font_info()) {
        Some(Ok(i)) => out.push(format!(
            "ps_font_info 0 {} {} {} {} {} {} {} {} {}",
            hex(i.version),
            hex(i.notice),
            hex(i.full_name),
            hex(i.family_name),
            hex(i.weight),
            i.italic_angle,
            u8::from(i.is_fixed_pitch),
            i.underline_position,
            i.underline_thickness
        )),
        _ => out.push("ps_font_info E".to_string()),
    }
    let ps = cff.as_ref().map_or(0, |c| c.ps_fs_type());
    let flags = if ps != 0 { ps } else { font.fs_type() };
    out.push(format!("fs_type_flags {flags}"));
    out
}
