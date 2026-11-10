//! ADR 0027: checked access to embedded variation stores.
#![allow(clippy::all, clippy::indexing_slicing, clippy::unwrap_used)]
use sofel_read::{base::Base, colr::Colr, gdef::Gdef};

fn table(header: usize, offset_field: usize, version: &[u8]) -> Vec<u8> {
    let mut data = vec![0; header];
    data[..version.len()].copy_from_slice(version);
    data[offset_field..offset_field.saturating_add(4)]
        .copy_from_slice(&u32::try_from(header).unwrap().to_be_bytes());
    data.extend([0, 1, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0]);
    data
}

#[test]
fn adr0027_embedded_store_views_preserve_absent_and_invalid() {
    let base = table(12, 8, &[0, 1, 0, 1]);
    let gdef = table(18, 14, &[0, 1, 0, 3]);
    let colr = table(34, 30, &[0, 1]);
    assert_eq!(
        Base::new(&base)
            .unwrap()
            .variation_store()
            .unwrap()
            .unwrap()
            .axis_count(),
        0
    );
    assert_eq!(
        Gdef::new(&gdef)
            .unwrap()
            .variation_store()
            .unwrap()
            .unwrap()
            .axis_count(),
        0
    );
    assert_eq!(
        Colr::new(&colr)
            .unwrap()
            .variation_store()
            .unwrap()
            .unwrap()
            .axis_count(),
        0
    );
    assert!(Base::new(&base[..12]).unwrap().variation_store().is_err());
    assert!(Gdef::new(&gdef[..18]).unwrap().variation_store().is_err());
    assert!(Colr::new(&colr[..34]).unwrap().variation_store().is_err());
    assert!(
        Base::new(&[0, 1, 0, 0, 0, 0, 0, 0])
            .unwrap()
            .variation_store()
            .unwrap()
            .is_none()
    );
    assert!(
        Gdef::new(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
            .unwrap()
            .variation_store()
            .unwrap()
            .is_none()
    );
}

#[test]
fn adr0027_colr_index_map_is_checked() {
    let mut data = vec![0; 34];
    data[1] = 1;
    assert!(Colr::new(&data).unwrap().var_index_map().unwrap().is_none());
    data[26..30].copy_from_slice(&34u32.to_be_bytes());
    assert!(Colr::new(&data).unwrap().var_index_map().is_err());
    data.extend([0, 0, 0, 1, 3]);
    assert_eq!(
        Colr::new(&data)
            .unwrap()
            .var_index_map()
            .unwrap()
            .unwrap()
            .map(0),
        0x10001
    );
}

#[test]
fn adr0027_embedded_store_validators_check_store_contents() {
    let mut base = table(12, 8, &[0, 1, 0, 1]);
    let mut gdef = table(18, 14, &[0, 1, 0, 3]);
    let mut colr = table(34, 30, &[0, 1]);
    base[13] = 2;
    gdef[19] = 2;
    colr[35] = 2;
    let mut diagnostics = Vec::new();
    sofel_read::base::validate(&base, &mut |d| diagnostics.push(d));
    sofel_read::gdef::validate(&gdef, &mut |d| diagnostics.push(d));
    sofel_read::colr::validate(&colr, None, None, &mut |d| diagnostics.push(d));
    for tag in [b"BASE", b"GDEF", b"COLR"] {
        assert!(
            diagnostics
                .iter()
                .any(|d| d.table == sofel_read::Tag::new(tag)
                    && matches!(d.kind, sofel_read::Kind::UnknownFormat { found: 2 })),
            "{tag:?}: {diagnostics:?}"
        );
    }
}
