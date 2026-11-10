//! The `MATH` (mathematical typesetting) table, version 1.0 (ADR 0022).
//!
//! The header points to three subtables: [`MathConstants`] (the global
//! constants), [`MathGlyphInfo`] (per-glyph italics corrections, top
//! accent attachments, extended shapes and [`MathKernInfo`]) and
//! [`MathVariants`] (the size variants and the [`MathGlyphAssembly`] of
//! stretchable glyphs, one [`MathGlyphConstruction`] per glyph and
//! direction). Values are [`MathValue`]s: a value in design units and an
//! optional [`Device`] table relative to the subtable that holds it.
//!
//! Methods that follow an offset return `Ok(None)` for a null offset
//! and an error when the table it points to cannot be read.
//!
//! Specification: <https://learn.microsoft.com/typography/opentype/spec/math>

use crate::be::{self, to_u32, to_usize};
use crate::device::Device;
use crate::layout::walk::Walker;
use crate::layout::{Coverage, counted, need, record};
use crate::{Diagnostic, GlyphId, Kind, ReadError, Tag};

/// The `MATH` tag.
pub const TAG: Tag = Tag::new(b"MATH");
/// The size of the header.
pub const HEADER_SIZE: usize = 10;
/// The size of the MathConstants table.
pub const CONSTANTS_SIZE: usize = 214;
/// The number of MathValueRecords of the MathConstants table, from
/// `mathLeading` to `radicalKernAfterDegree`.
pub const NUM_CONSTANT_VALUES: usize = 51;
/// The `Extender` bit of `partFlags`.
pub const PART_EXTENDER: u16 = 0x0001;

/// The table at the 16-bit offset read at `at` in `data`, relative to
/// the start of `data`.
fn follow16<'a, T>(
    data: &'a [u8],
    at: usize,
    make: impl FnOnce(&'a [u8]) -> Result<T, ReadError>,
) -> Result<Option<T>, ReadError> {
    let offset = be::u16_at(data, at).unwrap_or(0);
    crate::layout::follow(data, u32::from(offset), make)
}

/// A view over the `MATH` header.
#[derive(Clone, Copy, Debug)]
pub struct Math<'a> {
    data: &'a [u8],
}

impl<'a> Math<'a> {
    /// A view over the header.
    ///
    /// Fails with [`ReadError::UnknownVersion`] if `majorVersion` is not
    /// 1, and with [`ReadError::Truncated`] if the 10-byte header does
    /// not fit.
    pub fn new(data: &'a [u8]) -> Result<Math<'a>, ReadError> {
        need(data, HEADER_SIZE)?;
        let version = be::u32_at(data, 0).unwrap_or(0);
        if version >> 16 != 1 {
            return Err(ReadError::UnknownVersion(version));
        }
        Ok(Math { data })
    }
    /// The bytes of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `majorVersion` and `minorVersion` as one value.
    pub fn version(&self) -> u32 {
        be::u32_at(self.data, 0).unwrap_or(0)
    }
    /// `mathConstantsOffset`; 0 for none.
    pub fn constants_offset(&self) -> u16 {
        be::u16_at(self.data, 4).unwrap_or(0)
    }
    /// `mathGlyphInfoOffset`; 0 for none.
    pub fn glyph_info_offset(&self) -> u16 {
        be::u16_at(self.data, 6).unwrap_or(0)
    }
    /// `mathVariantsOffset`; 0 for none.
    pub fn variants_offset(&self) -> u16 {
        be::u16_at(self.data, 8).unwrap_or(0)
    }
    /// The global constants.
    pub fn constants(&self) -> Result<Option<MathConstants<'a>>, ReadError> {
        follow16(self.data, 4, MathConstants::new)
    }
    /// The per-glyph information.
    pub fn glyph_info(&self) -> Result<Option<MathGlyphInfo<'a>>, ReadError> {
        follow16(self.data, 6, MathGlyphInfo::new)
    }
    /// The size variants and assemblies.
    pub fn variants(&self) -> Result<Option<MathVariants<'a>>, ReadError> {
        follow16(self.data, 8, MathVariants::new)
    }
}

/// A MathValueRecord: a value in design units and the offset of a
/// Device or VariationIndex table from the start of the subtable that
/// holds the record.
#[derive(Clone, Copy, Debug)]
pub struct MathValue<'a> {
    base: &'a [u8],
    value: i16,
    device_offset: u16,
}

impl<'a> MathValue<'a> {
    /// The record at `at` in `base`.
    fn at(base: &'a [u8], at: usize) -> Option<MathValue<'a>> {
        Some(MathValue {
            base,
            value: be::i16_at(base, at)?,
            device_offset: be::u16_at(base, at.saturating_add(2))?,
        })
    }
    /// `value`, in design units.
    pub fn value(&self) -> i16 {
        self.value
    }
    /// `deviceOffset`; 0 for none.
    pub fn device_offset(&self) -> u16 {
        self.device_offset
    }
    /// The Device or VariationIndex table.
    pub fn device(&self) -> Result<Option<Device<'a>>, ReadError> {
        crate::layout::follow(self.base, u32::from(self.device_offset), Device::new)
    }
}

/// A view over the MathConstants table.
#[derive(Clone, Copy, Debug)]
pub struct MathConstants<'a> {
    data: &'a [u8],
}

impl<'a> MathConstants<'a> {
    /// A view over the 214-byte table. Fails with
    /// [`ReadError::Truncated`] if it does not fit.
    pub fn new(data: &'a [u8]) -> Result<MathConstants<'a>, ReadError> {
        need(data, CONSTANTS_SIZE)?;
        Ok(MathConstants { data })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `scriptPercentScaleDown`.
    pub fn script_percent_scale_down(&self) -> i16 {
        be::i16_at(self.data, 0).unwrap_or(0)
    }
    /// `scriptScriptPercentScaleDown`.
    pub fn script_script_percent_scale_down(&self) -> i16 {
        be::i16_at(self.data, 2).unwrap_or(0)
    }
    /// `delimitedSubFormulaMinHeight`, in design units.
    pub fn delimited_sub_formula_min_height(&self) -> u16 {
        be::u16_at(self.data, 4).unwrap_or(0)
    }
    /// `displayOperatorMinHeight`, in design units.
    pub fn display_operator_min_height(&self) -> u16 {
        be::u16_at(self.data, 6).unwrap_or(0)
    }
    /// MathValueRecord `index` (0 for `mathLeading`, 50 for
    /// `radicalKernAfterDegree`, in the specification's order); `None`
    /// from [`NUM_CONSTANT_VALUES`] on.
    pub fn value(&self, index: usize) -> Option<MathValue<'a>> {
        if index >= NUM_CONSTANT_VALUES {
            return None;
        }
        MathValue::at(self.data, index.saturating_mul(4).saturating_add(8))
    }
    /// `radicalDegreeBottomRaisePercent`.
    pub fn radical_degree_bottom_raise_percent(&self) -> i16 {
        be::i16_at(self.data, 212).unwrap_or(0)
    }
}

/// A view over a table of a Coverage offset and one MathValueRecord per
/// covered glyph: MathItalicsCorrectionInfo and MathTopAccentAttachment.
#[derive(Clone, Copy, Debug)]
pub struct MathValueTable<'a> {
    data: &'a [u8],
    len: usize,
}

impl<'a> MathValueTable<'a> {
    /// A view over the offset, the count and the records. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MathValueTable<'a>, ReadError> {
        let (len, _) = counted(data, 2, 2, 4)?;
        Ok(MathValueTable { data, len })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// The offset of the Coverage table; 0 for none.
    pub fn coverage_offset(&self) -> u16 {
        be::u16_at(self.data, 0).unwrap_or(0)
    }
    /// The covered glyphs.
    pub fn coverage(&self) -> Result<Option<Coverage<'a>>, ReadError> {
        follow16(self.data, 0, Coverage::new)
    }
    /// The number of records.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Record `index` (the coverage index of its glyph).
    pub fn value(&self, index: usize) -> Option<MathValue<'a>> {
        if index >= self.len {
            return None;
        }
        MathValue::at(self.data, index.saturating_mul(4).saturating_add(4))
    }
    /// The record of `glyph`; `None` when the glyph is not covered.
    pub fn get(&self, glyph: GlyphId) -> Option<MathValue<'a>> {
        let index = self.coverage().ok().flatten()?.get(glyph)?;
        self.value(to_usize(index))
    }
}

/// A view over the MathGlyphInfo table.
#[derive(Clone, Copy, Debug)]
pub struct MathGlyphInfo<'a> {
    data: &'a [u8],
}

impl<'a> MathGlyphInfo<'a> {
    /// A view over the four offsets. Fails with [`ReadError::Truncated`]
    /// if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MathGlyphInfo<'a>, ReadError> {
        need(data, 8)?;
        Ok(MathGlyphInfo { data })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// The italics corrections (MathItalicsCorrectionInfo).
    pub fn italics_correction(&self) -> Result<Option<MathValueTable<'a>>, ReadError> {
        follow16(self.data, 0, MathValueTable::new)
    }
    /// The top accent attachment points (MathTopAccentAttachment).
    pub fn top_accent_attachment(&self) -> Result<Option<MathValueTable<'a>>, ReadError> {
        follow16(self.data, 2, MathValueTable::new)
    }
    /// The extended shape glyphs.
    pub fn extended_shape_coverage(&self) -> Result<Option<Coverage<'a>>, ReadError> {
        follow16(self.data, 4, Coverage::new)
    }
    /// The kerning of the corners of glyphs.
    pub fn kern_info(&self) -> Result<Option<MathKernInfo<'a>>, ReadError> {
        follow16(self.data, 6, MathKernInfo::new)
    }
}

/// The corner of a glyph a MathKern table is for, in the order of the
/// offsets of a MathKernInfoRecord.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Corner {
    /// `topRightMathKernOffset`.
    TopRight = 0,
    /// `topLeftMathKernOffset`.
    TopLeft = 1,
    /// `bottomRightMathKernOffset`.
    BottomRight = 2,
    /// `bottomLeftMathKernOffset`.
    BottomLeft = 3,
}

/// A view over the MathKernInfo table.
#[derive(Clone, Copy, Debug)]
pub struct MathKernInfo<'a> {
    data: &'a [u8],
    len: usize,
}

impl<'a> MathKernInfo<'a> {
    /// A view over the offset, the count and the MathKernInfoRecords.
    /// Fails with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MathKernInfo<'a>, ReadError> {
        let (len, _) = counted(data, 2, 2, 8)?;
        Ok(MathKernInfo { data, len })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// The covered glyphs.
    pub fn coverage(&self) -> Result<Option<Coverage<'a>>, ReadError> {
        follow16(self.data, 0, Coverage::new)
    }
    /// The number of MathKernInfoRecords.
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether there are no records.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// The offset of the MathKern table of `corner` of record `index`,
    /// from the start of the MathKernInfo table.
    pub fn kern_offset(&self, index: usize, corner: Corner) -> Option<u16> {
        if index >= self.len {
            return None;
        }
        let at = index
            .saturating_mul(8)
            .saturating_add(4)
            .saturating_add((corner as usize).saturating_mul(2));
        be::u16_at(self.data, at)
    }
    /// The MathKern table of `corner` of record `index`;
    /// [`ReadError::OutOfBounds`] for an index past the records.
    pub fn kern(&self, index: usize, corner: Corner) -> Result<Option<MathKern<'a>>, ReadError> {
        let offset = self
            .kern_offset(index, corner)
            .ok_or(ReadError::OutOfBounds)?;
        crate::layout::follow(self.data, u32::from(offset), MathKern::new)
    }
}

/// A view over a MathKern table: `heightCount` correction heights and
/// one more kern values.
#[derive(Clone, Copy, Debug)]
pub struct MathKern<'a> {
    data: &'a [u8],
    height_count: usize,
}

impl<'a> MathKern<'a> {
    /// A view over the count and the records. Fails with
    /// [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MathKern<'a>, ReadError> {
        let n = usize::from(be::u16_at(data, 0).ok_or(ReadError::truncated(2, data.len()))?);
        need(
            data,
            n.saturating_mul(2)
                .saturating_add(1)
                .saturating_mul(4)
                .saturating_add(2),
        )?;
        Ok(MathKern {
            data,
            height_count: n,
        })
    }
    /// `heightCount`.
    pub fn height_count(&self) -> usize {
        self.height_count
    }
    /// Correction height `index`, below [`MathKern::height_count`].
    pub fn correction_height(&self, index: usize) -> Option<MathValue<'a>> {
        if index >= self.height_count {
            return None;
        }
        MathValue::at(self.data, index.saturating_mul(4).saturating_add(2))
    }
    /// Kern value `index`, up to [`MathKern::height_count`].
    pub fn kern_value(&self, index: usize) -> Option<MathValue<'a>> {
        if index > self.height_count {
            return None;
        }
        let at = self
            .height_count
            .saturating_add(index)
            .saturating_mul(4)
            .saturating_add(2);
        MathValue::at(self.data, at)
    }
}

/// A view over the MathVariants table.
#[derive(Clone, Copy, Debug)]
pub struct MathVariants<'a> {
    data: &'a [u8],
    vert: usize,
    horiz: usize,
}

impl<'a> MathVariants<'a> {
    /// A view over the header and the offsets of the
    /// MathGlyphConstruction tables. Fails with [`ReadError::Truncated`]
    /// if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MathVariants<'a>, ReadError> {
        need(data, 10)?;
        let vert = usize::from(be::u16_at(data, 6).unwrap_or(0));
        let horiz = usize::from(be::u16_at(data, 8).unwrap_or(0));
        need(
            data,
            vert.saturating_add(horiz)
                .saturating_mul(2)
                .saturating_add(10),
        )?;
        Ok(MathVariants { data, vert, horiz })
    }
    /// The bytes from the start of the table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    /// `minConnectorOverlap`, in design units.
    pub fn min_connector_overlap(&self) -> u16 {
        be::u16_at(self.data, 0).unwrap_or(0)
    }
    /// The glyphs with vertical constructions.
    pub fn vert_coverage(&self) -> Result<Option<Coverage<'a>>, ReadError> {
        follow16(self.data, 2, Coverage::new)
    }
    /// The glyphs with horizontal constructions.
    pub fn horiz_coverage(&self) -> Result<Option<Coverage<'a>>, ReadError> {
        follow16(self.data, 4, Coverage::new)
    }
    /// `vertGlyphCount`.
    pub fn vert_glyph_count(&self) -> usize {
        self.vert
    }
    /// `horizGlyphCount`.
    pub fn horiz_glyph_count(&self) -> usize {
        self.horiz
    }
    /// The offset of MathGlyphConstruction `index` from the start of
    /// the MathVariants table: the vertical ones first, then the
    /// horizontal ones.
    pub fn construction_offset(&self, index: usize) -> Option<u16> {
        if index >= self.vert.saturating_add(self.horiz) {
            return None;
        }
        be::u16_at(self.data, index.saturating_mul(2).saturating_add(10))
    }
    fn construction(&self, index: usize) -> Result<Option<MathGlyphConstruction<'a>>, ReadError> {
        let offset = self
            .construction_offset(index)
            .ok_or(ReadError::OutOfBounds)?;
        crate::layout::follow(self.data, u32::from(offset), MathGlyphConstruction::new)
    }
    /// The vertical construction of coverage index `index`;
    /// [`ReadError::OutOfBounds`] from `vertGlyphCount` on.
    pub fn vert_construction(
        &self,
        index: usize,
    ) -> Result<Option<MathGlyphConstruction<'a>>, ReadError> {
        if index >= self.vert {
            return Err(ReadError::OutOfBounds);
        }
        self.construction(index)
    }
    /// The horizontal construction of coverage index `index`;
    /// [`ReadError::OutOfBounds`] from `horizGlyphCount` on.
    pub fn horiz_construction(
        &self,
        index: usize,
    ) -> Result<Option<MathGlyphConstruction<'a>>, ReadError> {
        if index >= self.horiz {
            return Err(ReadError::OutOfBounds);
        }
        self.construction(self.vert.saturating_add(index))
    }
}

/// A view over a MathGlyphConstruction table.
#[derive(Clone, Copy, Debug)]
pub struct MathGlyphConstruction<'a> {
    data: &'a [u8],
    variants: &'a [u8],
    len: usize,
}

impl<'a> MathGlyphConstruction<'a> {
    /// A view over the offset, the count and the MathGlyphVariantRecords.
    /// Fails with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MathGlyphConstruction<'a>, ReadError> {
        let (len, variants) = counted(data, 2, 2, 4)?;
        Ok(MathGlyphConstruction {
            data,
            variants,
            len,
        })
    }
    /// The assembly of parts.
    pub fn glyph_assembly(&self) -> Result<Option<MathGlyphAssembly<'a>>, ReadError> {
        follow16(self.data, 0, MathGlyphAssembly::new)
    }
    /// The number of variants.
    pub fn num_variants(&self) -> usize {
        self.len
    }
    /// Variant `index`: its glyph and `advanceMeasurement`.
    pub fn variant(&self, index: usize) -> Option<(GlyphId, u16)> {
        let r = record(self.variants, index, 4)?;
        Some((GlyphId::from(be::u16_at(r, 0)?), be::u16_at(r, 2)?))
    }
}

/// A MathGlyphPartRecord.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GlyphPart {
    /// `glyphID`.
    pub glyph: GlyphId,
    /// `startConnectorLength`, in design units.
    pub start_connector_length: u16,
    /// `endConnectorLength`, in design units.
    pub end_connector_length: u16,
    /// `fullAdvance`, in design units.
    pub full_advance: u16,
    /// `partFlags` ([`PART_EXTENDER`]; the other bits are reserved).
    pub flags: u16,
}

/// A view over a MathGlyphAssembly table.
#[derive(Clone, Copy, Debug)]
pub struct MathGlyphAssembly<'a> {
    data: &'a [u8],
    parts: &'a [u8],
    len: usize,
}

impl<'a> MathGlyphAssembly<'a> {
    /// A view over the italics correction, the count and the part
    /// records. Fails with [`ReadError::Truncated`] if they do not fit.
    pub fn new(data: &'a [u8]) -> Result<MathGlyphAssembly<'a>, ReadError> {
        let (len, parts) = counted(data, 4, 2, 10)?;
        Ok(MathGlyphAssembly { data, parts, len })
    }
    /// The italics correction of the assembly.
    pub fn italics_correction(&self) -> MathValue<'a> {
        MathValue::at(self.data, 0).unwrap_or(MathValue {
            base: self.data,
            value: 0,
            device_offset: 0,
        })
    }
    /// The number of parts.
    pub fn num_parts(&self) -> usize {
        self.len
    }
    /// Part `index`, from left to right or bottom to top.
    pub fn part(&self, index: usize) -> Option<GlyphPart> {
        let r = record(self.parts, index, 10)?;
        Some(GlyphPart {
            glyph: GlyphId::from(be::u16_at(r, 0)?),
            start_connector_length: be::u16_at(r, 2)?,
            end_connector_length: be::u16_at(r, 4)?,
            full_advance: be::u16_at(r, 6)?,
            flags: be::u16_at(r, 8)?,
        })
    }
}

/// Checks a `MATH` table: an unknown version and truncation of the
/// header (and nothing else then), offsets whose tables do not fit,
/// Device tables that do not fit, malformed or unsorted Coverage tables,
/// arrays indexed by coverage index whose length is not the number of
/// covered glyphs, glyph ids of variants and parts not below
/// `num_glyphs` (when known; warnings) and reserved bits of `partFlags`
/// (warnings). Offsets are those of the offset fields that point to the
/// bad tables, or of the records.
pub fn validate(data: &[u8], num_glyphs: Option<u32>, report: &mut dyn FnMut(Diagnostic)) {
    match Math::new(data) {
        Ok(_) => {}
        Err(ReadError::UnknownVersion(found)) => {
            return report(Diagnostic::new(TAG, 0, Kind::UnknownVersion { found }));
        }
        Err(_) => {
            return report(Diagnostic::new(
                TAG,
                0,
                Kind::Truncated {
                    needed: to_u32(HEADER_SIZE),
                    available: to_u32(data.len()),
                },
            ));
        }
    }
    let mut w = Walker::new(TAG, data, report);
    if let Some((at, _)) = w.follow(4, 0, 2, MathConstants::new) {
        for i in 0..NUM_CONSTANT_VALUES {
            w.device(
                at.saturating_add(10).saturating_add(i.saturating_mul(4)),
                at,
            );
        }
    }
    if let Some((at, _)) = w.follow(6, 0, 2, MathGlyphInfo::new) {
        value_table(&mut w, at, at);
        value_table(&mut w, at.saturating_add(2), at);
        w.coverage(at.saturating_add(4), at, 2);
        kern_info(&mut w, at.saturating_add(6), at);
    }
    if let Some((at, v)) = w.follow(8, 0, 2, MathVariants::new) {
        variants(&mut w, at, &v, num_glyphs);
    }
}

/// MathItalicsCorrectionInfo or MathTopAccentAttachment at the offset at
/// `field` from `base`.
fn value_table(w: &mut Walker<'_, '_>, field: usize, base: usize) {
    let Some((at, t)) = w.follow(field, base, 2, MathValueTable::new) else {
        return;
    };
    let covered = w.coverage(at, at, 2);
    w.count_matches(at.saturating_add(2), covered, t.len());
    if !w.spend(t.len()) {
        return;
    }
    for i in 0..t.len() {
        w.device(at.saturating_add(6).saturating_add(i.saturating_mul(4)), at);
    }
}

fn kern_info(w: &mut Walker<'_, '_>, field: usize, base: usize) {
    let Some((at, k)) = w.follow(field, base, 2, MathKernInfo::new) else {
        return;
    };
    let covered = w.coverage(at, at, 2);
    w.count_matches(at.saturating_add(2), covered, k.len());
    for i in 0..k.len() {
        for c in 0..4usize {
            let f = at
                .saturating_add(4)
                .saturating_add(i.saturating_mul(8))
                .saturating_add(c.saturating_mul(2));
            let Some((kat, kern)) = w.follow(f, at, 2, MathKern::new) else {
                continue;
            };
            let n = kern.height_count().saturating_mul(2).saturating_add(1);
            if !w.spend(n) {
                return;
            }
            for j in 0..n {
                w.device(
                    kat.saturating_add(4).saturating_add(j.saturating_mul(4)),
                    kat,
                );
            }
        }
    }
}

fn glyph(w: &mut Walker<'_, '_>, at: usize, g: GlyphId, num_glyphs: Option<u32>) {
    if let Some(n) = num_glyphs
        && g.0 >= n
    {
        w.emit(
            at,
            Kind::GlyphIdOutOfRange {
                glyph: g.0,
                num_glyphs: n,
            },
        );
    }
}

fn variants(w: &mut Walker<'_, '_>, at: usize, v: &MathVariants<'_>, num_glyphs: Option<u32>) {
    let vert = w.coverage(at.saturating_add(2), at, 2);
    w.count_matches(at.saturating_add(6), vert, v.vert_glyph_count());
    let horiz = w.coverage(at.saturating_add(4), at, 2);
    w.count_matches(at.saturating_add(8), horiz, v.horiz_glyph_count());
    let total = v.vert_glyph_count().saturating_add(v.horiz_glyph_count());
    for i in 0..total {
        let f = at.saturating_add(10).saturating_add(i.saturating_mul(2));
        let Some((cat, c)) = w.follow(f, at, 2, MathGlyphConstruction::new) else {
            continue;
        };
        if !w.spend(c.num_variants()) {
            return;
        }
        for j in 0..c.num_variants() {
            if let Some((g, _)) = c.variant(j) {
                glyph(
                    w,
                    cat.saturating_add(4).saturating_add(j.saturating_mul(4)),
                    g,
                    num_glyphs,
                );
            }
        }
        let Some((aat, a)) = w.follow(cat, cat, 2, MathGlyphAssembly::new) else {
            continue;
        };
        w.device(aat.saturating_add(2), aat);
        if !w.spend(a.num_parts()) {
            return;
        }
        for j in 0..a.num_parts() {
            let Some(p) = a.part(j) else { continue };
            let rat = aat.saturating_add(6).saturating_add(j.saturating_mul(10));
            glyph(w, rat, p.glyph, num_glyphs);
            if p.flags & !PART_EXTENDER != 0 {
                w.emit(
                    rat.saturating_add(8),
                    Kind::ReservedBitsSet {
                        found: u32::from(p.flags),
                    },
                );
            }
        }
    }
}
