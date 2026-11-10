//! CFF and CFF2 read as HarfBuzz 15.1.0 reads them (ADR 0021): the
//! sanitizer's range checks and operation budget, the INDEX, charset,
//! encoding, FDSelect and Item Variation Store sanitizers and lookups,
//! and the DICT interpreters. HarfBuzz accepts and reads some tables
//! differently from FreeType ([`Index`](super::Index) and the other views
//! of this module's parent follow FreeType), so `sofel-font` builds
//! HarfBuzz's CFF accelerators from these.
//!
//! Ported from HarfBuzz 15.1.0: `src/hb-sanitize.hh`
//! (`hb_sanitize_context_t`), `src/hb-open-type.hh` (`CFFIndex`,
//! `ArrayOf`, `OffsetTo`), `src/hb-ot-cff-common.hh` (`FDSelect`,
//! `StructAtOffsetOrNull`), `src/hb-ot-cff1-table.hh` (`Encoding`,
//! `Charset`, the CFF DICT operator sets), `src/hb-ot-cff2-table.hh`
//! (`CFF2FDSelect`, `CFF2ItemVariationStore`, the CFF2 DICT operator
//! sets), `src/hb-ot-layout-common.hh` (`ItemVariationStore`,
//! `VarRegionList`, `VarData`), `src/hb-cff-interp-common.hh`,
//! `src/hb-cff-interp-dict-common.hh` and `src/hb-number-parser.hh`.
//! Copyright © 2007-2026 the HarfBuzz authors (Behdad Esfahbod, Adobe
//! Inc., Ebrahim Byagowi and others). Used under the MIT license; see
//! THIRD_PARTY_NOTICES.md.
//!
//! Positions are byte offsets from the start of the table, as `i64` so
//! that HarfBuzz's signed offsets can point before it (which its range
//! checks then reject).

use super::{EXPERT_CHARSET, EXPERT_SUBSET_CHARSET, STANDARD_ENCODING, Version};
use crate::be::{self, to_usize};

/// HarfBuzz's `CFF_UNDEF_SID`.
pub const UNDEF_SID: u32 = u32::MAX;

/// `HB_SANITIZE_MAX_OPS_FACTOR`, `_MIN` and `_MAX`.
const MAX_OPS_FACTOR: u64 = 64;
const MAX_OPS_MIN: i64 = 16384;
const MAX_OPS_MAX: i64 = 0x3FFF_FFFF;

/// HarfBuzz's `hb_sanitize_context_t` over one table: the range checks
/// and the operation budget (`max_ops`) that each checked range spends.
#[derive(Clone, Copy, Debug)]
pub struct Sanitizer {
    len: usize,
    ops: i64,
}

impl Sanitizer {
    /// A context over a table of `len` bytes with HarfBuzz's budget
    /// (`start_processing`): 64 per byte, at least 16384, at most
    /// 2^30 - 1.
    pub fn new(len: usize) -> Sanitizer {
        let ops = u32::try_from(len)
            .ok()
            .and_then(|l| u32::try_from(u64::from(l).saturating_mul(MAX_OPS_FACTOR)).ok())
            .map_or(MAX_OPS_MAX, |m| {
                i64::from(m).clamp(MAX_OPS_MIN, MAX_OPS_MAX)
            });
        Sanitizer { len, ops }
    }

    /// A context without a budget, for checks repeated after HarfBuzz's
    /// own run of them succeeded.
    pub fn unlimited(len: usize) -> Sanitizer {
        Sanitizer { len, ops: i64::MAX }
    }

    /// Whether the budget is spent: every later range check fails.
    pub fn exhausted(&self) -> bool {
        self.ops <= 0
    }

    /// `check_point`: `p` is in the table or just after it.
    pub fn check_point(&self, p: i64) -> bool {
        u64::try_from(p).is_ok_and(|p| usize::try_from(p).is_ok_and(|p| p <= self.len))
    }

    /// `check_range`: `len` bytes from `p` are in the table; spends `len`
    /// and fails when the budget is then spent.
    pub fn check_range(&mut self, p: i64, len: u64) -> bool {
        if !self.check_point(p) {
            return false;
        }
        let room = u64::try_from(self.len)
            .unwrap_or(u64::MAX)
            .saturating_sub(u64::try_from(p).unwrap_or(0));
        // HarfBuzz compares `(unsigned int) (end - p)`.
        if u64::from(u32::try_from(room).unwrap_or(u32::MAX)) < len {
            return false;
        }
        self.ops = self
            .ops
            .saturating_sub(i64::try_from(len).unwrap_or(i64::MAX));
        self.ops > 0
    }

    /// `check_array` and `check_range (base, a, b)`: `a * b` bytes, failing
    /// when the product overflows 32 bits.
    pub fn check_array(&mut self, p: i64, a: u32, b: u32) -> bool {
        a.checked_mul(b)
            .is_some_and(|m| self.check_range(p, u64::from(m)))
    }

    /// The first half of `StructAtOffsetOrNull`: `None` for offset 0 or
    /// when `base + offset` is outside the table, else the position.
    pub fn struct_at(&self, base: i64, offset: i32) -> Option<i64> {
        if offset == 0 {
            return None;
        }
        let p = base.saturating_add(i64::from(offset));
        self.check_point(p).then_some(p)
    }
}

fn pos(p: i64) -> Option<usize> {
    usize::try_from(p).ok()
}

fn u8_at(data: &[u8], p: i64) -> Option<u8> {
    be::u8_at(data, pos(p)?)
}

fn u16_at(data: &[u8], p: i64) -> Option<u16> {
    be::u16_at(data, pos(p)?)
}

fn u32_at(data: &[u8], p: i64) -> Option<u32> {
    be::u32_at(data, pos(p)?)
}

fn add(p: i64, n: u64) -> i64 {
    p.saturating_add(i64::try_from(n).unwrap_or(i64::MAX))
}

/// A sanitized `CFFIndex` (16-bit counts in CFF, 32-bit in CFF2), or
/// HarfBuzz's Null INDEX (no objects).
#[derive(Clone, Copy, Debug)]
pub struct HbIndex<'a> {
    data: &'a [u8],
    at: i64,
    count: u32,
    off_size: u8,
    count_len: u8,
}

impl<'a> HbIndex<'a> {
    /// HarfBuzz's Null INDEX.
    pub const fn null() -> HbIndex<'static> {
        HbIndex {
            data: &[],
            at: 0,
            count: 0,
            off_size: 0,
            count_len: 2,
        }
    }

    /// The INDEX at `p` in `data` if `CFFIndex::sanitize` accepts it.
    pub fn sanitize(
        data: &'a [u8],
        p: i64,
        version: Version,
        sc: &mut Sanitizer,
    ) -> Option<HbIndex<'a>> {
        let count_len = u8::try_from(version.count_len()).unwrap_or(2);
        if !sc.check_point(add(p, u64::from(count_len))) {
            return None;
        }
        let count = match version {
            Version::Cff => u32::from(u16_at(data, p)?),
            Version::Cff2 => u32_at(data, p)?,
        };
        let mut index = HbIndex {
            data,
            at: p,
            count,
            off_size: 0,
            count_len,
        };
        if count == 0 {
            return Some(index);
        }
        let count1 = count.checked_add(1)?;
        let off_at = add(p, u64::from(count_len));
        if !sc.check_point(add(off_at, 1)) {
            return None;
        }
        let off_size = u8_at(data, off_at)?;
        if !(1..=4).contains(&off_size) {
            return None;
        }
        index.off_size = off_size;
        if !sc.check_array(add(off_at, 1), u32::from(off_size), count1) {
            return None;
        }
        let last = index.offset_at(count);
        sc.check_range(index.data_base(), u64::from(last))
            .then_some(index)
    }

    /// The number of objects.
    pub fn len(&self) -> u32 {
        self.count
    }

    /// Whether there are no objects.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Where the INDEX starts.
    pub fn position(&self) -> i64 {
        self.at
    }

    /// `offset_at`: offset `i` as stored; 0 for a bad offset size.
    fn offset_at(&self, i: u32) -> u32 {
        let size = u64::from(self.off_size);
        let at = add(
            self.at,
            u64::from(self.count_len)
                .saturating_add(1)
                .saturating_add(u64::from(i).saturating_mul(size)),
        );
        pos(at)
            .and_then(|a| be::slice_at(self.data, a, usize::from(self.off_size)))
            .map_or(0, |b| b.iter().fold(0u32, |v, &x| (v << 8) | u32::from(x)))
    }

    /// `data_base`: one byte before the object data.
    fn data_base(&self) -> i64 {
        let array = u32::from(self.off_size).wrapping_mul(self.count.wrapping_add(1));
        add(
            self.at,
            u64::from(self.count_len).saturating_add(u64::from(array)),
        )
    }

    /// Object `i` with its position in the table, as [`HbIndex::get`].
    pub fn get_at(&self, i: u32) -> Option<(i64, &'a [u8])> {
        if i >= self.count {
            return None;
        }
        let off0 = self.offset_at(i);
        let off1 = self.offset_at(i.checked_add(1)?);
        if off1 < off0 || off1 > self.offset_at(self.count) {
            return None;
        }
        let at = add(self.data_base(), u64::from(off0));
        let start = pos(at)?;
        let bytes = self
            .data
            .get(start..start.saturating_add(to_usize(off1.saturating_sub(off0))))?;
        Some((at, bytes))
    }

    /// `operator []`: object `i`, or `None` (HarfBuzz's empty byte string
    /// without data, which fails sanitizing) when `i` is not below the
    /// count or the offsets are reversed or past the last one.
    pub fn get(&self, i: u32) -> Option<&'a [u8]> {
        self.get_at(i).map(|(_, b)| b)
    }

    /// `get_size` converted to the `int` offset HarfBuzz passes to
    /// `StructAtOffsetOrNull` for the next INDEX: on 64-bit machines the
    /// sum is not saturated, and the conversion keeps the low 32 bits.
    pub fn size_offset(&self) -> i32 {
        let size = if self.count == 0 {
            u64::from(self.count_len)
        } else {
            let array = u32::from(self.off_size).wrapping_mul(self.count.wrapping_add(1));
            u64::from(self.count_len)
                .saturating_add(1)
                .saturating_add(u64::from(array))
                .saturating_add(u64::from(self.offset_at(self.count).wrapping_sub(1)))
        };
        let low = u32::try_from(size & 0xFFFF_FFFF).unwrap_or(0);
        i32::from_ne_bytes(low.to_ne_bytes())
    }
}

/// `byte_str_ref_t`: a cursor over a byte string, in error once it moves
/// past the end.
#[derive(Clone, Copy, Debug)]
struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl Cursor<'_> {
    fn avail(&self, count: usize) -> bool {
        self.offset.saturating_add(count) <= self.data.len()
    }

    fn inc(&mut self, count: usize) {
        self.offset = self.offset.saturating_add(count);
    }

    fn set_error(&mut self) {
        self.offset = self.data.len().saturating_add(1);
    }

    fn in_error(&self) -> bool {
        self.offset > self.data.len()
    }

    /// `operator []`: the byte `i` after the cursor; 0 and in error past
    /// the end.
    fn byte(&mut self, i: usize) -> u8 {
        if let Some(&b) = self.data.get(self.offset.saturating_add(i)) {
            b
        } else {
            self.set_error();
            0
        }
    }

    fn fetch_op(&mut self) -> u32 {
        let Some(&b) = self.data.get(self.offset) else {
            return OP_INVALID;
        };
        self.inc(1);
        if b != 12 {
            return u32::from(b);
        }
        let Some(&e) = self.data.get(self.offset) else {
            return OP_INVALID;
        };
        self.inc(1);
        u32::from(e).saturating_add(256)
    }
}

/// `OpCode_Invalid`.
const OP_INVALID: u32 = 0xFFFF;

/// The argument stack of HarfBuzz's DICT interpreter: 513 `double`s.
struct Stack {
    values: [f64; 513],
    len: usize,
    error: bool,
}

impl Stack {
    fn new() -> Stack {
        Stack {
            values: [0.0; 513],
            len: 0,
            error: false,
        }
    }

    fn push(&mut self, v: f64) {
        if let Some(slot) = self.values.get_mut(self.len) {
            *slot = v;
            self.len = self.len.saturating_add(1);
        } else {
            self.error = true;
        }
    }

    fn pop(&mut self) -> f64 {
        if let Some(n) = self.len.checked_sub(1) {
            self.len = n;
            self.values.get(n).copied().unwrap_or(0.0)
        } else {
            self.error = true;
            0.0
        }
    }

    /// `pop_int`: the number truncated toward zero; an error outside the
    /// `int` range.
    fn pop_int(&mut self) -> i32 {
        let v = self.pop();
        to_int(v).unwrap_or_else(|| {
            self.error = true;
            0
        })
    }

    /// `pop_uint`: [`Stack::pop_int`], an error below 0.
    fn pop_uint(&mut self) -> u32 {
        let i = self.pop_int();
        u32::try_from(i).unwrap_or_else(|_| {
            self.error = true;
            0
        })
    }

    fn clear(&mut self) {
        self.len = 0;
    }
}

/// `number_t::to_int`: `v` truncated toward zero when it is in the
/// `int` range.
fn to_int(v: f64) -> Option<i32> {
    if v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX) {
        // In the i32 range, checked above; `as` truncates toward zero.
        #[allow(clippy::cast_possible_truncation)]
        Some(v as i32)
    } else {
        None
    }
}

/// `opset_t::process_op` and `dict_opset_t::process_op`: numbers are
/// pushed, other operators clear the stack.
fn process_number(op: u32, cur: &mut Cursor<'_>, stack: &mut Stack) {
    match op {
        28 => {
            let v = u16::from_be_bytes([cur.byte(0), cur.byte(1)]);
            stack.push(f64::from(i16::from_ne_bytes(v.to_ne_bytes())));
            cur.inc(2);
        }
        29 => {
            let v = u32::from_be_bytes([cur.byte(0), cur.byte(1), cur.byte(2), cur.byte(3)]);
            stack.push(f64::from(i32::from_ne_bytes(v.to_ne_bytes())));
            cur.inc(4);
        }
        30 => {
            let v = parse_bcd(cur);
            stack.push(v);
        }
        247..=250 => {
            let v = i32::try_from(op)
                .unwrap_or(0)
                .saturating_sub(247)
                .saturating_mul(256)
                .saturating_add(i32::from(cur.byte(0)))
                .saturating_add(108);
            stack.push(f64::from(v));
            cur.inc(1);
        }
        251..=254 => {
            let v = 251i32
                .saturating_sub(i32::try_from(op).unwrap_or(0))
                .saturating_mul(256)
                .saturating_sub(i32::from(cur.byte(0)))
                .saturating_sub(108);
            stack.push(f64::from(v));
            cur.inc(1);
        }
        32..=246 => {
            stack.push(f64::from(
                i32::try_from(op).unwrap_or(0).saturating_sub(139),
            ));
        }
        _ => stack.clear(),
    }
}

/// `dict_opset_t::parse_bcd`: a real number; 0 and the cursor in error
/// when it is malformed.
fn parse_bcd(cur: &mut Cursor<'_>) -> f64 {
    const MAX_BCD_CHARS: u32 = 32;
    const CHARS: &[u8; 16] = b"0123456789.EE?-?";
    if cur.in_error() {
        return 0.0;
    }
    let mut parser = NumberParser::default();
    let mut byte = 0u8;
    let mut i = 0u32;
    let mut count = 0u32;
    while count < MAX_BCD_CHARS {
        let nibble = if i & 1 == 0 {
            if !cur.avail(1) {
                break;
            }
            byte = cur.byte(0);
            cur.inc(1);
            byte >> 4
        } else {
            byte & 0x0F
        };
        if nibble == 13 {
            break;
        } else if nibble == 15 {
            let length = parser.commit();
            if length == 0 || length != count {
                break;
            }
            return parser.value();
        } else {
            let c = CHARS.get(usize::from(nibble)).copied().unwrap_or(b'?');
            if !parser.consume(c) {
                break;
            }
            if nibble == 12 {
                count = count.saturating_add(1);
                if count == MAX_BCD_CHARS || !parser.consume(b'-') {
                    break;
                }
            }
        }
        i = i.saturating_add(1);
        count = count.saturating_add(1);
    }
    cur.set_error();
    0.0
}

const SMALL_POWERS_OF_10: [f64; 16] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15,
];
const LARGE_POWERS_OF_10: [f64; 20] = [
    1e0, 1e16, 1e32, 1e48, 1e64, 1e80, 1e96, 1e112, 1e128, 1e144, 1e160, 1e176, 1e192, 1e208,
    1e224, 1e240, 1e256, 1e272, 1e288, 1e304,
];

/// `hb_number_parser_t::_pow10`.
fn pow10(n: u32) -> f64 {
    if n > 308 {
        return f64::INFINITY;
    }
    let small = |k: u32| {
        SMALL_POWERS_OF_10
            .get(to_usize(k & 15))
            .copied()
            .unwrap_or(1.0)
    };
    if n < 16 {
        return small(n);
    }
    LARGE_POWERS_OF_10
        .get(to_usize(n >> 4))
        .copied()
        .unwrap_or(f64::INFINITY)
        * small(n)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum ParseState {
    #[default]
    Start,
    Sign,
    IntDigits,
    AfterDotFromInt,
    AfterDotFromStart,
    FracDigits,
    ExpStart,
    ExpSign,
    ExpDigits,
}

/// The fraction digits of `hb_number_parser_t`.
#[derive(Clone, Copy, Debug, Default)]
struct Fraction {
    count: u32,
    fraction: u64,
    drop: Option<u8>,
    sticky: bool,
}

impl Fraction {
    const MAX: u64 = 10_000_000_000_000_000_000;

    fn add(&mut self, digit: u8) {
        if self.fraction <= Fraction::MAX / 10 {
            self.fraction = self
                .fraction
                .saturating_mul(10)
                .saturating_add(u64::from(digit));
            self.count = self.count.saturating_add(1);
        } else if self.drop.is_none() {
            self.drop = Some(digit);
        } else if digit != 0 {
            self.sticky = true;
        }
    }

    fn value(&self) -> f64 {
        // (double) fraction rounds to nearest, as in C.
        #[allow(clippy::cast_precision_loss)]
        let mut result = self.fraction as f64;
        let drop = self.drop.unwrap_or(0);
        if self.drop.is_some()
            && (drop > 5 || (drop == 5 && (self.sticky || self.fraction & 1 == 1)))
        {
            result += 1.0;
        }
        result / pow10(self.count)
    }
}

/// HarfBuzz's `hb_number_parser_t`, fed one character at a time.
#[derive(Clone, Copy, Debug, Default)]
struct NumberParser {
    state: ParseState,
    pre_exp_state: ParseState,
    value: f64,
    fraction: Fraction,
    exp: u32,
    neg: bool,
    exp_neg: bool,
    exp_overflow: bool,
    exp_speculation_active: bool,
    pos: u32,
    pre_exp_pos: u32,
}

impl NumberParser {
    fn digit(&mut self, c: u8) {
        self.value = self.value * 10.0 + f64::from(c.saturating_sub(b'0'));
        self.pos = self.pos.saturating_add(1);
    }

    fn enter_exponent(&mut self) {
        self.pre_exp_state = self.state;
        self.pre_exp_pos = self.pos;
        self.exp_speculation_active = true;
        self.state = ParseState::ExpStart;
        self.pos = self.pos.saturating_add(1);
    }

    fn rollback_exponent(&mut self) {
        if !self.exp_speculation_active {
            return;
        }
        self.state = self.pre_exp_state;
        self.pos = self.pre_exp_pos;
        self.exp = 0;
        self.exp_neg = false;
        self.exp_overflow = false;
        self.exp_speculation_active = false;
    }

    fn frac(&mut self, c: u8) {
        self.fraction.add(c.saturating_sub(b'0'));
        self.state = ParseState::FracDigits;
        self.pos = self.pos.saturating_add(1);
    }

    /// `consume`: whether `c` was accepted.
    fn consume(&mut self, c: u8) -> bool {
        use ParseState as S;
        let digit = c.is_ascii_digit();
        let e = c == b'e' || c == b'E';
        match self.state {
            S::Start | S::Sign => {
                if self.state == S::Start && (c == b'+' || c == b'-') {
                    self.neg = c == b'-';
                    self.state = S::Sign;
                    self.pos = self.pos.saturating_add(1);
                } else if digit {
                    self.digit(c);
                    self.state = S::IntDigits;
                } else if c == b'.' {
                    self.state = S::AfterDotFromStart;
                    self.pos = self.pos.saturating_add(1);
                } else {
                    return false;
                }
                true
            }
            S::IntDigits => {
                if digit {
                    self.digit(c);
                } else if c == b'.' {
                    self.state = S::AfterDotFromInt;
                    self.pos = self.pos.saturating_add(1);
                } else if e {
                    self.enter_exponent();
                } else {
                    return false;
                }
                true
            }
            S::AfterDotFromInt | S::FracDigits => {
                if digit {
                    self.frac(c);
                } else if e {
                    self.enter_exponent();
                } else {
                    return false;
                }
                true
            }
            S::AfterDotFromStart => {
                if digit {
                    self.frac(c);
                    true
                } else {
                    false
                }
            }
            S::ExpStart | S::ExpSign => {
                if self.state == S::ExpStart && (c == b'+' || c == b'-') {
                    self.exp_neg = c == b'-';
                    self.state = S::ExpSign;
                    self.pos = self.pos.saturating_add(1);
                    true
                } else if digit {
                    self.exp = u32::from(c.saturating_sub(b'0'));
                    self.state = S::ExpDigits;
                    self.pos = self.pos.saturating_add(1);
                    true
                } else {
                    self.rollback_exponent();
                    false
                }
            }
            S::ExpDigits => {
                if digit {
                    const MAX_EXP: u32 = 0x7FF;
                    let e = self
                        .exp
                        .saturating_mul(10)
                        .saturating_add(u32::from(c.saturating_sub(b'0')));
                    if e <= MAX_EXP {
                        self.exp = e;
                    } else {
                        self.exp_overflow = true;
                    }
                    self.pos = self.pos.saturating_add(1);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// `commit`: the accepted length, 0 when no number was parsed.
    fn commit(&mut self) -> u32 {
        if matches!(self.state, ParseState::ExpStart | ParseState::ExpSign) {
            self.rollback_exponent();
        }
        if matches!(self.state, ParseState::Sign | ParseState::AfterDotFromStart) {
            0
        } else {
            self.pos
        }
    }

    /// `get_value`.
    fn value(&self) -> f64 {
        let mut v = self.value;
        if self.fraction.count != 0 {
            v += self.fraction.value();
        }
        if self.neg {
            v *= -1.0;
        }
        if v == 0.0 {
            return v;
        }
        if self.exp_overflow {
            return match (self.exp_neg, self.neg) {
                (true, true) => -0.0,
                (true, false) => 0.0,
                (false, true) => f64::NEG_INFINITY,
                (false, false) => f64::INFINITY,
            };
        }
        if self.exp != 0 {
            let mut e = self.exp;
            if self.exp_neg {
                if e > 308 {
                    v /= 1e308;
                    e = e.saturating_sub(308);
                    if e > 308 {
                        v /= 1e308;
                        e = e.saturating_sub(308);
                    }
                }
                v /= pow10(e);
            } else {
                v *= pow10(e);
            }
        }
        v
    }
}

/// Which of HarfBuzz's DICT operator sets interprets a DICT.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DictKind {
    /// `cff1_top_dict_opset_t`.
    Cff1Top,
    /// `cff1_font_dict_opset_t`.
    Cff1Font,
    /// `cff1_private_dict_opset_t`.
    Cff1Private,
    /// `cff2_top_dict_opset_t`.
    Cff2Top,
    /// `cff2_font_dict_opset_t`.
    Cff2Font,
    /// `cff2_private_dict_opset_t`.
    Cff2Private,
}

/// The DICT values HarfBuzz keeps that its accelerators use, with
/// HarfBuzz's initial values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HbDict {
    /// The registry SID of `ROS` ([`UNDEF_SID`] without `ROS`): CFF Top.
    pub registry: u32,
    /// `charset` (CFF Top).
    pub charset: i32,
    /// `Encoding` (CFF Top).
    pub encoding: i32,
    /// `CharStrings` (Top).
    pub char_strings: i32,
    /// `FDArray` (Top).
    pub fd_array: i32,
    /// `FDSelect` (Top).
    pub fd_select: i32,
    /// `vstore` (CFF2 Top).
    pub vstore: i32,
    /// The `Private` offset (CFF Top, Font DICTs), as HarfBuzz passes it
    /// on: an `int`.
    pub private_offset: i32,
    /// The `Private` size.
    pub private_size: u32,
    /// `Subrs` (Private).
    pub subrs: i32,
    /// `vsindex` (CFF2 Private).
    pub ivs: u32,
}

impl Default for HbDict {
    fn default() -> HbDict {
        HbDict {
            registry: UNDEF_SID,
            charset: 0,
            encoding: 0,
            char_strings: 0,
            fd_array: 0,
            fd_select: 0,
            vstore: 0,
            private_offset: 0,
            private_size: 0,
            subrs: 0,
            ivs: 0,
        }
    }
}

const CHARSET: u32 = 15;
const ENCODING: u32 = 16;
const CHAR_STRINGS: u32 = 17;
const PRIVATE: u32 = 18;
const SUBRS: u32 = 19;
const VSINDEX: u32 = 22;
const BLEND: u32 = 23;
const VSTORE: u32 = 24;
const ROS: u32 = 256 + 30;
const CID_COUNT: u32 = 256 + 34;
const FD_ARRAY: u32 = 256 + 36;
const FD_SELECT: u32 = 256 + 37;
const FONT_NAME: u32 = 256 + 38;
const FONT_MATRIX: u32 = 256 + 7;
const PAINT_TYPE: u32 = 256 + 5;

/// Whether `op` only clears the stack in HarfBuzz's CFF Top DICT: the
/// name SIDs pop one operand but keep nothing we use.
fn cff1_top_ignored(op: u32) -> bool {
    matches!(
        op,
        257..=262 | 264 | 276 | 279 | 287..=289 | 291 | 5 | 13 | 14
    )
}

/// The name SID operators of the CFF Top DICT (`version`, `Notice`,
/// `Copyright`, `FullName`, `FontName`, `FamilyName`, `Weight`,
/// `PostScript`, `BaseFontName`): one `pop_uint` each.
fn cff1_top_name(op: u32) -> bool {
    matches!(op, 0..=4 | 256 | FONT_NAME | 277 | 278)
}

/// Hint operators that HarfBuzz's Private DICT operator sets clear.
fn private_hint(op: u32, cff2: bool) -> bool {
    let shared = matches!(
        op,
        6..=11 | 265..=269 | 273 | 274
    );
    if cff2 {
        shared
    } else {
        shared || matches!(op, 270 | 275 | 20 | 21)
    }
}

/// `dict_interpreter_t::interpret` with HarfBuzz's operator set for
/// `kind`: the values, or `None` when HarfBuzz's interpreter fails
/// (a stack overflow or underflow, a number out of range, a malformed
/// number).
pub fn interpret_dict(data: &[u8], kind: DictKind) -> Option<HbDict> {
    let mut cur = Cursor { data, offset: 0 };
    let mut stack = Stack::new();
    let mut d = HbDict::default();
    let mut seen_vsindex = false;
    while cur.avail(1) {
        let op = cur.fetch_op();
        match kind {
            DictKind::Cff1Top => match op {
                op if cff1_top_name(op) => {
                    stack.pop_uint();
                    stack.clear();
                }
                CID_COUNT => {
                    stack.pop_uint();
                    stack.clear();
                }
                ROS => {
                    stack.pop_uint();
                    stack.pop_uint();
                    d.registry = stack.pop_uint();
                    stack.clear();
                }
                ENCODING => {
                    d.encoding = stack.pop_int();
                    stack.clear();
                }
                CHARSET => {
                    d.charset = stack.pop_int();
                    stack.clear();
                }
                FD_SELECT => {
                    d.fd_select = stack.pop_int();
                    stack.clear();
                }
                PRIVATE => {
                    d.private_offset = stack.pop_int();
                    d.private_size = stack.pop_uint();
                    stack.clear();
                }
                op if cff1_top_ignored(op) => stack.clear(),
                CHAR_STRINGS => {
                    d.char_strings = stack.pop_int();
                    stack.clear();
                }
                FD_ARRAY => {
                    d.fd_array = stack.pop_int();
                    stack.clear();
                }
                FONT_MATRIX => stack.clear(),
                op => process_number(op, &mut cur, &mut stack),
            },
            DictKind::Cff1Font => match op {
                FONT_NAME => {
                    stack.pop_uint();
                    stack.clear();
                }
                FONT_MATRIX | PAINT_TYPE => stack.clear(),
                PRIVATE => {
                    let o = stack.pop_uint();
                    d.private_offset = i32::from_ne_bytes(o.to_ne_bytes());
                    d.private_size = stack.pop_uint();
                    stack.clear();
                }
                op => process_number(op, &mut cur, &mut stack),
            },
            DictKind::Cff1Private | DictKind::Cff2Private => {
                let cff2 = kind == DictKind::Cff2Private;
                match op {
                    op if private_hint(op, cff2) => stack.clear(),
                    SUBRS => {
                        d.subrs = stack.pop_int();
                        stack.clear();
                    }
                    VSINDEX if cff2 => {
                        if !seen_vsindex {
                            d.ivs = stack.pop_uint();
                        }
                        seen_vsindex = true;
                        stack.clear();
                    }
                    BLEND if cff2 => stack.clear(),
                    op => process_number(op, &mut cur, &mut stack),
                }
            }
            DictKind::Cff2Top => match op {
                FONT_MATRIX => stack.clear(),
                VSTORE => {
                    d.vstore = stack.pop_int();
                    stack.clear();
                }
                FD_SELECT => {
                    d.fd_select = stack.pop_int();
                    stack.clear();
                }
                CHAR_STRINGS => {
                    d.char_strings = stack.pop_int();
                    stack.clear();
                }
                FD_ARRAY => {
                    d.fd_array = stack.pop_int();
                    stack.clear();
                }
                op => process_number(op, &mut cur, &mut stack),
            },
            DictKind::Cff2Font => match op {
                PRIVATE => {
                    let o = stack.pop_uint();
                    d.private_offset = i32::from_ne_bytes(o.to_ne_bytes());
                    d.private_size = stack.pop_uint();
                    stack.clear();
                }
                op => process_number(op, &mut cur, &mut stack),
            },
        }
        if cur.in_error() || stack.error {
            return None;
        }
    }
    Some(d)
}

/// A charset as HarfBuzz's CFF accelerator keeps it: a predefined one
/// (HarfBuzz's Null charset, by the `charset` offset of 2 or less) or
/// the sanitized data.
#[derive(Clone, Copy, Debug)]
pub enum HbCharset<'a> {
    /// The Top DICT `charset` value: 0 ISOAdobe, 1 Expert, 2
    /// ExpertSubset; others (negative) have no SIDs.
    Predefined(i32),
    /// Format 0, 1 or 2 at a position in the table.
    Data {
        /// The table.
        data: &'a [u8],
        /// Where the format byte is.
        at: i64,
        /// The format.
        format: u8,
    },
}

/// `Charset::sanitize` for the charset at `p` with the face's glyph
/// count `face_glyphs` (`sc.get_num_glyphs`).
pub fn sanitize_charset<'a>(
    data: &'a [u8],
    p: i64,
    face_glyphs: u32,
    sc: &mut Sanitizer,
) -> Option<HbCharset<'a>> {
    if !sc.check_point(add(p, 1)) {
        return None;
    }
    let format = u8_at(data, p)?;
    let body = add(p, 1);
    let ok = match format {
        0 => sc.check_array(body, face_glyphs.wrapping_sub(1), 2),
        1 | 2 => {
            let size: u64 = if format == 1 { 3 } else { 4 };
            let mut left = face_glyphs.wrapping_sub(1);
            let mut at = body;
            let mut ok = true;
            while left > 0 {
                if !sc.check_point(add(at, size)) {
                    ok = false;
                    break;
                }
                let n_left = if format == 1 {
                    u32::from(u8_at(data, add(at, 2)).unwrap_or(0))
                } else {
                    u32::from(u16_at(data, add(at, 2)).unwrap_or(0))
                };
                let n = n_left.saturating_add(1);
                if left < n {
                    ok = false;
                    break;
                }
                left = left.saturating_sub(n);
                at = add(at, size);
            }
            ok
        }
        _ => false,
    };
    ok.then_some(HbCharset::Data {
        data,
        at: p,
        format,
    })
}

impl HbCharset<'_> {
    fn range(data: &[u8], at: i64, format: u8, i: u32) -> Option<(u32, u32)> {
        let size: u64 = if format == 1 { 3 } else { 4 };
        let r = add(at, 1u64.saturating_add(u64::from(i).saturating_mul(size)));
        let first = u32::from(u16_at(data, r)?);
        let n_left = if format == 1 {
            u32::from(u8_at(data, add(r, 2))?)
        } else {
            u32::from(u16_at(data, add(r, 2))?)
        };
        Some((first, n_left))
    }

    /// `glyph_to_sid` for `num_glyphs` glyphs (the CharStrings count): the
    /// SID of `glyph`, 0 past the glyphs.
    pub fn sid(&self, glyph: u32, num_glyphs: u32) -> u32 {
        match *self {
            HbCharset::Predefined(offset) => {
                let at = |t: &[u16]| t.get(to_usize(glyph)).map_or(0, |&s| u32::from(s));
                match offset {
                    0 if glyph <= 228 => glyph,
                    1 => at(&EXPERT_CHARSET),
                    2 => at(&EXPERT_SUBSET_CHARSET),
                    _ => 0,
                }
            }
            HbCharset::Data { data, at, format } => {
                if glyph >= num_glyphs || glyph == 0 {
                    return 0;
                }
                if format == 0 {
                    let p = add(
                        at,
                        1u64.saturating_add(u64::from(glyph.saturating_sub(1)).saturating_mul(2)),
                    );
                    return u16_at(data, p).map_or(0, u32::from);
                }
                let mut g = glyph.saturating_sub(1);
                let mut i = 0u32;
                while let Some((first, n_left)) = HbCharset::range(data, at, format, i) {
                    if g <= n_left {
                        return first.saturating_add(g);
                    }
                    g = g.saturating_sub(n_left.saturating_add(1));
                    i = i.saturating_add(1);
                }
                0
            }
        }
    }

    /// `sid_to_glyph` for `num_glyphs` glyphs: the first glyph with
    /// `sid`, 0 when there is none.
    pub fn glyph(&self, sid: u32, num_glyphs: u32) -> u32 {
        match *self {
            HbCharset::Predefined(offset) => {
                let find = |t: &[u16]| {
                    if sid == 0 {
                        return 0;
                    }
                    (0u32..)
                        .zip(t)
                        .find(|&(_, &s)| u32::from(s) == sid)
                        .map_or(0, |(g, _)| g)
                };
                match offset {
                    0 if sid <= 228 => sid,
                    1 => find(&EXPERT_CHARSET),
                    2 => find(&EXPERT_SUBSET_CHARSET),
                    _ => 0,
                }
            }
            HbCharset::Data { data, at, format } => {
                if sid == 0 {
                    return 0;
                }
                if format == 0 {
                    return (1..num_glyphs)
                        .find(|&g| {
                            let p = add(
                                at,
                                1u64.saturating_add(
                                    u64::from(g.saturating_sub(1)).saturating_mul(2),
                                ),
                            );
                            u16_at(data, p).map(u32::from) == Some(sid)
                        })
                        .unwrap_or(0);
                }
                let mut glyph = 1u32;
                let mut i = 0u32;
                loop {
                    if glyph >= num_glyphs {
                        return 0;
                    }
                    let Some((first, n_left)) = HbCharset::range(data, at, format, i) else {
                        return 0;
                    };
                    if first <= sid && sid <= first.saturating_add(n_left) {
                        return glyph.saturating_add(sid.saturating_sub(first));
                    }
                    glyph = glyph.saturating_add(n_left.saturating_add(1));
                    i = i.saturating_add(1);
                }
            }
        }
    }

    /// `std_code_to_glyph`: the glyph of a Standard encoding code (seac),
    /// 0 when there is none.
    pub fn std_code_to_glyph(&self, code: u32, num_glyphs: u32) -> u32 {
        let Some(&sid) = STANDARD_ENCODING.get(to_usize(code)) else {
            return 0;
        };
        let sid = u32::from(sid);
        match *self {
            HbCharset::Data { .. } => self.glyph(sid, num_glyphs),
            HbCharset::Predefined(0) if code <= 228 => sid,
            HbCharset::Predefined(_) => 0,
        }
    }
}

/// `Encoding::sanitize` for the encoding at `p`.
pub fn sanitize_encoding(data: &[u8], p: i64, sc: &mut Sanitizer) -> bool {
    if !sc.check_point(add(p, 1)) {
        return false;
    }
    let Some(format) = u8_at(data, p) else {
        return false;
    };
    let len_at = add(p, 1);
    if !sc.check_point(add(len_at, 1)) {
        return false;
    }
    let n = u32::from(u8_at(data, len_at).unwrap_or(0));
    let elem = match format & 0x7F {
        0 => 1u32,
        1 => 2,
        _ => return false,
    };
    if !sc.check_range(add(len_at, 1), u64::from(n.saturating_mul(elem))) {
        return false;
    }
    if format & 0x80 == 0 {
        return true;
    }
    if n == 0 {
        // The supplement after element -1 is HarfBuzz's Null object,
        // outside the table.
        return false;
    }
    let supp = add(
        len_at,
        1u64.saturating_add(u64::from(n.saturating_mul(elem))),
    );
    if !sc.check_point(add(supp, 1)) {
        return false;
    }
    let s = u32::from(u8_at(data, supp).unwrap_or(0));
    sc.check_range(add(supp, 1), u64::from(s.saturating_mul(3)))
}

/// A sanitized FDSelect, or HarfBuzz's Null FDSelect (every glyph in
/// Font DICT 0).
#[derive(Clone, Copy, Debug)]
pub struct HbFdSelect<'a> {
    data: &'a [u8],
    at: i64,
    format: u8,
}

/// The counts an FDSelect is sanitized against.
#[derive(Clone, Copy, Debug)]
pub struct FdSelectCounts {
    /// The FDArray count: ranges must select Font DICTs below it.
    pub fd_count: u32,
    /// The glyphs of format 0, and the least sentinel of formats 3 and
    /// 4: the face's glyph count in CFF, the CharStrings count in CFF2.
    pub glyphs: u32,
    /// The face's glyph count: range starts must be below it, the
    /// sentinel not above it.
    pub face_glyphs: u32,
}

impl<'a> HbFdSelect<'a> {
    /// HarfBuzz's Null FDSelect.
    pub const fn null() -> HbFdSelect<'static> {
        HbFdSelect {
            data: &[],
            at: 0,
            format: 0xFF,
        }
    }

    /// `FDSelect::sanitize` (CFF: formats 0 and 3) or
    /// `CFF2FDSelect::sanitize` (CFF2: also format 4) for the FDSelect at
    /// `p`.
    pub fn sanitize(
        data: &'a [u8],
        p: i64,
        version: Version,
        counts: FdSelectCounts,
        sc: &mut Sanitizer,
    ) -> Option<HbFdSelect<'a>> {
        let min_size: u64 = match version {
            Version::Cff => 1,
            Version::Cff2 => 2,
        };
        if !sc.check_point(add(p, min_size)) {
            return None;
        }
        let format = u8_at(data, p)?;
        let body = add(p, 1);
        let ok = match (format, version) {
            (0, _) => sc.check_point(body) && sc.check_array(body, counts.glyphs, 1),
            (3, _) => sanitize_ranges(data, body, 2, counts, sc),
            (4, Version::Cff2) => sanitize_ranges(data, body, 4, counts, sc),
            _ => false,
        };
        ok.then_some(HbFdSelect {
            data,
            at: p,
            format,
        })
    }

    fn range(&self, gid_size: u8, i: u32) -> Option<(u32, u32)> {
        let size = u64::from(gid_size).saturating_add(u64::from(gid_size / 2));
        let r = add(
            self.at,
            1u64.saturating_add(u64::from(gid_size))
                .saturating_add(u64::from(i).saturating_mul(size)),
        );
        if gid_size == 2 {
            Some((
                u32::from(u16_at(self.data, r)?),
                u32::from(u8_at(self.data, add(r, 2))?),
            ))
        } else {
            Some((
                u32_at(self.data, r)?,
                u32::from(u16_at(self.data, add(r, 4))?),
            ))
        }
    }

    /// `get_fd`: the Font DICT of `glyph`.
    pub fn fd(&self, glyph: u32) -> u32 {
        let gid_size = match self.format {
            0 => {
                let p = add(self.at, 1u64.saturating_add(u64::from(glyph)));
                return u8_at(self.data, p).map_or(0, u32::from);
            }
            3 => 2u8,
            4 => 4,
            _ => return 0,
        };
        let n = if gid_size == 2 {
            u16_at(self.data, add(self.at, 1)).map_or(0, u32::from)
        } else {
            u32_at(self.data, add(self.at, 1)).unwrap_or(0)
        };
        // hb_bsearch over ranges 0..n-1, each covering up to the next
        // range's first glyph; the last range's fd otherwise.
        let (mut lo, mut hi) = (0u32, n.saturating_sub(1));
        while lo < hi {
            let mid = lo.saturating_add(hi.saturating_sub(lo) / 2);
            let (Some((first, fd)), Some((next, _))) = (
                self.range(gid_size, mid),
                self.range(gid_size, mid.saturating_add(1)),
            ) else {
                return 0;
            };
            if glyph < first {
                hi = mid;
            } else if glyph < next {
                return fd;
            } else {
                lo = mid.saturating_add(1);
            }
        }
        self.range(gid_size, n.saturating_sub(1))
            .map_or(0, |(_, fd)| fd)
    }
}

/// `FDSelect3_4::sanitize` with `gid_size`-byte glyph ids (2: format 3,
/// 4: format 4) for the ranges after the format byte at `p`.
fn sanitize_ranges(
    data: &[u8],
    p: i64,
    gid_size: u8,
    counts: FdSelectCounts,
    sc: &mut Sanitizer,
) -> bool {
    let g = u64::from(gid_size);
    let range_size = g.saturating_add(g / 2);
    // check_struct (this) and the ArrayOf's length.
    if !sc.check_point(add(p, g)) {
        return false;
    }
    let read_gid = |at: i64| {
        if gid_size == 2 {
            u16_at(data, at).map(u32::from)
        } else {
            u32_at(data, at)
        }
    };
    let read_fd = |at: i64| {
        if gid_size == 2 {
            u8_at(data, at).map(u32::from)
        } else {
            u16_at(data, at).map(u32::from)
        }
    };
    let Some(n) = read_gid(p) else {
        return false;
    };
    let ranges = add(p, g);
    let range_bytes = u32::try_from(range_size).unwrap_or(0);
    let shallow = if gid_size >= 4 {
        sc.check_array(ranges, n, range_bytes)
    } else {
        sc.check_range(ranges, u64::from(n.wrapping_mul(range_bytes)))
    };
    if !shallow {
        return false;
    }
    let at = |i: u32| add(ranges, u64::from(i).saturating_mul(range_size));
    let mut previous = None;
    for i in 0..n {
        let r = at(i);
        if !sc.check_point(add(r, range_size)) {
            return false;
        }
        let (Some(first), Some(fd)) = (read_gid(r), read_fd(add(r, g))) else {
            return false;
        };
        if first >= counts.face_glyphs || fd >= counts.fd_count {
            return false;
        }
        previous = Some((i, first));
    }
    let Some((_, last_first)) = previous else {
        return false;
    };
    if read_gid(at(0)) != Some(0) {
        return false;
    }
    for i in 1..n {
        let (Some(a), Some(b)) = (read_gid(at(i.saturating_sub(1))), read_gid(at(i))) else {
            return false;
        };
        if a >= b {
            return false;
        }
    }
    let sentinel_at = at(n);
    if !sc.check_point(add(sentinel_at, g)) {
        return false;
    }
    let Some(sentinel) = read_gid(sentinel_at) else {
        return false;
    };
    sentinel > last_first && sentinel <= counts.face_glyphs && sentinel >= counts.glyphs
}

/// `CFF2ItemVariationStore::sanitize` for the store at `p` (its 2-byte
/// length first).
pub fn sanitize_cff2_var_store(data: &[u8], p: i64, sc: &mut Sanitizer) -> bool {
    if !sc.check_point(add(p, 10)) {
        return false;
    }
    let Some(size) = u16_at(data, p) else {
        return false;
    };
    let vs = add(p, 2);
    sc.check_range(vs, u64::from(size)) && sanitize_var_store(data, vs, sc)
}

/// `ItemVariationStore::sanitize` for the store at `vs`.
fn sanitize_var_store(data: &[u8], vs: i64, sc: &mut Sanitizer) -> bool {
    if !sc.check_point(add(vs, 8)) || u16_at(data, vs) != Some(1) {
        return false;
    }
    // regions: Offset32To<VarRegionList>.
    if !sc.check_point(add(vs, 6)) {
        return false;
    }
    let Some(regions) = u32_at(data, add(vs, 2)) else {
        return false;
    };
    if regions != 0 {
        let r = add(vs, u64::from(regions));
        if !sc.check_point(add(r, 4)) {
            return false;
        }
        let axes = u32::from(u16_at(data, r).unwrap_or(0));
        let count = u32::from(u16_at(data, add(r, 2)).unwrap_or(0) & 0x7FFF);
        if !sc.check_array(add(r, 4), axes.saturating_mul(count), 6) {
            return false;
        }
    }
    // dataSets: Array16OfOffset32To<VarData>.
    let len_at = add(vs, 6);
    if !sc.check_point(add(len_at, 2)) {
        return false;
    }
    let n = u32::from(u16_at(data, len_at).unwrap_or(0));
    let offsets = add(len_at, 2);
    if !sc.check_range(offsets, u64::from(n.saturating_mul(4))) {
        return false;
    }
    (0..n).all(|i| {
        let o = add(offsets, u64::from(i).saturating_mul(4));
        if !sc.check_point(add(o, 4)) {
            return false;
        }
        let Some(off) = u32_at(data, o) else {
            return false;
        };
        off == 0 || sanitize_var_data(data, add(vs, u64::from(off)), sc)
    })
}

/// `VarData::sanitize` for the subtable at `p`.
fn sanitize_var_data(data: &[u8], p: i64, sc: &mut Sanitizer) -> bool {
    if !sc.check_point(add(p, 6)) {
        return false;
    }
    let item_count = u32::from(u16_at(data, p).unwrap_or(0));
    let word_size_count = u16_at(data, add(p, 2)).unwrap_or(0);
    let n = u32::from(u16_at(data, add(p, 4)).unwrap_or(0));
    if !sc.check_range(add(p, 6), u64::from(n.saturating_mul(2))) {
        return false;
    }
    let word_count = u32::from(word_size_count & 0x7FFF);
    if word_count > n {
        return false;
    }
    let long = word_size_count & 0x8000 != 0;
    let row = word_count
        .saturating_add(n)
        .saturating_mul(if long { 2 } else { 1 });
    let deltas = add(p, 6u64.saturating_add(u64::from(n.saturating_mul(2))));
    sc.check_array(deltas, item_count, row)
}

/// `get_region_index_count (major)` of the CFF2 Item Variation Store at
/// `p` (its 2-byte length first), sanitized: the regions of
/// ItemVariationData `ivs`, 0 when there is no such subtable.
pub fn var_store_region_count(data: &[u8], p: i64, ivs: u32) -> u32 {
    let vs = add(p, 2);
    let n = u16_at(data, add(vs, 6)).map_or(0, u32::from);
    if ivs >= n {
        return 0;
    }
    let o = add(vs, 8u64.saturating_add(u64::from(ivs).saturating_mul(4)));
    match u32_at(data, o) {
        Some(0) | None => 0,
        Some(off) => u16_at(data, add(add(vs, u64::from(off)), 4)).map_or(0, u32::from),
    }
}
