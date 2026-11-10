//! Rust source for packed tables (ADR 0019). The output passes the
//! workspace's clippy lints: every read goes through `get`, and shifts and
//! masks of zero are left out.

use std::collections::BTreeSet;
use std::fmt::Write;

use crate::pack::{Level, Packed};

/// A table to emit.
pub struct Table<'a> {
    /// Name of the lookup function; the arrays are named after it.
    pub name: &'a str,
    /// Doc comment of the lookup function.
    pub doc: &'a str,
    /// The packed table, or `None` if every value is the default.
    pub packed: Option<Packed>,
    /// The value beyond the table.
    pub default: u32,
}

fn element(bits: u32) -> &'static str {
    if bits == 16 { "u16" } else { "u8" }
}

fn getter(bits: u32) -> String {
    format!("get{bits}")
}

/// The bytes of a level as stored: 1- and 4-bit elements packed into
/// bytes, least significant first.
pub fn stored(level: &Level) -> Vec<u32> {
    match level.bits {
        1 => level
            .items
            .chunks(8)
            .map(|c| c.iter().enumerate().map(|(i, &v)| v << i).sum())
            .collect(),
        4 => level
            .items
            .chunks(2)
            .map(|c| c.iter().enumerate().map(|(i, &v)| v << (4 * i)).sum())
            .collect(),
        _ => level.items.clone(),
    }
}

fn array(out: &mut String, name: &str, level: &Level) {
    let items = stored(level);
    let per_line = if level.bits == 16 { 12 } else { 16 };
    let width = if level.bits == 16 { 5 } else { 3 };
    writeln!(
        out,
        "const {name}: [{}; {}] = [",
        element(level.bits),
        items.len()
    )
    .unwrap();
    for line in items.chunks(per_line) {
        let cells: Vec<String> = line.iter().map(|v| format!("{v:>width$},")).collect();
        writeln!(out, "    {}", cells.join(" ").trim_end()).unwrap();
    }
    writeln!(out, "];").unwrap();
}

fn shr(x: &str, k: u32) -> String {
    if k == 0 {
        x.to_string()
    } else {
        format!("({x} >> {k})")
    }
}

fn shl(x: &str, k: u32) -> String {
    if k == 0 {
        x.to_string()
    } else {
        format!("({x} << {k})")
    }
}

/// `s` without parentheses around the whole of it (rustc warns about them
/// around a function argument).
fn bare(s: String) -> String {
    let mut depth = 0i32;
    let closes_at_end = s.starts_with('(')
        && s.char_indices().all(|(i, ch)| {
            depth += match ch {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            depth > 0 || i == s.len() - 1
        });
    if closes_at_end {
        s[1..s.len() - 1].to_string()
    } else {
        s
    }
}

fn or_low(high: String, x: &str, k: u32) -> String {
    if k == 0 {
        high
    } else {
        format!("{high} | ({x} & {:#x})", (1u32 << k) - 1)
    }
}

fn doc(out: &mut String, text: &str, indent: &str) {
    text.lines().for_each(|l| {
        let sep = if l.is_empty() { "" } else { " " };
        writeln!(out, "{indent}///{sep}{l}").unwrap();
    });
}

/// The getters a table uses, by width.
pub fn widths(t: &Table) -> BTreeSet<u32> {
    t.packed
        .iter()
        .flat_map(|p| [Some(&p.s1), p.s2.as_ref(), Some(&p.s3)])
        .flatten()
        .map(|l| l.bits)
        .collect()
}

/// The arrays and the lookup function of a table.
pub fn table(t: &Table) -> String {
    let mut out = String::new();
    let upper = t.name.to_uppercase();
    let Some(p) = &t.packed else {
        doc(&mut out, t.doc, "");
        writeln!(out, "#[inline]").unwrap();
        writeln!(
            out,
            "pub(crate) fn {}(_cp: u32) -> u32 {{\n    {}\n}}",
            t.name, t.default
        )
        .unwrap();
        return out;
    };
    array(&mut out, &format!("{upper}_S1"), &p.s1);
    if let Some(s2) = &p.s2 {
        array(&mut out, &format!("{upper}_S2"), s2);
    }
    array(&mut out, &format!("{upper}_S3"), &p.s3);
    doc(&mut out, t.doc, "");
    writeln!(out, "#[inline]").unwrap();
    writeln!(out, "pub(crate) fn {}(cp: u32) -> u32 {{", t.name).unwrap();
    writeln!(out, "    {}_inside(cp).unwrap_or({})", t.name, t.default).unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "#[inline]").unwrap();
    writeln!(out, "fn {}_inside(cp: u32) -> Option<u32> {{", t.name).unwrap();
    if p.lo == 0 {
        writeln!(out, "    let c = cp;").unwrap();
    } else {
        writeln!(out, "    let c = cp.checked_sub({:#x})?;", p.lo).unwrap();
    }
    writeln!(
        out,
        "    if c > {:#x} {{\n        return None;\n    }}",
        p.hi - p.lo
    )
    .unwrap();
    let g1 = getter(p.s1.bits);
    writeln!(
        out,
        "    let i1 = {g1}(&{upper}_S1, {})?;",
        bare(shr("c", p.k2 + p.k3))
    )
    .unwrap();
    let last = match &p.s2 {
        Some(s2) => {
            let i = bare(or_low(shl("i1", p.k2), &shr("c", p.k3), p.k2));
            writeln!(out, "    let i2 = {}(&{upper}_S2, {i})?;", getter(s2.bits)).unwrap();
            "i2"
        }
        None => "i1",
    };
    let i = bare(or_low(shl(last, p.k3), "c", p.k3));
    writeln!(out, "    {}(&{upper}_S3, {i})", getter(p.s3.bits)).unwrap();
    writeln!(out, "}}").unwrap();
    out
}

/// The getters for the given widths.
pub fn getters(widths: &BTreeSet<u32>) -> String {
    let bodies = [
        (
            1,
            "fn get1(a: &[u8], i: u32) -> Option<u32> {\n    a.get((i >> 3) as usize)\n        .and_then(|b| b.checked_shr(i & 7))\n        .map(|b| u32::from(b & 1))\n}",
        ),
        (
            4,
            "fn get4(a: &[u8], i: u32) -> Option<u32> {\n    a.get((i >> 1) as usize)\n        .map(|&b| u32::from(if i & 1 == 0 { b & 15 } else { b >> 4 }))\n}",
        ),
        (
            8,
            "fn get8(a: &[u8], i: u32) -> Option<u32> {\n    a.get(i as usize).map(|&b| u32::from(b))\n}",
        ),
        (
            16,
            "fn get16(a: &[u16], i: u32) -> Option<u32> {\n    a.get(i as usize).map(|&b| u32::from(b))\n}",
        ),
    ];
    bodies
        .iter()
        .filter(|(w, _)| widths.contains(w))
        .map(|(_, b)| format!("#[inline]\n{b}\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One line of the header describing the size of a table.
pub fn size_line(t: &Table) -> String {
    match &t.packed {
        None => format!("{}: no table (always {})", t.name, t.default),
        Some(p) => {
            let levels: Vec<String> = [Some(&p.s1), p.s2.as_ref(), Some(&p.s3)]
                .into_iter()
                .flatten()
                .map(|l| format!("{}x{}bit", l.items.len(), l.bits))
                .collect();
            format!(
                "{}: {} bytes, U+{:04X}..U+{:04X}, k2={} k3={} ({})",
                t.name,
                p.bytes(),
                p.lo,
                p.hi,
                p.k2,
                p.k3,
                levels.join(" + ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::pack;

    #[test]
    fn packed_bits() {
        let one = Level {
            bits: 1,
            items: vec![1, 0, 1, 1, 0, 0, 0, 0, 1],
        };
        assert_eq!(stored(&one), vec![0b1101, 1]);
        let four = Level {
            bits: 4,
            items: vec![3, 10, 7],
        };
        assert_eq!(stored(&four), vec![0xA3, 7]);
    }

    #[test]
    fn lookup_code_has_no_zero_shifts() {
        let mut v = vec![0u32; 64];
        v[0] = 1;
        let t = Table {
            name: "x",
            doc: "X.",
            packed: pack(&v, 0),
            default: 0,
        };
        let code = table(&t);
        assert!(!code.contains(">> 0)") && !code.contains("<< 0)") && !code.contains("& 0x0"));
        assert!(code.contains("pub(crate) fn x(cp: u32) -> u32"));
    }

    #[test]
    fn outer_parentheses_are_removed() {
        assert_eq!(bare("(c >> 9)".to_string()), "c >> 9");
        assert_eq!(
            bare("(i1 << 5) | (c & 0xf)".to_string()),
            "(i1 << 5) | (c & 0xf)"
        );
        assert_eq!(bare("c".to_string()), "c");
    }

    #[test]
    fn an_empty_table_returns_the_default() {
        let t = Table {
            name: "y",
            doc: "Y.",
            packed: None,
            default: 5,
        };
        assert!(table(&t).contains("fn y(_cp: u32) -> u32 {\n    5\n}"));
    }
}
