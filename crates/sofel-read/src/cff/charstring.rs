//! Type 2 charstrings: the lexer, and FreeType's charstring decoder
//! without hinting.
//!
//! The decoder is a port of FreeType 2.15.0's Adobe engine
//! (`cf2_interpT2CharString` in `src/psaux/psintrp.c`, `cf2_glyphpath_*`
//! in `src/psaux/pshints.c`, `src/psaux/psstack.c`) with hinting and stem
//! darkening taken out. What stays: the operand stack with its number
//! types, the subroutine stack, the width, the stem counts and hint masks
//! (they decide errors and which zero-length lines are kept), flex, seac,
//! the arithmetic operators, CFF2 `blend` and `vsindex`, and the glyph
//! path, which drops zero-length lines and maps points to device space.
//! Copyright 2007-2014 Adobe Systems Incorporated, and Copyright (C)
//! 1996-2026 by David Turner, Robert Wilhelm, and Werner Lemberg. Used
//! under the FreeType License (FTL) with the Adobe patent license; see
//! THIRD_PARTY_NOTICES.md.

use super::dict::BlendWeights;
use super::{Charset, Index, STANDARD_ENCODING, Version, ft};
use crate::be;

/// One token of a charstring.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CharStringToken {
    /// An integer (bytes 28 and 32 to 254).
    Integer(i32),
    /// A 16.16 number (byte 255).
    Fixed(i32),
    /// An operator: the byte for one-byte operators, `0x0C00 | b` for
    /// the two-byte operators `12 b`.
    Operator(u16),
}

/// The tokens of a charstring. The bytes after `hintmask` and `cntrmask`
/// depend on the stem count, which the lexer does not know: the caller
/// skips them with [`CharStringTokens::skip`].
#[derive(Clone, Debug)]
pub struct CharStringTokens<'a> {
    data: &'a [u8],
    pos: usize,
    version: Version,
}

impl<'a> CharStringTokens<'a> {
    /// The tokens of `data`, a charstring of `version`.
    pub fn new(data: &'a [u8], version: Version) -> CharStringTokens<'a> {
        CharStringTokens {
            data,
            pos: 0,
            version,
        }
    }

    /// The version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The position of the next token.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Whether every byte has been read.
    pub fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Skips `n` bytes. `None`, without moving, when fewer are left.
    pub fn skip(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = be::slice_at(self.data, self.pos, n)?;
        self.pos = self.pos.saturating_add(n);
        Some(s)
    }

    /// The next token, or `Err` with its position when it is cut by the
    /// end of the data. `None` at the end.
    pub fn next_token(&mut self) -> Option<Result<CharStringToken, usize>> {
        let at = self.pos;
        let b = be::u8_at(self.data, at)?;
        let bytes = |n: usize| be::slice_at(self.data, at.saturating_add(1), n);
        let (token, len) = match b {
            28 => match bytes(2) {
                Some(&[x, y]) => (
                    CharStringToken::Integer(i32::from(i16::from_be_bytes([x, y]))),
                    3,
                ),
                _ => return Some(Err(at)),
            },
            32..=246 => (
                CharStringToken::Integer(i32::from(b).saturating_sub(139)),
                1,
            ),
            247..=250 => match bytes(1) {
                Some(&[x]) => (
                    CharStringToken::Integer(
                        i32::from(b)
                            .wrapping_sub(247)
                            .saturating_mul(256)
                            .saturating_add(i32::from(x))
                            .saturating_add(108),
                    ),
                    2,
                ),
                _ => return Some(Err(at)),
            },
            251..=254 => match bytes(1) {
                Some(&[x]) => (
                    CharStringToken::Integer(
                        i32::from(b)
                            .wrapping_sub(251)
                            .saturating_mul(-256)
                            .saturating_sub(i32::from(x))
                            .saturating_sub(108),
                    ),
                    2,
                ),
                _ => return Some(Err(at)),
            },
            255 => match bytes(4) {
                Some(&[w, x, y, z]) => {
                    (CharStringToken::Fixed(i32::from_be_bytes([w, x, y, z])), 5)
                }
                _ => return Some(Err(at)),
            },
            12 => match bytes(1) {
                Some(&[x]) => (CharStringToken::Operator(0x0C00 | u16::from(x)), 2),
                _ => return Some(Err(at)),
            },
            _ => (CharStringToken::Operator(u16::from(b)), 1),
        };
        self.pos = at.saturating_add(len);
        Some(Ok(token))
    }
}

/// Why a charstring cannot be decoded. FreeType fails the glyph for
/// each of them (with codes ADR 0021 does not compare).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CharStringErrorKind {
    /// More operands than the stack holds.
    StackOverflow,
    /// An operator without the operands it needs, or an operand read
    /// beyond the stack.
    StackUnderflow,
    /// A 16.16 number where an integer is needed (`cf2_stack_popInt`).
    TypeMismatch,
    /// A number, an operator or mask bytes cut by the end of the data.
    Truncated,
    /// A subroutine number outside its INDEX.
    SubroutineOutOfRange,
    /// Subroutines nested deeper than 16.
    SubroutineTooDeep,
    /// `return` outside a subroutine.
    ReturnOutsideSubroutine,
    /// More than 96 stem hints.
    TooManyHints,
    /// `seac` inside a `seac` component.
    NestedSeac,
    /// A `seac` code with no glyph in the standard encoding and charset.
    SeacComponentMissing,
    /// CFF2 `vsindex` after `blend`.
    VsindexAfterBlend,
    /// CFF2 `vsindex` outside the Variation Store, or a blend vector that
    /// cannot be built.
    VsindexOutOfRange,
    /// CFF2 `blend` in a font without a Variation Store.
    BlendWithoutStore,
    /// More than 20,000,000 operators.
    InstructionLimit,
}

/// A decoding error and where it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CharStringError {
    /// What failed.
    pub kind: CharStringErrorKind,
    /// The position in the charstring or subroutine being run.
    pub position: usize,
    /// The subroutine depth there: 0 for the glyph's charstring.
    pub depth: u8,
}

/// Where the decoder sends the outline: FreeType's builder callbacks
/// (`cf2_builder_moveTo`, `lineTo` and `cubeTo`), with points in
/// FreeType's device space (16.16 times the decoder's scale).
pub trait PathSink {
    /// A new contour starts; the open one, if any, is closed.
    fn move_to(&mut self);
    /// A line from `from` (the current point) to `to`.
    fn line_to(&mut self, from: (i32, i32), to: (i32, i32));
    /// A cubic curve from `from` (the current point).
    fn cubic_to(&mut self, from: (i32, i32), c1: (i32, i32), c2: (i32, i32), to: (i32, i32));
}

/// A sink that drops everything (for validation).
#[derive(Clone, Copy, Debug, Default)]
pub struct NullSink;

impl PathSink for NullSink {
    fn move_to(&mut self) {}
    fn line_to(&mut self, _: (i32, i32), _: (i32, i32)) {}
    fn cubic_to(&mut self, _: (i32, i32), _: (i32, i32), _: (i32, i32), _: (i32, i32)) {}
}

/// What the decoder needs besides the charstring: FreeType's decoder
/// fields for one glyph.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// CFF or CFF2.
    pub version: Version,
    /// The CharStrings INDEX, for `seac` components.
    pub char_strings: Index<'a>,
    /// The charset, for `seac` components (CFF).
    pub charset: Option<Charset<'a>>,
    /// The Global Subrs INDEX.
    pub global_subrs: Index<'a>,
    /// [`Index::ft_offsets_sorted`] of the Global Subrs INDEX.
    pub global_subrs_sorted: bool,
    /// The Local Subrs INDEX of the glyph's Private DICT (empty if none).
    pub local_subrs: Index<'a>,
    /// [`Index::ft_offsets_sorted`] of the Local Subrs INDEX.
    pub local_subrs_sorted: bool,
    /// `CharstringType` of the Top DICT: 1 makes the subroutine bias 0.
    pub charstring_type: i32,
    /// `defaultWidthX`.
    pub default_width_x: i64,
    /// `nominalWidthX`.
    pub nominal_width_x: i64,
    /// The operand stack size of CFF2 (`maxstack`); CFF uses 48.
    pub max_stack: u32,
    /// Whether the Variation Store has ItemVariationData (FreeType's
    /// `hasVariations`); `blend` fails without.
    pub has_variations: bool,
    /// `vsindex` of the Private DICT.
    pub vsindex: u32,
    /// The weights of `blend`.
    pub blend: &'a dyn BlendWeights,
    /// The first state of `random`.
    pub random_seed: u32,
    /// The scale from character space to device space (`0x400` for
    /// FreeType without hinting).
    pub scale: i32,
}

impl core::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Context")
            .field("version", &self.version)
            .field("max_stack", &self.max_stack)
            .finish_non_exhaustive()
    }
}

/// The largest operand stack (CFF2 `maxstack`).
pub const MAX_STACK: usize = 513;
/// FreeType's `CF2_MAX_SUBR`.
const MAX_SUBR: usize = 16;
/// `CF2_MAX_HINTS`.
const MAX_HINTS: usize = 96;
/// `CF2_STORAGE_SIZE`.
const STORAGE_SIZE: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Number {
    Int(i32),
    Fixed(i32),
}

impl Number {
    fn real(self) -> i32 {
        match self {
            Number::Int(i) => int_to_fixed(i),
            Number::Fixed(f) => f,
        }
    }
}

fn int_to_fixed(i: i32) -> i32 {
    (i.cast_unsigned() << 16).cast_signed()
}

fn add(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

fn sub(a: i32, b: i32) -> i32 {
    a.wrapping_sub(b)
}

fn mul_fix(a: i32, b: i32) -> i32 {
    ft::to_i32(ft::mul_fix(i64::from(a), i64::from(b)))
}

/// The work area of the decoder: the operand stack, the subroutine
/// stack, the storage, the hint state and the glyph path. The caller
/// owns it and may reuse it; each decode resets it.
#[derive(Clone, Debug)]
pub struct Work {
    stack: [Number; MAX_STACK],
    len: usize,
    storage: [i32; STORAGE_SIZE],
    hints: Hints,
    path: GlyphPath,
    random: u32,
    used_blend: bool,
    vsindex: u32,
    unknown: Option<(usize, u16)>,
    implicit_end: bool,
}

impl Work {
    /// Number of horizontal and vertical stems seen in the last decode.
    pub fn hint_stem_count(&self) -> usize {
        self.hints.total()
    }

    /// The first operator of the last decode that is reserved or belongs
    /// to the other version, with its position.
    pub fn unknown_operator(&self) -> Option<(usize, u16)> {
        self.unknown
    }

    /// Whether the last decoded CFF charstring ended without `endchar`.
    pub fn ended_without_endchar(&self) -> bool {
        self.implicit_end
    }
}

impl Default for Work {
    fn default() -> Work {
        Work {
            stack: [Number::Int(0); MAX_STACK],
            len: 0,
            storage: [0; STORAGE_SIZE],
            hints: Hints::default(),
            path: GlyphPath::default(),
            random: 0,
            used_blend: false,
            vsindex: 0,
            unknown: None,
            implicit_end: false,
        }
    }
}

/// The hint state FreeType keeps without hinting: it decides errors and
/// whether a zero-length line starts a new hint map.
#[derive(Clone, Copy, Debug, Default)]
struct Hints {
    h_stems: usize,
    v_stems: usize,
    mask_valid: bool,
    mask_new: bool,
    mask_bits: usize,
    initial_map_valid: bool,
}

impl Hints {
    fn total(&self) -> usize {
        self.h_stems.saturating_add(self.v_stems)
    }

    /// `cf2_hintmask_setCounts`.
    fn set_counts(&mut self, bits: usize) -> Result<(), CharStringErrorKind> {
        if bits > MAX_HINTS {
            return Err(CharStringErrorKind::TooManyHints);
        }
        self.mask_bits = bits;
        self.mask_valid = true;
        self.mask_new = true;
        Ok(())
    }

    /// `cf2_hintmap_build` for the glyph path's hint map; returns whether
    /// the map became valid.
    fn build(&mut self) -> Result<bool, CharStringErrorKind> {
        if !self.initial_map_valid {
            if self.total() > MAX_HINTS {
                return Err(CharStringErrorKind::TooManyHints);
            }
            self.initial_map_valid = true;
        }
        if !self.mask_valid {
            self.set_counts(self.total())?;
        }
        if self.h_stems > self.mask_bits {
            return Ok(false);
        }
        self.mask_new = false;
        Ok(true)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Elem {
    #[default]
    Line,
    Cube,
}

/// `CF2_GlyphPathRec` without hinting and darkening.
#[derive(Clone, Copy, Debug, Default)]
struct GlyphPath {
    move_pending: bool,
    open: bool,
    closing: bool,
    queued: bool,
    map_valid: bool,
    current: (i32, i32),
    start: (i32, i32),
    current_ds: (i32, i32),
    prev: Elem,
    prev_p: [(i32, i32); 4],
    start0: (i32, i32),
    start1: (i32, i32),
}

struct Run<'s, 'c> {
    ctx: &'s Context<'c>,
    work: &'s mut Work,
    sink: &'s mut dyn PathSink,
}

type Step<T> = Result<T, CharStringErrorKind>;

impl Run<'_, '_> {
    fn ds(&self, p: (i32, i32)) -> (i32, i32) {
        let s = self.ctx.scale;
        (mul_fix(s, p.0), mul_fix(p.1, s))
    }

    fn build_map(&mut self) -> Step<()> {
        if self.work.hints.build()? {
            self.work.path.map_valid = true;
        }
        Ok(())
    }

    /// `cf2_glyphpath_moveTo`.
    fn move_to(&mut self, x: i32, y: i32) -> Step<()> {
        self.close_open_path()?;
        let p = &mut self.work.path;
        p.current = (x, y);
        p.start = (x, y);
        p.move_pending = true;
        if !p.map_valid || self.work.hints.mask_new {
            self.build_map()?;
        }
        Ok(())
    }

    /// `cf2_glyphpath_pushMove`.
    fn push_move(&mut self, start: (i32, i32)) -> Step<()> {
        if !self.work.path.map_valid {
            let s = self.work.path.start;
            self.move_to(s.0, s.1)?;
        }
        let pt = self.ds(start);
        self.sink.move_to();
        self.work.path.current_ds = pt;
        self.work.path.start0 = start;
        Ok(())
    }

    /// `cf2_glyphpath_pushPrevElem` without offsets: the previous and the
    /// next element always join.
    fn push_prev(&mut self, next0: (i32, i32), close: bool) {
        let p = self.work.path;
        match p.prev {
            Elem::Line => {
                let pt1 = self.ds(p.prev_p[1]);
                if pt1 != p.current_ds {
                    self.sink.line_to(p.current_ds, pt1);
                    self.work.path.current_ds = pt1;
                }
            }
            Elem::Cube => {
                let (a, b, c) = (
                    self.ds(p.prev_p[1]),
                    self.ds(p.prev_p[2]),
                    self.ds(p.prev_p[3]),
                );
                self.sink.cubic_to(p.current_ds, a, b, c);
                self.work.path.current_ds = c;
            }
        }
        // No intersection is used, so the connecting line is always
        // considered.
        let _ = close;
        let pt1 = self.ds(next0);
        let cur = self.work.path.current_ds;
        if pt1 != cur {
            self.sink.line_to(cur, pt1);
            self.work.path.current_ds = pt1;
        }
    }

    /// `cf2_glyphpath_lineTo`.
    fn line_to(&mut self, x: i32, y: i32) -> Step<()> {
        let new_map = self.work.hints.mask_new && !self.work.path.closing;
        if self.work.path.current == (x, y) && !new_map {
            return Ok(());
        }
        let p0 = self.work.path.current;
        let p1 = (x, y);
        if self.work.path.move_pending {
            self.push_move(p0)?;
            let p = &mut self.work.path;
            p.move_pending = false;
            p.open = true;
            p.start1 = p1;
        }
        if self.work.path.queued {
            self.push_prev(p0, false);
        }
        let p = &mut self.work.path;
        p.queued = true;
        p.prev = Elem::Line;
        p.prev_p[0] = p0;
        p.prev_p[1] = p1;
        if new_map {
            self.build_map()?;
        }
        self.work.path.current = (x, y);
        Ok(())
    }

    /// `cf2_glyphpath_curveTo`.
    fn curve_to(&mut self, c1: (i32, i32), c2: (i32, i32), to: (i32, i32)) -> Step<()> {
        let p0 = self.work.path.current;
        if self.work.path.move_pending {
            self.push_move(p0)?;
            let p = &mut self.work.path;
            p.move_pending = false;
            p.open = true;
            p.start1 = c1;
        }
        if self.work.path.queued {
            self.push_prev(p0, false);
        }
        let p = &mut self.work.path;
        p.queued = true;
        p.prev = Elem::Cube;
        p.prev_p = [p0, c1, c2, to];
        if self.work.hints.mask_new {
            self.build_map()?;
        }
        self.work.path.current = to;
        Ok(())
    }

    /// `cf2_glyphpath_closeOpenPath`.
    fn close_open_path(&mut self) -> Step<()> {
        if self.work.path.open {
            self.work.path.closing = true;
            let s = self.work.path.start;
            self.line_to(s.0, s.1)?;
            if self.work.path.queued {
                let s0 = self.work.path.start0;
                self.push_prev(s0, true);
            }
            let p = &mut self.work.path;
            p.move_pending = true;
            p.open = false;
            p.closing = false;
            p.queued = false;
        }
        Ok(())
    }

    // The operand stack (`psstack.c`).

    fn push(&mut self, n: Number) -> Step<()> {
        let max = match self.ctx.version {
            Version::Cff => 48,
            Version::Cff2 => usize::try_from(self.ctx.max_stack).unwrap_or(MAX_STACK),
        }
        .min(MAX_STACK);
        if self.work.len >= max {
            return Err(CharStringErrorKind::StackOverflow);
        }
        if let Some(slot) = self.work.stack.get_mut(self.work.len) {
            *slot = n;
        }
        self.work.len = self.work.len.saturating_add(1);
        Ok(())
    }

    fn pop_int(&mut self) -> Step<i32> {
        let top = self
            .work
            .len
            .checked_sub(1)
            .ok_or(CharStringErrorKind::StackUnderflow)?;
        match self.work.stack.get(top) {
            Some(Number::Int(i)) => {
                let i = *i;
                self.work.len = top;
                Ok(i)
            }
            _ => Err(CharStringErrorKind::TypeMismatch),
        }
    }

    fn pop_fixed(&mut self) -> Step<i32> {
        let top = self
            .work
            .len
            .checked_sub(1)
            .ok_or(CharStringErrorKind::StackUnderflow)?;
        self.work.len = top;
        Ok(self.work.stack.get(top).map_or(0, |n| n.real()))
    }

    fn get(&self, i: usize) -> Step<i32> {
        if i >= self.work.len {
            return Err(CharStringErrorKind::StackUnderflow);
        }
        Ok(self.work.stack.get(i).map_or(0, |n| n.real()))
    }

    fn set(&mut self, i: usize, v: i32) -> Step<()> {
        if i >= self.work.len {
            return Err(CharStringErrorKind::StackUnderflow);
        }
        if let Some(slot) = self.work.stack.get_mut(i) {
            *slot = Number::Fixed(v);
        }
        Ok(())
    }

    fn count(&self) -> usize {
        self.work.len
    }

    fn clear(&mut self) {
        self.work.len = 0;
    }

    /// `cf2_stack_roll`.
    fn roll(&mut self, count: i32, shift: i32) -> Step<()> {
        if count < 2 {
            return Ok(());
        }
        let n = usize::try_from(count).unwrap_or(usize::MAX);
        if n > self.work.len {
            return Err(CharStringErrorKind::StackOverflow);
        }
        let offset = self.work.len.saturating_sub(n);
        let shift = if shift < 0 {
            shift
                .unsigned_abs()
                .checked_rem(count.unsigned_abs())
                .unwrap_or(0)
                .cast_signed()
                .wrapping_neg()
        } else {
            shift.checked_rem(count).unwrap_or(0)
        };
        if shift == 0 {
            return Ok(());
        }
        let slice = self
            .work
            .stack
            .get_mut(offset..offset.saturating_add(n))
            .ok_or(CharStringErrorKind::StackOverflow)?;
        let k = if shift > 0 {
            usize::try_from(shift).unwrap_or(0)
        } else {
            n.saturating_sub(usize::try_from(shift.unsigned_abs()).unwrap_or(0))
        };
        slice.rotate_right(k.checked_rem(n).unwrap_or(0));
        Ok(())
    }

    /// `cf2_doStems`.
    fn stems(&mut self, horizontal: bool, have_width: &mut bool, width: &mut i32) -> Step<()> {
        let count = self.count();
        let has_width = count & 1 == 1;
        if has_width && !*have_width {
            *width = add(
                self.get(0)?,
                int_to_fixed(ft::to_i32(self.ctx.nominal_width_x)),
            );
        }
        let pairs = count.saturating_sub(usize::from(has_width)) / 2;
        if horizontal {
            self.work.hints.h_stems = self.work.hints.h_stems.saturating_add(pairs);
        } else {
            self.work.hints.v_stems = self.work.hints.v_stems.saturating_add(pairs);
        }
        self.clear();
        *have_width = true;
        Ok(())
    }

    /// `cf2_doFlex`.
    fn flex(
        &mut self,
        cur: &mut (i32, i32),
        read: &[bool; 12],
        conditional_last: bool,
    ) -> Step<()> {
        let mut vals = [0i32; 14];
        vals[0] = cur.0;
        vals[1] = cur.1;
        let mut idx = 0usize;
        let is_h = !read[9];
        let top = if is_h { 9 } else { 10 };
        for i in 0..top {
            let mut v = vals.get(i).copied().unwrap_or(0);
            if read.get(i).copied().unwrap_or(false) {
                v = add(v, self.get(idx)?);
                idx = idx.saturating_add(1);
            }
            if let Some(slot) = vals.get_mut(i.saturating_add(2)) {
                *slot = v;
            }
        }
        if is_h {
            vals[11] = cur.1;
        }
        if conditional_last {
            let last_is_x = abs32(sub(vals[10], cur.0)) > abs32(sub(vals[11], cur.1));
            let last = self.get(idx)?;
            if last_is_x {
                vals[12] = add(vals[10], last);
                vals[13] = cur.1;
            } else {
                vals[12] = cur.0;
                vals[13] = add(vals[11], last);
            }
        } else {
            vals[12] = if read[10] {
                let v = add(vals[10], self.get(idx)?);
                idx = idx.saturating_add(1);
                v
            } else {
                cur.0
            };
            vals[13] = if read[11] {
                add(vals[11], self.get(idx)?)
            } else {
                cur.1
            };
        }
        for j in 0..2usize {
            let b = j.saturating_mul(6);
            let v = |k: usize| vals.get(b.saturating_add(k)).copied().unwrap_or(0);
            self.curve_to((v(2), v(3)), (v(4), v(5)), (v(6), v(7)))?;
        }
        self.clear();
        *cur = (vals[12], vals[13]);
        Ok(())
    }

    /// `cf2_doBlend`.
    fn blend(&mut self, n: u32) -> Step<()> {
        let vsindex = self.work.vsindex;
        let regions = self
            .ctx
            .blend
            .regions(vsindex)
            .ok_or(CharStringErrorKind::VsindexOutOfRange)?;
        let len_bv = usize::try_from(regions)
            .unwrap_or(usize::MAX)
            .saturating_add(1);
        let count = self.count();
        let n = usize::try_from(n).unwrap_or(usize::MAX);
        if n > count.checked_div(len_bv).unwrap_or(0) {
            return Err(CharStringErrorKind::StackUnderflow);
        }
        let operands = n.saturating_mul(len_bv);
        let base = count.saturating_sub(operands);
        let mut delta = base.saturating_add(n);
        for i in 0..n {
            let mut sum = self.get(base.saturating_add(i))?;
            for k in 0..regions {
                let w = self.ctx.blend.weight(vsindex, k);
                sum = add(sum, mul_fix(w, self.get(delta)?));
                delta = delta.saturating_add(1);
            }
            self.set(base.saturating_add(i), sum)?;
        }
        self.work.len = count.saturating_sub(operands.saturating_sub(n));
        Ok(())
    }
}

fn abs32(v: i32) -> i32 {
    if v < 0 { v.wrapping_neg() } else { v }
}

/// What a successful decode found besides the outline.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Decoded {
    /// The width, 16.16 (the advance when the font has no `hmtx`):
    /// `defaultWidthX`, or the width operand plus `nominalWidthX`. Always
    /// `defaultWidthX` in CFF2.
    pub width: i32,
}

/// Decodes `charstring` (a glyph of the font `ctx` describes) as
/// FreeType's Adobe engine does without hinting, sending the outline to
/// `sink`. `work` is reset first. On an error, what was sent stays sent.
pub fn decode(
    charstring: &[u8],
    ctx: &Context<'_>,
    work: &mut Work,
    sink: &mut dyn PathSink,
) -> Result<Decoded, CharStringError> {
    work.random = ctx.random_seed;
    work.used_blend = false;
    work.vsindex = ctx.vsindex;
    work.unknown = None;
    work.implicit_end = false;
    let mut run = Run { ctx, work, sink };
    let mut width = int_to_fixed(ft::to_i32(ctx.default_width_x));
    interpret(&mut run, charstring, false, (0, 0), &mut width)?;
    Ok(Decoded { width })
}

#[derive(Clone, Copy)]
struct Frame<'a> {
    data: &'a [u8],
    pos: usize,
}

/// `cf2_interpT2CharString`.
fn interpret(
    r: &mut Run<'_, '_>,
    charstring: &[u8],
    doing_seac: bool,
    origin: (i32, i32),
    width: &mut i32,
) -> Result<(), CharStringError> {
    // Fresh per call: stack, storage, hints and glyph path.
    r.work.len = 0;
    r.work.storage = [0; STORAGE_SIZE];
    r.work.hints = Hints::default();
    r.work.path = GlyphPath {
        move_pending: true,
        ..GlyphPath::default()
    };
    let cff2 = r.ctx.version == Version::Cff2;
    let mut have_width = cff2;
    let mut cur = origin;
    let mut frames: [Frame<'_>; MAX_SUBR + 1] = [Frame { data: &[], pos: 0 }; MAX_SUBR + 1];
    frames[0] = Frame {
        data: charstring,
        pos: 0,
    };
    let mut depth = 0usize;
    let mut limit: u32 = 20_000_000;
    let mut seac: Option<(i32, i32, i32, i32)> = None;
    let mut op_at = 0usize;
    let result = (|| -> Step<()> {
        loop {
            let frame = frames
                .get(depth)
                .copied()
                .unwrap_or(Frame { data: &[], pos: 0 });
            op_at = frame.pos;
            let mut tokens = CharStringTokens {
                data: frame.data,
                pos: frame.pos,
                version: r.ctx.version,
            };
            let op: u16 = if tokens.at_end() {
                if depth > 0 {
                    11
                } else {
                    if !cff2 && !doing_seac {
                        r.work.implicit_end = true;
                    }
                    14
                }
            } else {
                match tokens.next_token() {
                    Some(Ok(CharStringToken::Integer(v))) => {
                        save(&mut frames, depth, tokens.pos);
                        count_down(&mut limit)?;
                        r.push(Number::Int(v))?;
                        continue;
                    }
                    Some(Ok(CharStringToken::Fixed(v))) => {
                        save(&mut frames, depth, tokens.pos);
                        count_down(&mut limit)?;
                        r.push(Number::Fixed(v))?;
                        continue;
                    }
                    Some(Ok(CharStringToken::Operator(o))) => {
                        if !known(o, cff2) && r.work.unknown.is_none() {
                            r.work.unknown = Some((frame.pos, o));
                        }
                        if cff2 && (o == 11 || o == 14) { 0 } else { o }
                    }
                    Some(Err(_)) | None => return Err(CharStringErrorKind::Truncated),
                }
            };
            save(&mut frames, depth, tokens.pos);
            count_down(&mut limit)?;
            let mut clear = true;
            match op {
                // vsindex
                15 if cff2 => {
                    if r.work.used_blend {
                        return Err(CharStringErrorKind::VsindexAfterBlend);
                    }
                    let v = r.pop_int()?;
                    if v >= 0 {
                        r.work.vsindex = v.cast_unsigned();
                    }
                }
                // blend
                16 if cff2 => {
                    if !r.ctx.has_variations {
                        return Err(CharStringErrorKind::BlendWithoutStore);
                    }
                    r.ctx
                        .blend
                        .regions(r.work.vsindex)
                        .ok_or(CharStringErrorKind::VsindexOutOfRange)?;
                    let n = r.pop_int()?.cast_unsigned();
                    r.blend(n)?;
                    r.work.used_blend = true;
                    clear = false;
                }
                // hstem, hstemhm, vstem, vstemhm
                1 | 18 | 3 | 23 => {
                    if !r.work.hints.mask_valid {
                        r.stems(op == 1 || op == 18, &mut have_width, width)?;
                    }
                }
                // vmoveto
                4 => {
                    if r.count() > 1 && !have_width {
                        *width = add(r.get(0)?, int_to_fixed(ft::to_i32(r.ctx.nominal_width_x)));
                    }
                    have_width = true;
                    cur.1 = add(cur.1, r.pop_fixed()?);
                    r.move_to(cur.0, cur.1)?;
                }
                // rlineto
                5 => {
                    let n = r.count();
                    let mut i = 0usize;
                    while i < n {
                        cur.0 = add(cur.0, r.get(i)?);
                        cur.1 = add(cur.1, r.get(i.saturating_add(1))?);
                        r.line_to(cur.0, cur.1)?;
                        i = i.saturating_add(2);
                    }
                }
                // hlineto, vlineto
                6 | 7 => {
                    let mut is_x = op == 6;
                    for i in 0..r.count() {
                        let v = r.get(i)?;
                        if is_x {
                            cur.0 = add(cur.0, v);
                        } else {
                            cur.1 = add(cur.1, v);
                        }
                        is_x = !is_x;
                        r.line_to(cur.0, cur.1)?;
                    }
                }
                // rrcurveto, rcurveline
                8 | 24 => {
                    let n = r.count();
                    let mut i = 0usize;
                    while i.saturating_add(6) <= n {
                        curve6(r, &mut cur, i)?;
                        i = i.saturating_add(6);
                    }
                    if op == 24 {
                        cur.0 = add(cur.0, r.get(i)?);
                        cur.1 = add(cur.1, r.get(i.saturating_add(1))?);
                        r.line_to(cur.0, cur.1)?;
                    }
                }
                // callsubr, callgsubr
                10 | 29 => {
                    if depth >= MAX_SUBR {
                        return Err(CharStringErrorKind::SubroutineTooDeep);
                    }
                    let n = r.pop_int()?;
                    let (index, sorted) = if op == 29 {
                        (r.ctx.global_subrs, r.ctx.global_subrs_sorted)
                    } else {
                        (r.ctx.local_subrs, r.ctx.local_subrs_sorted)
                    };
                    let bias = if r.ctx.charstring_type == 1 {
                        0
                    } else {
                        super::subr_bias(index.len())
                    };
                    let i = n.wrapping_add(bias).cast_unsigned();
                    let data = index
                        .ft_pointer(i, sorted)
                        .ok_or(CharStringErrorKind::SubroutineOutOfRange)?;
                    depth = depth.saturating_add(1);
                    if let Some(f) = frames.get_mut(depth) {
                        *f = Frame { data, pos: 0 };
                    }
                    clear = false;
                }
                // return
                11 => {
                    if depth < 1 {
                        return Err(CharStringErrorKind::ReturnOutsideSubroutine);
                    }
                    depth = depth.saturating_sub(1);
                    clear = false;
                }
                // endchar
                14 => {
                    let n = r.count();
                    if (n == 1 || n == 5) && !have_width {
                        *width = add(r.get(0)?, int_to_fixed(ft::to_i32(r.ctx.nominal_width_x)));
                    }
                    r.close_open_path()?;
                    if !cff2 && r.count() > 1 {
                        if doing_seac {
                            return Err(CharStringErrorKind::NestedSeac);
                        }
                        let achar = r.pop_int()?;
                        let bchar = r.pop_int()?;
                        let y = r.pop_fixed()?;
                        let x = r.pop_fixed()?;
                        seac = Some((x, y, bchar, achar));
                    }
                    return Ok(());
                }
                // hintmask, cntrmask
                19 | 20 => {
                    if !(r.count() > 1 && r.work.hints.mask_valid) {
                        r.stems(false, &mut have_width, width)?;
                        let bits = r.work.hints.total();
                        if op == 19 {
                            r.work.hints.set_counts(bits)?;
                        } else {
                            // The counter mask is read into a temporary
                            // mask and map: only its errors and the
                            // initial hint map remain.
                            if bits > MAX_HINTS {
                                return Err(CharStringErrorKind::TooManyHints);
                            }
                            r.work.hints.initial_map_valid = true;
                        }
                        let bytes = bits.saturating_add(7) / 8;
                        let mut t = CharStringTokens {
                            data: frame.data,
                            pos: tokens.pos,
                            version: r.ctx.version,
                        };
                        t.skip(bytes).ok_or(CharStringErrorKind::Truncated)?;
                        save(&mut frames, depth, t.pos);
                    }
                }
                // rmoveto
                21 => {
                    if r.count() > 2 && !have_width {
                        *width = add(r.get(0)?, int_to_fixed(ft::to_i32(r.ctx.nominal_width_x)));
                    }
                    have_width = true;
                    cur.1 = add(cur.1, r.pop_fixed()?);
                    cur.0 = add(cur.0, r.pop_fixed()?);
                    r.move_to(cur.0, cur.1)?;
                }
                // hmoveto
                22 => {
                    if r.count() > 1 && !have_width {
                        *width = add(r.get(0)?, int_to_fixed(ft::to_i32(r.ctx.nominal_width_x)));
                    }
                    have_width = true;
                    cur.0 = add(cur.0, r.pop_fixed()?);
                    r.move_to(cur.0, cur.1)?;
                }
                // rlinecurve
                25 => {
                    let n = r.count();
                    let mut i = 0usize;
                    while i.saturating_add(6) < n {
                        cur.0 = add(cur.0, r.get(i)?);
                        cur.1 = add(cur.1, r.get(i.saturating_add(1))?);
                        r.line_to(cur.0, cur.1)?;
                        i = i.saturating_add(2);
                    }
                    while i < n {
                        curve6(r, &mut cur, i)?;
                        i = i.saturating_add(6);
                    }
                }
                // vvcurveto, hhcurveto
                26 | 27 => {
                    let n1 = r.count();
                    let n = n1 & !2usize;
                    let mut i = n1.saturating_sub(n);
                    let vertical = op == 26;
                    while i < n {
                        let (mut x1, mut y1) = cur;
                        if (n.saturating_sub(i)) & 1 == 1 {
                            if vertical {
                                x1 = add(r.get(i)?, cur.0);
                            } else {
                                y1 = add(r.get(i)?, cur.1);
                            }
                            i = i.saturating_add(1);
                        }
                        let g = |k: usize| r.get(i.saturating_add(k));
                        let (c1, c2, to) = if vertical {
                            let y1 = add(g(0)?, cur.1);
                            let x2 = add(g(1)?, x1);
                            let y2 = add(g(2)?, y1);
                            ((x1, y1), (x2, y2), (x2, add(g(3)?, y2)))
                        } else {
                            let x1 = add(g(0)?, cur.0);
                            let x2 = add(g(1)?, x1);
                            let y2 = add(g(2)?, y1);
                            ((x1, y1), (x2, y2), (add(g(3)?, x2), y2))
                        };
                        r.curve_to(c1, c2, to)?;
                        cur = to;
                        i = i.saturating_add(4);
                    }
                }
                // vhcurveto, hvcurveto
                30 | 31 => {
                    let n1 = r.count();
                    let n = n1 & !2usize;
                    let mut i = n1.saturating_sub(n);
                    let mut alternate = op == 31;
                    while i < n {
                        let g = |k: usize| r.get(i.saturating_add(k));
                        let last = n.saturating_sub(i) == 5;
                        let (c1, c2, to);
                        if alternate {
                            let x1 = add(g(0)?, cur.0);
                            let y1 = cur.1;
                            let x2 = add(g(1)?, x1);
                            let y2 = add(g(2)?, y1);
                            let y3 = add(g(3)?, y2);
                            let x3 = if last { add(g(4)?, x2) } else { x2 };
                            (c1, c2, to) = ((x1, y1), (x2, y2), (x3, y3));
                        } else {
                            let x1 = cur.0;
                            let y1 = add(g(0)?, cur.1);
                            let x2 = add(g(1)?, x1);
                            let y2 = add(g(2)?, y1);
                            let x3 = add(g(3)?, x2);
                            let y3 = if last { add(g(4)?, y2) } else { y2 };
                            (c1, c2, to) = ((x1, y1), (x2, y2), (x3, y3));
                        }
                        if last {
                            i = i.saturating_add(1);
                        }
                        alternate = !alternate;
                        r.curve_to(c1, c2, to)?;
                        cur = to;
                        i = i.saturating_add(4);
                    }
                }
                o if o >= 0x0C00 => {
                    clear = escape(r, o & 0xFF, &mut cur, cff2)?;
                }
                // Reserved and Type 1 operators, and in CFF vsindex and
                // blend: the stack is cleared.
                _ => {}
            }
            if clear {
                r.clear();
            }
        }
    })();
    let at = op_at;
    let err = |kind| CharStringError {
        kind,
        position: at,
        depth: u8::try_from(depth).unwrap_or(u8::MAX),
    };
    result.map_err(err)?;
    if let Some((x, y, bchar, achar)) = seac {
        let accent = seac_component(r.ctx, achar).map_err(err)?;
        let mut dummy = 0;
        interpret(r, accent, true, (x, y), &mut dummy)?;
        let base = seac_component(r.ctx, bchar).map_err(err)?;
        interpret(r, base, true, (0, 0), &mut dummy)?;
    }
    Ok(())
}

fn save(frames: &mut [Frame<'_>], depth: usize, pos: usize) {
    if let Some(f) = frames.get_mut(depth) {
        f.pos = pos;
    }
}

fn count_down(limit: &mut u32) -> Step<()> {
    *limit = limit.saturating_sub(1);
    if *limit == 0 {
        return Err(CharStringErrorKind::InstructionLimit);
    }
    Ok(())
}

fn curve6(r: &mut Run<'_, '_>, cur: &mut (i32, i32), i: usize) -> Step<()> {
    let g = |k: usize| r.get(i.saturating_add(k));
    let x1 = add(g(0)?, cur.0);
    let y1 = add(g(1)?, cur.1);
    let x2 = add(g(2)?, x1);
    let y2 = add(g(3)?, y1);
    let x3 = add(g(4)?, x2);
    let y3 = add(g(5)?, y2);
    r.curve_to((x1, y1), (x2, y2), (x3, y3))?;
    *cur = (x3, y3);
    Ok(())
}

/// `cff_lookup_glyph_by_stdcharcode` and `cff_get_glyph_data`.
fn seac_component<'a>(ctx: &Context<'a>, code: i32) -> Step<&'a [u8]> {
    let missing = CharStringErrorKind::SeacComponentMissing;
    let charset = ctx.charset.ok_or(missing)?;
    let code = usize::try_from(code).map_err(|_| missing)?;
    let sid = *STANDARD_ENCODING.get(code).ok_or(missing)?;
    let glyph = charset.glyph(sid).ok_or(missing)?;
    ctx.char_strings.ft_element(glyph).ok_or(missing)
}

const HFLEX: [bool; 12] = [
    true, false, true, true, true, false, true, false, true, false, true, false,
];
const FLEX: [bool; 12] = [true; 12];
const HFLEX1: [bool; 12] = [
    true, true, true, true, true, false, true, false, true, true, true, false,
];
const FLEX1: [bool; 12] = [
    true, true, true, true, true, true, true, true, true, true, false, false,
];

/// The two-byte operators; returns whether the stack is cleared.
fn escape(r: &mut Run<'_, '_>, op2: u16, cur: &mut (i32, i32), cff2: bool) -> Step<bool> {
    match op2 {
        34 => {
            r.flex(cur, &HFLEX, false)?;
            return Ok(false);
        }
        35 => {
            r.flex(cur, &FLEX, false)?;
            return Ok(true);
        }
        36 => {
            r.flex(cur, &HFLEX1, false)?;
            return Ok(false);
        }
        37 => {
            r.flex(cur, &FLEX1, true)?;
            return Ok(false);
        }
        8 | 13 | 19 | 25 | 31 | 32 => return Ok(true),
        _ => {}
    }
    if cff2 || op2 >= 38 {
        return Ok(true);
    }
    match op2 {
        // and, or
        3 | 4 => {
            let b = r.pop_fixed()?;
            let a = r.pop_fixed()?;
            let v = if op2 == 3 {
                a != 0 && b != 0
            } else {
                a != 0 || b != 0
            };
            r.push(Number::Int(i32::from(v)))?;
        }
        // not
        5 => {
            let a = r.pop_fixed()?;
            r.push(Number::Int(i32::from(a == 0)))?;
        }
        // abs
        9 => {
            let a = r.pop_fixed()?;
            r.push(Number::Fixed(if a == i32::MIN {
                i32::MAX
            } else {
                a.abs()
            }))?;
        }
        // add, sub
        10 | 11 => {
            let b = r.pop_fixed()?;
            let a = r.pop_fixed()?;
            r.push(Number::Fixed(if op2 == 10 { add(a, b) } else { sub(a, b) }))?;
        }
        // div
        12 => {
            let b = r.pop_fixed()?;
            let a = r.pop_fixed()?;
            r.push(Number::Fixed(ft::to_i32(ft::div_fix(
                i64::from(a),
                i64::from(b),
            ))))?;
        }
        // neg
        14 => {
            let a = r.pop_fixed()?;
            r.push(Number::Fixed(if a == i32::MIN {
                i32::MAX
            } else {
                a.wrapping_neg()
            }))?;
        }
        // eq
        15 => {
            let b = r.pop_fixed()?;
            let a = r.pop_fixed()?;
            r.push(Number::Int(i32::from(a == b)))?;
        }
        // drop
        18 => {
            r.pop_fixed()?;
        }
        // put
        20 => {
            let i = r.pop_int()?;
            let v = r.pop_fixed()?;
            if let Some(slot) = usize::try_from(i.cast_unsigned())
                .ok()
                .and_then(|i| r.work.storage.get_mut(i))
            {
                *slot = v;
            }
        }
        // get
        21 => {
            let i = r.pop_int()?;
            if let Some(v) = usize::try_from(i.cast_unsigned())
                .ok()
                .and_then(|i| r.work.storage.get(i))
                .copied()
            {
                r.push(Number::Fixed(v))?;
            }
        }
        // ifelse
        22 => {
            let c2 = r.pop_fixed()?;
            let c1 = r.pop_fixed()?;
            let b = r.pop_fixed()?;
            let a = r.pop_fixed()?;
            r.push(Number::Fixed(if c1 <= c2 { a } else { b }))?;
        }
        // random
        23 => {
            let v = ((r.work.random & 0xFFFF).saturating_add(1)).cast_signed();
            r.work.random = random_next(r.work.random);
            r.push(Number::Fixed(v))?;
        }
        // mul
        24 => {
            let b = r.pop_fixed()?;
            let a = r.pop_fixed()?;
            r.push(Number::Fixed(mul_fix(a, b)))?;
        }
        // sqrt
        26 => {
            let a = r.pop_fixed()?;
            let v = if a > 0 {
                ft::sqrt_fixed(a.cast_unsigned()).cast_signed()
            } else {
                0
            };
            r.push(Number::Fixed(v))?;
        }
        // dup
        27 => {
            let a = r.pop_fixed()?;
            r.push(Number::Fixed(a))?;
            r.push(Number::Fixed(a))?;
        }
        // exch
        28 => {
            let b = r.pop_fixed()?;
            let a = r.pop_fixed()?;
            r.push(Number::Fixed(b))?;
            r.push(Number::Fixed(a))?;
        }
        // index
        29 => {
            let i = r.pop_int()?;
            let size = r.count();
            if size > 0 {
                let k = if i < 0 {
                    size.saturating_sub(1)
                } else {
                    let i = usize::try_from(i).unwrap_or(usize::MAX);
                    if i >= size {
                        0
                    } else {
                        size.saturating_sub(1).saturating_sub(i)
                    }
                };
                let v = r.get(k)?;
                r.push(Number::Fixed(v))?;
            }
        }
        // roll
        30 => {
            let shift = r.pop_int()?;
            let count = r.pop_int()?;
            r.roll(count, shift)?;
        }
        // dotsection and the Type 1 operators.
        _ => return Ok(true),
    }
    Ok(false)
}

/// Whether `op` is a Type 2 operator of the version (CFF or CFF2).
fn known(op: u16, cff2: bool) -> bool {
    if op >= 0x0C00 {
        let e = op & 0xFF;
        return matches!(e, 34..=37)
            || (!cff2
                && matches!(
                    e,
                    0 | 3 | 4 | 5 | 9 | 10 | 11 | 12 | 14 | 15 | 18 | 20..=24 | 26..=30
                ));
    }
    match op {
        1 | 3..=8 | 10 | 18..=27 | 29..=31 => true,
        11 | 14 => !cff2,
        15 | 16 => cff2,
        _ => false,
    }
}

/// FreeType's `cff_random`: a 32-bit xorshift.
pub fn random_next(r: u32) -> u32 {
    let r = r ^ (r << 13);
    let r = r ^ (r >> 17);
    r ^ (r << 5)
}
