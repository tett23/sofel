//! The serializer against hand-assembled bytes (ADR 0022), after
//! HarfBuzz's `src/test-serialize.cc`.

#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]

use sofel_write::{Errors, Packed, Serializer, Whence};

fn child(s: &mut Serializer, bytes: &[u8], share: bool) -> u32 {
    s.push();
    s.extend(bytes);
    s.pop_pack(share)
}

#[test]
fn root_only() {
    let mut s = Serializer::new();
    s.u16(0x1234);
    let p = s.end().unwrap();
    assert_eq!(p, Packed::Root(vec![0x12, 0x34]));
    assert_eq!(p.resolve().unwrap(), vec![0x12, 0x34]);
}

#[test]
fn children_are_laid_out_last_packed_first() {
    let mut s = Serializer::new();
    let at_a = s.u16(0);
    let at_b = s.u16(0);
    let a = child(&mut s, &[0xAA], true);
    let b = child(&mut s, &[0xBB, 0xBB], true);
    s.link16(at_a, a);
    s.link16(at_b, b);
    let out = s.end().unwrap().resolve().unwrap();
    // root, b, a
    assert_eq!(out, vec![0, 6, 0, 4, 0xBB, 0xBB, 0xAA]);
}

#[test]
fn shared_and_merged_children() {
    let mut s = Serializer::new();
    let at = [s.u16(0), s.u16(0), s.u16(0)];
    let a = child(&mut s, &[1, 2], true);
    let b = child(&mut s, &[1, 2], true);
    let c = child(&mut s, &[1, 2], false);
    assert_eq!(a, b);
    assert_ne!(a, c);
    for (p, o) in at.iter().zip([a, b, c]) {
        s.link16(*p, o);
    }
    let out = s.end().unwrap().resolve().unwrap();
    assert_eq!(out, vec![0, 8, 0, 8, 0, 6, 1, 2, 1, 2]);
}

#[test]
fn nested_and_link_kinds() {
    let mut s = Serializer::new();
    s.zeros(9); // 2 + 3 + 4
    s.push();
    s.u16(0);
    let leaf = child(&mut s, &[0xCC], true);
    s.add_link(0, 2, true, leaf, Whence::Tail, 0);
    let mid = s.pop_pack(true);
    s.add_link(0, 2, false, mid, Whence::Head, 0);
    s.add_link(2, 3, false, mid, Whence::Absolute, 0);
    s.add_link(5, 4, false, leaf, Whence::Head, 1);
    let out = s.end().unwrap().resolve().unwrap();
    // root(9) mid(2) leaf(1)
    assert_eq!(out, vec![0, 9, 0, 0, 9, 0, 0, 0, 10, 0, 0, 0xCC]);
}

#[test]
fn snapshot_revert() {
    let mut s = Serializer::new();
    s.u16(7);
    let snap = s.snapshot();
    s.u16(8);
    let a = child(&mut s, &[1], true);
    s.link16(2, a);
    s.err(Errors::INT_OVERFLOW);
    s.revert(snap);
    assert!(!s.in_error());
    assert_eq!(s.current(), &[0, 7]);
    // The reverted object is gone, and so is its sharing entry.
    let b = child(&mut s, &[1], true);
    assert_eq!(b, 1);
    let out = s.end().unwrap();
    assert_eq!(out.objects().len(), 3);
}

#[test]
fn pop_discard_drops_grandchildren() {
    let mut s = Serializer::new();
    s.u16(0);
    s.push();
    s.u8(1);
    let _g = child(&mut s, &[2], true);
    s.pop_discard();
    assert_eq!(s.end().unwrap(), Packed::Root(vec![0, 0]));
}

#[test]
fn overflow_is_detected() {
    let mut s = Serializer::new();
    let at = s.u16(0);
    let a = child(&mut s, &[1], true);
    let big = child(&mut s, &vec![0; 70000], true);
    s.link16(at, a);
    let _ = big;
    let p = s.end().unwrap();
    // a is laid out after the 70000 bytes of big.
    assert_eq!(p.resolve(), Err(Errors::OFFSET_OVERFLOW));
}

#[test]
fn signed_offsets_overflow_at_32768() {
    let mut s = Serializer::new();
    let at = s.u16(0);
    let a = child(&mut s, &[1], true);
    let _pad = child(&mut s, &vec![9; 32766], true);
    s.add_link(at, 2, true, a, Whence::Head, 0);
    // root(2) pad(32766) a: offset 32768.
    assert_eq!(s.end().unwrap().resolve(), Err(Errors::OFFSET_OVERFLOW));
}

#[test]
fn virtual_links_merge_on_share() {
    let mut s = Serializer::new();
    let x = child(&mut s, &[5], true);
    s.push();
    s.u8(1);
    s.add_virtual_link(x);
    let a = s.pop_pack(true);
    s.push();
    s.u8(1);
    let b = s.pop_pack(true);
    assert_eq!(a, b);
    let p = s.end().unwrap();
    assert_eq!(p.objects()[a as usize].virtual_links.len(), 1);
}

/// The room HarfBuzz's buffer needs: the bytes on the stack and the packed
/// bytes at their highest; discarded, reverted and shared objects give
/// their room back.
#[test]
fn peak_len_is_the_most_bytes_held() {
    let mut s = Serializer::new();
    assert_eq!(s.peak_len(), 0);
    s.extend(b"root");
    s.push();
    s.extend(&[1; 10]);
    assert_eq!(s.peak_len(), 14);
    let a = s.pop_pack(true);
    s.push();
    s.extend(&[1; 10]);
    assert_eq!(s.pop_pack(true), a); // shared: no new bytes packed
    assert_eq!(s.peak_len(), 24);
    s.push();
    s.zeros(6);
    s.pop_discard();
    assert_eq!(s.peak_len(), 24);
    s.push();
    s.extend(&[2; 3]);
    assert_eq!(s.peak_len(), 24);
    let snap = s.snapshot();
    s.extend(&[2; 20]);
    assert_eq!(s.peak_len(), 4 + 10 + 23);
    s.revert(snap);
    s.extend(&[2; 1]);
    assert_eq!(s.peak_len(), 37);
}
