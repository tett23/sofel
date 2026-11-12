//! Glyph boxes as HarfBuzz gives them (ADR 0021, "HarfBuzzの箱"):
//! `hb_ot_get_glyph_extents` without the color and bitmap formats, that
//! is VARC (not yet), glyf, CFF2, then CFF, with HarfBuzz's own Type 2
//! charstring interpreter for the last two.
//!
//! HarfBuzz's interpreter is not FreeType's: its operand stack holds 513
//! numbers, subroutines nest 10 deep, a glyph runs at most 200000
//! operators (`HB_CFF_MAX_OPS`), the arithmetic operators are unknown
//! operators that clear the stack, and CFF2's `blend` keeps the default
//! values when the font has no variation coordinates. The box covers
//! every point of every path that draws, curve control points included;
//! a path of moves only is not in it. HarfBuzz keeps numbers as
//! `double`; here variable blends use 16.16 units in `f64`. When all
//! region scalars are zero, the same interpreter uses exact 16.16 `i64`
//! arithmetic. The edges
//! are rounded as HarfBuzz 15.1.0 rounds them, `floor (x + 0.5)` on the
//! `double` (its `roundf` macro, `_hb_roundf (double)`).
//!
//! Ported from HarfBuzz 15.1.0: `src/hb-cff-interp-common.hh`,
//! `src/hb-cff-interp-cs-common.hh`, `src/hb-cff1-interp-cs.hh`,
//! `src/hb-cff2-interp-cs.hh`, `src/hb-ot-cff1-table.cc` (`bounds_t`,
//! `cff1_path_procs_extents_t`, `cff1_cs_opset_extents_t`,
//! `_get_bounds`, `get_extents`), `src/hb-ot-cff2-table.cc`
//! (`cff2_extents_param_t`, `cff2_path_procs_extents_t`,
//! `get_extents_at`) and `src/hb-ot-font.cc` (`hb_ot_get_glyph_extents`).
//! Copyright © 2018 Adobe Inc.; © 2011, 2014 Google, Inc. Used under
//! HarfBuzz's "Old MIT" license; see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;
use sofel_font::read::GlyphId;
use sofel_font::read::cff::hb::HbIndex;
use sofel_font::read::cff::subr_bias;
use sofel_font::var::{Instance, Location};
use sofel_font::{Font, GlyphExtents, HbCff1, HbCff2, HbScale};
use sofel_geom::PointF;
use sofel_geom::float::hb_roundf;

use crate::HbOutlineCommand;

/// `HB_CFF_MAX_OPS`.
const MAX_OPS: u32 = 200_000;
/// The operand stack (`arg_stack_t`).
const STACK: usize = 513;
/// `kMaxCallLimit`.
const MAX_CALLS: usize = 10;
/// The 16.16 scale of charstring operands.
const SCALE: f64 = 65_536.0;
const ONE: i64 = 0x1_0000;

/// Arithmetic used by the shared Type 2 interpreter. The integer case is
/// selected only when every CFF2 region contributes zero at this location.
trait CsNumber: Copy + Default + PartialOrd {
    fn zero() -> Self;
    fn from_integer(value: i64) -> Self;
    fn from_fixed(value: i32) -> Self;
    fn add(self, other: Self) -> Self;
    fn abs(self) -> Self;
    fn to_int(self) -> i64;
    fn round(self) -> i64;
    fn to_real(self) -> f32;
    fn blend(self, scalar: f32, delta: Self) -> Self;
}

impl CsNumber for i64 {
    fn zero() -> Self {
        0
    }
    fn from_integer(value: i64) -> Self {
        value.saturating_mul(ONE)
    }
    fn from_fixed(value: i32) -> Self {
        i64::from(value)
    }
    fn add(self, other: Self) -> Self {
        self.saturating_add(other)
    }
    fn abs(self) -> Self {
        self.saturating_abs()
    }
    fn to_int(self) -> i64 {
        self.checked_div(ONE).unwrap_or(0)
    }
    fn round(self) -> i64 {
        self.saturating_add(ONE / 2) >> 16
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn to_real(self) -> f32 {
        (self as f64 / SCALE) as f32
    }
    fn blend(self, _: f32, _: Self) -> Self {
        self
    }
}

impl CsNumber for f64 {
    fn zero() -> Self {
        0.0
    }
    fn from_integer(value: i64) -> Self {
        value as f64 * SCALE
    }
    fn from_fixed(value: i32) -> Self {
        f64::from(value)
    }
    fn add(self, other: Self) -> Self {
        self + other
    }
    fn abs(self) -> Self {
        self.abs()
    }
    #[allow(clippy::cast_possible_truncation)]
    fn to_int(self) -> i64 {
        (self / SCALE) as i64
    }
    #[allow(clippy::cast_possible_truncation)]
    fn round(self) -> i64 {
        hb_roundf(self / SCALE) as i64
    }
    #[allow(clippy::cast_possible_truncation)]
    fn to_real(self) -> f32 {
        (self / SCALE) as f32
    }
    fn blend(self, scalar: f32, delta: Self) -> Self {
        self + f64::from(scalar) * delta
    }
}

/// HarfBuzz's glyph boxes of one font: the CFF and CFF2 accelerators,
/// built once with [`HbExtents::new`] and kept by the caller (ADR 0003).
#[derive(Clone, Debug)]
pub struct HbExtents<'data, 'loc, 'source> {
    font: Font<'data>,
    location: &'loc sofel_font::var::Location<'source>,
    cff2: Option<HbCff2<'data>>,
    cff2_scalars: Option<Vec<f32>>,
    cff1: Option<HbCff1<'data>>,
}

fn cff2_scalars_at(
    cff2: Option<&HbCff2<'_>>,
    instance: Instance<'_, '_, '_, '_>,
) -> Option<Vec<f32>> {
    instance.font().var_axes().next()?;
    cff2.and_then(|cff| {
        cff.variation_store()
            .map(|store| instance.cff2_hb_region_scalars(&store).into_owned())
    })
}

impl<'data, 'loc, 'source> HbExtents<'data, 'loc, 'source> {
    /// The glyph boxes of `font`. Builds HarfBuzz's CFF2 and CFF
    /// accelerators ([`HbCff2::new`], [`HbCff1::new`]), which takes time
    /// in proportion to their charsets and Font DICTs.
    pub fn new(
        instance: sofel_font::var::Instance<'_, 'data, 'loc, 'source>,
    ) -> HbExtents<'data, 'loc, 'source> {
        let font = instance.font();
        let cff2 = HbCff2::new(font);
        let cff2_scalars = cff2_scalars_at(cff2.as_ref(), instance);
        HbExtents {
            font: *font,
            location: instance.location(),
            cff2,
            cff2_scalars,
            cff1: HbCff1::new(font),
        }
    }

    /// Reuses the CFF accelerators for another location of the same font.
    pub fn with_location<'new_loc, 'new_source>(
        &self,
        location: &'new_loc Location<'new_source>,
    ) -> HbExtents<'data, 'new_loc, 'new_source> {
        HbExtents {
            font: self.font,
            location,
            cff2: self.cff2,
            cff2_scalars: cff2_scalars_at(self.cff2.as_ref(), self.font.instance(location)),
            cff1: self.cff1,
        }
    }

    /// The font.
    pub fn font(&self) -> &Font<'data> {
        &self.font
    }

    /// HarfBuzz's CFF2 accelerator, if valid.
    pub fn cff2(&self) -> Option<&HbCff2<'data>> {
        self.cff2.as_ref()
    }

    /// HarfBuzz's CFF accelerator, if valid.
    pub fn cff1(&self) -> Option<&HbCff1<'data>> {
        self.cff1.as_ref()
    }

    /// The box of `glyph` in font units, as HarfBuzz's
    /// `hb_font_get_glyph_extents` gives it at the default scale for a
    /// font without color or bitmap glyphs: from glyf
    /// ([`sofel_font::var::Instance::glyf_extents`]), else from CFF2, else from CFF. `None`
    /// when none of them gives one (a charstring HarfBuzz's interpreter
    /// rejects, a glyph id past them).
    pub fn glyph_extents(&self, glyph: GlyphId) -> Option<GlyphExtents> {
        self.scaled_glyph_extents(glyph, &HbScale::unscaled(&self.font))
    }

    /// [`HbExtents::glyph_extents`] at `scale` (HarfBuzz's
    /// `scale_glyph_extents` of the unscaled box).
    pub fn scaled_glyph_extents(&self, glyph: GlyphId, scale: &HbScale) -> Option<GlyphExtents> {
        let instance = self.font.instance(self.location);
        if let Some(e) = instance
            .glyf_extents(glyph)
            .map(|e| scale.scale_glyph_extents(e))
        {
            return Some(e);
        }
        self.cff2
            .as_ref()
            .and_then(|c| cff2_extents_with_scalars(c, glyph.0, self.cff2_scalars.as_deref()))
            .or_else(|| self.cff1.as_ref().and_then(|c| cff1_extents(c, glyph.0)))
            .map(|e| scale.scale_glyph_extents(e))
    }

    /// Draw a CFF or CFF2 glyph as HarfBuzz's `get_path` does, before
    /// synthetic transformations. Glyf paths use [`crate::Scratch::hb_decompose`].
    pub fn cff_glyph_path(&self, glyph: GlyphId, scale: &HbScale) -> Option<Vec<HbOutlineCommand>> {
        let x_factor = (scale.x_scale() as f32) / f32::from(self.font.hb_units_per_em());
        let y_factor = (scale.y_scale() as f32) / f32::from(self.font.hb_units_per_em());
        if let Some(cff) = self.cff2.as_ref() {
            let scalars = self.cff2_scalars.as_deref();
            let path = if scalars.is_none_or(|values| values.iter().all(|&value| value == 0.0)) {
                path2::<i64>(cff, glyph.0, None).map(|path| convert_path(path, x_factor, y_factor))
            } else {
                path2::<f64>(cff, glyph.0, scalars)
                    .map(|path| convert_path(path, x_factor, y_factor))
            };
            if path.is_some() {
                return path;
            }
        }
        self.cff1.as_ref().and_then(|cff| {
            path1::<i64>(cff, glyph.0, false).map(|path| convert_path(path, x_factor, y_factor))
        })
    }
}

fn convert_path<N: CsNumber>(
    path: Vec<PathCommand<N>>,
    x_factor: f32,
    y_factor: f32,
) -> Vec<HbOutlineCommand> {
    path.into_iter()
        .map(|command| command.scaled(x_factor, y_factor))
        .collect()
}

#[derive(Clone, Copy)]
enum PathCommand<N: CsNumber> {
    Move(Point<N>),
    Line(Point<N>),
    Cubic(Point<N>, Point<N>, Point<N>),
    Close,
}

impl<N: CsNumber> PathCommand<N> {
    fn scaled(self, x: f32, y: f32) -> HbOutlineCommand {
        let point = |p: Point<N>| PointF::new(p.x.to_real() * x, p.y.to_real() * y);
        match self {
            Self::Move(p) => HbOutlineCommand::MoveTo(point(p)),
            Self::Line(p) => HbOutlineCommand::LineTo(point(p)),
            Self::Cubic(a, b, p) => HbOutlineCommand::CubicTo(point(a), point(b), point(p)),
            Self::Close => HbOutlineCommand::Close,
        }
    }

    fn offset(self, delta: Point<N>) -> Self {
        let moved = |p: Point<N>| p.moved(delta.x, delta.y);
        match self {
            Self::Move(p) => Self::Move(moved(p)),
            Self::Line(p) => Self::Line(moved(p)),
            Self::Cubic(a, b, p) => Self::Cubic(moved(a), moved(b), moved(p)),
            Self::Close => Self::Close,
        }
    }
}

fn path2<N: CsNumber>(
    cff: &HbCff2<'_>,
    glyph: u32,
    scalars: Option<&[f32]>,
) -> Option<Vec<PathCommand<N>>> {
    if glyph >= cff.num_glyphs() {
        return None;
    }
    let fd = cff.fd(glyph);
    let (local, ivs) = cff.private(fd);
    let mut env = Env::<N>::new(
        cff.char_string(glyph).unwrap_or_default(),
        cff.global_subrs(),
        local,
        Kind::Cff2 { cff, ivs },
        scalars,
    );
    env.record_path = true;
    env.run()?;
    Some(env.path)
}

fn path1<N: CsNumber>(cff: &HbCff1<'_>, glyph: u32, in_seac: bool) -> Option<Vec<PathCommand<N>>> {
    if glyph >= cff.num_glyphs() {
        return None;
    }
    let local = cff.local_subrs(cff.fd(glyph));
    let mut env = Env::<N>::new(
        cff.char_string(glyph).unwrap_or_default(),
        cff.global_subrs(),
        local,
        Kind::Cff1 { cff, in_seac },
        None,
    );
    env.record_path = true;
    env.run()?;
    Some(env.path)
}

/// The box of `glyph` from HarfBuzz's CFF accelerator, before scaling
/// (`OT::cff1::accelerator_t::get_extents`).
pub fn cff1_extents(cff: &HbCff1<'_>, glyph: u32) -> Option<GlyphExtents> {
    bounds1::<i64>(cff, glyph, false).map(|b| b.extents())
}

/// The box of `glyph` from HarfBuzz's CFF2 accelerator at the default
/// instance, before scaling (`OT::cff2::accelerator_t::get_extents`).
pub fn cff2_extents(cff: &HbCff2<'_>, glyph: u32) -> Option<GlyphExtents> {
    cff2_extents_with_scalars(cff, glyph, None)
}

/// HarfBuzz's CFF2 box at one variable-font location.
pub fn cff2_extents_at(
    cff: &HbCff2<'_>,
    glyph: u32,
    instance: &Instance<'_, '_, '_, '_>,
) -> Option<GlyphExtents> {
    // hb_font_create initializes one zero coordinate per fvar axis, even
    // when the caller supplied no variations.
    let scalars = if instance.font().var_axes().next().is_some() {
        cff.variation_store()
            .map(|store| instance.cff2_hb_region_scalars(&store))
    } else {
        None
    };
    cff2_extents_with_scalars(cff, glyph, scalars.as_deref())
}

fn cff2_extents_with_scalars(
    cff: &HbCff2<'_>,
    glyph: u32,
    scalars: Option<&[f32]>,
) -> Option<GlyphExtents> {
    if scalars.is_none_or(|values| values.iter().all(|&value| value == 0.0)) {
        bounds2::<i64>(cff, glyph, None).map(|bounds| bounds.extents())
    } else {
        bounds2::<f64>(cff, glyph, scalars).map(|bounds| bounds.extents())
    }
}

fn bounds2<N: CsNumber>(
    cff: &HbCff2<'_>,
    glyph: u32,
    scalars: Option<&[f32]>,
) -> Option<Bounds<N>> {
    if glyph >= cff.num_glyphs() {
        return None;
    }
    let fd = cff.fd(glyph);
    let (local, ivs) = cff.private(fd);
    let mut env = Env::<N>::new(
        cff.char_string(glyph).unwrap_or_default(),
        cff.global_subrs(),
        local,
        Kind::Cff2 { cff, ivs },
        scalars,
    );
    env.run()?;
    Some(env.bounds)
}

/// `_get_bounds`: the bounds of a CFF glyph, `None` when its charstring
/// fails.
fn bounds1<N: CsNumber>(cff: &HbCff1<'_>, glyph: u32, in_seac: bool) -> Option<Bounds<N>> {
    if glyph >= cff.num_glyphs() {
        return None;
    }
    let local = cff.local_subrs(cff.fd(glyph));
    let mut env = Env::<N>::new(
        cff.char_string(glyph).unwrap_or_default(),
        cff.global_subrs(),
        local,
        Kind::Cff1 { cff, in_seac },
        None,
    );
    env.run()?;
    Some(env.bounds)
}

/// A point in 16.16 units.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct Point<N: CsNumber> {
    x: N,
    y: N,
}

impl<N: CsNumber> Point<N> {
    fn moved(self, dx: N, dy: N) -> Point<N> {
        Point {
            x: self.x.add(dx),
            y: self.y.add(dy),
        }
    }
}

/// `bounds_t` (CFF) and the bounds of `cff2_extents_param_t`.
#[derive(Clone, Copy, Debug)]
struct Bounds<N: CsNumber> {
    min: Point<N>,
    max: Point<N>,
}

impl<N: CsNumber> Bounds<N> {
    fn new() -> Bounds<N> {
        Bounds {
            min: Point {
                x: N::from_integer(i64::from(i32::MAX)),
                y: N::from_integer(i64::from(i32::MAX)),
            },
            max: Point {
                x: N::from_integer(i64::from(i32::MIN)),
                y: N::from_integer(i64::from(i32::MIN)),
            },
        }
    }

    fn update(&mut self, p: Point<N>) {
        self.min.x = if self.min.x < p.x { self.min.x } else { p.x };
        self.max.x = if self.max.x > p.x { self.max.x } else { p.x };
        self.min.y = if self.min.y < p.y { self.min.y } else { p.y };
        self.max.y = if self.max.y > p.y { self.max.y } else { p.y };
    }

    fn empty(&self) -> bool {
        self.min.x >= self.max.x || self.min.y >= self.max.y
    }

    fn merge(&mut self, b: &Bounds<N>) {
        if self.empty() {
            *self = *b;
        } else if !b.empty() {
            self.update(b.min);
            self.update(b.max);
        }
    }

    fn offset(&mut self, d: Point<N>) {
        if !self.empty() {
            self.min = self.min.moved(d.x, d.y);
            self.max = self.max.moved(d.x, d.y);
        }
    }

    /// `get_extents`: each axis rounded with `floor (x + 0.5)`, or 0 when
    /// its minimum is not below its maximum.
    fn extents(&self) -> GlyphExtents {
        let axis = |min: N, max: N, flip: bool| {
            if min >= max {
                return (0, 0);
            }
            let (lo, hi) = (min.round(), max.round());
            if flip {
                (clamp(hi), clamp(lo.saturating_sub(hi)))
            } else {
                (clamp(lo), clamp(hi.saturating_sub(lo)))
            }
        };
        let (x_bearing, width) = axis(self.min.x, self.max.x, false);
        let (y_bearing, height) = axis(self.min.y, self.max.y, true);
        GlyphExtents {
            x_bearing,
            y_bearing,
            width,
            height,
        }
    }
}

/// The default-instance fixed-point rounding equivalence (ADR 0026).
#[cfg(test)]
fn round(v: i64) -> i64 {
    v.saturating_add(ONE / 2) >> 16
}

fn clamp(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX })
}

/// The table being read and what only it has.
#[derive(Clone, Copy)]
enum Kind<'a, 'c> {
    /// CFF: the accelerator (for seac) and whether this glyph is a seac
    /// component.
    Cff1 { cff: &'c HbCff1<'a>, in_seac: bool },
    /// CFF2: the accelerator and the Private DICT's `vsindex`.
    Cff2 { cff: &'c HbCff2<'a>, ivs: u32 },
}

/// `byte_str_ref_t`: a byte string and a cursor, in error past the end.
#[derive(Clone, Copy, Debug)]
struct Str<'a> {
    data: &'a [u8],
    offset: usize,
}

impl Str<'_> {
    fn avail(&self, n: usize) -> bool {
        self.offset.saturating_add(n) <= self.data.len()
    }

    fn inc(&mut self, n: usize) {
        self.offset = self.offset.saturating_add(n);
    }

    fn set_error(&mut self) {
        self.offset = self.data.len().saturating_add(1);
    }

    fn in_error(&self) -> bool {
        self.offset > self.data.len()
    }

    fn byte(&mut self, i: usize) -> u8 {
        if let Some(&b) = self.data.get(self.offset.saturating_add(i)) {
            b
        } else {
            self.set_error();
            0
        }
    }
}

/// What `fetch_op` gives besides operators.
enum Fetched {
    Op(u32),
    /// `OpCode_Invalid` because the string ended: in CFF, every later
    /// fetch gives it too, so the glyph fails once the operator budget
    /// is spent.
    End,
}

/// The interpreter state (`cff1_cs_interp_env_t`,
/// `cff2_cs_interp_env_t`) and the extents parameter.
struct Env<'a, 'c, 's, N: CsNumber> {
    s: Str<'a>,
    stack: [N; STACK],
    len: usize,
    stack_error: bool,
    calls: [Str<'a>; MAX_CALLS],
    ncalls: usize,
    call_error: bool,
    global: HbIndex<'a>,
    local: HbIndex<'a>,
    hstem_count: u32,
    vstem_count: u32,
    hintmask_size: u32,
    seen_hintmask: bool,
    pt: Point<N>,
    endchar: bool,
    kind: Kind<'a, 'c>,
    ivs: u32,
    seen_vsindex: bool,
    seen_blend: bool,
    region_count: u32,
    region_scalars: Option<&'s [f32]>,
    weights: Vec<f32>,
    path_open: bool,
    path_start: Point<N>,
    bounds: Bounds<N>,
    record_path: bool,
    path: Vec<PathCommand<N>>,
}

impl<'a, 'c, 's, N: CsNumber> Env<'a, 'c, 's, N> {
    fn new(
        data: &'a [u8],
        global: HbIndex<'a>,
        local: HbIndex<'a>,
        kind: Kind<'a, 'c>,
        region_scalars: Option<&'s [f32]>,
    ) -> Self {
        let s = Str { data, offset: 0 };
        let ivs = match kind {
            Kind::Cff2 { ivs, .. } => ivs,
            Kind::Cff1 { .. } => 0,
        };
        Env {
            s,
            stack: [N::zero(); STACK],
            len: 0,
            stack_error: false,
            calls: [s; MAX_CALLS],
            ncalls: 0,
            call_error: false,
            global,
            local,
            hstem_count: 0,
            vstem_count: 0,
            hintmask_size: 0,
            seen_hintmask: false,
            pt: Point::default(),
            endchar: false,
            kind,
            ivs,
            seen_vsindex: false,
            seen_blend: false,
            region_count: 0,
            region_scalars,
            weights: Vec::new(),
            path_open: false,
            path_start: Point::default(),
            bounds: Bounds::new(),
            record_path: false,
            path: Vec::new(),
        }
    }

    fn cff2(&self) -> bool {
        matches!(self.kind, Kind::Cff2 { .. })
    }

    fn in_error(&self) -> bool {
        self.call_error || self.s.in_error() || self.stack_error
    }

    /// `cs_interpreter_t::interpret`.
    fn run(&mut self) -> Option<()> {
        let mut ops = MAX_OPS;
        loop {
            match self.fetch_op() {
                Fetched::Op(op) => self.process_op(op),
                Fetched::End => return None,
            }
            ops = ops.saturating_sub(1);
            if self.in_error() || ops == 0 {
                return None;
            }
            if self.endchar {
                return Some(());
            }
        }
    }

    /// `fetch_op`, with CFF2's made-up `return` and `endchar` at the end
    /// of a string.
    fn fetch_op(&mut self) -> Fetched {
        if !self.s.avail(1) {
            if !self.cff2() {
                return Fetched::End;
            }
            return Fetched::Op(if self.ncalls == 0 { 14 } else { 11 });
        }
        let b = self.s.byte(0);
        self.s.inc(1);
        if b != 12 {
            return Fetched::Op(u32::from(b));
        }
        if !self.s.avail(1) {
            if self.cff2() {
                // OpCode_Invalid: an unknown operator.
                return Fetched::Op(0xFFFF);
            }
            return Fetched::End;
        }
        let e = self.s.byte(0);
        self.s.inc(1);
        Fetched::Op(u32::from(e).saturating_add(256))
    }

    fn push(&mut self, v: N) {
        if let Some(slot) = self.stack.get_mut(self.len) {
            *slot = v;
            self.len = self.len.saturating_add(1);
        } else {
            self.stack_error = true;
        }
    }

    fn pop(&mut self) -> N {
        if let Some(n) = self.len.checked_sub(1) {
            self.len = n;
            self.stack.get(n).copied().unwrap_or(N::zero())
        } else {
            self.stack_error = true;
            N::zero()
        }
    }

    /// `pop_uint`: truncated, an error below 0.
    fn pop_uint(&mut self) -> u32 {
        let v = self.pop().to_int();
        u32::try_from(v).unwrap_or_else(|_| {
            self.stack_error = true;
            0
        })
    }

    /// `eval_arg`: operand `i` from the bottom; an error past the top.
    fn arg(&mut self, i: usize) -> N {
        if i < self.len {
            self.stack.get(i).copied().unwrap_or(N::zero())
        } else {
            self.stack_error = true;
            N::zero()
        }
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn process_op(&mut self, op: u32) {
        let cff2 = self.cff2();
        match op {
            11 => self.return_from_subr(),
            14 => {
                if !cff2 && self.len >= 4 {
                    self.seac();
                }
                self.close_path();
                self.clear();
                self.endchar = true;
            }
            255 => {
                if self.s.avail(4) {
                    let v = i32::from_be_bytes([
                        self.s.byte(0),
                        self.s.byte(1),
                        self.s.byte(2),
                        self.s.byte(3),
                    ]);
                    self.push(N::from_fixed(v));
                    self.s.inc(4);
                }
            }
            10 => self.call_subr(false),
            29 => self.call_subr(true),
            1 | 18 => {
                self.hstem_count = self.hstem_count.saturating_add(self.count() / 2);
                self.clear();
            }
            3 | 23 => {
                self.vstem_count = self.vstem_count.saturating_add(self.count() / 2);
                self.clear();
            }
            19 | 20 => {
                if !self.seen_hintmask {
                    self.vstem_count = self.vstem_count.saturating_add(self.count() / 2);
                    self.hintmask_size = self
                        .hstem_count
                        .saturating_add(self.vstem_count)
                        .saturating_add(7)
                        >> 3;
                    self.seen_hintmask = true;
                }
                let size = usize::try_from(self.hintmask_size).unwrap_or(usize::MAX);
                if self.s.avail(size) {
                    self.clear();
                    self.s.inc(size);
                }
            }
            21 => {
                let dy = self.pop();
                let dx = self.pop();
                let p = self.pt.moved(dx, dy);
                self.moveto(p);
                self.clear();
            }
            22 => {
                let dx = self.pop();
                let p = self.pt.moved(dx, N::zero());
                self.moveto(p);
                self.clear();
            }
            4 => {
                let dy = self.pop();
                let p = self.pt.moved(N::zero(), dy);
                self.moveto(p);
                self.clear();
            }
            5..=8 | 24..=27 | 30 | 31 | 290..=293 => {
                self.path(op);
                self.clear();
            }
            256 if !cff2 => {}
            15 if cff2 => self.vsindex(),
            16 if cff2 => self.blend(),
            _ => self.number(op),
        }
    }

    fn count(&self) -> u32 {
        u32::try_from(self.len).unwrap_or(u32::MAX)
    }

    /// `opset_t::process_op`: Type 2 numbers.
    fn number(&mut self, op: u32) {
        match op {
            28 => {
                let v = i16::from_be_bytes([self.s.byte(0), self.s.byte(1)]);
                self.push(N::from_integer(i64::from(v)));
                self.s.inc(2);
            }
            247..=250 => {
                let v = i64::from(op)
                    .saturating_sub(247)
                    .saturating_mul(256)
                    .saturating_add(i64::from(self.s.byte(0)))
                    .saturating_add(108);
                self.push(N::from_integer(v));
                self.s.inc(1);
            }
            251..=254 => {
                let v = 251i64
                    .saturating_sub(i64::from(op))
                    .saturating_mul(256)
                    .saturating_sub(i64::from(self.s.byte(0)))
                    .saturating_sub(108);
                self.push(N::from_integer(v));
                self.s.inc(1);
            }
            32..=246 => self.push(N::from_integer(i64::from(op).saturating_sub(139))),
            _ => self.clear(),
        }
    }

    /// `call_subr`.
    fn call_subr(&mut self, global: bool) {
        let subrs = if global { self.global } else { self.local };
        let n = self
            .pop()
            .to_int()
            .saturating_add(i64::from(subr_bias(subrs.len())));
        let index = u32::try_from(n).ok().filter(|&i| i < subrs.len());
        let (Some(index), true) = (index, self.ncalls < MAX_CALLS) else {
            self.s.set_error();
            return;
        };
        if let Some(slot) = self.calls.get_mut(self.ncalls) {
            *slot = self.s;
        }
        self.ncalls = self.ncalls.saturating_add(1);
        self.s = Str {
            data: subrs.get(index).unwrap_or_default(),
            offset: 0,
        };
    }

    /// `return_from_subr`.
    fn return_from_subr(&mut self) {
        if let Some(n) = self.ncalls.checked_sub(1) {
            self.ncalls = n;
            self.s = self.calls.get(n).copied().unwrap_or(Str {
                data: &[],
                offset: 0,
            });
        } else {
            self.call_error = true;
            self.s = Str {
                data: &[],
                offset: 0,
            };
        }
    }

    /// The extents path procedures' `moveto`.
    fn moveto(&mut self, p: Point<N>) {
        self.close_path();
        self.pt = p;
    }

    fn open(&mut self) {
        if !self.path_open {
            self.path_open = true;
            self.path_start = self.pt;
            self.bounds.update(self.pt);
            if self.record_path {
                self.path.push(PathCommand::Move(self.pt));
            }
        }
    }

    fn close_path(&mut self) {
        if self.path_open {
            if self.record_path {
                if self.pt != self.path_start {
                    self.path.push(PathCommand::Line(self.path_start));
                }
                self.path.push(PathCommand::Close);
            }
            self.path_open = false;
        }
    }

    fn line(&mut self, p: Point<N>) {
        self.open();
        self.pt = p;
        self.bounds.update(p);
        if self.record_path {
            self.path.push(PathCommand::Line(p));
        }
    }

    fn curve(&mut self, p1: Point<N>, p2: Point<N>, p3: Point<N>) {
        self.open();
        self.bounds.update(p1);
        self.bounds.update(p2);
        self.pt = p3;
        self.bounds.update(p3);
        if self.record_path {
            self.path.push(PathCommand::Cubic(p1, p2, p3));
        }
    }

    /// `path_procs_t`: the lines, curves and flexes.
    fn path(&mut self, op: u32) {
        let n = self.len;
        match op {
            5 => {
                let mut i = 0usize;
                while i.saturating_add(2) <= n {
                    let p = self.pt.moved(self.arg(i), self.arg(i.saturating_add(1)));
                    self.line(p);
                    i = i.saturating_add(2);
                }
            }
            6 | 7 => {
                let horizontal_first = op == 6;
                let mut i = 0usize;
                while i.saturating_add(2) <= n {
                    let a = self.arg(i);
                    let b = self.arg(i.saturating_add(1));
                    let p1 = self.axis(self.pt, a, horizontal_first);
                    self.line(p1);
                    let p2 = self.axis(p1, b, !horizontal_first);
                    self.line(p2);
                    i = i.saturating_add(2);
                }
                if i < n {
                    let a = self.arg(i);
                    let p = self.axis(self.pt, a, horizontal_first);
                    self.line(p);
                }
            }
            8 => {
                let mut i = 0usize;
                while i.saturating_add(6) <= n {
                    self.rcurve(i);
                    i = i.saturating_add(6);
                }
            }
            24 => {
                if n < 8 {
                    return;
                }
                let mut i = 0usize;
                while i.saturating_add(6) <= n.saturating_sub(2) {
                    self.rcurve(i);
                    i = i.saturating_add(6);
                }
                let p = self.pt.moved(self.arg(i), self.arg(i.saturating_add(1)));
                self.line(p);
            }
            25 => {
                if n < 8 {
                    return;
                }
                let mut i = 0usize;
                while i.saturating_add(2) <= n.saturating_sub(6) {
                    let p = self.pt.moved(self.arg(i), self.arg(i.saturating_add(1)));
                    self.line(p);
                    i = i.saturating_add(2);
                }
                self.rcurve(i);
            }
            26 | 27 => self.vv_hh(op == 27),
            30 | 31 => self.vh_hv(op == 31),
            290..=293 => self.flex(op),
            _ => {}
        }
    }

    /// `p` moved along x (`horizontal`) or y by `d`.
    fn axis(&self, p: Point<N>, d: N, horizontal: bool) -> Point<N> {
        if horizontal {
            p.moved(d, N::zero())
        } else {
            p.moved(N::zero(), d)
        }
    }

    /// A curve from operands `i..i.saturating_add(6)`, relative.
    fn rcurve(&mut self, i: usize) {
        let p1 = self.pt.moved(self.arg(i), self.arg(i.saturating_add(1)));
        let p2 = p1.moved(self.arg(i.saturating_add(2)), self.arg(i.saturating_add(3)));
        let p3 = p2.moved(self.arg(i.saturating_add(4)), self.arg(i.saturating_add(5)));
        self.curve(p1, p2, p3);
    }

    /// `vvcurveto` (`hh` false) and `hhcurveto` (`hh` true).
    fn vv_hh(&mut self, hh: bool) {
        let n = self.len;
        let mut i = 0usize;
        let mut p1 = self.pt;
        if n & 1 != 0 {
            let d = self.arg(i);
            p1 = self.axis(p1, d, !hh);
            i = i.saturating_add(1);
        }
        while i.saturating_add(4) <= n {
            let d = self.arg(i);
            p1 = self.axis(p1, d, hh);
            let p2 = p1.moved(self.arg(i.saturating_add(1)), self.arg(i.saturating_add(2)));
            let d3 = self.arg(i.saturating_add(3));
            let p3 = self.axis(p2, d3, hh);
            self.curve(p1, p2, p3);
            p1 = self.pt;
            i = i.saturating_add(4);
        }
    }

    /// `vhcurveto` (`hv` false) and `hvcurveto` (`hv` true): curves whose
    /// tangents alternate, starting horizontal for `hv`.
    fn vh_hv(&mut self, hv: bool) {
        let n = self.len;
        let mut i = 0usize;
        // One curve from operands i..i+4: first along `first`, last
        // across it.
        let quarter = |env: &mut Self, start: Point<N>, i: usize, first_h: bool| {
            let d0 = env.arg(i);
            let p1 = env.axis(start, d0, first_h);
            let p2 = p1.moved(env.arg(i.saturating_add(1)), env.arg(i.saturating_add(2)));
            let d3 = env.arg(i.saturating_add(3));
            let p3 = env.axis(p2, d3, !first_h);
            (p1, p2, p3)
        };
        if n % 8 >= 4 {
            let start = self.pt;
            let (mut p1, mut p2, mut p3) = quarter(self, start, i, hv);
            i = i.saturating_add(4);
            while i.saturating_add(8) <= n {
                self.curve(p1, p2, p3);
                let start = self.pt;
                let (a1, a2, a3) = quarter(self, start, i, !hv);
                self.curve(a1, a2, a3);
                let (b1, b2, b3) = quarter(self, a3, i.saturating_add(4), hv);
                (p1, p2, p3) = (b1, b2, b3);
                i = i.saturating_add(8);
            }
            if i < n {
                // The last curve ends along the first axis too.
                let d = self.arg(i);
                p3 = self.axis(p3, d, hv);
            }
            self.curve(p1, p2, p3);
        } else {
            while i.saturating_add(8) <= n {
                let start = self.pt;
                let (a1, a2, a3) = quarter(self, start, i, hv);
                self.curve(a1, a2, a3);
                let (b1, b2, mut b3) = quarter(self, a3, i.saturating_add(4), !hv);
                if n.saturating_sub(i) < 16 && n & 1 != 0 {
                    let d = self.arg(i.saturating_add(8));
                    b3 = self.axis(b3, d, !hv);
                }
                self.curve(b1, b2, b3);
                i = i.saturating_add(8);
            }
        }
    }

    /// `hflex`, `flex`, `hflex1` and `flex1`: two curves, or an error for
    /// another operand count.
    fn flex(&mut self, op: u32) {
        let n = self.len;
        let pt = self.pt;
        let points = match (op, n) {
            (290, 7) => {
                let p1 = pt.moved(self.arg(0), N::zero());
                let p2 = p1.moved(self.arg(1), self.arg(2));
                let p3 = p2.moved(self.arg(3), N::zero());
                let p4 = p3.moved(self.arg(4), N::zero());
                let mut p5 = p4.moved(self.arg(5), N::zero());
                p5.y = p1.y;
                let p6 = p5.moved(self.arg(6), N::zero());
                [p1, p2, p3, p4, p5, p6]
            }
            (291, 13) => {
                let p1 = pt.moved(self.arg(0), self.arg(1));
                let p2 = p1.moved(self.arg(2), self.arg(3));
                let p3 = p2.moved(self.arg(4), self.arg(5));
                let p4 = p3.moved(self.arg(6), self.arg(7));
                let p5 = p4.moved(self.arg(8), self.arg(9));
                let p6 = p5.moved(self.arg(10), self.arg(11));
                [p1, p2, p3, p4, p5, p6]
            }
            (292, 9) => {
                let p1 = pt.moved(self.arg(0), self.arg(1));
                let p2 = p1.moved(self.arg(2), self.arg(3));
                let p3 = p2.moved(self.arg(4), N::zero());
                let p4 = p3.moved(self.arg(5), N::zero());
                let p5 = p4.moved(self.arg(6), self.arg(7));
                let mut p6 = p5.moved(self.arg(8), N::zero());
                p6.y = pt.y;
                [p1, p2, p3, p4, p5, p6]
            }
            (293, 11) => {
                let (mut dx, mut dy) = (N::zero(), N::zero());
                for k in 0usize..5 {
                    dx = dx.add(self.arg(k.saturating_mul(2)));
                    dy = dy.add(self.arg(k.saturating_mul(2).saturating_add(1)));
                }
                let p1 = pt.moved(self.arg(0), self.arg(1));
                let p2 = p1.moved(self.arg(2), self.arg(3));
                let p3 = p2.moved(self.arg(4), self.arg(5));
                let p4 = p3.moved(self.arg(6), self.arg(7));
                let p5 = p4.moved(self.arg(8), self.arg(9));
                let mut p6 = p5;
                if dx.abs() > dy.abs() {
                    p6.x = p6.x.add(self.arg(10));
                    p6.y = pt.y;
                } else {
                    p6.x = pt.x;
                    p6.y = p6.y.add(self.arg(10));
                }
                [p1, p2, p3, p4, p5, p6]
            }
            _ => {
                self.s.set_error();
                return;
            }
        };
        let [p1, p2, p3, p4, p5, p6] = points;
        self.curve(p1, p2, p3);
        self.curve(p4, p5, p6);
    }

    /// CFF's `endchar` with four operands or more
    /// (`cff1_cs_opset_extents_t::process_seac`): the base and accent
    /// glyphs of the Standard encoding codes, the accent moved.
    fn seac(&mut self) {
        let Kind::Cff1 { cff, in_seac } = self.kind else {
            return;
        };
        let n = self.len;
        let top = |k: usize| {
            n.checked_sub(k)
                .and_then(|i| self.stack.get(i).copied())
                .unwrap_or(N::zero())
        };
        let delta = Point {
            x: top(4),
            y: top(3),
        };
        let code = |v: N| u32::try_from(v.to_int()).unwrap_or(u32::MAX);
        let base = cff.std_code_to_glyph(code(top(2)));
        let accent = cff.std_code_to_glyph(code(top(1)));
        let parts = if in_seac || base == 0 || accent == 0 {
            None
        } else {
            bounds1::<N>(cff, base, true)
                .and_then(|b| bounds1::<N>(cff, accent, true).map(|a| (b, a)))
        };
        match parts {
            Some((b, mut a)) => {
                self.bounds.merge(&b);
                a.offset(delta);
                self.bounds.merge(&a);
                if self.record_path {
                    let paths = path1::<N>(cff, base, true).and_then(|base_path| {
                        path1::<N>(cff, accent, true).map(|a| (base_path, a))
                    });
                    if let Some((base_path, accent_path)) = paths {
                        self.close_path();
                        self.path.extend(base_path);
                        self.path
                            .extend(accent_path.into_iter().map(|command| command.offset(delta)));
                    } else {
                        self.s.set_error();
                    }
                }
            }
            None => self.s.set_error(),
        }
    }

    /// CFF2's `vsindex`.
    fn vsindex(&mut self) {
        let index = self.pop_uint();
        if self.seen_vsindex || self.seen_blend {
            self.s.set_error();
        } else {
            self.ivs = index;
        }
        self.seen_vsindex = true;
        self.clear();
    }

    /// CFF2's `blend`: add every delta column in its original order.
    fn blend(&mut self) {
        let Kind::Cff2 { cff, .. } = self.kind else {
            return;
        };
        if !self.seen_blend {
            self.region_count = cff.region_count(self.ivs);
            if let (Some(store), Some(scalars), Ok(ivs)) = (
                cff.variation_store(),
                self.region_scalars,
                u16::try_from(self.ivs),
            ) && let Some(data) = store.item_data(ivs)
                && u32::from(data.region_index_count()) == self.region_count
            {
                for column in 0..usize::from(data.region_index_count()) {
                    let Some(index) = data.region_index(column) else {
                        break;
                    };
                    let Some(&value) = scalars.get(usize::from(index)) else {
                        break;
                    };
                    self.weights.push(value);
                }
            }
            self.seen_blend = true;
        }
        let k = self.region_count;
        let n = self.pop_uint();
        let total = k.checked_add(1).and_then(|k1| k1.checked_mul(n));
        match total {
            Some(t) if t <= self.count() => {
                let start = self
                    .len
                    .saturating_sub(usize::try_from(t).unwrap_or(usize::MAX));
                let n_values = usize::try_from(n).unwrap_or(usize::MAX);
                let n_regions = usize::try_from(k).unwrap_or(usize::MAX);
                if self.weights.len() == n_regions
                    && n_regions > 0
                    && let Some(columns) = self.stack.get_mut(start..self.len)
                    && let Some((defaults, deltas)) = columns.split_at_mut_checked(n_values)
                {
                    for (value, columns) in defaults.iter_mut().zip(deltas.chunks_exact(n_regions))
                    {
                        for (scalar, delta) in self.weights.iter().zip(columns) {
                            *value = value.blend(*scalar, *delta);
                        }
                    }
                }
                let drop = usize::try_from(k.saturating_mul(n)).unwrap_or(usize::MAX);
                self.len = self.len.saturating_sub(drop);
            }
            _ => self.s.set_error(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss
    )]

    use super::{ONE, round};
    use sofel_geom::float::hb_roundf;

    /// `hb_roundf` of the 16.16 value `v`, as HarfBuzz computes it on
    /// its `double`.
    fn via_double(v: i64) -> i64 {
        hb_roundf(v as f64 / ONE as f64) as i64
    }

    fn check_near(c: i64) {
        for v in c - 2..=c + 2 {
            assert_eq!(round(v), via_double(v), "round({v:#x})");
        }
    }

    #[test]
    fn integer_rounding_is_hb_roundf() {
        // Around every integer k and every half k + 0.5 of the 16-bit
        // integer range and a little beyond.
        for k in -0x1_0001..=0x1_0001_i64 {
            check_near(k * ONE);
            check_near(k * ONE + ONE / 2);
        }
        // The ends of the 16.16 range in i32, and of the sums the
        // interpreter keeps in i64 (|v| < 2^52 is exact in double).
        for c in [
            i64::from(i32::MIN),
            i64::from(i32::MAX),
            -(1_i64 << 52),
            (1_i64 << 52) - 2,
        ] {
            check_near(c);
        }
        let mut s: u64 = 0x0021_0026;
        for _ in 0..1_000_000 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            // Any width up to 52 bits, with a sign.
            let bits = (s >> 58) as u32 % 53;
            let m = (s & ((1_u64 << bits) - 1)) as i64;
            let v = if s & (1 << 57) == 0 { m } else { -m };
            assert_eq!(round(v), via_double(v), "round({v:#x})");
        }
    }
}
