//! Type 2 charstrings as HarfBuzz's subsetter reads and writes them: the
//! charstring interpreter without path construction
//! (`src/hb-cff-interp-common.hh`, `src/hb-cff-interp-cs-common.hh`,
//! `src/hb-cff1-interp-cs.hh`), the number encoder (`str_encoder_t` of
//! `src/hb-subset-cff-common.hh`), and the three operator sets the
//! subsetter runs it with: subroutine subsetting
//! (`cff1_cs_opset_subr_subset_t`), desubroutinizing
//! (`cff1_cs_opset_flatten_t`, both in `src/hb-subset-cff1.cc`) and the
//! `seac` search (`cff1_cs_opset_seac_t`, `src/hb-ot-cff1-table.cc`).
//!
//! Numbers are `f64`, as HarfBuzz's `number_t`; any error fails the whole
//! interpretation, as in HarfBuzz.

use alloc::vec::Vec;

use sofel_font::read::cff::hb::HbIndex;

/// `HB_CFF_MAX_OPS`.
const MAX_OPS: u32 = 200_000;
/// `kMaxCallLimit`.
const MAX_CALLS: usize = 10;
/// The argument stack limit.
const MAX_ARGS: usize = 513;

pub(crate) const HSTEM: u32 = 1;
pub(crate) const VSTEM: u32 = 3;
pub(crate) const VMOVETO: u32 = 4;
pub(crate) const CALLSUBR: u32 = 10;
pub(crate) const RETURN: u32 = 11;
pub(crate) const ENDCHAR: u32 = 14;
/// `endchar` as a byte: the charstring of a glyph not kept.
pub(crate) const ENDCHAR_BYTE: u8 = 14;
pub(crate) const HSTEMHM: u32 = 18;
pub(crate) const HINTMASK: u32 = 19;
pub(crate) const CNTRMASK: u32 = 20;
pub(crate) const RMOVETO: u32 = 21;
pub(crate) const HMOVETO: u32 = 22;
pub(crate) const VSTEMHM: u32 = 23;
pub(crate) const SHORTINT: u32 = 28;
pub(crate) const CALLGSUBR: u32 = 29;
pub(crate) const FIXEDCS: u32 = 255;
pub(crate) const DOTSECTION: u32 = 256;
/// `OpCode_Invalid`.
const INVALID: u32 = 0xFFFF;

/// Which program a call context runs (`cs_type_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    CharString,
    Global,
    Local,
}

/// A program and the position in it (`call_context_t` with its
/// `byte_str_ref_t`); an offset past the end is the error state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ctx<'a> {
    bytes: &'a [u8],
    off: usize,
    pub kind: Kind,
    pub subr_num: u32,
}

/// The bias of a subroutine INDEX with `count` entries.
pub(crate) fn bias(count: u32) -> i32 {
    if count < 1240 {
        107
    } else if count < 33900 {
        1131
    } else {
        32768
    }
}

/// `number_t::to_int`: truncated toward zero, saturated outside `int`.
pub(crate) fn to_int(v: f64) -> i32 {
    if v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX) {
        // In range, checked above; `as` truncates toward zero.
        #[allow(clippy::cast_possible_truncation)]
        let i = v as i32;
        i
    } else if v < 0.0 {
        i32::MIN
    } else if v > 0.0 {
        i32::MAX
    } else {
        0
    }
}

/// `number_t::to_int (int *)`: `None` outside the `int` range.
fn checked_int(v: f64) -> Option<i32> {
    (v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX)).then(|| to_int(v))
}

/// HarfBuzz's charstring interpreter state (`cff1_cs_interp_env_t`).
#[derive(Clone, Debug)]
pub(crate) struct Env<'a> {
    pub ctx: Ctx<'a>,
    pub calls: Vec<Ctx<'a>>,
    call_error: bool,
    pub args: Vec<f64>,
    arg_error: bool,
    endchar: bool,
    seen_moveto: bool,
    seen_hintmask: bool,
    hstem_count: u32,
    vstem_count: u32,
    pub hintmask_size: u32,
    gsubrs: HbIndex<'a>,
    lsubrs: HbIndex<'a>,
    processed_width: bool,
    pub has_width: bool,
    pub arg_start: usize,
    pub width: f64,
}

impl<'a> Env<'a> {
    /// An interpreter for `charstring` with the global and local
    /// subroutines `gsubrs` and `lsubrs`.
    pub fn new(charstring: &'a [u8], gsubrs: HbIndex<'a>, lsubrs: HbIndex<'a>) -> Env<'a> {
        Env {
            ctx: Ctx {
                bytes: charstring,
                off: 0,
                kind: Kind::CharString,
                subr_num: 0,
            },
            calls: Vec::new(),
            call_error: false,
            args: Vec::new(),
            arg_error: false,
            endchar: false,
            seen_moveto: true,
            seen_hintmask: false,
            hstem_count: 0,
            vstem_count: 0,
            hintmask_size: 0,
            gsubrs,
            lsubrs,
            processed_width: false,
            has_width: false,
            arg_start: 0,
            width: 0.0,
        }
    }

    pub fn offset(&self) -> usize {
        self.ctx.off
    }

    pub fn in_error(&self) -> bool {
        self.call_error || self.arg_error || self.ctx.off > self.ctx.bytes.len()
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

    fn fetch_op(&mut self) -> u32 {
        let Some(&b) = self.ctx.bytes.get(self.ctx.off) else {
            return INVALID;
        };
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
            self.args.push(v);
        } else {
            self.arg_error = true;
        }
    }

    fn pop(&mut self) -> f64 {
        self.args.pop().unwrap_or_else(|| {
            self.arg_error = true;
            0.0
        })
    }

    fn pop_int(&mut self) -> i32 {
        let v = self.pop();
        checked_int(v).unwrap_or_else(|| {
            self.arg_error = true;
            0
        })
    }

    /// `interp_env_t::clear_args`.
    fn clear_args_base(&mut self) {
        self.args.clear();
    }

    /// `cff1_cs_interp_env_t::clear_args`: also forgets the width
    /// argument.
    pub fn clear_args(&mut self) {
        self.arg_start = 0;
        self.args.clear();
    }

    /// `pop_n_args`.
    fn pop_n_args(&mut self, n: usize) {
        if self.args.len() >= n {
            self.args.truncate(self.args.len().saturating_sub(n));
        } else {
            self.arg_error = true;
        }
    }

    fn call_subr(&mut self, kind: Kind) {
        let subrs = if kind == Kind::Local {
            self.lsubrs
        } else {
            self.gsubrs
        };
        let n = i64::from(self.pop_int()).saturating_add(i64::from(bias(subrs.len())));
        let num = u32::try_from(n).ok().filter(|&n| n < subrs.len());
        let Some(num) = num.filter(|_| self.calls.len() < MAX_CALLS) else {
            self.set_error();
            return;
        };
        self.calls.push(self.ctx);
        self.ctx = Ctx {
            bytes: subrs.get(num).unwrap_or(&[]),
            off: 0,
            kind,
            subr_num: num,
        };
    }

    fn return_from_subr(&mut self) {
        // HarfBuzz marks the returning program in error when it is, and
        // then replaces it with the caller's.
        match self.calls.pop() {
            Some(c) => self.ctx = c,
            None => {
                self.call_error = true;
                self.ctx = Ctx {
                    bytes: &[],
                    off: 0,
                    kind: Kind::CharString,
                    subr_num: 0,
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

    fn set_width(&mut self, has_width: bool) {
        if !self.processed_width && !self.args.is_empty() && has_width {
            self.width = self.args.first().copied().unwrap_or(0.0);
            self.has_width = true;
            self.arg_start = 1;
        }
        self.processed_width = true;
    }

    /// `opset_t::process_op`: numbers are pushed, other operators clear
    /// the arguments.
    fn process_number(&mut self, op: u32) {
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
            _ => self.clear_args_base(),
        }
    }

    /// `push_fixed_from_substr`: nothing happens when fewer than 4 bytes
    /// remain.
    fn push_fixed(&mut self) {
        if !self.avail(4) {
            return;
        }
        let v = u32::from_be_bytes([self.byte(0), self.byte(1), self.byte(2), self.byte(3)]);
        self.push(f64::from(i32::from_ne_bytes(v.to_ne_bytes())) / 65536.0);
        self.inc(4);
    }
}

/// `cff1_cs_opset_t::check_width`.
fn check_width(op: u32, env: &mut Env<'_>) {
    if env.processed_width {
        return;
    }
    let n = env.args.len();
    let has_width = match op {
        ENDCHAR | HSTEM | HSTEMHM | VSTEM | VSTEMHM | HINTMASK | CNTRMASK => n & 1 != 0,
        HMOVETO | VMOVETO => n > 1,
        RMOVETO => n > 2,
        _ => return,
    };
    env.set_width(has_width);
}

/// An operator set: the hooks HarfBuzz's subclasses of `cs_opset_t`
/// override.
pub(crate) trait Ops<'a> {
    /// `process_op` of the operator set.
    fn process_op(&mut self, op: u32, env: &mut Env<'a>) {
        cff1_process_op(self, op, env);
    }

    fn flush_args_and_op(&mut self, op: u32, env: &mut Env<'a>) {
        self.flush_args(env);
        self.flush_op(op, env);
    }

    fn flush_args(&mut self, env: &mut Env<'a>) {
        env.pop_n_args(env.args.len());
        env.clear_args();
    }

    fn flush_op(&mut self, _op: u32, _env: &mut Env<'a>) {}

    fn flush_hintmask(&mut self, op: u32, env: &mut Env<'a>) {
        self.flush_args_and_op(op, env);
    }

    fn process_seac(&mut self, _env: &mut Env<'a>) {}
}

/// `cff1_cs_opset_t::process_op`.
pub(crate) fn cff1_process_op<'a, O: Ops<'a> + ?Sized>(o: &mut O, op: u32, env: &mut Env<'a>) {
    match op {
        DOTSECTION => o.flush_op(op, env),
        ENDCHAR => {
            check_width(op, env);
            if env.args.len() >= 4 {
                o.process_seac(env);
            }
            o.flush_args_and_op(op, env);
            env.endchar = true;
        }
        _ => cs_process_op(o, op, env),
    }
}

/// `cs_opset_t::process_op` with the null path procedures.
fn cs_process_op<'a, O: Ops<'a> + ?Sized>(o: &mut O, op: u32, env: &mut Env<'a>) {
    match op {
        RETURN => env.return_from_subr(),
        FIXEDCS => env.push_fixed(),
        CALLSUBR => env.call_subr(Kind::Local),
        CALLGSUBR => env.call_subr(Kind::Global),
        HSTEM | HSTEMHM => {
            check_width(op, env);
            let n = u32::try_from(env.args.len() / 2).unwrap_or(u32::MAX);
            env.hstem_count = env.hstem_count.saturating_add(n);
            o.flush_args_and_op(op, env);
        }
        VSTEM | VSTEMHM => {
            check_width(op, env);
            let n = u32::try_from(env.args.len() / 2).unwrap_or(u32::MAX);
            env.vstem_count = env.vstem_count.saturating_add(n);
            o.flush_args_and_op(op, env);
        }
        HINTMASK | CNTRMASK => {
            check_width(op, env);
            env.determine_hintmask_size();
            let size = env.hintmask_size as usize;
            if env.avail(size) {
                o.flush_hintmask(op, env);
                env.inc(size);
            }
        }
        RMOVETO | HMOVETO | VMOVETO => {
            check_width(op, env);
            if !env.seen_moveto {
                env.determine_hintmask_size();
                env.seen_moveto = true;
            }
            o.flush_args_and_op(op, env);
        }
        // rlineto hlineto vlineto rrcurveto, rcurveline rlinecurve
        // vvcurveto hhcurveto, vhcurveto hvcurveto, hflex flex hflex1 flex1
        5..=8 | 24..=27 | 30 | 31 | 290..=293 => o.flush_args_and_op(op, env),
        _ => env.process_number(op),
    }
}

/// `cs_interpreter_t::interpret`: runs until `endchar`; false on any
/// error and after [`MAX_OPS`] operators.
pub(crate) fn interpret<'a, O: Ops<'a>>(env: &mut Env<'a>, o: &mut O) -> bool {
    env.endchar = false;
    let mut left = MAX_OPS;
    loop {
        let op = env.fetch_op();
        o.process_op(op, env);
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

/// `str_encoder_t::encode_int`.
pub(crate) fn encode_int(out: &mut Vec<u8>, v: i32) {
    let byte = |x: i32| u8::try_from(x & 0xFF).unwrap_or(0);
    if (-1131..=1131).contains(&v) {
        if (-107..=107).contains(&v) {
            out.push(byte(v.saturating_add(139)));
        } else if v > 0 {
            let w = v.saturating_sub(108);
            out.extend([byte((w >> 8).saturating_add(247)), byte(w)]);
        } else {
            let w = v.saturating_neg().saturating_sub(108);
            out.extend([byte((w >> 8).saturating_add(251)), byte(w)]);
        }
    } else {
        let w = v.clamp(-32768, 32767);
        out.extend([28, byte(w >> 8), byte(w)]);
    }
}

/// `number_t::in_int_range`.
fn in_int_range(v: f64) -> bool {
    // In the i16 range, checked first; `as` truncates toward zero.
    #[allow(clippy::cast_possible_truncation)]
    let t = v as i16;
    (-32768.0..=32767.0).contains(&v) && f64::from(t) == v
}

/// `str_encoder_t::encode_num_cs`: an integer form, or a 16.16 number.
pub(crate) fn encode_num_cs(out: &mut Vec<u8>, v: f64) {
    if in_int_range(v) {
        encode_int(out, to_int(v));
    } else {
        let f = (v * 65536.0).clamp(f64::from(i32::MIN), f64::from(i32::MAX));
        // Clamped to the i32 range; `as` truncates toward zero.
        #[allow(clippy::cast_possible_truncation)]
        let f = f as i32;
        out.push(255);
        out.extend(f.to_be_bytes());
    }
}

/// `str_encoder_t::encode_op`.
pub(crate) fn encode_op(out: &mut Vec<u8>, op: u32) {
    let low = u8::try_from(op & 0xFF).unwrap_or(0);
    if op >= 256 {
        out.extend([12, low]);
    } else {
        out.push(low);
    }
}

/// `cff1_cs_opset_flatten_t`: the program with subroutines inlined,
/// numbers re-encoded, and hints dropped with `drop_hints`.
pub(crate) struct Flatten {
    pub out: Vec<u8>,
    pub drop_hints: bool,
}

impl<'a> Ops<'a> for Flatten {
    fn flush_args_and_op(&mut self, op: u32, env: &mut Env<'a>) {
        if env.arg_start > 0 {
            encode_num_cs(&mut self.out, env.width);
        }
        let hint = matches!(
            op,
            HSTEM | HSTEMHM | VSTEM | VSTEMHM | HINTMASK | CNTRMASK | DOTSECTION
        );
        if hint && self.drop_hints {
            env.clear_args();
            return;
        }
        self.flush_args(env);
        self.flush_op(op, env);
    }

    fn flush_args(&mut self, env: &mut Env<'a>) {
        for &v in env.args.get(env.arg_start..).unwrap_or(&[]) {
            encode_num_cs(&mut self.out, v);
        }
        env.pop_n_args(env.args.len());
        env.clear_args();
    }

    fn flush_op(&mut self, op: u32, _env: &mut Env<'a>) {
        encode_op(&mut self.out, op);
    }

    fn flush_hintmask(&mut self, op: u32, env: &mut Env<'a>) {
        self.flush_args_and_op(op, env);
        if !self.drop_hints {
            for i in 0..env.hintmask_size as usize {
                let b = env.byte(i);
                self.out.push(b);
            }
        }
    }
}

/// `cff1_cs_opset_seac_t`: the Standard encoding codes of `seac`'s base
/// and accent.
#[derive(Default)]
pub(crate) struct Seac {
    pub codes: Option<(u32, u32)>,
}

impl<'a> Ops<'a> for Seac {
    fn process_seac(&mut self, env: &mut Env<'a>) {
        let n = env.args.len();
        let at = |i: usize| env.args.get(i).copied().unwrap_or(0.0);
        let code = |v: f64| u32::from_ne_bytes(to_int(v).to_ne_bytes());
        self.codes = Some((code(at(n.saturating_sub(2))), code(at(n.saturating_sub(1)))));
    }
}

/// One token of a parsed program (`parsed_cs_op_t`): an operator, or a
/// number, with its bytes `start..end` in the program.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tok {
    pub op: u32,
    pub start: usize,
    pub end: usize,
    pub hinting: bool,
    pub subr_num: u32,
}

/// A parsed program (`parsed_cs_str_t`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Parsed {
    pub values: Vec<Tok>,
    pub parsed: bool,
    pub hint_dropped: bool,
    /// The width, to be written first when hints are dropped.
    pub prefix: Option<f64>,
    pub has_calls: bool,
    op_start: usize,
}

impl Parsed {
    fn add_op(&mut self, op: u32, end: usize, subr_num: u32) {
        if self.parsed {
            return;
        }
        self.values.push(Tok {
            op,
            start: self.op_start,
            end,
            hinting: false,
            subr_num,
        });
        self.op_start = end;
    }

    fn has_immediate_subr_number(&self) -> bool {
        if self.parsed {
            return true;
        }
        let op = self.values.last().map_or(INVALID, |t| t.op);
        op == SHORTINT || op == FIXEDCS || (32..=254).contains(&op)
    }

    fn add_call_op(&mut self, op: u32, end: usize, subr_num: u32) {
        if self.parsed {
            return;
        }
        self.has_calls = true;
        self.values.pop();
        self.add_op(op, end, subr_num);
    }

    /// `compact`: joins each run of adjacent tokens that are not calls,
    /// have the same hint mark and span at most 255 bytes into one token
    /// (with an invalid operator). The encoded program is the same.
    pub fn compact(&mut self) {
        let is_call = |t: &Tok| t.op == CALLSUBR || t.op == CALLGSUBR;
        let mut out: Vec<Tok> = Vec::with_capacity(self.values.len());
        for t in self.values.drain(..) {
            match out.last_mut() {
                Some(j)
                    if !is_call(j)
                        && !is_call(&t)
                        && j.hinting == t.hinting
                        && j.end == t.start
                        && t.end.saturating_sub(j.start) <= 255 =>
                {
                    j.end = t.end;
                    j.op = INVALID;
                }
                _ => out.push(t),
            }
        }
        self.values = out;
    }

    /// `at_end`: whether token `pos` is the last before `return`.
    pub fn at_end(&self, pos: usize) -> bool {
        let next = pos.saturating_add(1);
        next >= self.values.len() || self.values.get(next).is_some_and(|t| t.op == RETURN)
    }
}

/// The parsed charstrings and subroutines of one Font DICT's view: the
/// program a context names.
pub(crate) struct Programs<'p> {
    pub charstring: &'p mut Parsed,
    pub gsubrs: &'p mut [Parsed],
    pub lsubrs: &'p mut [Parsed],
}

impl Programs<'_> {
    pub fn get(&mut self, kind: Kind, num: u32) -> Option<&mut Parsed> {
        match kind {
            Kind::CharString => Some(&mut *self.charstring),
            Kind::Global => self.gsubrs.get_mut(num as usize),
            Kind::Local => self.lsubrs.get_mut(num as usize),
        }
    }
}

/// `cff1_cs_opset_subr_subset_t` with `subr_subset_param_t`: parses the
/// programs it runs and collects the subroutine closures.
pub(crate) struct SubrSubset<'p, 'c> {
    pub programs: Programs<'p>,
    pub global_closure: &'c mut alloc::collections::BTreeSet<u32>,
    pub local_closure: &'c mut alloc::collections::BTreeSet<u32>,
    /// The program being parsed (`current_parsed_str`).
    pub current: (Kind, u32),
}

impl SubrSubset<'_, '_> {
    fn current(&mut self) -> Option<&mut Parsed> {
        let (k, n) = self.current;
        self.programs.get(k, n)
    }

    /// `set_current_str`.
    fn set_current(&mut self, env: &mut Env<'_>, calling: bool) {
        let (kind, num) = (env.ctx.kind, env.ctx.subr_num);
        let Some(p) = self.programs.get(kind, num) else {
            env.set_error();
            return;
        };
        if calling && !p.parsed && !p.values.is_empty() {
            env.set_error();
        } else {
            self.current = (kind, num);
        }
    }

    fn call(&mut self, op: u32, kind: Kind, env: &mut Env<'_>) {
        if !self
            .current()
            .is_some_and(|p| p.has_immediate_subr_number())
        {
            env.set_error();
            return;
        }
        let end = env.offset();
        env.call_subr(kind);
        let num = env.ctx.subr_num;
        if let Some(p) = self.current() {
            p.add_call_op(op, end, num);
        }
        if kind == Kind::Local {
            self.local_closure.insert(num);
        } else {
            self.global_closure.insert(num);
        }
        self.set_current(env, true);
    }

    /// `complete_parsed_str` (`cff1_subr_subsetter_t`).
    pub fn complete(&mut self, env: &mut Env<'_>) {
        if env.has_width {
            self.programs.charstring.prefix = Some(env.width);
        }
        if let Some(p) = self.current() {
            p.parsed = true;
        }
        for i in 0..env.calls.len() {
            let Some(c) = env.calls.get(i).copied() else {
                continue;
            };
            match self.programs.get(c.kind, c.subr_num) {
                Some(p) => p.parsed = true,
                None => env.set_error(),
            }
        }
    }
}

impl<'a> Ops<'a> for SubrSubset<'_, '_> {
    fn process_op(&mut self, op: u32, env: &mut Env<'a>) {
        match op {
            RETURN => {
                let end = env.offset();
                if let Some(p) = self.current() {
                    p.add_op(op, end, 0);
                    p.parsed = true;
                }
                env.return_from_subr();
                self.set_current(env, false);
            }
            ENDCHAR => {
                let end = env.offset();
                if let Some(p) = self.current() {
                    p.add_op(op, end, 0);
                    p.parsed = true;
                }
                cff1_process_op(self, op, env);
            }
            CALLSUBR => self.call(op, Kind::Local, env),
            CALLGSUBR => self.call(op, Kind::Global, env),
            _ => {
                cff1_process_op(self, op, env);
                let end = env.offset();
                if let Some(p) = self.current() {
                    p.add_op(op, end, 0);
                }
            }
        }
    }
}
