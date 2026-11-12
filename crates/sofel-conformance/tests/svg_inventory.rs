//! Required observations and the result of a partial verification (ADR 0074).
use std::process::Command;

const INVENTORY: &str = include_str!("../../../conformance/svg-cases.tsv");

fn run(ledger: &str) -> (bool, String) {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_sofel-svg-conformance"))
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(ledger.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

#[test]
fn i1_svg_inventory() {
    let row = INVENTORY
        .lines()
        .find(|line| line.starts_with("in-document\tout-of-range\t"))
        .unwrap();
    let (ok, empty) = run("");
    assert!(!ok && empty.contains("missing case: in-document/out-of-range"));
    let (_, missing) = run(&INVENTORY.replace(row, ""));
    assert!(missing.contains("missing case: in-document/out-of-range"));
    let (_, duplicate) = run(&format!("{INVENTORY}\n{row}\n"));
    assert!(duplicate.contains("duplicate case: in-document/out-of-range"));
    let (_, missing) = run(&INVENTORY.replace("page-size,scene-tree", "page-size"));
    assert!(missing.contains("missing observation: out-page/empty/scene-tree"));
}

#[test]
fn i2_partial_cases_fail_and_i3_results_repeat() {
    let missing_reference = INVENTORY.replace("SVG 2 shapes and coordinate systems", "");
    let result = run(&missing_reference);
    assert!(!result.0);
    assert!(result.1.contains("in-document\tout-of-range\tinput\t"));
    assert!(result.1.lines().any(|line| line.starts_with("in-document\tout-of-range\t") && line.ends_with("\tPASS")));
    assert!(result.1.contains("missing expectation source"));
    assert_eq!(result, run(&missing_reference));
}
