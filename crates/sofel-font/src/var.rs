//! Variable-font coordinates and variation arithmetic (ADR 0027, ADR 0030).

mod cff2;
pub use cff2::{BlendError, BlendScalars};
pub mod gvar;

use alloc::vec::Vec;
use alloc::{format, string::String};
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf32, i32_to_f32};
use sofel_geom::{div_fix, mul_div, mul_fix};
use sofel_read::item_variation::{ItemVariationStore, Region};
use sofel_read::{
    Fixed, GlyphId, ReadError, Tag,
    avar::{Avar, SegmentMap},
    base::{Base, BaseCoord},
    colr::Colr,
    delta_set_index_map::DeltaSetIndexMap,
    fvar::{Fvar, InstanceRecord},
    gdef::Gdef,
    hvar::{Hvar, Vvar},
    mvar::Mvar,
};

use crate::{Cff, Direction, Font, FontExtents, GlyphExtents, HbScale, MetricsTag};

/// ItemVariationStore belonging to a variable font table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    /// Horizontal metrics.
    Hvar,
    /// Vertical metrics.
    Vvar,
    /// Metric values.
    Mvar,
    /// GDEF variation devices.
    Gdef,
    /// BASE variation devices.
    Base,
    /// COLR paint parameters.
    Colr,
    /// CFF2 blend values.
    Cff2,
}

impl StoreKind {
    const ALL: [Self; STORE_COUNT] = [
        Self::Hvar,
        Self::Vvar,
        Self::Mvar,
        Self::Gdef,
        Self::Base,
        Self::Colr,
        Self::Cff2,
    ];
    const fn index(self) -> usize {
        self as usize
    }
}

fn variation_store<'a>(font: &Font<'a>, kind: StoreKind) -> Option<ItemVariationStore<'a>> {
    match kind {
        StoreKind::Hvar => font
            .table(Tag::new(b"HVAR"))
            .and_then(|d| Hvar::new(d).ok()?.store().ok()),
        StoreKind::Vvar => font
            .table(Tag::new(b"VVAR"))
            .and_then(|d| Vvar::new(d).ok()?.store().ok()),
        StoreKind::Mvar => font
            .table(Tag::new(b"MVAR"))
            .and_then(|d| Mvar::new(d).ok()?.store().ok()),
        StoreKind::Gdef => font
            .table(Tag::new(b"GDEF"))
            .and_then(|d| Gdef::new(d).ok()?.variation_store().ok().flatten()),
        StoreKind::Base => font
            .table(Tag::new(b"BASE"))
            .and_then(|d| Base::new(d).ok()?.variation_store().ok().flatten()),
        StoreKind::Colr => font
            .table(Tag::new(b"COLR"))
            .and_then(|d| Colr::new(d).ok()?.variation_store().ok().flatten()),
        StoreKind::Cff2 => Cff::new(font).ok()?.variation_store(),
    }
}

const STORE_COUNT: usize = 7;

/// One HarfBuzz-style design-space axis setting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variation {
    /// Four-byte axis tag.
    pub tag: Tag,
    /// Design-space value.
    pub value: f32,
}

/// HarfBuzz's information for one design axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VarAxisInfo {
    /// Zero-based axis index.
    pub index: u32,
    /// Axis tag.
    pub tag: Tag,
    /// Axis name ID.
    pub name_id: u16,
    /// Whether the axis is hidden in a user interface.
    pub hidden: bool,
    /// Ordered minimum design value.
    pub min: f32,
    /// Default design value.
    pub default: f32,
    /// Ordered maximum design value.
    pub max: f32,
}

/// HarfBuzz's information for one fvar named instance.
#[derive(Clone, Copy, Debug)]
pub struct NamedInstanceInfo<'a> {
    /// Subfamily name ID.
    pub subfamily_name_id: u16,
    /// PostScript name ID, when present in the fvar record.
    pub postscript_name_id: Option<u16>,
    record: InstanceRecord<'a>,
}

impl NamedInstanceInfo<'_> {
    /// Design coordinates in fvar axis order.
    pub fn design_coords(&self) -> impl Iterator<Item = f32> + '_ {
        (0..self.record.num_coordinates())
            .filter_map(|i| self.record.coordinate(i).map(fixed_to_f32))
    }
}

/// FreeType's information for one design axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FtAxisInfo {
    /// Axis tag.
    pub tag: Tag,
    /// Raw axis name ID.
    pub name_id: u16,
    /// `FT_Get_Var_Axis_Flags` value.
    pub flags: u16,
    /// Minimum design value in 16.16.
    pub min: Fixed,
    /// Default design value in 16.16.
    pub default: Fixed,
    /// Maximum design value in 16.16.
    pub max: Fixed,
}

/// FreeType's information for one named style.
#[derive(Clone, Copy, Debug)]
pub struct FtNamedStyle<'a> {
    /// Subfamily string ID.
    pub strid: u16,
    /// PostScript string ID, or 0xFFFF when absent from fvar.
    pub psid: u16,
    fvar: Fvar<'a>,
    record: Option<InstanceRecord<'a>>,
    zero_coords: bool,
}

impl FtNamedStyle<'_> {
    /// Design coordinates in fvar axis order, in 16.16.
    pub fn design_coords(&self) -> impl Iterator<Item = Fixed> + '_ {
        (0..self.fvar.axis_count()).filter_map(|i| {
            if self.zero_coords {
                return Some(Fixed(0));
            }
            self.record
                .and_then(|record| record.coordinate(i))
                .or_else(|| self.fvar.axis(i).map(|axis| axis.default))
        })
    }
}

/// A zero-copy view corresponding to FreeType's `FT_MM_Var`.
#[derive(Clone, Copy, Debug)]
pub struct FtMmVar<'a> {
    fvar: Fvar<'a>,
    synthetic: bool,
    synthetic_strid: u16,
    synthetic_psid: u16,
}

impl<'a> FtMmVar<'a> {
    /// FreeType axes, including its invalid-range collapse.
    pub fn axes(&self) -> impl Iterator<Item = FtAxisInfo> + '_ {
        self.fvar.axes().map(|axis| {
            let (min, max) = if axis.min.0 > axis.default.0 || axis.default.0 > axis.max.0 {
                (axis.default, axis.default)
            } else {
                (axis.min, axis.max)
            };
            FtAxisInfo {
                tag: axis.tag,
                name_id: axis.name_id,
                flags: axis.flags,
                min,
                default: axis.default,
                max,
            }
        })
    }

    /// FreeType named styles, including a valid synthesized default style.
    pub fn named_styles(&self) -> impl Iterator<Item = FtNamedStyle<'a>> + '_ {
        let count = self
            .fvar
            .instance_count()
            .saturating_add(usize::from(self.synthetic));
        (0..count).map(move |i| {
            if let Some(record) = self.fvar.instance(i) {
                FtNamedStyle {
                    strid: record.subfamily_name_id,
                    psid: record.postscript_name_id.unwrap_or(0xFFFF),
                    fvar: self.fvar,
                    record: Some(record),
                    zero_coords: false,
                }
            } else {
                FtNamedStyle {
                    strid: self.synthetic_strid,
                    psid: self.synthetic_psid,
                    fvar: self.fvar,
                    record: None,
                    zero_coords: self.synthetic_strid == 0,
                }
            }
        })
    }

    /// One-based index from `FT_Get_Default_Named_Instance`.
    pub fn default_named_instance(&self) -> Option<u16> {
        for index in 0..self.fvar.instance_count() {
            let Some(record) = self.fvar.instance(index) else {
                continue;
            };
            if (0..self.fvar.axis_count())
                .all(|axis| self.fvar.axis(axis).map(|a| a.default) == record.coordinate(axis))
            {
                return u16::try_from(index.saturating_add(1)).ok();
            }
        }
        self.synthetic
            .then(|| self.fvar.instance_count().saturating_add(1))
            .and_then(|index| u16::try_from(index).ok())
    }
}

fn ft_has_name_id(font: &Font<'_>, name_id: u16) -> bool {
    (0..font.sfnt_name_count())
        .filter_map(|index| font.sfnt_name(index))
        .any(|record| {
            record.name_id == name_id
                && ((record.platform_id == 3 && matches!(record.encoding_id, 0 | 1))
                    || (record.platform_id == 1 && record.encoding_id == 0))
        })
}

fn name_string(font: &Font<'_>, name_id: u16) -> Option<String> {
    let len = font.name_utf8(name_id, None, &mut []).total;
    if len == 0 {
        return None;
    }
    let mut bytes = alloc::vec![0; len.saturating_add(1)];
    let written = font.name_utf8(name_id, None, &mut bytes).written;
    bytes.truncate(written);
    String::from_utf8(bytes).ok()
}

fn write_name(name: &str, out: &mut [u8]) -> usize {
    let bytes = name.as_bytes();
    let copied = out.len().min(bytes.len());
    if let (Some(dest), Some(src)) = (out.get_mut(..copied), bytes.get(..copied)) {
        dest.copy_from_slice(src);
    }
    bytes.len()
}

/// FreeType's `fixed2float` shortest decimal for a 16.16 design value.
fn ft_fixed_to_short_decimal(value: Fixed) -> String {
    if value.0 == 0 {
        return String::from("0");
    }
    let mut result = String::new();
    if value.0 < 0 {
        result.push('-');
    }
    let absolute = value.0.unsigned_abs();
    let integer = absolute >> 16;
    if integer != 0 {
        result.push_str(&format!("{integer}"));
    }
    let fraction = absolute & 0xFFFF;
    if fraction == 0 {
        return result;
    }
    result.push('.');
    let mut remaining = fraction.saturating_mul(10).saturating_add(5);
    let mut digits = [0u8; 5];
    let mut count = 0usize;
    for digit in &mut digits {
        *digit = b'0'.saturating_add(u8::try_from(remaining >> 16).unwrap_or(0));
        count = count.saturating_add(1);
        let remainder = remaining & 0xFFFF;
        if remainder == 0 {
            break;
        }
        remaining = remainder.saturating_mul(10);
    }
    if count == 5
        && let Some(last) = digits.get_mut(4)
        && ((remaining < 344_800 && *last == b'1')
            || (remaining == 172_320 && *last & 1 != 0)
            || (remaining < 172_320 && *last != b'0'))
    {
        *last = last.saturating_sub(1);
    }
    while count > 0 && digits.get(count.saturating_sub(1)) == Some(&b'0') {
        count = count.saturating_sub(1);
    }
    for digit in digits.iter().take(count) {
        result.push(char::from(*digit));
    }
    result
}

#[allow(clippy::indexing_slicing)] // Fixed four-word MurmurHash3 state follows FreeType.
fn ft_ps_hash(data: &[u8]) -> [u32; 4] {
    const C1: u32 = 0x239b_961b;
    const C2: u32 = 0xab0e_9789;
    const C3: u32 = 0x38b3_4ae5;
    const C4: u32 = 0xa1e3_8b93;
    let word = |bytes: &[u8], offset: usize| {
        u32::from_le_bytes(core::array::from_fn(|i| {
            bytes.get(offset.saturating_add(i)).copied().unwrap_or(0)
        }))
    };
    let mut hash = [123_456_789u32; 4];
    let (chunks, tail) = data.as_chunks::<16>();
    for block in chunks {
        let (mut k1, mut k2, mut k3, mut k4) = (
            word(block, 0),
            word(block, 4),
            word(block, 8),
            word(block, 12),
        );
        k1 = k1.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2);
        hash[0] ^= k1;
        hash[0] = hash[0]
            .rotate_left(19)
            .wrapping_add(hash[1])
            .wrapping_mul(5)
            .wrapping_add(0x561c_cd1b);
        k2 = k2.wrapping_mul(C2).rotate_left(16).wrapping_mul(C3);
        hash[1] ^= k2;
        hash[1] = hash[1]
            .rotate_left(17)
            .wrapping_add(hash[2])
            .wrapping_mul(5)
            .wrapping_add(0x0bca_a747);
        k3 = k3.wrapping_mul(C3).rotate_left(17).wrapping_mul(C4);
        hash[2] ^= k3;
        hash[2] = hash[2]
            .rotate_left(15)
            .wrapping_add(hash[3])
            .wrapping_mul(5)
            .wrapping_add(0x96cd_1c35);
        k4 = k4.wrapping_mul(C4).rotate_left(18).wrapping_mul(C1);
        hash[3] ^= k4;
        hash[3] = hash[3]
            .rotate_left(13)
            .wrapping_add(hash[0])
            .wrapping_mul(5)
            .wrapping_add(0x32ac_3b17);
    }
    if !tail.is_empty() {
        hash[0] ^= word(tail, 0)
            .wrapping_mul(C1)
            .rotate_left(15)
            .wrapping_mul(C2);
    }
    if tail.len() > 4 {
        hash[1] ^= word(tail, 4)
            .wrapping_mul(C2)
            .rotate_left(16)
            .wrapping_mul(C3);
    }
    if tail.len() > 8 {
        hash[2] ^= word(tail, 8)
            .wrapping_mul(C3)
            .rotate_left(17)
            .wrapping_mul(C4);
    }
    if tail.len() > 12 {
        hash[3] ^= word(tail, 12)
            .wrapping_mul(C4)
            .rotate_left(18)
            .wrapping_mul(C1);
    }
    let length = u32::try_from(data.len()).unwrap_or(u32::MAX);
    for h in &mut hash {
        *h ^= length;
    }
    hash[0] = hash[0]
        .wrapping_add(hash[1])
        .wrapping_add(hash[2])
        .wrapping_add(hash[3]);
    for index in 1..4 {
        hash[index] = hash[index].wrapping_add(hash[0]);
    }
    for h in &mut hash {
        *h = ft_fmix32(*h);
    }
    hash[0] = hash[0]
        .wrapping_add(hash[1])
        .wrapping_add(hash[2])
        .wrapping_add(hash[3]);
    for index in 1..4 {
        hash[index] = hash[index].wrapping_add(hash[0]);
    }
    hash
}

fn ft_fmix32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x85eb_ca6b);
    value ^= value >> 13;
    value = value.wrapping_mul(0xc2b2_ae35);
    value ^= value >> 16;
    value
}

impl<'data> Font<'data> {
    /// Axis information in HarfBuzz's order.
    pub fn var_axes(&self) -> impl Iterator<Item = VarAxisInfo> + '_ {
        let fvar = self
            .table(Tag::new(b"fvar"))
            .and_then(|d| Fvar::new(d).ok());
        let count = fvar.map_or(0, |view| view.axis_count());
        (0..count).filter_map(move |index| {
            let axis = fvar?.axis(index)?;
            let (min, default, max) = axis.ordered();
            Some(VarAxisInfo {
                index: u32::try_from(index).unwrap_or(u32::MAX),
                tag: axis.tag,
                name_id: axis.name_id,
                hidden: axis.is_hidden(),
                min: fixed_to_f32(min),
                default: fixed_to_f32(default),
                max: fixed_to_f32(max),
            })
        })
    }

    /// Find the first axis with this tag, as HarfBuzz does.
    pub fn find_var_axis(&self, tag: Tag) -> Option<VarAxisInfo> {
        self.var_axes().find(|axis| axis.tag == tag)
    }

    /// Named instances stored in fvar, excluding FreeType's synthetic style.
    pub fn named_instances(&self) -> impl Iterator<Item = NamedInstanceInfo<'data>> + '_ {
        let fvar = self
            .table(Tag::new(b"fvar"))
            .and_then(|d| Fvar::new(d).ok());
        let count = fvar.map_or(0, |view| view.instance_count());
        (0..count).filter_map(move |index| {
            let record = fvar?.instance(index)?;
            Some(NamedInstanceInfo {
                subfamily_name_id: record.subfamily_name_id,
                postscript_name_id: record.postscript_name_id,
                record,
            })
        })
    }

    /// Strict FreeType MM Var view of a valid fvar table.
    pub fn ft_mm_var(&self) -> Option<FtMmVar<'data>> {
        let data = self.table(Tag::new(b"fvar"))?;
        if !data.starts_with(&[0, 1, 0, 0]) {
            return None;
        }
        let fvar = Fvar::new(data).ok()?;
        let axes = fvar.axis_count();
        if axes == 0 || axes > 0x3FFE || fvar.instance_count() > 0x7EFF {
            return None;
        }
        let short = axes.checked_mul(4)?.checked_add(4)?;
        if fvar.instance_size() != short && fvar.instance_size() != short.checked_add(2)? {
            return None;
        }
        let has_default = (0..fvar.instance_count()).any(|index| {
            fvar.instance(index).is_some_and(|record| {
                (0..axes).all(|axis| fvar.axis(axis).map(|a| a.default) == record.coordinate(axis))
            })
        });
        let synthetic_strid = if ft_has_name_id(self, 6) && ft_has_name_id(self, 17) {
            17
        } else if ft_has_name_id(self, 6) && ft_has_name_id(self, 2) {
            2
        } else {
            0
        };
        Some(FtMmVar {
            fvar,
            synthetic: !has_default,
            synthetic_strid,
            synthetic_psid: if synthetic_strid == 0 { 0 } else { 6 },
        })
    }

    /// One-based default named instance index reported by FreeType.
    pub fn ft_default_named_instance(&self) -> Option<u16> {
        self.ft_mm_var()?.default_named_instance()
    }
}

impl Variation {
    /// Parse a HarfBuzz or CSS font variation setting.
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        let (tag, rest) = if let Some(quote @ ('\'' | '"')) = input.chars().next() {
            let tail = input.get(1..)?;
            let end = tail.find(quote)?;
            if end != 4 {
                return None;
            }
            (tail.get(..end)?, tail.get(end.saturating_add(1)..)?)
        } else {
            let end = input
                .find(|c: char| c.is_ascii_whitespace() || c == '=')
                .unwrap_or(input.len());
            (input.get(..end)?, input.get(end..)?)
        };
        if tag.is_empty() || tag.len() > 4 || !tag.is_ascii() {
            return None;
        }
        let rest = rest.trim_start();
        let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
        if rest.is_empty() {
            return None;
        }
        let value = rest.parse::<f64>().ok()?;
        let mut bytes = [b' '; 4];
        bytes.get_mut(..tag.len())?.copy_from_slice(tag.as_bytes());
        #[allow(clippy::cast_possible_truncation)]
        Some(Self {
            tag: Tag(bytes),
            value: value as f32,
        })
    }

    /// Format as `hb_variation_to_string` does, with six significant digits.
    pub fn format(&self) -> String {
        let end = self
            .tag
            .0
            .iter()
            .rposition(|b| *b != b' ')
            .map_or(0, |i| i.saturating_add(1));
        let name = core::str::from_utf8(self.tag.0.get(..end).unwrap_or(&[])).unwrap_or("????");
        format!("{name}={}", format_g6(self.value))
    }
}

fn format_g6(value: f32) -> String {
    if value.is_nan() {
        return String::from("nan");
    }
    if value == f32::INFINITY {
        return String::from("inf");
    }
    if value == f32::NEG_INFINITY {
        return String::from("-inf");
    }
    let scientific = format!("{:.5e}", f64::from(value));
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return scientific;
    };
    let exponent = exponent.parse::<i32>().unwrap_or(0);
    if (-4..6).contains(&exponent) {
        let precision = usize::try_from(5i32.saturating_sub(exponent)).unwrap_or(0);
        let fixed = format!("{:.*}", precision, f64::from(value));
        if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            fixed
        }
    } else {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        format!(
            "{mantissa}e{}{:#02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.unsigned_abs()
        )
    }
}

/// A normalized location and its cached variation-region coefficients.
///
/// A location borrows the font bytes, so they must outlive it (ADR 0030 I03).
/// The font handle itself can be dropped first.
///
/// ```compile_fail,E0515
/// use sofel_font::{Font, var::Location};
/// fn invalid() -> Location<'static> {
///     let bytes = Vec::from([0u8; 64]);
///     let font = Font::new(&bytes, 0).unwrap();
///     Location::new(&font, None, &[])
/// }
/// ```
#[derive(Clone, Debug)]
pub struct Location<'data> {
    hb_coords: Vec<i32>,
    hb_design: Vec<f32>,
    ft_coords: Vec<Fixed>,
    ft_design: Vec<Fixed>,
    named_instance: Option<u16>,
    hb_nonzero: bool,
    ft_default: bool,
    ft_coords_supplied: bool,
    ft_error: Option<FtLocationError>,
    source: Option<SourceFont<'data>>,
    hb_scalars: [Option<StoreScalars<f32>>; STORE_COUNT],
    ft_scalars: [Option<StoreScalars<i32>>; STORE_COUNT],
    gvar_hb: Vec<f32>,
    gvar_ft: Vec<i32>,
}

impl Location<'static> {
    /// The static default location, usable with any font.
    pub fn default_ref() -> &'static Self {
        static DEFAULT: Location<'static> = Location {
            hb_coords: Vec::new(),
            hb_design: Vec::new(),
            ft_coords: Vec::new(),
            ft_design: Vec::new(),
            named_instance: None,
            hb_nonzero: false,
            ft_default: true,
            ft_coords_supplied: false,
            ft_error: None,
            source: None,
            hb_scalars: [const { None }; STORE_COUNT],
            ft_scalars: [const { None }; STORE_COUNT],
            gvar_hb: Vec::new(),
            gvar_ft: Vec::new(),
        };
        &DEFAULT
    }
}

impl<'data> Location<'data> {
    /// Set a named instance, then apply variations in input order.
    pub fn new(font: &Font<'data>, named_instance: Option<u16>, variations: &[Variation]) -> Self {
        let Some(fvar) = font
            .table(Tag::new(b"fvar"))
            .and_then(|d| Fvar::new(d).ok())
        else {
            let mut location = Self::from_coords(font, Vec::new(), Vec::new(), false);
            location.named_instance = named_instance;
            if named_instance.is_some() {
                location.ft_error = Some(FtLocationError::InvalidArgument);
            }
            return location;
        };
        let mut design: Vec<f32> = fvar.axes().map(|axis| fixed_to_f32(axis.default)).collect();
        let named = named_instance.and_then(|i| fvar.instance(usize::from(i)));
        let ft_synthetic = named_instance.is_some_and(|index| {
            usize::from(index) == fvar.instance_count()
                && font.ft_mm_var().is_some_and(|mm| {
                    mm.named_styles()
                        .nth(usize::from(index))
                        .is_some_and(|style| style.record.is_none() && style.strid != 0)
                })
        });
        if let Some(record) = named {
            for (axis, value) in design.iter_mut().enumerate() {
                if let Some(coord) = record.coordinate(axis) {
                    *value = fixed_to_f32(coord);
                }
            }
        }
        for variation in variations {
            for (axis, value) in fvar.axes().zip(&mut design) {
                if axis.tag == variation.tag {
                    *value = variation.value;
                }
            }
        }
        let mut location = Self::from_design(font, &design);
        location.named_instance = named_instance;
        location.ft_default = named_instance.is_none() && location.ft_default;
        if named_instance.is_some() && named.is_none() && !ft_synthetic {
            location.ft_error = Some(FtLocationError::InvalidArgument);
        }
        location
    }

    /// Select one zero-based fvar named instance.
    pub fn named_instance(font: &Font<'data>, index: u16) -> Self {
        Self::new(font, Some(index), &[])
    }

    /// Set design coordinates as `hb_font_set_var_coords_design` and FreeType do.
    pub fn from_design(font: &Font<'data>, values: &[f32]) -> Self {
        let Some(fvar) = font
            .table(Tag::new(b"fvar"))
            .and_then(|d| Fvar::new(d).ok())
        else {
            return Self::from_coords(font, Vec::new(), Vec::new(), false);
        };
        let design: Vec<f32> = fvar
            .axes()
            .enumerate()
            .map(|(i, axis)| {
                values
                    .get(i)
                    .copied()
                    .unwrap_or_else(|| fixed_to_f32(axis.default))
            })
            .collect();
        let ft_design: Vec<Fixed> = design
            .iter()
            .map(|v| Fixed(hb_clamp_to_i32(f64::from(*v * 65536.0))))
            .collect();
        let mut hb_coords: Vec<i32> = fvar
            .axes()
            .zip(&design)
            .map(|(axis, value)| {
                let (min, default, max) = axis.ordered();
                let normalized = normalize_axis_value(
                    fixed_to_f32(min),
                    fixed_to_f32(default),
                    fixed_to_f32(max),
                    *value,
                );
                hb_clamp_to_i32(hb_roundf32(normalized * 65536.0))
            })
            .collect();
        let mut ft_coords: Vec<Fixed> = fvar
            .axes()
            .zip(&ft_design)
            .map(|(axis, value)| {
                let normalized = if value.0 > axis.default.0 {
                    if value.0 >= axis.max.0 {
                        0x10000
                    } else {
                        div_fix(
                            value.0.wrapping_sub(axis.default.0),
                            axis.max.0.wrapping_sub(axis.default.0),
                        )
                    }
                } else if value.0 < axis.default.0 {
                    if value.0 <= axis.min.0 {
                        -0x10000
                    } else {
                        div_fix(
                            value.0.wrapping_sub(axis.default.0),
                            axis.default.0.wrapping_sub(axis.min.0),
                        )
                    }
                } else {
                    0
                };
                Fixed(normalized)
            })
            .collect();
        if let Some(avar) = font
            .table(Tag::new(b"avar"))
            .and_then(|d| Avar::new(d).ok())
        {
            for (index, coord) in hb_coords.iter_mut().enumerate().take(avar.axis_count()) {
                if let Some(map) = avar.segment_map(index) {
                    let value = i32_to_f32(*coord) / 65536.0;
                    *coord = hb_clamp_to_i32(hb_roundf32(map_float(&map, value) * 65536.0));
                }
            }
            if avar.axis_count() == fvar.axis_count() {
                for (index, coord) in ft_coords.iter_mut().enumerate() {
                    if let Some(map) = avar.segment_map(index) {
                        *coord = Fixed(map_ft_avar(&map, coord.0));
                    }
                }
            }
            apply_avar2(
                &avar,
                &mut hb_coords,
                &mut ft_coords,
                avar.axis_count() == fvar.axis_count(),
            );
        }
        for coord in &mut hb_coords {
            *coord = coord.wrapping_add(2) >> 2;
        }
        let mut location = Self::from_coords(font, hb_coords, ft_coords, fvar.axis_count() != 0);
        location.hb_design = design;
        location.ft_design = ft_design;
        location
    }

    /// Set normalized 16.16 coordinates as `FT_Set_Var_Blend_Coordinates` does.
    pub fn from_normalized_16_16(font: &Font<'data>, values: &[Fixed]) -> Self {
        let axes = font
            .table(Tag::new(b"fvar"))
            .and_then(|d| Fvar::new(d).ok())
            .map_or(0, |f| f.axis_count());
        let ft_coords: Vec<Fixed> = (0..axes)
            .map(|i| values.get(i).copied().unwrap_or_default())
            .collect();
        let hb_coords = ft_coords.iter().map(|c| c.0.wrapping_add(2) >> 2).collect();
        let invalid = ft_coords
            .iter()
            .any(|c| !(-0x10000..=0x10000).contains(&c.0));
        let mut location = Self::from_coords(font, hb_coords, ft_coords, axes != 0);
        location.hb_design = alloc::vec![f32::NAN; axes];
        location.ft_design = ft_to_design(font, &location.ft_coords);
        if invalid {
            location.ft_error = Some(FtLocationError::InvalidArgument);
        }
        location
    }

    /// Set normalized 2.14 coordinates as `hb_font_set_var_coords_normalized` does.
    pub fn from_normalized_2_14(font: &Font<'data>, values: &[i32]) -> Self {
        let axes = font
            .table(Tag::new(b"fvar"))
            .and_then(|d| Fvar::new(d).ok())
            .map_or(0, |f| f.axis_count());
        let hb_coords: Vec<i32> = (0..axes)
            .map(|i| values.get(i).copied().unwrap_or(0))
            .collect();
        let invalid = hb_coords.iter().any(|c| !(-0x4000..=0x4000).contains(c));
        let ft_coords = hb_coords
            .iter()
            .map(|c| Fixed(c.saturating_mul(4)))
            .collect();
        let mut location = Self::from_coords(font, hb_coords, ft_coords, axes != 0);
        location.hb_design = alloc::vec![f32::NAN; axes];
        location.ft_design = ft_to_design(font, &location.ft_coords);
        if invalid {
            location.ft_error = Some(FtLocationError::InvalidArgument);
        }
        location
    }

    /// HarfBuzz 2.14 normalized coordinates.
    pub fn coords_normalized(&self) -> &[i32] {
        &self.hb_coords
    }
    /// HarfBuzz design coordinates.
    pub fn coords_design(&self) -> &[f32] {
        &self.hb_design
    }
    /// FreeType 16.16 normalized coordinates.
    pub fn ft_coords(&self) -> &[Fixed] {
        &self.ft_coords
    }
    /// FreeType design coordinates.
    pub fn ft_design_coords(&self) -> &[Fixed] {
        &self.ft_design
    }
    /// Selected named instance, if any.
    pub fn named_instance_index(&self) -> Option<u16> {
        self.named_instance
    }
    /// Whether HarfBuzz has any nonzero normalized coordinate.
    pub fn hb_nonzero(&self) -> bool {
        self.hb_nonzero
    }
    /// Whether FreeType treats this as the default instance.
    pub fn ft_default(&self) -> bool {
        self.ft_default
    }

    fn from_coords(
        font: &Font<'data>,
        hb_coords: Vec<i32>,
        ft_coords: Vec<Fixed>,
        supplied: bool,
    ) -> Self {
        let source = Some(SourceFont {
            data: font.sfnt.file(),
            face_index: font.face_index,
        });
        let hb_scalars = core::array::from_fn(|index| {
            StoreKind::ALL
                .get(index)
                .and_then(|kind| variation_store(font, *kind))
                .map(|store| StoreScalars::from_store::<HbF32>(&store, &hb_coords, true))
        });
        let ft_scalars = core::array::from_fn(|index| {
            StoreKind::ALL
                .get(index)
                .and_then(|kind| variation_store(font, *kind))
                .map(|store| StoreScalars::from_store::<Ft>(&store, &ft_coords, supplied))
        });
        let hb_nonzero = hb_coords.iter().any(|c| *c != 0);
        let ft_default = ft_coords.iter().all(|c| c.0 == 0);
        let gvar = gvar::table(font);
        let gvar_hb = gvar
            .as_ref()
            .map(|table| gvar::shared_hb(table, &hb_coords))
            .unwrap_or_default();
        let gvar_ft = gvar
            .as_ref()
            .filter(|_| supplied)
            .map(|table| {
                let coords: Vec<i32> = ft_coords.iter().map(|coord| coord.0).collect();
                gvar::shared_ft(table, &coords)
            })
            .unwrap_or_default();
        Self {
            hb_coords,
            hb_design: Vec::new(),
            ft_coords,
            ft_design: Vec::new(),
            named_instance: None,
            hb_nonzero,
            ft_default,
            ft_coords_supplied: supplied,
            ft_error: None,
            source,
            hb_scalars,
            ft_scalars,
            gvar_hb,
            gvar_ft,
        }
    }

    /// FreeType's coordinate-setting error, when that side rejected this location.
    pub fn ft_error(&self) -> Option<FtLocationError> {
        self.ft_error
    }
}

fn fixed_to_f32(value: Fixed) -> f32 {
    i32_to_f32(value.0) * (1.0 / 65536.0)
}

/// HarfBuzz's axis normalization with an ordered design-space range.
pub fn normalize_axis_value(min: f32, default: f32, max: f32, value: f32) -> f32 {
    let value = if value >= min { value } else { min };
    let value = if value <= max { value } else { max };
    if value == default {
        0.0
    } else if value < default {
        (value - default) / (default - min)
    } else {
        (value - default) / (max - default)
    }
}

fn apply_avar2(avar: &Avar<'_>, hb: &mut [i32], ft: &mut [Fixed], ft_enabled: bool) {
    let Some(store) = avar.variation_store().ok().flatten() else {
        return;
    };
    let map = avar.axis_index_map().ok().flatten();
    let index = |axis: usize| {
        let axis = u32::try_from(axis).unwrap_or(u32::MAX);
        let value = map.as_ref().map_or(axis, |m| m.map(axis));
        (
            u16::try_from(value >> 16).unwrap_or(u16::MAX),
            u16::try_from(value & 0xffff).unwrap_or(u16::MAX),
        )
    };
    let hb_pre: Vec<i32> = hb
        .iter()
        .map(|c| hb_clamp_to_i32(hb_roundf32(i32_to_f32(*c) / 4.0)))
        .collect();
    let hb_live = OnTheFly::<HbF32>::new(&store, &hb_pre, true);
    let hb_new: Vec<i32> = hb
        .iter()
        .enumerate()
        .map(|(axis, old)| {
            let (outer, inner) = index(axis);
            let delta = item_delta::<HbF32>(&store, &hb_live, outer, inner);
            let rounded = hb_clamp_to_i32(hb_roundf32((delta * 4.0).clamp(-131072.0, 131072.0)));
            old.saturating_add(rounded).clamp(-65536, 65536)
        })
        .collect();
    hb.copy_from_slice(&hb_new);
    if ft_enabled {
        let ft_live = OnTheFly::<Ft>::new(&store, ft, true);
        let ft_new: Vec<Fixed> = ft
            .iter()
            .enumerate()
            .map(|(axis, old)| {
                let (outer, inner) = index(axis);
                let delta = item_delta::<Ft>(&store, &ft_live, outer, inner);
                Fixed(
                    old.0
                        .saturating_add(delta.saturating_mul(4))
                        .clamp(-65536, 65536),
                )
            })
            .collect();
        ft.copy_from_slice(&ft_new);
    }
}

/// HarfBuzz's `SegmentMaps::map_float`, including its duplicate endpoint rules.
pub fn map_float(map: &SegmentMap<'_>, value: f32) -> f32 {
    let len = map.len();
    let pair = |i: usize| {
        map.pair(i).map_or((0.0, 0.0), |(f, t)| {
            (
                i32_to_f32(i32::from(f.0)) / 16384.0,
                i32_to_f32(i32::from(t.0)) / 16384.0,
            )
        })
    };
    let from = |i: usize| pair(i).0;
    let to = |i: usize| pair(i).1;
    if len < 2 {
        return if len == 0 {
            value
        } else {
            value - from(0) + to(0)
        };
    }
    let mut start = 0usize;
    let mut end = len;
    if from(start) == -1.0 && to(start) == -1.0 && from(start.saturating_add(1)) == -1.0 {
        start = start.saturating_add(1);
    }
    let last = end.saturating_sub(1);
    if from(last) == 1.0 && to(last) == 1.0 && from(end.saturating_sub(2)) == 1.0 {
        end = last;
    }
    if let Some(i) = (start..end).find(|&i| value == from(i)) {
        let mut j = i;
        while j.saturating_add(1) < end && value == from(j.saturating_add(1)) {
            j = j.saturating_add(1);
        }
        if i == j {
            return to(i);
        }
        if i.saturating_add(2) == j {
            return to(i.saturating_add(1));
        }
        if value < 0.0 {
            return to(j);
        }
        if value > 0.0 {
            return to(i);
        }
        return if to(i).abs() < to(j).abs() {
            to(i)
        } else {
            to(j)
        };
    }
    let i = (start..end).find(|&i| value < from(i)).unwrap_or(end);
    if i == start {
        return value - from(start) + to(start);
    }
    if i == end {
        let e = end.saturating_sub(1);
        return value - from(e) + to(e);
    }
    let b = i.saturating_sub(1);
    to(b) + ((to(i) - to(b)) * (value - from(b))) / (from(i) - from(b))
}

fn map_ft_avar(map: &SegmentMap<'_>, mut value: i32) -> i32 {
    for j in 1..map.len() {
        let Some((from_prev, to_prev)) = map.pair(j.saturating_sub(1)) else {
            break;
        };
        let Some((from, to)) = map.pair(j) else { break };
        let from = i32::from(from.0).saturating_mul(4);
        let from_prev = i32::from(from_prev.0).saturating_mul(4);
        if value < from {
            value = mul_div(
                value.wrapping_sub(from_prev),
                i32::from(to.0)
                    .saturating_mul(4)
                    .wrapping_sub(i32::from(to_prev.0).saturating_mul(4)),
                from.wrapping_sub(from_prev),
            )
            .wrapping_add(i32::from(to_prev.0).saturating_mul(4));
            break;
        }
    }
    value
}

fn ft_to_design(font: &Font<'_>, coords: &[Fixed]) -> Vec<Fixed> {
    let Some(fvar) = font
        .table(Tag::new(b"fvar"))
        .and_then(|d| Fvar::new(d).ok())
    else {
        return Vec::new();
    };
    let avar = font
        .table(Tag::new(b"avar"))
        .and_then(|d| Avar::new(d).ok())
        .filter(|a| a.axis_count() == fvar.axis_count());
    coords
        .iter()
        .enumerate()
        .map(|(index, coord)| {
            let Some(axis) = fvar.axis(index) else {
                return Fixed(0);
            };
            let mut normalized = coord.0;
            if let Some(map) = avar.and_then(|a| a.segment_map(index)) {
                for j in 1..map.len() {
                    let Some((from_prev, to_prev)) = map.pair(j.saturating_sub(1)) else {
                        break;
                    };
                    let Some((from, to)) = map.pair(j) else { break };
                    let to = i32::from(to.0).saturating_mul(4);
                    let to_prev = i32::from(to_prev.0).saturating_mul(4);
                    if normalized < to {
                        normalized = mul_div(
                            normalized.wrapping_sub(to_prev),
                            i32::from(from.0)
                                .saturating_mul(4)
                                .wrapping_sub(i32::from(from_prev.0).saturating_mul(4)),
                            to.wrapping_sub(to_prev),
                        )
                        .wrapping_add(i32::from(from_prev.0).saturating_mul(4));
                        break;
                    }
                }
            }
            let value = if normalized < 0 {
                axis.default
                    .0
                    .wrapping_add(mul_fix(normalized, axis.default.0.wrapping_sub(axis.min.0)))
            } else if normalized > 0 {
                axis.default
                    .0
                    .wrapping_add(mul_fix(normalized, axis.max.0.wrapping_sub(axis.default.0)))
            } else {
                axis.default.0
            };
            Fixed(value)
        })
        .collect()
}

/// A FreeType-side location error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FtLocationError {
    /// A coordinate or selected named instance is invalid.
    InvalidArgument,
}

/// An error evaluating a variable-font value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarError {
    /// FreeType rejected the location before table evaluation.
    FtLocation(FtLocationError),
}

/// A font paired with one immutable location.
#[derive(Clone, Copy, Debug)]
pub struct Instance<'font, 'data, 'loc, 'source> {
    font: &'font Font<'data>,
    location: &'loc Location<'source>,
}

impl<'font, 'data> From<&'font Font<'data>> for Instance<'font, 'data, 'static, 'static> {
    fn from(font: &'font Font<'data>) -> Self {
        font.default_instance()
    }
}

impl<'font, 'data, 'loc, 'source> Instance<'font, 'data, 'loc, 'source> {
    /// The font whose tables this instance reads.
    pub fn font(&self) -> &'font Font<'data> {
        self.font
    }

    /// The normalized location used by this instance.
    pub fn location(&self) -> &'loc Location<'source> {
        self.location
    }

    /// Horizontal advance in font units at this location.
    pub fn advance(&self, glyph: GlyphId) -> u32 {
        self.advance_with_scratch(glyph, &mut crate::GlyphScratch::default())
    }

    /// Horizontal advance using a reusable glyf variation work area.
    pub fn advance_with_scratch(&self, glyph: GlyphId, scratch: &mut crate::GlyphScratch) -> u32 {
        let base = u32::from(
            self.font
                .metrics
                .advance(glyph, self.font.hb_units_per_em() / 2),
        );
        if !self.font.metrics.has_data() || !self.location.hb_nonzero {
            return base;
        }
        let Some(hvar) = self
            .font
            .table(Tag::new(b"HVAR"))
            .and_then(|data| Hvar::new(data).ok())
        else {
            let Some(phantoms) = crate::hb_glyf::phantoms(*self, glyph, scratch) else {
                return base;
            };
            let value = hb_roundf32(phantoms[1].x - phantoms[0].x);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            return value.clamp(0.0, f64::from(u32::MAX)) as u32;
        };
        self.advance_with_delta(glyph, base, StoreKind::Hvar, hvar.advance_map())
    }

    fn advance_with_delta(
        &self,
        glyph: GlyphId,
        base: u32,
        kind: StoreKind,
        map: Result<Option<DeltaSetIndexMap<'_>>, ReadError>,
    ) -> u32 {
        let index = match map {
            Ok(Some(map)) => map.map(glyph.0),
            Ok(None) if glyph.0 <= u32::from(u16::MAX) => glyph.0,
            _ => return base,
        };
        let outer = u16::try_from(index >> 16).unwrap_or(0);
        let inner = u16::try_from(index & 0xffff).unwrap_or(0);
        let delta = self.item_delta::<HbF32>(kind, outer, inner).unwrap_or(0.0);
        let value = f64::from(base) + hb_roundf32(delta);
        if value.is_nan() {
            return 0;
        }
        // The clamp makes this finite value nonnegative and within u32.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let result = value.clamp(0.0, f64::from(u32::MAX)) as u32;
        result
    }

    /// Horizontal font extents in font units at this location.
    pub fn h_extents(&self) -> FontExtents {
        self.scaled_h_extents(&HbScale::unscaled(self.font))
    }

    /// Horizontal font extents at a HarfBuzz scale.
    pub fn scaled_h_extents(&self, scale: &HbScale) -> FontExtents {
        let get = |tag| self.scaled_metrics_position(scale, tag);
        match (
            get(MetricsTag::HorizontalAscender),
            get(MetricsTag::HorizontalDescender),
            get(MetricsTag::HorizontalLineGap),
        ) {
            (Some(ascender), Some(descender), Some(line_gap)) => FontExtents {
                ascender,
                descender,
                line_gap,
            },
            _ => scale.h_extents(self.font),
        }
    }

    /// Vertical font extents in font units at this location.
    pub fn v_extents(&self) -> FontExtents {
        self.scaled_v_extents(&HbScale::unscaled(self.font))
    }

    /// Vertical font extents at a HarfBuzz scale.
    pub fn scaled_v_extents(&self, scale: &HbScale) -> FontExtents {
        let get = |tag| self.scaled_metrics_position(scale, tag);
        match (
            get(MetricsTag::VerticalAscender),
            get(MetricsTag::VerticalDescender),
            get(MetricsTag::VerticalLineGap),
        ) {
            (Some(ascender), Some(descender), Some(line_gap)) => FontExtents {
                ascender,
                descender,
                line_gap,
            },
            _ => scale.v_extents(self.font),
        }
    }

    /// Vertical advance in font units at this location.
    pub fn v_advance(&self, glyph: GlyphId) -> i32 {
        self.v_advance_with_scratch(glyph, &mut crate::GlyphScratch::default())
    }

    /// Vertical advance using a reusable glyf variation work area.
    pub fn v_advance_with_scratch(&self, glyph: GlyphId, scratch: &mut crate::GlyphScratch) -> i32 {
        if !self.font.vmetrics.has_data() {
            let extents = self.h_extents();
            return extents.descender.wrapping_sub(extents.ascender);
        }
        let base = u32::from(
            self.font
                .vmetrics
                .advance(glyph, self.font.hb_units_per_em()),
        );
        let advance = if self.location.hb_nonzero {
            self.font
                .table(Tag::new(b"VVAR"))
                .and_then(|data| Vvar::new(data).ok())
                .map_or_else(
                    || {
                        let Some(phantoms) = crate::hb_glyf::phantoms(*self, glyph, scratch) else {
                            return base;
                        };
                        let value = hb_roundf32(phantoms[2].y - phantoms[3].y);
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let result = value.clamp(0.0, f64::from(u32::MAX)) as u32;
                        result
                    },
                    |vvar| {
                        self.advance_with_delta(glyph, base, StoreKind::Vvar, vvar.advance_map())
                    },
                )
        } else {
            base
        };
        #[allow(clippy::cast_possible_wrap)] // HarfBuzz casts its unsigned advance to int.
        let signed = advance as i32;
        signed.wrapping_neg()
    }

    /// Vertical advance at a HarfBuzz scale, including the font-extents fallback.
    pub fn scaled_v_advance(&self, scale: &HbScale, glyph: GlyphId) -> i32 {
        self.scaled_v_advance_with_scratch(scale, glyph, &mut crate::GlyphScratch::default())
    }

    /// Scaled vertical advance using a reusable glyf variation work area.
    pub fn scaled_v_advance_with_scratch(
        &self,
        scale: &HbScale,
        glyph: GlyphId,
        scratch: &mut crate::GlyphScratch,
    ) -> i32 {
        if !self.font.vmetrics.has_data() {
            let extents = self.scaled_h_extents(scale);
            return extents.descender.wrapping_sub(extents.ascender);
        }
        scale.em_scale_y(self.v_advance_with_scratch(glyph, scratch))
    }

    /// Vertical origin in font units at this location.
    pub fn v_origin(
        &self,
        glyph: GlyphId,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> (i32, i32) {
        let scale = HbScale::unscaled(self.font);
        self.scaled_v_origin(&scale, glyph, |g| {
            extents(g).map(|value| scale.scale_glyph_extents(value))
        })
    }

    /// Vertical origin at a HarfBuzz scale; `extents` gives scaled boxes.
    pub fn scaled_v_origin(
        &self,
        scale: &HbScale,
        glyph: GlyphId,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> (i32, i32) {
        self.scaled_v_origin_with_scratch(
            scale,
            glyph,
            extents,
            &mut crate::GlyphScratch::default(),
        )
    }

    /// Scaled vertical origin using a reusable glyf variation work area.
    pub fn scaled_v_origin_with_scratch(
        &self,
        scale: &HbScale,
        glyph: GlyphId,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
        scratch: &mut crate::GlyphScratch,
    ) -> (i32, i32) {
        #[allow(clippy::cast_possible_wrap)]
        // HarfBuzz passes its unsigned advance as hb_position_t.
        let advance = self.advance(glyph) as i32;
        let x = scale.em_scale_x(advance) / 2;
        if let Some(vorg) = self.font.vorg {
            let base = f32::from(vorg.y_origin(glyph));
            let delta = if self.location.hb_nonzero {
                self.font
                    .table(Tag::new(b"VVAR"))
                    .and_then(|data| Vvar::new(data).ok())
                    .and_then(|vvar| vvar.vorg_map().ok().flatten())
                    .map_or(0.0, |map| {
                        let index = map.map(glyph.0);
                        let outer = u16::try_from(index >> 16).unwrap_or(0);
                        let inner = u16::try_from(index & 0xffff).unwrap_or(0);
                        self.item_delta::<HbF32>(StoreKind::Vvar, outer, inner)
                            .unwrap_or(0.0)
                    })
            } else {
                0.0
            };
            return (x, scale.em_scalef_y(base + delta));
        }
        if self.location.hb_nonzero
            && self.font.vmetrics.has_data()
            && let Some(phantoms) = crate::hb_glyf::phantoms(*self, glyph, scratch)
        {
            return (x, scale.em_scalef_y(phantoms[2].y));
        }
        if !self.font.vmetrics.has_data() {
            let h = self.scaled_h_extents(scale);
            let advance = i64::from(h.ascender).saturating_sub(i64::from(h.descender));
            let y = extents(glyph).map_or(h.ascender, |bounds| {
                let value = i64::from(bounds.y_bearing)
                    .saturating_add(advance.saturating_add(i64::from(bounds.height)) >> 1);
                i32::try_from(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(0)
            });
            return (x, y);
        }
        let (_, y) = scale.v_origin_scaled(self.font, glyph, extents);
        (x, y)
    }

    /// The glyf glyph box in font units at this location.
    pub fn glyf_extents(&self, glyph: GlyphId) -> Option<GlyphExtents> {
        self.glyf_extents_with_scratch(glyph, &mut crate::GlyphScratch::default())
    }

    /// The glyf glyph box using a reusable glyf variation work area.
    pub fn glyf_extents_with_scratch(
        &self,
        glyph: GlyphId,
        scratch: &mut crate::GlyphScratch,
    ) -> Option<GlyphExtents> {
        if self.location.hb_nonzero
            && let Some(extents) = crate::hb_glyf::extents(*self, glyph, scratch)
        {
            return Some(extents);
        }
        HbScale::unscaled(self.font).glyf_extents(self.font, glyph)
    }

    /// A font-wide metric in font units at this location.
    pub fn metrics_position(&self, tag: MetricsTag) -> Option<i32> {
        self.scaled_metrics_position(&HbScale::unscaled(self.font), tag)
    }

    /// A font-wide metric at a HarfBuzz scale.
    pub fn scaled_metrics_position(&self, scale: &HbScale, tag: MetricsTag) -> Option<i32> {
        scale.metrics_position_with_var(self.font, tag, self.hb_mvar_delta(tag.tag()))
    }

    /// A font-wide metric with HarfBuzz's fallback at a scale.
    pub fn scaled_metrics_position_with_fallback(
        &self,
        scale: &HbScale,
        tag: MetricsTag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        scale.with_fallback_using(self.font, tag, &extents, &|tag| {
            self.scaled_metrics_position(scale, tag)
        })
    }

    fn hb_mvar_delta(&self, tag: Tag) -> f32 {
        let record = self
            .font
            .table(Tag::new(b"MVAR"))
            .and_then(|data| Mvar::new(data).ok())
            .and_then(|mvar| mvar.find(tag));
        record.map_or(0.0, |record| {
            self.item_delta::<HbF32>(StoreKind::Mvar, record.outer, record.inner)
                .unwrap_or(0.0)
        })
    }

    /// A font-wide metric with HarfBuzz's fallback at this location.
    pub fn metrics_position_with_fallback(
        &self,
        tag: MetricsTag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        let scale = HbScale::unscaled(self.font);
        self.scaled_metrics_position_with_fallback(&scale, tag, |glyph| {
            extents(glyph).map(|value| scale.scale_glyph_extents(value))
        })
    }

    /// BASE baseline in font units at this location.
    pub fn baseline(&self, tag: Tag, direction: Direction, script: Tag) -> Option<i32> {
        self.scaled_baseline(&HbScale::unscaled(self.font), tag, direction, script)
    }

    /// BASE baseline at a HarfBuzz scale.
    pub fn scaled_baseline(
        &self,
        scale: &HbScale,
        tag: Tag,
        direction: Direction,
        script: Tag,
    ) -> Option<i32> {
        let coord = crate::layout::base_coord(self.font, tag, direction, script)?;
        Some(self.scaled_base_coord(scale, direction, coord))
    }

    fn scaled_base_coord(
        &self,
        scale: &HbScale,
        direction: Direction,
        coord: BaseCoord<'_>,
    ) -> i32 {
        let base = match direction {
            Direction::Horizontal => scale.em_scale_y(i32::from(coord.coordinate())),
            Direction::Vertical => scale.em_scale_x(i32::from(coord.coordinate())),
        };
        if !self.location.hb_nonzero {
            return base;
        }
        let index = coord
            .device()
            .ok()
            .flatten()
            .and_then(|device| device.variation_index());
        let Some((outer, inner)) = index else {
            return base;
        };
        let delta = self
            .item_delta::<HbF32>(StoreKind::Base, outer, inner)
            .unwrap_or(0.0);
        let scaled = match direction {
            Direction::Horizontal => scale.em_scalef_y(delta),
            Direction::Vertical => scale.em_scalef_x(delta),
        };
        base.saturating_add(scaled)
    }

    /// BASE baseline with HarfBuzz's fallback at this location.
    pub fn baseline_with_fallback(
        &self,
        tag: Tag,
        direction: Direction,
        script: Tag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        let scale = HbScale::unscaled(self.font);
        self.scaled_baseline_with_fallback(&scale, tag, direction, script, |glyph| {
            extents(glyph).map(|value| scale.scale_glyph_extents(value))
        })
    }

    /// BASE baseline or HarfBuzz's fallback at a scale; `extents` gives scaled boxes.
    pub fn scaled_baseline_with_fallback(
        &self,
        scale: &HbScale,
        tag: Tag,
        direction: Direction,
        script: Tag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        scale.baseline_fallback_using(
            self.font,
            tag,
            direction,
            script,
            &extents,
            &crate::layout::BaselineFallback {
                baseline: &|tag, direction, script| {
                    self.scaled_baseline(scale, tag, direction, script)
                },
                metric: &|tag| self.scaled_metrics_position_with_fallback(scale, tag, &extents),
                font_extents: &|direction| match direction {
                    Direction::Horizontal => self.scaled_h_extents(scale),
                    Direction::Vertical => self.scaled_v_extents(scale),
                },
            },
        )
    }

    /// Script and language extents from BASE at this location.
    pub fn layout_font_extents(
        &self,
        direction: Direction,
        script: Tag,
        language: Tag,
    ) -> (FontExtents, bool) {
        self.scaled_layout_font_extents(&HbScale::unscaled(self.font), direction, script, language)
    }

    /// Script and language extents from BASE at a HarfBuzz scale.
    pub fn scaled_layout_font_extents(
        &self,
        scale: &HbScale,
        direction: Direction,
        script: Tag,
        language: Tag,
    ) -> (FontExtents, bool) {
        scale.layout_font_extents_using(
            self.font,
            direction,
            script,
            language,
            &|coord| self.scaled_base_coord(scale, direction, coord),
            &|direction| match direction {
                Direction::Horizontal => self.scaled_h_extents(scale),
                Direction::Vertical => self.scaled_v_extents(scale),
            },
        )
    }

    /// FreeType's left side bearing in font units at this location.
    pub fn ft_left_side_bearing(&self, glyph: GlyphId) -> i16 {
        self.font.ft_metrics.left_side_bearing(glyph)
    }

    /// FreeType's unscaled horizontal advance after HVAR adjustment.
    pub fn ft_h_advance(&self, glyph: GlyphId) -> Option<u16> {
        let base = self.font.ft_h_advance_default(glyph)?;
        if self.location.ft_default() {
            return Some(base);
        }
        let Some(hvar) = self
            .font
            .table(Tag::new(b"HVAR"))
            .and_then(|data| Hvar::new(data).ok())
        else {
            return Some(base);
        };
        let index = match hvar.advance_map() {
            Ok(Some(map)) => map.map(glyph.0),
            Ok(None) if glyph.0 <= u32::from(u16::MAX) => glyph.0,
            _ => return Some(base),
        };
        let outer = u16::try_from(index >> 16).unwrap_or(0);
        let inner = u16::try_from(index & 0xffff).unwrap_or(0);
        let delta = self
            .item_delta::<Ft>(StoreKind::Hvar, outer, inner)
            .unwrap_or(0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // `tt_face_get_metrics` stores the adjusted FT_Int as FT_UShort.
        let advance = i32::from(base).wrapping_add(delta) as u16;
        Some(advance)
    }

    /// FreeType's unscaled top side bearing; its VVAR callback is absent.
    pub fn ft_top_side_bearing(&self, glyph: GlyphId, y_max: i16) -> Option<i16> {
        self.font.ft_top_side_bearing_default(glyph, y_max)
    }

    /// FreeType's face metrics at this location.
    pub fn ft_face_metrics(&self) -> crate::FtFaceMetrics {
        let mut metrics = self.font.ft_face_metrics_default();
        if !self.font.ft_has_outline() {
            return metrics;
        }
        let hasc = self.ft_mvar_delta(Tag::new(b"hasc"));
        let hdsc = self.ft_mvar_delta(Tag::new(b"hdsc"));
        let hlgp = self.ft_mvar_delta(Tag::new(b"hlgp"));
        metrics.ascender = metrics.ascender.wrapping_add(hasc);
        metrics.descender = metrics.descender.wrapping_add(hdsc);
        metrics.height = metrics
            .height
            .wrapping_add(hasc)
            .wrapping_sub(hdsc)
            .wrapping_add(hlgp);
        let original_underline_position = metrics
            .underline_position
            .wrapping_add(metrics.underline_thickness / 2);
        metrics.underline_thickness = metrics
            .underline_thickness
            .wrapping_add(self.ft_mvar_delta(Tag::new(b"unds")));
        let varied_underline_position =
            original_underline_position.wrapping_add(self.ft_mvar_delta(Tag::new(b"undo")));
        metrics.underline_position =
            varied_underline_position.wrapping_sub(metrics.underline_thickness / 2);
        metrics
    }

    fn ft_mvar_delta(&self, tag: Tag) -> i16 {
        let record = self
            .font
            .table(Tag::new(b"MVAR"))
            .and_then(|data| Mvar::new(data).ok())
            .and_then(|mvar| mvar.find(tag));
        let value = record.map_or(0, |record| {
            self.item_delta::<Ft>(StoreKind::Mvar, record.outer, record.inner)
                .unwrap_or(0)
        });
        #[allow(clippy::cast_possible_truncation)]
        // FreeType stores each adjusted value as FT_Short.
        let short = value as i16;
        short
    }

    /// FreeType's unscaled vertical advance at this location.
    pub fn ft_v_advance(&self, glyph: GlyphId) -> Option<u16> {
        let base = self.font.ft_v_advance_default(glyph)?;
        if self.font.ft_vertical().is_none() {
            return Some(base);
        }
        let vvar = self
            .font
            .table(Tag::new(b"VVAR"))
            .and_then(|data| Vvar::new(data).ok());
        let Some(vvar) = vvar else {
            return Some(base);
        };
        let index = match vvar.advance_map() {
            Ok(Some(map)) => map.map(glyph.0),
            Ok(None) if glyph.0 <= u32::from(u16::MAX) => glyph.0,
            _ => return Some(base),
        };
        let outer = u16::try_from(index >> 16).unwrap_or(0);
        let inner = u16::try_from(index & 0xffff).unwrap_or(0);
        let delta = self
            .item_delta::<Ft>(StoreKind::Vvar, outer, inner)
            .unwrap_or(0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let advance = i32::from(base).wrapping_add(delta) as u16;
        Some(advance)
    }

    /// HarfBuzz 2.14 normalized coordinates.
    pub fn coords_normalized(&self) -> &[i32] {
        self.location.coords_normalized()
    }

    /// HarfBuzz design coordinates.
    pub fn coords_design(&self) -> &[f32] {
        self.location.coords_design()
    }

    /// FreeType 16.16 normalized coordinates.
    pub fn ft_coords(&self) -> &[Fixed] {
        self.location.ft_coords()
    }

    /// FreeType design coordinates in 16.16.
    pub fn ft_design_coords(&self) -> &[Fixed] {
        self.location.ft_design_coords()
    }

    /// FreeType's PostScript name for this location.
    pub fn postscript_name(&self, out: &mut [u8]) -> Option<usize> {
        if self.location.ft_default {
            return self.font.postscript_name(out);
        }
        let fvar = self
            .font
            .table(Tag::new(b"fvar"))
            .and_then(|data| Fvar::new(data).ok())?;
        if let Some(index) = self.location.named_instance
            && let Some(record) = fvar.instance(usize::from(index))
            && (0..fvar.axis_count())
                .all(|axis| record.coordinate(axis) == self.location.ft_design.get(axis).copied())
            && let Some(psid) = record.postscript_name_id
            && (psid == 6 || (256..32768).contains(&psid))
            && let Some(name) = name_string(self.font, psid)
        {
            return Some(write_name(&name, out));
        }
        let prefix = [25, 16, 1]
            .into_iter()
            .find_map(|id| name_string(self.font, id))?;
        let mut name: String = prefix
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(91)
            .collect();
        let prefix_len = name.len();
        for (index, axis) in fvar.axes().enumerate() {
            let value = self
                .location
                .ft_design
                .get(index)
                .copied()
                .unwrap_or(axis.default);
            if value == axis.default {
                continue;
            }
            name.push('_');
            let number = ft_fixed_to_short_decimal(value);
            name.push_str(&number);
            for byte in axis.tag.to_u32().to_be_bytes() {
                if byte.is_ascii_alphanumeric() {
                    name.push(char::from(byte));
                }
            }
        }
        if name.len().saturating_add(1) > 127 {
            let mut hashed = name.into_bytes();
            hashed.push(0);
            let [h1, h2, h3, h4] = ft_ps_hash(&hashed);
            name = format!(
                "{}-{h1:08X}{h2:08X}{h3:08X}{h4:08X}...",
                String::from_utf8_lossy(hashed.get(..prefix_len).unwrap_or_default())
            );
        }
        Some(write_name(&name, out))
    }

    /// Evaluate one ItemVariationStore row with the selected reference arithmetic.
    pub fn item_delta<A: Arith>(
        &self,
        kind: StoreKind,
        outer: u16,
        inner: u16,
    ) -> Result<A::Delta, VarError> {
        if A::checks_ft_error()
            && let Some(error) = self.location.ft_error
        {
            return Err(VarError::FtLocation(error));
        }
        let zero = A::finish(A::zero());
        let Some(store) = variation_store(self.font, kind) else {
            return Ok(zero);
        };
        let same = self
            .location
            .source
            .is_some_and(|source| source.matches(self.font.sfnt.file(), self.font.face_index));
        if same && let Some(cached) = A::cached(self.location, kind) {
            return Ok(item_delta::<A>(&store, cached, outer, inner));
        }
        let live = OnTheFly::<A>::new(&store, A::coords(self.location), A::supplied(self.location));
        Ok(item_delta::<A>(&store, &live, outer, inner))
    }
}

impl<'data> Font<'data> {
    /// Pair this font with a location. The location may originate from another font.
    pub fn instance<'font, 'loc, 'source>(
        &'font self,
        location: &'loc Location<'source>,
    ) -> Instance<'font, 'data, 'loc, 'source> {
        Instance {
            font: self,
            location,
        }
    }

    /// Pair this font with the static default location.
    pub fn default_instance(&self) -> Instance<'_, 'data, 'static, 'static> {
        self.instance(Location::default_ref())
    }
}

mod sealed {
    pub trait Sealed {}
}

/// Arithmetic policy for ItemVariationStore regions and delta rows.
pub trait Arith: sealed::Sealed {
    /// Normalized coordinate type.
    type Coord: Copy + Default;
    /// Region scalar type.
    type Scalar: Copy + Default + PartialEq;
    /// Delta accumulation type.
    type Acc: Copy;
    /// Finished delta type.
    type Delta: Copy;

    /// Scalar one.
    fn one() -> Self::Scalar;
    /// Fold one region axis into the accumulated scalar.
    fn fold_region_axis(
        acc: Self::Scalar,
        coord: Self::Coord,
        start: i16,
        peak: i16,
        end: i16,
    ) -> Self::Scalar;
    /// Initial delta accumulator.
    fn zero() -> Self::Acc;
    /// Add a scalar times an integer delta in column order.
    fn mul_add(acc: Self::Acc, scalar: Self::Scalar, delta: i32) -> Self::Acc;
    /// Finish the accumulated delta.
    fn finish(acc: Self::Acc) -> Self::Delta;
    /// Normalized coordinates for this arithmetic policy.
    fn coords<'a>(location: &'a Location<'_>) -> &'a [Self::Coord];
    /// Cached scalars for this arithmetic policy.
    fn cached<'a>(
        location: &'a Location<'_>,
        kind: StoreKind,
    ) -> Option<&'a StoreScalars<Self::Scalar>>;
    /// Whether this policy evaluates region scalars for the location.
    fn supplied(location: &Location<'_>) -> bool;
    /// Whether a FreeType location error prevents evaluation.
    fn checks_ft_error() -> bool;
}

/// HarfBuzz's f32 ItemVariationStore arithmetic.
#[derive(Clone, Copy, Debug)]
pub struct HbF32;
/// HarfBuzz's f32 region scalars and f64 delta sum.
#[derive(Clone, Copy, Debug)]
pub struct HbF64;
/// FreeType's 16.16 scalars and 64-bit delta sum.
#[derive(Clone, Copy, Debug)]
pub struct Ft;

impl sealed::Sealed for HbF32 {}
impl sealed::Sealed for HbF64 {}
impl sealed::Sealed for Ft {}

fn hb_fold(acc: f32, coord: i32, start: i16, peak: i16, end: i16) -> f32 {
    let peak = i32::from(peak);
    if peak == 0 || coord == peak {
        return acc;
    }
    if coord == 0 {
        return 0.0;
    }
    let (start, end) = (i32::from(start), i32::from(end));
    if start > peak || peak > end || (start < 0 && end > 0) {
        return acc;
    }
    if coord <= start || end <= coord {
        return 0.0;
    }
    let factor = if coord < peak {
        i32_to_f32(coord.wrapping_sub(start)) / i32_to_f32(peak.wrapping_sub(start))
    } else {
        i32_to_f32(end.wrapping_sub(coord)) / i32_to_f32(end.wrapping_sub(peak))
    };
    acc * factor
}

fn ft_fold(acc: i32, coord: i32, start: i16, peak: i16, end: i16) -> i32 {
    let (start, mut peak, end) = (
        i32::from(start).saturating_mul(4),
        i32::from(peak).saturating_mul(4),
        i32::from(end).saturating_mul(4),
    );
    if start > peak || peak > end || (start < 0 && end > 0) {
        peak = 0;
    }
    if peak == coord || peak == 0 {
        return acc;
    }
    if coord <= start || coord >= end {
        return 0;
    }
    if coord < peak {
        mul_div(acc, coord.wrapping_sub(start), peak.wrapping_sub(start))
    } else {
        mul_div(acc, end.wrapping_sub(coord), end.wrapping_sub(peak))
    }
}

impl Arith for HbF32 {
    type Coord = i32;
    type Scalar = f32;
    type Acc = f32;
    type Delta = f32;
    fn one() -> f32 {
        1.0
    }
    fn fold_region_axis(acc: f32, coord: i32, start: i16, peak: i16, end: i16) -> f32 {
        hb_fold(acc, coord, start, peak, end)
    }
    fn zero() -> f32 {
        0.0
    }
    fn mul_add(acc: f32, scalar: f32, delta: i32) -> f32 {
        acc + scalar * i32_to_f32(delta)
    }
    fn finish(acc: f32) -> f32 {
        acc
    }
    fn coords<'a>(location: &'a Location<'_>) -> &'a [i32] {
        &location.hb_coords
    }
    fn cached<'a>(location: &'a Location<'_>, kind: StoreKind) -> Option<&'a StoreScalars<f32>> {
        location
            .hb_scalars
            .get(kind.index())
            .and_then(Option::as_ref)
    }
    fn supplied(_: &Location<'_>) -> bool {
        true
    }
    fn checks_ft_error() -> bool {
        false
    }
}

impl Arith for HbF64 {
    type Coord = i32;
    type Scalar = f32;
    type Acc = f64;
    type Delta = f64;
    fn one() -> f32 {
        1.0
    }
    fn fold_region_axis(acc: f32, coord: i32, start: i16, peak: i16, end: i16) -> f32 {
        hb_fold(acc, coord, start, peak, end)
    }
    fn zero() -> f64 {
        0.0
    }
    fn mul_add(acc: f64, scalar: f32, delta: i32) -> f64 {
        acc + f64::from(scalar) * f64::from(delta)
    }
    fn finish(acc: f64) -> f64 {
        acc
    }
    fn coords<'a>(location: &'a Location<'_>) -> &'a [i32] {
        &location.hb_coords
    }
    fn cached<'a>(location: &'a Location<'_>, kind: StoreKind) -> Option<&'a StoreScalars<f32>> {
        location
            .hb_scalars
            .get(kind.index())
            .and_then(Option::as_ref)
    }
    fn supplied(_: &Location<'_>) -> bool {
        true
    }
    fn checks_ft_error() -> bool {
        false
    }
}

impl Arith for Ft {
    type Coord = Fixed;
    type Scalar = i32;
    type Acc = i64;
    type Delta = i32;
    fn one() -> i32 {
        0x10000
    }
    fn fold_region_axis(acc: i32, coord: Fixed, start: i16, peak: i16, end: i16) -> i32 {
        ft_fold(acc, coord.0, start, peak, end)
    }
    fn zero() -> i64 {
        0
    }
    fn mul_add(acc: i64, scalar: i32, delta: i32) -> i64 {
        acc.wrapping_add(i64::from(scalar).wrapping_mul(i64::from(delta)))
    }
    fn finish(acc: i64) -> i32 {
        // FreeType casts the rounded FT_INT64 to a 32-bit delta before shifting.
        #[allow(clippy::cast_possible_truncation)]
        let low = acc.wrapping_add(0x8000) as i32;
        low >> 16
    }
    fn coords<'a>(location: &'a Location<'_>) -> &'a [Fixed] {
        &location.ft_coords
    }
    fn cached<'a>(location: &'a Location<'_>, kind: StoreKind) -> Option<&'a StoreScalars<i32>> {
        location
            .ft_scalars
            .get(kind.index())
            .and_then(Option::as_ref)
    }
    fn supplied(location: &Location<'_>) -> bool {
        location.ft_coords_supplied
    }
    fn checks_ft_error() -> bool {
        true
    }
}

/// Dense region scalars, stored once per ItemVariationStore and location.
#[derive(Clone, Debug)]
pub struct StoreScalars<S> {
    regions: Vec<S>,
    all_zero: bool,
}

impl<S: Copy + Default + PartialEq> StoreScalars<S> {
    /// Evaluate every region with `A` and keep scalars in region-number order.
    pub fn from_store<A: Arith<Scalar = S>>(
        store: &ItemVariationStore<'_>,
        coords: &[A::Coord],
        coords_supplied: bool,
    ) -> Self {
        let regions: Vec<S> = (0..store.region_count())
            .map(|index| region_scalar::<A>(store.region(index), coords, coords_supplied))
            .collect();
        let all_zero = regions.iter().all(|value| *value == S::default());
        Self { regions, all_zero }
    }

    /// The scalar of every region, in table order.
    pub fn regions(&self) -> &[S] {
        &self.regions
    }
    /// True when every region has a zero scalar.
    pub fn all_zero(&self) -> bool {
        self.all_zero
    }
}

fn region_scalar<A: Arith>(
    region: Option<Region<'_>>,
    coords: &[A::Coord],
    supplied: bool,
) -> A::Scalar {
    if !supplied {
        return A::Scalar::default();
    }
    let Some(region) = region else {
        return A::Scalar::default();
    };
    let mut value = A::one();
    for index in 0..region.len() {
        let Some((start, peak, end)) = region.axis(index) else {
            return A::Scalar::default();
        };
        let coord = coords.get(index).copied().unwrap_or_default();
        value = A::fold_region_axis(value, coord, start, peak, end);
        if value == A::Scalar::default() {
            break;
        }
    }
    value
}

pub(crate) trait ScalarSource<S> {
    fn scalar(&self, region: u16) -> S;
    fn all_zero(&self) -> bool;
}

/// The borrowed source of a location's cached coefficients.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceFont<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) face_index: u32,
}

impl SourceFont<'_> {
    pub(crate) fn matches(&self, data: &[u8], face_index: u32) -> bool {
        self.face_index == face_index && core::ptr::eq(self.data, data)
    }
}

/// Computes region coefficients against the target store when the source differs.
pub(crate) struct OnTheFly<'a, 'b, A: Arith> {
    store: &'a ItemVariationStore<'b>,
    coords: &'a [A::Coord],
    supplied: bool,
}

impl<'a, 'b, A: Arith> OnTheFly<'a, 'b, A> {
    pub(crate) fn new(
        store: &'a ItemVariationStore<'b>,
        coords: &'a [A::Coord],
        supplied: bool,
    ) -> Self {
        Self {
            store,
            coords,
            supplied,
        }
    }
}

impl<A: Arith> ScalarSource<A::Scalar> for OnTheFly<'_, '_, A> {
    fn scalar(&self, region: u16) -> A::Scalar {
        region_scalar::<A>(self.store.region(region), self.coords, self.supplied)
    }
    fn all_zero(&self) -> bool {
        !self.supplied
            || (0..self.store.region_count()).all(|region| {
                region_scalar::<A>(self.store.region(region), self.coords, true)
                    == A::Scalar::default()
            })
    }
}

impl<S: Copy + Default + PartialEq> ScalarSource<S> for StoreScalars<S> {
    fn scalar(&self, region: u16) -> S {
        self.regions
            .get(usize::from(region))
            .copied()
            .unwrap_or_default()
    }
    fn all_zero(&self) -> bool {
        self.all_zero
    }
}

/// Evaluate a delta row using dense region scalars or on-the-fly scalars.
pub(crate) fn item_delta<A: Arith>(
    store: &ItemVariationStore<'_>,
    source: &impl ScalarSource<A::Scalar>,
    outer: u16,
    inner: u16,
) -> A::Delta {
    let zero = A::finish(A::zero());
    if source.all_zero() {
        return zero;
    }
    let Some(data) = store.item_data(outer) else {
        return zero;
    };
    let count = data.region_index_count();
    if count == 0 || data.delta(inner, count.saturating_sub(1)).is_none() {
        return zero;
    }
    let mut sum = A::zero();
    for column in 0..count {
        let Some(region) = data.region_index(usize::from(column)) else {
            return zero;
        };
        let scalar = source.scalar(region);
        if scalar == A::Scalar::default() {
            continue;
        }
        let Some(delta) = data.delta(inner, column) else {
            return zero;
        };
        sum = A::mul_add(sum, scalar, delta);
    }
    A::finish(sum)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use sofel_read::item_variation::ItemVariationStore;

    fn store(row: &[u8]) -> alloc::vec::Vec<u8> {
        let mut data = alloc::vec![
            0, 1, 0, 0, 0, 12, 0, 1, 0, 0, 0, 22, // store header
            0, 1, 0, 1, 0, 0, 0, 8, 0, 16, // region: [0, 8, 16]
            0, 1, 0, 1, 0, 1, 0, 0, // one item, one 16-bit delta, region 0
        ];
        data.extend_from_slice(row);
        data
    }

    #[test]
    fn i01_ft_cumulative_rounding() {
        let first = Ft::fold_region_axis(0x10000, Fixed(16), 0, 8, 16);
        assert_eq!(first, 32768);
        assert_eq!(Ft::fold_region_axis(first, Fixed(4), 0, 6, 12), 5461);
        assert_eq!(HbF32::fold_region_axis(1.0, 4, 0, 8, 16), 0.5);
        assert_eq!(HbF64::fold_region_axis(1.0, 4, 0, 8, 16), 0.5);
    }

    #[test]
    fn i01_item_finish_and_order() {
        assert_eq!(Ft::finish(Ft::mul_add(0, 32768, 7)), 4);
        assert_eq!(HbF32::finish(HbF32::mul_add(0.0, 0.5, 7)), 3.5);
        assert_eq!(HbF64::finish(HbF64::mul_add(0.0, 0.5, 7)), 3.5);
    }

    #[test]
    fn i05_delta_boundaries_and_i09_dense_regions() {
        let bytes = store(&7i16.to_be_bytes());
        let view = ItemVariationStore::new(&bytes).unwrap();
        let scalars = StoreScalars::<f32>::from_store::<HbF32>(&view, &[4], true);
        assert_eq!(scalars.regions().len(), 1);
        assert_eq!(item_delta::<HbF32>(&view, &scalars, 0, 0), 3.5);
        assert_eq!(item_delta::<HbF32>(&view, &scalars, 0, 1), 0.0);
        assert_eq!(
            item_delta::<HbF32>(&view, &scalars, u16::MAX, u16::MAX),
            0.0
        );
        let cut = store(&[0]);
        let cut = ItemVariationStore::new(&cut).unwrap();
        assert_eq!(item_delta::<HbF32>(&cut, &scalars, 0, 0), 0.0);
    }

    fn compare_sources<A: Arith>(
        view: &ItemVariationStore<'_>,
        coords: &[A::Coord],
        supplied: bool,
        outer: u16,
        inner: u16,
    ) where
        A::Delta: core::fmt::Debug + PartialEq,
    {
        let cached = StoreScalars::<A::Scalar>::from_store::<A>(view, coords, supplied);
        let live = OnTheFly::<A>::new(view, coords, supplied);
        assert_eq!(
            item_delta::<A>(view, &cached, outer, inner),
            item_delta::<A>(view, &live, outer, inner)
        );
    }

    #[test]
    fn i02_cached_equals_on_the_fly() {
        let mut seed = 27_u32;
        for i in 0..1000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let octets = seed.to_le_bytes();
            let mut bytes = store(&octets[..2]);
            if i % 7 == 0 {
                bytes
                    .get_mut(16..22)
                    .unwrap()
                    .copy_from_slice(&[0xff, 0xff, 0, 8, 0, 16]);
            }
            if i % 11 == 0 {
                bytes
                    .get_mut(28..30)
                    .unwrap()
                    .copy_from_slice(&u16::MAX.to_be_bytes());
            }
            let view = ItemVariationStore::new(&bytes).unwrap();
            let coord = match i % 5 {
                0 | 1 => 0,
                _ => i32::from(octets[2] % 24) - 4,
            };
            let hb = [coord];
            let ft = [Fixed(coord * 4)];
            let empty = i % 5 == 0;
            let outer = if i % 17 == 0 { u16::MAX } else { 0 };
            let inner = if i % 19 == 0 { 1 } else { 0 };
            compare_sources::<HbF32>(&view, if empty { &[] } else { &hb }, !empty, outer, inner);
            compare_sources::<HbF64>(&view, if empty { &[] } else { &hb }, !empty, outer, inner);
            compare_sources::<Ft>(&view, if empty { &[] } else { &ft }, !empty, outer, inner);
        }
    }

    #[test]
    fn i09_shared_data_memory() {
        let single = store(&7i16.to_be_bytes());
        for count in [1_u16, u16::MAX] {
            let regions_offset = 8 + 4 * u32::from(count);
            let mut bytes = Vec::from(1u16.to_be_bytes());
            bytes.extend_from_slice(&regions_offset.to_be_bytes());
            bytes.extend_from_slice(&count.to_be_bytes());
            let data_offset = (regions_offset + 10).to_be_bytes();
            for _ in 0..count {
                bytes.extend_from_slice(&data_offset);
            }
            bytes.extend_from_slice(single.get(12..).unwrap());
            let view = ItemVariationStore::new(&bytes).unwrap();
            let scalars = StoreScalars::<f32>::from_store::<HbF32>(&view, &[4], true);
            assert_eq!(scalars.regions(), &[0.5]);
            assert_eq!(item_delta::<HbF32>(&view, &scalars, count - 1, 0), 3.5);
        }
    }

    #[test]
    fn i03_source_identity_checks_slice_and_face() {
        let data = [1, 2, 3, 4];
        let copy = data;
        let source = SourceFont {
            data: &data,
            face_index: 1,
        };
        assert!(source.matches(&data, 1));
        assert!(!source.matches(&data, 0));
        assert!(!source.matches(&data[..3], 1));
        assert!(!source.matches(&copy, 1));
    }
}
