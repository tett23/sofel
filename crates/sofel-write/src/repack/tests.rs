//! Ports of HarfBuzz 15.1.0's `src/test-repacker.cc` and
//! `src/graph/test-classdef-graph.cc`: the same graphs and the same
//! expected results.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::panic,
    clippy::cast_sign_loss,
    clippy::too_many_arguments,
    clippy::needless_late_init,
    clippy::needless_range_loop
)]

use alloc::vec;
use alloc::vec::Vec;

use super::graph::{Graph, GraphError, MAX_VERTICES};
use super::gsubgpos::{Context, GPOS, GSUB};
use super::layout::{ClassDefSizeEstimator, class_def_bytes, class_of, coverage_bytes};
use super::pack::{Overflow, serialize, will_overflow};
use super::set::HbSet;
use super::{
    markbasepos, pairpos, resolve_graph_overflows, resolve_overflows, resolve_shared_overflow,
    with_resolved_offsets,
};
use crate::serialize::{Errors, Object, Packed, Serializer};

const NONE: u32 = 0;
const ABCD: u32 = u32::from_be_bytes(*b"abcd");

fn big(len: usize) -> Vec<u8> {
    vec![b'a'; len]
}

// --- The helpers of test-repacker.cc -------------------------------------

fn extend(value: &[u8], c: &mut Serializer) {
    c.extend(value);
}

fn start_object(value: &[u8], c: &mut Serializer) {
    c.push();
    extend(value, c);
}

fn add_object(value: &[u8], c: &mut Serializer) -> u32 {
    start_object(value, c);
    c.pop_pack(false)
}

fn add_object_shared(value: &[u8], c: &mut Serializer, shared: bool) -> u32 {
    start_object(value, c);
    c.pop_pack(shared)
}

fn add_offset(id: u32, c: &mut Serializer) {
    let at = c.zeros(2);
    c.link16(at, id);
}

fn add_24_offset(id: u32, c: &mut Serializer) {
    let at = c.zeros(3);
    c.link24(at, id);
}

fn add_wide_offset(id: u32, c: &mut Serializer) {
    let at = c.zeros(4);
    c.link32(at, id);
}

fn add_virtual_offset(id: u32, c: &mut Serializer) {
    c.add_virtual_link(id);
}

fn add_gsubgpos_header(lookup_list: u32, c: &mut Serializer) {
    start_object(&[0, 1, 0, 0, 0, 0, 0, 0], c);
    add_offset(lookup_list, c);
    c.pop_pack(false);
}

fn add_gsubgpos_header_1_2(lookup_list: u32, c: &mut Serializer) {
    let mut header = [0u8; 22];
    header[1] = 1;
    header[3] = 2;
    start_object(&header, c);
    add_wide_offset(lookup_list, c);
    c.pop_pack(false);
}

fn add_lookup_list(lookups: &[u32], c: &mut Serializer) -> u32 {
    start_object(&[0, lookups.len() as u8], c);
    for &l in lookups {
        add_offset(l, c);
    }
    c.pop_pack(false)
}

fn add_lookup_list_1_2(lookups: &[u32], c: &mut Serializer) -> u32 {
    start_object(&[0, lookups.len() as u8], c);
    for &l in lookups {
        add_wide_offset(l, c);
    }
    c.pop_pack(false)
}

fn start_lookup(kind: u8, num_subtables: u8, c: &mut Serializer) {
    start_object(&[0, kind, 0, 2, 0, num_subtables], c);
}

fn finish_lookup(c: &mut Serializer) -> u32 {
    extend(&[0, 0], c);
    c.pop_pack(false)
}

fn add_extension(child: u32, kind: u8, c: &mut Serializer) -> u32 {
    start_object(&[0, 1, 0, kind], c);
    add_wide_offset(child, c);
    c.pop_pack(false)
}

/// A Coverage of `[start, end]`.
fn add_coverage(start: u32, end: u32, c: &mut Serializer, shared: bool) -> u32 {
    let hi = |v: u32| (v >> 8) as u8;
    let lo = |v: u32| v as u8;
    if end - start == 0 {
        return add_object_shared(&[0, 1, 0, 1, hi(start), lo(start)], c, shared);
    }
    if end - start == 1 {
        return add_object_shared(
            &[0, 1, 0, 2, hi(start), lo(start), hi(end), lo(end)],
            c,
            shared,
        );
    }
    add_object_shared(
        &[0, 2, 0, 1, hi(start), lo(start), hi(end), lo(end), 0, 0],
        c,
        shared,
    )
}

fn add_coverage_of(glyphs: &[u32], c: &mut Serializer) -> u32 {
    c.push();
    c.extend(&coverage_bytes(glyphs, usize::MAX).unwrap());
    c.pop_pack(false)
}

/// A ClassDef mapping glyphs `[start_glyph, end_glyph)` to classes 1...n.
fn add_class_def(start_glyph: u16, end_glyph: u16, c: &mut Serializer) -> u32 {
    let count = end_glyph - start_glyph;
    let mut b = vec![0u8, 1];
    b.extend_from_slice(&start_glyph.to_be_bytes());
    b.extend_from_slice(&count.to_be_bytes());
    start_object(&b, c);
    for i in 1..=count {
        extend(&i.to_be_bytes(), c);
    }
    c.pop_pack(false)
}

fn add_pair_pos_1(pair_sets: &[u32], coverage: u32, c: &mut Serializer) -> u32 {
    start_object(&[0, 1], c);
    add_offset(coverage, c);
    extend(&[0, 0, 0, 0, 0, pair_sets.len() as u8], c);
    for &p in pair_sets {
        add_offset(p, c);
    }
    c.pop_pack(false)
}

fn add_liga_set_header(liga_count: u8, c: &mut Serializer) {
    start_object(&[0, liga_count], c);
}

fn add_liga_header(coverage: u32, liga_set_count: u32, c: &mut Serializer) {
    start_object(&[0, 1], c);
    add_offset(coverage, c);
    extend(&(liga_set_count as u16).to_be_bytes(), c);
}

fn add_pair_pos_2(
    starting_class: u32,
    coverage: u32,
    class_def_1: u32,
    class_def_1_count: u16,
    class_def_2: u32,
    class_def_2_count: u16,
    device_tables: Option<&[u32]>,
    c: &mut Serializer,
) -> u32 {
    start_object(&[0, 2], c);
    add_offset(coverage, c);
    let format1 = 0x01 | 0x02 | 0x08;
    let mut format2 = 0x04;
    if device_tables.is_some() {
        format2 |= 0x20;
    }
    extend(&[0, format1, 0, format2], c);
    add_offset(class_def_1, c);
    add_offset(class_def_2, c);
    extend(&class_def_1_count.to_be_bytes(), c);
    extend(&class_def_2_count.to_be_bytes(), c);
    let mut device_index = 0;
    for i in 0..class_def_1_count {
        for _ in 0..class_def_2_count {
            for _ in 0..4 {
                let v = (u32::from(i) + starting_class) as u8;
                extend(&[v, v], c);
            }
            if let Some(d) = device_tables {
                add_offset(d[device_index], c);
                device_index += 1;
            }
        }
    }
    c.pop_pack(false)
}

fn add_mark_base_pos_1(
    mark_coverage: u32,
    base_coverage: u32,
    mark_array: u32,
    base_array: u32,
    class_count: u32,
    c: &mut Serializer,
) -> u32 {
    start_object(&[0, 1], c);
    add_offset(mark_coverage, c);
    add_offset(base_coverage, c);
    extend(&(class_count as u16).to_be_bytes(), c);
    add_offset(mark_array, c);
    add_offset(base_array, c);
    c.pop_pack(false)
}

struct MarkBasePosBuffers {
    mark_count: u32,
    class_count: u32,
    base_count: u32,
    table_count: u32,
    base_anchors: Vec<u32>,
    mark_anchors: Vec<u32>,
    class_buffer: Vec<u8>,
}

impl MarkBasePosBuffers {
    fn new(
        mark_count: u32,
        class_count: u32,
        base_count: u32,
        table_count: u32,
        c: &mut Serializer,
    ) -> Self {
        let n = (class_count * base_count + 100) as usize;
        let mut anchor_buffers = vec![0u8; n];
        for i in 0..n / 2 {
            anchor_buffers[2 * i..2 * i + 2].copy_from_slice(&(i as u16).to_be_bytes());
        }
        let mut class_buffer = vec![0u8; class_count as usize * 2];
        let mut base_anchors = Vec::new();
        for i in 0..(class_count * base_count) as usize {
            base_anchors.push(add_object(&anchor_buffers[i..i + 100], c));
            if i < class_count as usize {
                class_buffer[i * 2] = (i >> 8) as u8;
                class_buffer[i * 2 + 1] = i as u8;
            }
        }
        let mark_anchors = (0..mark_count as usize)
            .map(|i| add_object(&anchor_buffers[i..i + 4], c))
            .collect();
        MarkBasePosBuffers {
            mark_count,
            class_count,
            base_count,
            table_count,
            base_anchors,
            mark_anchors,
            class_buffer,
        }
    }

    fn create_mark_base_pos_1(&self, table_index: u32, c: &mut Serializer) -> u32 {
        let class_per_table = self.class_count / self.table_count;
        let mark_per_class = self.mark_count / self.class_count;
        let start_class = class_per_table * table_index;
        let end_class = class_per_table * (table_index + 1) - 1;

        start_object(&(self.base_count as u16).to_be_bytes(), c);
        for base in 0..self.base_count {
            for klass in start_class..=end_class {
                let i = base * self.class_count + klass;
                add_offset(self.base_anchors[i as usize], c);
            }
        }
        let base_array = c.pop_pack(false);

        let num_marks = class_per_table * mark_per_class;
        start_object(&(num_marks as u16).to_be_bytes(), c);
        for mark in 0..self.mark_count {
            let klass = mark % self.class_count;
            if klass < start_class || klass > end_class {
                continue;
            }
            let klass = klass - start_class;
            extend(
                &self.class_buffer[2 * klass as usize..2 * klass as usize + 2],
                c,
            );
            add_offset(self.mark_anchors[mark as usize], c);
        }
        let mark_array = c.pop_pack(false);

        let glyphs: Vec<u32> = (0..self.mark_count)
            .filter(|m| {
                let klass = m % self.class_count;
                klass >= class_per_table * table_index
                    && klass < class_per_table * (table_index + 1)
            })
            .collect();
        let mark_coverage = add_coverage_of(&glyphs, c);
        let base_coverage = add_coverage(10, 10 + self.base_count - 1, c, false);
        add_mark_base_pos_1(
            mark_coverage,
            base_coverage,
            mark_array,
            base_array,
            class_per_table,
            c,
        )
    }
}

/// `c.end_serialize ()` and `c.object_graph ()`, with whether an offset
/// overflowed (`c.offset_overflow ()`).
fn finish(c: Serializer) -> (Vec<Object>, bool) {
    let packed = c.end().unwrap();
    let overflow = packed.resolve() == Err(Errors::OFFSET_OVERFLOW);
    let objs = match packed {
        Packed::Graph(o) => with_resolved_offsets(&o),
        Packed::Root(_) => panic!("no objects"),
    };
    (objs, overflow)
}

fn graph(c: Serializer) -> Graph {
    Graph::create(&finish(c).0).unwrap()
}

fn run_resolve_overflow_test(
    name: &str,
    overflowing: Serializer,
    expected: Serializer,
    num_iterations: u32,
    recalculate_extensions: bool,
    tag: u32,
    check_binary_equivalence: bool,
) {
    let (objs, overflow) = finish(overflowing);
    let mut g = Graph::create(&objs).unwrap();
    let mut e = Graph::create(&finish(expected).0).unwrap();
    if will_overflow(&mut e, None).unwrap() {
        assert!(
            !check_binary_equivalence,
            "{name}: the expected graph overflows"
        );
        assert!(e.assign_spaces().is_ok());
        assert!(e.sort_shortest_distance().is_ok());
    }
    assert!(overflow, "{name}: no overflow to resolve");
    let r = resolve_graph_overflows(tag, num_iterations, recalculate_extensions, &mut g);
    assert!(r.is_ok(), "{name}: {r:?}");
    let out1 = serialize(&g).unwrap();
    let out2 = serialize(&e).unwrap();
    if check_binary_equivalence {
        assert!(
            out1 == out2,
            "{name}: result\n{}expected\n{}",
            g.describe(),
            e.describe()
        );
    }
    g.normalize();
    e.normalize();
    assert!(
        g.graph_equals(&e),
        "{name}: expected\n{}result\n{}",
        e.describe(),
        g.describe()
    );
}

// --- The graphs -----------------------------------------------------------

fn populate_serializer_simple(c: &mut Serializer) {
    let obj_1 = add_object(b"ghi", c);
    let obj_2 = add_object(b"def", c);
    start_object(b"abc", c);
    add_offset(obj_2, c);
    add_offset(obj_1, c);
    c.pop_pack(false);
}

fn populate_serializer_virtual(c: &mut Serializer, with_overflow: bool) {
    let large = big(50000);
    let (obj_4, obj_5) = if with_overflow {
        let obj_5 = add_object(b"55555", c);
        let obj_4 = add_object(&large, c);
        (obj_4, obj_5)
    } else {
        let obj_4 = add_object(&large, c);
        let obj_5 = add_object(b"55555", c);
        (obj_4, obj_5)
    };
    start_object(&large[..20000], c);
    add_offset(obj_5, c);
    let obj_3 = c.pop_pack(false);
    start_object(b"2\0", c);
    add_virtual_offset(obj_5, c);
    let obj_2 = c.pop_pack(false);
    start_object(b"1", c);
    add_offset(obj_2, c);
    add_offset(obj_3, c);
    add_offset(obj_4, c);
    c.pop_pack(false);
}

fn populate_serializer_with_overflow(c: &mut Serializer) {
    let obj_1 = add_object(&big(10000), c);
    let obj_2 = add_object(&big(20000), c);
    let obj_3 = add_object(&big(50000), c);
    start_object(b"abc", c);
    add_offset(obj_3, c);
    add_offset(obj_2, c);
    add_offset(obj_1, c);
    c.pop_pack(false);
}

fn populate_serializer_with_priority_overflow(c: &mut Serializer) {
    let obj_e = add_object(b"e", c);
    let obj_d = add_object(b"d", c);
    start_object(&big(50000), c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(&big(20000), c);
    add_offset(obj_d, c);
    let obj_b = c.pop_pack(false);
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_offset(obj_c, c);
    c.pop_pack(false);
}

fn populate_serializer_with_priority_overflow_expected(c: &mut Serializer) {
    let obj_e = add_object(b"e", c);
    start_object(&big(50000), c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    let obj_d = add_object(b"d", c);
    start_object(&big(20000), c);
    add_offset(obj_d, c);
    let obj_b = c.pop_pack(false);
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_offset(obj_c, c);
    c.pop_pack(false);
}

fn populate_serializer_with_dedup_overflow(c: &mut Serializer) {
    let obj_1 = add_object(b"def", c);
    start_object(&big(60000), c);
    add_offset(obj_1, c);
    let obj_2 = c.pop_pack(false);
    start_object(&big(10000), c);
    add_offset(obj_2, c);
    add_offset(obj_1, c);
    c.pop_pack(false);
}

fn populate_serializer_with_multiple_dedup_overflow(c: &mut Serializer) {
    let leaf = add_object(b"def", c);
    let mid: Vec<u32> = (0..20)
        .map(|i| {
            start_object(&big(10000 + i), c);
            add_offset(leaf, c);
            c.pop_pack(false)
        })
        .collect();
    start_object(b"abc", c);
    for &m in &mid {
        add_wide_offset(m, c);
    }
    c.pop_pack(false);
}

fn populate_serializer_with_unresolvable_priority_overflow(c: &mut Serializer) {
    let leaves: Vec<u32> = (0..10000u32)
        .map(|i| add_object(&[b'l', (i >> 8) as u8, i as u8], c))
        .collect();
    start_object(b"s", c);
    for &l in &leaves[..40] {
        add_offset(l, c);
    }
    let small_parent = c.pop_pack(false);
    start_object(&big(70000), c);
    for &l in &leaves {
        add_offset(l, c);
    }
    let large_parent = c.pop_pack(false);
    start_object(b"r", c);
    add_offset(small_parent, c);
    add_offset(large_parent, c);
    c.pop_pack(false);
}

fn populate_serializer_with_isolation_overflow(c: &mut Serializer) {
    let obj_4 = add_object(b"4", c);
    start_object(&big(60000), c);
    add_offset(obj_4, c);
    let obj_3 = c.pop_pack(false);
    start_object(&big(10000), c);
    add_offset(obj_4, c);
    let obj_2 = c.pop_pack(false);
    start_object(b"1", c);
    add_wide_offset(obj_3, c);
    add_offset(obj_2, c);
    c.pop_pack(false);
}

fn populate_serializer_with_isolation_overflow_complex(c: &mut Serializer) {
    let obj_f = add_object(b"f", c);
    start_object(b"e", c);
    add_offset(obj_f, c);
    let obj_e = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(b"d", c);
    add_offset(obj_e, c);
    let obj_d = c.pop_pack(false);
    start_object(&big(60000), c);
    add_offset(obj_d, c);
    let obj_h = c.pop_pack(false);
    start_object(&big(60000), c);
    add_offset(obj_c, c);
    add_offset(obj_h, c);
    let obj_b = c.pop_pack(false);
    start_object(&big(10000), c);
    add_offset(obj_d, c);
    let obj_g = c.pop_pack(false);
    start_object(&big(11000), c);
    add_offset(obj_d, c);
    let obj_i = c.pop_pack(false);
    start_object(b"a", c);
    add_wide_offset(obj_b, c);
    add_offset(obj_g, c);
    add_offset(obj_i, c);
    c.pop_pack(false);
}

fn populate_serializer_with_isolation_overflow_complex_expected(c: &mut Serializer) {
    // space 1
    let obj_f_prime = add_object(b"f", c);
    start_object(b"e", c);
    add_offset(obj_f_prime, c);
    let obj_e_prime = c.pop_pack(false);
    start_object(b"d", c);
    add_offset(obj_e_prime, c);
    let obj_d_prime = c.pop_pack(false);
    start_object(&big(60000), c);
    add_offset(obj_d_prime, c);
    let obj_h = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e_prime, c);
    let obj_c = c.pop_pack(false);
    start_object(&big(60000), c);
    add_offset(obj_c, c);
    add_offset(obj_h, c);
    let obj_b = c.pop_pack(false);
    // space 0
    let obj_f = add_object(b"f", c);
    start_object(b"e", c);
    add_offset(obj_f, c);
    let obj_e = c.pop_pack(false);
    start_object(b"d", c);
    add_offset(obj_e, c);
    let obj_d = c.pop_pack(false);
    start_object(&big(11000), c);
    add_offset(obj_d, c);
    let obj_i = c.pop_pack(false);
    start_object(&big(10000), c);
    add_offset(obj_d, c);
    let obj_g = c.pop_pack(false);
    start_object(b"a", c);
    add_wide_offset(obj_b, c);
    add_offset(obj_g, c);
    add_offset(obj_i, c);
    c.pop_pack(false);
}

fn populate_serializer_with_isolation_overflow_spaces(c: &mut Serializer) {
    let obj_d = add_object(b"f", c);
    let obj_e = add_object(b"f", c);
    start_object(&big(60000), c);
    add_offset(obj_d, c);
    let obj_b = c.pop_pack(true);
    start_object(&big(60000), c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(true);
    start_object(b"a", c);
    add_wide_offset(obj_b, c);
    add_wide_offset(obj_c, c);
    c.pop_pack(true);
}

fn populate_serializer_with_repack_last(c: &mut Serializer, with_overflow: bool) {
    let large = vec![b'c'; 70000];
    c.push();
    let mut obj_e_1 = 0;
    let obj_e_2;
    if with_overflow {
        obj_e_1 = add_object(b"a", c);
        obj_e_2 = obj_e_1;
    } else {
        obj_e_2 = add_object(b"a", c);
    }
    c.push();
    add_offset(obj_e_2, c);
    extend(&large[..30000], c);
    let obj_d = c.pop_pack(false);
    add_offset(obj_d, c);
    assert_eq!(c.last_added_child_index(), Some(obj_d));
    if !with_overflow {
        obj_e_1 = add_object(b"a", c);
    }
    c.push();
    add_offset(obj_e_1, c);
    extend(&large[..40000], c);
    let obj_c = c.pop_pack(false);
    add_offset(obj_c, c);
    let obj_b = add_object(b"b", c);
    add_offset(obj_b, c);
    c.repack_last(obj_d);
    c.pop_pack(false);
}

fn populate_serializer_spaces(c: &mut Serializer, with_overflow: bool) {
    let large = big(70000);
    let mut obj_i = 0;
    if with_overflow {
        obj_i = add_object(b"i", c);
    }
    // Space 2
    let obj_h = add_object(b"h", c);
    start_object(&large[..30000], c);
    add_offset(obj_h, c);
    let obj_e = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_e, c);
    let obj_b = c.pop_pack(false);
    // Space 1
    if !with_overflow {
        obj_i = add_object(b"i", c);
    }
    start_object(&large[..30000], c);
    add_offset(obj_i, c);
    let obj_g = c.pop_pack(false);
    start_object(&large[..30000], c);
    add_offset(obj_i, c);
    let obj_f = c.pop_pack(false);
    start_object(b"d", c);
    add_offset(obj_g, c);
    let obj_d = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_f, c);
    let obj_c = c.pop_pack(false);
    start_object(b"a", c);
    add_wide_offset(obj_b, c);
    add_wide_offset(obj_c, c);
    add_wide_offset(obj_d, c);
    c.pop_pack(false);
}

fn populate_serializer_spaces_16bit_connection(c: &mut Serializer) {
    let obj_g = add_object(b"g", c);
    let obj_h = add_object(b"h", c);
    start_object(&big(40000), c);
    add_offset(obj_g, c);
    let obj_e = c.pop_pack(false);
    start_object(&big(40000), c);
    add_offset(obj_h, c);
    let obj_f = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(b"d", c);
    add_offset(obj_f, c);
    let obj_d = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_e, c);
    add_offset(obj_h, c);
    let obj_b = c.pop_pack(false);
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_wide_offset(obj_c, c);
    add_wide_offset(obj_d, c);
    c.pop_pack(false);
}

fn populate_serializer_spaces_16bit_connection_expected(c: &mut Serializer) {
    let obj_g_prime = add_object(b"g", c);
    start_object(&big(40000), c);
    add_offset(obj_g_prime, c);
    let obj_e_prime = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e_prime, c);
    let obj_c = c.pop_pack(false);
    let obj_h_prime = add_object(b"h", c);
    start_object(&big(40000), c);
    add_offset(obj_h_prime, c);
    let obj_f = c.pop_pack(false);
    start_object(b"d", c);
    add_offset(obj_f, c);
    let obj_d = c.pop_pack(false);
    let obj_g = add_object(b"g", c);
    start_object(&big(40000), c);
    add_offset(obj_g, c);
    let obj_e = c.pop_pack(false);
    let obj_h = add_object(b"h", c);
    start_object(b"b", c);
    add_offset(obj_e, c);
    add_offset(obj_h, c);
    let obj_b = c.pop_pack(false);
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_wide_offset(obj_c, c);
    add_wide_offset(obj_d, c);
    c.pop_pack(false);
}

fn populate_serializer_short_and_wide_subgraph_root(c: &mut Serializer) {
    let obj_e = add_object(b"e", c);
    start_object(&big(40000), c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(&big(40000), c);
    add_offset(obj_c, c);
    let obj_d = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_c, c);
    add_offset(obj_e, c);
    let obj_b = c.pop_pack(false);
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_wide_offset(obj_c, c);
    add_wide_offset(obj_d, c);
    c.pop_pack(false);
}

fn populate_serializer_short_and_wide_subgraph_root_expected(c: &mut Serializer) {
    let obj_e_prime = add_object(b"e", c);
    start_object(&big(40000), c);
    add_offset(obj_e_prime, c);
    let obj_c_prime = c.pop_pack(false);
    start_object(&big(40000), c);
    add_offset(obj_c_prime, c);
    let obj_d = c.pop_pack(false);
    let obj_e = add_object(b"e", c);
    start_object(&big(40000), c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_c, c);
    add_offset(obj_e, c);
    let obj_b = c.pop_pack(false);
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_wide_offset(obj_c_prime, c);
    add_wide_offset(obj_d, c);
    c.pop_pack(false);
}

fn populate_serializer_with_split_spaces(c: &mut Serializer) {
    let obj_f = add_object(b"f", c);
    start_object(&big(40000), c);
    add_offset(obj_f, c);
    let obj_d = c.pop_pack(false);
    start_object(&big(40000), c);
    add_offset(obj_f, c);
    let obj_e = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_d, c);
    let obj_b = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(b"a", c);
    add_wide_offset(obj_b, c);
    add_wide_offset(obj_c, c);
    c.pop_pack(false);
}

fn populate_serializer_with_split_spaces_2(c: &mut Serializer) {
    let obj_f = add_object(b"f", c);
    start_object(&big(40000), c);
    add_offset(obj_f, c);
    let obj_d = c.pop_pack(false);
    start_object(&big(40000), c);
    add_offset(obj_f, c);
    let obj_e = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_d, c);
    let obj_b = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_wide_offset(obj_b, c);
    add_wide_offset(obj_c, c);
    c.pop_pack(false);
}

fn populate_serializer_with_split_spaces_expected(c: &mut Serializer) {
    let obj_f_prime = add_object(b"f", c);
    start_object(&big(40000), c);
    add_offset(obj_f_prime, c);
    let obj_d = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_d, c);
    let obj_b = c.pop_pack(false);
    let obj_f = add_object(b"f", c);
    start_object(&big(40000), c);
    add_offset(obj_f, c);
    let obj_e = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(b"a", c);
    add_wide_offset(obj_b, c);
    add_wide_offset(obj_c, c);
    c.pop_pack(false);
}

fn populate_serializer_with_split_spaces_expected_2(c: &mut Serializer) {
    // Space 2
    let obj_f_double_prime = add_object(b"f", c);
    start_object(&big(40000), c);
    add_offset(obj_f_double_prime, c);
    let obj_d_prime = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_d_prime, c);
    let obj_b_prime = c.pop_pack(false);
    // Space 1
    let obj_f_prime = add_object(b"f", c);
    start_object(&big(40000), c);
    add_offset(obj_f_prime, c);
    let obj_e = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    // Space 0
    let obj_f = add_object(b"f", c);
    start_object(&big(40000), c);
    add_offset(obj_f, c);
    let obj_d = c.pop_pack(false);
    start_object(b"b", c);
    add_offset(obj_d, c);
    let obj_b = c.pop_pack(false);
    // Root
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_wide_offset(obj_b_prime, c);
    add_wide_offset(obj_c, c);
    c.pop_pack(false);
}

fn populate_serializer_complex_2(c: &mut Serializer) {
    let obj_5 = add_object(b"mn", c);
    let obj_4 = add_object(b"jkl", c);
    start_object(b"ghi", c);
    add_offset(obj_4, c);
    let obj_3 = c.pop_pack(false);
    start_object(b"def", c);
    add_offset(obj_3, c);
    let obj_2 = c.pop_pack(false);
    start_object(b"abc", c);
    add_offset(obj_2, c);
    add_offset(obj_4, c);
    add_offset(obj_5, c);
    c.pop_pack(false);
}

fn populate_serializer_complex_3(c: &mut Serializer) {
    let obj_6 = add_object(b"opqrst", c);
    let obj_5 = add_object(b"mn", c);
    start_object(b"jkl", c);
    add_offset(obj_6, c);
    let obj_4 = c.pop_pack(false);
    start_object(b"ghi", c);
    add_offset(obj_4, c);
    let obj_3 = c.pop_pack(false);
    start_object(b"def", c);
    add_offset(obj_3, c);
    let obj_2 = c.pop_pack(false);
    start_object(b"abc", c);
    add_offset(obj_2, c);
    add_offset(obj_4, c);
    add_offset(obj_5, c);
    c.pop_pack(false);
}

fn populate_serializer_virtual_link(c: &mut Serializer) {
    let obj_d = add_object(b"d", c);
    start_object(b"b", c);
    add_offset(obj_d, c);
    let obj_b = c.pop_pack(false);
    start_object(b"e", c);
    add_virtual_offset(obj_b, c);
    let obj_e = c.pop_pack(false);
    start_object(b"c", c);
    add_offset(obj_e, c);
    let obj_c = c.pop_pack(false);
    start_object(b"a", c);
    add_offset(obj_b, c);
    add_offset(obj_c, c);
    c.pop_pack(false);
}

fn populate_serializer_with_24_and_32_bit_offsets(c: &mut Serializer) {
    let large = big(60000);
    let obj_f = add_object(b"f", c);
    let obj_g = add_object(b"g", c);
    let obj_j = add_object(b"j", c);
    let obj_k = add_object(b"k", c);
    start_object(&large[..40000], c);
    add_offset(obj_f, c);
    let obj_c = c.pop_pack(false);
    start_object(&large[..40000], c);
    add_offset(obj_g, c);
    let obj_d = c.pop_pack(false);
    start_object(&large[..40000], c);
    add_offset(obj_j, c);
    let obj_h = c.pop_pack(false);
    start_object(&large[..40000], c);
    add_offset(obj_k, c);
    let obj_i = c.pop_pack(false);
    start_object(b"e", c);
    add_wide_offset(obj_h, c);
    add_wide_offset(obj_i, c);
    let obj_e = c.pop_pack(false);
    start_object(b"b", c);
    add_24_offset(obj_c, c);
    add_24_offset(obj_d, c);
    add_24_offset(obj_e, c);
    let obj_b = c.pop_pack(false);
    start_object(b"a", c);
    add_24_offset(obj_b, c);
    c.pop_pack(false);
}

fn populate_serializer_with_extension_promotion(
    c: &mut Serializer,
    num_extensions: i32,
    shared_subtables: bool,
    version_1_2: bool,
) {
    const NUM_LOOKUPS: i32 = 5;
    const NUM_SUBTABLES: i32 = NUM_LOOKUPS * 2;
    let mut lookups = [0u32; NUM_LOOKUPS as usize];
    let mut subtables = [0u32; NUM_SUBTABLES as usize];
    let mut extensions = [0u32; NUM_SUBTABLES as usize];
    let large = big(60000);
    for i in (0..NUM_SUBTABLES).rev() {
        subtables[i as usize] = add_object(&large[..(15000 + i) as usize], c);
    }
    let mut i = NUM_SUBTABLES - 1;
    while i >= (NUM_LOOKUPS - num_extensions) * 2 {
        extensions[i as usize] = add_extension(subtables[i as usize], 5, c);
        i -= 1;
    }
    for i in (0..NUM_LOOKUPS).rev() {
        let is_ext = i >= NUM_LOOKUPS - num_extensions;
        start_lookup(
            if is_ext { 7 } else { 5 },
            if shared_subtables && i > 2 { 3 } else { 2 },
            c,
        );
        let t = if is_ext { &extensions } else { &subtables };
        if shared_subtables && i > 2 {
            add_offset(t[(i * 2 - 1) as usize], c);
        }
        add_offset(t[(i * 2) as usize], c);
        add_offset(t[(i * 2 + 1) as usize], c);
        lookups[i as usize] = finish_lookup(c);
    }
    let lookup_list = if version_1_2 {
        add_lookup_list_1_2(&lookups, c)
    } else {
        add_lookup_list(&lookups, c)
    };
    if version_1_2 {
        add_gsubgpos_header_1_2(lookup_list, c);
    } else {
        add_gsubgpos_header(lookup_list, c);
    }
}

fn populate_serializer_with_large_pair_pos_1(
    c: &mut Serializer,
    num_pair_pos_1: usize,
    num_pair_set: usize,
    as_extension: bool,
) {
    let large = big(60000);
    let total = num_pair_pos_1 * num_pair_set;
    let mut pair_set = vec![0u32; total];
    let mut pair_pos_1 = vec![0u32; num_pair_pos_1];
    for i in (0..num_pair_pos_1).rev() {
        for j in (i * num_pair_set..(i + 1) * num_pair_set).rev() {
            pair_set[j] = add_object(&large[..30000 + j], c);
        }
        let coverage = add_coverage(
            (i * num_pair_set) as u32,
            ((i + 1) * num_pair_set - 1) as u32,
            c,
            false,
        );
        pair_pos_1[i] = add_pair_pos_1(
            &pair_set[i * num_pair_set..(i + 1) * num_pair_set],
            coverage,
            c,
        );
    }
    let mut pair_pos_2 = add_object(&large[..200], c);
    if as_extension {
        pair_pos_2 = add_extension(pair_pos_2, 2, c);
        for i in (0..num_pair_pos_1).rev() {
            pair_pos_1[i] = add_extension(pair_pos_1[i], 2, c);
        }
    }
    start_lookup(
        if as_extension { 9 } else { 2 },
        1 + num_pair_pos_1 as u8,
        c,
    );
    for &p in &pair_pos_1 {
        add_offset(p, c);
    }
    add_offset(pair_pos_2, c);
    let lookup = finish_lookup(c);
    let lookup_list = add_lookup_list(&[lookup], c);
    add_gsubgpos_header(lookup_list, c);
}

fn populate_serializer_with_shared_pair_pos_1(
    c: &mut Serializer,
    num_pair_set: usize,
    num_shared: usize,
) {
    let mut pair_set = vec![0u32; num_shared];
    for i in (0..num_shared).rev() {
        pair_set[i] = add_object(&[0, 1, 0, i as u8], c);
    }
    let coverage = add_coverage(0, num_pair_set as u32 - 1, c, false);
    start_object(&[0, 1], c);
    add_offset(coverage, c);
    extend(
        &[0, 0, 0, 0, (num_pair_set >> 8) as u8, num_pair_set as u8],
        c,
    );
    for i in 0..num_pair_set {
        add_offset(pair_set[i % num_shared], c);
    }
    let pair_pos_1 = c.pop_pack(false);
    start_lookup(2, 1, c);
    add_offset(pair_pos_1, c);
    let lookup = finish_lookup(c);
    let lookup_list = add_lookup_list(&[lookup], c);
    add_gsubgpos_header(lookup_list, c);
}

fn populate_serializer_with_large_pair_pos_2(
    c: &mut Serializer,
    num_pair_pos_2: usize,
    num_class_1: usize,
    num_class_2: usize,
    as_extension: bool,
    with_device_tables: bool,
    extra_table: bool,
) {
    let large = big(100000);
    let mut class_def_1 = vec![0u32; num_pair_pos_2];
    let mut class_def_2 = vec![0u32; num_pair_pos_2];
    let mut pair_pos_2 = vec![0u32; num_pair_pos_2];
    let mut device_tables = vec![0u32; num_pair_pos_2 * num_class_1 * num_class_2];
    for i in (0..num_pair_pos_2).rev() {
        let start_glyph = 5 + i * num_class_1;
        if num_class_2 >= num_class_1 {
            class_def_2[i] = add_class_def(11, 10 + num_class_2 as u16, c);
            class_def_1[i] = add_class_def(
                (start_glyph + 1) as u16,
                (start_glyph + num_class_1) as u16,
                c,
            );
        } else {
            class_def_1[i] = add_class_def(
                (start_glyph + 1) as u16,
                (start_glyph + num_class_1) as u16,
                c,
            );
            class_def_2[i] = add_class_def(11, 10 + num_class_2 as u16, c);
        }
        let coverage = add_coverage(
            start_glyph as u32,
            (start_glyph + num_class_1 - 1) as u32,
            c,
            false,
        );
        if with_device_tables {
            for j in (i * num_class_1 * num_class_2..(i + 1) * num_class_1 * num_class_2).rev() {
                device_tables[j] = add_object(&[(j >> 8) as u8, j as u8], c);
            }
        }
        let n = num_class_1 * num_class_2;
        pair_pos_2[i] = add_pair_pos_2(
            (1 + i * num_class_1) as u32,
            coverage,
            class_def_1[i],
            num_class_1 as u16,
            class_def_2[i],
            num_class_2 as u16,
            with_device_tables.then(|| &device_tables[i * n..(i + 1) * n]),
            c,
        );
    }
    let mut pair_pos_1 = 0;
    if extra_table {
        pair_pos_1 = add_object(&large, c);
    }
    if as_extension {
        for i in (0..num_pair_pos_2).rev() {
            pair_pos_2[i] = add_extension(pair_pos_2[i], 2, c);
        }
        if extra_table {
            pair_pos_1 = add_extension(pair_pos_1, 2, c);
        }
    }
    start_lookup(
        if as_extension { 9 } else { 2 },
        1 + num_pair_pos_2 as u8,
        c,
    );
    if extra_table {
        add_offset(pair_pos_1, c);
    }
    for &p in &pair_pos_2 {
        add_offset(p, c);
    }
    let lookup = finish_lookup(c);
    let lookup_list = add_lookup_list(&[lookup], c);
    add_gsubgpos_header(lookup_list, c);
}

fn populate_serializer_with_large_mark_base_pos_1(
    c: &mut Serializer,
    mark_count: u32,
    class_count: u32,
    base_count: u32,
    table_count: u32,
) {
    let buffers = MarkBasePosBuffers::new(mark_count, class_count, base_count, table_count, c);
    let mut mark_base_pos: Vec<u32> = (0..table_count)
        .map(|i| buffers.create_mark_base_pos_1(i, c))
        .collect();
    for m in &mut mark_base_pos {
        *m = add_extension(*m, 4, c);
    }
    start_lookup(9, table_count as u8, c);
    for &m in &mark_base_pos {
        add_offset(m, c);
    }
    let lookup = finish_lookup(c);
    let lookup_list = add_lookup_list(&[lookup], c);
    add_gsubgpos_header(lookup_list, c);
}

#[allow(clippy::too_many_arguments)]
fn populate_serializer_with_large_ligsubst(
    c: &mut Serializer,
    lig_subst_count: u32,
    liga_set_count: u32,
    liga_per_set_count: u32,
    liga_size: usize,
    sequential_liga_sets: bool,
    shared: bool,
    liga_subst_id: &mut [u32],
    unique_lig_str: bool,
) {
    let mut ch = b'a';
    let mut liga = vec![0u32; (liga_set_count * liga_per_set_count) as usize];
    let mut liga_set = vec![0u32; liga_set_count as usize];
    for l in 0..lig_subst_count {
        let (cs, ce) = if sequential_liga_sets {
            (l * liga_set_count, (l + 1) * liga_set_count - 1)
        } else {
            (0, liga_set_count - 1)
        };
        let coverage = add_coverage(cs, ce, c, shared);
        for i in 0..liga_set_count {
            for j in 0..liga_per_set_count {
                start_object(&vec![ch; liga_size], c);
                if unique_lig_str {
                    ch = ch.wrapping_add(1);
                }
                add_virtual_offset(coverage, c);
                liga[(i * liga_per_set_count + j) as usize] = c.pop_pack(shared);
            }
            add_liga_set_header(liga_per_set_count as u8, c);
            add_virtual_offset(coverage, c);
            for j in 0..liga_per_set_count {
                add_offset(liga[(i * liga_per_set_count + j) as usize], c);
            }
            liga_set[i as usize] = c.pop_pack(shared);
        }
        add_liga_header(coverage, liga_set_count, c);
        for &s in &liga_set {
            add_offset(s, c);
        }
        liga_subst_id[l as usize] = c.pop_pack(shared);
    }
    for l in 0..lig_subst_count as usize {
        liga_subst_id[l] = add_extension(liga_subst_id[l], 4, c);
    }
}

fn populate_serializer_with_large_liga(
    c: &mut Serializer,
    liga_subst_count: u32,
    liga_set_count: u32,
    liga_per_set_count: u32,
    liga_size: usize,
    sequential_liga_sets: bool,
) {
    let mut liga_subst = vec![0u32; liga_subst_count as usize];
    populate_serializer_with_large_ligsubst(
        c,
        liga_subst_count,
        liga_set_count,
        liga_per_set_count,
        liga_size,
        sequential_liga_sets,
        false,
        &mut liga_subst,
        false,
    );
    start_lookup(7, liga_subst_count as u8, c);
    for &l in &liga_subst {
        add_offset(l, c);
    }
    let lookup = finish_lookup(c);
    let lookup_list = add_lookup_list(&[lookup], c);
    add_gsubgpos_header(lookup_list, c);
}

fn populate_serializer_with_large_liga_overlapping_clone_result(c: &mut Serializer) {
    let large = big(30000);
    let mut liga = [0u32; 6];
    let mut liga_subst = [0u32; 3];
    let mut liga_set = [0u32; 2];

    // LigaSubst 3
    let coverage = add_coverage(1, 1, c, false);
    for i in (0..2).rev() {
        start_object(&large, c);
        add_virtual_offset(coverage, c);
        liga[i] = c.pop_pack(false);
    }
    add_liga_set_header(2, c);
    add_virtual_offset(coverage, c);
    for &l in &liga[..2] {
        add_offset(l, c);
    }
    liga_set[0] = c.pop_pack(false);
    add_liga_header(coverage, 1, c);
    add_offset(liga_set[0], c);
    liga_subst[2] = c.pop_pack(false);

    // LigaSubst 2
    let coverage = add_coverage(0, 1, c, false);
    for i in (0..2).rev() {
        start_object(&large, c);
        add_virtual_offset(coverage, c);
        liga[i] = c.pop_pack(false);
    }
    add_liga_set_header(1, c);
    add_virtual_offset(coverage, c);
    add_offset(liga[1], c);
    liga_set[1] = c.pop_pack(false);
    add_liga_set_header(1, c);
    add_virtual_offset(coverage, c);
    add_offset(liga[0], c);
    liga_set[0] = c.pop_pack(false);
    add_liga_header(coverage, 2, c);
    add_offset(liga_set[0], c);
    add_offset(liga_set[1], c);
    liga_subst[1] = c.pop_pack(false);

    // LigaSubst 1
    let coverage = add_coverage(0, 0, c, false);
    for i in (0..2).rev() {
        start_object(&large, c);
        add_virtual_offset(coverage, c);
        liga[i] = c.pop_pack(false);
    }
    add_liga_set_header(2, c);
    add_virtual_offset(coverage, c);
    for &l in &liga[..2] {
        add_offset(l, c);
    }
    liga_set[0] = c.pop_pack(false);
    add_liga_header(coverage, 1, c);
    add_offset(liga_set[0], c);
    liga_subst[0] = c.pop_pack(false);

    for l in (0..3).rev() {
        liga_subst[l] = add_extension(liga_subst[l], 4, c);
    }
    start_lookup(7, 3, c);
    for &l in &liga_subst {
        add_offset(l, c);
    }
    let lookup = finish_lookup(c);
    let lookup_list = add_lookup_list(&[lookup], c);
    add_gsubgpos_header(lookup_list, c);
}

fn populate_serializer_with_large_liga_null_offsets(c: &mut Serializer, split: bool) {
    let large = big(30000);
    let mut liga = [0u32; 3];
    let mut liga_subst = [0u32; 2];
    let mut liga_set = [0u32; 2];
    let liga_subst_count = if split { 2 } else { 1 };
    if split {
        // LigaSubst 2
        let coverage = add_coverage(1, 1, c, false);
        start_object(&large, c);
        add_virtual_offset(coverage, c);
        liga[2] = c.pop_pack(false);
        add_liga_set_header(1, c);
        add_virtual_offset(coverage, c);
        add_offset(liga[2], c);
        liga_set[0] = c.pop_pack(false);
        add_liga_header(coverage, 1, c);
        add_offset(liga_set[0], c);
        liga_subst[1] = c.pop_pack(false);
    }
    // LigaSubst 1
    let coverage = add_coverage(0, 1, c, false);
    for i in (0..=if split { 1 } else { 2 }).rev() {
        start_object(&large, c);
        add_virtual_offset(coverage, c);
        liga[i] = c.pop_pack(false);
    }
    add_liga_set_header(if split { 1 } else { 2 }, c);
    add_virtual_offset(coverage, c);
    add_offset(liga[1], c);
    if !split {
        add_offset(liga[2], c);
    }
    liga_set[1] = c.pop_pack(false);
    // One liga followed by three null offsets.
    add_liga_set_header(4, c);
    add_virtual_offset(coverage, c);
    add_offset(liga[0], c);
    extend(&[0; 6], c);
    liga_set[0] = c.pop_pack(false);
    add_liga_header(coverage, 2, c);
    add_offset(liga_set[0], c);
    add_offset(liga_set[1], c);
    liga_subst[0] = c.pop_pack(false);
    for l in (0..liga_subst_count).rev() {
        liga_subst[l] = add_extension(liga_subst[l], 4, c);
    }
    start_lookup(7, liga_subst_count as u8, c);
    for &l in &liga_subst[..liga_subst_count] {
        add_offset(l, c);
    }
    let lookup = finish_lookup(c);
    let lookup_list = add_lookup_list(&[lookup], c);
    add_gsubgpos_header(lookup_list, c);
}

fn populate_serializer_with_shared_large_liga(
    c: &mut Serializer,
    liga_subst_count: u32,
    liga_set_count: u32,
    liga_per_set_count: u32,
    liga_size: usize,
    sequential_liga_sets: bool,
) {
    let mut lookups = [0u32; 2];
    let mut idx = vec![0u32; liga_subst_count as usize];
    populate_serializer_with_large_ligsubst(
        c,
        liga_subst_count,
        liga_set_count,
        liga_per_set_count,
        liga_size,
        sequential_liga_sets,
        true,
        &mut idx,
        true,
    );
    start_lookup(7, liga_subst_count as u8, c);
    for &l in &idx {
        add_offset(l, c);
    }
    lookups[0] = finish_lookup(c);

    let mut liga_subst = vec![0u32; liga_subst_count as usize + 1];
    populate_serializer_with_large_ligsubst(
        c,
        1,
        1,
        1,
        10,
        sequential_liga_sets,
        false,
        &mut liga_subst[..1],
        false,
    );
    populate_serializer_with_large_ligsubst(
        c,
        liga_subst_count,
        liga_set_count,
        liga_per_set_count,
        liga_size,
        sequential_liga_sets,
        true,
        &mut liga_subst[1..],
        true,
    );
    start_lookup(7, liga_subst_count as u8 + 1, c);
    for &l in &liga_subst {
        add_offset(l, c);
    }
    lookups[1] = finish_lookup(c);
    let lookup_list = add_lookup_list(&lookups, c);
    add_gsubgpos_header(lookup_list, c);
}

fn populate_serializer_with_liga_shared_coverage(
    c: &mut Serializer,
    liga_subst_count: u32,
    liga_set_count: u32,
    liga_per_set_count: u32,
    liga_size: usize,
) {
    let mut liga_subst = vec![0u32; liga_subst_count as usize + 1];
    populate_serializer_with_large_ligsubst(c, 1, 6, 2, 10, true, true, &mut liga_subst[..1], true);
    populate_serializer_with_large_ligsubst(
        c,
        liga_subst_count,
        liga_set_count,
        liga_per_set_count,
        liga_size,
        true,
        true,
        &mut liga_subst[1..],
        true,
    );
    start_lookup(7, liga_subst_count as u8 + 1, c);
    for &l in &liga_subst {
        add_offset(l, c);
    }
    let lookup = finish_lookup(c);
    let lookup_list = add_lookup_list(&[lookup], c);
    add_gsubgpos_header(lookup_list, c);
}

fn ser(f: impl FnOnce(&mut Serializer)) -> Serializer {
    let mut c = Serializer::new();
    f(&mut c);
    c
}

// --- test-repacker.cc -----------------------------------------------------

#[test]
fn test_sort_shortest() {
    let mut g = graph(ser(populate_serializer_complex_2));
    assert!(g.sort_shortest_distance().is_ok());
    g.normalize();
    let mut e = ser(|e| {
        let jkl = add_object(b"jkl", e);
        start_object(b"ghi", e);
        add_offset(jkl, e);
        let ghi = e.pop_pack(false);
        start_object(b"def", e);
        add_offset(ghi, e);
        let def = e.pop_pack(false);
        let mn = add_object(b"mn", e);
        start_object(b"abc", e);
        add_offset(def, e);
        add_offset(jkl, e);
        add_offset(mn, e);
        e.pop_pack(false);
    });
    let mut expected = graph(core::mem::take(&mut e));
    expected.normalize();
    assert!(expected.graph_equals(&g));
}

#[test]
fn test_duplicate_leaf() {
    let mut g = graph(ser(populate_serializer_complex_2));
    assert!(g.duplicate_child(4, 1, false).is_ok());
    g.normalize();
    let e = ser(|e| {
        let mn = add_object(b"mn", e);
        let jkl_2 = add_object(b"jkl", e);
        start_object(b"ghi", e);
        add_offset(jkl_2, e);
        let ghi = e.pop_pack(false);
        start_object(b"def", e);
        add_offset(ghi, e);
        let def = e.pop_pack(false);
        let jkl_1 = add_object(b"jkl", e);
        start_object(b"abc", e);
        add_offset(def, e);
        add_offset(jkl_1, e);
        add_offset(mn, e);
        e.pop_pack(false);
    });
    let mut expected = graph(e);
    expected.normalize();
    assert!(expected.graph_equals(&g));
}

#[test]
fn test_duplicate_interior() {
    let mut g = graph(ser(populate_serializer_complex_3));
    assert!(g.duplicate_child(3, 2, false).is_ok());
    let obj = |g: &Graph, i: u32| {
        (
            g.bytes(i).to_vec(),
            g.v(i)
                .unwrap()
                .real_links
                .iter()
                .map(|l| l.objidx)
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(&obj(&g, 6).0[..3], b"jkl");
    assert_eq!(obj(&g, 6).1, [0]);
    assert_eq!(&obj(&g, 5).0[..3], b"abc");
    assert_eq!(obj(&g, 5).1, [4, 2, 1]);
    assert_eq!(&obj(&g, 4).0[..3], b"def");
    assert_eq!(obj(&g, 4).1, [3]);
    assert_eq!(&obj(&g, 3).0[..3], b"ghi");
    assert_eq!(obj(&g, 3).1, [6]);
    assert_eq!(&obj(&g, 2).0[..3], b"jkl");
    assert_eq!(obj(&g, 2).1, [0]);
    assert_eq!(&obj(&g, 1).0[..2], b"mn");
    assert!(obj(&g, 1).1.is_empty());
    assert_eq!(&obj(&g, 0).0[..6], b"opqrst");
    assert!(obj(&g, 0).1.is_empty());
}

#[test]
fn test_serialize() {
    let c = ser(populate_serializer_simple);
    let packed = c.end().unwrap();
    let expected = packed.resolve().unwrap();
    let g = Graph::create(&with_resolved_offsets(packed.objects())).unwrap();
    assert_eq!(serialize(&g).unwrap(), expected);
}

#[test]
fn test_will_overflow_1() {
    let mut g = graph(ser(populate_serializer_complex_2));
    assert!(!will_overflow(&mut g, None).unwrap());
}

#[test]
fn test_will_overflow_2() {
    let mut g = graph(ser(populate_serializer_with_overflow));
    assert!(will_overflow(&mut g, None).unwrap());
}

#[test]
fn test_will_overflow_3() {
    let mut g = graph(ser(populate_serializer_with_dedup_overflow));
    assert!(will_overflow(&mut g, None).unwrap());
}

#[test]
fn test_will_overflow_records() {
    let c = ser(|c| {
        let obj_leaf = add_object(b"d", c);
        let obj_large = add_object(&big(70000), c);
        start_object(b"c", c);
        add_offset(obj_leaf, c);
        add_offset(obj_leaf, c);
        let obj_c = c.pop_pack(false);
        start_object(b"b", c);
        add_offset(obj_leaf, c);
        add_offset(obj_leaf, c);
        let obj_b = c.pop_pack(false);
        start_object(b"a", c);
        add_offset(obj_b, c);
        add_offset(obj_c, c);
        add_offset(obj_large, c);
        c.pop_pack(false);
    });
    let mut g = graph(c);
    let mut overflows = Vec::new();
    assert!(will_overflow(&mut g, Some(&mut overflows)).unwrap());
    assert_eq!(overflows.len(), 2);
    assert_ne!(overflows[0].parent, overflows[1].parent);
    assert_eq!(overflows[0].child, overflows[1].child);
}

fn resolve(c: Serializer, tag: u32, max_rounds: u32) -> Result<Vec<u8>, GraphError> {
    resolve_overflows(&finish(c).0, tag, max_rounds, false)
}

#[test]
fn test_resolve_overflows_via_sort() {
    let out = resolve(ser(populate_serializer_with_overflow), NONE, 32).unwrap();
    assert_eq!(out.len(), 80000 + 3 + 3 * 2);
}

#[test]
fn test_resolve_overflows_via_duplication() {
    let out = resolve(ser(populate_serializer_with_dedup_overflow), NONE, 32).unwrap();
    assert_eq!(out.len(), 10000 + 2 * 2 + 60000 + 2 + 3 * 2);
}

#[test]
fn test_resolve_overflows_via_multiple_duplications() {
    assert!(
        resolve(
            ser(populate_serializer_with_multiple_dedup_overflow),
            NONE,
            5
        )
        .is_ok()
    );
}

#[test]
fn test_resolve_overflows_via_space_assignment() {
    run_resolve_overflow_test(
        "test_resolve_overflows_via_space_assignment",
        ser(|c| populate_serializer_spaces(c, true)),
        ser(|c| populate_serializer_spaces(c, false)),
        0,
        false,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_overflows_via_isolation() {
    let (objs, overflow) = finish(ser(populate_serializer_with_isolation_overflow));
    assert!(overflow);
    let out = resolve_overflows(&objs, GSUB, 0, false).unwrap();
    assert_eq!(out.len(), 1 + 10000 + 60000 + 1 + 1 + 4 + 3 * 2);
}

#[test]
fn test_resolve_overflows_via_isolation_with_recursive_duplication() {
    run_resolve_overflow_test(
        "test_resolve_overflows_via_isolation_with_recursive_duplication",
        ser(populate_serializer_with_isolation_overflow_complex),
        ser(populate_serializer_with_isolation_overflow_complex_expected),
        0,
        false,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_overflows_via_isolating_16bit_space() {
    run_resolve_overflow_test(
        "test_resolve_overflows_via_isolating_16bit_space",
        ser(populate_serializer_spaces_16bit_connection),
        ser(populate_serializer_spaces_16bit_connection_expected),
        0,
        false,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_overflows_via_isolating_16bit_space_2() {
    run_resolve_overflow_test(
        "test_resolve_overflows_via_isolating_16bit_space_2",
        ser(populate_serializer_short_and_wide_subgraph_root),
        ser(populate_serializer_short_and_wide_subgraph_root_expected),
        0,
        false,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_overflows_via_isolation_spaces() {
    let (objs, overflow) = finish(ser(populate_serializer_with_isolation_overflow_spaces));
    assert!(overflow);
    let out = resolve_overflows(&objs, GSUB, 0, false).unwrap();
    assert_eq!(out.len(), 3 + 2 * 60000 + 2 * 4 + 2 * 2);
}

#[test]
fn test_resolve_mixed_overflows_via_isolation_spaces() {
    let (objs, overflow) = finish(ser(populate_serializer_with_24_and_32_bit_offsets));
    assert!(overflow);
    let out = resolve_overflows(&objs, GSUB, 0, false).unwrap();
    assert_eq!(out.len(), 7 + 4 * 40000 + 2 * 4 + 4 * 3 + 4 * 2);
}

#[test]
fn test_resolve_with_extension_promotion() {
    run_resolve_overflow_test(
        "test_resolve_with_extension_promotion",
        ser(|c| populate_serializer_with_extension_promotion(c, 0, false, false)),
        ser(|c| populate_serializer_with_extension_promotion(c, 3, false, false)),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_with_shared_extension_promotion() {
    run_resolve_overflow_test(
        "test_resolve_with_shared_extension_promotion",
        ser(|c| populate_serializer_with_extension_promotion(c, 0, true, false)),
        ser(|c| populate_serializer_with_extension_promotion(c, 3, true, false)),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_with_extension_promotion_1_2() {
    run_resolve_overflow_test(
        "test_resolve_with_extension_promotion_1_2",
        ser(|c| populate_serializer_with_extension_promotion(c, 0, false, true)),
        ser(|c| populate_serializer_with_extension_promotion(c, 3, false, true)),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_with_repeated_extension_promotion() {
    let c = ser(|c| {
        let subtable = add_object(b"subt", c);
        start_lookup(5, 2, c);
        add_offset(subtable, c);
        add_offset(subtable, c);
        let lookup = finish_lookup(c);
        let lookup_list = add_lookup_list(&[lookup], c);
        add_gsubgpos_header(lookup_list, c);
    });
    let mut g = graph(c);
    let mut context = Context::create(GSUB, &g).unwrap();
    assert_eq!(context.lookups.len(), 1);
    let lookup_index = *context.lookups.first().unwrap();
    assert!(context.make_extension(&mut g, lookup_index).is_ok());
    let lv = g.v(lookup_index).unwrap();
    assert_eq!(lv.real_links.len(), 2);
    let extension_index = lv.real_links[0].objidx;
    assert_eq!(lv.real_links[1].objidx, extension_index);
    let promoted = g.index_for_offset(extension_index, 4).unwrap();
    assert_eq!(g.table_size(promoted), 4);
    assert!(g.update_parents().is_ok());
    assert_eq!(g.v(extension_index).unwrap().incoming_edges(), 2);
    assert_eq!(g.v(promoted).unwrap().incoming_edges(), 1);
    assert!(serialize(&g).is_ok());
}

#[test]
fn test_resolve_with_basic_pair_pos_1_split() {
    run_resolve_overflow_test(
        "test_resolve_with_basic_pair_pos_1_split",
        ser(|c| populate_serializer_with_large_pair_pos_1(c, 1, 4, false)),
        ser(|c| populate_serializer_with_large_pair_pos_1(c, 2, 2, true)),
        20,
        true,
        GPOS,
        false,
    );
}

#[test]
fn test_resolve_with_extension_pair_pos_1_split() {
    run_resolve_overflow_test(
        "test_resolve_with_extension_pair_pos_1_split",
        ser(|c| populate_serializer_with_large_pair_pos_1(c, 1, 4, true)),
        ser(|c| populate_serializer_with_large_pair_pos_1(c, 2, 2, true)),
        20,
        true,
        GPOS,
        false,
    );
}

#[test]
fn test_resolve_with_shared_pair_set_pair_pos_1_split() {
    const NUM_PAIR_SET: usize = 40000;
    const NUM_SHARED: usize = 4;
    let mut g = graph(ser(|c| {
        populate_serializer_with_shared_pair_pos_1(c, NUM_PAIR_SET, NUM_SHARED)
    }));
    let mut context = Context::create(GPOS, &g).unwrap();
    assert_eq!(context.lookups.len(), 1);
    let lookup_index = *context.lookups.first().unwrap();
    assert!(
        context
            .split_subtables_if_needed(&mut g, lookup_index)
            .is_ok()
    );

    let subtable_count = g.rd16(lookup_index, 4);
    assert!(subtable_count > 1);
    assert_eq!(
        g.v(lookup_index).unwrap().real_links.len(),
        subtable_count as usize
    );
    let subtables = g.v(lookup_index).unwrap().position_to_index_map();
    let mut pair_set_count = 0usize;
    for i in 0..subtable_count {
        let subtable_index = subtables[&(6 + 2 * i)];
        let st = g.bytes(subtable_index).to_vec();
        assert!(st.len() >= 10);
        assert_eq!(u16::from_be_bytes([st[0], st[1]]), 1);
        let count = u16::from_be_bytes([st[8], st[9]]) as usize;
        assert!(count > 0);
        assert_eq!(st.len(), 10 + 2 * count);
        let sv = g.v(subtable_index).unwrap();
        assert_eq!(sv.real_links.len(), count + 1);
        let pair_sets = sv.position_to_index_map();
        assert!(pair_sets.contains_key(&2));
        for j in 0..count {
            let ps = pair_sets[&(10 + 2 * j as u32)];
            let b = g.bytes(ps);
            assert_eq!(b.len(), 4);
            assert_eq!(usize::from(b[3]), (pair_set_count + j) % NUM_SHARED);
        }
        pair_set_count += count;
    }
    assert_eq!(pair_set_count, NUM_PAIR_SET);
}

#[test]
fn test_resolve_with_basic_pair_pos_2_split() {
    run_resolve_overflow_test(
        "test_resolve_with_basic_pair_pos_2_split",
        ser(|c| populate_serializer_with_large_pair_pos_2(c, 1, 4, 3000, false, false, true)),
        ser(|c| populate_serializer_with_large_pair_pos_2(c, 2, 2, 3000, true, false, true)),
        20,
        true,
        GPOS,
        false,
    );
}

#[test]
fn test_resolve_with_close_to_limit_pair_pos_2_split() {
    run_resolve_overflow_test(
        "test_resolve_with_close_to_limit_pair_pos_2_split",
        ser(|c| populate_serializer_with_large_pair_pos_2(c, 1, 1636, 10, true, false, false)),
        ser(|c| populate_serializer_with_large_pair_pos_2(c, 2, 818, 10, true, false, false)),
        20,
        true,
        GPOS,
        false,
    );
}

#[test]
fn test_resolve_with_pair_pos_2_split_with_device_tables() {
    run_resolve_overflow_test(
        "test_resolve_with_pair_pos_2_split_with_device_tables",
        ser(|c| populate_serializer_with_large_pair_pos_2(c, 1, 4, 2000, false, true, true)),
        ser(|c| populate_serializer_with_large_pair_pos_2(c, 2, 2, 2000, true, true, true)),
        20,
        true,
        GPOS,
        false,
    );
}

#[test]
fn test_resolve_with_basic_mark_base_pos_1_split() {
    run_resolve_overflow_test(
        "test_resolve_with_basic_mark_base_pos_1_split",
        ser(|c| populate_serializer_with_large_mark_base_pos_1(c, 40, 10, 110, 1)),
        ser(|c| populate_serializer_with_large_mark_base_pos_1(c, 40, 10, 110, 2)),
        20,
        true,
        GPOS,
        false,
    );
}

#[test]
fn test_resolve_with_basic_liga_split() {
    run_resolve_overflow_test(
        "test_resolve_with_basic_liga_split",
        ser(|c| populate_serializer_with_large_liga(c, 1, 1, 2, 40000, false)),
        ser(|c| populate_serializer_with_large_liga(c, 2, 1, 1, 40000, false)),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_with_liga_split_move() {
    run_resolve_overflow_test(
        "test_resolve_with_liga_split_move",
        ser(|c| populate_serializer_with_large_liga(c, 1, 6, 2, 16000, true)),
        ser(|c| populate_serializer_with_large_liga(c, 3, 2, 2, 16000, true)),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_with_liga_split_overlapping_clone() {
    run_resolve_overflow_test(
        "test_resolve_with_liga_split_overlapping_clone",
        ser(|c| populate_serializer_with_large_liga(c, 1, 2, 3, 30000, true)),
        ser(populate_serializer_with_large_liga_overlapping_clone_result),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_with_liga_split_null_offsets() {
    run_resolve_overflow_test(
        "test_resolve_with_liga_split_null_offsets",
        ser(|c| populate_serializer_with_large_liga_null_offsets(c, false)),
        ser(|c| populate_serializer_with_large_liga_null_offsets(c, true)),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_with_liga_split_shared_table() {
    run_resolve_overflow_test(
        "test_resolve_with_liga_split_shared_table",
        ser(|c| populate_serializer_with_shared_large_liga(c, 1, 6, 2, 16000, true)),
        ser(|c| populate_serializer_with_shared_large_liga(c, 3, 2, 2, 16000, true)),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_with_liga_split_shared_coverage() {
    run_resolve_overflow_test(
        "test_resolve_with_liga_split_shared_coverage",
        ser(|c| populate_serializer_with_liga_shared_coverage(c, 1, 6, 2, 16000)),
        ser(|c| populate_serializer_with_liga_shared_coverage(c, 3, 2, 2, 16000)),
        20,
        true,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_overflows_via_splitting_spaces() {
    run_resolve_overflow_test(
        "test_resolve_overflows_via_splitting_spaces",
        ser(populate_serializer_with_split_spaces),
        ser(populate_serializer_with_split_spaces_expected),
        1,
        false,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_overflows_via_splitting_spaces_2() {
    run_resolve_overflow_test(
        "test_resolve_overflows_via_splitting_spaces_2",
        ser(populate_serializer_with_split_spaces_2),
        ser(populate_serializer_with_split_spaces_expected_2),
        1,
        false,
        GSUB,
        false,
    );
}

#[test]
fn test_resolve_overflows_via_priority() {
    run_resolve_overflow_test(
        "test_resolve_overflows_via_priority",
        ser(populate_serializer_with_priority_overflow),
        ser(populate_serializer_with_priority_overflow_expected),
        3,
        false,
        GSUB,
        false,
    );
}

#[test]
fn test_unresolvable_priority_overflow() {
    assert!(
        resolve(
            ser(populate_serializer_with_unresolvable_priority_overflow),
            NONE,
            32
        )
        .is_err()
    );
}

#[test]
fn test_virtual_link() {
    let out = resolve(ser(populate_serializer_virtual_link), NONE, 32).unwrap();
    assert_eq!(out.len(), 5 + 4 * 2);
    assert_eq!(out[0], b'a');
    assert_eq!(out[5], b'c');
    assert_eq!(out[8], b'e');
    assert_eq!(out[9], b'b');
    assert_eq!(out[12], b'd');
}

#[test]
fn test_shared_node_with_virtual_links() {
    let mut c = Serializer::new();
    let obj_b = add_object(b"b", &mut c);
    let obj_c = add_object(b"c", &mut c);
    start_object(b"d", &mut c);
    add_virtual_offset(obj_b, &mut c);
    let obj_d_1 = c.pop_pack(true);
    start_object(b"d", &mut c);
    add_virtual_offset(obj_c, &mut c);
    let obj_d_2 = c.pop_pack(true);
    assert_eq!(obj_d_1, obj_d_2);
    start_object(b"a", &mut c);
    add_offset(obj_b, &mut c);
    add_offset(obj_c, &mut c);
    add_offset(obj_d_1, &mut c);
    add_offset(obj_d_2, &mut c);
    c.pop_pack(true);
    let packed = c.end().unwrap();
    let v = &packed.objects()[obj_d_1 as usize].virtual_links;
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].objidx, obj_b);
    assert_eq!(v[1].objidx, obj_c);
}

#[test]
fn test_repack_last() {
    run_resolve_overflow_test(
        "test_repack_last",
        ser(|c| populate_serializer_with_repack_last(c, true)),
        ser(|c| populate_serializer_with_repack_last(c, false)),
        20,
        false,
        ABCD,
        true,
    );
}

#[test]
fn test_dont_duplicate_virtual() {
    run_resolve_overflow_test(
        "test_dont_duplicate_virtual",
        ser(|c| populate_serializer_virtual(c, true)),
        ser(|c| populate_serializer_virtual(c, false)),
        20,
        false,
        ABCD,
        true,
    );
}

#[test]
fn test_deep_graph_traversal() {
    let c = ser(|c| {
        let mut prev = add_object(b"leaf", c);
        for _ in 0..2000 {
            start_object(b"node", c);
            add_offset(prev, c);
            prev = c.pop_pack(false);
        }
    });
    let mut g = graph(c);
    let root = g.root_idx();
    let mut visited = HbSet::new();
    assert!(g.find_subgraph_set(root, &mut visited).is_ok());
    assert_eq!(visited.population(), 2001);
    let mut size_set = HbSet::new();
    assert_eq!(
        g.find_subgraph_size(root, &mut size_set, u32::MAX).unwrap(),
        4 + 2000 * 6
    );
    let mut map = super::hbmap::HbMap::new();
    map.set(root, 1);
    assert!(g.find_subgraph_map(root, &mut map).is_ok());
    assert_eq!(map.population(), 2001);
    assert!(g.assign_spaces().is_ok());
    let mut index_map = super::hbmap::HbMap::new();
    assert!(g.duplicate_subgraph(root, &mut index_map).is_ok());
    assert_eq!(index_map.population(), 2001);
}

#[test]
fn test_duplicate_subgraph_vertex_limit() {
    let c = ser(|c| {
        let leaf = add_object(b"leaf", c);
        start_object(b"root", c);
        add_offset(leaf, c);
        c.pop_pack(false);
    });
    let mut g = graph(c);
    while g.len() < MAX_VERTICES - 1 {
        assert!(g.new_node(None).is_ok());
    }
    let mut index_map = super::hbmap::HbMap::new();
    let root = g.root_idx();
    assert_eq!(
        g.duplicate_subgraph(root, &mut index_map),
        Err(GraphError::LimitExceeded)
    );
}

#[test]
fn test_resolve_shared_overflow_vertex_limit() {
    let c = ser(|c| {
        let leaf = add_object(b"leaf", c);
        start_object(b"a\0\0\0", c);
        add_offset(leaf, c);
        let a = c.pop_pack(false);
        start_object(b"b\0\0\0", c);
        add_offset(leaf, c);
        let b = c.pop_pack(false);
        start_object(b"root", c);
        add_offset(a, c);
        add_offset(b, c);
        c.pop_pack(false);
    });
    let mut g = graph(c);
    while g.len() < MAX_VERTICES {
        assert!(g.new_node(None).is_ok());
    }
    let overflows = [Overflow {
        parent: 3,
        child: 1,
    }];
    assert_eq!(resolve_shared_overflow(&overflows, 0, &mut g), Ok(false));
    let overflows = [Overflow {
        parent: 1,
        child: 0,
    }];
    assert_eq!(
        resolve_shared_overflow(&overflows, 0, &mut g),
        Err(GraphError::LimitExceeded)
    );
}

#[test]
fn test_32bit_roots_traversal() {
    let (mut g, leaf1, leaf2, mid) = {
        let mut c = Serializer::new();
        let leaf1 = add_object(b"l1", &mut c);
        let leaf2 = add_object(b"l2", &mut c);
        start_object(b"mid", &mut c);
        add_wide_offset(leaf1, &mut c);
        let mid = c.pop_pack(false);
        start_object(b"root", &mut c);
        add_offset(mid, &mut c);
        add_wide_offset(leaf2, &mut c);
        c.pop_pack(false);
        (graph(c), leaf1, leaf2, mid)
    };
    let mut roots = HbSet::new();
    let root = g.root_idx();
    assert!(g.find_32bit_roots(root, &mut roots).is_ok());
    assert!(roots.has(leaf1 - 1));
    assert!(roots.has(leaf2 - 1));
    assert!(!roots.has(mid - 1));
    assert_eq!(roots.population(), 2);

    // A DFS would wrongly take b as a root; the BFS finds a first.
    let mut c2 = Serializer::new();
    let b = add_object(b"b", &mut c2);
    start_object(b"a", &mut c2);
    add_wide_offset(b, &mut c2);
    let a = c2.pop_pack(false);
    start_object(b"r", &mut c2);
    add_offset(a, &mut c2);
    add_wide_offset(a, &mut c2);
    c2.pop_pack(false);
    let mut g2 = graph(c2);
    let mut roots2 = HbSet::new();
    let root2 = g2.root_idx();
    assert!(g2.find_32bit_roots(root2, &mut roots2).is_ok());
    assert_eq!(roots2.population(), 1);
    assert!(roots2.has(a - 1));
    assert!(!roots2.has(b - 1));
}

#[test]
fn test_isolate_subgraph_shared_wide_parents() {
    const NUM_PARENTS: usize = 3;
    const NUM_LEAVES: usize = 3;
    let mut c = Serializer::new();
    let leaves: Vec<u32> = (0..NUM_LEAVES)
        .map(|_| add_object(b"leaf", &mut c))
        .collect();
    let parents: Vec<u32> = (0..NUM_PARENTS)
        .map(|_| {
            start_object(b"parent", &mut c);
            for &l in &leaves {
                add_wide_offset(l, &mut c);
            }
            c.pop_pack(false)
        })
        .collect();
    start_object(b"root", &mut c);
    for &p in &parents {
        add_offset(p, &mut c);
    }
    add_offset(leaves[0], &mut c);
    c.pop_pack(false);
    let mut g = graph(c);
    let num_vertices = g.len() as u32;
    assert_eq!(num_vertices as usize, 1 + NUM_PARENTS + NUM_LEAVES);
    let mut roots: HbSet = leaves.iter().map(|l| l - 1).collect();
    assert!(g.isolate_subgraph(&mut roots).unwrap());
    assert_eq!(g.len() as u32, num_vertices + 1);
    assert_eq!(roots.population() as usize, NUM_LEAVES);
    assert!(!roots.has(leaves[0] - 1));
    assert!(roots.has(num_vertices));
    for &l in &leaves[1..] {
        assert!(roots.has(l - 1));
    }
    assert!(g.update_parents().is_ok());
    assert_eq!(g.v(leaves[0] - 1).unwrap().incoming_edges(), 1);
    assert_eq!(
        g.v(num_vertices).unwrap().incoming_edges() as usize,
        NUM_PARENTS
    );
    for &l in &leaves[1..] {
        assert_eq!(g.v(l - 1).unwrap().incoming_edges() as usize, NUM_PARENTS);
    }
    assert!(!g.isolate_subgraph(&mut roots).unwrap());
    assert_eq!(g.len() as u32, num_vertices + 1);
}

#[test]
fn test_invalid_link_objidx() {
    let mut g = graph(ser(populate_serializer_complex_2));
    let root = g.root_idx();
    g.vm(root).unwrap().real_links[0].objidx = u32::MAX;
    assert!(g.sort_shortest_distance().is_err());
}

#[test]
fn test_invalid_link_add_and_move() {
    let mut g = graph(ser(populate_serializer_complex_2));
    assert!(g.add_link(0, 0, u32::MAX).is_err());
    // A position outside vertex 0 stands for a pointer outside it.
    assert!(g.move_child(0, u32::MAX, 1, u32::MAX, 2).is_err());
}

#[test]
fn test_anchor_matrix_sanitize_size_overflow() {
    // 2 * 46341 * 46341 wraps around to 9266 in 32-bit arithmetic.
    let count = 46341u32;
    let len = 2 + 2 * count as usize;
    assert!(markbasepos::anchor_matrix_sanitize(len, count, 1).is_ok());
    assert!(markbasepos::anchor_matrix_sanitize(len, count, 2).is_err());
    assert!(markbasepos::anchor_matrix_sanitize(len, count, count).is_err());
    assert!(markbasepos::anchor_matrix_sanitize(len, 0, count).is_ok());
    assert!(markbasepos::anchor_matrix_sanitize(1, 0, count).is_err());
}

#[test]
fn test_pair_pos_format_2_sanitize_size_overflow() {
    // 32769 * 65535 * 4 wraps around to 131068 in 32-bit arithmetic.
    let min = 16usize;
    assert!(pairpos::format2_sanitize(min + 131_068, 32769, 65535, 1, 1).is_err());
    assert!(pairpos::format2_sanitize(min + 3 * 2 * 4, 3, 2, 1, 1).is_ok());
    assert!(pairpos::format2_sanitize(min + 3 * 2 * 4 - 1, 3, 2, 1, 1).is_err());
    assert!(pairpos::format2_sanitize(min, 0, 2, 1, 1).is_ok());
    assert!(pairpos::format2_sanitize(min - 1, 0, 2, 1, 1).is_err());
}

#[test]
fn test_invalid_link_position() {
    let packed = ser(populate_serializer_simple).end().unwrap();
    let mut objs = with_resolved_offsets(packed.objects());
    let mut g = Graph::create(&objs).unwrap();
    let root = g.root_idx();
    g.vm(root).unwrap().real_links[0].position = u32::MAX;
    assert!(serialize(&g).is_err());
    objs.last_mut().unwrap().real_links[0].position = u32::MAX;
    assert!(Graph::create(&objs).is_err());
}

// --- test-classdef-graph.cc ----------------------------------------------

fn actual_class_def_size(pairs: &[(u32, u32)]) -> u32 {
    class_def_bytes(pairs, 100).unwrap().len() as u32
}

fn actual_coverage_size(glyphs: &[u32]) -> u32 {
    coverage_bytes(glyphs, 100).unwrap().len() as u32
}

fn filtered(map: &[(u32, u32)], classes: &[u32]) -> Vec<(u32, u32)> {
    map.iter()
        .copied()
        .filter(|p| classes.contains(&p.1))
        .collect()
}

fn check_add_class_def_size_running(
    estimator: &mut ClassDefSizeEstimator,
    map: &[(u32, u32)],
    klass: u32,
    klasses: &[u32],
) -> bool {
    let result = estimator.add_class_def_size(klass);
    let f = filtered(map, klasses);
    if result != actual_class_def_size(&f) {
        return false;
    }
    let glyphs: Vec<u32> = f.iter().map(|p| p.0).collect();
    estimator.coverage_size() == actual_coverage_size(&glyphs)
}

fn check_add_class_def_size(list: &[(u32, u32)], klass: u32) -> bool {
    let mut estimator = ClassDefSizeEstimator::new(list.iter().copied());
    let result = estimator.add_class_def_size(klass);
    let f = filtered(list, &[klass]);
    if result != actual_class_def_size(&f) {
        return false;
    }
    let glyphs: Vec<u32> = f.iter().map(|p| p.0).collect();
    estimator.coverage_size() == actual_coverage_size(&glyphs)
}

#[test]
fn test_class_and_coverage_size_estimates() {
    let empty: [(u32, u32); 0] = [];
    assert!(check_add_class_def_size(&empty, 0));
    assert!(check_add_class_def_size(&empty, 1));
    let class_zero = [(5, 0)];
    assert!(check_add_class_def_size(&class_zero, 0));
    let consecutive = [
        (4, 0),
        (5, 0),
        (6, 1),
        (7, 1),
        (8, 2),
        (9, 2),
        (10, 2),
        (11, 2),
    ];
    for k in 0..3 {
        assert!(check_add_class_def_size(&consecutive, k));
    }
    let non_consecutive = [
        (4, 0),
        (6, 0),
        (8, 1),
        (10, 1),
        (9, 2),
        (10, 2),
        (11, 2),
        (13, 2),
    ];
    for k in 0..3 {
        assert!(check_add_class_def_size(&non_consecutive, k));
    }
    let multiple_ranges = [
        (4, 0),
        (5, 0),
        (6, 1),
        (7, 1),
        (9, 1),
        (11, 1),
        (12, 1),
        (13, 1),
    ];
    assert!(check_add_class_def_size(&multiple_ranges, 0));
    assert!(check_add_class_def_size(&multiple_ranges, 1));
    assert!(check_add_class_def_size(&[(0x10000, 1), (0x10001, 1)], 1));
    assert!(check_add_class_def_size(&[(0x10000, 1), (0x10002, 1)], 1));
    assert!(check_add_class_def_size(&[(5, 0x10000)], 0x10000));
}

#[test]
fn test_class_def_format_3_values() {
    let data = [
        0x00, 0x03, 0x00, 0x00, 0x05, 0x00, 0x00, 0x02, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00,
    ];
    assert_eq!(class_of(&data, 5), 1);
    assert_eq!(class_of(&data, 6), 0x10000);
    // HarfBuzz also checks ClassDef::intersects_class here, which the
    // repacker does not use (layout subsetting, ADR 0022 stage 3).
}

#[test]
fn test_running_class_and_coverage_size_estimates() {
    let consecutive_map = [
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 2),
        (6, 3),
        (7, 3),
        (8, 3),
        (9, 3),
        (10, 3),
        (11, 3),
        (12, 3),
    ];
    let mut e1 = ClassDefSizeEstimator::new(consecutive_map.iter().copied());
    assert!(check_add_class_def_size_running(
        &mut e1,
        &consecutive_map,
        1,
        &[1]
    ));
    assert!(check_add_class_def_size_running(
        &mut e1,
        &consecutive_map,
        2,
        &[1, 2]
    ));
    assert!(check_add_class_def_size_running(
        &mut e1,
        &consecutive_map,
        2,
        &[1, 2]
    ));
    assert!(check_add_class_def_size_running(
        &mut e1,
        &consecutive_map,
        3,
        &[1, 2, 3]
    ));
    e1.reset();
    assert!(check_add_class_def_size_running(
        &mut e1,
        &consecutive_map,
        2,
        &[2]
    ));
    assert!(check_add_class_def_size_running(
        &mut e1,
        &consecutive_map,
        3,
        &[2, 3]
    ));

    let non_consecutive_map = [
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (6, 2),
        (8, 2),
        (9, 3),
        (10, 3),
        (11, 3),
        (12, 3),
        (13, 3),
        (14, 3),
        (15, 3),
    ];
    let mut e2 = ClassDefSizeEstimator::new(non_consecutive_map.iter().copied());
    assert!(check_add_class_def_size_running(
        &mut e2,
        &non_consecutive_map,
        1,
        &[1]
    ));
    assert!(check_add_class_def_size_running(
        &mut e2,
        &non_consecutive_map,
        2,
        &[1, 2]
    ));
    assert!(check_add_class_def_size_running(
        &mut e2,
        &non_consecutive_map,
        3,
        &[1, 2, 3]
    ));
    e2.reset();
    assert!(check_add_class_def_size_running(
        &mut e2,
        &non_consecutive_map,
        2,
        &[2]
    ));
    assert!(check_add_class_def_size_running(
        &mut e2,
        &non_consecutive_map,
        3,
        &[2, 3]
    ));
}

#[test]
fn test_running_class_size_estimates_with_locally_consecutive_glyphs() {
    let map = [(1, 1), (6, 2), (7, 3)];
    let mut e = ClassDefSizeEstimator::new(map.iter().copied());
    assert!(check_add_class_def_size_running(&mut e, &map, 1, &[1]));
    assert!(check_add_class_def_size_running(&mut e, &map, 2, &[1, 2]));
    assert!(check_add_class_def_size_running(
        &mut e,
        &map,
        3,
        &[1, 2, 3]
    ));
    e.reset();
    assert!(check_add_class_def_size_running(&mut e, &map, 2, &[2]));
    assert!(check_add_class_def_size_running(&mut e, &map, 3, &[2, 3]));
}

#[test]
fn test_running_coverage_size_estimates_with_interleaved_glyphs() {
    let map = [
        (0, 2),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 2),
        (5, 1),
        (6, 1),
        (7, 1),
        (8, 2),
        (10, 3),
    ];
    let mut e = ClassDefSizeEstimator::new(map.iter().copied());
    assert!(check_add_class_def_size_running(&mut e, &map, 1, &[1]));
    assert!(check_add_class_def_size_running(&mut e, &map, 2, &[1, 2]));
    assert!(check_add_class_def_size_running(
        &mut e,
        &map,
        3,
        &[1, 2, 3]
    ));
    e.reset();
    assert!(check_add_class_def_size_running(&mut e, &map, 2, &[2]));
    assert!(check_add_class_def_size_running(&mut e, &map, 1, &[1, 2]));
}

/// `test_class_def_intersects_class`, restricted to what the repacker
/// uses: `ClassDef::get_class` of format 2 tables with gaps, adjacent
/// ranges, explicit class zero and ranges at glyph 0.
#[test]
fn test_class_def_format_2_classes() {
    let cases: [[u16; 7]; 7] = [
        [0, 0, 0, 0, 0, 0, 0],
        [1, 0, 3, 1, 0, 0, 0],
        [1, 0, 7, 1, 0, 0, 0],
        [2, 0, 1, 1, 4, 5, 2],
        [2, 0, 1, 1, 2, 3, 2],
        [2, 0, 1, 0, 4, 5, 2],
        [2, 1, 2, 1, 4, 5, 2],
    ];
    for ranges in cases {
        let mut data = vec![0u8, 2];
        ranges
            .iter()
            .for_each(|v| data.extend_from_slice(&v.to_be_bytes()));
        let count = ranges[0] as usize;
        for g in 0..8u32 {
            let expected = (0..count)
                .map(|r| &ranges[1 + 3 * r..4 + 3 * r])
                .find(|r| u32::from(r[0]) <= g && g <= u32::from(r[1]))
                .map_or(0, |r| u32::from(r[2]));
            assert_eq!(class_of(&data, g), expected);
        }
    }
}

// --- More checks of the public functions ----------------------------------

#[test]
fn serialize_objects_matches_the_serializer() {
    let packed = ser(populate_serializer_simple).end().unwrap();
    let objs = packed.objects()[1..].to_vec();
    let out = crate::serialize_objects(sofel_read::Tag(*b"\0\0\0\0"), &objs).unwrap();
    assert_eq!(out, packed.resolve().unwrap());
}

#[test]
fn packed_repack_resolves_overflows() {
    let packed = ser(populate_serializer_with_dedup_overflow).end().unwrap();
    assert_eq!(packed.resolve(), Err(Errors::OFFSET_OVERFLOW));
    let out = packed.repack(sofel_read::Tag(*b"test")).unwrap();
    assert_eq!(out.len(), 10000 + 2 * 2 + 60000 + 2 + 3 * 2);
    let bad = ser(populate_serializer_with_unresolvable_priority_overflow)
        .end()
        .unwrap();
    assert_eq!(
        bad.repack(sofel_read::Tag(*b"test")),
        Err(crate::WriteError::OffsetOverflow)
    );
}
