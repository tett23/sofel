//! The MATH table's acceptance and extended shapes against HarfBuzz
//! (ADR 0091), from `hb-dump --math`.
//!
//! For each face `hb-dump --math` writes `face <index>`, `math <has data>
//! <glyph count>` (`hb_ot_math_has_data`, `hb_face_get_glyph_count`),
//! `extended <glyph ids>` (the glyphs among every glyph, the first id past
//! them and 0xFFFF for which `hb_ot_math_is_glyph_extended_shape` is
//! true) and `end`. One item per face, `<font>#<face>:hb:math`, compares
//! both values.

use sofel_font::read::GlyphId;
use sofel_font::{Font, HbScale};

use crate::compare::Item;

/// The output of `hb-dump --math` for one face.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HbMath {
    /// The face index.
    pub index: u32,
    /// `hb_ot_math_has_data`.
    pub has_data: bool,
    /// HarfBuzz's glyph count of the face.
    pub num_glyphs: u32,
    /// The extended shapes, ascending.
    pub extended: Vec<u32>,
}

fn num<T: core::str::FromStr>(s: Option<&str>, line: &str) -> Result<T, String> {
    s.and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("bad line: {line}"))
}

/// Parses the output of `hb-dump --math`.
pub fn parse_math(text: &str) -> Result<Vec<HbMath>, String> {
    let mut faces: Vec<HbMath> = Vec::new();
    for line in text.lines() {
        let mut w = line.split(' ');
        let key = w.next().unwrap_or("");
        if key == "face" {
            faces.push(HbMath {
                index: num(w.next(), line)?,
                ..HbMath::default()
            });
            continue;
        }
        let face = faces
            .last_mut()
            .ok_or_else(|| format!("line before any face: {line}"))?;
        match key {
            "math" => {
                face.has_data = num::<u8>(w.next(), line)? != 0;
                face.num_glyphs = num(w.next(), line)?;
            }
            "extended" => {
                face.extended = w
                    .filter(|s| !s.is_empty())
                    .map(|s| num(Some(s), line))
                    .collect::<Result<_, _>>()?;
            }
            "end" | "" => {}
            other => return Err(format!("unknown line kind {other}: {line}")),
        }
    }
    Ok(faces)
}

/// The glyph ids `hb-dump --math` probes for a face of `num_glyphs`.
pub fn probes(num_glyphs: u32) -> impl Iterator<Item = u32> {
    (0..=num_glyphs).chain((num_glyphs < 0xFFFF).then_some(0xFFFF))
}

/// sofel's values for the probes `hb-dump --math` made of `hb`.
pub fn sofel_math(font: &Font<'_>, hb: &HbMath) -> HbMath {
    let instance = font.default_instance();
    let query = instance.layout_queries(HbScale::unscaled(font), 0, 0);
    HbMath {
        index: hb.index,
        has_data: query.has_math(),
        num_glyphs: hb.num_glyphs,
        extended: probes(hb.num_glyphs)
            .filter(|g| query.math_is_extended_shape(GlyphId(*g)))
            .collect(),
    }
}

/// The item of one face.
pub fn compare_math(font_id: &str, hb: &HbMath, ours: &HbMath) -> Item {
    let ok = hb == ours;
    Item {
        id: format!("{font_id}#{}:hb:math", hb.index),
        ok,
        detail: if ok {
            String::new()
        } else {
            format!(
                "HarfBuzz math {} extended {:?}, sofel math {} extended {:?}",
                hb.has_data, hb.extended, ours.has_data, ours.extended
            )
        },
    }
}
