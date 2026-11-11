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
        assert!(!admitted(&bytes[..size - 1]), "version {version:#x}");
    }
}

#[test]
fn inv1_gdef_attach_list_descendants() {
    let mut bytes = gdef(0x0001_0000);
    bytes[6..8].copy_from_slice(&12u16.to_be_bytes());
    bytes.extend_from_slice(&Buf::new().u16(0).u16(1).u16(6).u16(1).u16(42).done());
    assert!(admitted(&bytes));
    let mut broken = bytes.clone();
    broken[16..18].copy_from_slice(&0xffffu16.to_be_bytes());
    assert!(!admitted(&broken));
}

#[test]
fn inv1_gdef_lig_caret_and_device_descendants() {
    let mut bytes = gdef(0x0001_0000);
    bytes[8..10].copy_from_slice(&12u16.to_be_bytes());
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
    broken[16..18].copy_from_slice(&0xffffu16.to_be_bytes());
    assert!(!admitted(&broken));

    let mut with_device = gdef(0x0001_0000);
    with_device[8..10].copy_from_slice(&12u16.to_be_bytes());
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
    mark[12..14].copy_from_slice(&14u16.to_be_bytes());
    mark.extend_from_slice(&Buf::new().u16(1).u16(1).u32(8).u16(1).u16(1).u16(5).done());
    assert!(admitted(&mark));
    let mut broken = mark.clone();
    broken[18..22].copy_from_slice(&0xffffu32.to_be_bytes());
    assert!(!admitted(&broken));

    let mut var = gdef(0x0001_0003);
    var[14..18].copy_from_slice(&18u32.to_be_bytes());
    var.extend_from_slice(&Buf::new().u16(1).u32(8).u16(0).u16(0).u16(0).done());
    assert!(admitted(&var));
    let mut broken = var.clone();
    broken[14..18].copy_from_slice(&0xffffu32.to_be_bytes());
    assert!(!admitted(&broken));
}

#[test]
fn inv1_gdef_v14_checks_narrow_and_wide_references() {
    let mut bytes = gdef(0x0001_0004);
    bytes[4..6].copy_from_slice(&0xffffu16.to_be_bytes());
    bytes[18..22].copy_from_slice(&38u32.to_be_bytes());
    bytes.extend_from_slice(&Buf::new().u16(1).u16(0).u16(0).done());
    assert!(!admitted(&bytes));
}
