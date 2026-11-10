//! Lists of expected failures (ADR 0005). A test that fails without being
//! listed is a regression; a listed test that passes must be removed from
//! the list.

use std::collections::BTreeSet;

/// The identifiers in an expected-failures file: one per line. A `#` at
/// the start of a line or after whitespace starts a comment (the reason);
/// identifiers themselves contain `#` (`<font>#<face>:<item>`). Blank
/// lines are ignored.
pub fn parse(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(|l| {
            let l = l.trim();
            if l.starts_with('#') {
                return "";
            }
            let end = l
                .char_indices()
                .find(|&(i, c)| c == '#' && l[..i].ends_with(char::is_whitespace))
                .map_or(l.len(), |(i, _)| i);
            l[..end].trim()
        })
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Problem {
    /// Failed but is not listed.
    Regressed(String),
    /// Listed but passed.
    Fixed(String),
    /// Listed but no such test ran.
    Unknown(String),
}

/// Compares `(identifier, passed)` results with the expected failures.
pub fn judge<'a>(
    results: impl IntoIterator<Item = (&'a str, bool)>,
    expected: &BTreeSet<String>,
) -> Vec<Problem> {
    let mut seen = BTreeSet::new();
    let mut problems = Vec::new();
    for (id, passed) in results {
        seen.insert(id.to_string());
        match (passed, expected.contains(id)) {
            (false, false) => problems.push(Problem::Regressed(id.to_string())),
            (true, true) => problems.push(Problem::Fixed(id.to_string())),
            _ => {}
        }
    }
    problems.extend(
        expected
            .iter()
            .filter(|id| !seen.contains(*id))
            .map(|id| Problem::Unknown(id.clone())),
    );
    problems
}

/// A report of the problems, for a failing test.
pub fn report(problems: &[Problem], list: &str) -> String {
    let mut out = String::new();
    for p in problems {
        let line = match p {
            Problem::Regressed(id) => format!("failed, not in {list}: {id}\n"),
            Problem::Fixed(id) => format!("passed, remove from {list}: {id}\n"),
            Problem::Unknown(id) => format!("no such test, remove from {list}: {id}\n"),
        };
        out.push_str(&line);
    }
    out
}

/// The identifiers of an expected-failures file with their comments (the
/// text after the identifier's `#`, trimmed; empty when there is none).
pub fn parse_reasons(text: &str) -> std::collections::BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let end = l
                .char_indices()
                .find(|&(i, c)| c == '#' && l[..i].ends_with(char::is_whitespace))
                .map_or(l.len(), |(i, _)| i);
            let reason = l.get(end + 1..).unwrap_or("").trim();
            Some((l[..end].trim().to_string(), reason.to_string()))
        })
        .collect()
}
