//! Item-by-item comparison of what FreeType and sofel read (ADR 0008).
//!
//! Items, identified by `<font>#<face>:<item>`:
//! - `open`: when FreeType opens the face, sofel reads its table
//!   directory. The other direction is not checked: FreeType opens a face
//!   only when it can render it (for example, it needs `glyf` or `CFF`),
//!   while sofel-read only reads the directory. Faces FreeType does not
//!   open are not compared further.
//! - `head`, `hhea`, `maxp`: the fields. FreeType adjusts two `maxp`
//!   fields for its interpreter when loading (`src/sfnt/ttload.c`): it
//!   raises `maxFunctionDefs` to 64 and caps `maxTwilightPoints` at
//!   0xFFFB. The same adjustment is applied to sofel's raw values here.
//! - `cmap:<platform>:<encoding>[:<n>]`: for each charmap FreeType
//!   accepted, sofel reads the same subtable (the `n`-th with that
//!   platform and encoding, when there are several) with the same format
//!   and mappings. Glyphs not below FreeType's numGlyphs are left out, as
//!   `FT_Get_Next_Char` skips them. Charmaps of platform 7 (Adobe) that
//!   are not in the `cmap` table are synthesized by FreeType from
//!   PostScript glyph names and are not compared.
//! - `cmap-validate:<platform>:<encoding>[:<n>]`: for each subtable
//!   FreeType dropped, the sofel validator reports an Error.
//! - `hmtx`: the advances of all glyphs below FreeType's numGlyphs.
//! - ADR 0016: `fs_type`, `ps_name`, `face_metrics` (the `FT_Face`
//!   values), `sfnt_names` (every `FT_Get_Sfnt_Name` record),
//!   `v_advance` (every glyph, `FT_LOAD_VERTICAL_LAYOUT`), `glyph_names`
//!   (`FT_Get_Glyph_Name`, every glyph) and `name_index`
//!   (`FT_Get_Name_Index` of every glyph's name).

use std::collections::BTreeMap;

use crate::dump::{Charmap, Face};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub ok: bool,
    /// What differed, for failure messages.
    pub detail: String,
}

fn item(font: &str, face: u32, name: &str, ok: bool, detail: impl Into<String>) -> Item {
    Item {
        id: format!("{font}#{face}:{name}"),
        ok,
        detail: if ok { String::new() } else { detail.into() },
    }
}

fn fields(name: &str, ft: &[(String, String)], ours: &[(String, String)]) -> String {
    let diff: Vec<String> = ft
        .iter()
        .zip(ours)
        .filter(|(a, b)| a != b)
        .map(|(a, b)| format!("{} {}: FreeType {}, sofel {} {}", name, a.0, a.1, b.0, b.1))
        .collect();
    if diff.is_empty() {
        format!(
            "{name}: FreeType has {} fields, sofel {}",
            ft.len(),
            ours.len()
        )
    } else {
        diff.join("; ")
    }
}

/// Keys `(platform, encoding, n)` where `n` counts earlier charmaps with
/// the same platform and encoding.
fn keyed(maps: &[Charmap]) -> BTreeMap<(u16, u16, usize), &Charmap> {
    let mut count: BTreeMap<(u16, u16), usize> = BTreeMap::new();
    maps.iter()
        .map(|m| {
            let n = count.entry((m.platform, m.encoding)).or_insert(0);
            let key = (m.platform, m.encoding, *n);
            *n += 1;
            (key, m)
        })
        .collect()
}

fn cmap_name(prefix: &str, (p, e, n): (u16, u16, usize)) -> String {
    if n == 0 {
        format!("{prefix}:{p}:{e}")
    } else {
        format!("{prefix}:{p}:{e}:{n}")
    }
}

fn first_difference(ft: &[(u32, u32)], ours: &[(u32, u32)]) -> String {
    let i = ft
        .iter()
        .zip(ours)
        .position(|(a, b)| a != b)
        .unwrap_or(ft.len().min(ours.len()));
    format!(
        "{} mappings in FreeType, {} in sofel; first difference at #{i}: FreeType {:?}, sofel {:?}",
        ft.len(),
        ours.len(),
        ft.get(i),
        ours.get(i)
    )
}

/// sofel's raw `maxp` fields as FreeType presents them.
fn freetype_maxp_view(fields: &[(String, String)]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|(k, v)| {
            let n: u32 = v.parse().unwrap_or(0);
            let v = match k.as_str() {
                "max_function_defs" => n.max(64).to_string(),
                "max_twilight_points" => n.min(0xFFFB).to_string(),
                _ => v.clone(),
            };
            (k.clone(), v)
        })
        .collect()
}

/// Compares two lists; the detail names the first difference.
fn list_item<T: PartialEq + std::fmt::Debug>(
    font: &str,
    face: u32,
    name: &str,
    ft: &[T],
    ours: &[T],
) -> Item {
    let i = ft
        .iter()
        .zip(ours)
        .position(|(a, b)| a != b)
        .unwrap_or(ft.len().min(ours.len()));
    item(
        font,
        face,
        name,
        ft == ours,
        format!(
            "{} in FreeType, {} in sofel; first difference: FreeType {:?}, sofel {:?}",
            ft.len(),
            ours.len(),
            ft.get(i),
            ours.get(i)
        ),
    )
}

/// Compares the faces of one font file.
pub fn compare(font: &str, ft: &[Face], ours: &[Face]) -> Vec<Item> {
    let mut items = Vec::new();
    for f in ft {
        if f.error.is_some() {
            continue;
        }
        let o = ours.iter().find(|o| o.index == f.index);
        items.push(item(
            font,
            f.index,
            "open",
            o.is_some_and(|o| o.error.is_none()),
            format!(
                "FreeType opens the face, sofel does not: {:?}",
                o.and_then(|o| o.error.clone())
            ),
        ));
        let Some(o) = o.filter(|o| o.error.is_none()) else {
            continue;
        };

        let our_maxp = freetype_maxp_view(&o.maxp);
        for (name, a, b) in [
            ("head", &f.head, &o.head),
            ("hhea", &f.hhea, &o.hhea),
            ("maxp", &f.maxp, &our_maxp),
        ] {
            if !a.is_empty() || !b.is_empty() {
                items.push(item(font, f.index, name, a == b, fields(name, a, b)));
            }
        }

        let limit = f.num_glyphs.unwrap_or(0);
        let ft_maps = keyed(&f.charmaps);
        let our_maps = keyed(&o.charmaps);
        for (key, fm) in &ft_maps {
            if key.0 == 7 && !our_maps.contains_key(key) {
                continue;
            }
            let name = cmap_name("cmap", *key);
            let (ok, detail) = match our_maps.get(key) {
                None => (false, "sofel has no such encoding record".to_string()),
                Some(om) if !om.readable => (false, "sofel cannot read the subtable".to_string()),
                Some(om) if om.format != fm.format => (
                    false,
                    format!("format: FreeType {}, sofel {}", fm.format, om.format),
                ),
                Some(om) => {
                    let ours: Vec<(u32, u32)> = om
                        .maps
                        .iter()
                        .copied()
                        .filter(|&(_, g)| g < limit)
                        .collect();
                    (ours == fm.maps, first_difference(&fm.maps, &ours))
                }
            };
            items.push(item(font, f.index, &name, ok, detail));
        }
        for (key, om) in &our_maps {
            if !ft_maps.contains_key(key) {
                let name = cmap_name("cmap-validate", *key);
                items.push(item(
                    font,
                    f.index,
                    &name,
                    om.validator_error,
                    format!("FreeType dropped the format {} subtable, but sofel's validator reports no Error", om.format),
                ));
            }
        }

        if !f.advances.is_empty() || !o.advances.is_empty() {
            let ours: Vec<(u32, Option<i64>)> = o.advances.clone();
            let ok = f.advances == ours;
            let i = f
                .advances
                .iter()
                .zip(&ours)
                .position(|(a, b)| a != b)
                .unwrap_or(0);
            let detail = format!(
                "{} advances in FreeType, {} in sofel; first difference: FreeType {:?}, sofel {:?}",
                f.advances.len(),
                ours.len(),
                f.advances.get(i),
                ours.get(i)
            );
            items.push(item(font, f.index, "hmtx", ok, detail));
        }

        if f.fs_type.is_some() {
            let pairs: [(&str, String, String); 2] = [
                (
                    "fs_type",
                    format!("{:?}", f.fs_type),
                    format!("{:?}", o.fs_type),
                ),
                (
                    "ps_name",
                    format!("{:?}", f.ps_name),
                    format!("{:?}", o.ps_name),
                ),
            ];
            for (name, a, b) in pairs {
                items.push(item(
                    font,
                    f.index,
                    name,
                    a == b,
                    format!("FreeType {a}, sofel {b}"),
                ));
            }
            items.push(item(
                font,
                f.index,
                "face_metrics",
                f.face_metrics == o.face_metrics,
                format!("FreeType {:?}, sofel {:?}", f.face_metrics, o.face_metrics),
            ));
            items.push(list_item(
                font,
                f.index,
                "sfnt_names",
                &f.sfnt_names,
                &o.sfnt_names,
            ));
            items.push(list_item(
                font,
                f.index,
                "v_advance",
                &f.v_advances,
                &o.v_advances,
            ));
            let names = |v: &[(u32, i64, String)]| -> Vec<(u32, bool, String)> {
                v.iter()
                    .map(|(g, e, h)| (*g, *e == 0, if *e == 0 { h.clone() } else { String::new() }))
                    .collect()
            };
            items.push(list_item(
                font,
                f.index,
                "glyph_names",
                &names(&f.glyph_names),
                &names(&o.glyph_names),
            ));
            items.push(list_item(
                font,
                f.index,
                "name_index",
                &f.name_indices,
                &o.name_indices,
            ));
        }
    }
    items
}
