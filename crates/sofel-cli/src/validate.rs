//! `sofel-validate`: validate sfnt, CEF and bare CFF fonts (ADR 0008, 0065).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use sofel_read::layout::damage::DamageEvent;
use sofel_read::sfnt::{FontFile, Sfnt};
use sofel_read::{
    DetailValue, Diagnostic, Kind, ReadError, Severity, Tag, cff, gdef, gpos, gsub, validate,
};
use sofel_type1::cid::CidFace;

use crate::json;

/// The usage text, printed for `--help` and after a usage error.
pub const HELP: &str = "\
usage: sofel-validate [--json] FILE...

Validates sfnt fonts, TTC/OTC collections, CEF, bare CFF 1, CID Type 1, BDF, FNT, and FON fonts.
Tables without a validator are skipped.

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

For GDEF, GSUB and GPOS, validation ends with a layout damage summary
for each font containing those tables. It counts distinct table offsets
with structural errors or warnings. If a reference cannot be followed or the walk limit is
reached, the count is a lower bound (at least N). JSON summaries have
kind layout_damage_summary, damage_count, complete, locations,
unchecked and work_limit_tables. Each location retains all its reasons.
Collections also end with a layout_damage_total summary across faces.

Nothing is written for a file without problems or layout tables.

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
        let bytes = match crate::font_input::unpack(bytes) {
            Ok(bytes) => bytes,
            Err(d) => {
                emit_diagnostic(&name, None, d, as_json, out, &mut errors);
                continue;
            }
        };
        if sofel_bitmapfont::winfnt::WinFont::is_win_font(&bytes) {
            let collection = bytes.starts_with(b"MZ");
            let count = sofel_bitmapfont::winfnt::WinFont::face_count(&bytes).unwrap_or(1);
            for face in 0..count {
                match sofel_bitmapfont::winfnt::WinFont::parse_with_offset(&bytes, face) {
                    Ok(_) => {}
                    Err(offset) => emit_diagnostic(
                        &name,
                        collection.then_some(face),
                        Diagnostic {
                            table: Tag::new(if collection { b"FON " } else { b"FNT " }),
                            offset: u32::try_from(offset).unwrap_or(u32::MAX),
                            severity: Severity::Error,
                            kind: Kind::InvalidFontContainer {
                                reason: "invalid Windows bitmap font",
                            },
                        },
                        as_json,
                        out,
                        &mut errors,
                    ),
                }
            }
            continue;
        }
        if sofel_bitmapfont::BdfFont::is_bdf(&bytes) {
            if let Err(offset) = sofel_bitmapfont::BdfFont::parse_with_offset(&bytes) {
                emit_diagnostic(
                    &name,
                    None,
                    Diagnostic {
                        table: Tag::new(b"BDF "),
                        offset: u32::try_from(offset).unwrap_or(u32::MAX),
                        severity: Severity::Error,
                        kind: Kind::InvalidFontContainer {
                            reason: "invalid BDF font",
                        },
                    },
                    as_json,
                    out,
                    &mut errors,
                );
            }
            continue;
        }
        if bytes.first() == Some(&1) {
            cff::validate_bare(&bytes, &mut |diagnostic| {
                emit_diagnostic(&name, None, diagnostic, as_json, out, &mut errors)
            });
            continue;
        }
        if bytes.starts_with(b"%!PS-Adobe-3.0 Resource-CIDFont") {
            if let Err(error) = CidFace::parse(&bytes, 67_108_864) {
                emit_diagnostic(
                    &name,
                    None,
                    Diagnostic {
                        table: Tag::new(b"CID "),
                        offset: u32::try_from(error.offset()).unwrap_or(u32::MAX),
                        severity: Severity::Error,
                        kind: Kind::InvalidFontContainer {
                            reason: "invalid CID-keyed Type 1 font",
                        },
                    },
                    as_json,
                    out,
                    &mut errors,
                );
            }
            continue;
        }
        match FontFile::new(&bytes) {
            Err(e) => emit_diagnostic(&name, None, file_diagnostic(e), as_json, out, &mut errors),
            Ok(FontFile::Sfnt(s)) if cff::is_cef(&s) => {
                let mut report = |d| emit_diagnostic(&name, None, d, as_json, out, &mut errors);
                validate::directory(&s, &mut report);
                if let Some(data) = s.table(cff::TAG) {
                    cff::validate_bare(data, &mut report);
                }
                if let Some(data) = s.table(sofel_font::read::cmap::TAG) {
                    sofel_font::read::cmap::validate(data, None, &mut report);
                }
            }
            Ok(FontFile::Sfnt(s)) => {
                validate_face(&s, &name, None, as_json, out, &mut errors);
            }
            Ok(FontFile::Collection(c)) => {
                validate::collection(&c, &mut |d| {
                    emit_diagnostic(&name, None, d, as_json, out, &mut errors)
                });
                validate_faces(
                    (0..c.num_fonts()).filter_map(|i| c.font(i).ok().map(|s| (i, s))),
                    &name,
                    as_json,
                    out,
                    &mut errors,
                );
            }
            Ok(FontFile::Dfont(d)) => {
                validate_faces(
                    (0..d.num_fonts()).filter_map(|i| d.font(i).ok().map(|s| (i, s))),
                    &name,
                    as_json,
                    out,
                    &mut errors,
                );
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

fn validate_faces<'a>(
    faces: impl Iterator<Item = (u32, Sfnt<'a>)>,
    name: &str,
    as_json: bool,
    out: &mut dyn Write,
    errors: &mut bool,
) {
    let summaries: Vec<_> = faces
        .filter_map(|(i, face)| {
            validate_face(&face, name, Some(i), as_json, out, errors).map(|summary| (i, summary))
        })
        .collect();
    if !summaries.is_empty() {
        let line = if as_json {
            total_json(name, &summaries)
        } else {
            total_text(name, &summaries)
        };
        let _ = writeln!(out, "{line}");
    }
}

fn emit_diagnostic(
    path: &str,
    font: Option<u32>,
    diagnostic: Diagnostic,
    as_json: bool,
    out: &mut dyn Write,
    errors: &mut bool,
) {
    *errors |= diagnostic.severity == Severity::Error;
    let line = if as_json {
        json_line(path, font, &diagnostic)
    } else {
        text_line(path, font, &diagnostic)
    };
    let _ = writeln!(out, "{line}");
}

fn target(tag: Tag) -> bool {
    tag == gdef::TAG || tag == gsub::TAG || tag == gpos::TAG
}

#[derive(Default)]
struct DamageSummary {
    locations: BTreeMap<(Tag, u32), Vec<Kind>>,
    unchecked: BTreeMap<(Tag, u32), Vec<Kind>>,
    limits: BTreeSet<Tag>,
}

impl DamageSummary {
    fn record(&mut self, d: Diagnostic) {
        if target(d.table)
            && !matches!(
                d.kind,
                Kind::LayoutTableDiscarded { .. } | Kind::WorkLimitExceeded
            )
        {
            let reasons = self.locations.entry((d.table, d.offset)).or_default();
            if !reasons.contains(&d.kind) {
                reasons.push(d.kind);
            }
        }
    }

    fn extra(&mut self, event: DamageEvent) {
        match event {
            DamageEvent::Unchecked {
                table,
                source,
                reason,
            } => {
                let reasons = self.unchecked.entry((table, source)).or_default();
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
            }
            DamageEvent::WorkLimit { table } => {
                self.limits.insert(table);
            }
            DamageEvent::Diagnostic(_) => {}
        }
    }

    fn complete(&self) -> bool {
        self.unchecked.is_empty() && self.limits.is_empty()
    }

    fn text(&self, path: &str, font: Option<u32>) -> String {
        let face = font.map(|n| format!("#{n}")).unwrap_or_default();
        let count = self.locations.len();
        let status = if self.complete() {
            format!("{count}")
        } else {
            format!("at least {count}")
        };
        let mut line = format!(
            "{path}{face}: layout damage: {status} location(s); {}",
            if self.complete() {
                "complete"
            } else {
                "incomplete"
            }
        );
        for ((tag, source), reasons) in &self.unchecked {
            for reason in reasons {
                line.push_str(&format!("; unchecked {tag}+{source:#x}: {reason}"));
            }
        }
        for tag in &self.limits {
            line.push_str(&format!("; unchecked {tag}: work limit exceeded"));
        }
        line
    }

    fn json(&self, path: &str, font: Option<u32>) -> String {
        let reasons = |items: &[Kind]| {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(|reason| json::object([
                        ("kind", json::string(reason.id())),
                        ("message", json::string(&reason.to_string())),
                    ]))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let locations = self
            .locations
            .iter()
            .map(|(&(tag, offset), kinds)| {
                json::object([
                    ("table", json::string(&tag.to_string())),
                    ("offset", offset.to_string()),
                    ("reasons", reasons(kinds)),
                ])
            })
            .collect::<Vec<_>>()
            .join(",");
        let unchecked = self
            .unchecked
            .iter()
            .map(|(&(tag, offset), kinds)| {
                json::object([
                    ("table", json::string(&tag.to_string())),
                    ("offset", offset.to_string()),
                    ("reasons", reasons(kinds)),
                ])
            })
            .collect::<Vec<_>>()
            .join(",");
        let limits = self
            .limits
            .iter()
            .map(|tag| json::string(&tag.to_string()))
            .collect::<Vec<_>>()
            .join(",");
        json::object([
            ("path", json::string(path)),
            (
                "font",
                font.map_or_else(|| "null".to_string(), |n| n.to_string()),
            ),
            ("kind", json::string("layout_damage_summary")),
            ("damage_count", self.locations.len().to_string()),
            ("complete", self.complete().to_string()),
            ("locations", format!("[{locations}]")),
            ("unchecked", format!("[{unchecked}]")),
            ("work_limit_tables", format!("[{limits}]")),
        ])
    }
}

fn validate_face(
    sfnt: &Sfnt<'_>,
    path: &str,
    font: Option<u32>,
    as_json: bool,
    out: &mut dyn Write,
    errors: &mut bool,
) -> Option<DamageSummary> {
    let has_layout = [gdef::TAG, gsub::TAG, gpos::TAG]
        .iter()
        .any(|&tag| sfnt.table(tag).is_some());
    let summary = RefCell::new(DamageSummary::default());
    validate::sfnt_with_layout_damage(
        sfnt,
        &mut |d| {
            summary.borrow_mut().record(d);
            emit_diagnostic(path, font, d, as_json, out, errors);
        },
        &mut |event| summary.borrow_mut().extra(event),
    );
    if has_layout {
        let summary = summary.into_inner();
        let line = if as_json {
            summary.json(path, font)
        } else {
            summary.text(path, font)
        };
        let _ = writeln!(out, "{line}");
        Some(summary)
    } else {
        None
    }
}

fn total_text(path: &str, faces: &[(u32, DamageSummary)]) -> String {
    let count: usize = faces.iter().map(|(_, s)| s.locations.len()).sum();
    let complete = faces.iter().all(|(_, s)| s.complete());
    let amount = if complete {
        count.to_string()
    } else {
        format!("at least {count}")
    };
    let mut line = format!(
        "{path}: layout damage total: {amount} location(s) across {} face(s); {}",
        faces.len(),
        if complete { "complete" } else { "incomplete" }
    );
    for (face, summary) in faces {
        for ((tag, source), reasons) in &summary.unchecked {
            for reason in reasons {
                line.push_str(&format!("; unchecked #{face} {tag}+{source:#x}: {reason}"));
            }
        }
        for tag in &summary.limits {
            line.push_str(&format!("; unchecked #{face} {tag}: work limit exceeded"));
        }
    }
    line
}

fn total_json(path: &str, faces: &[(u32, DamageSummary)]) -> String {
    let count: usize = faces.iter().map(|(_, s)| s.locations.len()).sum();
    let complete = faces.iter().all(|(_, s)| s.complete());
    let reasons = |items: &[Kind]| {
        format!(
            "[{}]",
            items
                .iter()
                .map(|reason| json::object([
                    ("kind", json::string(reason.id())),
                    ("message", json::string(&reason.to_string())),
                ]))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let mut locations = Vec::new();
    let mut unchecked = Vec::new();
    let mut limits = Vec::new();
    for (face, summary) in faces {
        for (&(tag, offset), kinds) in &summary.locations {
            locations.push(json::object([
                ("font", face.to_string()),
                ("table", json::string(&tag.to_string())),
                ("offset", offset.to_string()),
                ("reasons", reasons(kinds)),
            ]));
        }
        for (&(tag, offset), kinds) in &summary.unchecked {
            unchecked.push(json::object([
                ("font", face.to_string()),
                ("table", json::string(&tag.to_string())),
                ("offset", offset.to_string()),
                ("reasons", reasons(kinds)),
            ]));
        }
        for tag in &summary.limits {
            limits.push(json::object([
                ("font", face.to_string()),
                ("table", json::string(&tag.to_string())),
            ]));
        }
    }
    json::object([
        ("path", json::string(path)),
        ("font", "null".to_string()),
        ("kind", json::string("layout_damage_total")),
        ("damage_count", count.to_string()),
        ("complete", complete.to_string()),
        ("faces", faces.len().to_string()),
        ("locations", format!("[{}]", locations.join(","))),
        ("unchecked", format!("[{}]", unchecked.join(","))),
        ("work_limit_faces", format!("[{}]", limits.join(","))),
    ])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv4_structural_warnings_count_as_damage() {
        let mut summary = DamageSummary::default();
        for kind in [
            Kind::ReservedBitsSet { found: 0x20 },
            Kind::LayoutTableDiscarded { table: gsub::TAG },
        ] {
            summary.record(Diagnostic {
                table: gsub::TAG,
                offset: 0,
                severity: kind.severity(),
                kind,
            });
        }
        assert_eq!(
            summary.locations.get(&(gsub::TAG, 0)),
            Some(&vec![Kind::ReservedBitsSet { found: 0x20 }])
        );
    }

    #[test]
    fn inv4_damage_summary_deduplicates_locations_and_reasons() {
        let mut summary = DamageSummary::default();
        for kind in [
            Kind::UnknownFormat { found: 3 },
            Kind::UnknownFormat { found: 3 },
            Kind::OffsetOutOfBounds {
                offset: 50,
                length: 2,
                available: 10,
            },
        ] {
            summary.record(Diagnostic {
                table: gsub::TAG,
                offset: 4,
                severity: kind.severity(),
                kind,
            });
            summary.extra(DamageEvent::Unchecked {
                table: gsub::TAG,
                source: 4,
                reason: kind,
            });
        }
        summary.record(Diagnostic {
            table: gsub::TAG,
            offset: 0,
            severity: Severity::Error,
            kind: Kind::WorkLimitExceeded,
        });
        summary.extra(DamageEvent::WorkLimit { table: gsub::TAG });
        assert_eq!(summary.locations.len(), 1);
        assert_eq!(
            summary.locations.get(&(gsub::TAG, 4)).map(Vec::len),
            Some(2)
        );
        assert_eq!(summary.unchecked.len(), 1);
        assert_eq!(
            summary.unchecked.get(&(gsub::TAG, 4)).map(Vec::len),
            Some(2)
        );
        assert_eq!(summary.limits.len(), 1);
        assert!(!summary.complete());
    }
}
