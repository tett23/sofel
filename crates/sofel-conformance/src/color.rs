//! Color glyphs against HarfBuzz (ADR 0023): `hb-dump --paint`,
//! `--color` and `--extents`, and HarfBuzz's `test/api/results-paint/`.
//!
//! Items, identified by `<font>#<face>:<item>`:
//! - `paint:<palette>:<ppem>:<glyph>`: the text of HarfBuzz's
//!   `test/api/test-paint.c` for `hb_font_paint_glyph`.
//! - `color`: every `hb_ot_color_*` query of the face.
//! - `extents:<glyph>`: `hb_font_get_glyph_extents` at the default scale.
//!
//! sofel paints in font units without HarfBuzz's root scale; at
//! HarfBuzz's default scale that scale and the inverse scales HarfBuzz
//! puts around `PaintGlyph` and `PaintColrGlyph` are identities, which
//! [`HbCompat`] writes where HarfBuzz does. A glyph that is not a color
//! glyph gets HarfBuzz's fallback, a `fill glyph` with the foreground.

use std::path::{Path, PathBuf};

use sofel_cli::paint::{PaintText, format_g3};
use sofel_color::{
    ColorFormat, PaintOptions, Painted, Palette, Recording, Scratch, glyph_color_format, layers,
    paint_recording_at, palette_color_name_id, palette_colors, palette_count, palette_flags,
    palette_name_id, svg_document,
};
use sofel_font::Font;
use sofel_font::geom::{Brush, Color, CompositeMode, Image, Painter, Rect, Solid, Transform};
use sofel_outline::Outlines;

use crate::compare::Item;

const SFNT_MAGIC: [[u8; 4]; 5] = [*b"\x00\x01\x00\x00", *b"OTTO", *b"true", *b"typ1", *b"ttcf"];
const COLOR_TAGS: [&[u8; 4]; 5] = [b"COLR", b"SVG ", b"CBDT", b"sbix", b"CPAL"];

/// The fonts with `COLR`, `CPAL`, `SVG `, `CBDT` or `sbix` among
/// HarfBuzz's test fonts (shaping, subsetting, API and fuzzing fonts),
/// sorted.
pub fn color_fonts(testdata: &Path) -> Vec<PathBuf> {
    let hb = testdata.join("harfbuzz/test");
    let mut all = crate::fonts::harfbuzz_fonts(testdata);
    for dir in ["api/fonts", "fuzzing/fonts"] {
        if let Ok(entries) = std::fs::read_dir(hb.join(dir)) {
            all.extend(entries.flatten().map(|e| e.path()));
        }
    }
    let mut out: Vec<PathBuf> = all
        .into_iter()
        .filter(|p| {
            let Ok(b) = std::fs::read(p) else {
                return false;
            };
            b.first_chunk::<4>().is_some_and(|h| SFNT_MAGIC.contains(h))
                && (0..4).any(|face| {
                    Font::new(&b, face).is_ok_and(|f| {
                        COLOR_TAGS
                            .iter()
                            .any(|t| f.table(sofel_font::read::Tag::new(t)).is_some())
                    })
                })
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Writes what HarfBuzz writes around sofel's commands (see the module
/// documentation).
pub struct HbCompat<'t> {
    pub inner: &'t mut PaintText,
    /// The glyph is a `COLR` version 1 glyph.
    pub v1: bool,
    clips: Vec<bool>,
}

impl<'t> HbCompat<'t> {
    pub fn new(inner: &'t mut PaintText, v1: bool) -> HbCompat<'t> {
        HbCompat {
            inner,
            v1,
            clips: Vec::new(),
        }
    }
    fn identity(&mut self) {
        if self.v1 {
            self.inner.push_transform(Transform::IDENTITY);
        }
    }
    fn end(&mut self) {
        if self.v1 {
            self.inner.pop_transform();
        }
    }
}

impl Painter for HbCompat<'_> {
    fn push_transform(&mut self, t: Transform) {
        self.inner.push_transform(t);
    }
    fn pop_transform(&mut self) {
        self.inner.pop_transform();
    }
    fn push_clip_glyph(&mut self, glyph: u32) {
        self.identity();
        self.inner.push_clip_glyph(glyph);
        self.identity();
        self.clips.push(true);
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        self.inner.push_clip_rectangle(r);
        self.clips.push(false);
    }
    fn pop_clip(&mut self) {
        if self.clips.pop() == Some(true) {
            self.end();
            self.inner.pop_clip();
            self.end();
        } else {
            self.inner.pop_clip();
        }
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        self.inner.fill(brush);
    }
    fn fill_glyph(&mut self, glyph: u32, color: &Solid) {
        self.identity();
        self.inner.fill_glyph(glyph, color);
        self.end();
    }
    fn image(&mut self, image: &Image<'_>) -> bool {
        self.inner.image(image)
    }
    fn push_group(&mut self, mode: CompositeMode) {
        self.inner.push_group(mode);
    }
    fn pop_group(&mut self, mode: CompositeMode) {
        self.inner.pop_group(mode);
    }
    fn color_glyph(&mut self, glyph: u32) -> bool {
        self.identity();
        let r = self.inner.color_glyph(glyph);
        self.end();
        r
    }
}

/// The text HarfBuzz's test writes for `hb_font_paint_glyph` of `glyph`
/// with the foreground black, at the default scale, with a synthetic
/// slant of `slant`, as sofel paints it.
pub fn sofel_paint_text(
    font: &Font<'_>,
    glyph: u32,
    palette: u16,
    ppem: u16,
    slant: f32,
    scratch: &mut Scratch,
) -> String {
    sofel_paint_text_at(
        font.default_instance(),
        glyph,
        palette,
        ppem,
        slant,
        scratch,
    )
}

/// Formats the HarfBuzz paint commands at the supplied location.
pub fn sofel_paint_text_at(
    instance: sofel_font::var::Instance<'_, '_, '_, '_>,
    glyph: u32,
    palette: u16,
    ppem: u16,
    slant: f32,
    scratch: &mut Scratch,
) -> String {
    let mut text = PaintText::new(false);
    let foreground = Color::new(0, 0, 0, 255);
    let slanted = Transform {
        xy: slant,
        ..Transform::IDENTITY
    };
    if slant != 0.0 {
        text.push_transform(slanted);
    }
    let options = PaintOptions {
        palette: Palette::Font(palette),
        foreground,
        ppem,
    };
    let mut rec = Recording::new();
    let painted = paint_recording_at(instance, glyph, &options, scratch, &mut rec);
    let v1 = painted == Painted::ColrV1;
    {
        let mut compat = HbCompat::new(&mut text, v1);
        if v1 {
            compat.inner.push_transform(Transform::IDENTITY);
        }
        rec.replay(&mut compat);
        if v1 {
            compat.inner.pop_transform();
        }
    }
    if slant != 0.0 {
        text.pop_transform();
    }
    if painted == Painted::None {
        text.fill_glyph(
            glyph,
            &Solid {
                color: foreground,
                ..Solid::default()
            },
        );
    }
    text.out
}

/// One face of `hb-dump --paint`: `(palette, ppem, glyph, text)`.
pub struct HbPaintFace {
    pub index: u32,
    pub paints: Vec<(u16, u16, u32, String)>,
}

pub fn parse_paint(text: &str) -> Result<Vec<HbPaintFace>, String> {
    let mut faces: Vec<HbPaintFace> = Vec::new();
    let mut lines = text.split_inclusive('\n');
    while let Some(line) = lines.next() {
        let mut w = line.split_whitespace();
        match w.next() {
            Some("face") => faces.push(HbPaintFace {
                index: w.next().and_then(|v| v.parse().ok()).ok_or(line)?,
                paints: Vec::new(),
            }),
            Some("paint") => {
                let n: Vec<u32> = w
                    .map(|v| v.parse().map_err(|_| line.to_string()))
                    .collect::<Result<_, _>>()?;
                let [p, pp, g, count] = n[..] else {
                    return Err(format!("bad line {line}"));
                };
                let body: String = (0..count).filter_map(|_| lines.next()).collect();
                faces
                    .last_mut()
                    .ok_or("paint before face")?
                    .paints
                    .push((p as u16, pp as u16, g, body));
            }
            Some("end") | None => {}
            Some(o) => return Err(format!("unknown line kind {o}")),
        }
    }
    Ok(faces)
}

/// The paint items of one face.
pub fn compare_paint(font_id: &str, font: &Font<'_>, hb: &HbPaintFace) -> Vec<Item> {
    let mut scratch = Scratch::default();
    hb.paints
        .iter()
        .map(|(p, pp, g, text)| {
            compare_paint_record(font_id, font, hb.index, (*p, *pp, *g), text, &mut scratch)
        })
        .collect()
}

/// Compare one HarfBuzz paint body using the same identifier as a buffered face.
pub fn compare_paint_record(
    font_id: &str,
    font: &Font<'_>,
    face: u32,
    (p, pp, g): (u16, u16, u32),
    text: &str,
    scratch: &mut Scratch,
) -> Item {
    let ours = sofel_paint_text(font, g, p, pp, 0.0, scratch);
    let ok = ours == text;
    Item {
        id: format!("{font_id}#{face}:paint:{p}:{pp}:{g}"),
        ok,
        detail: if ok {
            String::new()
        } else {
            format!("HarfBuzz:\n{text}sofel:\n{ours}")
        },
    }
}

/// `hb-dump --color` of one face, as sofel gives it.
pub fn sofel_color_text(font: &Font<'_>, index: u32) -> String {
    use std::fmt::Write;
    let f = sofel_color::color_formats(font);
    let mut s = format!("face {index}\n");
    let b = |x: u8| u8::from(f.has(x));
    use sofel_color::ColorFormats as C;
    let _ = writeln!(
        s,
        "has {} {} {} {} {}",
        b(C::COLR_V0),
        b(C::COLR_V1),
        b(C::CPAL),
        b(C::SVG),
        u8::from(f.has(C::CBDT) || f.has(C::SBIX))
    );
    let palettes = palette_count(font);
    let _ = writeln!(s, "palettes {palettes}");
    for p in 0..palettes {
        let colors: Vec<String> = palette_colors(font, p)
            .map(|c| format!("{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a))
            .collect();
        let colors = if colors.is_empty() {
            "-".to_string()
        } else {
            colors.join(",")
        };
        let _ = writeln!(
            s,
            "palette {p} {} {} {colors}",
            palette_name_id(font, p),
            palette_flags(font, p)
        );
    }
    if palettes > 0 {
        for c in 0..palette_colors(font, 0).count() {
            let _ = writeln!(
                s,
                "color_name {c} {}",
                palette_color_name_id(font, c as u16)
            );
        }
    }
    for g in 0..font.num_glyphs() {
        let has_paint = glyph_color_format(font, g) == Some(ColorFormat::ColrV1);
        let l: Vec<String> = layers(font, g)
            .map(|(lg, i)| format!("{lg}:{}", i.unwrap_or(0xFFFF)))
            .collect();
        let l = if l.is_empty() {
            "-".to_string()
        } else {
            l.join(",")
        };
        let svg = svg_document(font, g);
        let svg_part = svg.map_or("- - -".to_string(), |d| {
            format!("{} {} {}", d.index, d.start_glyph, d.end_glyph)
        });
        let png = sofel_bitmap::sbix_png_data(font, g, 0)
            .or_else(|| sofel_bitmap::cbdt_png_data(font, g, 0))
            .map_or(0, <[u8]>::len);
        let _ = writeln!(
            s,
            "glyph {g} {} {l} {svg_part} {} {png}",
            u8::from(has_paint),
            svg.map_or(0, |d| d.data.len())
        );
    }
    s.push_str("end\n");
    s
}

/// The `color` item of one face of `hb-dump --color` (`text` is that
/// face's part).
pub fn compare_color(font_id: &str, index: u32, font: &Font<'_>, text: &str) -> Item {
    let ours = sofel_color_text(font, index);
    let ok = ours == text;
    Item {
        id: format!("{font_id}#{index}:color"),
        ok,
        detail: if ok {
            String::new()
        } else {
            let d: Vec<String> = text
                .lines()
                .zip(ours.lines())
                .filter(|(a, b)| a != b)
                .take(5)
                .map(|(a, b)| format!("HarfBuzz {a} / sofel {b}"))
                .collect();
            d.join("\n")
        },
    }
}

/// The faces of a dump split at `face` lines: `(index, text)`.
pub fn split_faces(text: &str) -> Vec<(u32, String)> {
    let mut out: Vec<(u32, String)> = Vec::new();
    for line in text.split_inclusive('\n') {
        if let Some(i) = line.strip_prefix("face ") {
            out.push((i.trim().parse().unwrap_or(0), String::new()));
        }
        if let Some(last) = out.last_mut() {
            last.1.push_str(line);
        }
    }
    out
}

/// The `extents:<glyph>` items of one face of `hb-dump --extents`.
pub fn compare_extents(font_id: &str, index: u32, font: &Font<'_>, text: &str) -> Vec<Item> {
    let outlines = Outlines::new(font);
    let mut scratch = Scratch::default();
    text.lines()
        .filter_map(|l| l.strip_prefix("extents "))
        .map(|l| {
            let v: Vec<i64> = l
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            let g = v[0] as u32;
            let ours = sofel_color::glyph_extents(&outlines, g, &mut scratch);
            let ours = ours.map_or(vec![0, 0, 0, 0, 0], |e| {
                vec![
                    1,
                    i64::from(e.x_bearing),
                    i64::from(e.y_bearing),
                    i64::from(e.width),
                    i64::from(e.height),
                ]
            });
            let ok = ours[..] == v[1..];
            Item {
                id: format!("{font_id}#{index}:extents:{g}"),
                ok,
                detail: format!("HarfBuzz {:?}, sofel {:?}", &v[1..], ours),
            }
        })
        .collect()
}

/// `%.3g`, re-exported for tests.
pub fn g3(v: f64) -> String {
    format_g3(v)
}

/// One token list of FreeType's paint tree (`ft-dump --colr`'s
/// `paint_tree`) for the raw paint at `p`.
fn raw_tree(
    font: &Font<'_>,
    p: Option<sofel_color::PaintRef<'_>>,
    depth: u32,
    budget: &mut i32,
    out: &mut String,
) {
    use sofel_color::Paint;
    use std::fmt::Write;
    if depth > 64 || *budget <= 0 {
        out.push_str(" ...");
        return;
    }
    *budget -= 1;
    let Some(p) = p else {
        out.push_str(" error");
        return;
    };
    let Some(paint) = p.paint() else {
        out.push_str(" error");
        return;
    };
    let i = |v: i16| i64::from(v) << 16;
    let f2 = |v: sofel_font::read::F2Dot14| i64::from(v.0) * 4;
    let child = |o: Option<usize>| o.map(|o| p.at(o));
    let stops = |line: Option<sofel_color::RawColorLine<'_>>, out: &mut String| match line {
        None => out.push_str(" error-line"),
        Some(l) => {
            let _ = write!(out, " extend {}", l.extend);
            for s in (0..l.len()).filter_map(|k| l.stop(k)) {
                let _ = write!(
                    out,
                    " stop {} {} {}",
                    f2(s.offset),
                    s.palette_index,
                    s.alpha.0
                );
            }
            out.push_str(" .");
        }
    };
    match paint {
        Paint::ColrLayers {
            num_layers,
            first_layer,
        } => {
            out.push_str(" layers");
            for k in first_layer..first_layer + u32::from(num_layers) {
                let l = sofel_color::layer_paint(font, k as usize);
                raw_tree(font, l, depth + 1, budget, out);
            }
            out.push_str(" .");
        }
        Paint::Solid {
            palette_index,
            alpha,
            ..
        } => {
            let _ = write!(out, " solid {palette_index} {}", alpha.0);
        }
        Paint::LinearGradient {
            color_line,
            points: [a, b, c, d, e, f],
            ..
        } => {
            let _ = write!(
                out,
                " linear {} {} {} {} {} {}",
                i(a),
                i(b),
                i(c),
                i(d),
                i(e),
                i(f)
            );
            stops(color_line, out);
        }
        Paint::RadialGradient {
            color_line,
            c0,
            r0,
            c1,
            r1,
            ..
        } => {
            let _ = write!(
                out,
                " radial {} {} {} {} {} {}",
                i(c0.0),
                i(c0.1),
                i64::from(r0) << 16,
                i(c1.0),
                i(c1.1),
                i64::from(r1) << 16
            );
            stops(color_line, out);
        }
        Paint::SweepGradient {
            color_line,
            center,
            start_angle,
            end_angle,
            ..
        } => {
            let _ = write!(
                out,
                " sweep {} {} {} {}",
                i(center.0),
                i(center.1),
                f2(start_angle),
                f2(end_angle)
            );
            stops(color_line, out);
        }
        Paint::Glyph { paint, glyph } => {
            let _ = write!(out, " glyph {glyph}");
            raw_tree(font, child(paint), depth + 1, budget, out);
        }
        Paint::ColrGlyph { glyph } => {
            let _ = write!(out, " colr_glyph {glyph}");
        }
        Paint::Transform { paint, transform } => match transform {
            None => out.push_str(" error"),
            Some(t) => {
                let _ = write!(
                    out,
                    " transform {} {} {} {} {} {}",
                    t.xx.0, t.yx.0, t.xy.0, t.yy.0, t.dx.0, t.dy.0
                );
                raw_tree(font, child(paint), depth + 1, budget, out);
            }
        },
        Paint::Translate { paint, delta, .. } => {
            let _ = write!(out, " translate {} {}", i(delta.0), i(delta.1));
            raw_tree(font, child(paint), depth + 1, budget, out);
        }
        Paint::Scale {
            paint,
            scale,
            center,
            ..
        } => {
            let (cx, cy) = center.unwrap_or((0, 0));
            let _ = write!(
                out,
                " scale {} {} {} {}",
                f2(scale.0),
                f2(scale.1),
                i(cx),
                i(cy)
            );
            raw_tree(font, child(paint), depth + 1, budget, out);
        }
        Paint::Rotate {
            paint,
            angle,
            center,
            ..
        } => {
            let (cx, cy) = center.unwrap_or((0, 0));
            let _ = write!(out, " rotate {} {} {}", f2(angle), i(cx), i(cy));
            raw_tree(font, child(paint), depth + 1, budget, out);
        }
        Paint::Skew {
            paint,
            angles,
            center,
            ..
        } => {
            let (cx, cy) = center.unwrap_or((0, 0));
            let _ = write!(
                out,
                " skew {} {} {} {}",
                f2(angles.0),
                f2(angles.1),
                i(cx),
                i(cy)
            );
            raw_tree(font, child(paint), depth + 1, budget, out);
        }
        Paint::Composite {
            source,
            mode,
            backdrop,
        } => {
            let _ = write!(out, " composite {mode}");
            raw_tree(font, child(source), depth + 1, budget, out);
            raw_tree(font, child(backdrop), depth + 1, budget, out);
        }
    }
}

/// `ft-dump --colr` of one face as sofel gives it: `(cpal text, glyph
/// lines by glyph)`.
pub fn sofel_colr(font: &Font<'_>) -> (String, std::collections::BTreeMap<u32, String>) {
    use std::fmt::Write;
    let mut cpal = String::new();
    let n = palette_count(font);
    {
        let entries = sofel_color::palette_entry_count(font);
        let _ = writeln!(cpal, "cpal {n} {entries}");
        for p in 0..n {
            let colors: Vec<String> = palette_colors(font, p)
                .map(|c| format!("{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a))
                .collect();
            let colors = if colors.is_empty() {
                "-".into()
            } else {
                colors.join(",")
            };
            let flags = palette_flags(font, p);
            let _ = writeln!(
                cpal,
                "palette {p} {} {flags} {colors}",
                palette_name_id(font, p)
            );
        }
        for c in 0..entries {
            let _ = writeln!(cpal, "color_name {c} {}", palette_color_name_id(font, c));
        }
    }
    let mut glyphs = std::collections::BTreeMap::new();
    for g in 0..font.num_glyphs() {
        let mut s = String::new();
        let l: Vec<String> = layers(font, g)
            .map(|(lg, i)| format!("{lg}:{}", i.unwrap_or(0xFFFF)))
            .collect();
        if !l.is_empty() {
            let _ = writeln!(s, "layers {g} {}", l.join(","));
        }
        if let Some(p) = sofel_color::base_paint(font, g) {
            let _ = write!(s, "paint {g}");
            let mut budget = 100_000;
            raw_tree(font, Some(p), 0, &mut budget, &mut s);
            s.push('\n');
        }
        if let Some(b) = sofel_color::clip_box(font, g) {
            let v = |x: i16| i64::from(x) * 64;
            let _ = writeln!(
                s,
                "clip {g} {} {} {} {}",
                v(b.x_min),
                v(b.y_min),
                v(b.x_max),
                v(b.y_max)
            );
        }
        if !s.is_empty() {
            glyphs.insert(g, s);
        }
    }
    (cpal, glyphs)
}

/// The `cpal` and `colr:<glyph>` items of one face of `ft-dump --colr`.
pub fn compare_colr(font_id: &str, index: u32, font: &Font<'_>, ft: &str) -> Vec<Item> {
    let (cpal, ours) = sofel_colr(font);
    let ft_cpal: String = ft
        .lines()
        .filter(|l| {
            l.starts_with("cpal ") || l.starts_with("palette ") || l.starts_with("color_name ")
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let mut ft_glyphs: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
    for l in ft.lines() {
        let mut w = l.split_whitespace();
        if let (Some("layers" | "paint" | "clip"), Some(g)) = (w.next(), w.next()) {
            let e = ft_glyphs.entry(g.parse().unwrap_or(u32::MAX)).or_default();
            e.push_str(l);
            e.push('\n');
        }
    }
    let mut items = vec![Item {
        id: format!("{font_id}#{index}:cpal"),
        ok: cpal == ft_cpal,
        detail: format!("FreeType:\n{ft_cpal}sofel:\n{cpal}"),
    }];
    let keys: std::collections::BTreeSet<u32> =
        ft_glyphs.keys().chain(ours.keys()).copied().collect();
    for g in keys {
        let (a, b) = (
            ft_glyphs.get(&g).cloned().unwrap_or_default(),
            ours.get(&g).cloned().unwrap_or_default(),
        );
        items.push(Item {
            id: format!("{font_id}#{index}:colr:{g}"),
            ok: a == b,
            detail: format!("FreeType:\n{a}sofel:\n{b}"),
        });
    }
    items
}

/// The `strikes` item of one face of `ft-dump --sbit`: FreeType's
/// `available_sizes` against [`sofel_bitmap::strikes`].
pub fn compare_strikes(font_id: &str, index: u32, font: &Font<'_>, ft: &str) -> Item {
    let theirs: String = ft
        .lines()
        .filter(|l| l.starts_with("strike "))
        .map(|l| format!("{l}\n"))
        .collect();
    let ours: String = sofel_bitmap::strikes(font)
        .enumerate()
        .map(|(i, s)| {
            format!(
                "strike {i} {} {} {} {} {}\n",
                s.height,
                s.width,
                i64::from(s.y_ppem) << 6,
                i64::from(s.x_ppem) << 6,
                i64::from(s.y_ppem) << 6
            )
        })
        .collect();
    Item {
        id: format!("{font_id}#{index}:strikes"),
        ok: ours == theirs,
        detail: format!("FreeType:\n{theirs}sofel:\n{ours}"),
    }
}

/// The foreground FreeType uses for palette entry 0xFFFF after
/// `FT_Palette_Select(palette)` (`tt_face_colr_blend_layer`): white when
/// the palette's flags, as FreeType reads them (16 bits each from the
/// `uint32` paletteTypes array of CPAL version 1), have
/// `FT_PALETTE_FOR_DARK_BACKGROUND`, else black.
pub fn freetype_foreground(font: &Font<'_>, palette: u16) -> Color {
    let flags = font
        .table(sofel_font::read::Tag::new(b"CPAL"))
        .and_then(|d| {
            let be16 = |at: usize| d.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
            if be16(0)? != 1 {
                return None;
            }
            let n = usize::from(be16(4)?);
            let at = 12 + 2 * n;
            let off = d
                .get(at..at + 4)
                .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))?;
            if off == 0 {
                return None;
            }
            be16(off as usize + 2 * usize::from(palette))
        });
    if flags.is_some_and(|f| f & 2 != 0) {
        Color::new(255, 255, 255, 255)
    } else {
        Color::new(0, 0, 0, 255)
    }
}

/// The `colr-raster:<ppem>:<palette>:<glyph>` items of one face of
/// `ft-dump --raster PPEM --color`: `sofel_cli::glyph::render_color`
/// with FreeType's palette and foreground.
pub fn compare_color_rasters(
    font_id: &str,
    index: u32,
    font: &Font<'_>,
    ppem: u16,
    ft: &str,
) -> Vec<Item> {
    use sofel_cli::glyph::{ColorGlyph, color_layers, render_color};
    use std::fmt::Write as _;
    // palette -> glyph -> lines
    let mut theirs: Vec<(u16, std::collections::BTreeMap<u32, String>)> = Vec::new();
    let mut glyph = None;
    for l in ft.lines() {
        if let Some(p) = l.strip_prefix("palette ") {
            theirs.push((p.parse().unwrap_or(0), std::collections::BTreeMap::new()));
            glyph = None;
        } else if l.starts_with("palette_error ") {
            theirs.pop();
        } else if let Some(g) = l.strip_prefix("glyph ") {
            glyph = g.parse::<u32>().ok();
        } else if l != "end"
            && let (Some(g), Some((_, map))) = (glyph, theirs.last_mut())
        {
            let e = map.entry(g).or_default();
            e.push_str(l);
            e.push('\n');
        }
    }
    let outlines = Outlines::new(font);
    let mut scratch = sofel_outline::Scratch::default();
    let mut raster = sofel_raster::Rasterizer::new();
    let mut image = sofel_raster::BgraImage::default();
    let mut items = Vec::new();
    for (palette, ft_glyphs) in theirs {
        let options = PaintOptions {
            palette: Palette::Font(palette),
            foreground: freetype_foreground(font, palette),
            ppem: 0,
        };
        let ours_glyphs = (0..font.num_glyphs()).filter(|&g| !color_layers(font, g).is_empty());
        let keys: std::collections::BTreeSet<u32> =
            ft_glyphs.keys().copied().chain(ours_glyphs).collect();
        for g in keys {
            let mut s = String::new();
            let r = render_color(
                &outlines,
                sofel_font::read::GlyphId(g),
                ppem,
                &options,
                &mut scratch,
                &mut raster,
                &mut image,
            );
            match r {
                Err(_) => s.push_str("error\n"),
                Ok(ColorGlyph::Gray(b)) => {
                    let _ = writeln!(s, "bitmap {} {} {} {} gray", b.left, b.top, b.width, b.rows);
                    if b.width > 0 {
                        for row in b.pixels.chunks(b.width as usize) {
                            row.iter().for_each(|v| {
                                let _ = write!(s, "{v:02x}");
                            });
                            s.push('\n');
                        }
                    }
                }
                Ok(ColorGlyph::Color) => {
                    let _ = writeln!(
                        s,
                        "bitmap {} {} {} {} bgra",
                        image.left, image.top, image.width, image.rows
                    );
                    if image.width > 0 {
                        for row in image.pixels.chunks(image.width as usize * 4) {
                            row.iter().for_each(|v| {
                                let _ = write!(s, "{v:02x}");
                            });
                            s.push('\n');
                        }
                    }
                }
            }
            let ft_text = ft_glyphs.get(&g).map(|t| {
                if t.starts_with("error ") {
                    "error\n".to_string()
                } else {
                    t.clone()
                }
            });
            let ok = ft_text.as_deref() == Some(s.as_str());
            let detail = if ok {
                String::new()
            } else {
                let t = ft_text.unwrap_or_else(|| "(no glyph)\n".into());
                let i = t
                    .lines()
                    .zip(s.lines())
                    .position(|(a, b)| a != b)
                    .unwrap_or(0);
                format!(
                    "FreeType {:?} / sofel {:?}",
                    t.lines().nth(i),
                    s.lines().nth(i)
                )
            };
            items.push(Item {
                id: format!("{font_id}#{index}:colr-raster:{ppem}:{palette}:{g}"),
                ok,
                detail,
            });
        }
    }
    items
}
