//! Layout queries at one font location, scale and pixel size (ADR 0062).

use alloc::vec::Vec;
use sofel_geom::float::i32_to_f32;
use sofel_geom::tan;
use sofel_read::{
    GlyphId, Tag,
    base::BaseCoord,
    be,
    device::Device,
    gdef::{CaretValue, Gdef},
    layout::LayoutTable,
    math::{Corner, Math, MathGlyphConstruction, MathValue},
    post::Post,
    stat::{AxisValue, Stat},
};

use crate::{
    Direction, Font, FontExtents, GlyphExtents, HbScale,
    var::{HbF32, Instance, StoreKind},
};

/// One scaled MATH kern value and its exclusive upper height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathKernEntry {
    /// The maximum correction height, or `i32::MAX` for the last value.
    pub max_correction_height: i32,
    /// The scaled horizontal kern.
    pub kern_value: i32,
}

/// One size variant of a MATH glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathGlyphVariant {
    /// Variant glyph identifier.
    pub glyph: GlyphId,
    /// Scaled advance in the stretch direction.
    pub advance: i32,
}

/// One scaled part of a MATH glyph assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MathGlyphPart {
    /// Part glyph identifier.
    pub glyph: GlyphId,
    /// Scaled start connector length.
    pub start_connector_length: i32,
    /// Scaled end connector length.
    pub end_connector_length: i32,
    /// Scaled full advance.
    pub full_advance: i32,
    /// The OpenType part flags.
    pub flags: u16,
}

/// The assembly parts and their italic correction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathAssembly {
    /// Scaled italic correction.
    pub italics_correction: i32,
    /// Scaled parts in table order.
    pub parts: Vec<MathGlyphPart>,
}

/// A font instance and the dimensions used to evaluate layout values.
#[derive(Clone, Copy, Debug)]
pub struct LayoutQueries<'font, 'data, 'loc, 'source> {
    instance: Instance<'font, 'data, 'loc, 'source>,
    scale: HbScale,
    x_ppem: u32,
    y_ppem: u32,
}

impl<'font, 'data, 'loc, 'source> Instance<'font, 'data, 'loc, 'source> {
    /// Prepare layout queries using the same dimensions as shaping.
    pub fn layout_queries(
        &self,
        scale: HbScale,
        x_ppem: u32,
        y_ppem: u32,
    ) -> LayoutQueries<'font, 'data, 'loc, 'source> {
        LayoutQueries {
            instance: *self,
            scale,
            x_ppem,
            y_ppem,
        }
    }
}

impl<'font, 'data, 'loc, 'source> LayoutQueries<'font, 'data, 'loc, 'source> {
    fn font(&self) -> &'font Font<'data> {
        self.instance.font()
    }

    fn math(&self) -> Option<Math<'data>> {
        self.font()
            .table(Tag::new(b"MATH"))
            .and_then(|data| Math::new(data).ok())
    }

    pub(crate) fn base_coord(&self, coordinate: BaseCoord<'_>, direction: Direction) -> i32 {
        let horizontal = direction == Direction::Vertical;
        let base = self.scale_direction(
            i32::from(coordinate.coordinate()),
            match direction {
                Direction::Horizontal => Direction::Vertical,
                Direction::Vertical => Direction::Horizontal,
            },
        );
        base.saturating_add(self.device_delta(
            coordinate.device().ok().flatten(),
            horizontal,
            StoreKind::Base,
        ))
    }

    /// BASE baseline at the query's scale and pixel size.
    pub fn baseline(&self, tag: Tag, direction: Direction, script: Tag) -> Option<i32> {
        let coord = crate::layout::base_coord(self.font(), tag, direction, script)?;
        Some(self.base_coord(coord, direction))
    }

    /// BASE baseline or the HarfBuzz fallback at the query's scale and size.
    pub fn baseline_with_fallback(
        &self,
        tag: Tag,
        direction: Direction,
        script: Tag,
        extents: impl Fn(GlyphId) -> Option<GlyphExtents>,
    ) -> i32 {
        self.scale.baseline_fallback_using(
            self.font(),
            tag,
            direction,
            script,
            &extents,
            &crate::layout::BaselineFallback {
                baseline: &|tag, direction, script| self.baseline(tag, direction, script),
                metric: &|tag| {
                    self.instance
                        .scaled_metrics_position_with_fallback(&self.scale, tag, &extents)
                },
                font_extents: &|direction| match direction {
                    Direction::Horizontal => self.instance.scaled_h_extents(&self.scale),
                    Direction::Vertical => self.instance.scaled_v_extents(&self.scale),
                },
            },
        )
    }

    /// BASE script and language extents at the query's scale and size.
    pub fn layout_font_extents(
        &self,
        direction: Direction,
        script: Tag,
        language: Tag,
    ) -> (FontExtents, bool) {
        self.scale.layout_font_extents_using(
            self.font(),
            direction,
            script,
            language,
            &|coord| self.base_coord(coord, direction),
            &|direction| match direction {
                Direction::Horizontal => self.instance.scaled_h_extents(&self.scale),
                Direction::Vertical => self.instance.scaled_v_extents(&self.scale),
            },
        )
    }

    /// Whether a readable MATH table exists.
    pub fn has_math(&self) -> bool {
        self.math().is_some()
    }

    fn stat_value(&self, tag: Tag) -> Option<f32> {
        let stat = Stat::new(self.font().table(Tag::new(b"STAT"))?).ok()?;
        let index = stat.design_axes().position(|axis| axis.tag == tag)?;
        let index = u16::try_from(index).ok()?;
        (0..stat.axis_value_count()).find_map(|i| {
            let (value, _) = stat.axis_value(i)?;
            let fixed = match value {
                AxisValue::Format1 {
                    axis_index, value, ..
                }
                | AxisValue::Format3 {
                    axis_index, value, ..
                } if axis_index == index => value,
                AxisValue::Format2 {
                    axis_index,
                    nominal,
                    ..
                } if axis_index == index => nominal,
                _ => return None,
            };
            Some(i32_to_f32(fixed.0) / 65536.0)
        })
    }

    fn gpos_design_size(&self) -> Option<u16> {
        let table = LayoutTable::new(self.font().table(Tag::new(b"GPOS"))?).ok()?;
        let features = table.feature_list().ok().flatten()?;
        (0..features.len()).find_map(|index| {
            if features.tag(index)? != Tag::new(b"size") {
                return None;
            }
            let feature = features.feature(index).ok().flatten()?;
            let offset = usize::from(feature.feature_params_offset());
            let design_size = be::u16_at(feature.data(), offset)?;
            (design_size != 0).then_some(design_size)
        })
    }

    /// A HarfBuzz style tag at this font location.
    pub fn style_value(&self, tag: Tag) -> f32 {
        if tag == Tag::new(b"Slnt") {
            let angle = self.style_value(Tag::new(b"slnt"));
            return tan(angle * -core::f32::consts::PI / 180.0);
        }
        if let Some(axis) = self.font().find_var_axis(tag) {
            return usize::try_from(axis.index)
                .ok()
                .and_then(|index| self.instance.location().coords_design().get(index).copied())
                .unwrap_or(axis.default);
        }
        if let Some(value) = self.stat_value(tag) {
            return value;
        }
        let os2 = self.font().os2();
        let mac_style = self.font().head().mac_style();
        if tag == Tag::new(b"ital") {
            return f32::from(u8::from(
                os2.is_some_and(|table| table.fs_selection() & 1 != 0) || mac_style & 2 != 0,
            ));
        }
        if tag == Tag::new(b"opsz") {
            if let Some((lower, upper)) = os2.and_then(|table| {
                Some((
                    table.us_lower_optical_point_size()?,
                    table.us_upper_optical_point_size()?,
                ))
            }) && lower < upper
                && lower >= 1
                && upper >= 2
            {
                return i32_to_f32(i32::from(lower).saturating_add(i32::from(upper))) / 2.0;
            }
            if let Some(design_size) = self.gpos_design_size() {
                return f32::from(design_size) / 10.0;
            }
            return 12.0;
        }
        if tag == Tag::new(b"slnt") {
            return self
                .font()
                .table(Tag::new(b"post"))
                .and_then(|data| Post::new(data).ok())
                .map_or(0.0, |post| i32_to_f32(post.italic_angle().0) / 65536.0);
        }
        if tag == Tag::new(b"wdth") {
            return os2.map_or_else(
                || {
                    if mac_style & 0x20 != 0 {
                        75.0
                    } else if mac_style & 0x40 != 0 {
                        125.0
                    } else {
                        100.0
                    }
                },
                |table| match table.us_width_class() {
                    1 => 50.0,
                    2 => 62.5,
                    3 => 75.0,
                    4 => 87.5,
                    6 => 112.5,
                    7 => 125.0,
                    8 => 150.0,
                    9 => 200.0,
                    _ => 100.0,
                },
            );
        }
        if tag == Tag::new(b"wght") {
            return os2.map_or_else(
                || if mac_style & 1 != 0 { 700.0 } else { 400.0 },
                |table| f32::from(table.us_weight_class()),
            );
        }
        0.0
    }

    fn device_delta(&self, device: Option<Device<'_>>, horizontal: bool, store: StoreKind) -> i32 {
        let Some(device) = device else { return 0 };
        if let Some((outer, inner)) = device.variation_index() {
            if !self.instance.location().hb_nonzero() {
                return 0;
            }
            let delta = self
                .instance
                .item_delta::<HbF32>(store, outer, inner)
                .unwrap_or(0.0);
            return if horizontal {
                self.scale.em_scalef_x(delta)
            } else {
                self.scale.em_scalef_y(delta)
            };
        }
        let (ppem, scale) = if horizontal {
            (self.x_ppem, self.scale.x_scale())
        } else {
            (self.y_ppem, self.scale.y_scale())
        };
        if ppem == 0 {
            return 0;
        }
        let pixels = device.delta_pixels(ppem);
        let scaled = i64::from(pixels)
            .wrapping_mul(i64::from(scale))
            .checked_div(i64::from(ppem))
            .unwrap_or(0);
        let bytes = scaled.to_le_bytes();
        i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }

    fn variation_delta(&self, device: Option<&Device<'_>>, store: StoreKind) -> Option<f32> {
        let (outer, inner) = device?.variation_index()?;
        if !self.instance.location().hb_nonzero() {
            return None;
        }
        Some(
            self.instance
                .item_delta::<HbF32>(store, outer, inner)
                .unwrap_or(0.0),
        )
    }

    /// Horizontal Device or GDEF VariationIndex delta.
    pub fn x_device_delta(&self, device: Option<&Device<'_>>) -> i32 {
        self.device_delta(device.copied(), true, StoreKind::Gdef)
    }

    /// Vertical Device or GDEF VariationIndex delta.
    pub fn y_device_delta(&self, device: Option<&Device<'_>>) -> i32 {
        self.device_delta(device.copied(), false, StoreKind::Gdef)
    }

    /// Horizontal delta before HarfBuzz's final floating-point rounding.
    pub fn x_device_fdelta(&self, device: Option<&Device<'_>>) -> f32 {
        self.variation_delta(device, StoreKind::Gdef).map_or_else(
            || i32_to_f32(self.x_device_delta(device)),
            |delta| {
                delta
                    * (i32_to_f32(self.scale.x_scale()) / f32::from(self.font().hb_units_per_em()))
            },
        )
    }

    /// Vertical delta before HarfBuzz's final floating-point rounding.
    pub fn y_device_fdelta(&self, device: Option<&Device<'_>>) -> f32 {
        self.variation_delta(device, StoreKind::Gdef).map_or_else(
            || i32_to_f32(self.y_device_delta(device)),
            |delta| {
                delta
                    * (i32_to_f32(self.scale.y_scale()) / f32::from(self.font().hb_units_per_em()))
            },
        )
    }

    fn math_value(&self, value: MathValue<'_>, horizontal: bool) -> i32 {
        let base = if horizontal {
            self.scale.em_scale_x(i32::from(value.value()))
        } else {
            self.scale.em_scale_y(i32::from(value.value()))
        };
        base.saturating_add(self.device_delta(
            value.device().ok().flatten(),
            horizontal,
            StoreKind::Gdef,
        ))
    }

    fn scale_direction(&self, value: i32, direction: Direction) -> i32 {
        match direction {
            Direction::Horizontal => self.scale.em_scale_x(value),
            Direction::Vertical => self.scale.em_scale_y(value),
        }
    }

    /// A `hb_ot_math_constant_t` value, or zero without MATH.
    pub fn math_constant(&self, constant: u32) -> i32 {
        let Some(constants) = self.math().and_then(|math| math.constants().ok().flatten()) else {
            return 0;
        };
        match constant {
            0 => i32::from(constants.script_percent_scale_down()),
            1 => i32::from(constants.script_script_percent_scale_down()),
            2 => self
                .scale
                .em_scale_y(i32::from(constants.delimited_sub_formula_min_height())),
            3 => self
                .scale
                .em_scale_y(i32::from(constants.display_operator_min_height())),
            4..=54 => constants
                .value(usize::try_from(constant.saturating_sub(4)).unwrap_or(0))
                .map_or(0, |value| {
                    self.math_value(value, matches!(constant, 17 | 41 | 53 | 54))
                }),
            55 => i32::from(constants.radical_degree_bottom_raise_percent()),
            _ => 0,
        }
    }

    /// Italics correction of a MATH glyph, or zero when absent.
    pub fn math_italics_correction(&self, glyph: GlyphId) -> i32 {
        self.math()
            .and_then(|math| math.glyph_info().ok().flatten())
            .and_then(|info| info.italics_correction().ok().flatten())
            .and_then(|table| table.get(glyph))
            .map_or(0, |value| self.math_value(value, true))
    }

    /// Top accent attachment, falling back to half the glyph advance.
    pub fn math_top_accent_attachment(&self, glyph: GlyphId) -> i32 {
        self.math()
            .and_then(|math| math.glyph_info().ok().flatten())
            .and_then(|info| info.top_accent_attachment().ok().flatten())
            .and_then(|table| table.get(glyph))
            .map_or_else(
                || {
                    self.scale
                        .em_scale_x(self.instance.advance(glyph).cast_signed())
                        / 2
                },
                |value| self.math_value(value, true),
            )
    }

    /// Whether the glyph is marked as an extended MATH shape.
    pub fn math_is_extended_shape(&self, glyph: GlyphId) -> bool {
        self.math()
            .and_then(|math| math.glyph_info().ok().flatten())
            .and_then(|info| info.extended_shape_coverage().ok().flatten())
            .is_some_and(|coverage| coverage.get(glyph).is_some())
    }

    fn math_kern(
        &self,
        glyph: GlyphId,
        corner: Corner,
    ) -> Option<sofel_read::math::MathKern<'data>> {
        let info = self
            .math()?
            .glyph_info()
            .ok()
            .flatten()?
            .kern_info()
            .ok()
            .flatten()?;
        let index = usize::try_from(info.coverage().ok().flatten()?.get(glyph)?).ok()?;
        info.kern(index, corner).ok().flatten()
    }

    /// All scaled MATH kern entries for one glyph and corner.
    pub fn math_kernings(&self, glyph: GlyphId, corner: Corner) -> Vec<MathKernEntry> {
        let Some(kern) = self.math_kern(glyph, corner) else {
            return Vec::new();
        };
        (0..=kern.height_count())
            .filter_map(|i| {
                let max_correction_height = if i == kern.height_count() {
                    i32::MAX
                } else {
                    self.math_value(kern.correction_height(i)?, false)
                };
                let kern_value = self.math_value(kern.kern_value(i)?, true);
                Some(MathKernEntry {
                    max_correction_height,
                    kern_value,
                })
            })
            .collect()
    }

    /// The selected scaled MATH kern for a correction height.
    pub fn math_kerning(&self, glyph: GlyphId, corner: Corner, correction_height: i32) -> i32 {
        let entries = self.math_kernings(glyph, corner);
        let reversed = self.scale.y_scale() < 0;
        entries
            .iter()
            .find(|entry| {
                if reversed {
                    correction_height > entry.max_correction_height
                } else {
                    correction_height < entry.max_correction_height
                }
            })
            .or_else(|| entries.last())
            .map_or(0, |entry| entry.kern_value)
    }

    fn math_construction(
        &self,
        glyph: GlyphId,
        direction: Direction,
    ) -> Option<MathGlyphConstruction<'data>> {
        let variants = self.math()?.variants().ok().flatten()?;
        match direction {
            Direction::Horizontal => {
                let index =
                    usize::try_from(variants.horiz_coverage().ok().flatten()?.get(glyph)?).ok()?;
                variants.horiz_construction(index).ok().flatten()
            }
            Direction::Vertical => {
                let index =
                    usize::try_from(variants.vert_coverage().ok().flatten()?.get(glyph)?).ok()?;
                variants.vert_construction(index).ok().flatten()
            }
        }
    }

    /// Scaled size variants for a MATH glyph.
    pub fn math_variants(&self, glyph: GlyphId, direction: Direction) -> Vec<MathGlyphVariant> {
        let Some(construction) = self.math_construction(glyph, direction) else {
            return Vec::new();
        };
        (0..construction.num_variants())
            .filter_map(|i| {
                let (glyph, advance) = construction.variant(i)?;
                Some(MathGlyphVariant {
                    glyph,
                    advance: self.scale_direction(i32::from(advance), direction),
                })
            })
            .collect()
    }

    /// Scaled assembly parts and italic correction for a MATH glyph.
    pub fn math_assembly(&self, glyph: GlyphId, direction: Direction) -> MathAssembly {
        let Some(assembly) = self
            .math_construction(glyph, direction)
            .and_then(|construction| construction.glyph_assembly().ok().flatten())
        else {
            return MathAssembly {
                italics_correction: 0,
                parts: Vec::new(),
            };
        };
        let parts = (0..assembly.num_parts())
            .filter_map(|i| {
                let part = assembly.part(i)?;
                Some(MathGlyphPart {
                    glyph: part.glyph,
                    start_connector_length: self
                        .scale_direction(i32::from(part.start_connector_length), direction),
                    end_connector_length: self
                        .scale_direction(i32::from(part.end_connector_length), direction),
                    full_advance: self.scale_direction(i32::from(part.full_advance), direction),
                    flags: part.flags & 1,
                })
            })
            .collect();
        MathAssembly {
            italics_correction: self.math_value(assembly.italics_correction(), true),
            parts,
        }
    }

    /// Minimum connector overlap in the stretch direction.
    pub fn math_min_connector_overlap(&self, direction: Direction) -> i32 {
        self.math()
            .and_then(|math| math.variants().ok().flatten())
            .map_or(0, |variants| {
                self.scale_direction(i32::from(variants.min_connector_overlap()), direction)
            })
    }

    /// Scaled GDEF ligature caret positions in table order.
    pub fn ligature_carets(&self, glyph: GlyphId, direction: Direction) -> Vec<i32> {
        let Some(carets) = self
            .font()
            .shaping_table(Tag::new(b"GDEF"))
            .and_then(|data| Gdef::new(data).ok())
            .and_then(|gdef| gdef.lig_caret_list().ok().flatten())
        else {
            return Vec::new();
        };
        let Some(index) = carets
            .coverage()
            .ok()
            .flatten()
            .and_then(|coverage| coverage.get(glyph))
            .and_then(|index| usize::try_from(index).ok())
        else {
            return Vec::new();
        };
        let Some(ligature) = carets.lig_glyph(index).ok().flatten() else {
            return Vec::new();
        };
        (0..ligature.len())
            .map(|i| match ligature.caret(i).ok().flatten() {
                Some(CaretValue::Coordinate(value)) => {
                    self.scale_direction(i32::from(value), direction)
                }
                Some(CaretValue::Device(value, device)) => self
                    .scale_direction(i32::from(value), direction)
                    .saturating_add(self.device_delta(
                        device,
                        direction == Direction::Horizontal,
                        StoreKind::Gdef,
                    )),
                _ => 0,
            })
            .collect()
    }
}
