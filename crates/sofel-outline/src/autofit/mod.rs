//! FreeType's automatic hinter (the `autofit` module), without HarfBuzz
//! (sofel ADR 0064).
//!
//! Ported from FreeType 2.15.0's `src/autofit/afloader.c` (the glyph
//! loader and the metrics of the hinted slot), `src/autofit/afglobal.c`
//! (the style coverage of each glyph from the Unicode cmap and the lazily
//! computed metrics of each style), `src/autofit/afshaper.c` (the path
//! without HarfBuzz), `src/autofit/afdummy.c` and `src/autofit/afindic.c`.
//! Copyright (C) 2003-2026 by David Turner, Robert Wilhelm, and Werner
//! Lemberg. Used under the FreeType License (FTL); see
//! THIRD_PARTY_NOTICES.md.
//!
//! The writing systems are in `latin` (`aflatin.c`) and `cjk`
//! (`afcjk.c`, also used by the Indic writing system); `hints` holds
//! `afhints.c`. The tables of scripts, styles, blue strings and
//! adjustments are generated from FreeType's sources by
//! tools/autofit-tables.

// Indices into the generated tables and the per-glyph arrays are bounded
// by their lengths; the arithmetic is FreeType's on 64-bit `FT_Pos`.
// The casts are FreeType's: `FT_Short` fields truncate, and point and
// contour indices travel through signed `FT_Int` and `FT_Pos` values.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

mod calc;
mod cjk;
mod hints;
mod latin;
#[rustfmt::skip]
mod tables;

use alloc::vec::Vec;

use sofel_font::read::cmap::{Cmap, Subtable, TAG as CMAP_TAG};
use sofel_font::read::{GlyphId, Tag};
use sofel_geom::Point;

use self::calc::{div_fix, mul_fix, pix_round};
use self::hints::{GlyphHints, HORZ, OutlineRef, SCALER_FLAG_NO_ADVANCE};
use self::tables::{SCRIPTS, STYLE_FALLBACK, STYLE_NONE_DFLT, STYLES};
use super::{Format, HintTarget, OutlineError, OutlineInfo, Outlines, Scale, Scratch};

/// `AF_WritingSystem`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WritingSystem {
    Dummy,
    Latin,
    Cjk,
    Indic,
}

/// `AF_Blue_StringRec`.
#[derive(Clone, Copy, Debug)]
pub(super) struct BlueStringRec {
    pub string: u16,
    pub properties: u16,
}

/// `AF_ScriptClassRec`.
#[derive(Clone, Copy, Debug)]
pub(super) struct ScriptClass {
    pub ranges: &'static [(u32, u32)],
    pub nonbase_ranges: &'static [(u32, u32)],
    pub tag: &'static [u8; 4],
    pub top_to_bottom: bool,
    pub standard: &'static [u8],
}

/// `AF_StyleClassRec`.
#[derive(Clone, Copy, Debug)]
pub(super) struct StyleClass {
    pub writing_system: WritingSystem,
    pub script: u16,
    pub blue_stringset: u16,
    pub coverage: u16,
    pub default_coverage: bool,
}

/// The script, feature and blue-zone strings an OpenType shaper needs to
/// compute one FreeType automatic-hinter style's GSUB coverage.
#[derive(Clone, Debug)]
pub struct AutoHintStyleSpec {
    /// Index into FreeType's ordered style table.
    pub index: u16,
    /// ISO 15924 script tag.
    pub script: [u8; 4],
    /// Unicode ranges used for cmap coverage.
    pub ranges: &'static [(u32, u32)],
    /// Unicode ranges marked as non-base glyphs.
    pub nonbase_ranges: &'static [(u32, u32)],
    /// OpenType feature tag, or `None` for a script's default style.
    pub feature: Option<[u8; 4]>,
    /// UTF-8 blue-zone strings from the pinned FreeType table.
    pub blue_strings: Vec<&'static [u8]>,
}

/// The load target as the scaler's `render_mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RenderMode {
    #[default]
    Normal,
    Light,
    Mono,
    Lcd,
    LcdV,
}

/// `AF_ScalerRec`, with the `x_ppem` of the size it was made for.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Scaler {
    pub x_scale: i64,
    pub y_scale: i64,
    pub x_delta: i64,
    pub y_delta: i64,
    pub render_mode: RenderMode,
    pub flags: u32,
    pub x_ppem: u16,
}

const STYLE_MASK: u16 = 0x1FFF;
const STYLE_UNASSIGNED: u16 = STYLE_MASK;
const DIGIT: u16 = 0x8000;
const NONBASE: u16 = 0x4000;
const HAS_CMAP_ENTRY: u16 = 0x2000;

/// The metrics of one style (`AF_StyleMetrics` and the derived records).
#[derive(Clone, Debug)]
enum StyleMetrics {
    Dummy,
    Latin(latin::LatinMetrics),
    Cjk(cjk::CjkMetrics),
}

/// The automatic hinter's state for one font at one location, which the
/// caller keeps across glyph loads (FreeType's `face->autohint.data`, the
/// `AF_FaceGlobals`).
///
/// It holds the style of each glyph, found from the Unicode cmap when the
/// cache is made, and the unscaled metrics of each style (standard widths
/// and blue zones), computed when the first glyph of the style is
/// loaded. Make it with [`AutoHintCache::new`] from the same
/// [`Outlines`] that [`Outlines::load_auto_hinted`] is called on; at
/// another location or for another font, make a new cache.
#[derive(Clone, Debug)]
pub struct AutoHintCache {
    glyph_styles: Vec<u16>,
    metrics: Vec<Option<StyleMetrics>>,
    italic: bool,
    fixed_width: bool,
    hints: GlyphHints,
}

impl AutoHintCache {
    /// Style requests in FreeType's assignment order for a shaper to
    /// evaluate against the font's GSUB and GPOS tables.
    pub fn style_specs() -> Vec<AutoHintStyleSpec> {
        STYLES
            .iter()
            .enumerate()
            .map(|(index, style)| {
                let mut blue_strings = Vec::new();
                let mut position = usize::from(style.blue_stringset);
                while let Some(blue) = tables::BLUE_STRINGSETS.get(position) {
                    if blue.string == tables::BLUE_STRING_MAX {
                        break;
                    }
                    let bytes = tables::BLUE_STRINGS
                        .get(usize::from(blue.string)..)
                        .unwrap_or_default();
                    let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(0);
                    blue_strings.push(&bytes[..end]);
                    position += 1;
                }
                AutoHintStyleSpec {
                    index: index as u16,
                    script: *SCRIPTS[usize::from(style.script)].tag,
                    ranges: SCRIPTS[usize::from(style.script)].ranges,
                    nonbase_ranges: SCRIPTS[usize::from(style.script)].nonbase_ranges,
                    feature: tables::COVERAGE_TAGS
                        .get(usize::from(style.coverage))
                        .copied(),
                    blue_strings,
                }
            })
            .collect()
    }

    /// The cache for `outlines`: assigns every glyph to a style through
    /// the Unicode cmap, as FreeType's
    /// `af_face_globals_compute_style_coverage` does without HarfBuzz.
    pub fn new(outlines: &Outlines<'_, '_, '_>) -> AutoHintCache {
        let face = Face::new(outlines);
        let font = outlines.font();
        let os2_italic = font.os2().map(|o| o.fs_selection() & (512 | 1) != 0);
        let italic = match (&outlines.format, os2_italic) {
            (_, Some(italic)) => italic,
            (_, None) => font.head().mac_style() & 2 != 0,
        };
        let fixed_width = font
            .table(Tag::new(b"post"))
            .and_then(|data| sofel_font::read::post::Post::new(data).ok())
            .is_some_and(|post| post.is_fixed_pitch() != 0);
        AutoHintCache {
            glyph_styles: compute_style_coverage(&face),
            metrics: alloc::vec![None; STYLES.len()],
            italic,
            fixed_width,
            hints: GlyphHints::default(),
        }
    }

    /// The raw style word of `glyph` as FreeType's `glyph-to-script-map`
    /// property gives it: the low 13 bits hold the style index, then the
    /// cmap, non-base and digit flags. Styles whose metrics have failed
    /// are already moved to the dummy style.
    pub fn glyph_style(&self, glyph: GlyphId) -> Option<u16> {
        self.glyph_styles
            .get(usize::try_from(glyph.0).ok()?)
            .copied()
    }

    /// The cmap-based style words for a shaper to extend with GSUB coverage.
    pub fn glyph_styles(&self) -> &[u16] {
        &self.glyph_styles
    }
}

/// A font as the automatic hinter queries it (`FT_Face` with its Unicode
/// charmap selected).
struct Face<'o, 'd, 'l, 's> {
    outlines: &'o Outlines<'d, 'l, 's>,
    cmap: Option<Subtable<'d>>,
    num_glyphs: u32,
    units_per_em: i64,
}

/// An outline loaded with `FT_LOAD_NO_SCALE`.
struct Unscaled {
    x: Vec<i64>,
    y: Vec<i64>,
    tags: Vec<u8>,
    ends: Vec<usize>,
}

impl Unscaled {
    fn from_scratch(scratch: &Scratch) -> Unscaled {
        Unscaled {
            x: scratch.points.iter().map(|p| i64::from(p.x)).collect(),
            y: scratch.points.iter().map(|p| i64::from(p.y)).collect(),
            tags: scratch.tags.clone(),
            ends: scratch.ends.clone(),
        }
    }

    fn as_ref(&self) -> OutlineRef<'_> {
        OutlineRef {
            x: &self.x,
            y: &self.y,
            tags: &self.tags,
            ends: &self.ends,
        }
    }

    /// `FT_Outline_Get_Orientation(outline) == FT_ORIENTATION_POSTSCRIPT`.
    fn postscript(&self) -> bool {
        orientation_is_postscript(&self.x, &self.y, &self.ends)
    }
}

fn orientation_is_postscript(x: &[i64], y: &[i64], ends: &[usize]) -> bool {
    let mut area: i64 = 0;
    let mut first = 0;
    for &last in ends {
        if last >= x.len() || first > last {
            return false;
        }
        let (mut prev_x, mut prev_y) = (x[last], y[last]);
        for n in first..=last {
            area = area.wrapping_add((y[n] - prev_y).wrapping_mul(x[n] + prev_x));
            prev_x = x[n];
            prev_y = y[n];
        }
        first = last + 1;
    }
    area > 0
}

/// FreeType's `find_unicode_charmap`: the last valid UCS-4 subtable, else
/// the last valid Unicode subtable of another format than 14 and 15.
fn unicode_subtable<'d>(font: &sofel_font::Font<'d>) -> Option<Subtable<'d>> {
    let cmap = Cmap::new(font.table(CMAP_TAG)?).ok()?;
    let records: Vec<_> = cmap
        .records()
        .filter_map(|r| r.subtable().ok().map(|s| (r.platform_id, r.encoding_id, s)))
        .collect();
    let unicode = |p: u16, e: u16| p == 0 || p == 2 || (p == 3 && matches!(e, 1 | 10));
    records
        .iter()
        .rev()
        .find(|(p, e, s)| {
            (*p == 3 && *e == 10)
                || (*p == 0 && *e == 4)
                || (*p == 0 && *e == 6 && s.format() == 13)
        })
        .or_else(|| {
            records
                .iter()
                .rev()
                .find(|(p, e, s)| unicode(*p, *e) && !matches!(s.format(), 14 | 15))
        })
        .map(|(_, _, s)| *s)
}

impl<'o, 'd, 'l, 's> Face<'o, 'd, 'l, 's> {
    fn new(outlines: &'o Outlines<'d, 'l, 's>) -> Self {
        let font = outlines.font();
        Face {
            outlines,
            cmap: unicode_subtable(font),
            num_glyphs: font.num_glyphs(),
            units_per_em: i64::from(font.units_per_em()),
        }
    }

    /// Whether `FT_Select_Charmap(face, FT_ENCODING_UNICODE)` succeeds.
    fn has_unicode(&self) -> bool {
        self.cmap.is_some()
    }

    /// `FT_Get_Char_Index` on the Unicode charmap.
    fn char_index(&self, c: u32) -> u32 {
        self.cmap
            .and_then(|s| s.glyph(c))
            .map_or(0, |g| if g.0 < self.num_glyphs { g.0 } else { 0 })
    }

    /// `FT_Load_Glyph` with `FT_LOAD_NO_SCALE`; `None` on an error.
    fn load_unscaled(&self, glyph: u32) -> Option<Unscaled> {
        let mut scratch = Scratch::default();
        self.outlines
            .load(GlyphId(glyph), Scale::Unscaled, &mut scratch)
            .ok()?;
        Some(Unscaled::from_scratch(&scratch))
    }

    /// `FT_Get_Advance` with `FT_LOAD_NO_SCALE | FT_LOAD_NO_HINTING`.
    fn get_advance(&self, glyph: u32) -> i64 {
        self.outlines
            .instance()
            .ft_h_advance(GlyphId(glyph))
            .map_or(0, i64::from)
    }
}

/// `GET_UTF8_CHAR`.
fn utf8_char(p: &[u8]) -> (u32, &[u8]) {
    let Some((&b, mut rest)) = p.split_first() else {
        return (0, p);
    };
    let mut ch = u32::from(b);
    if ch >= 0x80 {
        let len = if ch < 0xE0 {
            ch &= 0x1F;
            1
        } else if ch < 0xF0 {
            ch &= 0x0F;
            2
        } else {
            ch &= 0x07;
            3
        };
        for _ in 0..len {
            let Some((&c, r)) = rest.split_first() else {
                break;
            };
            ch = (ch << 6) | u32::from(c & 0x3F);
            rest = r;
        }
    }
    (ch, rest)
}

/// `af_shaper_get_cluster` and `af_shaper_get_elem` without HarfBuzz:
/// skips spaces, reads one cluster up to the next space, and gives the
/// rest, the number of glyphs (0 for a cluster of several characters) and
/// the glyph of a single character.
fn next_cluster<'p>(face: &Face<'_, '_, '_, '_>, mut p: &'p [u8]) -> (&'p [u8], u32, u32) {
    while p.first() == Some(&b' ') {
        p = &p[1..];
    }
    if p.is_empty() {
        return (p, 0, 0);
    }
    let (ch, mut rest) = utf8_char(p);
    let mut dummy = 0;
    while !rest.is_empty() && rest[0] != b' ' {
        let (c, r) = utf8_char(rest);
        dummy = c;
        rest = r;
    }
    if dummy != 0 {
        (rest, 0, 0)
    } else {
        (rest, 1, face.char_index(ch))
    }
}

/// `af_face_globals_compute_style_coverage` without HarfBuzz.
fn compute_style_coverage(face: &Face<'_, '_, '_, '_>) -> Vec<u16> {
    let n = usize::try_from(face.num_glyphs).unwrap_or(0);
    let mut gstyles = alloc::vec![STYLE_UNASSIGNED; n];
    if let Some(cmap) = face.cmap {
        let mappings: Vec<(u32, usize)> = cmap
            .mappings()
            .filter_map(|(c, g)| {
                let g = usize::try_from(g.0).ok()?;
                (g != 0 && g < n).then_some((c, g))
            })
            .collect();
        let in_range = |first: u32, last: u32| {
            let start = mappings.partition_point(|&(c, _)| c < first);
            mappings[start..]
                .iter()
                .take_while(move |&&(c, _)| c <= last)
        };
        for (ss, style) in STYLES.iter().enumerate() {
            if !style.default_coverage {
                continue;
            }
            let ss = ss as u16;
            let script = &SCRIPTS[usize::from(style.script)];
            for &(first, last) in script.ranges {
                for &(_, g) in in_range(first, last) {
                    if gstyles[g] & STYLE_MASK == STYLE_UNASSIGNED {
                        gstyles[g] = ss | HAS_CMAP_ENTRY;
                    }
                }
            }
            for &(first, last) in script.nonbase_ranges {
                for &(_, g) in in_range(first, last) {
                    if gstyles[g] & STYLE_MASK == ss {
                        gstyles[g] |= NONBASE;
                    }
                }
            }
        }
        for c in 0x30..=0x39 {
            let g = face.char_index(c) as usize;
            if g != 0 && g < n {
                gstyles[g] |= DIGIT;
            }
        }
    }
    for g in &mut gstyles {
        if *g & STYLE_MASK == STYLE_UNASSIGNED {
            *g &= !STYLE_MASK;
            *g |= STYLE_FALLBACK;
        }
    }
    gstyles
}

/// `af_face_globals_get_metrics`: the style of `glyph` and its unscaled
/// metrics, computed on first use.
fn style_metrics<'c>(
    cache: &'c mut AutoHintCache,
    face: &Face<'_, '_, '_, '_>,
    glyph: usize,
) -> (usize, &'c StyleMetrics) {
    let mut style = usize::from(cache.glyph_styles[glyph] & STYLE_UNASSIGNED);
    loop {
        if cache.metrics[style].is_some() {
            break;
        }
        let metrics = match STYLES[style].writing_system {
            WritingSystem::Dummy => Some(StyleMetrics::Dummy),
            WritingSystem::Latin => latin::metrics_init(face, style).map(StyleMetrics::Latin),
            WritingSystem::Cjk => Some(StyleMetrics::Cjk(cjk::metrics_init(face, style, true))),
            WritingSystem::Indic => Some(StyleMetrics::Cjk(cjk::metrics_init(face, style, false))),
        };
        match metrics {
            Some(m) => {
                cache.metrics[style] = Some(m);
                break;
            }
            None => {
                // FreeType's internal error -1: no blue zones; the style's
                // glyphs move to the dummy style.
                for g in &mut cache.glyph_styles {
                    if usize::from(*g & STYLE_MASK) == style {
                        *g = STYLE_NONE_DFLT;
                    }
                }
                style = usize::from(cache.glyph_styles[glyph] & STYLE_UNASSIGNED);
            }
        }
    }
    let metrics = cache.metrics[style].as_ref();
    (style, metrics.unwrap_or(&StyleMetrics::Dummy))
}

fn render_mode(target: HintTarget) -> RenderMode {
    match target {
        HintTarget::Normal => RenderMode::Normal,
        HintTarget::Light => RenderMode::Light,
        HintTarget::Mono => RenderMode::Mono,
        HintTarget::Lcd => RenderMode::Lcd,
        HintTarget::VerticalLcd => RenderMode::LcdV,
    }
}

/// The horizontal advance of `FT_Load_Glyph` with `FT_LOAD_NO_SCALE`
/// (`slot->metrics.horiAdvance`): a composite glyf glyph takes the
/// phantom points of its last component with USE_MY_METRICS.
fn unscaled_advance(outlines: &Outlines<'_, '_, '_>, glyph: GlyphId) -> i64 {
    let instance = outlines.instance();
    let source = if matches!(outlines.format, Format::Glyf) && instance.location().ft_default() {
        instance.ft_metrics_glyph(glyph).unwrap_or(glyph)
    } else {
        glyph
    };
    instance.ft_h_advance(source).map_or(0, i64::from)
}

/// `af_loader_load_glyph` at `ppem` with the `target` load mode, for the
/// glyf and CFF formats of [`Outlines`].
pub(super) fn load(
    outlines: &Outlines<'_, '_, '_>,
    glyph: GlyphId,
    ppem: u16,
    target: HintTarget,
    cache: &mut AutoHintCache,
    scratch: &mut Scratch,
) -> Result<(OutlineInfo, i32), OutlineError> {
    let fail = |scratch: &mut Scratch, e: OutlineError| {
        scratch.clear();
        Err(e)
    };
    let font = outlines.font();
    let Ok(index) = usize::try_from(glyph.0) else {
        return fail(scratch, OutlineError::InvalidGlyphIndex);
    };
    if index >= cache.glyph_styles.len() || glyph.0 >= font.num_glyphs() {
        return fail(scratch, OutlineError::InvalidGlyphIndex);
    }
    if !matches!(outlines.format, Format::Glyf | Format::Cff(_)) {
        // The outline loader gives the format's error.
        outlines.load(glyph, Scale::Unscaled, scratch)?;
        return fail(scratch, OutlineError::AutoHintUnsupported);
    }
    // FreeType never auto-hints a glyph that VARC covers.
    if font
        .table(Tag::new(b"VARC"))
        .and_then(|data| sofel_font::read::varc::Varc::new(data).ok())
        .is_some_and(|varc| varc.coverage().get(glyph).is_some())
    {
        return fail(scratch, OutlineError::AutoHintUnsupported);
    }
    let face = Face::new(outlines);
    let upem = face.units_per_em;
    let scale = div_fix(i64::from(ppem) << 6, upem);
    let scaler = Scaler {
        x_scale: scale,
        y_scale: scale,
        x_delta: 0,
        y_delta: 0,
        render_mode: render_mode(target),
        flags: 0,
        x_ppem: ppem,
    };
    let italic = cache.italic;
    let fixed_width = cache.fixed_width;
    let (style, metrics) = style_metrics(cache, &face, index);
    let mut metrics = metrics.clone();
    let style_class = STYLES[style];
    let top_to_bottom = SCRIPTS[usize::from(style_class.script)].top_to_bottom;
    let glyph_style = cache.glyph_styles[index];
    let mut hints = core::mem::take(&mut cache.hints);
    hints.units_per_em = upem;

    let (digits_have_same_width, metrics_scaler) = match &mut metrics {
        StyleMetrics::Dummy => {
            hints.scaler_flags = scaler.flags;
            hints.other_flags = 0;
            hints.x_scale = scaler.x_scale;
            hints.y_scale = scaler.y_scale;
            hints.x_delta = scaler.x_delta;
            hints.y_delta = scaler.y_delta;
            (false, scaler)
        }
        StyleMetrics::Latin(m) => {
            latin::metrics_scale(m, &scaler);
            latin::hints_init(&mut hints, m, italic);
            (m.digits_have_same_width, m.scaler)
        }
        StyleMetrics::Cjk(m) => {
            cjk::metrics_scale(m, &scaler);
            cjk::hints_init(&mut hints, m);
            (m.digits_have_same_width, m.scaler)
        }
    };

    let unscaled = match outlines.load(glyph, Scale::Unscaled, scratch) {
        Ok(info) => info,
        Err(e) => {
            cache.hints = hints;
            return Err(e);
        }
    };
    let hori_advance = unscaled_advance(outlines, glyph);

    let mut pp1x = hints.x_delta;
    let mut pp2x = mul_fix(hori_advance, hints.x_scale) + hints.x_delta;
    let mut x: Vec<i64> = scratch.points.iter().map(|p| i64::from(p.x)).collect();
    let mut y: Vec<i64> = scratch.points.iter().map(|p| i64::from(p.y)).collect();
    if !x.is_empty() {
        let postscript = orientation_is_postscript(&x, &y, &scratch.ends);
        let outline = OutlineRef {
            x: &x,
            y: &y,
            tags: &scratch.tags,
            ends: &scratch.ends,
        };
        match &metrics {
            StyleMetrics::Dummy => hints.reload(&outline, postscript),
            StyleMetrics::Latin(m) => latin::hints_apply(
                glyph.0,
                glyph_style & NONBASE != 0,
                &mut hints,
                &outline,
                postscript,
                m,
                top_to_bottom,
            ),
            StyleMetrics::Cjk(m) => cjk::hints_apply(&mut hints, &outline, postscript, m),
        }
        hints.save(&mut x, &mut y, &mut scratch.tags);

        if scaler.render_mode != RenderMode::Light {
            let axis = &hints.axis[HORZ];
            if axis.edges.len() > 1 && hints.scaler_flags & SCALER_FLAG_NO_ADVANCE == 0 {
                let edge1 = axis.edges[0];
                let edge2 = axis.edges[axis.edges.len() - 1];
                let old_rsb = pp2x - edge2.opos;
                let old_lsb = edge1.opos;
                let new_lsb = edge1.pos;
                let mut pp1x_uh = new_lsb - old_lsb;
                let mut pp2x_uh = edge2.pos + old_rsb;
                if old_lsb < 24 {
                    pp1x_uh -= 8;
                }
                if old_rsb < 24 {
                    pp2x_uh += 8;
                }
                pp1x = pix_round(pp1x_uh);
                pp2x = pix_round(pp2x_uh);
                if pp1x >= new_lsb && old_lsb > 0 {
                    pp1x -= 64;
                }
                if pp2x <= edge2.pos && old_rsb > 0 {
                    pp2x += 64;
                }
            } else {
                pp1x = pix_round(pp1x);
                pp2x = pix_round(pp2x);
            }
        } else {
            pp1x = pix_round(pp1x);
            pp2x = pix_round(pp2x);
        }
    }
    cache.hints = hints;

    if pp1x != 0 {
        for v in &mut x {
            *v -= pp1x;
        }
    }
    for (point, (&px, &py)) in scratch.points.iter_mut().zip(x.iter().zip(&y)) {
        *point = Point {
            x: px as i32,
            y: py as i32,
        };
    }
    let mut advance = hori_advance;
    if scaler.render_mode != RenderMode::Light
        && (fixed_width || (glyph_style & DIGIT != 0 && digits_have_same_width))
    {
        advance = mul_fix(advance, metrics_scaler.x_scale);
    } else if advance != 0 {
        advance = pp2x - pp1x;
    }
    let advance = pix_round(advance);
    Ok((
        OutlineInfo {
            overlap: unscaled.overlap,
            cbox: super::control_box(&scratch.points),
        },
        advance as i32,
    ))
}
