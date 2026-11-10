//! The Arabic private use area maps of HarfBuzz's `gen-arabic-pua.py`
//! (ADR 0019): from a Unicode character to its code in a legacy Arabic
//! symbol font. HarfBuzz is under the MIT license (THIRD_PARTY_NOTICES.md).

use crate::ucd::CODE_POINTS;

fn hex(s: &str) -> Result<u32, String> {
    let t = s.trim();
    let digits = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    u32::from_str_radix(digits, 16).map_err(|e| format!("bad code {s:?}: {e}"))
}

/// Read `ArabicPUASimplified.txt` or `ArabicPUATraditional.txt`
/// (`<PUA>\t<Unicode>\t# name`). Returns the PUA code of every code point,
/// 0 if none; a later line for the same Unicode character wins, as in the
/// Python dictionary of `gen-arabic-pua.py`.
pub fn parse(text: &str) -> Result<Vec<u32>, String> {
    let mut map = vec![0u32; CODE_POINTS];
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let f: Vec<&str> = line.split('\t').collect();
        let (pua, unicode) = match f[..] {
            [p, u, ..] => (hex(p)?, hex(u)?),
            _ => return Err(format!("bad PUA line: {line}")),
        };
        *map.get_mut(unicode as usize)
            .ok_or(format!("bad code point in {line}"))? = pua;
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_lines_win() {
        let m = parse("#\n#  Name: x\n0xF10C\t0x200C\t# ZWNJ\n0xF120\t0x0020\t# SPACE\n0xF1A0\t0x0020\t# SPACE\n").unwrap();
        assert_eq!(m[0x200C], 0xF10C);
        assert_eq!(m[0x20], 0xF1A0);
        assert_eq!(m[0x21], 0);
    }
}
