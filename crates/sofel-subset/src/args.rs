//! [`Input::to_args`], [`axis_range_from_string`] and
//! [`axis_range_to_string`]: HarfBuzz's `hb_subset_input_to_string_or_fail`,
//! `hb_subset_axis_range_from_string` and `hb_subset_axis_range_to_string`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use sofel_font::read::Tag;

use crate::input::{AxisRange, Flags, Input, SetKind};
use crate::set::RangeSet;

/// The flag options in HarfBuzz's order, as `hb_subset_input_to_string_or_fail`
/// with the experimental API writes them. `CFF_IDENTITY_CHARSET` has no
/// entry there, so it is not written.
const FLAG_OPTIONS: [(Flags, &str); 15] = [
    (Flags::NO_HINTING, "no-hinting"),
    (Flags::RETAIN_GIDS, "retain-gids"),
    (Flags::DESUBROUTINIZE, "desubroutinize"),
    (Flags::NAME_LEGACY, "name-legacy"),
    (Flags::SET_OVERLAPS_FLAG, "set-overlaps-flag"),
    (Flags::PASSTHROUGH_UNRECOGNIZED, "passthrough-tables"),
    (Flags::NOTDEF_OUTLINE, "notdef-outline"),
    (Flags::GLYPH_NAMES, "glyph-names"),
    (Flags::NO_PRUNE_UNICODE_RANGES, "no-prune-unicode-ranges"),
    (Flags::NO_LAYOUT_CLOSURE, "no-layout-closure"),
    (Flags::OPTIMIZE_IUP_DELTAS, "optimize"),
    (Flags::NO_BIDI_CLOSURE, "no-bidi-closure"),
    (Flags::IFTB_REQUIREMENTS, "iftb-requirements"),
    (Flags::RETAIN_NUM_GLYPHS, "retain-num-glyphs"),
    (Flags::DOWNGRADE_CFF2, "downgrade-cff2"),
];

/// How a set's members are written.
#[derive(Clone, Copy)]
enum Format {
    Decimal,
    Hex,
    Tags,
}

/// The sets in HarfBuzz's order, with their option names.
const SET_OPTIONS: [(SetKind, &str, Format); 8] = [
    (SetKind::Unicodes, "unicodes", Format::Hex),
    (SetKind::Glyphs, "gids", Format::Decimal),
    (SetKind::NameIds, "name-IDs", Format::Decimal),
    (SetKind::NameLanguages, "name-languages", Format::Decimal),
    (SetKind::LayoutFeatures, "layout-features", Format::Tags),
    (SetKind::LayoutScripts, "layout-scripts", Format::Tags),
    (SetKind::DropTables, "drop-tables", Format::Tags),
    // HarfBuzz writes the copied tables under this name, although
    // `hb-subset`'s `--passthrough-tables` is a flag; kept as HarfBuzz.
    (SetKind::NoSubsetTables, "passthrough-tables", Format::Tags),
];

impl Input {
    /// The input as `hb-subset` options, one string per option
    /// (`hb_subset_input_to_string_or_fail`, which joins them with
    /// spaces). The flags come first, then each set that differs from
    /// [`Input::new`]'s, then the glyph mapping and the axes.
    ///
    /// HarfBuzz fails only when out of memory or when an axis does not fit
    /// its 128-byte buffer, which cannot happen; so this cannot fail.
    pub fn to_args(&self) -> Vec<String> {
        let defaults = Input::new();
        let flags = FLAG_OPTIONS
            .iter()
            .filter(|(f, _)| self.flags().0 & f.0 != 0)
            .map(|(_, name)| format!("--{name}"));
        let sets = SET_OPTIONS
            .iter()
            .filter(|(kind, _, _)| self.set(*kind) != defaults.set(*kind))
            .flat_map(|&(kind, name, fmt)| set_args(self.set(kind), name, fmt));
        let map = glyph_map_arg(self);
        let axes = axes_arg(self);
        flags.chain(sets).chain(map).chain(axes).collect()
    }
}

/// `_format_set_arg`.
fn set_args(set: &RangeSet, name: &str, fmt: Format) -> Vec<String> {
    if set.is_inverted() {
        let mut excluded = set.clone();
        excluded.invert();
        let rest = if excluded.is_empty() {
            None
        } else {
            Some(format!("--{name}-={}", format_set(&excluded, fmt)))
        };
        core::iter::once(format!("--{name}=*"))
            .chain(rest)
            .collect()
    } else if set.is_empty() {
        alloc::vec![format!("--{name}-=*")]
    } else {
        alloc::vec![format!("--{name}={}", format_set(set, fmt))]
    }
}

fn format_set(set: &RangeSet, fmt: Format) -> String {
    match fmt {
        Format::Decimal => join(set.ranges().map(|(a, b)| {
            if a == b {
                format!("{a}")
            } else {
                format!("{a}-{b}")
            }
        })),
        Format::Hex => join(set.ranges().map(|(a, b)| {
            if a == b {
                format!("{a:X}")
            } else {
                format!("{a:X}-{b:X}")
            }
        })),
        Format::Tags => join(set.iter().map(format_tag)),
    }
}

fn join(it: impl Iterator<Item = String>) -> String {
    it.collect::<Vec<_>>().join(",")
}

/// `_format_tag`: trailing spaces dropped, and every byte other than
/// ASCII letters, digits, `-` and `_` written as `_`.
fn format_tag(tag: u32) -> String {
    let bytes = tag.to_be_bytes();
    let len = bytes
        .iter()
        .rposition(|&b| b != b' ')
        .map_or(0, |i| i.saturating_add(1));
    bytes
        .iter()
        .take(len)
        .map(|&b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                char::from(b)
            } else {
                '_'
            }
        })
        .collect()
}

/// `_format_glyph_map_arg`.
fn glyph_map_arg(input: &Input) -> Option<String> {
    let map = input.glyph_mapping();
    if map.is_empty() {
        return None;
    }
    let pairs = join(map.iter().map(|(k, v)| format!("{k}:{v}")));
    Some(format!("--gid-map={pairs}"))
}

/// `_format_axes_location_arg`.
fn axes_arg(input: &Input) -> Option<String> {
    let axes = input.axes();
    if axes.is_empty() {
        return None;
    }
    let mut sorted: Vec<(&Tag, &AxisRange)> = axes.iter().collect();
    sorted.sort_by_key(|(t, _)| t.to_u32());
    let specs = join(sorted.into_iter().map(|(tag, r)| {
        let name = format_tag(tag.to_u32());
        if r.min == r.default && r.default == r.max {
            format!("{name}={}", format_g(r.default))
        } else if r.min.is_nan() && r.default.is_nan() && r.max.is_nan() {
            format!("{name}=drop")
        } else {
            format!(
                "{name}={}:{}:{}",
                format_g(r.min),
                format_g(r.default),
                format_g(r.max)
            )
        }
    }));
    Some(format!("--variations={specs}"))
}

/// The range of axis `tag` as `min:default:max`, each written as C's
/// `%g` does, or `None` when the input has no range for it
/// (`hb_subset_axis_range_to_string`, which leaves the buffer untouched
/// then).
pub fn axis_range_to_string(input: &Input, tag: Tag) -> Option<String> {
    input.axis_range(tag).map(|r| {
        format!(
            "{}:{}:{}",
            format_g(r.min),
            format_g(r.default),
            format_g(r.max)
        )
    })
}

/// `v` as C's `printf ("%g", (double) v)` in the C locale writes it: six
/// significant digits, trailing zeros removed, and the exponent form when
/// the exponent is below -4 or at least 6.
pub(crate) fn format_g(v: f32) -> String {
    let v = f64::from(v);
    if v.is_nan() {
        return String::from(if v.is_sign_negative() { "-nan" } else { "nan" });
    }
    if v.is_infinite() {
        return String::from(if v < 0.0 { "-inf" } else { "inf" });
    }
    // The exponent after rounding to six significant digits, as `%e`.
    let e = format!("{v:.5e}");
    let (mantissa, exp) = e.split_once('e').unwrap_or((e.as_str(), "0"));
    let x: i32 = exp.parse().unwrap_or(0);
    if !(-4..6).contains(&x) {
        let sign = if x < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", strip_zeros(mantissa), x.unsigned_abs())
    } else {
        let precision = usize::try_from(5i32.saturating_sub(x)).unwrap_or(0);
        strip_zeros(&format!("{v:.precision$}")).into()
    }
}

/// Removes the trailing zeros of a fraction, and the point when nothing
/// follows it.
fn strip_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

/// Parses an axis range as `hb_subset_axis_range_from_string` does:
/// `drop`, a single value, `min:max` or `min:default:max`, where an empty
/// part means the font's value. Returns `(min, default, max)`, with `None`
/// for "the font's value" (HarfBuzz's NaN); `drop` gives three `None`s.
/// A single value gives it three times; `min:max` gives no default.
///
/// As in HarfBuzz, a number may be preceded by white space and followed
/// by anything, and the parts after the third are ignored.
pub fn axis_range_from_string(s: &str) -> Option<(Option<f32>, Option<f32>, Option<f32>)> {
    if !s.contains(':') {
        if s == "drop" {
            return Some((None, None, None));
        }
        let v = parse_double(s)?;
        return Some((Some(v), Some(v), Some(v)));
    }
    // Up to three parts; HarfBuzz stops after the third.
    let parts: Vec<&str> = s.splitn(4, ':').take(3).collect();
    let values = parts
        .iter()
        .map(|p| {
            if p.is_empty() {
                Some(None)
            } else {
                parse_double(p).map(Some)
            }
        })
        .collect::<Option<Vec<Option<f32>>>>()?;
    match values.as_slice() {
        [min, max] => Some((*min, None, *max)),
        [min, def, max] => Some((*min, *def, *max)),
        _ => None,
    }
}

/// `hb_parse_double` on `s` (not the whole buffer): white space, then the
/// longest prefix HarfBuzz's number parser accepts.
fn parse_double(s: &str) -> Option<f32> {
    let t = s.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let len = number_prefix(t.as_bytes());
    if len == 0 {
        return None;
    }
    t.get(..len)?.parse::<f32>().ok()
}

/// The length of the number at the start of `b`, as HarfBuzz's
/// `hb_number_parser_t`: a sign, digits, a point and digits (at least one
/// digit in all), and an exponent that is dropped when it has no digits.
fn number_prefix(b: &[u8]) -> usize {
    #[derive(Clone, Copy, PartialEq)]
    enum S {
        Start,
        Sign,
        Int,
        DotFromInt,
        DotFromStart,
        Frac,
        ExpStart,
        ExpSign,
        ExpDigits,
    }
    // (state, accepted length, length before the exponent)
    let step = |(st, pos, mark): (S, usize, usize), c: u8| -> Option<(S, usize, usize)> {
        let digit = c.is_ascii_digit();
        let next = pos.saturating_add(1);
        let exp = c == b'e' || c == b'E';
        match st {
            S::Start | S::Sign if st == S::Start && (c == b'+' || c == b'-') => {
                Some((S::Sign, next, mark))
            }
            S::Start | S::Sign if digit => Some((S::Int, next, mark)),
            S::Start | S::Sign if c == b'.' => Some((S::DotFromStart, next, mark)),
            S::Int if digit => Some((S::Int, next, mark)),
            S::Int if c == b'.' => Some((S::DotFromInt, next, mark)),
            S::DotFromInt | S::DotFromStart | S::Frac if digit => Some((S::Frac, next, mark)),
            S::Int | S::DotFromInt | S::Frac if exp => Some((S::ExpStart, next, pos)),
            S::ExpStart if c == b'+' || c == b'-' => Some((S::ExpSign, next, mark)),
            S::ExpStart | S::ExpSign | S::ExpDigits if digit => Some((S::ExpDigits, next, mark)),
            _ => None,
        }
    };
    let mut state = (S::Start, 0usize, 0usize);
    for &c in b {
        match step(state, c) {
            Some(s) => state = s,
            None => break,
        }
    }
    match state {
        (S::ExpStart | S::ExpSign, _, mark) => mark,
        (S::Sign | S::DotFromStart, _, _) => 0,
        (_, pos, _) => pos,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn g_format() {
        let cases: [(f32, &str); 12] = [
            (0.0, "0"),
            (-0.0, "-0"),
            (100.0, "100"),
            (0.5, "0.5"),
            (-1.25, "-1.25"),
            (123456.0, "123456"),
            (1234567.0, "1.23457e+06"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.0e20, "1e+20"),
            (f32::NAN, "nan"),
            (f32::NEG_INFINITY, "-inf"),
        ];
        for (v, s) in cases {
            assert_eq!(format_g(v), s, "{v}");
        }
        // f32 0.1 widened to double prints with six digits.
        assert_eq!(format_g(0.1), "0.1");
        assert_eq!(format_g(999999.5), "1e+06");
    }

    #[test]
    fn axes() {
        let r = |min, default, max| AxisRange { min, default, max };
        let input = Input::new()
            .with_axis_unchecked(Tag::new(b"wght"), r(200.0, 400.0, 900.0))
            .with_axis_unchecked(Tag::new(b"wdth"), r(100.0, 100.0, 100.0))
            .with_axis_unchecked(Tag::new(b"opsz"), r(f32::NAN, f32::NAN, f32::NAN));
        assert_eq!(
            input.to_args(),
            ["--variations=opsz=drop,wdth=100,wght=200:400:900"]
        );
        assert_eq!(
            axis_range_to_string(&input, Tag::new(b"wght")).as_deref(),
            Some("200:400:900")
        );
        assert_eq!(
            axis_range_to_string(&input, Tag::new(b"wdth")).as_deref(),
            Some("100:100:100")
        );
        assert_eq!(axis_range_to_string(&input, Tag::new(b"slnt")), None);
    }

    #[test]
    fn number_prefixes() {
        let cases: [(&str, usize); 12] = [
            ("1", 1),
            ("-1.5", 4),
            ("+.5", 3),
            (".", 0),
            ("-", 0),
            ("1.", 2),
            ("1e", 1),
            ("1e+", 1),
            ("1e5x", 3),
            ("1.e-2", 5),
            ("abc", 0),
            ("12abc", 2),
        ];
        for (s, n) in cases {
            assert_eq!(number_prefix(s.as_bytes()), n, "{s}");
        }
    }

    #[test]
    fn ranges() {
        assert_eq!(axis_range_from_string("drop"), Some((None, None, None)));
        assert_eq!(
            axis_range_from_string("300"),
            Some((Some(300.0), Some(300.0), Some(300.0)))
        );
        assert_eq!(
            axis_range_from_string(" 300x"),
            Some((Some(300.0), Some(300.0), Some(300.0)))
        );
        assert_eq!(
            axis_range_from_string("100:900"),
            Some((Some(100.0), None, Some(900.0)))
        );
        assert_eq!(
            axis_range_from_string(":300:500"),
            Some((None, Some(300.0), Some(500.0)))
        );
        assert_eq!(
            axis_range_from_string("100:"),
            Some((Some(100.0), None, None))
        );
        assert_eq!(axis_range_from_string("::"), Some((None, None, None)));
        assert_eq!(
            axis_range_from_string("1:2:3:4"),
            Some((Some(1.0), Some(2.0), Some(3.0)))
        );
        assert_eq!(axis_range_from_string("a:2"), None);
        assert_eq!(axis_range_from_string(""), None);
        assert_eq!(axis_range_from_string("drops"), None);
    }
}
