//! CFF DICTs as HarfBuzz's subsetter keeps them: the operators with the
//! bytes they were read from (`dict_values_t`, `parsed_values_t::add_op`)
//! and the values it renumbers, as its operator sets for the Top DICT
//! (`cff1_top_dict_opset_t`), the Font DICTs (`cff1_font_dict_opset_t`)
//! and the Private DICTs (`cff1_private_dict_opset_subset_t`) record them
//! (`src/hb-ot-cff1-table.hh`, `src/hb-cff-interp-dict-common.hh`).

use alloc::vec::Vec;

use sofel_font::read::cff::hb::{DictToken, UNDEF_SID, dict_token};

use super::cs::to_int;

/// The argument stack limit.
const MAX_ARGS: usize = 513;

/// Which operator set reads a DICT.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Top,
    Font,
    Private,
}

/// One recorded operator: `data[start..end]` holds it and the operands
/// (and anything before them that was not recorded).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct DictOp {
    pub op: u32,
    pub start: usize,
    pub end: usize,
    /// `ROS`: where its last operand starts, from `start`.
    pub last_arg: usize,
}

impl DictOp {
    /// An operator without bytes (`add_op` without a string).
    pub fn empty(op: u32) -> DictOp {
        DictOp {
            op,
            start: 0,
            end: 0,
            last_arg: 0,
        }
    }
}

/// Indexes of `name_dict_values_t`.
pub(crate) const REGISTRY: usize = 9;
pub(crate) const ORDERING: usize = 10;
/// The number of name SIDs.
pub(crate) const NAME_COUNT: usize = 11;

pub(crate) const CHARSET: u32 = 15;
pub(crate) const ENCODING: u32 = 16;
pub(crate) const CHAR_STRINGS: u32 = 17;
pub(crate) const PRIVATE: u32 = 18;
pub(crate) const SUBRS: u32 = 19;
pub(crate) const ROS: u32 = 256 + 30;
pub(crate) const FD_ARRAY: u32 = 256 + 36;
pub(crate) const FD_SELECT: u32 = 256 + 37;
pub(crate) const FONT_NAME: u32 = 256 + 38;

/// `name_op_to_index` for the name operators of the Top DICT.
pub(crate) fn name_index(op: u32) -> Option<usize> {
    Some(match op {
        0 => 0,
        1 => 1,
        256 => 2,
        2 => 3,
        3 => 4,
        4 => 5,
        277 => 6,
        FONT_NAME => 7,
        278 => 8,
        _ => return None,
    })
}

/// Top DICT operators HarfBuzz reads and drops the operands of.
fn top_cleared(op: u32) -> bool {
    matches!(
        op,
        257..=262 | 264 | 276 | 279 | 287..=289 | 291 | 5 | 13 | 14
    )
}

/// `dict_opset_t::is_hint_op`.
pub(crate) fn is_hint_op(op: u32) -> bool {
    matches!(op, 6..=11 | 265..=269 | 270 | 273 | 274)
}

/// A DICT as recorded.
#[derive(Clone, Debug, Default)]
pub(crate) struct Dict {
    pub ops: Vec<DictOp>,
    /// The Top DICT's name SIDs (`nameSIDs`), [`UNDEF_SID`] when absent.
    pub names: [u32; NAME_COUNT],
    /// A Font DICT's `FontName`.
    pub font_name: u32,
}

impl Dict {
    /// `has_op`.
    pub fn has_op(&self, op: u32) -> bool {
        self.ops.iter().any(|o| o.op == op)
    }
}

struct Stack(Vec<f64>, bool);

impl Stack {
    fn pop(&mut self) -> f64 {
        self.0.pop().unwrap_or_else(|| {
            self.1 = true;
            0.0
        })
    }

    fn pop_int(&mut self) -> i32 {
        let v = self.pop();
        if v >= f64::from(i32::MIN) && v <= f64::from(i32::MAX) {
            to_int(v)
        } else {
            self.1 = true;
            0
        }
    }

    fn pop_uint(&mut self) -> u32 {
        let i = self.pop_int();
        u32::try_from(i).unwrap_or_else(|_| {
            self.1 = true;
            0
        })
    }
}

/// Reads `data` with the operator set for `kind`; `None` when HarfBuzz's
/// interpreter fails on it.
pub(crate) fn parse(data: &[u8], kind: Kind) -> Option<Dict> {
    let mut d = Dict {
        ops: Vec::new(),
        names: [UNDEF_SID; NAME_COUNT],
        font_name: UNDEF_SID,
    };
    let mut stack = Stack(Vec::new(), false);
    let mut op_start = 0usize;
    let mut last_offset = 0usize;
    let mut off = 0usize;
    while off < data.len() {
        let (tok, next) = dict_token(data, off)?;
        let start = off;
        off = next;
        let op = match tok {
            DictToken::Number(v) => {
                if kind == Kind::Top {
                    last_offset = start.saturating_add(1);
                }
                if stack.0.len() >= MAX_ARGS {
                    return None;
                }
                stack.0.push(v);
                continue;
            }
            DictToken::Operator(op) => op,
        };
        let mut last_arg = 0usize;
        let mut record = true;
        match kind {
            Kind::Top => match op {
                op if name_index(op).is_some() => {
                    let v = stack.pop_uint();
                    if let Some(slot) = name_index(op).and_then(|i| d.names.get_mut(i)) {
                        *slot = v;
                    }
                }
                op if top_cleared(op) => {}
                290 => {
                    stack.pop_uint();
                }
                ROS => {
                    stack.pop_uint();
                    let ordering = stack.pop_uint();
                    let registry = stack.pop_uint();
                    if let Some(s) = d.names.get_mut(ORDERING) {
                        *s = ordering;
                    }
                    if let Some(s) = d.names.get_mut(REGISTRY) {
                        *s = registry;
                    }
                    last_arg = last_offset.checked_sub(1)?.checked_sub(op_start)?;
                }
                ENCODING | CHARSET => {
                    record = stack.pop_int() != 0;
                }
                FD_SELECT => {
                    stack.pop_int();
                }
                PRIVATE => {
                    stack.pop_int();
                    stack.pop_uint();
                }
                _ => {
                    last_offset = off;
                    if matches!(op, CHAR_STRINGS | FD_ARRAY) {
                        stack.pop_int();
                    }
                }
            },
            Kind::Font => match op {
                FONT_NAME => d.font_name = stack.pop_uint(),
                PRIVATE => {
                    stack.pop_uint();
                    stack.pop_uint();
                }
                _ => {}
            },
            Kind::Private => {
                if op == SUBRS {
                    stack.pop_int();
                }
            }
        }
        stack.0.clear();
        if stack.1 {
            return None;
        }
        if record {
            d.ops.push(DictOp {
                op,
                start: op_start,
                end: off,
                last_arg,
            });
            op_start = off;
        }
    }
    Some(d)
}
