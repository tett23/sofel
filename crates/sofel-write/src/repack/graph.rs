//! HarfBuzz 15.1.0's `graph_t` (`src/graph/graph.hh`): the packed objects
//! as a graph whose vertices can be reordered, duplicated and split.
//!
//! Vertex ids are stable: a clone gets the next id, and `ordering` holds
//! the current topological order (the root first). A vertex's bytes live
//! in a buffer of the graph; a clone made without copying shares its
//! original's buffer, so writing through one is seen by both, as with
//! HarfBuzz's pointers.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use super::hbmap::HbMap;
use super::queue::PriorityQueue;
use super::set::HbSet;
use crate::serialize::{Link, Object};

/// `HB_GRAPH_INVALID`.
pub(crate) const INVALID: u32 = u32::MAX;
/// `HB_REPACKER_MAX_VERTICES`.
pub(crate) const MAX_VERTICES: usize = 800_000;
/// `HB_REPACKER_MAX_SPACES`.
pub(crate) const MAX_SPACES: u32 = 8000;

const NULL_BUF: usize = usize::MAX;

/// Why the repacker failed (`graph_error_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GraphError {
    /// An allocation failed, or the output does not fit in a blob.
    AllocationFailure,
    /// A limit on vertices or spaces was reached.
    LimitExceeded,
    /// An object or link is not valid.
    InvalidArgument,
    /// The graph has a cycle.
    CycleDetected,
    /// A GSUB or GPOS table could not be read.
    SanitizeFailure,
    /// An index or position is out of bounds.
    OutOfBounds,
    /// Some object cannot be reached from the root.
    OrphanedNodes,
    /// The overflows could not be resolved.
    OverflowResolutionFailed,
}

pub(crate) type R<T> = Result<T, GraphError>;

/// A vertex (`graph_t::vertex_t`).
#[derive(Clone, Debug)]
pub(crate) struct Vertex {
    pub(crate) distance: i64,
    pub(crate) space: u32,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) priority: u32,
    buf: usize,
    head: usize,
    tail: usize,
    pub(crate) real_links: Vec<Link>,
    pub(crate) virtual_links: Vec<Link>,
    incoming_edges: u32,
    single_parent: u32,
    has_incoming_virtual_edges: bool,
    parents: HbMap,
    virtual_parents: BTreeSet<u32>,
}

impl Default for Vertex {
    fn default() -> Self {
        Vertex {
            distance: 0,
            space: 0,
            start: 0,
            end: 0,
            priority: 0,
            buf: NULL_BUF,
            head: 0,
            tail: 0,
            real_links: Vec::new(),
            virtual_links: Vec::new(),
            incoming_edges: 0,
            single_parent: INVALID,
            has_incoming_virtual_edges: false,
            parents: HbMap::new(),
            virtual_parents: BTreeSet::new(),
        }
    }
}

/// `link_t::cmp`: by position, then by child.
pub(crate) fn sort_links(links: &mut [Link]) {
    links.sort_by_key(|l| (l.position, l.objidx));
}

impl Vertex {
    /// `table_size`.
    pub(crate) fn table_size(&self) -> usize {
        self.tail.saturating_sub(self.head)
    }

    /// The parents: the single parent, then the others in map order
    /// (`parents_iter`).
    pub(crate) fn parents_iter(&self) -> impl Iterator<Item = u32> + '_ {
        (self.single_parent != INVALID)
            .then_some(self.single_parent)
            .into_iter()
            .chain(self.parents.keys())
    }

    /// `is_shared`.
    pub(crate) fn is_shared(&self) -> bool {
        self.parents.population()
            > u32::try_from(self.virtual_parents.len())
                .unwrap_or(u32::MAX)
                .wrapping_add(1)
    }

    /// `incoming_edges`.
    pub(crate) fn incoming_edges(&self) -> u32 {
        self.incoming_edges
    }

    /// `incoming_edges_from_parent`.
    pub(crate) fn incoming_edges_from_parent(&self, parent: u32) -> u32 {
        if self.single_parent != INVALID {
            return u32::from(self.single_parent == parent);
        }
        self.parents.get(parent).unwrap_or(0)
    }

    /// `has_incoming_virtual_edges`.
    pub(crate) fn has_incoming_virtual_edges(&self) -> bool {
        self.has_incoming_virtual_edges
    }

    /// `reset_parents`.
    pub(crate) fn reset_parents(&mut self) {
        self.incoming_edges = 0;
        self.has_incoming_virtual_edges = false;
        self.single_parent = INVALID;
        self.parents.clear();
        self.virtual_parents.clear();
    }

    /// `add_parent`.
    pub(crate) fn add_parent(&mut self, parent: u32, is_virtual: bool) {
        self.has_incoming_virtual_edges |= is_virtual;
        if is_virtual {
            self.virtual_parents.insert(parent);
        }
        if self.incoming_edges == 0 {
            self.single_parent = parent;
            self.incoming_edges = 1;
            return;
        } else if self.single_parent != INVALID {
            if !self.parents.set(self.single_parent, 1) {
                return;
            }
            self.single_parent = INVALID;
        }
        if let Some(v) = self.parents.get_mut(parent) {
            *v = v.wrapping_add(1);
            self.incoming_edges = self.incoming_edges.wrapping_add(1);
        } else if self.parents.set(parent, 1) {
            self.incoming_edges = self.incoming_edges.wrapping_add(1);
        }
    }

    /// `remove_parent`.
    pub(crate) fn remove_parent(&mut self, parent: u32) {
        if parent == self.single_parent {
            self.single_parent = INVALID;
            self.incoming_edges = self.incoming_edges.wrapping_sub(1);
            self.virtual_parents.clear();
            return;
        }
        let Some(v) = self.parents.get(parent) else {
            return;
        };
        self.incoming_edges = self.incoming_edges.wrapping_sub(1);
        if v > 1 {
            if let Some(v) = self.parents.get_mut(parent) {
                *v = v.wrapping_sub(1);
            }
        } else {
            self.parents.del(parent);
            self.virtual_parents.remove(&parent);
        }
        if self.incoming_edges == 1 {
            self.single_parent = self.parents.keys().next().unwrap_or(INVALID);
            self.parents.clear();
        }
    }

    /// `is_leaf`.
    pub(crate) fn is_leaf(&self) -> bool {
        self.real_links.is_empty() && self.virtual_links.is_empty()
    }

    /// `raise_priority`.
    pub(crate) fn raise_priority(&mut self) -> bool {
        if self.has_max_priority() {
            return false;
        }
        self.priority = self.priority.wrapping_add(1);
        true
    }

    /// `give_max_priority`.
    pub(crate) fn give_max_priority(&mut self) -> bool {
        let mut result = false;
        while !self.has_max_priority() {
            result = true;
            self.priority = self.priority.wrapping_add(1);
        }
        result
    }

    /// `has_max_priority`.
    pub(crate) fn has_max_priority(&self) -> bool {
        self.priority >= 3
    }

    fn size_i64(&self) -> i64 {
        i64::try_from(self.table_size()).unwrap_or(i64::MAX)
    }

    /// `modified_distance`.
    pub(crate) fn modified_distance(&self, order: u32) -> i64 {
        let mut d = self
            .distance
            .saturating_add(self.distance_modifier())
            .clamp(0, i64::MAX >> 18);
        if self.has_max_priority() {
            d = 0;
        }
        (d << 18) | i64::from(0x3_FFFF & order)
    }

    /// `distance_modifier`.
    pub(crate) fn distance_modifier(&self) -> i64 {
        match self.priority {
            0 => 0,
            1 => (self.size_i64() / 2).wrapping_neg(),
            _ => self.size_i64().wrapping_neg(),
        }
    }

    /// `sort_real_links`.
    pub(crate) fn sort_real_links(&mut self) {
        sort_links(&mut self.real_links);
    }

    /// `add_real_link` of a plain unsigned offset from the head.
    pub(crate) fn add_real_link(&mut self, width: u8, objidx: u32, position: u32) {
        self.real_links.push(Link {
            width,
            objidx,
            position,
            ..Link::default()
        });
    }

    /// `add_virtual_link`.
    pub(crate) fn add_virtual_link(&mut self, objidx: u32) {
        self.virtual_links.push(Link {
            objidx,
            ..Link::default()
        });
    }

    /// `remove_real_link_unordered (i)`: the last link takes its place.
    pub(crate) fn remove_real_link_unordered(&mut self, i: usize) {
        if i < self.real_links.len() {
            self.real_links.swap_remove(i);
        }
    }

    /// `position_to_index_map`.
    pub(crate) fn position_to_index_map(&self) -> BTreeMap<u32, u32> {
        self.real_links
            .iter()
            .map(|l| (l.position, l.objidx))
            .collect()
    }

    /// `link_positions_valid`.
    fn link_positions_valid(&self, num_objects: usize) -> R<()> {
        let mut assigned: BTreeMap<u32, u32> = BTreeMap::new();
        for l in &self.real_links {
            if l.objidx as usize >= num_objects || l.objidx == 0 {
                return Err(GraphError::InvalidArgument);
            }
            let start = l.position;
            let end = start.wrapping_add(u32::from(l.width)).wrapping_sub(1);
            if l.width < 2 || l.width > 4 {
                return Err(GraphError::InvalidArgument);
            }
            if end < start || end as usize >= self.table_size() {
                return Err(GraphError::OutOfBounds);
            }
            let overlaps = assigned
                .range(..=end)
                .next_back()
                .is_some_and(|(_, &e)| e >= start);
            if overlaps {
                return Err(GraphError::InvalidArgument);
            }
            assigned.insert(start, end);
        }
        Ok(())
    }
}

/// The objects of a serialization as a graph (`graph_t`).
#[derive(Clone, Debug)]
pub(crate) struct Graph {
    pub(crate) vertices: Vec<Vertex>,
    /// The current topological order: `ordering[pos]` is a vertex id.
    pub(crate) ordering: Vec<u32>,
    bufs: Vec<Vec<u8>>,
    parents_invalid: bool,
    distance_invalid: bool,
    positions_invalid: bool,
    num_roots_for_space: Vec<u32>,
}

/// Which link of a vertex.
#[derive(Clone, Copy)]
struct LinkAt {
    is_virtual: bool,
    i: usize,
}

impl Graph {
    /// `graph_t::create` from the packed objects (index 0 is the null
    /// object, the last is the root).
    pub(crate) fn create(objects: &[Object]) -> R<Graph> {
        if objects.len() > MAX_VERTICES {
            return Err(GraphError::LimitExceeded);
        }
        let count = objects.len();
        let mut g = Graph {
            vertices: Vec::with_capacity(count),
            ordering: alloc::vec![0; count.saturating_sub(1)],
            bufs: Vec::with_capacity(count),
            parents_invalid: true,
            distance_invalid: true,
            positions_invalid: true,
            num_roots_for_space: alloc::vec![1],
        };
        let mut order = count.saturating_sub(1);
        for (i, o) in objects.iter().enumerate().skip(1) {
            let mut v = Vertex {
                buf: g.bufs.len(),
                head: 0,
                tail: o.bytes.len(),
                real_links: o.real_links.iter().map(|l| mask_bias(*l)).collect(),
                virtual_links: o.virtual_links.iter().map(|l| mask_bias(*l)).collect(),
                ..Vertex::default()
            };
            g.bufs.push(o.bytes.clone());
            v.link_positions_valid(count)?;
            order = order.saturating_sub(1);
            if let Some(slot) = g.ordering.get_mut(order) {
                *slot = u32::try_from(i.wrapping_sub(1)).unwrap_or(INVALID);
            }
            v.real_links
                .iter_mut()
                .chain(v.virtual_links.iter_mut())
                .for_each(|l| l.objidx = l.objidx.wrapping_sub(1));
            g.vertices.push(v);
        }
        if g.vertices.is_empty() {
            return Err(GraphError::InvalidArgument);
        }
        g.is_fully_connected()?;
        Ok(g)
    }

    /// The number of vertices.
    pub(crate) fn len(&self) -> usize {
        self.vertices.len()
    }

    fn len_u32(&self) -> u32 {
        u32::try_from(self.vertices.len()).unwrap_or(u32::MAX)
    }

    /// The vertex `i`.
    pub(crate) fn v(&self, i: u32) -> R<&Vertex> {
        self.vertices.get(i as usize).ok_or(GraphError::OutOfBounds)
    }

    /// The vertex `i`, mutably.
    pub(crate) fn vm(&mut self, i: u32) -> R<&mut Vertex> {
        self.vertices
            .get_mut(i as usize)
            .ok_or(GraphError::OutOfBounds)
    }

    /// The table size of vertex `i` (0 when out of bounds).
    pub(crate) fn table_size(&self, i: u32) -> usize {
        self.v(i).map_or(0, Vertex::table_size)
    }

    /// The bytes of vertex `i`.
    pub(crate) fn bytes(&self, i: u32) -> &[u8] {
        self.v(i)
            .ok()
            .and_then(|v| self.bufs.get(v.buf).and_then(|b| b.get(v.head..v.tail)))
            .unwrap_or(&[])
    }

    /// Whether vertex `i` has no buffer (HarfBuzz's null head).
    pub(crate) fn is_null(&self, i: u32) -> bool {
        self.v(i).map_or(true, |v| v.buf == NULL_BUF)
    }

    /// Reads `n` bytes big-endian at `off` from vertex `i`'s head, within
    /// its buffer (not only its table); 0 outside it.
    pub(crate) fn rd(&self, i: u32, off: usize, n: usize) -> u32 {
        let Ok(v) = self.v(i) else { return 0 };
        let at = v.head.saturating_add(off);
        self.bufs
            .get(v.buf)
            .and_then(|b| b.get(at..at.saturating_add(n)))
            .map_or(0, |s| s.iter().fold(0u32, |a, &b| (a << 8) | u32::from(b)))
    }

    /// A big-endian `u16` at `off` of vertex `i`.
    pub(crate) fn rd16(&self, i: u32, off: usize) -> u32 {
        self.rd(i, off, 2)
    }

    /// Writes the low `n` bytes of `val` big-endian at `off` of vertex `i`
    /// (nothing outside its buffer).
    pub(crate) fn wr(&mut self, i: u32, off: usize, n: usize, val: u32) {
        let Some(v) = self.vertices.get(i as usize) else {
            return;
        };
        let at = v.head.saturating_add(off);
        if let Some(s) = self
            .bufs
            .get_mut(v.buf)
            .and_then(|b| b.get_mut(at..at.saturating_add(n)))
        {
            let b = val.to_be_bytes();
            s.copy_from_slice(b.get(4usize.saturating_sub(n)..).unwrap_or(&[]));
        }
    }

    /// Writes a big-endian `u16`.
    pub(crate) fn wr16(&mut self, i: u32, off: usize, val: u32) {
        self.wr(i, off, 2, val);
    }

    /// Gives vertex `i` a new buffer (`set_buffer` to fresh memory).
    pub(crate) fn set_new_buffer(&mut self, i: u32, bytes: Vec<u8>) {
        let len = bytes.len();
        let b = self.bufs.len();
        self.bufs.push(bytes);
        if let Some(v) = self.vertices.get_mut(i as usize) {
            v.buf = b;
            v.head = 0;
            v.tail = len;
        }
    }

    /// Sets vertex `i`'s table to `len` bytes from its head
    /// (`set_buffer (head, head + len)`).
    pub(crate) fn set_table_len(&mut self, i: u32, len: usize) {
        if let Some(v) = self.vertices.get_mut(i as usize) {
            v.tail = v.head.saturating_add(len);
        }
    }

    /// `shrink_buffer`.
    pub(crate) fn shrink_buffer(&mut self, i: u32, count: usize) {
        if let Some(v) = self.vertices.get_mut(i as usize) {
            v.tail = v.tail.wrapping_sub(count).max(v.head);
        }
    }

    /// `root_idx`: the first of the order.
    pub(crate) fn root_idx(&self) -> u32 {
        self.ordering.first().copied().unwrap_or(INVALID)
    }

    /// `add_link`: a 16-bit link from `parent` at `position` to `child`.
    pub(crate) fn add_link(&mut self, position: u32, parent: u32, child: u32) -> R<()> {
        if parent as usize >= self.len() || child as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        self.vm(parent)?.add_real_link(2, child, position);
        self.vm(child)?.add_parent(parent, false);
        Ok(())
    }

    /// `sort_shortest_distance_if_needed`.
    pub(crate) fn sort_shortest_distance_if_needed(&mut self) -> R<()> {
        if !self.positions_invalid {
            return Ok(());
        }
        self.sort_shortest_distance()
    }

    /// `sort_shortest_distance`.
    pub(crate) fn sort_shortest_distance(&mut self) -> R<()> {
        self.positions_invalid = true;
        if self.len() <= 1 {
            return Ok(());
        }
        self.update_distances()?;
        let n = self.len();
        let mut queue = PriorityQueue::new();
        let mut new_ordering = alloc::vec![0u32; n];
        let mut removed_edges = alloc::vec![0u32; n];
        self.update_parents()?;
        let root = self.root_idx();
        queue.insert(self.v(root)?.modified_distance(0), root);
        let mut order = 1u32;
        let mut pos = 0usize;
        while let Some((_, next_id)) = queue.pop_minimum() {
            let Some(slot) = new_ordering.get_mut(pos) else {
                return Err(GraphError::CycleDetected);
            };
            *slot = next_id;
            pos = pos.wrapping_add(1);
            let next = self.v(next_id)?;
            for link in next.real_links.iter().chain(next.virtual_links.iter()) {
                let Some(removed) = removed_edges.get_mut(link.objidx as usize) else {
                    return Err(GraphError::OutOfBounds);
                };
                *removed = removed.wrapping_add(1);
                let v = self.v(link.objidx)?;
                if v.incoming_edges().wrapping_sub(*removed) == 0 {
                    queue.insert(v.modified_distance(order), link.objidx);
                    order = order.wrapping_add(1);
                }
            }
        }
        self.ordering = new_ordering;
        if pos != n {
            return Err(GraphError::OrphanedNodes);
        }
        Ok(())
    }

    /// `find_space_roots`.
    fn find_space_roots(&mut self, visited: &mut HbSet, roots: &mut HbSet) -> R<()> {
        let root_index = self.root_idx();
        let order = self.ordering.clone();
        for i in order {
            if visited.has(i) {
                continue;
            }
            let links = self.v(i)?.real_links.clone();
            for l in links {
                if l.signed || l.width < 3 {
                    continue;
                }
                if i == root_index && l.width == 3 {
                    continue;
                }
                if l.width >= 3 {
                    let mut sub_roots = HbSet::new();
                    self.find_32bit_roots(l.objidx, &mut sub_roots)?;
                    if !sub_roots.is_empty() {
                        for s in sub_roots.iter() {
                            roots.add(s);
                            self.find_subgraph_set(s, visited)?;
                        }
                        continue;
                    }
                }
                roots.add(l.objidx);
                self.find_subgraph_set(l.objidx, visited)?;
            }
        }
        Ok(())
    }

    /// `index_for_offset`: the child linked from `position` of `node`.
    pub(crate) fn index_for_offset(&self, node: u32, position: u32) -> R<u32> {
        let v = self.v(node)?;
        if position as usize >= v.table_size() {
            return Err(GraphError::OutOfBounds);
        }
        v.real_links
            .iter()
            .find(|l| l.position == position)
            .map(|l| l.objidx)
            .ok_or(GraphError::InvalidArgument)
    }

    /// `mutable_index_for_offset`: as [`Self::index_for_offset`], but a
    /// child shared with other parents is first duplicated (with its
    /// bytes copied).
    pub(crate) fn mutable_index_for_offset(&mut self, node: u32, position: u32) -> R<u32> {
        let child = self.index_for_offset(node, position)?;
        let shared = self.v(child)?.parents_iter().any(|p| p != node);
        if shared {
            return self.duplicate_child(node, child, true);
        }
        Ok(child)
    }

    /// `assign_spaces`.
    pub(crate) fn assign_spaces(&mut self) -> R<bool> {
        self.update_parents()?;
        for v in &mut self.vertices {
            v.space = 0;
            v.priority = 0;
        }
        self.num_roots_for_space = alloc::vec![1];
        self.distance_invalid = true;
        self.positions_invalid = true;

        let mut visited = HbSet::new();
        let mut roots = HbSet::new();
        self.find_space_roots(&mut visited, &mut roots)?;
        visited.invert();
        if roots.is_empty() {
            return Ok(false);
        }
        while let Some(next) = roots.min() {
            let mut connected = HbSet::new();
            let before = roots.population();
            self.find_connected_nodes(next, &mut roots, &mut visited, &mut connected)?;
            self.isolate_subgraph(&mut connected)?;
            let next_space = self.next_space();
            if next_space >= MAX_SPACES {
                return Err(GraphError::LimitExceeded);
            }
            self.num_roots_for_space.push(0);
            for r in connected.iter() {
                if let Some(v) = self.vertices.get_mut(r as usize) {
                    v.space = next_space;
                }
                if let Some(n) = self.num_roots_for_space.get_mut(next_space as usize) {
                    *n = n.wrapping_add(1);
                }
                self.distance_invalid = true;
                self.positions_invalid = true;
            }
            if roots.population() == before {
                // HarfBuzz would loop forever here; it cannot happen for a
                // graph whose roots are in their own subgraphs.
                break;
            }
        }
        Ok(true)
    }

    /// `isolate_subgraph`: duplicates the parts of the subgraphs of
    /// `roots` linked from outside them; `roots` gets the new ids.
    pub(crate) fn isolate_subgraph(&mut self, roots: &mut HbSet) -> R<bool> {
        self.update_parents()?;
        let mut subgraph = HbMap::new();
        let mut parents = HbSet::new();
        self.wide_parents(roots, &mut subgraph, &mut parents)?;
        for r in roots.iter().collect::<Vec<_>>() {
            self.find_subgraph_map(r, &mut subgraph)?;
        }
        let mut index_map = HbMap::new();
        let mut made_changes = false;
        let entries: Vec<(u32, u32)> = subgraph.iter().collect();
        for (node, subgraph_incoming) in entries {
            if subgraph_incoming < self.v(node)?.incoming_edges() {
                made_changes = true;
                self.duplicate_subgraph(node, &mut index_map)?;
            }
        }
        if !made_changes {
            return Ok(false);
        }
        let new_subgraph: Vec<u32> = subgraph
            .keys()
            .map(|n| index_map.get(n).unwrap_or(n))
            .collect();
        self.remap_obj_indices(&index_map, &new_subgraph, false)?;
        let parent_list: Vec<u32> = parents.iter().collect();
        self.remap_obj_indices(&index_map, &parent_list, true)?;
        let old: Vec<u32> = roots.iter().collect();
        for r in old {
            if let Some(n) = index_map.get(r) {
                roots.del(r);
                roots.add(n);
            }
        }
        Ok(true)
    }

    /// `traverse_directed_bfs`. `visit` gets the graph, the parent (or
    /// [`INVALID`] for the start), the link (none for the start), the
    /// child and its depth, and returns whether to go on into the child.
    pub(crate) fn traverse_bfs<F>(&mut self, start: u32, mut visit: F) -> R<()>
    where
        F: FnMut(&mut Graph, u32, Option<Link>, u32, u32) -> bool,
    {
        if start as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        let cap = self.len();
        if !visit(self, INVALID, None, start, 0) {
            return Ok(());
        }
        let mut queue: Vec<u32> = Vec::with_capacity(cap);
        queue.push(start);
        let mut head = 0usize;
        let mut depth = 0u32;
        while head < queue.len() {
            let level_end = queue.len();
            while head < level_end {
                let node = queue.get(head).copied().unwrap_or(INVALID);
                head = head.wrapping_add(1);
                let (num_real, total) = match self.v(node) {
                    Ok(v) => (
                        v.real_links.len(),
                        v.real_links.len().wrapping_add(v.virtual_links.len()),
                    ),
                    Err(_) => (0, 0),
                };
                for i in 0..total {
                    let Ok(v) = self.v(node) else { break };
                    let link = if i < num_real {
                        v.real_links.get(i)
                    } else {
                        v.virtual_links.get(i.wrapping_sub(num_real))
                    };
                    let Some(&link) = link else { break };
                    let child = link.objidx;
                    if !visit(self, node, Some(link), child, depth.wrapping_add(1)) {
                        continue;
                    }
                    if queue.len() >= cap {
                        return Err(GraphError::CycleDetected);
                    }
                    queue.push(child);
                }
            }
            depth = depth.wrapping_add(1);
        }
        Ok(())
    }

    /// `find_subgraph` into a map of incoming edge counts (the start
    /// itself is not counted).
    pub(crate) fn find_subgraph_map(&mut self, node: u32, subgraph: &mut HbMap) -> R<()> {
        self.traverse_bfs(node, |_, _, _, child, depth| {
            if depth == 0 {
                return true;
            }
            if let Some(c) = subgraph.get_mut(child) {
                *c = c.wrapping_add(1);
                return false;
            }
            subgraph.set(child, 1);
            true
        })
    }

    /// `find_subgraph` into a set.
    pub(crate) fn find_subgraph_set(&mut self, node: u32, subgraph: &mut HbSet) -> R<()> {
        self.traverse_bfs(node, |_, _, _, child, _| {
            if subgraph.has(child) {
                return false;
            }
            subgraph.add(child);
            true
        })
    }

    /// `find_subgraph_size`: the bytes of the vertices reached from `node`
    /// within `max_depth` that are not yet in `subgraph`.
    pub(crate) fn find_subgraph_size(
        &mut self,
        node: u32,
        subgraph: &mut HbSet,
        max_depth: u32,
    ) -> R<usize> {
        let mut size = 0usize;
        self.traverse_bfs(node, |g, _, _, child, depth| {
            if subgraph.has(child) {
                return false;
            }
            subgraph.add(child);
            size = size.wrapping_add(g.table_size(child));
            depth < max_depth
        })?;
        Ok(size)
    }

    /// `find_32bit_roots`.
    pub(crate) fn find_32bit_roots(&mut self, node: u32, found: &mut HbSet) -> R<()> {
        let mut visited = HbSet::new();
        self.traverse_bfs(node, |_, parent, link, child, _| {
            if link.is_some() && found.has(parent) {
                return false;
            }
            if let Some(l) = link
                && !l.signed
                && l.width == 4
            {
                found.add(l.objidx);
                visited.add(l.objidx);
                return false;
            }
            if visited.has(child) {
                return false;
            }
            visited.add(child);
            true
        })
    }

    /// `move_child`: moves the link at `old_position` of `old_parent` to
    /// `new_position` of `new_parent` (an offset of `width` bytes).
    pub(crate) fn move_child(
        &mut self,
        old_parent: u32,
        old_position: u32,
        new_parent: u32,
        new_position: u32,
        width: u8,
    ) -> R<u32> {
        self.distance_invalid = true;
        self.positions_invalid = true;
        if old_parent as usize >= self.len() || new_parent as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        let child = self.index_for_offset(old_parent, old_position)?;
        if child as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        self.vm(new_parent)?
            .add_real_link(width, child, new_position);
        self.vm(child)?.add_parent(new_parent, false);
        let old = self.vm(old_parent)?;
        if let Some(i) = old
            .real_links
            .iter()
            .position(|l| l.objidx == child && l.position == old_position)
        {
            old.real_links.swap_remove(i);
        }
        self.vm(child)?.remove_parent(old_parent);
        Ok(child)
    }

    /// `move_children`: moves the links of `old_parent` at positions in
    /// `[old_start, old_end)` to `new_parent`, at the same distance from
    /// `new_start`.
    pub(crate) fn move_children(
        &mut self,
        old_parent: u32,
        old_start: u32,
        old_end: u32,
        new_parent: u32,
        new_start: u32,
        width: u8,
    ) -> R<()> {
        if old_parent as usize >= self.len() || new_parent as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        self.distance_invalid = true;
        self.positions_invalid = true;
        let links = self.v(old_parent)?.real_links.clone();
        let mut kept = Vec::new();
        for l in links {
            if l.position < old_start || l.position >= old_end {
                kept.push(l);
                continue;
            }
            let at = l.position.wrapping_sub(old_start);
            let child = l.objidx;
            if child as usize >= self.len() {
                return Err(GraphError::OutOfBounds);
            }
            self.vm(new_parent)?
                .add_real_link(width, child, new_start.wrapping_add(at));
            let c = self.vm(child)?;
            c.add_parent(new_parent, false);
            c.remove_parent(old_parent);
        }
        self.vm(old_parent)?.real_links = kept;
        Ok(())
    }

    /// `duplicate_subgraph`: clones every vertex reachable from `node`
    /// that `index_map` does not map yet; links are not reassigned.
    pub(crate) fn duplicate_subgraph(&mut self, node: u32, index_map: &mut HbMap) -> R<()> {
        let mut result = Ok(());
        self.traverse_bfs(node, |g, _, _, child, _| {
            if result.is_err() || index_map.has(child) {
                return false;
            }
            match g.duplicate(child, false) {
                Ok(c) => {
                    index_map.set(child, c);
                    true
                }
                Err(e) => {
                    result = Err(e);
                    false
                }
            }
        })?;
        result
    }

    /// `duplicate (node_idx, copy_table)`: a copy of `node` linking to the
    /// same children; returns its id.
    pub(crate) fn duplicate(&mut self, node: u32, copy_table: bool) -> R<u32> {
        if node as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        if self.len() >= MAX_VERTICES {
            return Err(GraphError::LimitExceeded);
        }
        self.positions_invalid = true;
        self.distance_invalid = true;
        let clone_idx = self.len_u32();
        self.ordering.push(clone_idx);
        let child = self.v(node)?.clone();
        let mut clone = Vertex::default();
        if copy_table && child.table_size() > 0 {
            let bytes = self.bytes(node).to_vec();
            clone.buf = self.bufs.len();
            clone.head = 0;
            clone.tail = bytes.len();
            self.bufs.push(bytes);
        } else {
            clone.buf = child.buf;
            clone.head = child.head;
            clone.tail = child.tail;
        }
        clone.distance = child.distance;
        clone.space = child.space;
        self.vertices.push(clone);
        for l in &child.real_links {
            self.vm(clone_idx)?.real_links.push(*l);
            self.vm(l.objidx)?.add_parent(clone_idx, false);
        }
        for l in &child.virtual_links {
            self.vm(clone_idx)?.virtual_links.push(*l);
            self.vm(l.objidx)?.add_parent(clone_idx, true);
        }
        Ok(clone_idx)
    }

    fn reassign_links_to(&mut self, parent: u32, child: u32, clone: u32) -> R<()> {
        let p = self.v(parent)?;
        let at: Vec<LinkAt> = p
            .real_links
            .iter()
            .enumerate()
            .map(|(i, l)| (false, i, l.objidx))
            .chain(
                p.virtual_links
                    .iter()
                    .enumerate()
                    .map(|(i, l)| (true, i, l.objidx)),
            )
            .filter(|&(_, _, o)| o == child)
            .map(|(is_virtual, i, _)| LinkAt { is_virtual, i })
            .collect();
        for a in at {
            self.reassign_link(parent, a, clone)?;
        }
        Ok(())
    }

    /// `duplicate (parent_idx, child_idx, copy_table)`: clones `child` and
    /// moves `parent`'s links to the clone.
    pub(crate) fn duplicate_child(&mut self, parent: u32, child: u32, copy_table: bool) -> R<u32> {
        if parent as usize >= self.len() || child as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        self.update_parents()?;
        let c = self.v(child)?;
        let links_to_child = c.incoming_edges_from_parent(parent);
        if c.incoming_edges() <= links_to_child || c.has_incoming_virtual_edges() {
            return Err(GraphError::InvalidArgument);
        }
        let clone = self.duplicate(child, copy_table)?;
        let mut parent = parent;
        if parent == clone {
            parent = parent.wrapping_add(1);
        }
        self.reassign_links_to(parent, child, clone)?;
        Ok(clone)
    }

    /// `duplicate (parents, child_idx)`: clones `child` and moves the
    /// links of all of `parents` to the clone.
    pub(crate) fn duplicate_parents(&mut self, parents: &HbSet, child: u32) -> R<u32> {
        if parents.is_empty() {
            return Err(GraphError::InvalidArgument);
        }
        if child as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        self.update_parents()?;
        let c = self.v(child)?;
        let mut links_to_child = 0u32;
        for p in parents.iter() {
            if p as usize >= self.len() {
                return Err(GraphError::OutOfBounds);
            }
            links_to_child = links_to_child.wrapping_add(c.incoming_edges_from_parent(p));
        }
        if c.incoming_edges() <= links_to_child || c.has_incoming_virtual_edges() {
            return Err(GraphError::InvalidArgument);
        }
        let clone = self.duplicate(child, false)?;
        for p in parents.iter().collect::<Vec<_>>() {
            let p = if p == clone { p.wrapping_add(1) } else { p };
            self.reassign_links_to(p, child, clone)?;
        }
        Ok(clone)
    }

    /// `new_node`: a vertex not linked to anything, with `bytes` (none for
    /// HarfBuzz's null head).
    pub(crate) fn new_node(&mut self, bytes: Option<Vec<u8>>) -> R<u32> {
        if self.len() >= MAX_VERTICES {
            return Err(GraphError::LimitExceeded);
        }
        self.positions_invalid = true;
        self.distance_invalid = true;
        let idx = self.len_u32();
        self.ordering.push(idx);
        let mut v = Vertex::default();
        if let Some(b) = bytes {
            v.buf = self.bufs.len();
            v.tail = b.len();
            self.bufs.push(b);
        }
        self.vertices.push(v);
        Ok(idx)
    }

    /// `remap_child`: clones `old_child` and moves `parent`'s links to it.
    pub(crate) fn remap_child(&mut self, parent: u32, old_child: u32) -> R<u32> {
        if parent as usize >= self.len() || old_child as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        let new_child = self.duplicate(old_child, false)?;
        self.reassign_links_to(parent, old_child, new_child)?;
        Ok(new_child)
    }

    /// `raise_childrens_priority`.
    pub(crate) fn raise_childrens_priority(&mut self, parent: u32) -> R<bool> {
        let v = self.v(parent)?;
        let children: Vec<u32> = v
            .real_links
            .iter()
            .chain(v.virtual_links.iter())
            .map(|l| l.objidx)
            .collect();
        let mut made_change = false;
        for c in children {
            made_change |= self.vm(c)?.raise_priority();
        }
        Ok(made_change)
    }

    /// `is_fully_connected`.
    fn is_fully_connected(&mut self) -> R<()> {
        self.update_parents()?;
        let root = self.root_idx();
        if self.v(root)?.incoming_edges() != 0 {
            return Err(GraphError::OrphanedNodes);
        }
        for i in 0..root {
            if self.v(i)?.incoming_edges() == 0 {
                return Err(GraphError::OrphanedNodes);
            }
        }
        Ok(())
    }

    /// `num_roots_for_space`.
    pub(crate) fn num_roots_for_space(&self, space: u32) -> u32 {
        self.num_roots_for_space
            .get(space as usize)
            .copied()
            .unwrap_or(0)
    }

    /// `next_space`.
    pub(crate) fn next_space(&self) -> u32 {
        u32::try_from(self.num_roots_for_space.len()).unwrap_or(u32::MAX)
    }

    /// `move_to_new_space`.
    pub(crate) fn move_to_new_space(&mut self, indices: &HbSet) -> R<()> {
        if self.next_space() >= MAX_SPACES {
            return Err(GraphError::LimitExceeded);
        }
        self.num_roots_for_space.push(0);
        let new_space = self.next_space().wrapping_sub(1);
        for i in indices.iter() {
            let old = self.v(i)?.space;
            if let Some(n) = self.num_roots_for_space.get_mut(old as usize) {
                *n = n.wrapping_sub(1);
            }
            if let Some(n) = self.num_roots_for_space.get_mut(new_space as usize) {
                *n = n.wrapping_add(1);
            }
            self.vm(i)?.space = new_space;
            self.distance_invalid = true;
            self.positions_invalid = true;
        }
        Ok(())
    }

    /// `space_for`: the space of `index` and the root of that space
    /// (following each vertex's first parent).
    pub(crate) fn space_for(&self, mut index: u32) -> (u32, u32) {
        for _ in 0..=self.len() {
            let Ok(node) = self.v(index) else {
                return (0, index);
            };
            if node.space != 0 {
                return (node.space, index);
            }
            if node.incoming_edges() == 0 {
                return (0, index);
            }
            match node.parents_iter().next() {
                Some(p) => index = p,
                None => return (0, index),
            }
        }
        (0, index)
    }

    /// `total_size_in_bytes`.
    pub(crate) fn total_size_in_bytes(&self) -> usize {
        self.vertices
            .iter()
            .fold(0usize, |a, v| a.wrapping_add(v.table_size()))
    }

    /// `wide_parents`.
    fn wide_parents(&self, nodes: &HbSet, counts: &mut HbMap, parents: &mut HbSet) -> R<()> {
        let mut all_parents = HbSet::new();
        for n in nodes.iter() {
            let v = self.v(n)?;
            counts.set(n, 0);
            v.parents_iter().for_each(|p| all_parents.add(p));
        }
        for p in all_parents.iter() {
            for l in &self.v(p)?.real_links {
                if (l.width == 3 || l.width == 4)
                    && !l.signed
                    && let Some(c) = counts.get_mut(l.objidx)
                {
                    *c = c.wrapping_add(1);
                    parents.add(p);
                }
            }
        }
        Ok(())
    }

    /// `update_parents`.
    pub(crate) fn update_parents(&mut self) -> R<()> {
        if !self.parents_invalid {
            return Ok(());
        }
        let count = self.len();
        self.vertices.iter_mut().for_each(Vertex::reset_parents);
        for p in 0..count {
            let pu = u32::try_from(p).unwrap_or(INVALID);
            let (real, virt): (Vec<u32>, Vec<u32>) = match self.vertices.get(p) {
                Some(v) => (
                    v.real_links.iter().map(|l| l.objidx).collect(),
                    v.virtual_links.iter().map(|l| l.objidx).collect(),
                ),
                None => continue,
            };
            for c in real {
                let v = self
                    .vertices
                    .get_mut(c as usize)
                    .ok_or(GraphError::InvalidArgument)?;
                v.add_parent(pu, false);
            }
            for c in virt {
                let v = self
                    .vertices
                    .get_mut(c as usize)
                    .ok_or(GraphError::InvalidArgument)?;
                v.add_parent(pu, true);
            }
        }
        self.parents_invalid = false;
        Ok(())
    }

    /// `update_positions`.
    pub(crate) fn update_positions(&mut self) {
        if !self.positions_invalid {
            return;
        }
        let mut pos = 0u32;
        for &i in &self.ordering {
            if let Some(v) = self.vertices.get_mut(i as usize) {
                v.start = pos;
                pos = pos.wrapping_add(u32::try_from(v.table_size()).unwrap_or(u32::MAX));
                v.end = pos;
            }
        }
        self.positions_invalid = false;
    }

    /// `update_distances`: Dijkstra from the root.
    fn update_distances(&mut self) -> R<()> {
        if !self.distance_invalid {
            return Ok(());
        }
        self.vertices.iter_mut().for_each(|v| v.distance = i64::MAX);
        let root = self.root_idx();
        self.vm(root)?.distance = 0;
        let mut queue = PriorityQueue::new();
        queue.insert(0, root);
        let mut visited = alloc::vec![false; self.len()];
        while let Some((_, next_idx)) = queue.pop_minimum() {
            if visited.get(next_idx as usize).copied().unwrap_or(true) {
                continue;
            }
            let next = self.v(next_idx)?;
            let next_distance = next.distance;
            let links: Vec<Link> = next
                .real_links
                .iter()
                .chain(next.virtual_links.iter())
                .copied()
                .collect();
            if let Some(v) = visited.get_mut(next_idx as usize) {
                *v = true;
            }
            for link in links {
                let Some(&seen) = visited.get(link.objidx as usize) else {
                    return Err(GraphError::OutOfBounds);
                };
                if seen {
                    continue;
                }
                let child = self.vm(link.objidx)?;
                let width = if link.width == 0 { 4 } else { link.width };
                let size = i64::try_from(child.table_size()).unwrap_or(i64::MAX);
                let weight = size.saturating_add(
                    (1i64 << (u32::from(width) * 8).min(62))
                        .saturating_mul(i64::from(child.space).saturating_add(1)),
                );
                let d = next_distance.saturating_add(weight);
                if d < child.distance {
                    child.distance = d;
                    queue.insert(d, link.objidx);
                }
            }
        }
        self.distance_invalid = false;
        Ok(())
    }

    /// `reassign_link`.
    fn reassign_link(&mut self, parent: u32, at: LinkAt, new_idx: u32) -> R<()> {
        let p = self.vm(parent)?;
        let link = if at.is_virtual {
            p.virtual_links.get_mut(at.i)
        } else {
            p.real_links.get_mut(at.i)
        };
        let Some(link) = link else {
            return Err(GraphError::OutOfBounds);
        };
        let old = link.objidx;
        link.objidx = new_idx;
        self.vm(old)?.remove_parent(parent);
        self.vm(new_idx)?.add_parent(parent, at.is_virtual);
        Ok(())
    }

    /// `remap_obj_indices`.
    fn remap_obj_indices(&mut self, id_map: &HbMap, subgraph: &[u32], only_wide: bool) -> R<()> {
        if id_map.is_empty() {
            return Ok(());
        }
        for &i in subgraph {
            let v = self.v(i)?;
            let num_real = v.real_links.len();
            let total = num_real.wrapping_add(v.virtual_links.len());
            for k in 0..total {
                let v = self.v(i)?;
                let is_virtual = k >= num_real;
                let link = if is_virtual {
                    v.virtual_links.get(k.wrapping_sub(num_real))
                } else {
                    v.real_links.get(k)
                };
                let Some(&link) = link else { break };
                let Some(n) = id_map.get(link.objidx) else {
                    continue;
                };
                if only_wide && (link.signed || (link.width != 4 && link.width != 3)) {
                    continue;
                }
                let at = LinkAt {
                    is_virtual,
                    i: if is_virtual {
                        k.wrapping_sub(num_real)
                    } else {
                        k
                    },
                };
                self.reassign_link(i, at, n)?;
            }
        }
        Ok(())
    }

    /// `find_connected_nodes`: the vertices reachable from `start`
    /// treating links as undirected; members of `targets` met are moved
    /// to `connected`.
    fn find_connected_nodes(
        &self,
        start: u32,
        targets: &mut HbSet,
        visited: &mut HbSet,
        connected: &mut HbSet,
    ) -> R<()> {
        if visited.has(start) {
            return Ok(());
        }
        if start as usize >= self.len() {
            return Err(GraphError::OutOfBounds);
        }
        let cap = self.len();
        let mut stack: Vec<u32> = Vec::new();
        let handle = |n: u32,
                      stack: &mut Vec<u32>,
                      visited: &mut HbSet,
                      targets: &mut HbSet,
                      connected: &mut HbSet|
         -> R<()> {
            visited.add(n);
            if targets.has(n) {
                targets.del(n);
                connected.add(n);
            }
            if stack.len() >= cap {
                return Err(GraphError::CycleDetected);
            }
            stack.push(n);
            Ok(())
        };
        handle(start, &mut stack, visited, targets, connected)?;
        while let Some(n) = stack.pop() {
            let v = self.v(n)?;
            let next: Vec<u32> = v
                .real_links
                .iter()
                .chain(v.virtual_links.iter())
                .map(|l| l.objidx)
                .chain(v.parents_iter())
                .collect();
            for c in next {
                if visited.has(c) {
                    continue;
                }
                handle(c, &mut stack, visited, targets, connected)?;
            }
        }
        Ok(())
    }
}

fn mask_bias(mut l: Link) -> Link {
    l.bias &= 0x3FF_FFFF;
    l
}

/// The offset bytes of `width` at `at`, zeroed (used by `normalize`).
#[cfg(test)]
pub(crate) fn zero_offsets(bytes: &mut [u8], links: &[Link]) {
    for l in links {
        let at = l.position as usize;
        if let Some(s) = bytes.get_mut(at..at.saturating_add(usize::from(l.width))) {
            s.fill(0);
        }
    }
}

#[cfg(test)]
impl Graph {
    /// `normalize`: sorts real links and zeroes the offsets.
    pub(crate) fn normalize(&mut self) {
        for i in 0..self.len() {
            let Some(v) = self.vertices.get_mut(i) else {
                continue;
            };
            v.sort_real_links();
            let (buf, head, tail, links) = (v.buf, v.head, v.tail, v.real_links.clone());
            if let Some(b) = self.bufs.get_mut(buf).and_then(|b| b.get_mut(head..tail)) {
                zero_offsets(b, &links);
            }
        }
    }

    /// `vertex_t::equals`, recursively through the real links.
    fn vertex_equals(&self, a: u32, other: &Graph, b: u32) -> bool {
        if self.bytes(a) != other.bytes(b) {
            return false;
        }
        let (Ok(va), Ok(vb)) = (self.v(a), other.v(b)) else {
            return false;
        };
        va.real_links.len() == vb.real_links.len()
            && va.real_links.iter().zip(&vb.real_links).all(|(la, lb)| {
                la.width == lb.width
                    && la.signed == lb.signed
                    && la.whence == lb.whence
                    && la.position == lb.position
                    && la.bias == lb.bias
                    && self.vertex_equals(la.objidx, other, lb.objidx)
            })
    }

    /// `graph_t::operator==`.
    pub(crate) fn graph_equals(&self, other: &Graph) -> bool {
        self.vertex_equals(self.root_idx(), other, other.root_idx())
    }

    /// The vertex `i`'s set of buffer-sharing peers is not tested; this
    /// prints the order for debugging (`print`).
    pub(crate) fn describe(&self) -> alloc::string::String {
        use core::fmt::Write;
        let mut s = alloc::string::String::new();
        for &id in &self.ordering {
            let Some(v) = self.vertices.get(id as usize) else {
                continue;
            };
            let _ = write!(s, "{id}: {} [", v.table_size());
            for l in &v.real_links {
                let _ = write!(s, "{}, ", l.objidx);
            }
            for l in &v.virtual_links {
                let _ = write!(s, "v{}, ", l.objidx);
            }
            let _ = writeln!(s, "]");
        }
        s
    }
}
