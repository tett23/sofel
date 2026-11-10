//! `sofel-validate`: validate sfnt fonts and collections (ADR 0008).

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use sofel_read::sfnt::FontFile;
use sofel_read::{DetailValue, Diagnostic, Kind, ReadError, Severity, validate};

use crate::json;

/// The usage text, printed for `--help` and after a usage error.
pub const HELP: &str = "\
usage: sofel-validate [--json] FILE...

Validates sfnt fonts and TTC/OTC collections with the validators of
sofel-read. Tables without a validator are skipped.

Each problem is one line on standard output:

  <path>[#<font>]: <table>+0x<offset>: <error|warning>: <message>

<font> is the index in a collection and is omitted for single fonts.
<table> is the table tag; `sfnt` is the table directory and `ttcf` the
collection header. <offset> is from the start of the table (or of the
directory or file).

--json writes one JSON object per line (JSON Lines) instead, with keys:

  path      the file path (string)
  font      the index in a collection (number), or null
  table     the table tag (string of 4 characters)
  offset    the offset from the start of the table (number)
  severity  \"error\" or \"warning\"
  kind      the identifier of the problem, e.g. \"segments_not_sorted\"
  message   a description for people (string)
  details   an object with the values of the problem, e.g. expected and
            found values

Nothing is written for a file without problems.

Exit status:
  0  no errors (warnings may have been written)
  1  at least one error
  2  wrong usage, or a file could not be read; the other files are still
     validated
";

/// Runs the command with `args` (without the program name). Returns the
/// exit status.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut as_json = false;
    let mut files = Vec::new();
    for a in args {
        match a.to_str() {
            Some("--help" | "-h") => {
                let _ = out.write_all(HELP.as_bytes());
                return 0;
            }
            Some("--json") => as_json = true,
            Some(s) if s.starts_with('-') && s.len() > 1 => {
                let _ = writeln!(err, "sofel-validate: unknown option {s}\n\n{HELP}");
                return 2;
            }
            _ => files.push(a),
        }
    }
    if files.is_empty() {
        let _ = writeln!(err, "sofel-validate: no files given\n\n{HELP}");
        return 2;
    }

    let mut errors = false;
    let mut unreadable = false;
    for path in files {
        let path = Path::new(path);
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                let _ = writeln!(err, "sofel-validate: {}: {e}", path.display());
                unreadable = true;
                continue;
            }
        };
        let name = path.display().to_string();
        let mut emit = |font: Option<u32>, d: Diagnostic| {
            errors |= d.severity == Severity::Error;
            let line = if as_json {
                json_line(&name, font, &d)
            } else {
                text_line(&name, font, &d)
            };
            let _ = writeln!(out, "{line}");
        };
        match FontFile::new(&bytes) {
            Err(e) => emit(None, file_diagnostic(e)),
            Ok(FontFile::Sfnt(s)) => validate::sfnt(&s, &mut |d| emit(None, d)),
            Ok(FontFile::Collection(c)) => {
                validate::collection(&c, &mut |d| emit(None, d));
                for i in 0..c.num_fonts() {
                    if let Ok(s) = c.font(i) {
                        validate::sfnt(&s, &mut |d| emit(Some(i), d));
                    }
                }
            }
        }
    }
    if unreadable {
        2
    } else if errors {
        1
    } else {
        0
    }
}

/// A diagnostic for a file that is neither an sfnt nor a collection.
fn file_diagnostic(e: ReadError) -> Diagnostic {
    let kind = match e {
        ReadError::UnknownVersion(found) => Kind::UnknownVersion { found },
        ReadError::Truncated { needed, available } => Kind::Truncated {
            needed: u32::try_from(needed).unwrap_or(u32::MAX),
            available: u32::try_from(available).unwrap_or(u32::MAX),
        },
        ReadError::UnknownFormat(f) => Kind::UnknownFormat {
            found: u32::from(f),
        },
        ReadError::OutOfBounds | ReadError::InvalidValue => Kind::Truncated {
            needed: 0,
            available: 0,
        },
    };
    Diagnostic {
        table: validate::DIRECTORY,
        offset: 0,
        severity: kind.severity(),
        kind,
    }
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

fn text_line(path: &str, font: Option<u32>, d: &Diagnostic) -> String {
    let font = font.map(|i| format!("#{i}")).unwrap_or_default();
    format!(
        "{path}{font}: {}+{:#x}: {}: {}",
        d.table,
        d.offset,
        severity(d.severity),
        d.kind
    )
}

fn json_line(path: &str, font: Option<u32>, d: &Diagnostic) -> String {
    let mut details = Vec::new();
    d.kind.details(&mut |k, v| {
        let v = match v {
            DetailValue::Uint(n) => n.to_string(),
            DetailValue::Int(n) => n.to_string(),
            DetailValue::Tag(t) => json::string(&t.to_string()),
        };
        details.push((k, v));
    });
    json::object([
        ("path", json::string(path)),
        (
            "font",
            font.map_or_else(|| "null".to_string(), |i| i.to_string()),
        ),
        ("table", json::string(&d.table.to_string())),
        ("offset", d.offset.to_string()),
        ("severity", json::string(severity(d.severity))),
        ("kind", json::string(d.kind.id())),
        ("message", json::string(&d.kind.to_string())),
        ("details", json::object(details)),
    ])
}
