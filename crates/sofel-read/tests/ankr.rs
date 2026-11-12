#![allow(clippy::indexing_slicing, clippy::expect_used)]

use sofel_read::ankr::Ankr;

#[test]
fn adr0060_i2_checks_anchor_lists_and_accepts_either_table_order() {
    let lookup = [0, 8, 0, 1, 0, 2, 0, 0, 0, 8];
    let anchors = [
        0, 0, 0, 1, 0, 250, 2, 88, 0, 0, 0, 2, 0, 75, 0, 0, 0, 0, 0, 0,
    ];
    for lookup_first in [false, true] {
        let mut table = vec![0; 12];
        let (lookup_offset, anchor_offset) = if lookup_first {
            table.extend_from_slice(&lookup);
            table.extend_from_slice(&anchors);
            (12u32, 22u32)
        } else {
            table.extend_from_slice(&anchors);
            table.extend_from_slice(&lookup);
            (32, 12)
        };
        table[4..8].copy_from_slice(&lookup_offset.to_be_bytes());
        table[8..12].copy_from_slice(&anchor_offset.to_be_bytes());
        let view = Ankr::new(&table, 3).expect("valid anchor table");
        assert_eq!(view.get(1, 0), Some((250, 600)));
        assert_eq!(view.get(2, 1), Some((0, 0)));
        assert_eq!(view.get(2, 2), None);
        table[anchor_offset as usize + 11] = 255;
        assert!(Ankr::new(&table, 3).is_none());
    }
}
