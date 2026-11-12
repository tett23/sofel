//! CFF2 charstrings as HarfBuzz's subsetter interprets them when it pins
//! every axis (`src/hb-cff2-interp-cs.hh`, `src/hb-cff-interp-cs-common.hh`,
//! `src/hb-subset-cff2.cc`): the interpreter with `blend` and `vsindex`
//! and the operator sets the instancing path runs it with, the usage pass
//! (`cff2_cs_opset_usage_t`), the flattener (`cff2_cs_opset_flatten_t`
//! with the rounding of `src/hb-subset-cff2-round.hh` and the command
//! capture for the conversion to CFF) and the glyph boxes
//! (`cff2_cs_opset_extents_t` of `src/hb-ot-cff2-table.cc`).
//!
//! Numbers are `f64`, as HarfBuzz's `number_t`; any error fails the whole
//! interpretation, as in HarfBuzz.

use alloc::vec::Vec;

use sofel_font::read::be::{u16_at, u32_at};
use sofel_font::read::cff::hb::HbIndex;
use sofel_geom::float::{floor, i32_to_f32};

use super::cs::{
    CALLGSUBR, CALLSUBR, CNTRMASK, DOTSECTION, ENDCHAR, FIXEDCS, HINTMASK, HMOVETO, HSTEM, HSTEMHM,
    RETURN, RMOVETO, SHORTINT, VMOVETO, VSTEM, VSTEMHM, bias, encode_int, encode_num_cs, encode_op,
};

/// `HB_CFF_MAX_OPS`.
const MAX_OPS: u32 = 200_000;
/// `kMaxCallLimit`.
const MAX_CALLS: usize = 10;
/// The argument stack limit.
const MAX_ARGS: usize = 513;
/// `OpCode_Invalid`.
pub(crate) const INVALID: u32 = 0xFFFF;

pub(crate) const RLINETO: u32 = 5;
pub(crate) const HLINETO: u32 = 6;
pub(crate) const VLINETO: u32 = 7;
pub(crate) const RRCURVETO: u32 = 8;
pub(crate) const VSINDEXCS: u32 = 15;
pub(crate) const BLENDCS: u32 = 16;
pub(crate) const RCURVELINE: u32 = 24;
pub(crate) const RLINECURVE: u32 = 25;
pub(crate) const VVCURVETO: u32 = 26;
pub(crate) const HHCURVETO: u32 = 27;
pub(crate) const VHCURVETO: u32 = 30;
pub(crate) const HVCURVETO: u32 = 31;
pub(crate) const HFLEX: u32 = 256 + 34;
pub(crate) const FLEX: u32 = 256 + 35;
pub(crate) const HFLEX1: u32 = 256 + 36;
pub(crate) const FLEX1: u32 = 256 + 37;

/// C's `round`: halves away from zero.
pub(crate) fn c_round(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    let t = if v < 0.0 { -floor(-v) } else { floor(v) };
    if (v - t).abs() >= 0.5 {
        t + 1.0f64.copysign(v)
    } else {
        t
    }
}

/// Whether `op` is a hint operator of a charstring.
fn is_hint(op: u32) -> bool {
    matches!(op, HSTEM | HSTEMHM | VSTEM | VSTEMHM | HINTMASK | CNTRMASK)
}

/// A CFF2 Variation Store as `blend` reads it
/// (`CFF2ItemVariationStore`): the position of the table's length field,
/// or none.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Store<'a> {
    pub data: &'a [u8],
    pub at: Option<usize>,
}

impl Store<'_> {
    /// The ItemVariationStore.
    fn vs(&self) -> Option<usize> {
        self.at.map(|p| p.saturating_add(2))
    }

    /// `size`: the length field.
    pub fn size(&self) -> u16 {
        self.at.and_then(|p| u16_at(self.data, p)).unwrap_or(0)
    }

    /// The ItemVariationData `ivs`, if any.
    fn var_data(&self, ivs: u32) -> Option<usize> {
        let vs = self.vs()?;
        let n = u32::from(u16_at(self.data, vs.saturating_add(6))?);
        if ivs >= n {
            return None;
        }
        let o = u32_at(
            self.data,
            vs.saturating_add(8)
                .saturating_add((ivs as usize).saturating_mul(4)),
        )?;
        (o != 0).then(|| vs.saturating_add(o as usize))
    }

    /// `get_region_index_count`.
    pub fn region_count(&self, ivs: u32) -> u32 {
        self.var_data(ivs)
            .and_then(|d| u16_at(self.data, d.saturating_add(4)))
            .map_or(0, u32::from)
    }

    /// `VarRegionList::evaluate` of region `r` at `coords`.
    fn evaluate(&self, r: u32, coords: &[i32]) -> f32 {
        let Some(vs) = self.vs() else {
            return 0.0;
        };
        let off = u32_at(self.data, vs.saturating_add(2)).unwrap_or(0);
        if off == 0 {
            return 0.0;
        }
        let list = vs.saturating_add(off as usize);
        let axes = usize::from(u16_at(self.data, list).unwrap_or(0));
        let count = u32::from(u16_at(self.data, list.saturating_add(2)).unwrap_or(0) & 0x7FFF);
        if r >= count {
            return 0.0;
        }
        let first = list
            .saturating_add(4)
            .saturating_add((r as usize).saturating_mul(axes).saturating_mul(6));
        let mut v = 1.0f32;
        for i in 0..axes {
            let at = first.saturating_add(i.saturating_mul(6));
            let get = |k: usize| {
                u16_at(self.data, at.saturating_add(k))
                    .map_or(0, |x| i32::from(i16::from_ne_bytes(x.to_ne_bytes())))
            };
            let coord = coords.get(i).copied().unwrap_or(0);
            let factor = axis_factor(get(0), get(2), get(4), coord);
            if factor == 0.0 {
                return 0.0;
            }
            v *= factor;
        }
        v
    }

    /// `get_region_scalars` of ItemVariationData `ivs`: `n` scalars at
    /// `coords`.
    pub fn scalars(&self, ivs: u32, coords: &[i32], n: u32) -> Vec<f32> {
        let d = self.var_data(ivs);
        let indexes = d.map_or(0, |d| {
            u32::from(u16_at(self.data, d.saturating_add(4)).unwrap_or(0))
        });
        (0..n)
            .map(|i| match d {
                Some(d) if i < indexes => {
                    let at = d
                        .saturating_add(6)
                        .saturating_add((i as usize).saturating_mul(2));
                    let r = u32::from(u16_at(self.data, at).unwrap_or(0));
                    self.evaluate(r, coords)
                }
                _ => 0.0,
            })
            .collect()
    }
}

/// `VarRegionAxis::evaluate`.
fn axis_factor(start: i32, peak: i32, end: i32, coord: i32) -> f32 {
    if peak == 0 || coord == peak {
        return 1.0;
    }
    if coord == 0 {
        return 0.0;
    }
    if start > peak || peak > end {
        return 1.0;
    }
    if start < 0 && end > 0 {
        return 1.0;
    }
    if coord <= start || end <= coord {
        return 0.0;
    }
    if coord < peak {
        i32_to_f32(coord.saturating_sub(start)) / i32_to_f32(peak.saturating_sub(start))
    } else {
        i32_to_f32(end.saturating_sub(coord)) / i32_to_f32(end.saturating_sub(peak))
    }
}

/// An operand (`blend_arg_t`): the value and, after `blend` without
/// coordinates, its deltas.
#[derive(Clone, Debug, Default)]
pub(crate) struct Arg {
    pub v: f64,
    pub deltas: Vec<f64>,
    pub num_values: u32,
    pub value_index: u32,
}

impl Arg {
    fn num(v: f64) -> Arg {
        Arg {
            v,
            ..Arg::default()
        }
    }

    pub fn blending(&self) -> bool {
        !self.deltas.is_empty()
    }

    /// `set_real`: the value, without deltas.
    pub fn set_real(&mut self, v: f64) {
        self.v = v;
        self.deltas.clear();
        self.num_values = 0;
        self.value_index = 0;
    }
}

/// Which subroutines a call takes (`cs_type_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Global,
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Source {
    Glyph,
    Global(u32),
    Local(u32),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CallTrace {
    pub source: Source,
    pub start: usize,
    pub end: usize,
    pub target: Source,
    caller_arguments: Option<(usize, usize)>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MaskTrace {
    pub source: Source,
    pub at: usize,
    pub size: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HintTrace {
    pub source: Source,
    pub start: usize,
    pub end: usize,
    pub can_follow_call: bool,
}

/// A program and the position in it; an offset past the end is the error
/// state.
#[derive(Clone, Copy, Debug)]
struct Ctx<'a> {
    bytes: &'a [u8],
    off: usize,
    source: Source,
}

/// The fonts' parts an interpreter reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Programs<'a> {
    pub gsubrs: HbIndex<'a>,
    pub lsubrs: HbIndex<'a>,
    pub store: Store<'a>,
    /// The Private DICT's `vsindex`.
    pub ivs: u32,
}

/// HarfBuzz's CFF2 interpreter state (`cff2_cs_interp_env_t`).
pub(crate) struct Env<'a, 'c> {
    ctx: Ctx<'a>,
    calls: Vec<Ctx<'a>>,
    call_error: bool,
    called_subr: bool,
    trace_calls: bool,
    trace_drop_hints: bool,
    trace_valid: bool,
    last_literal: Option<(Source, usize, usize)>,
    fetched_explicit: bool,
    call_trace: Vec<CallTrace>,
    mask_trace: Vec<MaskTrace>,
    hint_trace: Vec<HintTrace>,
    literal_run: Option<(Source, usize, usize, usize)>,
    path_started: bool,
    pub args: Vec<Arg>,
    arg_error: bool,
    endchar: bool,
    seen_moveto: bool,
    seen_hintmask: bool,
    hstem_count: u32,
    vstem_count: u32,
    pub hintmask_size: u32,
    p: Programs<'a>,
    coords: &'c [i32],
    do_blend: bool,
    scalars: Vec<f32>,
    region_count: u32,
    ivs: u32,
    seen_vsindex: bool,
    seen_blend: bool,
    /// `round_blends`.
    pub round_blends: bool,
    /// The operands are `number_t` (the glyph boxes), not `blend_arg_t`.
    numbers: bool,
    /// The current point.
    pt: (f64, f64),
}

impl<'a, 'c> Env<'a, 'c> {
    /// An interpreter for `charstring` with `coords` (none: blends keep
    /// their deltas).
    pub fn new(charstring: &'a [u8], p: Programs<'a>, coords: &'c [i32]) -> Env<'a, 'c> {
        Env {
            ctx: Ctx {
                bytes: charstring,
                off: 0,
                source: Source::Glyph,
            },
            calls: Vec::new(),
            call_error: false,
            called_subr: false,
            trace_calls: false,
            trace_drop_hints: false,
            trace_valid: true,
            last_literal: None,
            fetched_explicit: false,
            call_trace: Vec::new(),
            mask_trace: Vec::new(),
            hint_trace: Vec::new(),
            literal_run: None,
            path_started: false,
            args: Vec::new(),
            arg_error: false,
            endchar: false,
            seen_moveto: true,
            seen_hintmask: false,
            hstem_count: 0,
            vstem_count: 0,
            hintmask_size: 0,
            p,
            coords,
            do_blend: !coords.is_empty() && p.store.size() != 0,
            scalars: Vec::new(),
            region_count: 0,
            ivs: p.ivs,
            seen_vsindex: false,
            seen_blend: false,
            round_blends: true,
            numbers: false,
            pt: (0.0, 0.0),
        }
    }

    fn in_error(&self) -> bool {
        self.call_error || self.arg_error || self.ctx.off > self.ctx.bytes.len()
    }

    pub(crate) fn called_subr(&self) -> bool {
        self.called_subr
    }

    pub(crate) fn enable_call_trace(&mut self, drop_hints: bool) {
        self.trace_calls = true;
        self.trace_drop_hints = drop_hints;
    }

    pub(crate) fn call_trace(&self) -> Option<&[CallTrace]> {
        self.trace_valid.then_some(&self.call_trace)
    }

    pub(crate) fn mask_trace(&self) -> &[MaskTrace] {
        &self.mask_trace
    }

    pub(crate) fn hint_trace(&self) -> &[HintTrace] {
        &self.hint_trace
    }

    fn record_literal(&mut self, start: usize) {
        if !self.trace_calls {
            return;
        }
        self.last_literal = Some((self.ctx.source, start, self.ctx.off));
        if !self.trace_drop_hints {
            return;
        }
        self.literal_run = match self.literal_run {
            Some((source, first, end, count)) if source == self.ctx.source && end == start => {
                Some((source, first, self.ctx.off, count.saturating_add(1)))
            }
            _ => Some((self.ctx.source, start, self.ctx.off, 1)),
        };
    }

    fn record_hint(&mut self, end: usize, mask: bool) {
        if !self.trace_drop_hints {
            return;
        }
        let independent_mask = mask && self.path_started && self.args.is_empty();
        if (self.path_started && !independent_mask) || !self.args.len().is_multiple_of(2) {
            self.trace_valid = false;
        }
        let start = if self.args.is_empty() {
            self.ctx.off.saturating_sub(1)
        } else {
            match self.literal_run {
                Some((source, start, _, count))
                    if source == self.ctx.source && count == self.args.len() =>
                {
                    start
                }
                _ if self.ctx.off == 1 => {
                    // An initial stem can consume literals pushed before the call.
                    let Some(call) = self
                        .call_trace
                        .last()
                        .filter(|c| c.target == self.ctx.source)
                    else {
                        self.trace_valid = false;
                        return;
                    };
                    let Some((start, _)) = call
                        .caller_arguments
                        .filter(|&(_, count)| count == self.args.len())
                    else {
                        self.trace_valid = false;
                        return;
                    };
                    self.hint_trace.push(HintTrace {
                        source: call.source,
                        start,
                        end: call.start,
                        can_follow_call: false,
                    });
                    0
                }
                _ => {
                    self.trace_valid = false;
                    return;
                }
            }
        };
        self.hint_trace.push(HintTrace {
            source: self.ctx.source,
            start,
            end,
            can_follow_call: independent_mask,
        });
    }

    pub fn set_error(&mut self) {
        self.ctx.off = self.ctx.bytes.len().saturating_add(1);
    }

    fn avail(&self, n: usize) -> bool {
        self.ctx.off.saturating_add(n) <= self.ctx.bytes.len()
    }

    fn inc(&mut self, n: usize) {
        self.ctx.off = self.ctx.off.saturating_add(n);
    }

    /// `str_ref[i]`: in error past the end.
    pub fn byte(&mut self, i: usize) -> u8 {
        if let Some(&b) = self.ctx.bytes.get(self.ctx.off.saturating_add(i)) {
            b
        } else {
            self.set_error();
            0
        }
    }

    /// `fetch_op`, with the made-up `return` and `endchar` at the end of a
    /// string.
    fn fetch_op(&mut self) -> u32 {
        let Some(&b) = self.ctx.bytes.get(self.ctx.off) else {
            self.fetched_explicit = false;
            return if self.calls.is_empty() {
                ENDCHAR
            } else {
                RETURN
            };
        };
        self.fetched_explicit = true;
        self.inc(1);
        if b != 12 {
            return u32::from(b);
        }
        let Some(&e) = self.ctx.bytes.get(self.ctx.off) else {
            return INVALID;
        };
        self.inc(1);
        u32::from(e).saturating_add(256)
    }

    fn push(&mut self, v: f64) {
        if self.args.len() < MAX_ARGS {
            self.args.push(Arg::num(v));
        } else {
            self.arg_error = true;
        }
    }

    fn pop(&mut self) -> f64 {
        self.args.pop().map_or_else(
            || {
                self.arg_error = true;
                0.0
            },
            |a| a.v,
        )
    }

    fn pop_int(&mut self) -> i32 {
        let v = self.pop();
        if v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX) {
            super::cs::to_int(v)
        } else {
            self.arg_error = true;
            0
        }
    }

    fn pop_uint(&mut self) -> u32 {
        let i = self.pop_int();
        u32::try_from(i).unwrap_or_else(|_| {
            self.arg_error = true;
            0
        })
    }

    /// `eval_arg`: an error past the top.
    fn arg(&mut self, i: usize) -> f64 {
        match self.args.get(i) {
            Some(a) => a.v,
            None => {
                self.arg_error = true;
                0.0
            }
        }
    }

    pub fn clear_args(&mut self) {
        self.args.clear();
    }

    fn pop_n_args(&mut self, n: usize) {
        if self.args.len() >= n {
            self.args.truncate(self.args.len().saturating_sub(n));
        } else {
            self.arg_error = true;
        }
    }

    fn call_subr(&mut self, kind: Kind) {
        self.called_subr = true;
        let subrs = if kind == Kind::Local {
            self.p.lsubrs
        } else {
            self.p.gsubrs
        };
        let n = i64::from(self.pop_int()).saturating_add(i64::from(bias(subrs.len())));
        let num = u32::try_from(n).ok().filter(|&n| n < subrs.len());
        let Some(num) = num.filter(|_| self.calls.len() < MAX_CALLS) else {
            self.set_error();
            return;
        };
        let target = if kind == Kind::Local {
            Source::Local(num)
        } else {
            Source::Global(num)
        };
        if self.trace_calls {
            let op_start = self.ctx.off.saturating_sub(1);
            match self.last_literal {
                Some((source, start, end)) if source == self.ctx.source && end == op_start => {
                    self.call_trace.push(CallTrace {
                        source,
                        start,
                        end: self.ctx.off,
                        target,
                        caller_arguments: self.literal_run.and_then(
                            |(owner, first, end, count)| {
                                (owner == source && end == op_start && count > 1)
                                    .then_some((first, count.saturating_sub(1)))
                            },
                        ),
                    });
                }
                _ => self.trace_valid = false,
            }
        }
        self.calls.push(self.ctx);
        self.ctx = Ctx {
            bytes: subrs.get(num).unwrap_or(&[]),
            off: 0,
            source: target,
        };
    }

    fn return_from_subr(&mut self) {
        if self.trace_drop_hints && self.fetched_explicit {
            self.hint_trace.push(HintTrace {
                source: self.ctx.source,
                start: self.ctx.off.saturating_sub(1),
                end: self.ctx.off,
                can_follow_call: true,
            });
        }
        if self.trace_calls && self.ctx.off < self.ctx.bytes.len() {
            self.trace_valid = false;
        }
        // HarfBuzz marks the returning program in error when it is, and
        // then replaces it with the caller's.
        match self.calls.pop() {
            Some(c) => self.ctx = c,
            None => {
                self.call_error = true;
                self.ctx = Ctx {
                    bytes: &[],
                    off: 0,
                    source: Source::Glyph,
                };
            }
        }
    }

    fn determine_hintmask_size(&mut self) {
        if !self.seen_hintmask {
            let n = u32::try_from(self.args.len() / 2).unwrap_or(u32::MAX);
            self.vstem_count = self.vstem_count.saturating_add(n);
            self.hintmask_size = self
                .hstem_count
                .saturating_add(self.vstem_count)
                .saturating_add(7)
                >> 3;
            self.seen_hintmask = true;
        }
    }

    /// `opset_t::process_op`: numbers are pushed, unknown operators clear
    /// the arguments.
    fn process_number(&mut self, op: u32) {
        let start = self.ctx.off.saturating_sub(1);
        match op {
            SHORTINT => {
                let v = u16::from_be_bytes([self.byte(0), self.byte(1)]);
                self.push(f64::from(i16::from_ne_bytes(v.to_ne_bytes())));
                self.inc(2);
            }
            247..=250 => {
                let b = i32::from(self.byte(0));
                let v = i32::try_from(op.saturating_sub(247))
                    .unwrap_or(0)
                    .saturating_mul(256)
                    .saturating_add(b)
                    .saturating_add(108);
                self.push(f64::from(v));
                self.inc(1);
            }
            251..=254 => {
                let b = i32::from(self.byte(0));
                let v = 0i32
                    .saturating_sub(i32::try_from(op.saturating_sub(251)).unwrap_or(0))
                    .saturating_mul(256)
                    .saturating_sub(b)
                    .saturating_sub(108);
                self.push(f64::from(v));
                self.inc(1);
            }
            32..=246 => {
                self.push(f64::from(
                    i32::try_from(op).unwrap_or(0).saturating_sub(139),
                ));
            }
            _ => self.clear_args(),
        }
        if matches!(op, SHORTINT | 247..=254 | 32..=246) {
            self.record_literal(start);
        }
    }

    /// `push_fixed_from_substr`.
    fn push_fixed(&mut self) {
        let start = self.ctx.off.saturating_sub(1);
        if !self.avail(4) {
            return;
        }
        let v = u32::from_be_bytes([self.byte(0), self.byte(1), self.byte(2), self.byte(3)]);
        self.push(f64::from(i32::from_ne_bytes(v.to_ne_bytes())) / 65536.0);
        self.inc(4);
        self.record_literal(start);
    }

    /// `cff2_cs_interp_env_t::process_blend`.
    fn begin_blend(&mut self) {
        if !self.seen_blend {
            self.region_count = self.p.store.region_count(self.ivs);
            if self.do_blend {
                self.scalars = self
                    .p
                    .store
                    .scalars(self.ivs, self.coords, self.region_count);
            }
            self.seen_blend = true;
        }
    }

    /// `process_vsindex`.
    fn process_vsindex(&mut self) {
        let index = self.pop_uint();
        if self.seen_vsindex || self.seen_blend {
            self.set_error();
        } else {
            self.ivs = index;
        }
        self.seen_vsindex = true;
    }

    /// `blend_deltas`.
    fn blend_deltas(&self, deltas: &[Arg]) -> f64 {
        if !self.do_blend || self.scalars.len() != deltas.len() {
            return 0.0;
        }
        self.scalars
            .iter()
            .zip(deltas)
            .fold(0.0, |v, (&s, d)| v + f64::from(s) * d.v)
    }

    /// `process_arg_blend` of the operand at `at` with `blends`.
    fn process_arg_blend(&mut self, at: usize, blends: &[Arg], n: u32, i: u32) {
        let numbers = self.numbers;
        let have_coords = !self.coords.is_empty();
        let round = self.round_blends;
        let d = self.blend_deltas(blends);
        let mut error = false;
        if let Some(arg) = self.args.get_mut(at) {
            if numbers || have_coords {
                let v = arg.v + d;
                arg.set_real(if !numbers && round { c_round(v) } else { v });
            } else {
                let inner = core::mem::take(&mut arg.deltas);
                arg.num_values = n;
                arg.value_index = i;
                arg.deltas = blends.iter().map(|b| b.v).collect();
                if !inner.is_empty() {
                    if inner.len() == arg.deltas.len() {
                        for (a, b) in arg.deltas.iter_mut().zip(&inner) {
                            *a += b;
                        }
                    } else {
                        error = true;
                    }
                }
            }
        }
        if error {
            self.set_error();
        }
    }

    /// `cff2_cs_opset_t::process_blend`.
    fn process_blend(&mut self) {
        self.begin_blend();
        let k = self.region_count;
        let n = self.pop_uint();
        let count = self.args.len();
        let total = k
            .checked_add(1)
            .and_then(|k1| k1.checked_mul(n))
            .map(|t| t as usize)
            .filter(|&t| t <= count);
        let Some(total) = total else {
            self.set_error();
            return;
        };
        let start = count.saturating_sub(total);
        let (n, k) = (n as usize, k as usize);
        for i in 0..n {
            let from = start.saturating_add(n).saturating_add(i.saturating_mul(k));
            let blends: Vec<Arg> = self
                .args
                .get(from..from.saturating_add(k))
                .map(<[Arg]>::to_vec)
                .unwrap_or_default();
            let (nn, ii) = (
                u32::try_from(n).unwrap_or(u32::MAX),
                u32::try_from(i).unwrap_or(u32::MAX),
            );
            self.process_arg_blend(start.saturating_add(i), &blends, nn, ii);
        }
        self.args
            .truncate(count.saturating_sub(k.saturating_mul(n)));
    }
}

/// An operator set: the hooks of HarfBuzz's `cs_opset_t` subclasses.
pub(crate) trait Ops {
    fn flush_args_and_op(&mut self, op: u32, env: &mut Env<'_, '_>) {
        self.flush_args(env);
        self.flush_op(op, env);
    }

    fn flush_args(&mut self, env: &mut Env<'_, '_>) {
        env.pop_n_args(env.args.len());
    }

    fn flush_op(&mut self, _op: u32, _env: &mut Env<'_, '_>) {}

    fn flush_hintmask(&mut self, op: u32, env: &mut Env<'_, '_>) {
        self.flush_args_and_op(op, env);
    }

    /// The path procedures (`PATH::rmoveto` and the others).
    fn path(&mut self, _op: u32, _env: &mut Env<'_, '_>) {}
}

/// `cff2_cs_opset_t::process_op` over `cs_opset_t::process_op`.
fn process_op<O: Ops>(o: &mut O, op: u32, env: &mut Env<'_, '_>) {
    if env.trace_drop_hints && matches!(op, BLENDCS | VSINDEXCS | DOTSECTION) {
        env.trace_valid = false;
    }
    if env.trace_drop_hints
        && matches!(
            op,
            RMOVETO
                | HMOVETO
                | VMOVETO
                | RLINETO
                | HLINETO
                | VLINETO
                | RRCURVETO
                | RCURVELINE
                | RLINECURVE
                | VVCURVETO
                | HHCURVETO
                | HVCURVETO
                | HFLEX
                | FLEX
                | HFLEX1
                | FLEX1
        )
    {
        env.path_started = true;
    }
    match op {
        RETURN => env.return_from_subr(),
        ENDCHAR => {
            if env.trace_drop_hints && env.fetched_explicit {
                env.hint_trace.push(HintTrace {
                    source: env.ctx.source,
                    start: env.ctx.off.saturating_sub(1),
                    end: env.ctx.off,
                    can_follow_call: true,
                });
            }
            if env.trace_calls && env.ctx.off < env.ctx.bytes.len() {
                env.trace_valid = false;
            }
            env.endchar = true;
            o.flush_args_and_op(op, env);
        }
        FIXEDCS => env.push_fixed(),
        CALLSUBR => env.call_subr(Kind::Local),
        CALLGSUBR => env.call_subr(Kind::Global),
        HSTEM | HSTEMHM => {
            env.record_hint(env.ctx.off, false);
            let n = u32::try_from(env.args.len() / 2).unwrap_or(u32::MAX);
            env.hstem_count = env.hstem_count.saturating_add(n);
            o.flush_args_and_op(op, env);
        }
        VSTEM | VSTEMHM => {
            env.record_hint(env.ctx.off, false);
            let n = u32::try_from(env.args.len() / 2).unwrap_or(u32::MAX);
            env.vstem_count = env.vstem_count.saturating_add(n);
            o.flush_args_and_op(op, env);
        }
        HINTMASK | CNTRMASK => {
            env.determine_hintmask_size();
            let size = env.hintmask_size as usize;
            env.record_hint(env.ctx.off.saturating_add(size), true);
            if env.trace_calls {
                env.mask_trace.push(MaskTrace {
                    source: env.ctx.source,
                    at: env.ctx.off.saturating_sub(1),
                    size: env.hintmask_size,
                });
            }
            if env.avail(size) {
                o.flush_hintmask(op, env);
                env.inc(size);
            }
        }
        RMOVETO | HMOVETO | VMOVETO => {
            o.path(op, env);
            if !env.seen_moveto {
                env.determine_hintmask_size();
                env.seen_moveto = true;
            }
            o.flush_args_and_op(op, env);
        }
        RLINETO | HLINETO | VLINETO | RRCURVETO | RCURVELINE | RLINECURVE | VVCURVETO
        | HHCURVETO | VHCURVETO | HVCURVETO | HFLEX | FLEX | HFLEX1 | FLEX1 => {
            o.path(op, env);
            o.flush_args_and_op(op, env);
        }
        BLENDCS => env.process_blend(),
        VSINDEXCS => {
            env.process_vsindex();
            env.clear_args();
        }
        _ => env.process_number(op),
    }
    if !matches!(op, SHORTINT | 247..=254 | 32..=246 | FIXEDCS) {
        env.literal_run = None;
    }
}

/// `cs_interpreter_t::interpret`: runs until `endchar`; false on any
/// error and after [`MAX_OPS`] operators.
pub(crate) fn interpret<O: Ops>(env: &mut Env<'_, '_>, o: &mut O) -> bool {
    env.endchar = false;
    let mut left = MAX_OPS;
    loop {
        let op = env.fetch_op();
        process_op(o, op, env);
        left = left.saturating_sub(1);
        if env.in_error() || left == 0 {
            env.set_error();
            return false;
        }
        if env.endchar {
            return true;
        }
    }
}

/// `cff2_cs_opset_usage_t` without partial instancing: whether a kept
/// operator takes a blended operand, and whether the glyph uses `flex1`.
#[derive(Default)]
pub(crate) struct Usage {
    pub drop_hints: bool,
    pub has_blend: bool,
    pub has_flex1: bool,
}

impl Ops for Usage {
    fn flush_args_and_op(&mut self, op: u32, env: &mut Env<'_, '_>) {
        self.has_flex1 |= op == FLEX1;
        let counts = !(op == RETURN || op == ENDCHAR || (is_hint(op) && self.drop_hints));
        if counts && env.args.iter().any(Arg::blending) {
            self.has_blend = true;
        }
        env.clear_args();
    }
}

/// `coordinate_t` of `cff2_cs_rounding_t` without deltas: the absolute
/// position along one axis.
#[derive(Clone, Copy, Default)]
struct Coordinate {
    value: f64,
}

impl Coordinate {
    /// `round`: the operand becomes the difference of the rounded
    /// absolute positions.
    fn round(&mut self, arg: &mut Arg) {
        arg.deltas.clear();
        let previous = self.value;
        self.value += arg.v;
        arg.v = floor(self.value + 0.5) - floor(previous + 0.5);
    }
}

/// `cff2_cs_rounding_t` for a full instance (no deltas left): rounds the
/// absolute positions of the points and the stem edges
/// (fonttools/fonttools#4338).
#[derive(Default)]
pub(crate) struct Rounding {
    point: [Coordinate; 2],
}

impl Rounding {
    fn at(&mut self, axis: usize) -> &mut Coordinate {
        if axis == 0 {
            &mut self.point[0]
        } else {
            &mut self.point[1]
        }
    }

    fn alternating(&mut self, args: &mut [Arg], axis: usize) {
        let mut axis = axis;
        for a in args {
            self.at(axis).round(a);
            axis ^= 1;
        }
    }

    fn round_one(&mut self, axis: usize, args: &mut [Arg], i: usize) {
        if let Some(a) = args.get_mut(i) {
            self.at(axis).round(a);
        }
    }

    /// `round`: false for an operand count the operator does not take
    /// and for operators it does not round.
    pub fn round(&mut self, op: u32, args: &mut [Arg]) -> bool {
        let n = args.len();
        match op {
            RMOVETO => {
                if n != 2 {
                    return false;
                }
                self.alternating(args, 0);
            }
            HMOVETO | VMOVETO => {
                if n != 1 {
                    return false;
                }
                self.round_one(usize::from(op == VMOVETO), args, 0);
            }
            RLINETO => {
                if n == 0 || !n.is_multiple_of(2) {
                    return false;
                }
                self.alternating(args, 0);
            }
            RRCURVETO => {
                if n == 0 || !n.is_multiple_of(6) {
                    return false;
                }
                self.alternating(args, 0);
            }
            RCURVELINE => {
                if n < 8 || !n.saturating_sub(2).is_multiple_of(6) {
                    return false;
                }
                self.alternating(args, 0);
            }
            RLINECURVE => {
                if n < 8 || !n.is_multiple_of(2) {
                    return false;
                }
                self.alternating(args, 0);
            }
            HLINETO | VLINETO => {
                if n == 0 {
                    return false;
                }
                self.alternating(args, usize::from(op == VLINETO));
            }
            HHCURVETO | VVCURVETO => {
                if n < 4 || n % 4 > 1 {
                    return false;
                }
                let axis = usize::from(op == VVCURVETO);
                let mut i = 0usize;
                if n & 1 != 0 {
                    self.round_one(axis ^ 1, args, 0);
                    i = 1;
                }
                while i < n {
                    self.round_one(axis, args, i);
                    self.round_one(0, args, i.saturating_add(1));
                    self.round_one(1, args, i.saturating_add(2));
                    self.round_one(axis, args, i.saturating_add(3));
                    i = i.saturating_add(4);
                }
            }
            HVCURVETO | VHCURVETO => {
                if n < 4 || n % 4 > 1 {
                    return false;
                }
                let mut axis = usize::from(op == VHCURVETO);
                let mut i = 0usize;
                while i.saturating_add(4) <= n {
                    self.round_one(axis, args, i);
                    self.round_one(0, args, i.saturating_add(1));
                    self.round_one(1, args, i.saturating_add(2));
                    self.round_one(axis ^ 1, args, i.saturating_add(3));
                    if i.saturating_add(5) == n {
                        self.round_one(axis, args, i.saturating_add(4));
                        return true;
                    }
                    i = i.saturating_add(4);
                    axis ^= 1;
                }
            }
            FLEX => {
                if n != 13 {
                    return false;
                }
                if let Some(first) = args.get_mut(..12) {
                    self.alternating(first, 0);
                }
                if let Some(a) = args.get_mut(12) {
                    Coordinate::default().round(a);
                }
            }
            HFLEX | HFLEX1 => {
                let want = if op == HFLEX { 7 } else { 9 };
                if n != want {
                    return false;
                }
                // These operators return to the initial y.
                let y = self.point[1];
                for i in 0..n {
                    let axis = if op == HFLEX {
                        i == 2
                    } else {
                        i == 1 || i == 3 || i == 7
                    };
                    self.round_one(usize::from(axis), args, i);
                }
                self.point[1] = y;
            }
            HSTEM | HSTEMHM | VSTEM | VSTEMHM | HINTMASK | CNTRMASK => {
                // Stem operands are successive edges, from zero for each
                // operator.
                if !n.is_multiple_of(2) {
                    return false;
                }
                let mut stem = Coordinate::default();
                for a in args {
                    stem.round(a);
                }
            }
            _ => return false,
        }
        true
    }
}

/// A charstring command (`cs_command_t`): an operator, its operands and a
/// mask's bytes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Command {
    pub op: u32,
    pub args: Vec<f64>,
    pub mask: Vec<u8>,
}

impl Command {
    pub fn new(op: u32, args: Vec<f64>) -> Command {
        Command {
            op,
            args,
            mask: Vec::new(),
        }
    }
}

/// `cff2_cs_opset_flatten_t` without partial instancing: the program
/// with subroutines inlined and blends evaluated, numbers re-encoded,
/// hints dropped with `drop_hints`, the operands rounded with `rounding`
/// and the commands captured in `commands`.
#[derive(Default)]
pub(crate) struct Flatten {
    pub out: Vec<u8>,
    pub drop_hints: bool,
    pub rounding: Option<Rounding>,
    pub commands: Option<Vec<Command>>,
}

impl Flatten {
    /// `flatten_blends`: a blend group written back as a `blend`.
    fn flatten_blends(&mut self, i: usize, env: &mut Env<'_, '_>) -> usize {
        let nv = env.args.get(i).map_or(0, |a| a.num_values) as usize;
        let k = env.region_count as usize;
        let group = env.args.get(i..i.saturating_add(nv)).unwrap_or(&[]);
        let ok = group.len() == nv
            && group.iter().enumerate().all(|(j, a)| {
                a.blending()
                    && a.num_values as usize == nv
                    && a.value_index as usize == j
                    && a.deltas.len() == k
            });
        if !ok {
            env.set_error();
            return nv;
        }
        for a in group {
            encode_num_cs(&mut self.out, a.v);
        }
        for a in group {
            for &d in &a.deltas {
                encode_num_cs(&mut self.out, d);
            }
        }
        encode_int(&mut self.out, i32::try_from(nv).unwrap_or(i32::MAX));
        encode_op(&mut self.out, BLENDCS);
        nv
    }
}

impl Ops for Flatten {
    fn flush_args_and_op(&mut self, op: u32, env: &mut Env<'_, '_>) {
        let skip = op == RETURN || op == ENDCHAR || (is_hint(op) && self.drop_hints);
        if !skip
            && let Some(r) = &mut self.rounding
            && !r.round(op, &mut env.args)
        {
            env.set_error();
            return;
        }
        if !skip && let Some(c) = &mut self.commands {
            c.push(Command::new(op, env.args.iter().map(|a| a.v).collect()));
        }
        match op {
            RETURN | ENDCHAR => {}
            op if is_hint(op) && self.drop_hints => env.clear_args(),
            _ => {
                self.flush_args(env);
                self.flush_op(op, env);
            }
        }
    }

    fn flush_args(&mut self, env: &mut Env<'_, '_>) {
        let count = env.args.len();
        let mut i = 0usize;
        while i < count {
            let Some(a) = env.args.get(i) else {
                break;
            };
            if a.blending() && self.rounding.is_none() {
                let nv = a.num_values as usize;
                if nv == 0 || count < nv {
                    env.set_error();
                    return;
                }
                i = i.saturating_add(self.flatten_blends(i, env));
                if env.in_error() {
                    return;
                }
            } else {
                encode_num_cs(&mut self.out, a.v);
                i = i.saturating_add(1);
            }
        }
        env.pop_n_args(count);
    }

    fn flush_op(&mut self, op: u32, _env: &mut Env<'_, '_>) {
        if op != RETURN && op != ENDCHAR {
            encode_op(&mut self.out, op);
        }
    }

    fn flush_hintmask(&mut self, op: u32, env: &mut Env<'_, '_>) {
        self.flush_args_and_op(op, env);
        let size = env.hintmask_size as usize;
        if !self.drop_hints {
            let mask: Vec<u8> = (0..size).map(|i| env.byte(i)).collect();
            if let Some(cmd) = self.commands.as_mut().and_then(|c| c.last_mut())
                && cmd.op == op
            {
                cmd.mask.clone_from(&mask);
            }
            self.out.extend_from_slice(&mask);
        }
    }
}

/// `cff2_extents_param_t` with `cff2_path_procs_extents_t`: the bounds of
/// the points of the paths that draw.
pub(crate) struct Extents {
    path_open: bool,
    pub min: (f64, f64),
    pub max: (f64, f64),
}

impl Default for Extents {
    fn default() -> Extents {
        Extents {
            path_open: false,
            min: (f64::from(i32::MAX), f64::from(i32::MAX)),
            max: (f64::from(i32::MIN), f64::from(i32::MIN)),
        }
    }
}

type Pt = (f64, f64);

impl Extents {
    fn update(&mut self, p: Pt) {
        if p.0 < self.min.0 {
            self.min.0 = p.0;
        }
        if p.0 > self.max.0 {
            self.max.0 = p.0;
        }
        if p.1 < self.min.1 {
            self.min.1 = p.1;
        }
        if p.1 > self.max.1 {
            self.max.1 = p.1;
        }
    }

    fn moveto(&mut self, env: &mut Env<'_, '_>, p: Pt) {
        self.path_open = false;
        env.pt = p;
    }

    fn open(&mut self, env: &Env<'_, '_>) {
        if !self.path_open {
            self.path_open = true;
            self.update(env.pt);
        }
    }

    fn line(&mut self, env: &mut Env<'_, '_>, p: Pt) {
        self.open(env);
        env.pt = p;
        self.update(p);
    }

    fn curve(&mut self, env: &mut Env<'_, '_>, p1: Pt, p2: Pt, p3: Pt) {
        self.open(env);
        self.update(p1);
        self.update(p2);
        env.pt = p3;
        self.update(p3);
    }

    /// The box (`get_extents_at`): `(x_bearing, y_bearing, width,
    /// height)`.
    pub fn extents(&self) -> (i32, i32, i32, i32) {
        use sofel_geom::float::{hb_clamp_to_i32, hb_roundf};
        let (x_bearing, width) = if self.min.0 >= self.max.0 {
            (0, 0)
        } else {
            let xb = hb_roundf(self.min.0);
            (
                hb_clamp_to_i32(xb),
                hb_clamp_to_i32(hb_roundf(self.max.0) - xb),
            )
        };
        let (y_bearing, height) = if self.min.1 >= self.max.1 {
            (0, 0)
        } else {
            let yb = hb_roundf(self.max.1);
            (
                hb_clamp_to_i32(yb),
                hb_clamp_to_i32(hb_roundf(self.min.1) - yb),
            )
        };
        (x_bearing, y_bearing, width, height)
    }
}

/// `p` moved by `d` along x (`h`) or y.
fn along(p: Pt, d: f64, h: bool) -> Pt {
    if h { (p.0 + d, p.1) } else { (p.0, p.1 + d) }
}

fn moved(p: Pt, dx: f64, dy: f64) -> Pt {
    (p.0 + dx, p.1 + dy)
}

impl Extents {
    fn rcurve(&mut self, env: &mut Env<'_, '_>, i: usize) {
        let p1 = moved(env.pt, env.arg(i), env.arg(i.saturating_add(1)));
        let p2 = moved(
            p1,
            env.arg(i.saturating_add(2)),
            env.arg(i.saturating_add(3)),
        );
        let p3 = moved(
            p2,
            env.arg(i.saturating_add(4)),
            env.arg(i.saturating_add(5)),
        );
        self.curve(env, p1, p2, p3);
    }

    /// `vhcurveto` (`hv` false) and `hvcurveto` (`hv` true).
    fn vh_hv(&mut self, env: &mut Env<'_, '_>, hv: bool) {
        let n = env.args.len();
        let mut i = 0usize;
        if n % 8 >= 4 {
            let mut p1 = along(env.pt, env.arg(i), hv);
            let mut p2 = moved(
                p1,
                env.arg(i.saturating_add(1)),
                env.arg(i.saturating_add(2)),
            );
            let mut p3 = along(p2, env.arg(i.saturating_add(3)), !hv);
            i = i.saturating_add(4);
            while i.saturating_add(8) <= n {
                self.curve(env, p1, p2, p3);
                p1 = along(env.pt, env.arg(i), !hv);
                p2 = moved(
                    p1,
                    env.arg(i.saturating_add(1)),
                    env.arg(i.saturating_add(2)),
                );
                p3 = along(p2, env.arg(i.saturating_add(3)), hv);
                self.curve(env, p1, p2, p3);
                p1 = along(p3, env.arg(i.saturating_add(4)), hv);
                p2 = moved(
                    p1,
                    env.arg(i.saturating_add(5)),
                    env.arg(i.saturating_add(6)),
                );
                p3 = along(p2, env.arg(i.saturating_add(7)), !hv);
                i = i.saturating_add(8);
            }
            if i < n {
                p3 = along(p3, env.arg(i), hv);
            }
            self.curve(env, p1, p2, p3);
        } else {
            while i.saturating_add(8) <= n {
                let p1 = along(env.pt, env.arg(i), hv);
                let p2 = moved(
                    p1,
                    env.arg(i.saturating_add(1)),
                    env.arg(i.saturating_add(2)),
                );
                let p3 = along(p2, env.arg(i.saturating_add(3)), !hv);
                self.curve(env, p1, p2, p3);
                let p1 = along(p3, env.arg(i.saturating_add(4)), !hv);
                let p2 = moved(
                    p1,
                    env.arg(i.saturating_add(5)),
                    env.arg(i.saturating_add(6)),
                );
                let mut p3 = along(p2, env.arg(i.saturating_add(7)), hv);
                if n.saturating_sub(i) < 16 && n & 1 != 0 {
                    p3 = along(p3, env.arg(i.saturating_add(8)), !hv);
                }
                self.curve(env, p1, p2, p3);
                i = i.saturating_add(8);
            }
        }
    }
}

impl Ops for Extents {
    fn path(&mut self, op: u32, env: &mut Env<'_, '_>) {
        let n = env.args.len();
        match op {
            RMOVETO => {
                let dy = env.pop();
                let dx = env.pop();
                let p = moved(env.pt, dx, dy);
                self.moveto(env, p);
            }
            HMOVETO => {
                let dx = env.pop();
                let p = along(env.pt, dx, true);
                self.moveto(env, p);
            }
            VMOVETO => {
                let dy = env.pop();
                let p = along(env.pt, dy, false);
                self.moveto(env, p);
            }
            RLINETO => {
                let mut i = 0usize;
                while i.saturating_add(2) <= n {
                    let p = moved(env.pt, env.arg(i), env.arg(i.saturating_add(1)));
                    self.line(env, p);
                    i = i.saturating_add(2);
                }
            }
            HLINETO | VLINETO => {
                let h = op == HLINETO;
                let mut i = 0usize;
                while i.saturating_add(2) <= n {
                    let p1 = along(env.pt, env.arg(i), h);
                    self.line(env, p1);
                    let p2 = along(p1, env.arg(i.saturating_add(1)), !h);
                    self.line(env, p2);
                    i = i.saturating_add(2);
                }
                if i < n {
                    let p = along(env.pt, env.arg(i), h);
                    self.line(env, p);
                }
            }
            RRCURVETO => {
                let mut i = 0usize;
                while i.saturating_add(6) <= n {
                    self.rcurve(env, i);
                    i = i.saturating_add(6);
                }
            }
            RCURVELINE => {
                if n < 8 {
                    return;
                }
                let limit = n.saturating_sub(2);
                let mut i = 0usize;
                while i.saturating_add(6) <= limit {
                    self.rcurve(env, i);
                    i = i.saturating_add(6);
                }
                let p = moved(env.pt, env.arg(i), env.arg(i.saturating_add(1)));
                self.line(env, p);
            }
            RLINECURVE => {
                if n < 8 {
                    return;
                }
                let limit = n.saturating_sub(6);
                let mut i = 0usize;
                while i.saturating_add(2) <= limit {
                    let p = moved(env.pt, env.arg(i), env.arg(i.saturating_add(1)));
                    self.line(env, p);
                    i = i.saturating_add(2);
                }
                self.rcurve(env, i);
            }
            VVCURVETO | HHCURVETO => {
                let hh = op == HHCURVETO;
                let mut i = 0usize;
                let mut p1 = env.pt;
                if n & 1 != 0 {
                    p1 = along(p1, env.arg(0), !hh);
                    i = 1;
                }
                while i.saturating_add(4) <= n {
                    p1 = along(p1, env.arg(i), hh);
                    let p2 = moved(
                        p1,
                        env.arg(i.saturating_add(1)),
                        env.arg(i.saturating_add(2)),
                    );
                    let p3 = along(p2, env.arg(i.saturating_add(3)), hh);
                    self.curve(env, p1, p2, p3);
                    p1 = env.pt;
                    i = i.saturating_add(4);
                }
            }
            VHCURVETO => self.vh_hv(env, false),
            HVCURVETO => self.vh_hv(env, true),
            HFLEX => {
                if n != 7 {
                    env.set_error();
                    return;
                }
                let p1 = along(env.pt, env.arg(0), true);
                let p2 = moved(p1, env.arg(1), env.arg(2));
                let p3 = along(p2, env.arg(3), true);
                let p4 = along(p3, env.arg(4), true);
                let mut p5 = along(p4, env.arg(5), true);
                p5.1 = p1.1;
                let p6 = along(p5, env.arg(6), true);
                self.curve(env, p1, p2, p3);
                self.curve(env, p4, p5, p6);
            }
            FLEX => {
                if n != 13 {
                    env.set_error();
                    return;
                }
                let p1 = moved(env.pt, env.arg(0), env.arg(1));
                let p2 = moved(p1, env.arg(2), env.arg(3));
                let p3 = moved(p2, env.arg(4), env.arg(5));
                let p4 = moved(p3, env.arg(6), env.arg(7));
                let p5 = moved(p4, env.arg(8), env.arg(9));
                let p6 = moved(p5, env.arg(10), env.arg(11));
                self.curve(env, p1, p2, p3);
                self.curve(env, p4, p5, p6);
            }
            HFLEX1 => {
                if n != 9 {
                    env.set_error();
                    return;
                }
                let p1 = moved(env.pt, env.arg(0), env.arg(1));
                let p2 = moved(p1, env.arg(2), env.arg(3));
                let p3 = along(p2, env.arg(4), true);
                let p4 = along(p3, env.arg(5), true);
                let p5 = moved(p4, env.arg(6), env.arg(7));
                let mut p6 = along(p5, env.arg(8), true);
                p6.1 = env.pt.1;
                self.curve(env, p1, p2, p3);
                self.curve(env, p4, p5, p6);
            }
            FLEX1 => {
                if n != 11 {
                    env.set_error();
                    return;
                }
                let mut d = (0.0f64, 0.0f64);
                for i in (0..10).step_by(2) {
                    d = moved(d, env.arg(i), env.arg(i.saturating_add(1)));
                }
                let p1 = moved(env.pt, env.arg(0), env.arg(1));
                let p2 = moved(p1, env.arg(2), env.arg(3));
                let p3 = moved(p2, env.arg(4), env.arg(5));
                let p4 = moved(p3, env.arg(6), env.arg(7));
                let p5 = moved(p4, env.arg(8), env.arg(9));
                let mut p6 = p5;
                if d.0.abs() > d.1.abs() {
                    p6.0 += env.arg(10);
                    p6.1 = env.pt.1;
                } else {
                    p6.0 = env.pt.0;
                    p6.1 += env.arg(10);
                }
                self.curve(env, p1, p2, p3);
                self.curve(env, p4, p5, p6);
            }
            _ => {}
        }
    }
}

/// The box of a charstring at `coords` (`get_extents_at` without
/// scaling); `None` when HarfBuzz's interpreter fails on it.
pub(crate) fn extents(
    charstring: &[u8],
    p: Programs<'_>,
    coords: &[i32],
) -> Option<(i32, i32, i32, i32)> {
    let mut env = Env::new(charstring, p, coords);
    env.numbers = true;
    let mut e = Extents::default();
    interpret(&mut env, &mut e).then(|| e.extents())
}
