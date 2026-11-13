use sofel_read::cff::validate_bare;
use sofel_read::{Kind, Severity};

const TWO_FACES: &[u8] = &[
    1, 0, 4, 4, 0, 2, 1, 1, 2, 3, b'A', b'B', 0, 2, 1, 1, 3, 7, 167, 17, 167, 17, 181, 13, 0, 0, 0,
    0, 0, 1, 1, 1, 2, 14,
];

const MISSING_SECOND_CHARSTRINGS: &[u8] = &[
    1, 0, 4, 4, 0, 2, 1, 1, 2, 3, b'A', b'B', 0, 2, 1, 1, 3, 5, 165, 17, 181, 13, 0, 0, 0, 0, 0, 1,
    1, 1, 2, 14,
];

#[test]
fn adr0065_i1_bare_cff_validator_accepts_two_faces() {
    let mut diagnostics = Vec::new();
    validate_bare(TWO_FACES, &mut |diagnostic| diagnostics.push(diagnostic));
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
    );
}

#[test]
fn adr0065_i2_bare_cff_validator_checks_second_face() {
    let mut diagnostics = Vec::new();
    validate_bare(MISSING_SECOND_CHARSTRINGS, &mut |diagnostic| {
        diagnostics.push(diagnostic)
    });
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| matches!(diagnostic.kind, Kind::MissingCharStrings))
    );
}
