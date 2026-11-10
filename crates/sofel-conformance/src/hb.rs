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
//! sofel's glyph boxes come from glyf, CFF2 and CFF
//! ([`HbExtents`], ADR 0021); the boxes of color glyphs come with
//! ADR 0023.

use sofel_font::read::GlyphId;
use sofel_font::{Font, FontExtents, GlyphExtents, HbScale, MetricsTag};
use sofel_outline::HbExtents;

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
    let extents = HbExtents::new(font);
    let boxes = |g: GlyphId| extents.glyph_extents(g);
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
                        extents.scaled_glyph_extents(g, &scale).map(ext4),
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

/// The output of `hb-dump --names` for one face.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HbNames {
    pub index: u32,
    /// `(name id, language)`.
    pub names: Vec<(u16, String)>,
    /// `(kind, name id, language, capacity, total, written, hex)`.
    pub utf: Vec<(String, u16, String, usize, usize, usize, String)>,
    /// `(glyph, found, hex)`.
    pub glyph_names: Vec<(u32, bool, String)>,
    /// `(glyph, found, glyph from its name)`.
    pub from_names: Vec<(u32, bool, u32)>,
}

/// Parses the output of `hb-dump --names`.
pub fn parse_names(text: &str) -> Result<Vec<HbNames>, String> {
    let mut faces: Vec<HbNames> = Vec::new();
    for line in text.lines() {
        let mut w = line.split(' ');
        let key = w.next().unwrap_or("");
        if key == "face" {
            faces.push(HbNames {
                index: num(w.next(), line)?,
                ..HbNames::default()
            });
            continue;
        }
        let face = faces
            .last_mut()
            .ok_or_else(|| format!("line before any face: {line}"))?;
        let word = |w: Option<&str>| {
            w.map(str::to_string)
                .ok_or_else(|| format!("bad line: {line}"))
        };
        match key {
            "name" => face.names.push((num(w.next(), line)?, word(w.next())?)),
            "utf8" | "utf16" | "utf32" => face.utf.push((
                key.to_string(),
                num(w.next(), line)?,
                word(w.next())?,
                num(w.next(), line)?,
                num(w.next(), line)?,
                num(w.next(), line)?,
                word(w.next())?,
            )),
            "glyph_name" => {
                let g = num(w.next(), line)?;
                let found: u8 = num(w.next(), line)?;
                face.glyph_names.push((g, found != 0, word(w.next())?));
            }
            "glyph_from_name" => {
                let g = num(w.next(), line)?;
                let found: u8 = num(w.next(), line)?;
                face.from_names.push((g, found != 0, num(w.next(), line)?));
            }
            "end" | "" => {}
            other => return Err(format!("unknown line kind {other}: {line}")),
        }
    }
    Ok(faces)
}

fn units_hex<T: Copy + Into<u64>>(u: &[T], digits: usize) -> String {
    if u.is_empty() {
        return "-".to_string();
    }
    u.iter()
        .map(|&v| format!("{:0digits$x}", v.into()))
        .collect()
}

fn from_hex(h: &str) -> Vec<u8> {
    if h == "-" {
        return Vec::new();
    }
    (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap_or(0))
        .collect()
}

/// sofel's values for the calls `hb-dump --names` made.
pub fn sofel_names(font: &Font<'_>, hb: &HbNames) -> HbNames {
    let names = font
        .names()
        .map(|e| {
            let mut b = vec![0u8; e.language.len()];
            e.language.write(&mut b);
            (e.name_id, String::from_utf8_lossy(&b).into_owned())
        })
        .collect();
    let utf = hb
        .utf
        .iter()
        .map(|(kind, id, lang, cap, _, _, _)| {
            let l = (lang != "-").then_some(lang.as_bytes());
            let (n, hex) = match kind.as_str() {
                "utf8" => {
                    let mut b = vec![0u8; *cap];
                    let n = font.name_utf8(*id, l, &mut b);
                    (n, units_hex(&b[..n.written], 2))
                }
                "utf16" => {
                    let mut b = vec![0u16; *cap];
                    let n = font.name_utf16(*id, l, &mut b);
                    (n, units_hex(&b[..n.written], 4))
                }
                _ => {
                    let mut b = vec![0u32; *cap];
                    let n = font.name_utf32(*id, l, &mut b);
                    (n, units_hex(&b[..n.written], 8))
                }
            };
            (
                kind.clone(),
                *id,
                lang.clone(),
                *cap,
                n.total,
                n.written,
                hex,
            )
        })
        .collect();
    let mut index = vec![0u32; font.glyph_names_len()];
    let gn = font.glyph_names(&mut index);
    let glyph_names = hb
        .glyph_names
        .iter()
        .map(|&(g, _, _)| {
            let n = font.glyph_name(GlyphId(g));
            // hb-dump reads the name back as a C string from 256 bytes.
            let n = n.map(|n| {
                let n = &n[..n.len().min(255)];
                n.split(|&b| b == 0).next().unwrap_or_default()
            });
            (g, n.is_some(), n.map_or("-".to_string(), crate::dump::hex))
        })
        .collect();
    let from_names = hb
        .from_names
        .iter()
        .map(|&(g, _, _)| {
            let name = hb
                .glyph_names
                .iter()
                .find(|x| x.0 == g)
                .map(|x| from_hex(&x.2))
                .unwrap_or_default();
            let found = gn.as_ref().and_then(|n| n.glyph_from_name(&name));
            (g, found.is_some(), found.map_or(0, |g| g.0))
        })
        .collect();
    HbNames {
        index: hb.index,
        names,
        utf,
        glyph_names,
        from_names,
    }
}

fn list<T: PartialEq + std::fmt::Debug>(
    font: &str,
    face: u32,
    name: &str,
    a: &[T],
    b: &[T],
) -> Item {
    let i = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    item(
        font,
        face,
        name,
        a == b,
        format!(
            "{} in HarfBuzz, {} in sofel; first difference: HarfBuzz {:?}, sofel {:?}",
            a.len(),
            b.len(),
            a.get(i),
            b.get(i)
        ),
    )
}

/// Compares the names of one face: items `names`, `name_utf`,
/// `glyph_names` and `glyph_from_name`.
///
/// When two glyphs or more have the same name, the glyph HarfBuzz finds
/// for it depends on its sort, and sofel finds the one with the smallest
/// glyph id (ADR 0021): for such fonts `glyph_from_name` compares the
/// name of the glyph found instead of its id.
pub fn compare_names(font: &str, hb: &HbNames, ours: &HbNames) -> Vec<Item> {
    let f = hb.index;
    let named: Vec<&str> = hb
        .glyph_names
        .iter()
        .filter(|x| x.1)
        .map(|x| x.2.as_str())
        .collect();
    let mut sorted = named.clone();
    sorted.sort_unstable();
    let duplicates = sorted.windows(2).any(|w| w[0] == w[1]);
    let from_names = if duplicates {
        let name_of = |g: u32| {
            hb.glyph_names
                .iter()
                .find(|x| x.0 == g)
                .map_or("-".to_string(), |x| x.2.clone())
        };
        let by_name = |v: &[(u32, bool, u32)]| -> Vec<(u32, bool, String)> {
            v.iter()
                .map(|&(g, found, r)| (g, found, if found { name_of(r) } else { "-".into() }))
                .collect()
        };
        list(
            font,
            f,
            "glyph_from_name",
            &by_name(&hb.from_names),
            &by_name(&ours.from_names),
        )
    } else {
        list(font, f, "glyph_from_name", &hb.from_names, &ours.from_names)
    };
    vec![
        list(font, f, "names", &hb.names, &ours.names),
        list(font, f, "name_utf", &hb.utf, &ours.utf),
        list(font, f, "glyph_names", &hb.glyph_names, &ours.glyph_names),
        from_names,
    ]
}

/// The output of `hb-dump --cmap` for one face.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HbCmap {
    pub index: u32,
    pub num_glyphs: u32,
    /// `(code point, glyph)`.
    pub maps: Vec<(u32, u32)>,
}

/// Parses the output of `hb-dump --cmap`.
pub fn parse_cmap(text: &str) -> Result<Vec<HbCmap>, String> {
    let mut faces: Vec<HbCmap> = Vec::new();
    for line in text.lines() {
        let mut w = line.split(' ');
        match w.next().unwrap_or("") {
            "face" => faces.push(HbCmap {
                index: num(w.next(), line)?,
                ..HbCmap::default()
            }),
            "num_glyphs" => {
                faces
                    .last_mut()
                    .ok_or_else(|| format!("line before any face: {line}"))?
                    .num_glyphs = num(w.next(), line)?
            }
            "map" => {
                let m = (num(w.next(), line)?, num(w.next(), line)?);
                faces
                    .last_mut()
                    .ok_or_else(|| format!("line before any face: {line}"))?
                    .maps
                    .push(m);
            }
            "end" | "" => {}
            other => return Err(format!("unknown line kind {other}: {line}")),
        }
    }
    Ok(faces)
}

/// Item `nominal`: `hb_font_get_nominal_glyph` for every code point.
/// Glyphs HarfBuzz gives that are not below the glyph count are left
/// out, as sofel counts them as unmapped (ADR 0010).
pub fn compare_cmap(font_id: &str, hb: &HbCmap, font: &Font<'_>) -> Item {
    let want: Vec<(u32, u32)> = hb
        .maps
        .iter()
        .copied()
        .filter(|&(_, g)| g < hb.num_glyphs)
        .collect();
    let ours: Vec<(u32, u32)> = (0..=0x10_FFFF)
        .filter_map(|c| font.nominal_glyph(c).map(|g| (c, g.0)))
        .collect();
    list(font_id, hb.index, "nominal", &want, &ours)
}
