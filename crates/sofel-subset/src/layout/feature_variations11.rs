//! LookupVariationRecord subsetting for FeatureVariations 1.1 (ADR 0022).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use sofel_font::read::feature_variations::{Condition, FeatureVariations};

use super::common::Sub;

enum Node {
    Axis(u16, i16, i16),
    Value(i16, u32),
    And(Vec<Node>),
    Or(Vec<Node>),
    Not(alloc::boxed::Box<Node>),
}

enum ResultNode {
    True,
    False,
    Keep(Node),
}

pub(super) fn evaluate_condition(
    source: Condition<'_>,
    coords: &[i32],
    deltas: &BTreeMap<u32, f64>,
    depth: u8,
) -> Option<bool> {
    if depth == 0 {
        return None;
    }
    Some(match source.format() {
        0 => true,
        1 => {
            let (axis, min, max) = source.axis_range()?;
            let coord = *coords.get(usize::from(axis))?;
            coord >= i32::from(min) && coord <= i32::from(max)
        }
        2 => {
            let (default, index) = source.value()?;
            f64::from(default) + deltas.get(&index).copied().unwrap_or(0.0) > 0.0
        }
        3 => {
            let mut result = true;
            for i in 0..source.children_len()? {
                if let Some(child) = source.child(i).ok()? {
                    result &= evaluate_condition(child, coords, deltas, depth.saturating_sub(1))?;
                }
            }
            result
        }
        4 => {
            let mut result = false;
            for i in 0..source.children_len()? {
                result |= source.child(i).ok()?.map_or(Some(true), |child| {
                    evaluate_condition(child, coords, deltas, depth.saturating_sub(1))
                })?;
            }
            result
        }
        5 => !source.child(0).ok()?.map_or(Some(true), |child| {
            evaluate_condition(child, coords, deltas, depth.saturating_sub(1))
        })?,
        _ => false,
    })
}

fn condition(s: &Sub<'_, '_>, source: Condition<'_>, depth: u8) -> Option<ResultNode> {
    if depth == 0 {
        return None;
    }
    Some(match source.format() {
        0 => ResultNode::True,
        1 => {
            let (axis, min, max) = source.axis_range()?;
            if s.p.var.user().is_empty() {
                ResultNode::Keep(Node::Axis(axis, min, max))
            } else {
                let tag = s.p.var.n.axis_tags.get(usize::from(axis))?;
                let (lo, hi) =
                    s.p.var
                        .n
                        .axes_location
                        .get(tag)
                        .map_or((-1.0, 1.0), |r| (r.min, r.max));
                let (a, b) = (f32::from(min) / 16384.0, f32::from(max) / 16384.0);
                if a > b || b < lo || a > hi {
                    ResultNode::False
                } else if a <= lo && hi <= b {
                    ResultNode::True
                } else {
                    let new_axis =
                        s.p.var
                            .n
                            .retained_axis_indices
                            .binary_search(&usize::from(axis))
                            .ok()?;
                    let Some(range) = s.p.var.n.axes_location.get(tag) else {
                        return Some(ResultNode::Keep(Node::Axis(
                            u16::try_from(new_axis).ok()?,
                            min,
                            max,
                        )));
                    };
                    let distance =
                        s.p.var
                            .n
                            .axis_distances
                            .get(usize::from(axis))
                            .copied()
                            .unwrap_or([1.0, 1.0]);
                    let convert = |v: f32| {
                        crate::instance::lo16(crate::instance::f64_to_i32(
                            sofel_geom::float::hb_roundf32(
                                crate::instance::renormalize(
                                    v.clamp(range.min, range.max),
                                    [range.min, range.default, range.max],
                                    distance,
                                ) * 16384.0,
                            ),
                        ))
                        .cast_signed()
                    };
                    ResultNode::Keep(Node::Axis(
                        u16::try_from(new_axis).ok()?,
                        convert(a),
                        convert(b),
                    ))
                }
            }
        }
        2 => {
            let (default, index) = source.value()?;
            if s.p.var.user().is_empty() {
                ResultNode::Keep(Node::Value(
                    default,
                    s.p.layout
                        .gdef_var_map
                        .get(&index)
                        .copied()
                        .unwrap_or(index),
                ))
            } else if index == u32::MAX {
                if default > 0 {
                    ResultNode::True
                } else {
                    ResultNode::False
                }
            } else if let Some(&(new, value)) = s.p.layout.gdef_condition_map.get(&(index, default))
            {
                if new == u32::MAX {
                    if value > 0 {
                        ResultNode::True
                    } else {
                        ResultNode::False
                    }
                } else {
                    ResultNode::Keep(Node::Value(value, new))
                }
            } else if let Some(&new) = s.p.layout.gdef_var_map.get(&index) {
                let delta =
                    s.p.layout
                        .pinned_layout_deltas
                        .get(&index)
                        .copied()
                        .unwrap_or(0);
                ResultNode::Keep(Node::Value(
                    i16::try_from(i32::from(default).saturating_add(delta)).ok()?,
                    new,
                ))
            } else if default > 0 {
                ResultNode::True
            } else {
                ResultNode::False
            }
        }
        3 | 4 => {
            let and = source.format() == 3;
            let mut children = Vec::new();
            for i in 0..source.children_len()? {
                let next = source
                    .child(i)
                    .ok()?
                    .map_or(Some(ResultNode::True), |child| {
                        condition(s, child, depth.saturating_sub(1))
                    })?;
                match next {
                    ResultNode::True if !and => return Some(ResultNode::True),
                    ResultNode::False if and => return Some(ResultNode::False),
                    ResultNode::Keep(child) => children.push(child),
                    _ => {}
                }
            }
            if children.is_empty() {
                if and {
                    ResultNode::True
                } else {
                    ResultNode::False
                }
            } else if and {
                ResultNode::Keep(Node::And(children))
            } else {
                ResultNode::Keep(Node::Or(children))
            }
        }
        5 => match source
            .child(0)
            .ok()?
            .map_or(Some(ResultNode::True), |child| {
                condition(s, child, depth.saturating_sub(1))
            })? {
            ResultNode::True => ResultNode::False,
            ResultNode::False => ResultNode::True,
            ResultNode::Keep(child) => ResultNode::Keep(Node::Not(alloc::boxed::Box::new(child))),
        },
        _ => ResultNode::False,
    })
}

fn write_node(s: &mut Sub<'_, '_>, node: &Node) -> bool {
    match node {
        Node::Axis(axis, min, max) => {
            s.s.u16(1);
            s.s.u16(*axis);
            s.s.i16(*min);
            s.s.i16(*max);
            true
        }
        Node::Value(value, index) => {
            s.s.u16(2);
            s.s.i16(*value);
            s.s.u32(*index);
            true
        }
        Node::And(children) | Node::Or(children) => {
            let Ok(count) = u8::try_from(children.len()) else {
                return false;
            };
            s.s.u16(if matches!(node, Node::And(_)) { 3 } else { 4 });
            s.s.u8(count);
            for child in children {
                if !s.offset_append(3, |s| write_node(s, child)) {
                    return false;
                }
            }
            true
        }
        Node::Not(child) => {
            s.s.u16(5);
            let at = s.s.u24(0);
            s.serialize_subset(at, 3, |s| write_node(s, child))
        }
    }
}

pub(super) fn write_set_condition(s: &mut Sub<'_, '_>, source: Option<Condition<'_>>) -> bool {
    let node = match source.map_or(Some(ResultNode::True), |c| condition(s, c, 64)) {
        Some(ResultNode::Keep(node)) => node,
        Some(ResultNode::True) => Node::Value(1, u32::MAX),
        Some(ResultNode::False) => Node::Value(0, u32::MAX),
        None => {
            s.unsupported("malformed or deeply nested feature variation condition");
            return false;
        }
    };
    write_node(s, &node)
}

pub(super) fn subset_lookup_records(s: &mut Sub<'_, '_>, fv: FeatureVariations<'_>) -> bool {
    let count_at = s.s.u32(0);
    let mut count = 0u32;
    for i in 0..fv.lookup_record_count() {
        let Some(record) = fv.lookup_record(i) else {
            continue;
        };
        let Some(&feature) = s
            .lp()
            .features_w_dups
            .get(&u32::from(record.feature_index()))
        else {
            continue;
        };
        let Ok(Some(table)) = record.feature_lookups() else {
            continue;
        };
        let snapshot = s.s.snapshot();
        let Ok(feature) = u16::try_from(feature) else {
            continue;
        };
        s.s.u16(feature);
        let at = s.s.u32(0);
        if !s.serialize_subset(at, 4, |s| {
            s.s.u32(table.version());
            s.s.u16(table.flags());
            let records_at = s.s.u32(0);
            let mut records = 0u32;
            for j in 0..table.len() {
                let Some(r) = table.record(j) else { continue };
                let node = match r.condition().ok().flatten() {
                    Some(source) => condition(s, source, 64),
                    None => Some(ResultNode::True),
                };
                let Some(node) = node else { return false };
                let node = match node {
                    ResultNode::False => continue,
                    ResultNode::True => Node::Value(1, u32::MAX),
                    ResultNode::Keep(node) => node,
                };
                let Some(indices) = r.lookup_indices().ok().flatten() else {
                    continue;
                };
                let kept = (0..indices.len())
                    .filter_map(|k| {
                        indices
                            .get(k)
                            .and_then(|old| s.lookup_map().get(&u32::from(old)).copied())
                    })
                    .collect::<Vec<_>>();
                if kept.is_empty() {
                    continue;
                }
                let condition_at = s.s.u32(0);
                let indices_at = s.s.u32(0);
                if !s.serialize_subset(condition_at, 4, |s| write_node(s, &node)) {
                    return false;
                }
                if !s.serialize_subset(indices_at, 4, |s| {
                    let Ok(count) = u16::try_from(kept.len()) else {
                        return false;
                    };
                    s.s.u16(count);
                    for &index in &kept {
                        let Ok(index) = u16::try_from(index) else {
                            return false;
                        };
                        s.s.u16(index);
                    }
                    true
                }) {
                    return false;
                }
                records = records.saturating_add(1);
            }
            s.s.set_u32(records_at, records);
            records != 0
        }) {
            s.s.revert(snapshot);
            continue;
        }
        count = count.saturating_add(1);
    }
    s.s.set_u32(count_at, count);
    count != 0
}
