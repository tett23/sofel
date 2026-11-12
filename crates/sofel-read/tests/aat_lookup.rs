#![allow(clippy::unwrap_used)]
use sofel_read::aat_lookup::AatLookup;
#[test]
fn i1_aat_shape_sparse_gid_lookups_are_exact() {
    let bytes = [0, 6, 0, 4, 0, 2, 0, 8, 0, 1, 0, 0, 0, 2, 0, 5, 0, 4, 0, 7];
    let lookup = AatLookup::new(&bytes, 8).unwrap();
    assert_eq!(lookup.get(2), Some(5));
    assert_eq!(lookup.get(3), None);
    assert_eq!(lookup.get(4), Some(7));
}
#[test]
fn i2_aat_state_lookup_does_not_cross_input() {
    let bytes = [0, 8, 0, 2, 0, 1, 0, 5];
    for n in 0..bytes.len() {
        assert!(AatLookup::new(bytes.get(..n).unwrap(), 10).is_none());
    }
}

#[test]
fn i1_aat_packed_lookup_returns_inline_index_values_only() {
    use sofel_read::aat_lookup::AatIndexLookup;
    let bytes = [0, 10, 0, 3, 0, 7, 0, 1, 1, 2, 3];
    assert_eq!(
        AatIndexLookup::new(&bytes, 8, true).unwrap().get(7),
        Some(0x010203)
    );
    assert_eq!(
        AatIndexLookup::new(&bytes, 8, false).unwrap().get(7),
        Some(0x0203)
    );
    assert_eq!(AatLookup::new(&bytes, 8).unwrap().get(7), None);
    for len in 0..bytes.len() {
        assert!(AatIndexLookup::new(bytes.get(..len).unwrap(), 8, true).is_none());
    }
}
