//! Variable-font coordinates and variation arithmetic (ADR 0027, ADR 0030).

use alloc::vec::Vec;
use alloc::{format, string::String};
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf32, i32_to_f32};
use sofel_geom::{div_fix, mul_div, mul_fix};
use sofel_read::item_variation::{ItemVariationStore, Region};
use sofel_read::{
    Fixed, Tag,
    avar::{Avar, SegmentMap},
    base::Base,
    colr::Colr,
    fvar::Fvar,
    gdef::Gdef,
    hvar::{Hvar, Vvar},
    mvar::Mvar,
};

use crate::{Cff, Font};

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
            return Self::from_coords(font, Vec::new(), Vec::new(), false);
        };
        let mut design: Vec<f32> = fvar.axes().map(|axis| fixed_to_f32(axis.default)).collect();
        let named = named_instance.and_then(|i| fvar.instance(usize::from(i)));
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
        if named_instance.is_some() && named.is_none() {
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

impl<'font, 'data, 'loc, 'source> Instance<'font, 'data, 'loc, 'source> {
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

    #[test]
    fn i02_cached_equals_on_the_fly() {
        let bytes = store(&7i16.to_be_bytes());
        let view = ItemVariationStore::new(&bytes).unwrap();
        let cached = StoreScalars::<f32>::from_store::<HbF32>(&view, &[4], true);
        let live = OnTheFly::<HbF32>::new(&view, &[4], true);
        assert_eq!(
            item_delta::<HbF32>(&view, &cached, 0, 0),
            item_delta::<HbF32>(&view, &live, 0, 0)
        );
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
