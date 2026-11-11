//! What FreeType (through `ft-dump`) and sofel read from a font, in one
//! shape so that they can be compared. The format of `ft-dump` is
//! described at the top of `tools/ft-dump/ft-dump.c`.

use sofel_read::cmap::{Cmap, Subtable};
use sofel_read::head::Head;
use sofel_read::hhea::Hhea;
use sofel_read::hmtx::Hmtx;
use sofel_read::maxp::Maxp;
use sofel_read::sfnt::{FontFile, Sfnt};
use sofel_read::{CountWidth, GlyphId, Severity, Tag, cmap};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Face {
    pub index: u32,
    /// Why the face could not be opened.
    pub error: Option<String>,
    pub num_glyphs: Option<u32>,
    pub head: Vec<(String, String)>,
    pub hhea: Vec<(String, String)>,
    pub maxp: Vec<(String, String)>,
    pub charmaps: Vec<Charmap>,
    /// `(glyph, advance)`; `None` when sofel could not read it.
    pub advances: Vec<(u32, Option<i64>)>,
    /// ADR 0016: `FT_Get_FSType_Flags`.
    pub fs_type: Option<u32>,
    /// `FT_Get_Postscript_Name`, as hex (`-` for none).
    pub ps_name: Option<String>,
    /// The `FT_Face` values.
    pub face_metrics: Vec<i64>,
    /// `FT_Get_Sfnt_Name`: `(platform, encoding, language, name, hex)`.
    pub sfnt_names: Vec<(u32, u32, u32, u32, String)>,
    /// `(glyph, vertical advance)`.
    pub v_advances: Vec<(u32, i64)>,
    /// Raw hmtx/vmtx side bearings read from the face's SFNT tables.
    pub bearings: Vec<(u32, i64, Option<i64>)>,
    /// `(glyph, error, hex)`.
    pub glyph_names: Vec<(u32, i64, String)>,
    /// `(glyph, FT_Get_Name_Index of its name)`.
    pub name_indices: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Charmap {
    pub platform: u16,
    pub encoding: u16,
    pub format: i64,
    pub maps: Vec<(u32, u32)>,
    /// sofel only: whether the subtable could be read.
    pub readable: bool,
    /// sofel only: whether its validator reported an Error.
    pub validator_error: bool,
}

// ---------------------------------------------------------------- ft-dump

fn num<T: std::str::FromStr>(s: Option<&str>, line: &str) -> Result<T, String> {
    s.and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("bad line: {line}"))
}

/// Parses the output of `ft-dump`.
pub fn parse_ft_dump(text: &str) -> Result<Vec<Face>, String> {
    let mut faces: Vec<Face> = Vec::new();
    for line in text.lines() {
        let mut words = line.split(' ');
        let key = words.next().unwrap_or("");
        if key == "face" {
            faces.push(Face {
                index: num(words.next(), line)?,
                ..Face::default()
            });
            continue;
        }
        let face = faces
            .last_mut()
            .ok_or_else(|| format!("line before any face: {line}"))?;
        match key {
            "error" => face.error = Some(words.collect::<Vec<_>>().join(" ")),
            "num_glyphs" => face.num_glyphs = Some(num(words.next(), line)?),
            "head" | "hhea" | "maxp" => {
                let field = words
                    .next()
                    .ok_or_else(|| format!("bad line: {line}"))?
                    .to_string();
                let value = words.collect::<Vec<_>>().join(" ");
                let list = match key {
                    "head" => &mut face.head,
                    "hhea" => &mut face.hhea,
                    _ => &mut face.maxp,
                };
                list.push((field, value));
            }
            "cmap" => face.charmaps.push(Charmap {
                platform: num(words.next(), line)?,
                encoding: num(words.next(), line)?,
                format: num(words.next(), line)?,
                maps: Vec::new(),
                readable: true,
                validator_error: false,
            }),
            "map" => {
                let pair = (num(words.next(), line)?, num(words.next(), line)?);
                face.charmaps
                    .last_mut()
                    .ok_or_else(|| format!("map before any cmap: {line}"))?
                    .maps
                    .push(pair);
            }
            "advance" => face
                .advances
                .push((num(words.next(), line)?, Some(num(words.next(), line)?))),
            "fs_type" => face.fs_type = Some(num(words.next(), line)?),
            "ps_name" => face.ps_name = Some(words.next().unwrap_or("-").to_string()),
            "face_metrics" => {
                face.face_metrics = words
                    .map(|w| w.parse().map_err(|_| format!("bad line: {line}")))
                    .collect::<Result<_, _>>()?;
            }
            "sfnt_name" => {
                let _i: u32 = num(words.next(), line)?;
                face.sfnt_names.push((
                    num(words.next(), line)?,
                    num(words.next(), line)?,
                    num(words.next(), line)?,
                    num(words.next(), line)?,
                    words.next().unwrap_or("-").to_string(),
                ));
            }
            "v_advance" => face
                .v_advances
                .push((num(words.next(), line)?, num(words.next(), line)?)),
            "bearing" => face.bearings.push((
                num(words.next(), line)?,
                num(words.next(), line)?,
                words
                    .next()
                    .filter(|value| *value != "-")
                    .map(|value| num(Some(value), line))
                    .transpose()?,
            )),
            "glyph_name" => face.glyph_names.push((
                num(words.next(), line)?,
                num(words.next(), line)?,
                words.next().unwrap_or("-").to_string(),
            )),
            "name_index" => face
                .name_indices
                .push((num(words.next(), line)?, num(words.next(), line)?)),
            "end" | "" => {}
            other => return Err(format!("unknown line kind {other}: {line}")),
        }
    }
    Ok(faces)
}

// ---------------------------------------------------------------- sofel

fn field(name: &str, value: impl ToString) -> (String, String) {
    (name.to_string(), value.to_string())
}

/// The faces of `file` as sofel reads them. `glyphs_for(face)` gives the
/// number of glyphs to list advances for.
pub fn sofel_faces(file: &[u8], glyphs_for: &dyn Fn(u32) -> Option<u32>) -> Vec<Face> {
    let fonts: Vec<Result<Sfnt<'_>, String>> = match FontFile::new(file) {
        Err(e) => vec![Err(e.to_string())],
        Ok(FontFile::Sfnt(s)) => vec![Ok(s)],
        Ok(FontFile::Collection(c)) => (0..c.num_fonts())
            .map(|i| c.font(i).map_err(|e| e.to_string()))
            .collect(),
        Ok(FontFile::Dfont(d)) => (0..d.num_fonts())
            .map(|i| d.font(i).map_err(|e| e.to_string()))
            .collect(),
    };
    fonts
        .into_iter()
        .zip(0u32..)
        .map(|(font, index)| match font {
            Err(e) => Face {
                index,
                error: Some(e),
                ..Face::default()
            },
            Ok(s) => sofel_face(&s, index, glyphs_for(index)),
        })
        .collect()
}

fn sofel_face(s: &Sfnt<'_>, index: u32, list_glyphs: Option<u32>) -> Face {
    let maxp = s
        .table(Tag::new(b"maxp"))
        .and_then(|d| Maxp::new(d, CountWidth::U16).ok());
    let hhea = s
        .table(Tag::new(b"hhea"))
        .and_then(|d| Hhea::new(d, CountWidth::U16).ok());
    let num_glyphs = maxp.map(|m| m.num_glyphs());

    let head = s
        .table(Tag::new(b"head"))
        .and_then(|d| Head::new(d).ok())
        .map_or_else(Vec::new, |h| {
            let date = |d: sofel_read::LongDateTime| {
                let v = d.0 as u64;
                format!("{} {}", v >> 32, v & 0xFFFF_FFFF)
            };
            vec![
                field(
                    "version",
                    (u32::from(h.major_version()) << 16) | u32::from(h.minor_version()),
                ),
                field("font_revision", h.font_revision().0),
                field("checksum_adjustment", h.checksum_adjustment()),
                field("magic_number", h.magic_number()),
                field("flags", h.flags()),
                field("units_per_em", h.units_per_em()),
                field("created", date(h.created())),
                field("modified", date(h.modified())),
                field("x_min", h.x_min()),
                field("y_min", h.y_min()),
                field("x_max", h.x_max()),
                field("y_max", h.y_max()),
                field("mac_style", h.mac_style()),
                field("lowest_rec_ppem", h.lowest_rec_ppem()),
                field("font_direction_hint", h.font_direction_hint()),
                field("index_to_loc_format", h.index_to_loc_format()),
                field("glyph_data_format", h.glyph_data_format()),
            ]
        });

    let hhea_fields = hhea.map_or_else(Vec::new, |h| {
        vec![
            field(
                "version",
                (u32::from(h.major_version()) << 16) | u32::from(h.minor_version()),
            ),
            field("ascender", h.ascender()),
            field("descender", h.descender()),
            field("line_gap", h.line_gap()),
            field("advance_width_max", h.advance_width_max()),
            field("min_left_side_bearing", h.min_left_side_bearing()),
            field("min_right_side_bearing", h.min_right_side_bearing()),
            field("x_max_extent", h.x_max_extent()),
            field("caret_slope_rise", h.caret_slope_rise()),
            field("caret_slope_run", h.caret_slope_run()),
            field("caret_offset", h.caret_offset()),
            field("metric_data_format", h.metric_data_format()),
            field("number_of_h_metrics", h.number_of_h_metrics()),
        ]
    });

    let maxp_fields = maxp.map_or_else(Vec::new, |m| {
        let mut f = vec![
            field("version", m.version()),
            field("num_glyphs", m.num_glyphs()),
        ];
        if let Some(t) = m.truetype() {
            f.extend([
                field("max_points", t.max_points),
                field("max_contours", t.max_contours),
                field("max_composite_points", t.max_composite_points),
                field("max_composite_contours", t.max_composite_contours),
                field("max_zones", t.max_zones),
                field("max_twilight_points", t.max_twilight_points),
                field("max_storage", t.max_storage),
                field("max_function_defs", t.max_function_defs),
                field("max_instruction_defs", t.max_instruction_defs),
                field("max_stack_elements", t.max_stack_elements),
                field("max_size_of_instructions", t.max_size_of_instructions),
                field("max_component_elements", t.max_component_elements),
                field("max_component_depth", t.max_component_depth),
            ]);
        }
        f
    });

    let charmaps = s
        .table(cmap::TAG)
        .map_or_else(Vec::new, |data| charmaps(data, num_glyphs));

    let advances = match (s.table(Tag::new(b"hmtx")), hhea, num_glyphs, list_glyphs) {
        (Some(d), Some(h), Some(n), Some(list)) => match Hmtx::new(d, h.number_of_h_metrics(), n) {
            Ok(m) => (0..list)
                .map(|g| (g, m.advance(GlyphId(g)).map(i64::from)))
                .collect(),
            Err(_) => (0..list).map(|g| (g, None)).collect(),
        },
        (_, _, _, Some(list)) => (0..list).map(|g| (g, None)).collect(),
        _ => Vec::new(),
    };

    Face {
        index,
        error: None,
        num_glyphs,
        head,
        hhea: hhea_fields,
        maxp: maxp_fields,
        charmaps,
        advances,
        ..Face::default()
    }
}

/// Hex of bytes, `-` for none.
pub fn hex(b: &[u8]) -> String {
    if b.is_empty() {
        return "-".to_string();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Fills the ADR 0016 values of `face` from sofel-font, for the glyphs
/// FreeType lists.
pub fn add_font_values(face: &mut Face, font: &sofel_font::Font<'_>, ft: &Face) {
    face.fs_type = Some(u32::from(font.fs_type()));
    let mut buf = [0u8; 256];
    face.ps_name = Some(match font.postscript_name(&mut buf) {
        Some(n) => hex(&buf[..n.min(buf.len())]),
        None => "-".to_string(),
    });
    let m = font.default_instance().ft_face_metrics();
    face.face_metrics = [
        m.ascender,
        m.descender,
        m.height,
        m.max_advance_width,
        m.max_advance_height,
        m.underline_position,
        m.underline_thickness,
    ]
    .iter()
    .chain(m.bbox.iter())
    .map(|&v| i64::from(v))
    .collect();
    face.sfnt_names = (0..font.sfnt_name_count())
        .filter_map(|i| font.sfnt_name(i))
        .map(|r| {
            (
                u32::from(r.platform_id),
                u32::from(r.encoding_id),
                u32::from(r.language_id),
                u32::from(r.name_id),
                hex(r.string),
            )
        })
        .collect();
    face.v_advances = ft
        .v_advances
        .iter()
        .map(|&(g, _)| {
            (
                g,
                font.default_instance()
                    .ft_v_advance(GlyphId(g))
                    .map_or(-1, i64::from),
            )
        })
        .collect();
    let mut index = vec![0u32; font.glyph_names_len()];
    let names = font.glyph_names(&mut index);
    face.glyph_names = ft
        .glyph_names
        .iter()
        .map(|&(g, _, _)| {
            let name = names.as_ref().and_then(|n| n.ft_glyph_name(GlyphId(g)));
            match name {
                Some(n) => (g, 0, hex(n.split(|&b| b == 0).next().unwrap_or_default())),
                None => (g, -1, "-".to_string()),
            }
        })
        .collect();
    face.name_indices = ft
        .name_indices
        .iter()
        .map(|&(g, _)| {
            let name = names.as_ref().and_then(|n| n.ft_glyph_name(GlyphId(g)));
            let idx = name
                .and_then(|n| names.as_ref()?.ft_glyph_from_name(n))
                .map_or(0, |g| g.0);
            (g, idx)
        })
        .collect();
}

fn charmaps(data: &[u8], num_glyphs: Option<u32>) -> Vec<Charmap> {
    let Ok(c) = Cmap::new(data) else {
        return Vec::new();
    };
    c.records()
        .map(|r| {
            let mut validator_error = false;
            cmap::validate_subtable(data, r.offset, num_glyphs, &mut |d| {
                validator_error |= d.severity == Severity::Error;
            });
            let sub: Result<Subtable<'_>, _> = r.subtable();
            let format = sofel_read::be::u16_at(data, sofel_read::be::to_usize(r.offset))
                .map_or(-1, i64::from);
            Charmap {
                platform: r.platform_id,
                encoding: r.encoding_id,
                format,
                maps: sub
                    .map(|s| s.mappings().map(|(c, g)| (c, g.0)).collect())
                    .unwrap_or_default(),
                readable: sub.is_ok(),
                validator_error,
            }
        })
        .collect()
}
