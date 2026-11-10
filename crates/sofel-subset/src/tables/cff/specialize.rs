//! What converting a pinned CFF2 font to CFF adds to its charstrings
//! (`src/hb-cff-specializer.hh`, `src/hb-cff-width-optimizer.hh`, after
//! fontTools' `cffLib.specializer` and `cffLib.width`): the commands
//! respecialized within CFF's stack limit, and the `defaultWidthX` and
//! `nominalWidthX` that make the widths cheapest.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use super::cs::{CNTRMASK, HINTMASK, HMOVETO, RMOVETO, VMOVETO, encode_num_cs, encode_op};
use super::cs2::{Command, HLINETO, INVALID, RLINETO, RRCURVETO, VLINETO};

/// `generalize_commands`: moves and lines as `rmoveto` and single
/// `rlineto` segments, curves as single `rrcurveto` segments.
fn generalize(commands: &[Command]) -> Vec<Command> {
    let mut out = Vec::with_capacity(commands.len().saturating_mul(2));
    for c in commands {
        match c.op {
            HMOVETO | VMOVETO => {
                let args = match c.args.first() {
                    Some(&a) if c.op == HMOVETO => alloc::vec![a, 0.0],
                    Some(&a) => alloc::vec![0.0, a],
                    None => Vec::new(),
                };
                out.push(Command::new(RMOVETO, args));
            }
            HLINETO | VLINETO => {
                let h = c.op == HLINETO;
                for (j, &a) in c.args.iter().enumerate() {
                    let horizontal = if h { j % 2 == 0 } else { j % 2 == 1 };
                    let args = if horizontal {
                        alloc::vec![a, 0.0]
                    } else {
                        alloc::vec![0.0, a]
                    };
                    out.push(Command::new(RLINETO, args));
                }
            }
            RLINETO => out.extend(
                c.args
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| Command::new(RLINETO, p.to_vec())),
            ),
            RRCURVETO => out.extend(
                c.args
                    .as_chunks::<6>()
                    .0
                    .iter()
                    .map(|p| Command::new(RRCURVETO, p.to_vec())),
            ),
            _ => out.push(c.clone()),
        }
    }
    out
}

/// Merges `commands[i]` into `commands[i - 1]` when `merge` says so and
/// the operands stay below `maxstack`, walking down from the end as
/// HarfBuzz does: after a merge the command that follows is compared
/// with the merged one again.
fn combine(
    commands: &mut Vec<Command>,
    maxstack: usize,
    merge: impl Fn(&Command, &Command) -> bool,
) {
    let mut i = commands.len().saturating_sub(1);
    while i > 0 {
        let prev_at = i.saturating_sub(1);
        let merged = match (commands.get(prev_at), commands.get(i)) {
            (Some(prev), Some(cmd)) => {
                merge(prev, cmd) && prev.args.len().saturating_add(cmd.args.len()) < maxstack
            }
            _ => false,
        };
        if merged {
            let cmd = commands.remove(i);
            if let Some(prev) = commands.get_mut(prev_at) {
                prev.args.extend_from_slice(&cmd.args);
            }
            // `i++` before the loop's `i--`.
            continue;
        }
        i = prev_at;
    }
}

/// `specialize_commands`.
pub(crate) fn specialize(commands: &mut Vec<Command>, maxstack: usize) {
    if commands.is_empty() {
        return;
    }
    *commands = generalize(commands);
    for c in commands.iter_mut() {
        if (c.op == RMOVETO || c.op == RLINETO) && c.args.len() == 2 {
            let (dx, dy) = (
                c.args.first().copied().unwrap_or(0.0),
                c.args.get(1).copied().unwrap_or(0.0),
            );
            let (dx_zero, dy_zero) = (dx == 0.0, dy == 0.0);
            if dx_zero && !dy_zero {
                c.op = if c.op == RMOVETO { VMOVETO } else { VLINETO };
                c.args = alloc::vec![dy];
            } else if !dx_zero && dy_zero {
                c.op = if c.op == RMOVETO { HMOVETO } else { HLINETO };
                c.args.truncate(1);
            }
        }
    }
    combine(commands, maxstack, |prev, cmd| {
        ((prev.op == HLINETO && cmd.op == VLINETO) || (prev.op == VLINETO && cmd.op == HLINETO))
            && prev.args.len() & 1 != 0
    });
    combine(commands, maxstack, |prev, cmd| {
        prev.op == cmd.op
            && (cmd.op == RLINETO
                || cmd.op == RRCURVETO
                || ((cmd.op == HLINETO || cmd.op == VLINETO) && prev.args.len() & 1 == 0))
    });
}

/// `encode_commands`.
pub(crate) fn encode(commands: &[Command], out: &mut Vec<u8>) {
    for c in commands {
        for &a in &c.args {
            encode_num_cs(out, a);
        }
        if c.op != INVALID {
            encode_op(out, c.op);
        }
        if c.op == HINTMASK || c.op == CNTRMASK {
            out.extend_from_slice(&c.mask);
        }
    }
}

/// `width_delta_cost`.
fn width_delta_cost(delta: i32) -> u32 {
    let d = delta.unsigned_abs();
    if d <= 107 {
        1
    } else if d <= 1131 {
        2
    } else {
        5
    }
}

/// `optimize_widths`: `(defaultWidthX, nominalWidthX)` for `widths`, in
/// HarfBuzz's unsigned arithmetic.
pub(crate) fn optimize_widths(widths: &[u32]) -> (u32, u32) {
    let Some(&first) = widths.first() else {
        return (0, 0);
    };
    let mut freq: BTreeMap<u32, u32> = BTreeMap::new();
    let (mut min_w, mut max_w) = (first, first);
    for &w in widths {
        let f = freq.entry(w).or_insert(0);
        *f = f.wrapping_add(1);
        min_w = min_w.min(w);
        max_w = max_w.max(w);
    }
    let get = |w: u32| freq.get(&w).copied().unwrap_or(0);
    let span = (max_w.wrapping_sub(min_w) as usize).saturating_add(1);
    let mut cum_frq_u = Vec::with_capacity(span);
    let mut cum_max_u = Vec::with_capacity(span);
    let (mut s, mut m) = (0u32, 0u32);
    for x in min_w..=max_w {
        s = s.wrapping_add(get(x));
        m = m.max(get(x));
        cum_frq_u.push(s);
        cum_max_u.push(m);
    }
    let mut cum_frq_d = alloc::vec![0u32; span];
    let mut cum_max_d = alloc::vec![0u32; span];
    let (mut s, mut m) = (0u32, 0u32);
    for x in (min_w..=max_w).rev() {
        s = s.wrapping_add(get(x));
        m = m.max(get(x));
        let at = x.wrapping_sub(min_w) as usize;
        if let Some(slot) = cum_frq_d.get_mut(at) {
            *slot = s;
        }
        if let Some(slot) = cum_max_d.get_mut(at) {
            *slot = m;
        }
    }
    // `safe_get` of an unsigned expression passed as an `int`.
    let safe = |v: &[u32], x: u32| {
        let x = x.cast_signed();
        if x < min_w.cast_signed() || x > max_w.cast_signed() {
            return 0;
        }
        v.get(x.wrapping_sub(min_w.cast_signed()).cast_unsigned() as usize)
            .copied()
            .unwrap_or(0)
    };
    let nomn_cost = |x: u32| {
        safe(&cum_frq_u, x)
            .wrapping_add(safe(&cum_frq_u, x.wrapping_sub(108)))
            .wrapping_add(safe(&cum_frq_u, x.wrapping_sub(1132)).wrapping_mul(3))
            .wrapping_add(safe(&cum_frq_d, x))
            .wrapping_add(safe(&cum_frq_d, x.wrapping_add(108)))
            .wrapping_add(safe(&cum_frq_d, x.wrapping_add(1132)).wrapping_mul(3))
            .wrapping_sub(get(x))
    };
    let dflt_cost = |x: u32| {
        let u = safe(&cum_max_u, x)
            .max(safe(&cum_max_u, x.wrapping_sub(108)).wrapping_mul(2))
            .max(safe(&cum_max_u, x.wrapping_sub(1132)).wrapping_mul(5));
        let d = safe(&cum_max_d, x)
            .max(safe(&cum_max_d, x.wrapping_add(108)).wrapping_mul(2))
            .max(safe(&cum_max_d, x.wrapping_add(1132)).wrapping_mul(5));
        u.max(d)
    };
    let mut best_nominal = min_w;
    let mut best_cost = nomn_cost(min_w).wrapping_sub(dflt_cost(min_w));
    for x in min_w.saturating_add(1)..=max_w {
        let cost = nomn_cost(x).wrapping_sub(dflt_cost(x));
        if cost < best_cost {
            best_cost = cost;
            best_nominal = x;
        }
    }
    let nominal = best_nominal.cast_signed();
    let mut best_default = best_nominal;
    let mut best_default_cost = u32::MAX;
    for candidate in [
        nominal,
        nominal.wrapping_sub(108),
        nominal.wrapping_sub(1132),
        nominal.wrapping_add(108),
        nominal.wrapping_add(1132),
    ] {
        if candidate < min_w.cast_signed() || candidate > max_w.cast_signed() {
            continue;
        }
        let cost = freq
            .iter()
            .filter(|&(&w, _)| w != candidate.cast_unsigned())
            .fold(0u32, |c, (&w, &f)| {
                c.wrapping_add(
                    f.wrapping_mul(width_delta_cost(w.cast_signed().wrapping_sub(nominal))),
                )
            });
        if cost < best_default_cost {
            best_default_cost = cost;
            best_default = candidate.cast_unsigned();
        }
    }
    (best_default, best_nominal)
}
