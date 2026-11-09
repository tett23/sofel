//! A minimal JSON writer: string escaping per RFC 8259 and objects built
//! from already-encoded values.

use std::fmt::Write;

/// `s` as a JSON string literal, quotes included. Quotes, backslashes and
/// control characters are escaped; everything else is written as is.
pub fn string(s: &str) -> String {
    let mut out = String::with_capacity(s.len().saturating_add(2));
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// An object from `(key, encoded value)` pairs, in the given order.
pub fn object<'a>(fields: impl IntoIterator<Item = (&'a str, String)>) -> String {
    let mut out = String::from("{");
    for (i, (k, v)) in fields.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&string(k));
        out.push(':');
        out.push_str(&v);
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        assert_eq!(string("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(
            string("\n\r\t\u{8}\u{c}\u{1}\u{1f}"),
            r#""\n\r\t\b\f\u0001\u001f""#
        );
        assert_eq!(string("フォント"), "\"フォント\"");
    }

    #[test]
    fn objects() {
        assert_eq!(
            object([("a", "1".into()), ("b", string("x"))]),
            r#"{"a":1,"b":"x"}"#
        );
        assert_eq!(object([]), "{}");
    }
}
