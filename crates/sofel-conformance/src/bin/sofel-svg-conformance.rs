//! Compare the SVG observations required by ADR 0074.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::process::ExitCode;

const INVENTORY: &str = include_str!("../../../../conformance/svg-cases.tsv");
const DOCUMENT: &[u8] = include_bytes!("../../../../conformance/svg/two-glyphs.svg");
const REQUIRED: &[(&str, &str, &str)] = &[
    ("in-document", "plain", "document-bytes,target-element,rgba"),
    (
        "in-document",
        "gzip",
        "decompressed-bytes,target-element,rgba",
    ),
    ("in-document", "shared-first", "target-element,rgba"),
    ("in-document", "shared-second", "target-element,rgba"),
    ("in-document", "out-of-range", "absent-glyph"),
    ("in-geometry", "path-line", "outline,bounds"),
    ("in-geometry", "path-curve", "outline,bounds"),
    ("in-geometry", "rect", "outline,bounds"),
    ("in-geometry", "circle", "outline,bounds"),
    ("in-geometry", "nested-transform", "outline,bounds"),
    ("in-style", "inherited-fill", "inside-rgba,outside-rgba"),
    ("in-style", "override-fill", "inside-rgba,outside-rgba"),
    ("in-style", "context-fill-red", "inside-rgba,outside-rgba"),
    ("in-style", "context-fill-blue", "inside-rgba,outside-rgba"),
    ("in-style", "overlap", "intersection-rgba"),
    ("in-style", "reversed-overlap", "intersection-rgba"),
    (
        "in-reference",
        "use-transform",
        "expanded-geometry,transform-order",
    ),
    (
        "in-reference",
        "clip-transform",
        "inside-rgba,outside-rgba,transform-order",
    ),
    ("in-gradient", "linear", "sample-position,rgba"),
    ("in-gradient", "radial", "sample-position,rgba"),
    ("in-gradient", "aspect-ratio", "sample-position,rgba"),
    ("in-gradient", "stop-opacity", "sample-position,rgba"),
    ("in-gradient", "transformed", "sample-position,rgba"),
    ("in-gradient", "color-svg-19", "cli-rgba,gallery-rgba"),
    ("in-gradient", "color-svg-26", "cli-rgba,gallery-rgba"),
    (
        "in-restrictions",
        "external",
        "resource-requests,control-rgba",
    ),
    ("in-restrictions", "script", "changed-geometry,control-rgba"),
    ("out-page", "empty", "page-size,scene-tree"),
    ("out-page", "single", "page-size,scene-tree"),
    ("out-page", "translucent-overlap", "page-size,scene-tree"),
    ("out-page", "clipped-image", "page-size,scene-tree"),
];

// A one-entry SVG Document List covers glyphs 1 and 2. The fixture and the
// expected range are independent of the reader that resolves the glyph ID.
fn document_observation(case: &str, observation: &str) -> Option<Result<(), String>> {
    let (glyph, expected) = match (case, observation) {
        ("out-of-range", "absent-glyph") => (3, None),
        ("plain", "document-bytes") => (1, Some(DOCUMENT)),
        _ => return None,
    };
    let mut table = vec![0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 1, 0, 1, 0, 2, 0, 0, 0, 14];
    table.extend_from_slice(&u32::try_from(DOCUMENT.len()).unwrap().to_be_bytes());
    table.extend_from_slice(DOCUMENT);
    let svg = sofel_read::svg::Svg::new(&table).expect("valid fixed SVG table");
    let actual = svg.find(glyph).and_then(|entry| svg.document(entry));
    Some(if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "glyph {glyph}: expected {expected:?}, got {actual:?}"
        ))
    })
}

fn verify(text: &str) -> bool {
    let mut rows = BTreeMap::new();
    let mut ok = true;
    for (line, record) in text.lines().enumerate() {
        if record.is_empty() || record.starts_with('#') {
            continue;
        }
        let fields = record.split('\t').collect::<Vec<_>>();
        let [feature, case, names, source] = fields.as_slice() else {
            println!("invalid inventory row {}", line + 1);
            ok = false;
            continue;
        };
        let mut observations = BTreeSet::new();
        for name in names.split(',') {
            if name.is_empty() || !observations.insert(name) {
                println!("invalid observation: {feature}/{case}/{name}");
                ok = false;
            }
        }
        if rows
            .insert((*feature, *case), (observations, *source))
            .is_some()
        {
            println!("duplicate case: {feature}/{case}");
            ok = false;
        }
    }
    for &(feature, case, required) in REQUIRED {
        let Some((mut names, source)) = rows.remove(&(feature, case)) else {
            println!("missing case: {feature}/{case}");
            ok = false;
            continue;
        };
        let mut failures = Vec::new();
        if source.trim().is_empty() {
            failures.push("missing expectation source".into());
        }
        for name in required.split(',') {
            if !names.remove(name) {
                failures.push(format!("missing observation: {feature}/{case}/{name}"));
                continue;
            }
            let result = match feature {
                "in-document" => document_observation(case, name),
                _ => None,
            };
            match result {
                None => failures.push(format!("{name}: NOT RUN (verifier pending)")),
                Some(Err(reason)) => failures.push(format!("{name}: {reason}")),
                Some(Ok(())) => {}
            }
        }
        failures.extend(
            names
                .into_iter()
                .map(|name| format!("unknown observation: {name}")),
        );
        let direction = if feature == "out-page" {
            "output"
        } else {
            "input"
        };
        let result = if failures.is_empty() {
            "PASS".into()
        } else {
            ok = false;
            failures.join("; ")
        };
        println!("{feature}\t{case}\t{direction}\t{source}\t{result}");
    }
    for (feature, case) in rows.into_keys() {
        println!("unknown case: {feature}/{case}");
        ok = false;
    }
    ok
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut input = String::new();
    let ledger = match (args.next(), args.next()) {
        (None, None) => INVENTORY,
        (Some(arg), None) if arg == "-" => {
            if let Err(error) = io::stdin().read_to_string(&mut input) {
                eprintln!("cannot read SVG inventory: {error}");
                return ExitCode::FAILURE;
            }
            &input
        }
        _ => {
            eprintln!("usage: sofel-svg-conformance [-]");
            return ExitCode::FAILURE;
        }
    };
    if verify(ledger) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
