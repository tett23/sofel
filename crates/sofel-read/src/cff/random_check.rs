//! The interval check of CFF glyphs that run `random` (ADR 0082).
//!
//! The validator decodes each glyph from its seed, which follows one path
//! only. A glyph that runs `random` there is traced again with every
//! value held as an interval: `random` pushes (0, 1], arithmetic works on
//! the ends, and a comparison or `ifelse` whose result is not decided
//! follows each result as a separate branch, depth first. The first
//! problem found on any branch is returned.
//!
//! The check is allocation-free: pending branches are kept in a fixed
//! array of [`MAX_PENDING`] states. Every branch point has two results
//! (comparisons and `ifelse`); operators that take an integer stop the
//! check when they get a 16.16 number, so a range of integers never
//! reaches them.

use super::charstring::{
    CharStringToken, CharStringTokens, Context, Hints, MAX_HINTS, MAX_SUBR, STORAGE_SIZE,
};
use super::{Index, ft, subr_bias};
use crate::CffRandomReason;

/// The numbers and operators the branches may run together.
const LIMIT: u32 = 100_000;
/// The branches that may wait at once.
const MAX_PENDING: usize = 16;
/// The CFF operand stack.
const STACK: usize = 48;

const LO_IN: u8 = 1;
const HI_IN: u8 = 2;
const FIXED: u8 = 4;
const LO_INF: u8 = 8;
const HI_INF: u8 = 16;

const NEG_INF: i64 = i64::MIN;
const POS_INF: i64 = i64::MAX;

/// An interval of 16.16 numbers with FreeType's number type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Val {
    lo: i32,
    hi: i32,
    flags: u8,
}

/// What a value can be when an operator asks whether it is zero or
/// whether one value is at most another.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tri {
    No,
    Yes,
    Either,
}

impl Val {
    const ZERO: Val = Val::point(0, true);

    const fn point(v: i32, fixed: bool) -> Val {
        Val {
            lo: v,
            hi: v,
            flags: LO_IN | HI_IN | if fixed { FIXED } else { 0 },
        }
    }

    fn int(i: i32) -> Val {
        Val::point((i.cast_unsigned() << 16).cast_signed(), false)
    }

    fn has(self, f: u8) -> bool {
        self.flags & f != 0
    }

    fn fixed(self) -> bool {
        self.has(FIXED)
    }

    fn as_fixed(self) -> Val {
        Val {
            flags: self.flags | FIXED,
            ..self
        }
    }

    fn is_point(self) -> bool {
        !self.has(LO_INF) && !self.has(HI_INF) && self.lo == self.hi
    }

    fn infinite(self) -> bool {
        self.has(LO_INF) || self.has(HI_INF)
    }

    fn lo64(self) -> i64 {
        if self.has(LO_INF) {
            NEG_INF
        } else {
            i64::from(self.lo)
        }
    }

    fn hi64(self) -> i64 {
        if self.has(HI_INF) {
            POS_INF
        } else {
            i64::from(self.hi)
        }
    }

    /// A 16.16 interval from 64-bit ends; ends beyond 32 bits become
    /// infinite, and an interval of one number is a point.
    fn from_ends(lo: i64, lo_in: bool, hi: i64, hi_in: bool) -> Val {
        let mut flags = FIXED;
        let (lo, lo_in) = match i32::try_from(lo) {
            Ok(v) => (v, lo_in),
            Err(_) if lo < 0 => {
                flags |= LO_INF;
                (i32::MIN, false)
            }
            Err(_) => (i32::MAX, true),
        };
        let (hi, hi_in) = match i32::try_from(hi) {
            Ok(v) => (v, hi_in),
            Err(_) if hi > 0 => {
                flags |= HI_INF;
                (i32::MAX, false)
            }
            Err(_) => (i32::MIN, true),
        };
        if flags & (LO_INF | HI_INF) == 0 && lo >= hi {
            return Val::point(lo, true);
        }
        if lo_in {
            flags |= LO_IN;
        }
        if hi_in {
            flags |= HI_IN;
        }
        Val { lo, hi, flags }
    }

    fn contains_zero(self) -> bool {
        let below = self.lo64() < 0 || (self.lo64() == 0 && self.has(LO_IN));
        let above = self.hi64() > 0 || (self.hi64() == 0 && self.has(HI_IN));
        below && above
    }

    /// Whether the value is not zero, as `and`, `or` and `not` ask.
    fn truth(self) -> Tri {
        if self.is_point() {
            if self.lo != 0 { Tri::Yes } else { Tri::No }
        } else if self.contains_zero() {
            Tri::Either
        } else {
            Tri::Yes
        }
    }

    /// The smallest and the largest integer of the interval: the floor of
    /// the lower end and the ceiling of the upper end.
    fn integers(self) -> (i64, i64) {
        let lo = self.lo64() >> 16;
        let hi = self.hi64().saturating_neg() >> 16;
        (lo, hi.saturating_neg())
    }
}

/// Whether `a <= b` (`ifelse`).
fn at_most(a: Val, b: Val) -> Tri {
    if a.is_point() && b.is_point() {
        return if a.lo <= b.lo { Tri::Yes } else { Tri::No };
    }
    if a.hi64() <= b.lo64() {
        Tri::Yes
    } else if a.lo64() > b.hi64() || (a.lo64() == b.hi64() && !(a.has(LO_IN) && b.has(HI_IN))) {
        Tri::No
    } else {
        Tri::Either
    }
}

/// Whether `a == b` (`eq`).
fn equal(a: Val, b: Val) -> Tri {
    if a.is_point() && b.is_point() {
        return if a.lo == b.lo { Tri::Yes } else { Tri::No };
    }
    let below = |x: Val, y: Val| {
        x.hi64() < y.lo64() || (x.hi64() == y.lo64() && !(x.has(HI_IN) && y.has(LO_IN)))
    };
    if below(a, b) || below(b, a) {
        Tri::No
    } else {
        Tri::Either
    }
}

/// One candidate end of `mul` or `div`, with whether it is included.
#[derive(Clone, Copy)]
struct End {
    v: i64,
    inc: bool,
}

/// The interval of the candidate ends: the smallest and the largest,
/// each included when some pair that gives it has both ends included.
fn hull(ends: [End; 4]) -> Val {
    let lo = ends.iter().map(|e| e.v).min().unwrap_or(0);
    let hi = ends.iter().map(|e| e.v).max().unwrap_or(0);
    let inc = |v: i64| v != NEG_INF && v != POS_INF && ends.iter().any(|e| e.v == v && e.inc);
    Val::from_ends(lo, inc(lo), hi, inc(hi))
}

/// The four ends of `a` and `b` combined by `f`.
fn pairs(a: Val, b: Val, f: &dyn Fn(i64, i64) -> i64) -> [End; 4] {
    let ae = [(a.lo64(), a.has(LO_IN)), (a.hi64(), a.has(HI_IN))];
    let be = [(b.lo64(), b.has(LO_IN)), (b.hi64(), b.has(HI_IN))];
    let mut out = [End { v: 0, inc: false }; 4];
    for (slot, ((x, xi), (y, yi))) in out
        .iter_mut()
        .zip(ae.iter().flat_map(|&x| be.iter().map(move |&y| (x, y))))
    {
        let v = f(x, y);
        *slot = End {
            v,
            inc: xi && yi && v != NEG_INF && v != POS_INF,
        };
    }
    out
}

fn add_end(x: i64, y: i64) -> i64 {
    if x == NEG_INF || y == NEG_INF {
        NEG_INF
    } else if x == POS_INF || y == POS_INF {
        POS_INF
    } else {
        x.saturating_add(y)
    }
}

/// What the check found, and where: the position in the charstring or
/// subroutine being run and the subroutine depth there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Found {
    pub(super) finding: Finding,
    pub(super) position: usize,
    pub(super) depth: usize,
}

/// The three diagnostics of ADR 0082.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Finding {
    SubroutineOutOfRange,
    Unchecked(CffRandomReason),
    OperandRejected(u16),
}

#[derive(Clone, Copy)]
struct Frame<'a> {
    data: &'a [u8],
    pos: usize,
}

/// One branch: what FreeType's interpreter keeps for a glyph, with
/// intervals for numbers.
#[derive(Clone, Copy)]
struct State<'a> {
    stack: [Val; STACK],
    len: usize,
    storage: [Val; STORAGE_SIZE],
    hints: Hints,
    map_valid: bool,
    frames: [Frame<'a>; MAX_SUBR + 1],
    depth: usize,
}

/// Why a branch stopped running.
enum Stop {
    /// The branch ended (endchar, or an error FreeType also stops at).
    End,
    /// The operator at `at` has two results: the branch goes on with the
    /// one pushed, the other is `other`.
    Branch {
        other: Val,
        at: At,
    },
    Found(Found),
}

type Step<T> = Result<T, Stop>;

impl<'a> State<'a> {
    fn pop(&mut self) -> Step<Val> {
        self.len = self.len.checked_sub(1).ok_or(Stop::End)?;
        self.stack.get(self.len).copied().ok_or(Stop::End)
    }

    fn push(&mut self, v: Val) -> Step<()> {
        let slot = self.stack.get_mut(self.len).ok_or(Stop::End)?;
        *slot = v;
        self.len = self.len.saturating_add(1);
        Ok(())
    }

    /// Pushes `no`, the first branch, and stops for the branch of `yes`
    /// when the condition is not decided.
    fn decide(&mut self, t: Tri, no: Val, yes: Val, at: At) -> Step<()> {
        match t {
            Tri::No => self.push(no),
            Tri::Yes => self.push(yes),
            Tri::Either => {
                self.push(no)?;
                Err(Stop::Branch { other: yes, at })
            }
        }
    }

    fn frame(&self) -> Frame<'a> {
        self.frames
            .get(self.depth)
            .copied()
            .unwrap_or(Frame { data: &[], pos: 0 })
    }

    fn save(&mut self, pos: usize) {
        if let Some(f) = self.frames.get_mut(self.depth) {
            f.pos = pos;
        }
    }

    /// `cf2_doStems`: counts the stem pairs and clears the stack.
    fn stems(&mut self, horizontal: bool) {
        let pairs = self.len / 2;
        if horizontal {
            self.hints.h_stems = self.hints.h_stems.saturating_add(pairs);
        } else {
            self.hints.v_stems = self.hints.v_stems.saturating_add(pairs);
        }
        self.len = 0;
    }

    /// The hint map of a new contour or element (`cf2_hintmap_build`).
    fn build(&mut self, moving: bool) -> Step<()> {
        if ((moving && !self.map_valid) || self.hints.mask_new)
            && self.hints.build().map_err(|_| Stop::End)?
        {
            self.map_valid = true;
        }
        Ok(())
    }
}

/// Where the operator being run is, for a [`Found`].
#[derive(Clone, Copy)]
struct At {
    position: usize,
    depth: usize,
}

impl At {
    fn found(self, finding: Finding) -> Stop {
        Stop::Found(Found {
            finding,
            position: self.position,
            depth: self.depth,
        })
    }
}

/// Traces `charstring` with intervals; the first problem found on any
/// branch, in the branch order of ADR 0082.
pub(super) fn check(charstring: &[u8], ctx: &Context<'_>) -> Option<Found> {
    let first = State {
        stack: [Val::ZERO; STACK],
        len: 0,
        storage: [Val::ZERO; STORAGE_SIZE],
        hints: Hints::default(),
        map_valid: false,
        frames: [Frame {
            data: charstring,
            pos: 0,
        }; MAX_SUBR + 1],
        depth: 0,
    };
    let mut pending = [first; MAX_PENDING];
    let mut waiting = 0usize;
    let mut cur = first;
    let mut count = 0u32;
    loop {
        match run(&mut cur, ctx, &mut count) {
            Stop::Found(f) => return Some(f),
            Stop::Branch { other, at } => {
                let Some(slot) = pending.get_mut(waiting) else {
                    return Some(Found {
                        finding: Finding::Unchecked(CffRandomReason::Limit),
                        position: at.position,
                        depth: at.depth,
                    });
                };
                *slot = cur;
                if let Some(top) = cur.len.checked_sub(1).and_then(|t| slot.stack.get_mut(t)) {
                    *top = other;
                }
                waiting = waiting.saturating_add(1);
            }
            Stop::End => {
                waiting = waiting.checked_sub(1)?;
                cur = *pending.get(waiting)?;
            }
        }
    }
}

/// Runs `s` until it ends, branches or finds a problem.
fn run<'a>(s: &mut State<'a>, ctx: &Context<'a>, count: &mut u32) -> Stop {
    loop {
        if let Err(stop) = step(s, ctx, count) {
            return stop;
        }
    }
}

/// Runs one number or operator.
fn step<'a>(s: &mut State<'a>, ctx: &Context<'a>, count: &mut u32) -> Step<()> {
    let frame = s.frame();
    let at = At {
        position: frame.pos,
        depth: s.depth,
    };
    let mut tokens = CharStringTokens::new(frame.data, ctx.version);
    tokens.skip(frame.pos).ok_or(Stop::End)?;
    let op = if tokens.at_end() {
        if s.depth > 0 { 11 } else { 14 }
    } else {
        *count = count.saturating_add(1);
        if *count > LIMIT {
            return Err(at.found(Finding::Unchecked(CffRandomReason::Limit)));
        }
        match tokens.next_token() {
            Some(Ok(CharStringToken::Integer(v))) => {
                s.save(tokens.position());
                return s.push(Val::int(v));
            }
            Some(Ok(CharStringToken::Fixed(v))) => {
                s.save(tokens.position());
                return s.push(Val::point(v, true));
            }
            Some(Ok(CharStringToken::Operator(o))) => o,
            Some(Err(_)) | None => return Err(Stop::End),
        }
    };
    s.save(tokens.position());
    let mut clear = true;
    match op {
        // hstem, hstemhm, vstem, vstemhm
        1 | 18 | 3 | 23 => {
            if !s.hints.mask_valid {
                s.stems(op == 1 || op == 18);
            }
        }
        // vmoveto, hmoveto, rmoveto
        4 | 22 | 21 => {
            let n = if op == 21 { 2 } else { 1 };
            if s.len < n {
                return Err(Stop::End);
            }
            s.build(true)?;
        }
        5..=8 | 24..=27 | 30 | 31 => {
            if !path_reads_ok(op, s.len) {
                return Err(Stop::End);
            }
            if s.len > 0 {
                s.build(false)?;
            }
        }
        // callsubr, callgsubr
        10 | 29 => {
            if s.depth >= MAX_SUBR {
                return Err(Stop::End);
            }
            let v = s.pop()?;
            let (index, sorted) = if op == 29 {
                (ctx.global_subrs, ctx.global_subrs_sorted)
            } else {
                (ctx.local_subrs, ctx.local_subrs_sorted)
            };
            let bias = if ctx.charstring_type == 1 {
                0
            } else {
                subr_bias(index.len())
            };
            let out_of_range = at.found(Finding::SubroutineOutOfRange);
            if v.infinite() {
                return Err(out_of_range);
            }
            let (lo, hi) = v.integers();
            let bias = i64::from(bias);
            let count = i64::from(index.len());
            if lo.saturating_add(bias) < 0 || hi.saturating_add(bias) >= count {
                return Err(out_of_range);
            }
            if v.fixed() {
                return Err(at.found(Finding::OperandRejected(op)));
            }
            let i = u32::try_from(lo.saturating_add(bias)).map_err(|_| Stop::End)?;
            let data = subroutine(index, i, sorted).ok_or(out_of_range)?;
            s.depth = s.depth.saturating_add(1);
            if let Some(f) = s.frames.get_mut(s.depth) {
                *f = Frame { data, pos: 0 };
            }
            clear = false;
        }
        // return
        11 => {
            s.depth = s.depth.checked_sub(1).ok_or(Stop::End)?;
            clear = false;
        }
        // endchar: the branch ends (seac components are not traced).
        14 => return Err(Stop::End),
        // hintmask, cntrmask
        19 | 20 => {
            if !(s.len > 1 && s.hints.mask_valid) {
                s.stems(false);
                let bits = s.hints.total();
                if op == 19 {
                    s.hints.set_counts(bits).map_err(|_| Stop::End)?;
                } else {
                    if bits > MAX_HINTS {
                        return Err(Stop::End);
                    }
                    s.hints.initial_map_valid = true;
                }
                tokens.skip(bits.saturating_add(7) / 8).ok_or(Stop::End)?;
                s.save(tokens.position());
            }
        }
        o if o >= 0x0C00 => clear = escape(s, o, at)?,
        _ => {}
    }
    if clear {
        s.len = 0;
    }
    Ok(())
}

/// `ft_pointer` of the subroutine `i`.
fn subroutine<'a>(index: Index<'a>, i: u32, sorted: bool) -> Option<&'a [u8]> {
    index.ft_pointer(i, sorted)
}

/// Whether a path operator with `n` operands reads only within the stack,
/// as the decoder's loops read them.
fn path_reads_ok(op: u16, n: usize) -> bool {
    match op {
        // rlineto
        5 => n.is_multiple_of(2),
        // rcurveline
        24 => n % 6 >= 2,
        // rlinecurve
        25 => {
            if n == 0 {
                true
            } else if n <= 6 {
                n == 6
            } else {
                n.saturating_sub(6).is_multiple_of(2)
            }
        }
        // vvcurveto, hhcurveto
        26 | 27 => {
            let m = n & !2usize;
            let mut i = n.saturating_sub(m);
            while i < m {
                if m.saturating_sub(i) & 1 == 1 {
                    i = i.saturating_add(1);
                }
                if i.saturating_add(4) > n {
                    return false;
                }
                i = i.saturating_add(4);
            }
            true
        }
        // vhcurveto, hvcurveto
        30 | 31 => {
            let m = n & !2usize;
            let mut i = n.saturating_sub(m);
            while i < m {
                let last = m.saturating_sub(i) == 5;
                let reads = if last { 5 } else { 4 };
                if i.saturating_add(reads) > n {
                    return false;
                }
                i = i.saturating_add(reads);
            }
            true
        }
        // hlineto, vlineto, rrcurveto
        _ => true,
    }
}

/// The two-byte operators; returns whether the stack is cleared.
fn escape(s: &mut State<'_>, op: u16, at: At) -> Step<bool> {
    let op2 = op & 0xFF;
    // hflex, flex, hflex1, flex1 read 7, 12, 9 and 11 operands.
    let flex = match op2 {
        34 => Some(7),
        35 => Some(12),
        36 => Some(9),
        37 => Some(11),
        _ => None,
    };
    if let Some(n) = flex {
        if s.len < n {
            return Err(Stop::End);
        }
        s.build(false)?;
        return Ok(true);
    }
    match op2 {
        // and, or
        3 | 4 => {
            let b = s.pop()?.truth();
            let a = s.pop()?.truth();
            let t = if op2 == 3 {
                match (a, b) {
                    (Tri::No, _) | (_, Tri::No) => Tri::No,
                    (Tri::Yes, Tri::Yes) => Tri::Yes,
                    _ => Tri::Either,
                }
            } else {
                match (a, b) {
                    (Tri::Yes, _) | (_, Tri::Yes) => Tri::Yes,
                    (Tri::No, Tri::No) => Tri::No,
                    _ => Tri::Either,
                }
            };
            s.decide(t, Val::int(0), Val::int(1), at)?;
        }
        // not
        5 => {
            let t = match s.pop()?.truth() {
                Tri::Yes => Tri::No,
                Tri::No => Tri::Yes,
                Tri::Either => Tri::Either,
            };
            s.decide(t, Val::int(0), Val::int(1), at)?;
        }
        // abs
        9 => {
            let a = s.pop()?;
            s.push(abs(a))?;
        }
        // add, sub
        10 | 11 => {
            let b = s.pop()?;
            let a = s.pop()?;
            s.push(if op2 == 10 { add(a, b) } else { sub(a, b) })?;
        }
        // div
        12 => {
            let b = s.pop()?;
            let a = s.pop()?;
            s.push(div(a, b).map_err(|r| at.found(Finding::Unchecked(r)))?)?;
        }
        // neg
        14 => {
            let a = s.pop()?;
            s.push(neg(a))?;
        }
        // eq
        15 => {
            let b = s.pop()?;
            let a = s.pop()?;
            s.decide(equal(a, b), Val::int(0), Val::int(1), at)?;
        }
        // drop
        18 => {
            s.pop()?;
        }
        // put
        20 => {
            let i = int_operand(s.pop()?, op, at)?;
            let v = s.pop()?;
            if let Some(slot) = usize::try_from(i).ok().and_then(|i| s.storage.get_mut(i)) {
                *slot = v;
            }
        }
        // get
        21 => {
            let i = int_operand(s.pop()?, op, at)?;
            if let Some(v) = usize::try_from(i)
                .ok()
                .and_then(|i| s.storage.get(i))
                .copied()
            {
                s.push(v.as_fixed())?;
            }
        }
        // ifelse
        22 => {
            let c2 = s.pop()?;
            let c1 = s.pop()?;
            let b = s.pop()?.as_fixed();
            let a = s.pop()?.as_fixed();
            match at_most(c1, c2) {
                Tri::Yes => s.push(a)?,
                Tri::No => s.push(b)?,
                Tri::Either => {
                    s.push(a)?;
                    return Err(Stop::Branch { other: b, at });
                }
            }
        }
        // random
        23 => s.push(Val {
            lo: 0,
            hi: 0x1_0000,
            flags: HI_IN | FIXED,
        })?,
        // mul
        24 => {
            let b = s.pop()?;
            let a = s.pop()?;
            s.push(mul(a, b).map_err(|r| at.found(Finding::Unchecked(r)))?)?;
        }
        // sqrt
        26 => {
            let a = s.pop()?;
            s.push(sqrt(a).map_err(|r| at.found(Finding::Unchecked(r)))?)?;
        }
        // dup
        27 => {
            let a = s.pop()?.as_fixed();
            s.push(a)?;
            s.push(a)?;
        }
        // exch
        28 => {
            let b = s.pop()?.as_fixed();
            let a = s.pop()?.as_fixed();
            s.push(b)?;
            s.push(a)?;
        }
        // index
        29 => {
            let v = s.pop()?;
            let size = s.len;
            if v.infinite() {
                return Err(at.found(Finding::Unchecked(CffRandomReason::InfiniteBound)));
            }
            if !v.is_point() && v.integers().1 >= i64::try_from(size).unwrap_or(i64::MAX) {
                return Err(at.found(Finding::Unchecked(CffRandomReason::StackPosition)));
            }
            let i = int_operand(v, op, at)?;
            if size > 0 {
                let k = match usize::try_from(i) {
                    Err(_) => size.saturating_sub(1),
                    Ok(i) if i >= size => 0,
                    Ok(i) => size.saturating_sub(1).saturating_sub(i),
                };
                let v = s.stack.get(k).copied().ok_or(Stop::End)?.as_fixed();
                s.push(v)?;
            }
        }
        // roll
        30 => {
            let shift = s.pop()?;
            let count = s.pop()?;
            let size = i64::try_from(s.len).unwrap_or(i64::MAX);
            if shift.infinite() || count.infinite() {
                return Err(at.found(Finding::Unchecked(CffRandomReason::InfiniteBound)));
            }
            let (lo, hi) = count.integers();
            if !count.is_point() && (lo < 0 || hi > size) {
                return Err(at.found(Finding::Unchecked(CffRandomReason::StackPosition)));
            }
            let shift = int_operand(shift, op, at)?;
            let count = int_operand(count, op, at)?;
            roll(s, count, shift)?;
        }
        // dotsection, reserved and Type 1 operators.
        _ => return Ok(true),
    }
    Ok(false)
}

/// An operand of an operator that needs an integer: infinite ends and
/// 16.16 numbers stop the check.
fn int_operand(v: Val, op: u16, at: At) -> Step<i32> {
    if v.infinite() {
        return Err(at.found(Finding::Unchecked(CffRandomReason::InfiniteBound)));
    }
    if v.fixed() {
        return Err(at.found(Finding::OperandRejected(op)));
    }
    Ok(v.lo >> 16)
}

/// `cf2_stack_roll`.
fn roll(s: &mut State<'_>, count: i32, shift: i32) -> Step<()> {
    if count < 2 {
        return Ok(());
    }
    let n = usize::try_from(count).map_err(|_| Stop::End)?;
    if n > s.len {
        return Err(Stop::End);
    }
    let offset = s.len.saturating_sub(n);
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
    let slice = s
        .stack
        .get_mut(offset..offset.saturating_add(n))
        .ok_or(Stop::End)?;
    let k = if shift > 0 {
        usize::try_from(shift).unwrap_or(0)
    } else {
        n.saturating_sub(usize::try_from(shift.unsigned_abs()).unwrap_or(0))
    };
    slice.rotate_right(k.checked_rem(n).unwrap_or(0));
    Ok(())
}

fn add(a: Val, b: Val) -> Val {
    if a.is_point() && b.is_point() {
        return Val::point(a.lo.wrapping_add(b.lo), true);
    }
    Val::from_ends(
        add_end(a.lo64(), b.lo64()),
        a.has(LO_IN) && b.has(LO_IN),
        add_end(a.hi64(), b.hi64()),
        a.has(HI_IN) && b.has(HI_IN),
    )
}

fn neg(a: Val) -> Val {
    if a.is_point() {
        return Val::point(
            if a.lo == i32::MIN {
                i32::MAX
            } else {
                a.lo.wrapping_neg()
            },
            true,
        );
    }
    let flip = |v: i64| match v {
        NEG_INF => POS_INF,
        POS_INF => NEG_INF,
        v => v.saturating_neg(),
    };
    Val::from_ends(flip(a.hi64()), a.has(HI_IN), flip(a.lo64()), a.has(LO_IN))
}

fn sub(a: Val, b: Val) -> Val {
    if a.is_point() && b.is_point() {
        return Val::point(a.lo.wrapping_sub(b.lo), true);
    }
    add(a, neg(b))
}

fn abs(a: Val) -> Val {
    if a.is_point() {
        return Val::point(
            if a.lo == i32::MIN {
                i32::MAX
            } else {
                a.lo.abs()
            },
            true,
        );
    }
    if a.lo64() >= 0 {
        return a.as_fixed();
    }
    if a.hi64() <= 0 {
        return neg(a);
    }
    let n = neg(a);
    let (hi, hi_in) = if n.hi64() > a.hi64() {
        (n.hi64(), n.has(HI_IN))
    } else if n.hi64() < a.hi64() {
        (a.hi64(), a.has(HI_IN))
    } else {
        (a.hi64(), a.has(HI_IN) || n.has(HI_IN))
    };
    Val::from_ends(0, true, hi, hi_in)
}

fn mul(a: Val, b: Val) -> Result<Val, CffRandomReason> {
    if a.is_point() && b.is_point() {
        return Ok(Val::point(
            ft::to_i32(ft::mul_fix(i64::from(a.lo), i64::from(b.lo))),
            true,
        ));
    }
    if a.infinite() || b.infinite() {
        return Err(CffRandomReason::InfiniteBound);
    }
    Ok(hull(pairs(a, b, &ft::mul_fix)))
}

fn div(a: Val, b: Val) -> Result<Val, CffRandomReason> {
    if a.is_point() && b.is_point() {
        return Ok(Val::point(
            ft::to_i32(ft::div_fix(i64::from(a.lo), i64::from(b.lo))),
            true,
        ));
    }
    if a.infinite() || b.infinite() {
        return Err(CffRandomReason::InfiniteBound);
    }
    if b.contains_zero() {
        return Err(CffRandomReason::DivisionByZero);
    }
    // An end of the divisor at 0 is open; the divisor lies on one side.
    let positive = b.hi64() > 0;
    Ok(hull(pairs(a, b, &|x, y| {
        if y == 0 {
            match (x.signum(), positive) {
                (0, _) => 0,
                (1, true) | (-1, false) => POS_INF,
                _ => NEG_INF,
            }
        } else {
            ft::div_fix(x, y)
        }
    })))
}

fn sqrt(a: Val) -> Result<Val, CffRandomReason> {
    if a.is_point() {
        return Ok(Val::point(
            if a.lo > 0 {
                ft::sqrt_fixed(a.lo.cast_unsigned()).cast_signed()
            } else {
                0
            },
            true,
        ));
    }
    if a.lo64() < 0 {
        return Err(CffRandomReason::NegativeSqrt);
    }
    let root = |v: i64| match u32::try_from(v) {
        Ok(v) => i64::from(ft::sqrt_fixed(v)),
        Err(_) => POS_INF,
    };
    Ok(Val::from_ends(
        root(a.lo64()),
        a.has(LO_IN),
        root(a.hi64()),
        a.has(HI_IN),
    ))
}
