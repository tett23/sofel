//! HarfBuzz 15.1.0's object serializer (`src/hb-serialize.hh`).
//!
//! A table is built as a graph of objects joined by offsets. Objects are
//! built on a stack: [`Serializer::push`] starts a child, and
//! [`Serializer::pop_pack`] finishes it and returns its index, which the
//! parent then links to with [`Serializer::add_link`]. Finished objects
//! with the same bytes and links are merged. [`Serializer::end`] packs
//! the root; [`Packed::resolve`] lays the objects out (the last packed
//! first) and writes the offsets.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// An object's index among the packed objects; 0 is the null object.
pub type ObjIdx = u32;

/// What an offset is measured from (`hb_serialize_context_t::whence_t`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Whence {
    /// From the start of the object holding the offset.
    #[default]
    Head,
    /// From the end of the object holding the offset.
    Tail,
    /// From the start of the serialized table.
    Absolute,
}

/// A link from an object to a child (`object_t::link_t`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Link {
    /// The offset's width in bytes: 2, 3 or 4; 0 for a virtual link, which
    /// only orders objects.
    pub width: u8,
    /// Whether the offset is signed (widths 2 and 4).
    pub signed: bool,
    /// What the offset is measured from.
    pub whence: Whence,
    /// Subtracted from the offset.
    pub bias: u32,
    /// Where the offset is in the parent object.
    pub position: u32,
    /// The child.
    pub objidx: ObjIdx,
}

/// A packed object: its bytes and its links.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Object {
    /// The object's bytes, with its offsets still zero.
    pub bytes: Vec<u8>,
    /// The links that write offsets.
    pub real_links: Vec<Link>,
    /// The links that only order objects: the child is laid out after the
    /// parent.
    pub virtual_links: Vec<Link>,
}

/// The serializer's error bits (`hb_serialize_error_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Errors(pub u8);

impl Errors {
    /// No error.
    pub const NONE: Errors = Errors(0);
    /// Any other error.
    pub const OTHER: Errors = Errors(1);
    /// An offset does not fit in its field.
    pub const OFFSET_OVERFLOW: Errors = Errors(2);
    /// Out of room (never raised: the buffer grows).
    pub const OUT_OF_ROOM: Errors = Errors(4);
    /// An integer does not fit in its field.
    pub const INT_OVERFLOW: Errors = Errors(8);
    /// An array's length does not fit in its field.
    pub const ARRAY_OVERFLOW: Errors = Errors(16);

    /// Whether any bit of `e` is set.
    pub fn has(self, e: Errors) -> bool {
        self.0 & e.0 != 0
    }
}

/// A saved state of the serializer (`snapshot_t`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Snapshot {
    depth: usize,
    len: usize,
    real_links: usize,
    virtual_links: usize,
    packed: usize,
    errors: Errors,
}

#[derive(Clone, Debug, Default)]
struct Frame {
    obj: Object,
    packed_at_push: usize,
}

type Key = (Vec<u8>, Vec<Link>);

/// The serializer (`hb_serialize_context_t`). It starts with the root
/// object pushed.
#[derive(Clone, Debug)]
pub struct Serializer {
    stack: Vec<Frame>,
    packed: Vec<Object>,
    keys: Vec<Option<Key>>,
    map: BTreeMap<Key, ObjIdx>,
    errors: Errors,
    packed_bytes: usize,
    peak: usize,
}

impl Default for Serializer {
    fn default() -> Self {
        Serializer::new()
    }
}

/// The objects of a finished serialization.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Packed {
    /// Only the root, which was not packed (`end_serialize` packs only
    /// when there are other objects).
    Root(Vec<u8>),
    /// The packed objects; index 0 is the null object and the last is the
    /// root.
    Graph(Vec<Object>),
}

impl Serializer {
    /// A serializer with the root object pushed (`start_serialize`).
    pub fn new() -> Serializer {
        Serializer {
            stack: alloc::vec![Frame::default()],
            packed: alloc::vec![Object::default()],
            keys: alloc::vec![None],
            map: BTreeMap::new(),
            errors: Errors::NONE,
            packed_bytes: 0,
            peak: 0,
        }
    }

    /// The error bits.
    pub fn errors(&self) -> Errors {
        self.errors
    }

    /// Whether an error happened.
    pub fn in_error(&self) -> bool {
        self.errors != Errors::NONE
    }

    /// Whether the only errors are overflows, after which the stack is
    /// still kept (`only_overflow`).
    pub fn only_overflow(&self) -> bool {
        self.errors == Errors::OFFSET_OVERFLOW
            || self.errors == Errors::INT_OVERFLOW
            || self.errors == Errors::ARRAY_OVERFLOW
    }

    /// Sets error bits (`err`); returns whether there is no error.
    pub fn err(&mut self, e: Errors) -> bool {
        self.errors = Errors(self.errors.0 | e.0);
        !self.in_error()
    }

    /// `check_success`: false and sets `e` when `ok` is false; false when
    /// already in error.
    pub fn check_success(&mut self, ok: bool, e: Errors) -> bool {
        !self.in_error() && (ok || self.err(e))
    }

    fn stopped(&self) -> bool {
        self.in_error() && !self.only_overflow()
    }

    fn top(&mut self) -> Option<&mut Object> {
        self.stack.last_mut().map(|f| &mut f.obj)
    }

    /// The current object's bytes.
    pub fn current(&self) -> &[u8] {
        self.stack.last().map_or(&[], |f| f.obj.bytes.as_slice())
    }

    /// The current object's length (`length`).
    pub fn len(&self) -> usize {
        self.current().len()
    }

    /// Whether the current object is empty.
    pub fn is_empty(&self) -> bool {
        self.current().is_empty()
    }

    /// The total length of the packed objects (the tail of HarfBuzz's
    /// buffer: `end - tail`).
    pub fn packed_len(&self) -> usize {
        self.packed_bytes
    }

    /// The number of objects on the stack.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The most bytes the serializer held at once: the objects on the
    /// stack and the packed objects, at their highest. This is the room
    /// HarfBuzz's buffer needs (`head - start` plus `end - tail`), which
    /// runs out when it is smaller (`HB_SERIALIZE_ERROR_OUT_OF_ROOM`).
    pub fn peak_len(&self) -> usize {
        self.peak
    }

    /// Records the bytes held now in [`peak_len`](Self::peak_len).
    fn note_room(&mut self) {
        let held = self
            .stack
            .iter()
            .map(|f| f.obj.bytes.len())
            .fold(self.packed_bytes, usize::saturating_add);
        self.peak = self.peak.max(held);
    }

    /// Appends bytes to the current object and returns where they start.
    /// Nothing happens in error.
    pub fn extend(&mut self, bytes: &[u8]) -> usize {
        if self.in_error() {
            return self.len();
        }
        let at = match self.top() {
            Some(o) => {
                let at = o.bytes.len();
                o.bytes.extend_from_slice(bytes);
                at
            }
            None => return 0,
        };
        self.note_room();
        at
    }

    /// Appends `n` zeros (`allocate_size`) and returns where they start.
    pub fn zeros(&mut self, n: usize) -> usize {
        if self.in_error() {
            return self.len();
        }
        let at = match self.top() {
            Some(o) => {
                let at = o.bytes.len();
                o.bytes.resize(at.saturating_add(n), 0);
                at
            }
            None => return 0,
        };
        self.note_room();
        at
    }

    /// Appends a `u8`.
    pub fn u8(&mut self, v: u8) -> usize {
        self.extend(&[v])
    }
    /// Appends a big-endian `u16`.
    pub fn u16(&mut self, v: u16) -> usize {
        self.extend(&v.to_be_bytes())
    }
    /// Appends a big-endian `i16`.
    pub fn i16(&mut self, v: i16) -> usize {
        self.extend(&v.to_be_bytes())
    }
    /// Appends a big-endian 24-bit value.
    pub fn u24(&mut self, v: u32) -> usize {
        let b = v.to_be_bytes();
        self.extend(b.get(1..).unwrap_or(&[]))
    }
    /// Appends a big-endian `u32`.
    pub fn u32(&mut self, v: u32) -> usize {
        self.extend(&v.to_be_bytes())
    }

    /// Pads the current object with zeros to a multiple of `alignment`
    /// (`align`).
    pub fn align(&mut self, alignment: usize) {
        let r = self.len().checked_rem(alignment).unwrap_or(0);
        if r != 0 {
            self.zeros(alignment.saturating_sub(r));
        }
    }

    /// Overwrites bytes of the current object at `at`.
    pub fn write_at(&mut self, at: usize, bytes: &[u8]) {
        if self.in_error() {
            return;
        }
        if let Some(s) = self
            .top()
            .and_then(|o| o.bytes.get_mut(at..at.saturating_add(bytes.len())))
        {
            s.copy_from_slice(bytes);
        }
    }

    /// Overwrites a big-endian `u16` of the current object.
    pub fn set_u16(&mut self, at: usize, v: u16) {
        self.write_at(at, &v.to_be_bytes());
    }

    /// Overwrites a big-endian `u32` of the current object.
    pub fn set_u32(&mut self, at: usize, v: u32) {
        self.write_at(at, &v.to_be_bytes());
    }

    /// Overwrites a big-endian 24-bit value of the current object.
    pub fn set_u24(&mut self, at: usize, v: u32) {
        let b = v.to_be_bytes();
        self.write_at(at, b.get(1..).unwrap_or(&[]));
    }

    /// `check_assign` of a `u16` field: writes `v` truncated and sets `e`
    /// when it does not fit. Returns whether it fit.
    pub fn check_assign_u16(&mut self, at: usize, v: u64, e: Errors) -> bool {
        self.set_u16(at, (v & 0xFFFF) as u16);
        if v > 0xFFFF {
            return self.err(e);
        }
        true
    }

    /// `check_assign` of a 24-bit field.
    pub fn check_assign_u24(&mut self, at: usize, v: u64, e: Errors) -> bool {
        self.set_u24(at, (v & 0xFF_FFFF) as u32);
        if v > 0xFF_FFFF {
            return self.err(e);
        }
        true
    }

    /// `check_assign` of a `u32` field.
    pub fn check_assign_u32(&mut self, at: usize, v: u64, e: Errors) -> bool {
        self.set_u32(at, (v & 0xFFFF_FFFF) as u32);
        if v > 0xFFFF_FFFF {
            return self.err(e);
        }
        true
    }

    /// Starts a child object (`push`).
    pub fn push(&mut self) {
        if self.stopped() {
            return;
        }
        let packed_at_push = self.packed.len();
        self.stack.push(Frame {
            obj: Object::default(),
            packed_at_push,
        });
    }

    /// Drops the current object and every object packed since it was
    /// pushed (`pop_discard`).
    pub fn pop_discard(&mut self) {
        if self.stack.len() <= 1 || self.stopped() {
            return;
        }
        if let Some(f) = self.stack.pop() {
            self.truncate_packed(f.packed_at_push);
        }
    }

    /// Finishes the current object and returns its index, or 0 when it is
    /// empty (`pop_pack`). With `share`, an already packed object with the
    /// same bytes and links is returned instead, and the virtual links are
    /// added to it.
    pub fn pop_pack(&mut self, share: bool) -> ObjIdx {
        if self.stack.is_empty() || self.stopped() {
            return 0;
        }
        let Some(frame) = self.stack.pop() else {
            return 0;
        };
        let obj = frame.obj;
        if obj.bytes.is_empty() {
            return 0;
        }
        if share {
            let key = (obj.bytes, obj.real_links);
            if let Some(&idx) = self.map.get(&key) {
                if let Some(to) = self.packed.get_mut(idx as usize) {
                    to.virtual_links.extend_from_slice(&obj.virtual_links);
                }
                return idx;
            }
            let idx = u32::try_from(self.packed.len()).unwrap_or(u32::MAX);
            self.packed_bytes = self.packed_bytes.saturating_add(key.0.len());
            self.packed.push(Object {
                bytes: key.0.clone(),
                real_links: key.1.clone(),
                virtual_links: obj.virtual_links,
            });
            self.map.insert(key.clone(), idx);
            self.keys.push(Some(key));
            idx
        } else {
            let idx = u32::try_from(self.packed.len()).unwrap_or(u32::MAX);
            self.packed_bytes = self.packed_bytes.saturating_add(obj.bytes.len());
            self.packed.push(obj);
            self.keys.push(None);
            idx
        }
    }

    fn truncate_packed(&mut self, len: usize) {
        while self.packed.len() > len.max(1) {
            if let Some(o) = self.packed.pop() {
                self.packed_bytes = self.packed_bytes.saturating_sub(o.bytes.len());
            }
            if let Some(Some(key)) = self.keys.pop() {
                self.map.remove(&key);
            }
        }
    }

    /// Saves the state (`snapshot`).
    pub fn snapshot(&self) -> Snapshot {
        let top = self.stack.last();
        Snapshot {
            depth: self.stack.len(),
            len: top.map_or(0, |f| f.obj.bytes.len()),
            real_links: top.map_or(0, |f| f.obj.real_links.len()),
            virtual_links: top.map_or(0, |f| f.obj.virtual_links.len()),
            packed: self.packed.len(),
            errors: self.errors,
        }
    }

    /// Goes back to a saved state (`revert`): the current object's bytes
    /// and links after it, and the objects packed after it, are dropped,
    /// and the errors are those of the snapshot.
    pub fn revert(&mut self, s: Snapshot) {
        if self.stopped() {
            return;
        }
        self.stack.truncate(s.depth.max(1));
        if let Some(f) = self.stack.last_mut() {
            f.obj.bytes.truncate(s.len);
            f.obj.real_links.truncate(s.real_links);
            f.obj.virtual_links.truncate(s.virtual_links);
        }
        self.errors = s.errors;
        self.truncate_packed(s.packed);
    }

    /// Adds a link from the field at `position` of the current object to
    /// `objidx` (`add_link`). `width` is 2, 3 or 4 bytes. Nothing happens
    /// for the null object.
    pub fn add_link(
        &mut self,
        position: usize,
        width: u8,
        signed: bool,
        objidx: ObjIdx,
        whence: Whence,
        bias: u32,
    ) {
        if self.in_error() || objidx == 0 {
            return;
        }
        let link = Link {
            width,
            signed,
            whence,
            bias,
            position: u32::try_from(position).unwrap_or(u32::MAX),
            objidx,
        };
        if let Some(o) = self.top() {
            o.real_links.push(link);
        }
    }

    /// [`add_link`](Self::add_link) for an unsigned 16-bit offset from the
    /// object's start, the most common form.
    pub fn link16(&mut self, position: usize, objidx: ObjIdx) {
        self.add_link(position, 2, false, objidx, Whence::Head, 0);
    }

    /// An unsigned 24-bit offset from the object's start.
    pub fn link24(&mut self, position: usize, objidx: ObjIdx) {
        self.add_link(position, 3, false, objidx, Whence::Head, 0);
    }

    /// An unsigned 32-bit offset from the object's start.
    pub fn link32(&mut self, position: usize, objidx: ObjIdx) {
        self.add_link(position, 4, false, objidx, Whence::Head, 0);
    }

    /// Adds a virtual link from the current object to `objidx`
    /// (`add_virtual_link`).
    pub fn add_virtual_link(&mut self, objidx: ObjIdx) {
        if self.in_error() || objidx == 0 {
            return;
        }
        if let Some(o) = self.top() {
            o.virtual_links.push(Link {
                objidx,
                ..Link::default()
            });
        }
    }

    /// The child of the current object's last link (`last_added_child_index`).
    pub fn last_added_child_index(&self) -> Option<ObjIdx> {
        self.stack
            .last()
            .and_then(|f| f.obj.real_links.last())
            .map(|l| l.objidx)
    }

    /// Keeps `objidx` after the current object's other children
    /// (`repack_last`): adds a virtual link to it from each.
    pub fn repack_last(&mut self, objidx: ObjIdx) {
        if self.in_error() || objidx == 0 {
            return;
        }
        let children: Vec<ObjIdx> = self
            .stack
            .last()
            .map(|f| f.obj.real_links.iter().map(|l| l.objidx).collect())
            .unwrap_or_default();
        for c in children.into_iter().filter(|&c| c != objidx) {
            if let Some(o) = self.packed.get_mut(c as usize) {
                o.virtual_links.push(Link {
                    objidx,
                    ..Link::default()
                });
            }
        }
    }

    /// The packed object `objidx`.
    pub fn object(&self, objidx: ObjIdx) -> Option<&Object> {
        self.packed.get(objidx as usize)
    }

    /// Finishes the serialization (`end_serialize`). The root is packed
    /// (not shared) when there are other objects.
    ///
    /// # Errors
    ///
    /// The error bits when serialization failed; an offset overflow
    /// before the links are resolved is reported as [`Errors::OTHER`].
    pub fn end(mut self) -> Result<Packed, Errors> {
        if self.in_error() {
            if self.errors.has(Errors::OFFSET_OVERFLOW) {
                self.err(Errors::OTHER);
            }
            return Err(self.errors);
        }
        if self.packed.len() <= 1 {
            let root = self
                .stack
                .into_iter()
                .next()
                .map(|f| f.obj.bytes)
                .unwrap_or_default();
            return Ok(Packed::Root(root));
        }
        self.stack.truncate(1);
        self.pop_pack(false);
        Ok(Packed::Graph(self.packed))
    }
}

impl Packed {
    /// The objects, index 0 being the null object (an empty list for
    /// [`Packed::Root`]).
    pub fn objects(&self) -> &[Object] {
        match self {
            Packed::Root(_) => &[],
            Packed::Graph(o) => o,
        }
    }

    /// Lays the objects out, the last packed first, and writes the offsets
    /// (`resolve_links` and `copy_bytes`).
    ///
    /// # Errors
    ///
    /// [`Errors::OFFSET_OVERFLOW`] when an offset does not fit; the caller
    /// then repacks the graph.
    pub fn resolve(&self) -> Result<Vec<u8>, Errors> {
        match self {
            Packed::Root(b) => Ok(b.clone()),
            Packed::Graph(objs) => resolve_in_order(objs),
        }
    }
}

/// Lays `objs` (index 0 the null object) out from the last to the first
/// and writes the offsets of their real links.
fn resolve_in_order(objs: &[Object]) -> Result<Vec<u8>, Errors> {
    let mut starts = alloc::vec![0usize; objs.len()];
    let mut out = Vec::with_capacity(objs.iter().map(|o| o.bytes.len()).sum());
    for (i, o) in objs.iter().enumerate().skip(1).rev() {
        if let Some(s) = starts.get_mut(i) {
            *s = out.len();
        }
        out.extend_from_slice(&o.bytes);
    }
    let mut overflow = false;
    for (i, o) in objs.iter().enumerate().skip(1) {
        let head = starts.get(i).copied().unwrap_or(0);
        let tail = head.saturating_add(o.bytes.len());
        for l in &o.real_links {
            let Some(child) = starts
                .get(l.objidx as usize)
                .copied()
                .filter(|_| l.objidx != 0)
            else {
                return Err(Errors::OTHER);
            };
            let base = match l.whence {
                Whence::Head => head,
                Whence::Tail => tail,
                Whence::Absolute => 0,
            };
            let to32 = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
            let offset = to32(child).wrapping_sub(to32(base));
            let offset = offset.wrapping_sub(l.bias);
            let at = head.saturating_add(l.position as usize);
            overflow |= !write_offset(&mut out, at, l.width, l.signed, offset);
        }
    }
    if overflow {
        Err(Errors::OFFSET_OVERFLOW)
    } else {
        Ok(out)
    }
}

/// Writes `offset` into a field; false when it does not fit (the field
/// then holds the truncated value, as HarfBuzz's `check_assign`).
pub(crate) fn write_offset(
    out: &mut [u8],
    at: usize,
    width: u8,
    signed: bool,
    offset: u32,
) -> bool {
    let w = usize::from(width);
    let b = offset.to_be_bytes();
    let fits = match (width, signed) {
        (2, false) => offset <= 0xFFFF,
        (3, false) => offset <= 0xFF_FFFF,
        (4, false) => true,
        (2, true) => offset <= 0x7FFF,
        (4, true) => offset <= 0x7FFF_FFFF,
        _ => return width == 0,
    };
    if let (Some(dst), Some(src)) = (
        out.get_mut(at..at.saturating_add(w)),
        b.get(4usize.saturating_sub(w)..),
    ) {
        dst.copy_from_slice(src);
    }
    fits
}
