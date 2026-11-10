//! Languages (`hb_language_t`).

use alloc::string::String;

/// A BCP 47 language tag normalized as `hb_language_from_string` does:
/// ASCII letters lowercased, `_` turned into `-`, cut at the first byte
/// that is not a letter, digit, `-` or `_`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Language(String);

/// `canon_map` of `src/hb-common.cc`.
fn canon(b: u8) -> Option<u8> {
    match b {
        b'-' | b'_' => Some(b'-'),
        b'0'..=b'9' | b'a'..=b'z' => Some(b),
        b'A'..=b'Z' => Some(b.to_ascii_lowercase()),
        _ => None,
    }
}

impl Language {
    /// The language of `s`, normalized; `None` when nothing is left
    /// (HarfBuzz's `HB_LANGUAGE_INVALID`).
    pub fn new(s: &[u8]) -> Option<Language> {
        let normalized: String = s.iter().map_while(|&b| canon(b)).map(char::from).collect();
        (!normalized.is_empty()).then_some(Language(normalized))
    }
    /// The normalized tag.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
