//! The axes of the font and their normalization for instancing
//! (`hb_ot_var_get_axis_infos` and `normalize_axes_location` of
//! `src/hb-subset-plan-var.cc`), with HarfBuzz's float arithmetic
//! (ADR 0022, ADR 0026).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use sofel_font::Font;
use sofel_font::read::avar::Avar;
use sofel_font::read::fvar::Fvar;
use sofel_font::read::{Fixed, Tag};
use sofel_font::var::{map_float, normalize_axis_value};
use sofel_geom::float::{hb_roundf, hb_roundf32, i32_to_f32};

use crate::input::AxisRange;

/// An axis as `hb_ot_var_axis_info_t` gives it: the range ordered
/// around the default (`AxisRecord::get_coordinates`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct AxisInfo {
    pub tag: Tag,
    pub min: f32,
    pub default: f32,
    pub max: f32,
    pub hidden: bool,
}

/// `F16DOT16::to_float`.
pub(crate) fn fixed_to_f32(f: Fixed) -> f32 {
    i32_to_f32(f.0) * (1.0 / 65536.0)
}

/// `F2DOT14::to_float`.
pub(crate) fn f2dot14_to_f32(v: i16) -> f32 {
    i32_to_f32(i32::from(v)) * (1.0 / 16384.0)
}

/// `(float) v` for a `double`, rounding to nearest as C does.
pub(crate) fn f64_to_f32(v: f64) -> f32 {
    // Conversion to the nearest representable value is what C does.
    #[allow(clippy::cast_possible_truncation)]
    let f = v as f32;
    f
}

/// `(int) v` for an integral `double` (HarfBuzz assigns `roundf`'s
/// result to an `int`); saturating where C is undefined.
pub(crate) fn f64_to_i32(v: f64) -> i32 {
    sofel_geom::float::hb_clamp_to_i32(v)
}

/// The low 16 bits of `v`, as C stores an `int` in a 16-bit field.
pub(crate) fn lo16(v: i32) -> u16 {
    (v.cast_unsigned() & 0xFFFF) as u16
}

/// The low 8 bits of `v`.
pub(crate) fn lo8(v: i32) -> u8 {
    (v.cast_unsigned() & 0xFF) as u8
}

/// The font's `fvar`, as HarfBuzz's sanitizer accepts it.
pub(crate) fn fvar<'a>(font: &Font<'a>) -> Option<Fvar<'a>> {
    font.table(Tag::new(b"fvar"))
        .and_then(|d| Fvar::new(d).ok())
}

/// The font's `avar`, as HarfBuzz's sanitizer accepts it.
pub(crate) fn avar<'a>(font: &Font<'a>) -> Option<Avar<'a>> {
    font.table(Tag::new(b"avar"))
        .and_then(|d| Avar::new(d).ok())
}

/// `hb_ot_var_get_axis_infos`: the axes, none without a valid `fvar`.
pub(crate) fn axis_infos(font: &Font<'_>) -> Vec<AxisInfo> {
    fvar(font).map_or_else(Vec::new, |f| {
        f.axes()
            .map(|a| {
                let (min, default, max) = a.ordered();
                AxisInfo {
                    tag: a.tag,
                    min: fixed_to_f32(min),
                    default: fixed_to_f32(default),
                    max: fixed_to_f32(max),
                    hidden: a.is_hidden(),
                }
            })
            .collect()
    })
}

/// `hb_clamp` of floats: `hb_min (hb_max (v, lo), hi)`, where
/// `hb_max (a, b)` is `a >= b ? a : b` and `hb_min (a, b)` is
/// `a <= b ? a : b` (so NaN gives `lo`).
pub(crate) fn clamp_f32(v: f32, lo: f32, hi: f32) -> f32 {
    let m = if v >= lo { v } else { lo };
    if m <= hi { m } else { hi }
}

/// `hb_clamp` of doubles.
pub(crate) fn clamp_f64(v: f64, lo: f64, hi: f64) -> f64 {
    let m = if v >= lo { v } else { lo };
    if m <= hi { m } else { hi }
}

/// `roundf (v * 16384.f) / 16384.f`: rounded to 2.14.
fn round_2_14(v: f32) -> f32 {
    f64_to_f32(hb_roundf32(v * 16384.0) / 16384.0)
}

/// `avar::map_coords_2_14` of version 1 maps: each of the first
/// `min (len, axisCount)` coordinates mapped and rounded to 2.14.
fn map_coords_2_14(avar: &Avar<'_>, coords: &mut [f32]) {
    let count = coords.len().min(avar.axis_count());
    for (i, c) in coords.iter_mut().enumerate().take(count) {
        let mapped = avar.segment_map(i).map_or(*c, |m| map_float(&m, *c));
        let v = f64_to_i32(hb_roundf32(mapped * 16384.0));
        *c = i32_to_f32(v) / 16384.0;
    }
}

/// `head_maxp_info_t`: the bounds and maxp values found while
/// instancing the outlines.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct HeadMaxp {
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
    pub max_points: u32,
    pub max_contours: u32,
    pub max_composite_points: u32,
    pub max_composite_contours: u32,
    pub max_component_elements: u32,
    pub max_component_depth: u32,
    pub all_x_min_is_lsb: bool,
}

impl Default for HeadMaxp {
    fn default() -> HeadMaxp {
        HeadMaxp {
            x_min: 0x7FFF,
            x_max: -0x7FFF,
            y_min: 0x7FFF,
            y_max: -0x7FFF,
            max_points: 0,
            max_contours: 0,
            max_composite_points: 0,
            max_composite_contours: 0,
            max_component_elements: 0,
            max_component_depth: 0,
            all_x_min_is_lsb: true,
        }
    }
}

impl HeadMaxp {
    /// The bounds zeroed when no glyph set them (the inverted sentinels
    /// are not written).
    pub fn settle(&mut self) {
        if self.x_min > self.x_max {
            self.x_min = 0;
            self.x_max = 0;
            self.y_min = 0;
            self.y_max = 0;
        }
    }
}

/// The metrics HarfBuzz recomputes when instancing (`head_maxp_info`,
/// `hmtx_map`, `vmtx_map`, `bounds_width_vec`, `bounds_height_vec`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Metrics {
    pub head_maxp: HeadMaxp,
    /// New glyph id to `(advance, side bearing)`.
    pub hmtx: BTreeMap<u32, (u32, i32)>,
    pub vmtx: BTreeMap<u32, (u32, i32)>,
    /// Per new glyph id, `u32::MAX` when unset.
    pub bounds_width: Vec<u32>,
    pub bounds_height: Vec<u32>,
}

/// The normalized axis location of a plan (`normalize_axes_location`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Normalized {
    /// `user_axes_location` after dropping the ranges equal to `fvar`'s.
    pub user: BTreeMap<Tag, AxisRange>,
    /// `axes_location`: the normalized ranges.
    pub axes_location: BTreeMap<Tag, AxisRange>,
    /// `normalized_coords`: per `fvar` axis, in 2.14; empty when no axis
    /// is set.
    pub normalized_coords: Vec<i32>,
    /// `axes_old_index_tag_map`: the tag of each `fvar` axis.
    pub axis_tags: Vec<Tag>,
    /// Old `fvar` indices retained in the output, in source order.
    pub retained_axis_indices: Vec<usize>,
    pub all_axes_pinned: bool,
    pub pinned_at_default: bool,
    /// Whether `avar` is version 2 (HarfBuzz's `has_v2_data`).
    pub avar2: bool,
}

/// `normalize_axes_location` for `user` on `font` (the version 1 maps
/// of `avar`; [`Normalized::avar2`] tells the caller about version 2,
/// which this version does not instance).
pub(crate) fn normalize(font: &Font<'_>, user: &BTreeMap<Tag, AxisRange>) -> Normalized {
    let mut n = Normalized {
        user: user.clone(),
        pinned_at_default: true,
        ..Normalized::default()
    };
    // `all_axes_pinned` is false until the axes are normalized.
    if n.user.is_empty() {
        return n;
    }
    let axes = axis_infos(font);
    n.retained_axis_indices = retained_axis_indices(&axes, &n.user);
    for a in &axes {
        let same = n.user.get(&a.tag).is_some_and(|r| {
            !(r.min == r.default && r.default == r.max)
                && r.min == a.min
                && r.default == a.default
                && r.max == a.max
        });
        if same {
            n.user.remove(&a.tag);
        }
    }
    if n.user.is_empty() {
        return n;
    }
    n.normalized_coords = alloc::vec![0; axes.len()];
    let avar = avar(font);
    n.avar2 = avar.is_some_and(|a| a.major_version() == 2);
    let mut mins = alloc::vec![0.0f32; axes.len()];
    let mut defaults = alloc::vec![0.0f32; axes.len()];
    let mut maxs = alloc::vec![0.0f32; axes.len()];
    let mut not_pinned = false;
    let mut last = 0usize;
    for (i, a) in axes.iter().enumerate() {
        n.axis_tags.push(a.tag);
        let range = n.user.get(&a.tag).copied();
        if range.is_none_or(|r| !(r.min == r.default && r.default == r.max)) {
            not_pinned = true;
        }
        let Some(r) = range else {
            continue;
        };
        let min = round_2_14(normalize_axis_value(a.min, a.default, a.max, r.min));
        let def = round_2_14(normalize_axis_value(a.min, a.default, a.max, r.default));
        let max = round_2_14(normalize_axis_value(a.min, a.default, a.max, r.max));
        if avar.is_some() {
            if let (Some(m), Some(d), Some(x)) =
                (mins.get_mut(i), defaults.get_mut(i), maxs.get_mut(i))
            {
                *m = min;
                *d = def;
                *x = max;
            }
            last = i;
        } else {
            n.axes_location.insert(
                a.tag,
                AxisRange {
                    min,
                    default: def,
                    max,
                },
            );
            set_coord(&mut n, i, def);
        }
    }
    n.all_axes_pinned = !not_pinned;
    if let Some(avar) = avar {
        let len = last.saturating_add(1);
        for v in [&mut mins, &mut defaults, &mut maxs] {
            map_coords_2_14(&avar, v.get_mut(..len).unwrap_or(&mut []));
        }
        for (i, a) in axes.iter().enumerate() {
            if !n.user.contains_key(&a.tag) {
                continue;
            }
            let at = |v: &[f32]| v.get(i).copied().unwrap_or(0.0);
            let def = at(&defaults);
            n.axes_location.insert(
                a.tag,
                AxisRange {
                    min: at(&mins),
                    default: def,
                    max: at(&maxs),
                },
            );
            set_coord(&mut n, i, def);
        }
    }
    n
}

/// `axes_index_map`'s source indices in the order used by the output fvar.
fn retained_axis_indices(axes: &[AxisInfo], user: &BTreeMap<Tag, AxisRange>) -> Vec<usize> {
    axes.iter()
        .enumerate()
        .filter_map(|(index, axis)| {
            let pinned = user
                .get(&axis.tag)
                .is_some_and(|range| range.min == range.default && range.default == range.max);
            (!pinned).then_some(index)
        })
        .collect()
}

/// The default of axis `i` set into `normalized_coords`, and
/// `pinned_at_default` updated.
fn set_coord(n: &mut Normalized, i: usize, def: f32) {
    // -0 compares equal to 0.
    if def != 0.0 {
        n.pinned_at_default = false;
    }
    if let Some(c) = n.normalized_coords.get_mut(i) {
        *c = f64_to_i32(hb_roundf(f64::from(def * 16384.0)));
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    fn axis(min: f32, default: f32, max: f32) -> AxisInfo {
        AxisInfo {
            tag: Tag::new(b"wght"),
            min,
            default,
            max,
            hidden: false,
        }
    }

    #[test]
    fn normalizes_as_harfbuzz() {
        let a = axis(100.0, 400.0, 900.0);
        assert_eq!(normalize_axis_value(a.min, a.default, a.max, 400.0), 0.0);
        assert_eq!(normalize_axis_value(a.min, a.default, a.max, 100.0), -1.0);
        assert_eq!(normalize_axis_value(a.min, a.default, a.max, 50.0), -1.0);
        assert_eq!(normalize_axis_value(a.min, a.default, a.max, 650.0), 0.5);
        assert_eq!(round_2_14(1.0 / 3.0), 5461.0 / 16384.0);
    }

    #[test]
    fn i1_subset_instance_maps_old_axis_indices_after_partial_pin() {
        let axes = [
            axis(100.0, 400.0, 900.0),
            AxisInfo {
                tag: Tag::new(b"wdth"),
                ..axis(75.0, 100.0, 125.0)
            },
            AxisInfo {
                tag: Tag::new(b"opsz"),
                ..axis(8.0, 12.0, 72.0)
            },
        ];
        let user = BTreeMap::from([
            (
                Tag::new(b"wght"),
                AxisRange {
                    min: 400.0,
                    default: 400.0,
                    max: 400.0,
                },
            ),
            (
                Tag::new(b"wdth"),
                AxisRange {
                    min: 80.0,
                    default: 100.0,
                    max: 120.0,
                },
            ),
        ]);
        assert_eq!(retained_axis_indices(&axes, &user), alloc::vec![1, 2]);
    }
}

/// The instancing part of a plan.
#[derive(Clone, Debug, Default)]
pub(crate) struct VarPlan {
    pub n: Normalized,
    pub metrics: Metrics,
    /// The glyf glyphs compiled at the default location, by position in
    /// the glyph mapping (when glyf is instanced).
    pub glyf: Option<Vec<Vec<u8>>>,
}

impl VarPlan {
    /// `plan->normalized_coords` is not empty.
    pub fn instancing(&self) -> bool {
        !self.n.normalized_coords.is_empty()
    }

    /// `plan->all_axes_pinned`.
    pub fn all_axes_pinned(&self) -> bool {
        self.n.all_axes_pinned
    }

    /// `plan->user_axes_location`.
    pub fn user(&self) -> &BTreeMap<Tag, AxisRange> {
        &self.n.user
    }
}
