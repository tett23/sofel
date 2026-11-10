//! What HarfBuzz (through `hb-dump`) and sofel give for font-wide and
//! glyph metrics at HarfBuzz's scales (ADR 0016). The format of
//! `hb-dump` is described at the top of `tools/hb-dump/hb-dump.cc`.
//!
//! Items, identified by `<font>#<face>:hb:<item>`, each covering every
//! scale `hb-dump` writes:
//! - `num_glyphs`: the glyph count.
//! - `h_extents`, `v_extents`: `hb_font_get_extents_for_direction`.
//! - `metrics`: `hb_ot_metrics_get_position` for every tag.
//! - `metrics-fallback`: `hb_ot_metrics_get_position_with_fallback` for
//!   every tag.
//! - `h_advance`, `v_advance`, `v_origin`, `extents`: every glyph.
//!
//! sofel's glyph boxes come from glyf only ([`Font::glyf_extents`]);
//! the boxes of other outline formats and of color glyphs come with
//! ADR 0021 and ADR 0023.

use sofel_font::read::GlyphId;
use sofel_font::{Font, FontExtents, GlyphExtents, HbScale, MetricsTag};

use crate::compare::Item;

/// `(h_advance, v_advance, v_origin_x, v_origin_y, extents)` of a glyph.
pub type GlyphValues = (i32, i32, i32, i32, Option<[i32; 4]>);

/// The values of one scale.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scaled {
    pub x_scale: i32,
    pub y_scale: i32,
    pub h_extents: [i32; 3],
    pub v_extents: [i32; 3],
    /// `(tag, found, value, fallback)`.
    pub metrics: Vec<(String, bool, i32, i32)>,
    /// `(h_advance, v_advance, v_origin_x, v_origin_y, extents)`.
    pub glyphs: Vec<GlyphValues>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HbFace {
    pub index: u32,
    pub num_glyphs: u32,
    pub upem: i32,
    pub scales: Vec<Scaled>,
}

fn scaled<'f>(face: &'f mut HbFace, line: &str) -> Result<&'f mut Scaled, String> {
    face.scales
        .last_mut()
        .ok_or_else(|| format!("line before any scale: {line}"))
}

fn num<T: std::str::FromStr>(s: Option<&str>, line: &str) -> Result<T, String> {
    s.and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("bad line: {line}"))
}

/// Parses the output of `hb-dump`.
pub fn parse(text: &str) -> Result<Vec<HbFace>, String> {
    let mut faces: Vec<HbFace> = Vec::new();
    for line in text.lines() {
        let mut w = line.split(' ');
        let key = w.next().unwrap_or("");
        if key == "face" {
            faces.push(HbFace {
                index: num(w.next(), line)?,
                ..HbFace::default()
            });
            continue;
        }
        let face = faces
            .last_mut()
            .ok_or_else(|| format!("line before any face: {line}"))?;
        match key {
            "num_glyphs" => face.num_glyphs = num(w.next(), line)?,
            "upem" => face.upem = num(w.next(), line)?,
            "scale" => face.scales.push(Scaled {
                x_scale: num(w.next(), line)?,
                y_scale: num(w.next(), line)?,
                ..Scaled::default()
            }),
            "h_extents" | "v_extents" => {
                let v = [
                    num(w.next(), line)?,
                    num(w.next(), line)?,
                    num(w.next(), line)?,
                ];
                let s = scaled(face, line)?;
                if key == "h_extents" {
                    s.h_extents = v;
                } else {
                    s.v_extents = v;
                }
            }
            "metric" => {
                let tag = w
                    .next()
                    .ok_or_else(|| format!("bad line: {line}"))?
                    .to_string();
                let found: u8 = num(w.next(), line)?;
                let entry = (tag, found != 0, num(w.next(), line)?, num(w.next(), line)?);
                scaled(face, line)?.metrics.push(entry);
            }
            "glyph" => {
                let _g: u32 = num(w.next(), line)?;
                let v: Vec<i32> = w
                    .map(|x| x.parse().map_err(|_| format!("bad line: {line}")))
                    .collect::<Result<_, _>>()?;
                let [ha, va, ox, oy, has, xb, yb, wd, ht] = v[..] else {
                    return Err(format!("bad line: {line}"));
                };
                let e = (has != 0).then_some([xb, yb, wd, ht]);
                scaled(face, line)?.glyphs.push((ha, va, ox, oy, e));
            }
            "end" | "" => {}
            other => return Err(format!("unknown line kind {other}: {line}")),
        }
    }
    Ok(faces)
}

fn ext3(e: FontExtents) -> [i32; 3] {
    [e.ascender, e.descender, e.line_gap]
}

fn ext4(e: GlyphExtents) -> [i32; 4] {
    [e.x_bearing, e.y_bearing, e.width, e.height]
}

/// sofel's values for the scales HarfBuzz wrote, glyphs below
/// `num_glyphs`.
pub fn sofel_face(font: &Font<'_>, hb: &HbFace) -> HbFace {
    let boxes = |g: GlyphId| font.glyf_extents(g);
    let scales = hb
        .scales
        .iter()
        .map(|s| {
            let scale = HbScale::for_font(font, s.x_scale, s.y_scale);
            let metrics = MetricsTag::ALL
                .iter()
                .map(|&t| {
                    let found = scale.metrics_position(font, t);
                    let fallback = scale.metrics_position_with_fallback(font, t, boxes);
                    let tag = String::from_utf8_lossy(&t.tag().0).into_owned();
                    (tag, found.is_some(), found.unwrap_or(0), fallback)
                })
                .collect();
            let glyphs = (0..hb.num_glyphs)
                .map(|g| {
                    let g = GlyphId(g);
                    let (ox, oy) = scale.v_origin(font, g, boxes);
                    (
                        scale.h_advance(font, g),
                        scale.v_advance(font, g),
                        ox,
                        oy,
                        scale.glyf_extents(font, g).map(ext4),
                    )
                })
                .collect();
            Scaled {
                x_scale: s.x_scale,
                y_scale: s.y_scale,
                h_extents: ext3(scale.h_extents(font)),
                v_extents: ext3(scale.v_extents(font)),
                metrics,
                glyphs,
            }
        })
        .collect();
    HbFace {
        index: hb.index,
        num_glyphs: font.num_glyphs(),
        upem: i32::from(font.hb_units_per_em()),
        scales,
    }
}

fn item(font: &str, face: u32, name: &str, ok: bool, detail: impl Into<String>) -> Item {
    Item {
        id: format!("{font}#{face}:hb:{name}"),
        ok,
        detail: if ok { String::new() } else { detail.into() },
    }
}

/// The first difference of a per-scale value, for failure messages.
fn per_scale<T: PartialEq + std::fmt::Debug>(
    hb: &HbFace,
    ours: &HbFace,
    get: impl Fn(&Scaled) -> T,
) -> (bool, String) {
    for (a, b) in hb.scales.iter().zip(&ours.scales) {
        let (x, y) = (get(a), get(b));
        if x != y {
            return (
                false,
                format!(
                    "at scale {}x{}: HarfBuzz {x:?}, sofel {y:?}",
                    a.x_scale, a.y_scale
                ),
            );
        }
    }
    (
        hb.scales.len() == ours.scales.len(),
        "scale count differs".to_string(),
    )
}

/// The first glyph whose value differs, for failure messages.
fn per_glyph<T: PartialEq + std::fmt::Debug>(
    hb: &HbFace,
    ours: &HbFace,
    get: impl Fn(&GlyphValues) -> T,
) -> (bool, String) {
    for (a, b) in hb.scales.iter().zip(&ours.scales) {
        for (g, (x, y)) in a.glyphs.iter().zip(&b.glyphs).enumerate() {
            let (x, y) = (get(x), get(y));
            if x != y {
                return (
                    false,
                    format!(
                        "glyph {g} at scale {}x{}: HarfBuzz {x:?}, sofel {y:?}",
                        a.x_scale, a.y_scale
                    ),
                );
            }
        }
        if a.glyphs.len() != b.glyphs.len() {
            return (false, "glyph count differs".to_string());
        }
    }
    (true, String::new())
}

/// Compares one face.
pub fn compare(font: &str, hb: &HbFace, ours: &HbFace) -> Vec<Item> {
    let f = hb.index;
    let mut items = vec![item(
        font,
        f,
        "num_glyphs",
        hb.num_glyphs == ours.num_glyphs && hb.upem == ours.upem,
        format!(
            "HarfBuzz {} glyphs, upem {}; sofel {} glyphs, upem {}",
            hb.num_glyphs, hb.upem, ours.num_glyphs, ours.upem
        ),
    )];
    let mut push = |name: &str, (ok, detail): (bool, String)| {
        items.push(item(font, f, name, ok, detail));
    };
    push("h_extents", per_scale(hb, ours, |s| s.h_extents));
    push("v_extents", per_scale(hb, ours, |s| s.v_extents));
    push(
        "metrics",
        per_scale(hb, ours, |s| {
            s.metrics
                .iter()
                .map(|(t, found, v, _)| (t.clone(), *found, *v))
                .collect::<Vec<_>>()
        }),
    );
    push(
        "metrics-fallback",
        per_scale(hb, ours, |s| {
            s.metrics
                .iter()
                .map(|(t, _, _, fb)| (t.clone(), *fb))
                .collect::<Vec<_>>()
        }),
    );
    push("h_advance", per_glyph(hb, ours, |g| g.0));
    push("v_advance", per_glyph(hb, ours, |g| g.1));
    push("v_origin", per_glyph(hb, ours, |g| (g.2, g.3)));
    push("extents", per_glyph(hb, ours, |g| g.4));
    items
}
