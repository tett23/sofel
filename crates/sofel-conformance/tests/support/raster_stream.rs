//! Bounded reference parsing and result retention for ADR 0037.
use sofel_conformance::expect::Problem;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

/// Visits glyphs in the fixed tool's face/glyph order. Retains one glyph.
pub fn stream(
    mut reader: impl BufRead,
    mut visit: impl FnMut(u32, u32, &[String]),
) -> Result<usize, String> {
    let mut line = String::new();
    let mut face = None;
    let mut declared = None;
    let mut face_error = false;
    let mut next_glyph = 0;
    let mut glyph = None;
    let mut lines = Vec::new();
    let mut faces = 0usize;
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|e| format!("reference read: {e}"))?
            == 0
        {
            break;
        }
        let text = line.trim_end_matches(['\r', '\n']);
        let mut words = text.split(' ');
        let key = words.next().unwrap_or("");
        let number = |value: Option<&str>| {
            value
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(|| format!("bad reference line: {text}"))
        };
        match key {
            "face" => {
                let index = number(words.next())?;
                if face.is_some() || index as usize != faces {
                    return Err("missing end or unordered face".into());
                }
                face = Some(index);
                declared = None;
                face_error = false;
                next_glyph = 0;
                faces += 1;
            }
            "num_glyphs" => {
                if face.is_none() || declared.is_some() || face_error {
                    return Err("misplaced glyph count".into());
                }
                declared = Some(number(words.next())?);
            }
            "glyph" => {
                let index = number(words.next())?;
                let f = face.ok_or("glyph outside face")?;
                let count = declared.ok_or("glyph before count")?;
                if face_error || index != next_glyph || index >= count {
                    return Err("missing, duplicate or excess glyph".into());
                }
                if let Some(g) = glyph.take() {
                    visit(f, g, &lines);
                    lines.clear();
                }
                glyph = Some(index);
                next_glyph += 1;
            }
            "end" => {
                let f = face.take().ok_or("end outside face")?;
                if !face_error && declared != Some(next_glyph) {
                    return Err("incomplete face".into());
                }
                if let Some(g) = glyph.take() {
                    visit(f, g, &lines);
                    lines.clear();
                }
            }
            "error" if glyph.is_none() => {
                if face.is_none() || face_error {
                    return Err("misplaced face error".into());
                }
                face_error = true;
            }
            _ => {
                if face.is_none() || glyph.is_none() {
                    return Err(format!("data outside glyph: {text}"));
                }
                lines.push(if text.starts_with("error") {
                    "error".into()
                } else {
                    text.into()
                });
            }
        }
    }
    if face.is_some() {
        return Err("reference ended before end".into());
    }
    Ok(faces)
}

/// `false` preserves the existing exclusion of fonts the reference cannot open.
/// Drain on parse failure before wait so a writer cannot block on a full pipe.
pub fn reference(
    command: &mut Command,
    visit: impl FnMut(u32, u32, &[String]),
) -> Result<bool, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(child.stdout.take().ok_or("missing stdout")?);
    let result = stream(&mut reader, visit);
    let drained = std::io::copy(&mut reader, &mut std::io::sink());
    let status = child.wait().map_err(|e| e.to_string())?;
    drained.map_err(|e| format!("reference drain: {e}"))?;
    if !status.success() {
        return Ok(false);
    }
    if result? == 0 {
        return Err("successful reference returned no faces".into());
    }
    Ok(true)
}

#[derive(Default)]
pub struct Results {
    pub count: usize,
    pub failed: usize,
    pub problems: Vec<Problem>,
    pub details: BTreeMap<String, String>,
    pub seen_expected: BTreeSet<String>,
}
impl Results {
    pub fn record(
        &mut self,
        id: String,
        passed: bool,
        detail: String,
        expected: &BTreeSet<String>,
    ) {
        self.count += 1;
        self.failed += usize::from(!passed);
        let known = expected.contains(&id);
        if known {
            self.seen_expected.insert(id.clone());
        }
        match (passed, known) {
            (false, false) => {
                self.details.entry(id.clone()).or_insert(detail);
                self.problems.push(Problem::Regressed(id));
            }
            (true, true) => self.problems.push(Problem::Fixed(id)),
            _ => {}
        }
    }
    pub fn merge(&mut self, other: Self) {
        self.count += other.count;
        self.failed += other.failed;
        self.problems.extend(other.problems);
        for (id, detail) in other.details {
            self.details.entry(id).or_insert(detail);
        }
        self.seen_expected.extend(other.seen_expected);
    }
    pub fn finish(&mut self, expected: &BTreeSet<String>) -> Vec<Problem> {
        let mut problems = std::mem::take(&mut self.problems);
        problems.extend(
            expected
                .difference(&self.seen_expected)
                .cloned()
                .map(Problem::Unknown),
        );
        problems
    }
}
