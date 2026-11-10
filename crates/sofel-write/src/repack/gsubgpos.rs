//! HarfBuzz 15.1.0's GSUB and GPOS specializations of the repacker
//! (`src/graph/gsubgpos-graph.hh`, `gsubgpos-context.*`,
//! `split-helpers.hh`): finding the lookups, promoting lookups to
//! extension lookups, and splitting subtables.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use super::graph::{Graph, GraphError, R};
use super::set::HbSet;
use super::{ligature, markbasepos, pairpos};

/// `HB_OT_TAG_GSUB`.
pub(crate) const GSUB: u32 = u32::from_be_bytes(*b"GSUB");
/// `HB_OT_TAG_GPOS`.
pub(crate) const GPOS: u32 = u32::from_be_bytes(*b"GPOS");

/// `ExtensionFormat1::static_size`.
const EXTENSION_SIZE: usize = 8;
/// `Lookup::min_size`.
const LOOKUP_MIN_SIZE: u32 = 6;

/// `gsubgpos_graph_context_t`.
#[derive(Clone, Debug)]
pub(crate) struct Context {
    pub(crate) table_tag: u32,
    pub(crate) lookup_list_index: u32,
    /// The lookups' vertex ids.
    pub(crate) lookups: BTreeSet<u32>,
    pub(crate) subtable_to_extension: BTreeMap<u32, u32>,
    pub(crate) split_subtables: BTreeMap<u32, Vec<u32>>,
}

/// `extension_type`.
fn extension_type(tag: u32) -> u32 {
    match tag {
        GPOS => 9,
        GSUB => 7,
        _ => 0,
    }
}

/// `GSTAR::sanitize` of the root.
fn gstar_sanitize(g: &Graph, root: u32) -> R<()> {
    let len = g.table_size(root);
    if len < 10 {
        return Err(GraphError::SanitizeFailure);
    }
    let version = g.rd(root, 0, 4);
    if version >> 16 != 1 {
        return Err(GraphError::SanitizeFailure);
    }
    let size = 10usize
        .saturating_add(if version >= 0x0001_0001 { 4 } else { 0 })
        .saturating_add(if version >= 0x0001_0002 { 12 } else { 0 });
    if len < size {
        return Err(GraphError::SanitizeFailure);
    }
    Ok(())
}

/// Whether the GSUB/GPOS header uses `lookupList2`.
fn uses_lookup_list2(g: &Graph, root: u32) -> bool {
    g.rd(root, 0, 4) >= 0x0001_0002 && g.rd(root, 22, 4) != 0
}

/// `get_lookup_list_index`.
fn lookup_list_index(g: &Graph) -> R<u32> {
    let root = g.root_idx();
    let field = if uses_lookup_list2(g, root) { 22 } else { 8 };
    g.index_for_offset(root, field)
}

/// `Lookup::sanitize`.
fn lookup_sanitize(g: &Graph, li: u32) -> R<()> {
    let len = g.table_size(li);
    if len < LOOKUP_MIN_SIZE as usize {
        return Err(GraphError::SanitizeFailure);
    }
    if len < lookup_size(g, li) {
        return Err(GraphError::SanitizeFailure);
    }
    Ok(())
}

/// `Lookup::get_size`.
fn lookup_size(g: &Graph, li: u32) -> usize {
    let count = g.rd16(li, 4) as usize;
    6usize
        .saturating_add(count.saturating_mul(2))
        .saturating_add(if use_mark_filtering_set(g, li) { 2 } else { 0 })
}

fn lookup_type(g: &Graph, li: u32) -> u32 {
    g.rd16(li, 0)
}

/// `Lookup::number_of_subtables`.
pub(crate) fn number_of_subtables(g: &Graph, li: u32) -> u32 {
    g.rd16(li, 4)
}

/// `Lookup::is_extension`.
pub(crate) fn is_extension(g: &Graph, li: u32, tag: u32) -> bool {
    lookup_type(g, li) == extension_type(tag)
}

fn use_mark_filtering_set(g: &Graph, li: u32) -> bool {
    g.rd16(li, 2) & 0x0010 != 0
}

fn subtable_position(i: u32) -> u32 {
    LOOKUP_MIN_SIZE.wrapping_add(i.wrapping_mul(2))
}

impl Context {
    /// `gsubgpos_graph_context_t::create`.
    pub(crate) fn create(tag: u32, g: &Graph) -> R<Context> {
        if tag != GPOS && tag != GSUB {
            return Err(GraphError::InvalidArgument);
        }
        let root = g.root_idx();
        if g.is_null(root) {
            return Err(GraphError::InvalidArgument);
        }
        gstar_sanitize(g, root)?;
        let item_size: u32 = if uses_lookup_list2(g, root) { 4 } else { 2 };
        let list = lookup_list_index(g)?;
        if g.is_null(list) {
            return Err(GraphError::InvalidArgument);
        }
        let len = g.table_size(list);
        let count = g.rd16(list, 0);
        if len < 2 || len < (item_size as usize).saturating_mul(count as usize) {
            return Err(GraphError::SanitizeFailure);
        }
        let mut lookups = BTreeSet::new();
        for i in 0..count {
            let pos = 2u32.wrapping_add(item_size.wrapping_mul(i));
            let li = g.index_for_offset(list, pos)?;
            if g.is_null(li) || lookup_sanitize(g, li).is_err() {
                continue;
            }
            lookups.insert(li);
        }
        Ok(Context {
            table_tag: tag,
            lookup_list_index: lookup_list_index(g)?,
            lookups,
            subtable_to_extension: BTreeMap::new(),
            split_subtables: BTreeMap::new(),
        })
    }

    /// `create_node`: a zeroed vertex of `size` bytes.
    pub(crate) fn create_node(g: &mut Graph, size: usize) -> R<u32> {
        g.new_node(Some(alloc::vec![0; size]))
    }

    /// `Lookup::make_extension`.
    pub(crate) fn make_extension(&mut self, g: &mut Graph, li: u32) -> R<()> {
        let ext_type = extension_type(self.table_tag);
        if ext_type == 0 || is_extension(g, li, self.table_tag) {
            return Ok(());
        }
        let mut seen = HbSet::new();
        let mut subtables = Vec::new();
        for i in 0..number_of_subtables(g, li) {
            let st = g.index_for_offset(li, subtable_position(i))?;
            if seen.has(st) {
                continue;
            }
            seen.add(st);
            subtables.push(st);
        }
        for st in subtables {
            self.make_subtable_extension(g, li, st)?;
        }
        g.wr16(li, 0, ext_type);
        Ok(())
    }

    /// `Lookup::create_extension_subtable`.
    fn create_extension_subtable(g: &mut Graph, subtable: u32, kind: u32) -> R<u32> {
        let ext = Context::create_node(g, EXTENSION_SIZE)?;
        g.wr16(ext, 0, 1);
        g.wr16(ext, 2, kind);
        g.wr(ext, 4, 4, 0);
        g.vm(ext)?.add_real_link(4, subtable, 4);
        Ok(ext)
    }

    /// `Lookup::make_subtable_extension`.
    fn make_subtable_extension(&mut self, g: &mut Graph, li: u32, st: u32) -> R<()> {
        let kind = lookup_type(g, li);
        let existing = self.subtable_to_extension.get(&st).copied();
        let ext = match existing {
            Some(e) => e,
            None => {
                let e = Context::create_extension_subtable(g, st, kind)?;
                self.subtable_to_extension.insert(st, e);
                e
            }
        };
        let mut replaced = 0u32;
        for l in &mut g.vm(li)?.real_links {
            if l.objidx == st {
                l.objidx = ext;
                replaced = replaced.wrapping_add(1);
            }
        }
        for _ in 0..replaced {
            g.vm(st)?.remove_parent(li);
            g.vm(ext)?.add_parent(li, false);
        }
        if existing.is_none() {
            g.vm(st)?.add_parent(ext, false);
        }
        Ok(())
    }

    fn is_supported_gsub_type(&self, kind: u32) -> bool {
        self.table_tag == GSUB && kind == 4
    }

    fn is_supported_gpos_type(&self, kind: u32) -> bool {
        self.table_tag == GPOS && (kind == 2 || kind == 4)
    }

    /// `Lookup::split_subtables_if_needed`.
    pub(crate) fn split_subtables_if_needed(&mut self, g: &mut Graph, li: u32) -> R<()> {
        let mut kind = lookup_type(g, li);
        let is_ext = is_extension(g, li, self.table_tag);
        if self.table_tag != GPOS && self.table_tag != GSUB {
            return Ok(());
        }
        if !is_ext && !self.is_supported_gpos_type(kind) && !self.is_supported_gsub_type(kind) {
            return Ok(());
        }
        let mut all_new: Vec<(u32, Vec<u32>)> = Vec::new();
        for i in 0..number_of_subtables(g, li) {
            let Ok(mut st) = g.index_for_offset(li, subtable_position(i)) else {
                continue;
            };
            if is_ext {
                let ext = st;
                if g.is_null(ext) || g.table_size(ext) < EXTENSION_SIZE {
                    continue;
                }
                st = g.index_for_offset(ext, 4)?;
                kind = g.rd16(ext, 2);
                if !self.is_supported_gpos_type(kind) && !self.is_supported_gsub_type(kind) {
                    continue;
                }
            }
            if let Some(done) = self.split_subtables.get(&st) {
                if done.is_empty() {
                    continue;
                }
                all_new.push((i, done.clone()));
            } else {
                let new = match (self.table_tag, kind) {
                    (GPOS, 2) => pairpos::split(self, g, st)?,
                    (GPOS, 4) => markbasepos::split(self, g, st)?,
                    (GSUB, 4) => ligature::split(self, g, st)?,
                    _ => Vec::new(),
                };
                self.split_subtables.insert(st, new.clone());
                if !new.is_empty() {
                    all_new.push((i, new));
                }
            }
        }
        if !all_new.is_empty() {
            self.add_sub_tables(g, li, kind, &all_new)?;
        }
        Ok(())
    }

    /// `Lookup::add_sub_tables`.
    fn add_sub_tables(
        &mut self,
        g: &mut Graph,
        li: u32,
        kind: u32,
        subtable_ids: &[(u32, Vec<u32>)],
    ) -> R<()> {
        let is_ext = is_extension(g, li, self.table_tag);
        fix_existing_subtable_links(g, li, subtable_ids)?;
        let new_count = subtable_ids.iter().fold(0u32, |a, p| {
            a.wrapping_add(u32::try_from(p.1.len()).unwrap_or(0))
        });
        let old = g.bytes(li).to_vec();
        let new_size = old.len().saturating_add(new_count as usize * 2);
        let mut buffer = alloc::vec![0u8; new_size];
        if let Some(d) = buffer.get_mut(..old.len()) {
            d.copy_from_slice(&old);
        }
        if use_mark_filtering_set(g, li)
            && let (Some(d), Some(s)) = (
                buffer.get_mut(new_size.saturating_sub(2)..),
                old.get(old.len().saturating_sub(2)..),
            )
            && d.len() == s.len()
        {
            d.copy_from_slice(s);
        }
        let count = number_of_subtables(g, li);
        g.set_new_buffer(li, buffer);
        g.wr16(li, 4, count.wrapping_add(new_count));
        let mut shift = 0u32;
        for (first, ids) in subtable_ids {
            let mut offset_index = first.wrapping_add(shift).wrapping_add(1);
            shift = shift.wrapping_add(u32::try_from(ids.len()).unwrap_or(0));
            for &id in ids {
                let mut id = id;
                if is_ext {
                    let ext = Context::create_extension_subtable(g, id, kind)?;
                    g.vm(id)?.add_parent(ext, false);
                    id = ext;
                }
                g.vm(li)?
                    .add_real_link(2, id, subtable_position(offset_index));
                offset_index = offset_index.wrapping_add(1);
                g.vm(id)?.add_parent(li, false);
            }
        }
        g.vm(li)?.sort_real_links();
        Ok(())
    }
}

/// `Lookup::fix_existing_subtable_links`.
fn fix_existing_subtable_links(g: &mut Graph, li: u32, subtable_ids: &[(u32, Vec<u32>)]) -> R<()> {
    let v = g.vm(li)?;
    let mut shift = 0u32;
    for (first, ids) in subtable_ids {
        let n = u32::try_from(ids.len()).unwrap_or(0);
        let insert_index = first.wrapping_add(shift);
        let pos_offset = n.wrapping_mul(2);
        let insert_offset = subtable_position(insert_index);
        shift = shift.wrapping_add(n);
        for l in v.real_links.iter_mut().chain(v.virtual_links.iter_mut()) {
            if l.position > insert_offset {
                l.position = l.position.wrapping_add(pos_offset);
            }
        }
    }
    Ok(())
}

/// `actuate_subtable_split`: clones `[split_points[i], split_points[i + 1])`
/// (the last up to `original_count`) and shrinks the original to the part
/// before the first split point.
pub(crate) fn actuate_split<C, S>(
    g: &mut Graph,
    split_points: &[u32],
    original_count: u32,
    mut clone_range: C,
    shrink: S,
) -> R<Vec<u32>>
where
    C: FnMut(&mut Graph, u32, u32) -> R<u32>,
    S: FnOnce(&mut Graph, u32) -> R<()>,
{
    let Some(&first) = split_points.first() else {
        return Ok(Vec::new());
    };
    let mut new_objects = Vec::new();
    for (i, &start) in split_points.iter().enumerate() {
        let end = split_points
            .get(i.wrapping_add(1))
            .copied()
            .unwrap_or(original_count);
        new_objects.push(clone_range(g, start, end)?);
    }
    shrink(g, first)?;
    Ok(new_objects)
}
