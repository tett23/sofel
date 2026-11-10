//! The `GDEF` (glyph definition) table, versions 1.0, 1.2, 1.3 and
//! HarfBuzz's 1.4 (ADR 0020).
//!
//! The table classifies glyphs ([`Gdef::glyph_class_def`],
//! [`glyph_class`]), lists attachment points ([`AttachList`]) and
//! ligature carets ([`LigCaretList`]), and defines mark attachment
//! classes and mark glyph sets ([`MarkGlyphSets`]). Version 1.3 adds the
//! offset to an ItemVariationStore, which is kept but not read. Version
//! 1.4 (HarfBuzz, beyond 64K glyphs) adds 32-bit offsets that replace
//! the 16-bit ones when not null, as HarfBuzz 15.1.0 reads them.
//!
//! [`is_blocklisted`] is HarfBuzz's list of broken `GDEF` tables of
//! particular fonts, recognized by the lengths of `GDEF`, `GSUB` and
//! `GPOS`.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/gdef>

use crate::be::{self, to_u32, to_usize};
use crate::device::Device;
use crate::layout::walk::Walker;
use crate::layout::{ClassDef, Coverage, counted, follow, need, uint};
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

/// The `GDEF` tag.
pub const TAG: Tag = Tag::new(b"GDEF");
/// Version 1.0.
pub const VERSION_1_0: u32 = 0x0001_0000;
/// Version 1.2, which adds MarkGlyphSetsDef.
pub const VERSION_1_2: u32 = 0x0001_0002;
/// Version 1.3, which adds the ItemVariationStore offset.
pub const VERSION_1_3: u32 = 0x0001_0003;
/// Version 1.4 (HarfBuzz), which adds 32-bit offsets.
pub const VERSION_1_4: u32 = 0x0001_0004;

/// The glyph classes of `glyphClassDef`.
pub mod glyph_class {
    /// A base glyph (single character, spacing glyph).
    pub const BASE: u32 = 1;
    /// A ligature glyph (multiple characters, spacing glyph).
    pub const LIGATURE: u32 = 2;
    /// A mark glyph (non-spacing combining glyph).
    pub const MARK: u32 = 3;
    /// A component glyph (part of a single character, spacing glyph).
    pub const COMPONENT: u32 = 4;
}

/// The size of the header of `version`.
fn header_size(version: u32) -> usize {
    if version >= VERSION_1_4 {
        38
    } else if version >= VERSION_1_3 {
        18
    } else if version >= VERSION_1_2 {
        14
    } else {
        12
    }
}

/// A view over the `GDEF` header.
#[derive(Clone, Copy, Debug)]
pub struct Gdef<'a> {
    data: &'a [u8],
    version: u32,
}

impl<'a> Gdef<'a> {
    /// A view over the header.
    ///
    /// Fails with [`ReadError::UnknownVersion`] if `majorVersion` is not
    /// 1, and with [`ReadError::Truncated`] if the header of the version
    /// does not fit.
    pub fn new(data: &'a [u8]) -> Result<Gdef<'a>, ReadError> {
        let version = be::u32_at(data, 0).ok_or(ReadError::truncated(4, data.len()))?;
        if version >> 16 != 1 {
            return Err(ReadError::UnknownVersion(version));
        }
        need(data, header_size(version))?;
        Ok(Gdef { data, version })
    }
    /// The bytes of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `majorVersion` and `minorVersion` as one value.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The offset of the 16-bit field at `narrow`, or of the version 1.4
    /// 32-bit field at `wide` when it is not null.
    fn offset(&self, narrow: usize, wide: usize) -> u32 {
        if self.version >= VERSION_1_4
            && let Some(o) = be::u32_at(self.data, wide)
            && o != 0
        {
            return o;
        }
        be::u16_at(self.data, narrow).map_or(0, u32::from)
    }
    /// The offset of the glyph class definition (`glyphClassDefOffset`,
    /// or `glyphClassDef2`).
    pub fn glyph_class_def_offset(&self) -> u32 {
        self.offset(4, 18)
    }
    /// The offset of the attachment point list.
    pub fn attach_list_offset(&self) -> u32 {
        self.offset(6, 22)
    }
    /// The offset of the ligature caret list.
    pub fn lig_caret_list_offset(&self) -> u32 {
        self.offset(8, 26)
    }
    /// Whether the ligature caret list is the version 1.4 one with 24-bit
    /// counts and offsets and a 32-bit Coverage offset.
    fn wide_lig_caret_list(&self) -> bool {
        self.version >= VERSION_1_4 && be::u32_at(self.data, 26).is_some_and(|o| o != 0)
    }
    /// The offset of the mark attachment class definition.
    pub fn mark_attach_class_def_offset(&self) -> u32 {
        self.offset(10, 30)
    }
    /// The offset of the mark glyph sets (version 1.2 on; 0 before).
    pub fn mark_glyph_sets_offset(&self) -> u32 {
        if self.version < VERSION_1_2 {
            return 0;
        }
        self.offset(12, 34)
    }

    /// Whether the table classifies glyphs (HarfBuzz
    /// `has_glyph_classes`: the offset is not null).
    pub fn has_glyph_classes(&self) -> bool {
        self.glyph_class_def_offset() != 0
    }
    /// The glyph classes ([`glyph_class`]).
    pub fn glyph_class_def(&self) -> Result<Option<ClassDef<'a>>, ReadError> {
        follow(self.data, self.glyph_class_def_offset(), ClassDef::new)
    }
    /// The mark attachment classes.
    pub fn mark_attach_class_def(&self) -> Result<Option<ClassDef<'a>>, ReadError> {
        follow(
            self.data,
            self.mark_attach_class_def_offset(),
            ClassDef::new,
        )
    }
    /// The attachment point list.
    pub fn attach_list(&self) -> Result<Option<AttachList<'a>>, ReadError> {
        follow(self.data, self.attach_list_offset(), AttachList::new)
    }
    /// The ligature caret list.
    pub fn lig_caret_list(&self) -> Result<Option<LigCaretList<'a>>, ReadError> {
        let wide = self.wide_lig_caret_list();
        follow(self.data, self.lig_caret_list_offset(), |d| {
            LigCaretList::new(d, wide)
        })
    }
    /// The mark glyph sets (version 1.2 on).
    pub fn mark_glyph_sets(&self) -> Result<Option<MarkGlyphSets<'a>>, ReadError> {
        follow(self.data, self.mark_glyph_sets_offset(), MarkGlyphSets::new)
    }
    /// `itemVarStoreOffset` (version 1.3 on); `None` before. The store is
    /// not read (ADR 0020).
    pub fn item_var_store_offset(&self) -> Option<u32> {
        if self.version >= VERSION_1_3 {
            be::u32_at(self.data, 14)
        } else {
            None
        }
    }
}

/// HarfBuzz's `GDEF::is_blocklisted`: whether the `GDEF` table of a font
/// is one HarfBuzz ignores because it misclassifies glyphs, recognized
/// by the lengths in bytes of `GDEF`, `GSUB` and `GPOS` (0 for a missing
/// table). The fonts are versions of Times New Roman Italic and Bold
/// Italic, Tahoma, Microsoft Himalaya, Cantarell, Padauk and Courier New.
pub fn is_blocklisted(gdef_len: usize, gsub_len: usize, gpos_len: usize) -> bool {
    BLOCKLIST.contains(&encode3(gdef_len, gsub_len, gpos_len))
}

/// HarfBuzz's `HB_CODEPOINT_ENCODE3` on 64 bits.
fn encode3(x: usize, y: usize, z: usize) -> u64 {
    let w = |v: usize| u64::try_from(v).unwrap_or(u64::MAX);
    w(x).wrapping_shl(42) | w(y).wrapping_shl(21) | w(z)
}

const fn e3(x: u64, y: u64, z: u64) -> u64 {
    (x << 42) | (y << 21) | z
}

/// The `(GDEF, GSUB, GPOS)` lengths of HarfBuzz 15.1.0's blocklist
/// (`hb-ot-layout.cc`).
const BLOCKLIST: [u64; 40] = [
    e3(442, 2874, 42038),
    e3(430, 2874, 40662),
    e3(442, 2874, 39116),
    e3(430, 2874, 39374),
    e3(490, 3046, 41638),
    e3(478, 3046, 41902),
    e3(898, 12554, 46470),
    e3(910, 12566, 47732),
    e3(928, 23298, 59332),
    e3(940, 23310, 60732),
    e3(964, 23836, 60072),
    e3(976, 23832, 61456),
    e3(994, 24474, 60336),
    e3(1006, 24470, 61740),
    e3(1006, 24576, 61346),
    e3(1018, 24572, 62828),
    e3(1006, 24576, 61352),
    e3(1018, 24572, 62834),
    e3(832, 7324, 47162),
    e3(844, 7302, 45474),
    e3(180, 13054, 7254),
    e3(192, 12638, 7254),
    e3(192, 12690, 7254),
    e3(188, 248, 3852),
    e3(188, 264, 3426),
    e3(1058, 47032, 11818),
    e3(1046, 47030, 12600),
    e3(1058, 71796, 16770),
    e3(1046, 71790, 17862),
    e3(1046, 71788, 17112),
    e3(1058, 71794, 17514),
    e3(1330, 109904, 57938),
    e3(1330, 109904, 58972),
    e3(1004, 59092, 14836),
    e3(588, 5078, 14418),
    e3(588, 5078, 14238),
    e3(894, 17162, 33960),
    e3(894, 17154, 34472),
    e3(816, 7868, 17052),
    e3(816, 7868, 17138),
];

/// A view over an AttachList table.
#[derive(Clone, Copy, Debug)]
pub struct AttachList<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
}

impl<'a> AttachList<'a> {
    /// A view over the Coverage offset, the count and the offsets. Fails
    /// with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<AttachList<'a>, ReadError> {
        let (_, offsets) = counted(data, 2, 2, 2)?;
        Ok(AttachList { data, offsets })
    }
    /// The coverage of the glyphs with attachment points.
    pub fn coverage(&self) -> Result<Option<Coverage<'a>>, ReadError> {
        follow(
            self.data,
            be::u16_at(self.data, 0).map_or(0, u32::from),
            Coverage::new,
        )
    }
    /// The number of AttachPoint tables.
    pub fn len(&self) -> usize {
        self.offsets.len() / 2
    }
    /// Whether there are no AttachPoint tables.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The contour point indices of the glyph with coverage index `index`;
    /// [`ReadError::OutOfBounds`] for an index past the offsets.
    pub fn points(&self, index: usize) -> Result<Option<AttachPoints<'a>>, ReadError> {
        let o = be::u16_at(self.offsets, index.saturating_mul(2)).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, u32::from(o), |d| {
            counted(d, 0, 2, 2).map(|(_, v)| AttachPoints { data: v })
        })
    }
    /// The contour point indices of `glyph`; `Ok(None)` when it is not
    /// covered.
    pub fn points_of(&self, glyph: GlyphId) -> Result<Option<AttachPoints<'a>>, ReadError> {
        let Some(c) = self.coverage()? else {
            return Ok(None);
        };
        match c.get(glyph) {
            Some(i) => self.points(to_usize(i)),
            None => Ok(None),
        }
    }
}

/// The contour point indices of an AttachPoint table.
#[derive(Clone, Copy, Debug)]
pub struct AttachPoints<'a> {
    data: &'a [u8],
}

impl<'a> AttachPoints<'a> {
    /// The number of values.
    pub fn len(&self) -> usize {
        self.data.len() / 2
    }
    /// Whether there are no values.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    /// Value `i`.
    pub fn get(&self, i: usize) -> Option<u16> {
        be::u16_at(self.data, i.checked_mul(2)?)
    }
}

/// A view over a LigCaretList table: 16-bit offsets and count, or (in
/// the version 1.4 `ligCaretList2`) a 32-bit Coverage offset and 24-bit
/// count and LigGlyph offsets.
#[derive(Clone, Copy, Debug)]
pub struct LigCaretList<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
    wide: bool,
}

impl<'a> LigCaretList<'a> {
    /// A view over the Coverage offset, the count and the offsets. Fails
    /// with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8], wide: bool) -> Result<LigCaretList<'a>, ReadError> {
        let (at, w) = if wide { (4, 3) } else { (2, 2) };
        let (_, offsets) = counted(data, at, w, w)?;
        Ok(LigCaretList {
            data,
            offsets,
            wide,
        })
    }
    fn width(&self) -> usize {
        if self.wide { 3 } else { 2 }
    }
    /// The coverage of the ligatures with carets.
    pub fn coverage(&self) -> Result<Option<Coverage<'a>>, ReadError> {
        let o = uint(self.data, 0, if self.wide { 4 } else { 2 }).unwrap_or(0);
        follow(self.data, o, Coverage::new)
    }
    /// The number of LigGlyph tables.
    pub fn len(&self) -> usize {
        self.offsets.len().checked_div(self.width()).unwrap_or(0)
    }
    /// Whether there are no LigGlyph tables.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The LigGlyph table of coverage index `index`;
    /// [`ReadError::OutOfBounds`] for an index past the offsets.
    pub fn lig_glyph(&self, index: usize) -> Result<Option<LigGlyph<'a>>, ReadError> {
        let w = self.width();
        let o = uint(self.offsets, index.saturating_mul(w), w).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, o, LigGlyph::new)
    }
}

/// A view over a LigGlyph table: the carets of one ligature.
#[derive(Clone, Copy, Debug)]
pub struct LigGlyph<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
}

impl<'a> LigGlyph<'a> {
    /// A view over the count and the CaretValue offsets. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<LigGlyph<'a>, ReadError> {
        let (_, offsets) = counted(data, 0, 2, 2)?;
        Ok(LigGlyph { data, offsets })
    }
    /// The number of carets.
    pub fn len(&self) -> usize {
        self.offsets.len() / 2
    }
    /// Whether there are no carets.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// Caret `i`; [`ReadError::OutOfBounds`] for an index past the
    /// offsets.
    pub fn caret(&self, i: usize) -> Result<Option<CaretValue<'a>>, ReadError> {
        let o = be::u16_at(self.offsets, i.saturating_mul(2)).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, u32::from(o), CaretValue::new)
    }
}

/// A CaretValue table.
#[derive(Clone, Copy, Debug)]
pub enum CaretValue<'a> {
    /// Format 1: a coordinate in font units.
    Coordinate(i16),
    /// Format 2: a contour point index on the ligature glyph.
    Point(u16),
    /// Format 3: a coordinate and a Device table (`None` for a null
    /// offset or one that cannot be read).
    Device(i16, Option<Device<'a>>),
}

impl<'a> CaretValue<'a> {
    /// A view over the table. Fails with [`ReadError::UnknownFormat`]
    /// for a format other than 1 to 3, and with [`ReadError::Truncated`]
    /// if it does not fit.
    pub fn new(data: &'a [u8]) -> Result<CaretValue<'a>, ReadError> {
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        match format {
            1 => {
                need(data, 4)?;
                Ok(CaretValue::Coordinate(be::i16_at(data, 2).unwrap_or(0)))
            }
            2 => {
                need(data, 4)?;
                Ok(CaretValue::Point(be::u16_at(data, 2).unwrap_or(0)))
            }
            3 => {
                need(data, 6)?;
                let o = be::u16_at(data, 4).map_or(0, u32::from);
                let device = follow(data, o, Device::new).ok().flatten();
                Ok(CaretValue::Device(be::i16_at(data, 2).unwrap_or(0), device))
            }
            _ => Err(ReadError::UnknownFormat(format)),
        }
    }
}

/// A view over a MarkGlyphSets table (format 1).
#[derive(Clone, Copy, Debug)]
pub struct MarkGlyphSets<'a> {
    data: &'a [u8],
    offsets: &'a [u8],
}

impl<'a> MarkGlyphSets<'a> {
    /// A view over the format, the count and the 32-bit Coverage offsets.
    /// Fails with [`ReadError::UnknownFormat`] for a format other than
    /// 1, and with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MarkGlyphSets<'a>, ReadError> {
        let format = be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?;
        if format != 1 {
            return Err(ReadError::UnknownFormat(format));
        }
        let (_, offsets) = counted(data, 2, 2, 4)?;
        Ok(MarkGlyphSets { data, offsets })
    }
    /// `markGlyphSetCount`.
    pub fn len(&self) -> usize {
        self.offsets.len() / 4
    }
    /// Whether there are no sets.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// The coverage of set `i`; [`ReadError::OutOfBounds`] for an index
    /// past the offsets.
    pub fn coverage(&self, i: usize) -> Result<Option<Coverage<'a>>, ReadError> {
        let o = be::u32_at(self.offsets, i.saturating_mul(4)).ok_or(ReadError::OutOfBounds)?;
        follow(self.data, o, Coverage::new)
    }
    /// Whether set `set` covers `glyph` (HarfBuzz `covers`); false for a
    /// set that is missing or cannot be read.
    pub fn covers(&self, set: u16, glyph: GlyphId) -> bool {
        matches!(self.coverage(usize::from(set)), Ok(Some(c)) if c.get(glyph).is_some())
    }
}

/// Checks a `GDEF` table: an unknown version and truncation of the
/// header (and nothing else then), offsets whose tables do not fit,
/// unsorted Coverage tables and overlapping ClassDef ranges, AttachList
/// and LigCaretList counts that are not those of their Coverage tables,
/// and unknown CaretValue and MarkGlyphSets formats. Offsets are those
/// of the fields that point to the bad tables, or of the records.
pub fn validate(data: &[u8], report: &mut dyn FnMut(Diagnostic)) {
    let gdef = match Gdef::new(data) {
        Ok(g) => g,
        Err(ReadError::UnknownVersion(found)) => {
            return report(Diagnostic::new(TAG, 0, Kind::UnknownVersion { found }));
        }
        Err(_) => {
            let needed = be::u32_at(data, 0).map_or(4, header_size);
            return report(Diagnostic::new(
                TAG,
                0,
                Kind::Truncated {
                    needed: to_u32(needed),
                    available: to_u32(data.len()),
                },
            ));
        }
    };
    let mut w = Walker::new(TAG, data, report);
    let v = gdef.version();
    let wide = v >= VERSION_1_4;
    let field = |narrow: usize, wide_at: usize| -> (usize, usize) {
        if wide && be::u32_at(data, wide_at).is_some_and(|o| o != 0) {
            (wide_at, 4)
        } else {
            (narrow, 2)
        }
    };
    let (f, fw) = field(4, 18);
    w.class_def(f, 0, fw);
    let (f, fw) = field(6, 22);
    attach_list(&mut w, f, fw);
    let (f, fw) = field(8, 26);
    lig_caret_list(&mut w, f, fw);
    let (f, fw) = field(10, 30);
    w.class_def(f, 0, fw);
    if v >= VERSION_1_2 {
        let (f, fw) = field(12, 34);
        mark_glyph_sets(&mut w, f, fw);
    }
    if v >= VERSION_1_3 {
        w.opaque(14, 0, 4, 8);
    }
}

fn attach_list(w: &mut Walker<'_, '_>, field: usize, width: usize) {
    let Some((at, list)) = w.follow(field, 0, width, AttachList::new) else {
        return;
    };
    let covered = w.coverage(at, at, 2);
    w.count_matches(at.saturating_add(2), covered, list.len());
    for i in 0..list.len() {
        let f = at.saturating_add(4).saturating_add(i.saturating_mul(2));
        w.follow(f, at, 2, |d| counted(d, 0, 2, 2));
    }
}

fn lig_caret_list(w: &mut Walker<'_, '_>, field: usize, width: usize) {
    let wide = width == 4;
    let Some((at, list)) = w.follow(field, 0, width, |d| LigCaretList::new(d, wide)) else {
        return;
    };
    let (cw, ow) = if wide { (4, 3) } else { (2, 2) };
    let covered = w.coverage(at, at, cw);
    let count_at = at.saturating_add(cw);
    w.count_matches(count_at, covered, list.len());
    for i in 0..list.len() {
        let f = count_at
            .saturating_add(ow)
            .saturating_add(i.saturating_mul(ow));
        let Some((gat, glyph)) = w.follow(f, at, ow, LigGlyph::new) else {
            continue;
        };
        if !w.spend(glyph.len()) {
            return;
        }
        for j in 0..glyph.len() {
            let f = gat.saturating_add(2).saturating_add(j.saturating_mul(2));
            if let Some((cat, CaretValue::Device(..))) = w.follow(f, gat, 2, CaretValue::new) {
                w.device(cat.saturating_add(4), cat);
            }
        }
    }
}

fn mark_glyph_sets(w: &mut Walker<'_, '_>, field: usize, width: usize) {
    let Some((at, sets)) = w.follow(field, 0, width, MarkGlyphSets::new) else {
        return;
    };
    for i in 0..sets.len() {
        w.coverage(
            at.saturating_add(4).saturating_add(i.saturating_mul(4)),
            at,
            4,
        );
    }
}
