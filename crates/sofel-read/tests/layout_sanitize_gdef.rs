//! GDEF load-time bounds in HarfBuzz 15.1.0 (ADR 0059).

#![allow(clippy::all, clippy::unwrap_used)]

mod support;
use support::build::Buf;

use sofel_read::gdef;
use sofel_read::layout::sanitize::{Admission, assess};

fn gdef(version: u32) -> Vec<u8> {
    let mut bytes = Buf::new().u32(version).u16(0).u16(0).u16(0).u16(0);
    if version >= 0x0001_0002 {
        bytes = bytes.u16(0);
    }
    if version >= 0x0001_0003 {
        bytes = bytes.u32(0);
    }
    if version >= 0x0001_0004 {
        for _ in 0..5 {
            bytes = bytes.u32(0);
        }
    }
    bytes.done()
}

fn admitted(bytes: &[u8]) -> bool {
    assess(gdef::TAG, Some(bytes)) == Admission::Accepted
}

#[test]
fn inv1_gdef_header_versions_and_boundaries() {
    for (version, size) in [
        (0x0001_0000, 12),
        (0x0001_0002, 14),
        (0x0001_0003, 18),
        (0x0001_0004, 38),
    ] {
        let bytes = gdef(version);
        assert_eq!(bytes.len(), size);
        assert!(admitted(&bytes), "version {version:#x}");
        assert!(
            !admitted(bytes.get(..size.saturating_sub(1)).unwrap()),
            "version {version:#x}"
        );
    }
}

#[test]
fn inv1_gdef_attach_list_descendants() {
    let mut bytes = gdef(0x0001_0000);
    bytes
        .get_mut(6..8)
        .unwrap()
        .copy_from_slice(&12u16.to_be_bytes());
    bytes.extend_from_slice(&Buf::new().u16(0).u16(1).u16(6).u16(1).u16(42).done());
    assert!(admitted(&bytes));
    let mut broken = bytes.clone();
    broken
        .get_mut(16..18)
        .unwrap()
        .copy_from_slice(&0xffffu16.to_be_bytes());
    assert!(!admitted(&broken));
}

#[test]
fn inv1_gdef_lig_caret_and_device_descendants() {
    let mut bytes = gdef(0x0001_0000);
    bytes
        .get_mut(8..10)
        .unwrap()
        .copy_from_slice(&12u16.to_be_bytes());
    bytes.extend_from_slice(
        &Buf::new()
            .u16(0)
            .u16(1)
            .u16(6)
            .u16(1)
            .u16(4)
            .u16(1)
            .u16(0)
            .done(),
    );
    assert!(admitted(&bytes));
    let mut broken = bytes.clone();
    broken
        .get_mut(16..18)
        .unwrap()
        .copy_from_slice(&0xffffu16.to_be_bytes());
    assert!(!admitted(&broken));

    let mut with_device = gdef(0x0001_0000);
    with_device
        .get_mut(8..10)
        .unwrap()
        .copy_from_slice(&12u16.to_be_bytes());
    with_device.extend_from_slice(
        &Buf::new()
            .u16(0)
            .u16(1)
            .u16(6)
            .u16(1)
            .u16(4)
            .u16(3)
            .u16(0)
            .u16(0xffff)
            .done(),
    );
    assert!(!admitted(&with_device));
}

#[test]
fn inv1_gdef_mark_sets_and_variation_store() {
    let mut mark = gdef(0x0001_0002);
    mark.get_mut(12..14)
        .unwrap()
        .copy_from_slice(&14u16.to_be_bytes());
    mark.extend_from_slice(&Buf::new().u16(1).u16(1).u32(8).u16(1).u16(1).u16(5).done());
    assert!(admitted(&mark));
    let mut broken = mark.clone();
    broken
        .get_mut(18..22)
        .unwrap()
        .copy_from_slice(&0xffffu32.to_be_bytes());
    assert!(!admitted(&broken));

    let mut var = gdef(0x0001_0003);
    var.get_mut(14..18)
        .unwrap()
        .copy_from_slice(&18u32.to_be_bytes());
    var.extend_from_slice(&Buf::new().u16(1).u32(8).u16(0).u16(0).u16(0).done());
    assert!(admitted(&var));
    let mut broken = var.clone();
    broken
        .get_mut(14..18)
        .unwrap()
        .copy_from_slice(&0xffffu32.to_be_bytes());
    assert!(!admitted(&broken));
}

#[test]
fn inv1_gdef_v14_prefers_nonzero_wide_reference() {
    let mut bytes = gdef(0x0001_0004);
    bytes
        .get_mut(4..6)
        .unwrap()
        .copy_from_slice(&0xffffu16.to_be_bytes());
    bytes
        .get_mut(18..22)
        .unwrap()
        .copy_from_slice(&38u32.to_be_bytes());
    bytes.extend_from_slice(&Buf::new().u16(1).u16(0).u16(0).done());
    assert!(admitted(&bytes));
    let mut broken_wide = bytes.clone();
    broken_wide
        .get_mut(18..22)
        .unwrap()
        .copy_from_slice(&0xffffu32.to_be_bytes());
    assert!(!admitted(&broken_wide));
    let mut narrow_fallback = bytes;
    narrow_fallback
        .get_mut(18..22)
        .unwrap()
        .copy_from_slice(&0u32.to_be_bytes());
    assert!(!admitted(&narrow_fallback));
}

// One MarkGlyphSets array points every entry at the same large Coverage. HarfBuzz
// charges the Coverage's 600 record bytes on every visit, despite sharing bytes.
fn shared_coverage_mark_sets(count: usize) -> Vec<u8> {
    let mut bytes = gdef(0x0001_0002);
    bytes
        .get_mut(12..14)
        .unwrap()
        .copy_from_slice(&14u16.to_be_bytes());
    let coverage_offset =
        u32::try_from(4usize.saturating_add(4usize.saturating_mul(count))).unwrap();
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&u16::try_from(count).unwrap().to_be_bytes());
    for _ in 0..count {
        bytes.extend_from_slice(&coverage_offset.to_be_bytes());
    }
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&300u16.to_be_bytes());
    for glyph in 0..300u16 {
        bytes.extend_from_slice(&glyph.to_be_bytes());
    }
    bytes
}

#[test]
fn inv1_gdef_budget_charges_repeated_coverage_bytes() {
    let within = shared_coverage_mark_sets(114);
    let beyond = shared_coverage_mark_sets(115);
    assert_eq!(within.len(), 1078);
    assert_eq!(beyond.len(), 1082);
    assert!(admitted(&within));
    assert!(!admitted(&beyond));
}
