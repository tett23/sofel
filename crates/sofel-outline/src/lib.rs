//! Glyph outlines from `glyf` and `GLYF`, the same as FreeType's without
//! hinting (ADR 0011).
//!
//! Ported from FreeType 2.15.0 (commit 37515cf56ba3349656049edbed89ffc6f6466f2d):
//! `src/truetype/ttgload.c` (`load_truetype_glyph`,
//! `TT_Process_Simple_Glyph`, `TT_Process_Composite_Component`),
//! `src/truetype/ttpload.c` (`tt_face_get_location`, `tt_face_load_loca`)
//! and `src/base/ftoutln.c` (`FT_Outline_Decompose`,
//! `FT_Outline_Get_CBox`). Copyright (C) 1996-2026 by David Turner,
//! Robert Wilhelm, and Werner Lemberg. Used under the FreeType License
//! (FTL); see THIRD_PARTY_NOTICES.md.
//!
//! Like FreeType, the loaded outline is moved left by phantom point 1,
//! `pp1.x = xMin - leftSideBearing` scaled like the points (ADR 0018).
//! Variable glyf outlines and all four phantom points follow ADR 0027;
//! hinting remains outside this crate.
//!
//! CFF and CFF2 outlines (ADR 0021) are ported from FreeType 2.15.0's
//! `src/psaux/psft.c` (the builder callbacks), `src/psaux/psobjs.c`
//! (`ps_builder_*`) and `src/cff/cffgload.c` (`cff_slot_load`); the
//! charstrings themselves are decoded by `sofel_read::cff::charstring`.
//! The Adobe files carry a patent license besides the FTL; see
//! THIRD_PARTY_NOTICES.md.
//!
//! `sofel-font` is re-exported as [`font`], through which `sofel-color`
//! uses fonts (ADR 0023).
//!
//! Points are in font units ([`Scale::Unscaled`]) or 26.6 fixed point
//! ([`Scale::Scaled`]), with y growing upwards as in the font.
//!
//! # Example
//!
//! Draw the outline of `A` at 16 pixels per em with an [`FnPen`]:
//!
//! ```no_run
//! use sofel_font::Font;
//! use sofel_geom::{Command, FnPen};
//! use sofel_outline::{Scale, Scratch, draw};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let data = std::fs::read("font.ttf")?;
//!     let font = Font::new(&data, 0)?;
//!     let glyph = font.nominal_glyph(u32::from('A')).ok_or("no glyph for A")?;
//!
//!     // Keep the Scratch and reuse it for the next glyphs.
//!     let mut scratch = Scratch::default();
//!     let mut commands: Vec<Command> = Vec::new();
//!     let info = draw(
//!         font.default_instance(),
//!         glyph,
//!         Scale::pixels(&font, 16, 16),
//!         &mut scratch,
//!         &mut FnPen(|c| commands.push(c)),
//!     )?;
//!     println!("{} commands, control box {:?}", commands.len(), info.cbox);
//!     Ok(())
//! }
//! ```
//!
//! [`FnPen`]: sofel_geom::FnPen

#![no_std]
#![warn(missing_docs)]
#![forbid(unsafe_code)]

extern crate alloc;

mod hb_cff;

pub use sofel_font as font;

pub use hb_cff::{HbExtents, cff1_extents, cff2_extents, cff2_extents_at};

use alloc::vec::Vec;
use core::fmt;

use sofel_font::read::cff::charstring::Work;
use sofel_font::read::glyf::{self, Component, Glyph, SCALED_COMPONENT_OFFSET, USE_MY_METRICS};
use sofel_font::read::loca::{self, Loca};
use sofel_font::read::{GlyphId, Tag};
use sofel_font::var::Instance;
use sofel_font::var::gvar::{self, FtScratch};
use sofel_font::{Cff, CffError, Font};
use sofel_geom::{Fixed, Matrix, Pen, Point, div_fix, mul_fix, transform, vector_length};

/// How to scale font units.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scale {
    /// Keep font units (`FT_LOAD_NO_SCALE`).
    Unscaled,
    /// Multiply x and y by these 16.16 factors with `FT_MulFix`, giving
    /// 26.6 coordinates (FreeType's `x_scale` and `y_scale`).
    Scaled {
        /// The horizontal factor, 16.16.
        x: Fixed,
        /// The vertical factor, 16.16.
        y: Fixed,
    },
}

impl Scale {
    /// The scale `FT_Set_Pixel_Sizes` sets: `FT_DivFix(ppem << 6, unitsPerEm)`
    /// for each axis, not rounded (as `FT_LOAD_NO_HINTING` uses it).
    pub fn pixels(font: &Font<'_>, x_ppem: u16, y_ppem: u16) -> Scale {
        let upem = i32::from(font.units_per_em());
        let s = |ppem: u16| Fixed(div_fix(i32::from(ppem).saturating_mul(64), upem));
        Scale::Scaled {
            x: s(x_ppem),
            y: s(y_ppem),
        }
    }
}

/// The smallest box holding every point, off-curve ones included
/// (`FT_Outline_Get_CBox`); all zero for an empty outline. In the unit of
/// the points: font units or 26.6, y up.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CBox {
    /// The smallest x.
    pub x_min: i32,
    /// The smallest y.
    pub y_min: i32,
    /// The largest x.
    pub x_max: i32,
    /// The largest y.
    pub y_max: i32,
}

/// What [`draw`] found besides the commands.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct OutlineInfo {
    /// FreeType's `FT_OUTLINE_OVERLAP`: a simple glyph whose first point
    /// has OVERLAP_SIMPLE, or a first component with OVERLAP_COMPOUND.
    pub overlap: bool,
    /// The control box of the loaded points, after the move by phantom
    /// point 1 and before any offset or multiplier given to
    /// [`Scratch::decompose`].
    pub cbox: CBox,
}

/// Why a glyph's outline could not be loaded or decomposed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutlineError {
    /// The glyph id is not below the glyph count.
    InvalidGlyphIndex,
    /// The font has no glyf and loca (or GLYF and LOCA), or its loca
    /// format is unknown.
    NoGlyphData,
    /// A glyph's data is broken, or the points cannot form contours.
    InvalidOutline,
    /// A composite glyph is broken, recursive, or too deep.
    InvalidComposite,
    /// The font has no outline table this crate reads: an `OTTO` font
    /// without `CFF ` or `CFF2`, which FreeType cannot open (ADR 0021).
    UnsupportedFormat,
    /// The CFF or CFF2 table cannot be opened ([`Cff::new`] fails; FreeType
    /// cannot open the face).
    InvalidCff,
    /// A charstring cannot be decoded, or its Font DICT cannot be read.
    InvalidCharString,
}

impl fmt::Display for OutlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OutlineError::InvalidGlyphIndex => "glyph id out of range",
            OutlineError::NoGlyphData => "no glyf and loca tables",
            OutlineError::InvalidOutline => "invalid outline",
            OutlineError::InvalidComposite => "invalid composite glyph",
            OutlineError::UnsupportedFormat => "no outline table",
            OutlineError::InvalidCff => "invalid CFF table",
            OutlineError::InvalidCharString => "invalid charstring",
        })
    }
}

impl core::error::Error for OutlineError {}

/// The work area of [`draw`] and [`load`]: the points, tags and contour
/// ends of the last loaded glyph, and the composite stack. The caller
/// owns it; keep one and reuse it across glyphs so its buffers are
/// allocated once. Each [`load`] clears it first, and a failed load
/// leaves it empty.
#[derive(Clone, Debug, Default)]
pub struct Scratch {
    points: Vec<Point>,
    /// FreeType's tags: bit 0 on-curve, bit 1 cubic.
    tags: Vec<u8>,
    /// The index of the last point of each contour.
    ends: Vec<usize>,
    components: Vec<Component>,
    /// Glyph ids of the composites being loaded, for cycle detection.
    stack: Vec<u32>,
    /// The work area of the Type 2 charstring decoder (ADR 0021).
    cff: Work,
    /// FreeType's reusable gvar buffers.
    gvar: FtScratch,
    varied: Vec<gvar::Point<i32>>,
    /// FreeType's four phantom points for the current glyph.
    phantoms: [Point; 4],
}

const TAG_ON: u8 = 1;
const TAG_CUBIC: u8 = 2;
/// FreeType fails composites nested deeper than this.
const MAX_RECURSION: usize = 100;

struct Context<'a, 'loc, 'source> {
    font: &'a Font<'a>,
    location: &'loc sofel_font::var::Location<'source>,
    glyf: &'a [u8],
    loca: Loca<'a>,
    num_glyphs: u32,
    extended: bool,
    scale: Scale,
}

impl Context<'_, '_, '_> {
    /// `tt_face_get_location`: the byte range of a glyph.
    fn location(&self, glyph: u32) -> (usize, usize) {
        let len = self.glyf.len();
        let n = self.loca.num_locations();
        let g = usize::try_from(glyph).unwrap_or(usize::MAX);
        let (pos1, pos2) = if g < n {
            let p1 = self.loca.offset(g).unwrap_or(0) as usize;
            let p2 = self
                .loca
                .offset(g.saturating_add(1))
                .map_or(p1, |o| o as usize);
            (p1, p2)
        } else {
            (0, 0)
        };
        if pos1 > len {
            return (0, 0);
        }
        let pos2 = if pos2 > len {
            if g.saturating_add(2) == n {
                len
            } else {
                return (0, 0);
            }
        } else {
            pos2
        };
        if pos2 >= pos1 {
            (pos1, pos2.saturating_sub(pos1))
        } else {
            (pos1, len.saturating_sub(pos1))
        }
    }

    /// Phantom point 1's x for a glyph whose header says `x_min`
    /// (`tt_loader_set_pp`), scaled like the points.
    fn pp1_x(&self, glyph: u32, x_min: i16) -> i32 {
        let x = i32::from(x_min).saturating_sub(i32::from(
            self.font
                .instance(self.location)
                .ft_left_side_bearing(GlyphId(glyph)),
        ));
        if !self.location.ft_default() {
            return x;
        }
        match self.scale {
            Scale::Unscaled => x,
            Scale::Scaled { x: sx, .. } => mul_fix(x, sx.0),
        }
    }

    fn phantoms(&self, glyph: u32, x_min: i16, y_max: i16) -> [Point; 4] {
        let left = self.pp1_x(glyph, x_min);
        if self.location.ft_default() {
            return [
                Point { x: left, y: 0 },
                Point::default(),
                Point::default(),
                Point::default(),
            ];
        }
        let instance = self.font.instance(self.location);
        let right = left.wrapping_add(i32::from(
            instance
                .ft_h_advance_before_gvar(GlyphId(glyph))
                .unwrap_or(0),
        ));
        let top = i32::from(y_max).wrapping_add(i32::from(
            instance
                .ft_top_side_bearing(GlyphId(glyph), y_max)
                .unwrap_or(0),
        ));
        let bottom = top.wrapping_sub(i32::from(
            instance
                .ft_v_advance_before_gvar(GlyphId(glyph))
                .unwrap_or(0),
        ));
        [
            Point { x: left, y: 0 },
            Point { x: right, y: 0 },
            Point { x: 0, y: top },
            Point { x: 0, y: bottom },
        ]
    }

    fn scale_point(&self, x: i32, y: i32) -> Point {
        match self.scale {
            Scale::Unscaled => Point { x, y },
            Scale::Scaled { x: sx, y: sy } => Point {
                x: mul_fix(x, sx.0),
                y: mul_fix(y, sy.0),
            },
        }
    }
}

/// Draws `glyph` into `pen` as FreeType's `FT_Load_Glyph` with
/// `FT_LOAD_NO_HINTING` (and `FT_LOAD_NO_SCALE` for
/// [`Scale::Unscaled`]) followed by `FT_Outline_Decompose` would: [`load`]
/// and then [`Scratch::decompose`] with no offset. On an error, the
/// commands sent so far stay sent.
///
/// This picks the outline format at every call ([`Outlines::new`]); a
/// caller that draws many glyphs of a CFF font keeps an [`Outlines`].
pub fn draw(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scale: Scale,
    scratch: &mut Scratch,
    pen: &mut impl Pen,
) -> Result<OutlineInfo, OutlineError> {
    Outlines::new(instance).draw(glyph, scale, scratch, pen)
}

/// How a font's outlines are read: chosen as FreeType chooses its driver.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::large_enum_variant)]
enum Format<'a> {
    Glyf,
    Cff(Cff<'a>),
    Unsupported(OutlineError),
}

/// The outlines of a font: the caller's value, made once per font
/// (ADR 0021). The format is chosen as FreeType chooses its driver: a
/// font whose sfnt version is `OTTO` is read from `CFF2` or `CFF `
/// ([`Cff::new`]), any other from `glyf` and `loca` (or `GLYF` and
/// `LOCA`). For glyf fonts it costs nothing over [`draw`]; for CFF fonts
/// it keeps the opened table.
#[derive(Clone, Debug)]
pub struct Outlines<'data, 'loc, 'source> {
    font: Font<'data>,
    location: &'loc sofel_font::var::Location<'source>,
    format: Format<'data>,
    extents: HbExtents<'data, 'loc, 'source>,
}

impl<'data, 'loc, 'source> Outlines<'data, 'loc, 'source> {
    /// The outlines of `font`. Also builds HarfBuzz's CFF accelerators
    /// for [`Outlines::glyph_extents`] ([`HbExtents::new`]).
    pub fn new(instance: Instance<'_, 'data, 'loc, 'source>) -> Self {
        let font = instance.font();
        let format = if font.sfnt().version() == u32::from_be_bytes(*b"OTTO") {
            match Cff::new(font) {
                Ok(c) => Format::Cff(c),
                Err(CffError::NotCff) => Format::Unsupported(OutlineError::UnsupportedFormat),
                Err(_) => Format::Unsupported(OutlineError::InvalidCff),
            }
        } else {
            Format::Glyf
        };
        Outlines {
            font: *font,
            location: instance.location(),
            format,
            extents: HbExtents::new(instance),
        }
    }

    /// Reuses the opened CFF and HarfBuzz views at another font location.
    pub fn with_location<'new_loc, 'new_source>(
        &self,
        location: &'new_loc sofel_font::var::Location<'new_source>,
    ) -> Outlines<'data, 'new_loc, 'new_source> {
        Outlines {
            font: self.font,
            location,
            format: self.format,
            extents: self.extents.with_location(location),
        }
    }

    /// HarfBuzz's glyph box of `glyph`, unscaled
    /// (`hb_ot_get_glyph_extents` without color and bitmap glyphs, in the
    /// order VARC (none yet), glyf, CFF2, CFF; ADR 0021). It comes from
    /// HarfBuzz's own interpreter and may differ from the control box of
    /// [`Outlines::draw`], which follows FreeType.
    pub fn glyph_extents(&self, glyph: GlyphId) -> Option<sofel_font::GlyphExtents> {
        self.extents.glyph_extents(glyph)
    }

    /// The HarfBuzz glyph boxes this value holds.
    pub fn hb_extents(&self) -> &HbExtents<'data, 'loc, 'source> {
        &self.extents
    }

    /// The font.
    pub fn font(&self) -> &Font<'data> {
        &self.font
    }

    /// The font and location used by this outline collection.
    pub fn instance(&self) -> Instance<'_, 'data, 'loc, 'source> {
        self.font.instance(self.location)
    }

    /// The opened CFF or CFF2 table, for CFF fonts.
    pub fn cff(&self) -> Option<&Cff<'data>> {
        match &self.format {
            Format::Cff(c) => Some(c),
            _ => None,
        }
    }

    /// Draws `glyph` into `pen` as FreeType's `FT_Load_Glyph` with
    /// `FT_LOAD_NO_HINTING` (and `FT_LOAD_NO_SCALE` for
    /// [`Scale::Unscaled`]) followed by `FT_Outline_Decompose` would:
    /// [`Outlines::load`] and then [`Scratch::decompose`] with no offset.
    pub fn draw(
        &self,
        glyph: GlyphId,
        scale: Scale,
        scratch: &mut Scratch,
        pen: &mut impl Pen,
    ) -> Result<OutlineInfo, OutlineError> {
        let info = self.load(glyph, scale, scratch)?;
        scratch.decompose(Point::default(), 1, pen)?;
        Ok(info)
    }

    /// Loads the points of `glyph` into `scratch` as `FT_Load_Glyph` with
    /// `FT_LOAD_NO_HINTING` would; see [`load`] for glyf fonts. CFF and
    /// CFF2 glyphs follow FreeType's Adobe engine without hinting: points
    /// in whole font units (fractions truncated), the FontMatrix applied,
    /// then the scale; never overlapping.
    pub fn load(
        &self,
        glyph: GlyphId,
        scale: Scale,
        scratch: &mut Scratch,
    ) -> Result<OutlineInfo, OutlineError> {
        match &self.format {
            Format::Glyf => load_glyf(self.instance(), glyph, scale, scratch),
            Format::Cff(c) => load_cff_at(&self.font, self.location, c, glyph, scale, scratch),
            Format::Unsupported(e) => {
                scratch.clear();
                Err(*e)
            }
        }
    }
}

/// Loads the points of `glyph` into `scratch`, as `FT_Load_Glyph` with
/// `FT_LOAD_NO_HINTING` would, without sending any command: the points
/// end up moved left by phantom point 1, `pp1.x`, the scaled difference
/// between the `xMin` of the glyph header and FreeType's left side
/// bearing ([`sofel_font::var::Instance::ft_left_side_bearing`]). A composite glyph takes the
/// phantom point of its last component with USE_MY_METRICS, if any.
/// Use [`Scratch::decompose`] to draw them.
///
/// This picks the outline format at every call; see [`Outlines`].
pub fn load(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scale: Scale,
    scratch: &mut Scratch,
) -> Result<OutlineInfo, OutlineError> {
    Outlines::new(instance).load(glyph, scale, scratch)
}

fn load_glyf(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scale: Scale,
    scratch: &mut Scratch,
) -> Result<OutlineInfo, OutlineError> {
    let font = instance.font();
    let extended = font.table(glyf::TAG_WIDE).is_some();
    let (glyf_tag, loca_tag): (Tag, Tag) = if extended {
        (glyf::TAG_WIDE, loca::TAG_WIDE)
    } else {
        (glyf::TAG, loca::TAG)
    };
    let loca_data = font.table(loca_tag).ok_or(OutlineError::NoGlyphData)?;
    let loca = Loca::new(loca_data, font.head().index_to_loc_format())
        .map_err(|_| OutlineError::NoGlyphData)?;
    let ctx = Context {
        font,
        location: instance.location(),
        glyf: font.table(glyf_tag).unwrap_or(&[]),
        loca,
        num_glyphs: font.num_glyphs(),
        extended,
        scale,
    };
    scratch.points.clear();
    scratch.tags.clear();
    scratch.ends.clear();
    scratch.components.clear();
    scratch.stack.clear();
    if !ctx.location.ft_default() {
        scratch.phantoms = [Point::default(); 4];
    }
    if glyph.0 >= ctx.num_glyphs {
        return Err(OutlineError::InvalidGlyphIndex);
    }
    let mut overlap = false;
    let mut pp1_x = 0;
    let loaded = load_glyph(&ctx, scratch, glyph.0, 0, &mut overlap, &mut pp1_x);
    if let Err(e) = loaded {
        scratch.points.clear();
        scratch.tags.clear();
        scratch.ends.clear();
        scratch.phantoms = [Point::default(); 4];
        return Err(e);
    }
    // `TT_Load_Glyph`: whatever bit 1 of the head flags says.
    if pp1_x != 0 {
        for p in &mut scratch.points {
            p.x = p.x.saturating_sub(pp1_x);
        }
    }
    Ok(OutlineInfo {
        overlap,
        cbox: control_box(&scratch.points),
    })
}

impl Scratch {
    fn clear(&mut self) {
        self.points.clear();
        self.tags.clear();
        self.ends.clear();
        self.components.clear();
        self.stack.clear();
        self.phantoms = [Point::default(); 4];
    }

    /// Sends the outline last loaded by [`load`] to `pen`, as
    /// `FT_Outline_Decompose` would after `FT_Outline_Translate` by
    /// `offset` and multiplying every point by `multiplier`. FreeType's
    /// smooth renderer decomposes the outline after moving it to the
    /// bitmap origin (and, for overlapping outlines, after multiplying it
    /// by 4); the implied on-curve points, `(a + b) / 2` truncated toward
    /// zero, depend on that, so renderers that must match FreeType pass
    /// the same offset and multiplier here.
    pub fn decompose(
        &self,
        offset: Point,
        multiplier: i32,
        pen: &mut impl Pen,
    ) -> Result<(), OutlineError> {
        decompose(
            &self.points,
            &self.tags,
            &self.ends,
            offset,
            multiplier,
            pen,
        )
    }
}

fn control_box(points: &[Point]) -> CBox {
    let Some(first) = points.first() else {
        return CBox::default();
    };
    points.iter().fold(
        CBox {
            x_min: first.x,
            y_min: first.y,
            x_max: first.x,
            y_max: first.y,
        },
        |b, p| CBox {
            x_min: b.x_min.min(p.x),
            y_min: b.y_min.min(p.y),
            x_max: b.x_max.max(p.x),
            y_max: b.y_max.max(p.y),
        },
    )
}

/// `load_truetype_glyph` without hinting; tracks all four phantom points.
fn load_glyph(
    ctx: &Context<'_, '_, '_>,
    s: &mut Scratch,
    glyph: u32,
    depth: usize,
    overlap: &mut bool,
    pp1_x: &mut i32,
) -> Result<(), OutlineError> {
    if depth > MAX_RECURSION {
        return Err(OutlineError::InvalidComposite);
    }
    let (offset, len) = ctx.location(glyph);
    if len == 0 {
        // An empty glyph's box is all zero.
        *pp1_x = ctx.pp1_x(glyph, 0);
        if !ctx.location.ft_default() {
            s.phantoms = ctx.phantoms(glyph, 0, 0);
            vary_empty_phantoms(ctx, s, glyph, pp1_x)?;
        }
        return Ok(());
    }
    let data = ctx
        .glyf
        .get(offset..offset.saturating_add(len))
        .ok_or(OutlineError::InvalidOutline)?;
    let g = Glyph::new(data).map_err(|_| OutlineError::InvalidOutline)?;
    let n = g.number_of_contours();
    if n == 0 {
        *pp1_x = ctx.pp1_x(glyph, 0);
        if !ctx.location.ft_default() {
            s.phantoms = ctx.phantoms(glyph, 0, 0);
            vary_empty_phantoms(ctx, s, glyph, pp1_x)?;
        }
        return Ok(());
    }
    *pp1_x = ctx.pp1_x(glyph, g.x_min());
    if !ctx.location.ft_default() {
        s.phantoms = ctx.phantoms(glyph, g.x_min(), g.y_max());
    }
    if n > 0 {
        load_simple(ctx, s, &g, glyph, pp1_x, overlap)
    } else {
        load_composite(ctx, s, &g, glyph, depth, overlap, pp1_x)
    }
}

fn vary_empty_phantoms(
    ctx: &Context<'_, '_, '_>,
    s: &mut Scratch,
    glyph: u32,
    pp1_x: &mut i32,
) -> Result<(), OutlineError> {
    if ctx.location.ft_default() {
        return Ok(());
    }
    s.varied.clear();
    s.varied.extend(s.phantoms.map(|p| gvar::Point {
        x: p.x.wrapping_shl(16),
        y: p.y.wrapping_shl(16),
        end: false,
    }));
    ctx.font
        .instance(ctx.location)
        .apply_gvar_ft(glyph, &mut s.varied, &mut s.gvar)
        .map_err(|_| OutlineError::InvalidOutline)?;
    let hvar = ctx.font.table(Tag::new(b"HVAR")).is_some();
    let vvar = ctx.font.table(Tag::new(b"VVAR")).is_some();
    for (index, (phantom, varied)) in s.phantoms.iter_mut().zip(&s.varied).enumerate() {
        if (index < 2 && hvar) || (index >= 2 && vvar) {
            continue;
        }
        phantom.x = varied.x.wrapping_add(0x8000) >> 16;
        phantom.y = varied.y.wrapping_add(0x8000) >> 16;
    }
    if let Scale::Scaled { x, y } = ctx.scale {
        for phantom in &mut s.phantoms {
            phantom.x = mul_fix(phantom.x, x.0);
            phantom.y = mul_fix(phantom.y, y.0);
        }
    }
    *pp1_x = s.phantoms.first().map_or(0, |p| p.x);
    Ok(())
}

#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The slice bounds are lengths recorded immediately above from the same vectors.
fn load_simple(
    ctx: &Context<'_, '_, '_>,
    s: &mut Scratch,
    g: &Glyph<'_>,
    glyph: u32,
    pp1_x: &mut i32,
    overlap: &mut bool,
) -> Result<(), OutlineError> {
    let simple = g.simple().map_err(|_| OutlineError::InvalidOutline)?;
    let base = s.points.len();
    let varied = !ctx.location.ft_default();
    for p in simple.points(ctx.extended) {
        s.points.push(if varied {
            Point { x: p.x, y: p.y }
        } else {
            ctx.scale_point(p.x, p.y)
        });
        s.tags
            .push(if p.on_curve { TAG_ON } else { 0 } | if p.cubic { TAG_CUBIC } else { 0 });
    }
    if s.points.len().saturating_sub(base) != simple.num_points() as usize {
        return Err(OutlineError::InvalidOutline);
    }
    s.ends.extend(
        simple
            .end_points()
            .map(|e| base.saturating_add(usize::from(e))),
    );
    if varied {
        let outline_len = s.points.len() - base;
        s.varied.clear();
        for (index, point) in s.points[base..].iter().enumerate() {
            s.varied.push(gvar::Point {
                x: point.x.wrapping_shl(16),
                y: point.y.wrapping_shl(16),
                end: s.ends[0..].contains(&(base + index)),
            });
        }
        s.varied.extend(s.phantoms.map(|p| gvar::Point {
            x: p.x.wrapping_shl(16),
            y: p.y.wrapping_shl(16),
            end: false,
        }));
        ctx.font
            .instance(ctx.location)
            .apply_gvar_ft(glyph, &mut s.varied, &mut s.gvar)
            .map_err(|_| OutlineError::InvalidOutline)?;
        let hvar = ctx.font.table(Tag::new(b"HVAR")).is_some();
        let vvar = ctx.font.table(Tag::new(b"VVAR")).is_some();
        for (index, (phantom, varied)) in s
            .phantoms
            .iter_mut()
            .zip(s.varied.iter().skip(outline_len))
            .enumerate()
        {
            let apply = !((index < 2 && hvar) || (index >= 2 && vvar));
            let original = *phantom;
            let scale_coord = |original: i32, fixed: i32, factor: Fixed| {
                if !apply {
                    return mul_fix(original, factor.0);
                }
                let delta = fixed.wrapping_sub(original.wrapping_shl(16));
                let unrounded = original
                    .wrapping_shl(6)
                    .wrapping_add(delta.wrapping_add(0x200) >> 10);
                (mul_fix(unrounded, factor.0).wrapping_add(32)) >> 6
            };
            if apply {
                phantom.x = varied.x.wrapping_add(0x8000) >> 16;
                phantom.y = varied.y.wrapping_add(0x8000) >> 16;
            }
            if let Scale::Scaled { x, y } = ctx.scale {
                phantom.x = scale_coord(original.x, varied.x, x);
                phantom.y = scale_coord(original.y, varied.y, y);
            }
        }
        *pp1_x = s.phantoms.first().map_or(0, |p| p.x);
        for (point, varied) in s.points[base..].iter_mut().zip(&s.varied[..outline_len]) {
            let scale = |original: i32, fixed: i32, factor: Fixed| {
                let delta = fixed.wrapping_sub(original.wrapping_shl(16));
                let unrounded = original
                    .wrapping_shl(6)
                    .wrapping_add(delta.wrapping_add(0x200) >> 10);
                (mul_fix(unrounded, factor.0).wrapping_add(32)) >> 6
            };
            match ctx.scale {
                Scale::Unscaled => {
                    point.x = varied.x.wrapping_add(0x8000) >> 16;
                    point.y = varied.y.wrapping_add(0x8000) >> 16;
                }
                Scale::Scaled { x, y } => {
                    point.x = scale(point.x, varied.x, x);
                    point.y = scale(point.y, varied.y, y);
                }
            }
        }
    }
    if simple.overlap() {
        *overlap = true;
    }
    Ok(())
}

#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// Component and phantom indexes are bounded by the vectors built in this function.
fn load_composite(
    ctx: &Context<'_, '_, '_>,
    s: &mut Scratch,
    g: &Glyph<'_>,
    glyph: u32,
    depth: usize,
    overlap: &mut bool,
    pp1_x: &mut i32,
) -> Result<(), OutlineError> {
    if s.stack.contains(&glyph) {
        return Err(OutlineError::InvalidComposite);
    }
    // Read all records first, as FreeType does.
    let first_component = s.components.len();
    for c in g.components(ctx.extended) {
        let c = c.map_err(|_| OutlineError::InvalidComposite)?;
        if c.glyph.0 >= ctx.num_glyphs {
            return Err(OutlineError::InvalidComposite);
        }
        s.components.push(c);
    }
    let end_component = s.components.len();
    if depth == 0
        && s.components
            .get(first_component)
            .is_some_and(|c| c.flags & glyf::OVERLAP_COMPOUND != 0)
    {
        *overlap = true;
    }
    if !ctx.location.ft_default() {
        let count = end_component - first_component;
        s.varied.clear();
        for component in &s.components[first_component..end_component] {
            s.varied.push(gvar::Point {
                x: component.arg1.wrapping_shl(16),
                y: component.arg2.wrapping_shl(16),
                end: true,
            });
        }
        s.varied.extend(s.phantoms.map(|p| gvar::Point {
            x: p.x.wrapping_shl(16),
            y: p.y.wrapping_shl(16),
            end: false,
        }));
        ctx.font
            .instance(ctx.location)
            .apply_gvar_ft(glyph, &mut s.varied, &mut s.gvar)
            .map_err(|_| OutlineError::InvalidOutline)?;
        for (component, varied) in s.components[first_component..end_component]
            .iter_mut()
            .zip(&s.varied[..count])
        {
            if component.args_are_xy() {
                component.arg1 = i32::from((varied.x.wrapping_add(0x8000) >> 16) as i16);
                component.arg2 = i32::from((varied.y.wrapping_add(0x8000) >> 16) as i16);
            }
        }
        let hvar = ctx.font.table(Tag::new(b"HVAR")).is_some();
        let vvar = ctx.font.table(Tag::new(b"VVAR")).is_some();
        for (index, (phantom, varied)) in s
            .phantoms
            .iter_mut()
            .zip(s.varied.iter().skip(count))
            .enumerate()
        {
            if (index < 2 && hvar) || (index >= 2 && vvar) {
                continue;
            }
            phantom.x = varied.x.wrapping_add(0x8000) >> 16;
            phantom.y = varied.y.wrapping_add(0x8000) >> 16;
        }
        *pp1_x = s.phantoms.first().map_or(0, |p| p.x);
    }
    s.stack.push(glyph);
    let start_point = s.points.len();
    let mut metrics_component = false;
    for i in first_component..end_component {
        let Some(c) = s.components.get(i).copied() else {
            break;
        };
        let base = s.points.len();
        let saved_pp1 = *pp1_x;
        let saved = s.phantoms;
        load_glyph(ctx, s, c.glyph.0, depth.saturating_add(1), overlap, pp1_x)?;
        // The component's phantom points stay only with USE_MY_METRICS;
        // its offset and transform do not move them.
        if c.flags & USE_MY_METRICS == 0 {
            if ctx.location.ft_default() {
                *pp1_x = saved_pp1;
            } else {
                s.phantoms = saved;
                *pp1_x = s.phantoms.first().map_or(0, |p| p.x);
            }
        } else {
            metrics_component = true;
        }
        if s.points.len() == base {
            continue;
        }
        place_component(ctx, s, &c, start_point, base)?;
    }
    s.stack.pop();
    s.components.truncate(first_component);
    if !ctx.location.ft_default() && !metrics_component {
        if let Scale::Scaled { x, y } = ctx.scale {
            for phantom in &mut s.phantoms {
                phantom.x = mul_fix(phantom.x, x.0);
                phantom.y = mul_fix(phantom.y, y.0);
            }
        }
        *pp1_x = s.phantoms.first().map_or(0, |p| p.x);
    }
    Ok(())
}

/// `TT_Process_Composite_Component` without hinting.
fn place_component(
    ctx: &Context<'_, '_, '_>,
    s: &mut Scratch,
    c: &Component,
    start_point: usize,
    base: usize,
) -> Result<(), OutlineError> {
    let fixed = |v: i16| Fixed(i32::from(v).saturating_mul(4));
    let m = Matrix {
        xx: fixed(c.xx),
        xy: fixed(c.xy),
        yx: fixed(c.yx),
        yy: fixed(c.yy),
    };
    let points = s
        .points
        .get_mut(base..)
        .ok_or(OutlineError::InvalidComposite)?;
    if c.has_scale() {
        for p in points.iter_mut() {
            *p = transform(*p, &m);
        }
    }
    let (x, y) = if !c.args_are_xy() {
        let k = usize::try_from(c.arg1)
            .unwrap_or(usize::MAX)
            .saturating_add(start_point);
        let l = usize::try_from(c.arg2)
            .unwrap_or(usize::MAX)
            .saturating_add(base);
        if k >= base || l >= s.points.len() {
            return Err(OutlineError::InvalidComposite);
        }
        let (Some(p1), Some(p2)) = (s.points.get(k), s.points.get(l)) else {
            return Err(OutlineError::InvalidComposite);
        };
        (p1.x.saturating_sub(p2.x), p1.y.saturating_sub(p2.y))
    } else {
        let (mut x, mut y) = (c.arg1, c.arg2);
        if x == 0 && y == 0 {
            return Ok(());
        }
        if c.has_scale() && c.flags & SCALED_COMPONENT_OFFSET != 0 {
            x = mul_fix(x, vector_length(m.xx.0, m.xy.0));
            y = mul_fix(y, vector_length(m.yy.0, m.yx.0));
        }
        if let Scale::Scaled { x: sx, y: sy } = ctx.scale {
            x = mul_fix(x, sx.0);
            y = mul_fix(y, sy.0);
        }
        (x, y)
    };
    if x != 0 || y != 0 {
        for p in s.points.get_mut(base..).into_iter().flatten() {
            p.x = p.x.saturating_add(x);
            p.y = p.y.saturating_add(y);
        }
    }
    Ok(())
}

fn mid(a: Point, b: Point) -> Point {
    // C's `/ 2` truncates toward zero; so does Rust's on i64.
    let h = |u: i32, v: i32| {
        i32::try_from((i64::from(u).saturating_add(i64::from(v))) / 2).unwrap_or(0)
    };
    Point {
        x: h(a.x, b.x),
        y: h(a.y, b.y),
    }
}

/// `FT_Outline_Decompose` with shift 0 and delta 0, plus `close` at the
/// end of each contour. `point` mirrors FreeType's pointer: the index of
/// the last point consumed, which starts one before `first` when the
/// contour starts off the curve.
fn decompose(
    points: &[Point],
    tags: &[u8],
    ends: &[usize],
    offset: Point,
    multiplier: i32,
    pen: &mut impl Pen,
) -> Result<(), OutlineError> {
    let invalid = OutlineError::InvalidOutline;
    let at = |i: i64| usize::try_from(i).ok();
    let place = |p: Point| Point {
        x: p.x.saturating_add(offset.x).saturating_mul(multiplier),
        y: p.y.saturating_add(offset.y).saturating_mul(multiplier),
    };
    let pt = |i: i64| {
        at(i)
            .and_then(|i| points.get(i))
            .copied()
            .map(place)
            .ok_or(invalid)
    };
    let tg = |i: i64| {
        at(i)
            .and_then(|i| tags.get(i))
            .map(|t| t & 3)
            .ok_or(invalid)
    };
    let mut last: i64 = -1;
    for &end in ends {
        let first = last.saturating_add(1);
        last = i64::try_from(end).map_err(|_| invalid)?;
        if last < first {
            return Err(invalid);
        }
        let mut limit = last;
        let mut v_start = pt(first)?;
        let v_last = pt(last)?;
        let mut point = first;
        let tag = tg(first)?;
        if tag != TAG_ON {
            let last_tag = tg(last)?;
            if last_tag == TAG_ON {
                v_start = v_last;
                limit = limit.saturating_sub(1);
            } else {
                if last_tag != tag {
                    return Err(invalid);
                }
                v_start = mid(v_start, v_last);
            }
            point = point.saturating_sub(1);
        }
        pen.move_to(v_start);
        let mut closed = false;
        'contour: while point < limit {
            point = point.saturating_add(1);
            match tg(point)? {
                TAG_ON => pen.line_to(pt(point)?),
                0 => {
                    let mut v_control = pt(point)?;
                    loop {
                        if point < limit {
                            point = point.saturating_add(1);
                            let vec = pt(point)?;
                            let t = tg(point)?;
                            if t == TAG_ON {
                                pen.quad_to(v_control, vec);
                                continue 'contour;
                            }
                            if t != 0 {
                                return Err(invalid);
                            }
                            pen.quad_to(v_control, mid(v_control, vec));
                            v_control = vec;
                        } else {
                            pen.quad_to(v_control, v_start);
                            closed = true;
                            break 'contour;
                        }
                    }
                }
                _ => {
                    if point.saturating_add(1) > limit || tg(point.saturating_add(1))? != TAG_CUBIC
                    {
                        return Err(invalid);
                    }
                    point = point.saturating_add(2);
                    let vec1 = pt(point.saturating_sub(2))?;
                    let vec2 = pt(point.saturating_sub(1))?;
                    if point <= limit {
                        let t = tg(point)?;
                        let vec = match t {
                            TAG_CUBIC => mid(vec2, pt(point)?),
                            TAG_ON => pt(point)?,
                            _ => return Err(invalid),
                        };
                        pen.cubic_to(vec1, vec2, vec);
                        if t == TAG_CUBIC {
                            point = point.saturating_sub(1);
                        }
                    } else {
                        pen.cubic_to(vec1, vec2, v_start);
                        closed = true;
                        break 'contour;
                    }
                }
            }
        }
        if !closed {
            pen.line_to(v_start);
        }
        pen.close();
    }
    Ok(())
}

mod cff;
use cff::load_cff_at;
