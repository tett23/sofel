//! Drawing VARC components through the existing integer pen interface.
//! The slot transform ports FreeType 2.15.0's `tt_varc_build_transform`,
//! `FT_Vector_Unit`, and `FT_Tan`. Copyright (C) 2001-2026 by David Turner,
//! Robert Wilhelm, and Werner Lemberg. Used under the FreeType License (FTL);
//! see THIRD_PARTY_NOTICES.md.

use alloc::vec::Vec;

use sofel_font::HbScale;
use sofel_font::read::GlyphId;
use sofel_font::read::glyf::Glyph;
use sofel_font::read::loca::Loca;
use sofel_font::read::varc::{TupleValues, Varc};
use sofel_font::read::{glyf, loca};
use sofel_font::var::Location;
use sofel_font::var::gvar::{HbScratch, Point as GvarPoint};
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf, hb_roundf32};
use sofel_geom::{Pen, Point, PointF, Transform, div_fix, mul_fix};

use crate::{
    CBox, HbOutlineCommand, OutlineError, OutlineInfo, Outlines, Scale, Scratch, TAG_CUBIC, TAG_ON,
};

struct DrawState {
    path: Vec<u32>,
    edges_left: u32,
    bounds: Option<CBox>,
    overlap: bool,
}

fn component_transform(flags: u32, raw: [f32; 9]) -> Transform {
    let tx = if flags & (1 << 4) != 0 { raw[0] } else { 0.0 };
    let ty = if flags & (1 << 5) != 0 { raw[1] } else { 0.0 };
    let rotation = if flags & (1 << 6) != 0 {
        raw[2] * core::f32::consts::PI / 4096.0
    } else {
        0.0
    };
    let sx = if flags & (1 << 8) != 0 {
        raw[3] / 1024.0
    } else {
        1.0
    };
    let sy = if flags & (1 << 9) != 0 {
        raw[4] / 1024.0
    } else {
        sx
    };
    let skew_x = if flags & (1 << 13) != 0 {
        raw[5] * core::f32::consts::PI / 4096.0
    } else {
        0.0
    };
    let skew_y = if flags & (1 << 14) != 0 {
        raw[6] * core::f32::consts::PI / 4096.0
    } else {
        0.0
    };
    let cx = if flags & (1 << 10) != 0 { raw[7] } else { 0.0 };
    let cy = if flags & (1 << 11) != 0 { raw[8] } else { 0.0 };
    Transform::translate(tx + cx, ty + cy)
        .multiply(&Transform::rotate(rotation))
        .multiply(&Transform::scale(sx, sy))
        .multiply(&Transform::skew(-skew_x, skew_y))
        .multiply(&Transform::translate(-cx, -cy))
}

fn emit_point(point: Point, transform: &Transform, scale: Scale, state: &mut DrawState) -> Point {
    let mut transform = *transform;
    if let Scale::Scaled { x, y } = scale {
        transform.dx *= x.0 as f32 / 65536.0;
        transform.dy *= y.0 as f32 / 65536.0;
    }
    let point = transform.apply(PointF {
        x: point.x as f32,
        y: point.y as f32,
    });
    let point = Point {
        x: hb_clamp_to_i32(hb_roundf32(point.x)),
        y: hb_clamp_to_i32(hb_roundf32(point.y)),
    };
    state.bounds = Some(state.bounds.map_or(
        CBox {
            x_min: point.x,
            y_min: point.y,
            x_max: point.x,
            y_max: point.y,
        },
        |bounds| CBox {
            x_min: bounds.x_min.min(point.x),
            y_min: bounds.y_min.min(point.y),
            x_max: bounds.x_max.max(point.x),
            y_max: bounds.y_max.max(point.y),
        },
    ));
    point
}

#[derive(Clone, Copy)]
struct SlotTransform {
    xx: i32,
    xy: i32,
    yx: i32,
    yy: i32,
    dx: i32,
    dy: i32,
}

impl SlotTransform {
    const IDENTITY: Self = Self {
        xx: 0x10000,
        xy: 0,
        yx: 0,
        yy: 0x10000,
        dx: 0,
        dy: 0,
    };

    fn multiply(self, child: Self) -> Self {
        Self {
            xx: mul_fix(self.xx, child.xx).wrapping_add(mul_fix(self.xy, child.yx)),
            xy: mul_fix(self.xx, child.xy).wrapping_add(mul_fix(self.xy, child.yy)),
            yx: mul_fix(self.yx, child.xx).wrapping_add(mul_fix(self.yy, child.yx)),
            yy: mul_fix(self.yx, child.xy).wrapping_add(mul_fix(self.yy, child.yy)),
            dx: mul_fix(child.dx, self.xx)
                .wrapping_add(mul_fix(child.dy, self.xy))
                .wrapping_add(self.dx),
            dy: mul_fix(child.dx, self.yx)
                .wrapping_add(mul_fix(child.dy, self.yy))
                .wrapping_add(self.dy),
        }
    }

    fn apply(self, point: Point, scale: Scale, state: &mut DrawState) -> Point {
        let delta = |value: i32, axis: i32| match scale {
            Scale::Unscaled => value.wrapping_add(32) >> 6,
            Scale::Scaled { .. } => {
                let scaled = i64::from(value)
                    .wrapping_mul(i64::from(axis))
                    .wrapping_add(0x200000)
                    >> 22;
                i32::try_from(scaled).unwrap_or(if scaled < 0 { i32::MIN } else { i32::MAX })
            }
        };
        let (x_scale, y_scale) = match scale {
            Scale::Unscaled => (0, 0),
            Scale::Scaled { x, y } => (x.0, y.0),
        };
        let point = Point {
            x: mul_fix(point.x, self.xx)
                .wrapping_add(mul_fix(point.y, self.xy))
                .wrapping_add(delta(self.dx, x_scale)),
            y: mul_fix(point.x, self.yx)
                .wrapping_add(mul_fix(point.y, self.yy))
                .wrapping_add(delta(self.dy, y_scale)),
        };
        state.bounds = Some(state.bounds.map_or(
            CBox {
                x_min: point.x,
                y_min: point.y,
                x_max: point.x,
                y_max: point.y,
            },
            |bounds| CBox {
                x_min: bounds.x_min.min(point.x),
                y_min: bounds.y_min.min(point.y),
                x_max: bounds.x_max.max(point.x),
                y_max: bounds.y_max.max(point.y),
            },
        ));
        point
    }
}

fn slot_pseudo_rotate(mut x: i32, mut y: i32, mut theta: i32) -> (i32, i32) {
    // FreeType's FT_Vector_Unit and FT_Tan share this 23-step integer CORDIC.
    const ATAN: [i32; 22] = [
        1740967, 919879, 466945, 234379, 117304, 58666, 29335, 14668, 7334, 3667, 1833, 917, 458,
        229, 115, 57, 29, 14, 7, 4, 2, 1,
    ];
    while theta < -(45 << 16) {
        (x, y) = (y, x.wrapping_neg());
        theta = theta.wrapping_add(90 << 16);
    }
    while theta > 45 << 16 {
        (x, y) = (y.wrapping_neg(), x);
        theta = theta.wrapping_sub(90 << 16);
    }
    for (i, atan) in (1u32..).zip(ATAN) {
        let b = 1i32.wrapping_shl(i.wrapping_sub(1));
        if theta < 0 {
            (x, y) = (
                x.wrapping_add(y.wrapping_add(b) >> i),
                y.wrapping_sub(x.wrapping_add(b) >> i),
            );
            theta = theta.wrapping_add(atan);
        } else {
            (x, y) = (
                x.wrapping_sub(y.wrapping_add(b) >> i),
                y.wrapping_add(x.wrapping_add(b) >> i),
            );
            theta = theta.wrapping_sub(atan);
        }
    }
    (x, y)
}

fn slot_unit(angle: i32) -> (i32, i32) {
    let (x, y) = slot_pseudo_rotate(0x00dbd95b, 0, angle);
    (x.wrapping_add(0x80) >> 8, y.wrapping_add(0x80) >> 8)
}

fn slot_tan(angle: i32) -> i32 {
    let (x, y) = slot_pseudo_rotate(1 << 24, 0, angle);
    div_fix(y, x)
}

fn slot_component_transform(flags: u32, raw: [f32; 9]) -> SlotTransform {
    let value = |index: usize, bit: u32, factor: i32, fallback: i32| {
        if flags & (1 << bit) == 0 {
            fallback
        } else {
            hb_clamp_to_i32(hb_roundf32(
                raw.get(index).copied().unwrap_or(0.0) * factor as f32,
            ))
        }
    };
    let sx = value(3, 8, 64, 0x10000);
    let sy = value(4, 9, 64, sx);
    let rotation = value(2, 6, 16, 0);
    let (cos, sin) = if rotation == 0 {
        (0x10000, 0)
    } else {
        slot_unit(mul_fix(rotation, 180 << 16))
    };
    let mut matrix = SlotTransform {
        xx: mul_fix(cos, sx),
        xy: mul_fix(sin.wrapping_neg(), sy),
        yx: mul_fix(sin, sx),
        yy: mul_fix(cos, sy),
        dx: 0,
        dy: 0,
    };
    let skew_x = value(5, 13, 16, 0);
    let skew_y = value(6, 14, 16, 0);
    if skew_x != 0 || skew_y != 0 {
        let skew = SlotTransform {
            xy: slot_tan(mul_fix(skew_x, 180 << 16)).wrapping_neg(),
            yx: slot_tan(mul_fix(skew_y, 180 << 16)),
            ..SlotTransform::IDENTITY
        };
        matrix = matrix.multiply(skew);
    }
    let cx = value(7, 10, 64, 0);
    let cy = value(8, 11, 64, 0);
    matrix.dx = value(0, 4, 64, 0)
        .wrapping_add(cx)
        .wrapping_sub(mul_fix(matrix.xx, cx))
        .wrapping_sub(mul_fix(matrix.xy, cy));
    matrix.dy = value(1, 5, 64, 0)
        .wrapping_add(cy)
        .wrapping_sub(mul_fix(matrix.yx, cx))
        .wrapping_sub(mul_fix(matrix.yy, cy));
    matrix
}

struct TransformPen<'a, P> {
    inner: &'a mut P,
    transform: Transform,
    scale: Scale,
    state: &'a mut DrawState,
}

impl<P: Pen> Pen for TransformPen<'_, P> {
    fn move_to(&mut self, point: Point) {
        self.inner
            .move_to(emit_point(point, &self.transform, self.scale, self.state));
    }
    fn line_to(&mut self, point: Point) {
        self.inner
            .line_to(emit_point(point, &self.transform, self.scale, self.state));
    }
    fn quad_to(&mut self, control: Point, point: Point) {
        let control = emit_point(control, &self.transform, self.scale, self.state);
        let point = emit_point(point, &self.transform, self.scale, self.state);
        self.inner.quad_to(control, point);
    }
    fn cubic_to(&mut self, c1: Point, c2: Point, point: Point) {
        let c1 = emit_point(c1, &self.transform, self.scale, self.state);
        let c2 = emit_point(c2, &self.transform, self.scale, self.state);
        let point = emit_point(point, &self.transform, self.scale, self.state);
        self.inner.cubic_to(c1, c2, point);
    }
    fn close(&mut self) {
        self.inner.close();
    }
}

trait LeafSink {
    fn enter_component(&mut self, _flags: u32, _raw: [f32; 9]) {}
    fn leave_component(&mut self) {}

    fn condition(
        &self,
        conditions: Option<sofel_font::read::varc::ConditionList<'_>>,
        index: usize,
        coords: &[i32],
        delta: &impl Fn(u32) -> f64,
    ) -> Option<bool> {
        conditions?.evaluate(index, coords, delta)
    }

    fn checkpoint(&self, _state: &DrawState) -> Option<SlotCheckpoint> {
        None
    }

    fn failed_child(
        &mut self,
        error: OutlineError,
        _checkpoint: Option<SlotCheckpoint>,
        _state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        Err(error)
    }

    fn cycle(
        &mut self,
        _outlines: &Outlines<'_, '_, '_>,
        _glyph: GlyphId,
        _transform: Transform,
        _coords: &[i32],
        _scratch: &mut Scratch,
        _state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        Ok(())
    }

    fn depth_limit(
        &mut self,
        _outlines: &Outlines<'_, '_, '_>,
        _glyph: GlyphId,
        _transform: Transform,
        _coords: &[i32],
        _scratch: &mut Scratch,
        _state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        Ok(())
    }

    fn leaf(
        &mut self,
        outlines: &Outlines<'_, '_, '_>,
        glyph: GlyphId,
        transform: Transform,
        coords: &[i32],
        scratch: &mut Scratch,
        state: &mut DrawState,
    ) -> Result<(), OutlineError>;
}

#[derive(Clone, Copy)]
struct SlotCheckpoint {
    points: usize,
    ends: usize,
    bounds: Option<CBox>,
    overlap: bool,
    edges_left: u32,
}

struct IntegerSink<'a, P> {
    scale: Scale,
    pen: &'a mut P,
}

impl<P: Pen> LeafSink for IntegerSink<'_, P> {
    fn leaf(
        &mut self,
        outlines: &Outlines<'_, '_, '_>,
        glyph: GlyphId,
        transform: Transform,
        coords: &[i32],
        scratch: &mut Scratch,
        state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        let info = if coords == outlines.instance().coords_normalized() {
            outlines.load_base(glyph, self.scale, scratch)?
        } else {
            let location = Location::from_normalized_2_14(&outlines.font, coords);
            outlines
                .with_location(&location)
                .load_base(glyph, self.scale, scratch)?
        };
        state.overlap |= info.overlap;
        scratch.decompose(
            Point::default(),
            1,
            &mut TransformPen {
                inner: self.pen,
                transform,
                scale: self.scale,
                state,
            },
        )
    }
}

struct FloatSink {
    scale: HbScale,
    commands: Vec<HbOutlineCommand>,
}

fn simple_float_glyf_path(
    outlines: &Outlines<'_, '_, '_>,
    glyph: GlyphId,
    scratch: &mut Scratch,
    x_factor: f32,
    y_factor: f32,
) -> Option<Vec<HbOutlineCommand>> {
    let font = &outlines.font;
    let extended = font.table(glyf::TAG_WIDE).is_some();
    let glyf_tag = if extended { glyf::TAG_WIDE } else { glyf::TAG };
    let loca_tag = if extended { loca::TAG_WIDE } else { loca::TAG };
    let data = font.table(glyf_tag)?;
    let locations = Loca::new(font.table(loca_tag)?, font.head().index_to_loc_format()).ok()?;
    let glyph_index = usize::try_from(glyph.0).ok()?;
    let start = usize::try_from(locations.offset(glyph_index)?).ok()?;
    let end = usize::try_from(locations.offset(glyph_index.checked_add(1)?)?).ok()?;
    let raw = data.get(start..end)?;
    let glyph_data = Glyph::new(raw).ok()?;
    if glyph_data.number_of_contours() < 0 {
        return None;
    }
    let simple = glyph_data.simple().ok()?;
    let ends: Vec<usize> = simple.end_points().map(usize::from).collect();
    let mut last = ends.iter().copied().peekable();
    let mut tags = Vec::new();
    let mut points: Vec<GvarPoint<f32>> = simple
        .points(extended)
        .enumerate()
        .map(|(index, point)| {
            tags.push(
                (if point.on_curve { TAG_ON } else { 0 }) | if point.cubic { TAG_CUBIC } else { 0 },
            );
            GvarPoint {
                x: point.x as f32,
                y: point.y as f32,
                end: last.next_if(|&end| end == index).is_some(),
            }
        })
        .collect();
    if points.len() != usize::try_from(simple.num_points()).ok()? {
        return None;
    }
    let point_count = points.len();
    points.extend(
        [GvarPoint {
            x: 0.0,
            y: 0.0,
            end: false,
        }; 4],
    );
    let mut budget = 1 << 24;
    outlines
        .instance()
        .apply_gvar_hb(
            glyph.0,
            &mut points,
            &mut HbScratch::default(),
            false,
            &mut budget,
        )
        .ok()?;
    let glyph_points: Vec<PointF> = points
        .get(..point_count)?
        .iter()
        .map(|point| PointF::new(point.x, point.y))
        .collect();
    let mut path = Vec::new();
    crate::hb_draw::decompose_float_points(
        &glyph_points,
        &tags,
        &ends,
        x_factor,
        y_factor,
        |command| path.push(command),
    )
    .ok()?;
    scratch.clear();
    Some(path)
}

fn transform_command(command: HbOutlineCommand, transform: &Transform) -> HbOutlineCommand {
    match command {
        HbOutlineCommand::MoveTo(p) => HbOutlineCommand::MoveTo(transform.apply(p)),
        HbOutlineCommand::LineTo(p) => HbOutlineCommand::LineTo(transform.apply(p)),
        HbOutlineCommand::QuadTo(c, p) => {
            HbOutlineCommand::QuadTo(transform.apply(c), transform.apply(p))
        }
        HbOutlineCommand::CubicTo(c1, c2, p) => {
            HbOutlineCommand::CubicTo(transform.apply(c1), transform.apply(c2), transform.apply(p))
        }
        HbOutlineCommand::Close => HbOutlineCommand::Close,
    }
}

impl LeafSink for FloatSink {
    fn leaf(
        &mut self,
        outlines: &Outlines<'_, '_, '_>,
        glyph: GlyphId,
        transform: Transform,
        coords: &[i32],
        scratch: &mut Scratch,
        state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        let location = Location::from_normalized_2_14(&outlines.font, coords);
        let local = outlines.with_location(&location);
        let mut transform = transform;
        let upem = f32::from(outlines.font.hb_units_per_em());
        let x_factor = self.scale.x_scale() as f32 / upem;
        let y_factor = self.scale.y_scale() as f32 / upem;
        transform.dx *= x_factor;
        transform.dy *= y_factor;
        if let Some(path) = simple_float_glyf_path(&local, glyph, scratch, x_factor, y_factor)
            .or_else(|| local.hb_extents().cff_glyph_path(glyph, &self.scale))
        {
            self.commands.extend(
                path.into_iter()
                    .map(|command| transform_command(command, &transform)),
            );
            return Ok(());
        }
        let info = local.load(glyph, Scale::Unscaled, scratch)?;
        state.overlap |= info.overlap;
        scratch.hb_decompose(x_factor, y_factor, |command| {
            self.commands.push(transform_command(command, &transform));
        })
    }
}

#[allow(clippy::too_many_arguments)] // The recursive frame keeps location and transform explicit.
fn visit<S: LeafSink>(
    outlines: &Outlines<'_, '_, '_>,
    varc: &Varc<'_>,
    glyph: GlyphId,
    transform: Transform,
    coords: &[i32],
    depth_left: u32,
    scratch: &mut Scratch,
    sink: &mut S,
    state: &mut DrawState,
) -> Result<(), OutlineError> {
    if glyph.0 >= outlines.font.num_glyphs() {
        return Ok(());
    }
    if let Some(components) = varc.components(glyph) {
        if state.path.contains(&glyph.0) {
            return sink.cycle(outlines, glyph, transform, coords, scratch, state);
        }
        if depth_left == 0 {
            return sink.depth_limit(outlines, glyph, transform, coords, scratch, state);
        }
        if state.edges_left == 0 {
            return Ok(());
        }
        state.edges_left = state.edges_left.saturating_sub(1);
        state.path.push(glyph.0);
        let record = varc.record(glyph).unwrap_or(&[]);
        let mut at = 0usize;
        for component in components {
            let component = component.ok_or(OutlineError::InvalidComposite)?;
            let end = at
                .checked_add(component.size)
                .ok_or(OutlineError::InvalidComposite)?;
            let encoded = record.get(at..end).ok_or(OutlineError::InvalidComposite)?;
            at = end;
            if let Some(index) = component.condition_index {
                let store = varc.variation_store();
                let delta = |var_index| {
                    let mut value = [0.0];
                    store
                        .and_then(|store| store.add_delta(var_index, coords, &mut value))
                        .map_or(0.0, |_| value[0])
                };
                match sink.condition(varc.condition_list(), index as usize, coords, &delta) {
                    Some(true) => {}
                    Some(false) => continue,
                    None => {
                        state.path.pop();
                        return Err(OutlineError::InvalidComposite);
                    }
                }
            }
            let mut child_coords = if component.flags & 1 != 0 {
                outlines.instance().coords_normalized().to_vec()
            } else {
                coords.to_vec()
            };
            if let Some(index) = component.axis_indices_index {
                let indices = varc
                    .axis_indices(index)
                    .ok_or(OutlineError::InvalidComposite)?;
                let values = encoded
                    .get(
                        component.axis_values_offset.unwrap_or(0)
                            ..component.axis_values_end.unwrap_or(0),
                    )
                    .ok_or(OutlineError::InvalidComposite)?;
                let mut values: Vec<f64> = TupleValues::new(values)
                    .ok_or(OutlineError::InvalidComposite)?
                    .map(f64::from)
                    .collect();
                if let Some(var_index) = component.axis_values_var_index {
                    varc.variation_store()
                        .and_then(|store| store.add_delta(var_index, coords, &mut values))
                        .ok_or(OutlineError::InvalidComposite)?;
                }
                for (axis, value) in TupleValues::new(indices)
                    .ok_or(OutlineError::InvalidComposite)?
                    .zip(values)
                {
                    let axis = usize::try_from(axis).map_err(|_| OutlineError::InvalidComposite)?;
                    if axis < 64 {
                        child_coords.resize(child_coords.len().max(axis.saturating_add(1)), 0);
                        *child_coords
                            .get_mut(axis)
                            .ok_or(OutlineError::InvalidComposite)? =
                            hb_clamp_to_i32(hb_roundf(value));
                    }
                }
            }
            let mut raw = component.transform.map(f32::from);
            if let Some(var_index) = component.transform_var_index {
                let bits = [4, 5, 6, 8, 9, 13, 14, 10, 11];
                let mut values: Vec<f64> = bits
                    .iter()
                    .enumerate()
                    .filter(|(_, bit)| component.flags & (1 << *bit) != 0)
                    .filter_map(|(index, _)| raw.get(index).map(|value| f64::from(*value)))
                    .collect();
                varc.variation_store()
                    .and_then(|store| store.add_delta(var_index, coords, &mut values))
                    .ok_or(OutlineError::InvalidComposite)?;
                let mut values = values.into_iter();
                for (index, bit) in bits.iter().enumerate() {
                    if component.flags & (1 << bit) != 0 {
                        #[allow(clippy::cast_possible_truncation)]
                        // HarfBuzz stores varied transforms as float.
                        let value = values.next().ok_or(OutlineError::InvalidComposite)? as f32;
                        *raw.get_mut(index).ok_or(OutlineError::InvalidComposite)? = value;
                    }
                }
            }
            let child = transform.multiply(&component_transform(component.flags, raw));
            let checkpoint = sink.checkpoint(state);
            let path_len = state.path.len();
            sink.enter_component(component.flags, raw);
            let result = visit(
                outlines,
                varc,
                component.glyph,
                child,
                &child_coords,
                depth_left.saturating_sub(1),
                scratch,
                sink,
                state,
            );
            sink.leave_component();
            if let Err(error) = result {
                state.path.truncate(path_len);
                sink.failed_child(error, checkpoint, state)?;
            }
        }
        state.path.pop();
        return Ok(());
    }
    sink.leaf(outlines, glyph, transform, coords, scratch, state)
}

struct SlotBlend<'a> {
    store: sofel_font::read::item_variation::ItemVariationStore<'a>,
    scalars: sofel_font::var::StoreScalars<i32>,
}

impl sofel_font::read::cff::dict::BlendWeights for SlotBlend<'_> {
    fn regions(&self, index: u32) -> Option<u32> {
        let data = self.store.item_data(u16::try_from(index).ok()?)?;
        for column in 0..usize::from(data.region_index_count()) {
            if data.region_index(column)? >= self.store.region_count() {
                return None;
            }
        }
        Some(u32::from(data.region_index_count()))
    }
    fn weight(&self, index: u32, column: u32) -> i32 {
        let value = u16::try_from(index)
            .ok()
            .and_then(|index| self.store.item_data(index))
            .and_then(|data| data.region_index(usize::try_from(column).ok()?))
            .and_then(|region| self.scalars.regions().get(usize::from(region)).copied());
        value.unwrap_or(0)
    }
}

struct SlotSink {
    scale: Scale,
    points: Vec<Point>,
    tags: Vec<u8>,
    ends: Vec<usize>,
    transforms: Vec<SlotTransform>,
}

impl LeafSink for SlotSink {
    fn enter_component(&mut self, flags: u32, raw: [f32; 9]) {
        let parent = self
            .transforms
            .last()
            .copied()
            .unwrap_or(SlotTransform::IDENTITY);
        self.transforms
            .push(parent.multiply(slot_component_transform(flags, raw)));
    }

    fn leave_component(&mut self) {
        self.transforms.pop();
    }

    fn condition(
        &self,
        conditions: Option<sofel_font::read::varc::ConditionList<'_>>,
        index: usize,
        coords: &[i32],
        delta: &impl Fn(u32) -> f64,
    ) -> Option<bool> {
        match conditions {
            None => Some(true),
            Some(conditions) if index >= conditions.len() => Some(true),
            Some(conditions) => Some(
                conditions
                    .evaluate_ft(index, coords, delta)
                    .unwrap_or(false),
            ),
        }
    }

    fn checkpoint(&self, state: &DrawState) -> Option<SlotCheckpoint> {
        Some(SlotCheckpoint {
            points: self.points.len(),
            ends: self.ends.len(),
            bounds: state.bounds,
            overlap: state.overlap,
            edges_left: state.edges_left,
        })
    }

    fn failed_child(
        &mut self,
        _error: OutlineError,
        checkpoint: Option<SlotCheckpoint>,
        state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        let checkpoint = checkpoint.ok_or(OutlineError::InvalidComposite)?;
        self.points.truncate(checkpoint.points);
        self.tags.truncate(checkpoint.points);
        self.ends.truncate(checkpoint.ends);
        state.bounds = checkpoint.bounds;
        state.overlap = checkpoint.overlap;
        state.edges_left = checkpoint.edges_left;
        Ok(())
    }

    fn cycle(
        &mut self,
        outlines: &Outlines<'_, '_, '_>,
        glyph: GlyphId,
        transform: Transform,
        coords: &[i32],
        scratch: &mut Scratch,
        state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        self.leaf(outlines, glyph, transform, coords, scratch, state)
    }

    fn depth_limit(
        &mut self,
        outlines: &Outlines<'_, '_, '_>,
        glyph: GlyphId,
        transform: Transform,
        coords: &[i32],
        scratch: &mut Scratch,
        state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        self.leaf(outlines, glyph, transform, coords, scratch, state)
    }

    fn leaf(
        &mut self,
        outlines: &Outlines<'_, '_, '_>,
        glyph: GlyphId,
        _transform: Transform,
        coords: &[i32],
        scratch: &mut Scratch,
        state: &mut DrawState,
    ) -> Result<(), OutlineError> {
        let location = Location::from_normalized_2_14(&outlines.font, coords);
        let child = outlines.with_location(&location);
        let info = match &child.format {
            crate::Format::Cff(cff) => {
                if matches!(self.scale, Scale::Scaled { .. }) && child.font.units_per_em() > 0x7fff
                {
                    return Err(OutlineError::InvalidOutline);
                }
                if let Some(store) = cff.variation_store() {
                    let raw: Vec<_> = coords
                        .iter()
                        .map(|coord| sofel_font::read::Fixed(coord.saturating_mul(4)))
                        .collect();
                    let scalars = sofel_font::var::StoreScalars::from_store::<sofel_font::var::Ft>(
                        &store,
                        &raw,
                        !raw.is_empty(),
                    );
                    crate::cff::load_cff_with_blend(
                        cff,
                        glyph,
                        self.scale,
                        &SlotBlend { store, scalars },
                        scratch,
                        None,
                    )?
                    .0
                } else {
                    crate::cff::load_cff_at(
                        &child.font,
                        child.location,
                        cff,
                        glyph,
                        self.scale,
                        scratch,
                    )?
                }
            }
            crate::Format::Glyf => crate::load_glyf_with_coords(
                child.instance(),
                glyph,
                self.scale,
                scratch,
                None,
                Some(coords),
            )?,
            crate::Format::Unsupported(error) => return Err(*error),
        };
        state.overlap |= info.overlap;
        let offset = self.points.len();
        let transform = self
            .transforms
            .last()
            .copied()
            .unwrap_or(SlotTransform::IDENTITY);
        self.points.extend(
            scratch
                .points
                .iter()
                .map(|&point| transform.apply(point, self.scale, state)),
        );
        self.tags.extend_from_slice(&scratch.tags);
        for &end in &scratch.ends {
            self.ends.push(
                offset
                    .checked_add(end)
                    .ok_or(OutlineError::InvalidOutline)?,
            );
        }
        Ok(())
    }
}

pub(super) fn load(
    outlines: &Outlines<'_, '_, '_>,
    varc: &Varc<'_>,
    glyph: GlyphId,
    scale: Scale,
    scratch: &mut Scratch,
) -> Result<OutlineInfo, OutlineError> {
    scratch.clear();
    let mut state = DrawState {
        path: Vec::new(),
        edges_left: 16_384,
        bounds: None,
        overlap: false,
    };
    let mut sink = SlotSink {
        scale,
        points: Vec::new(),
        tags: Vec::new(),
        ends: Vec::new(),
        transforms: Vec::new(),
    };
    if let Err(error) = visit(
        outlines,
        varc,
        glyph,
        Transform::IDENTITY,
        outlines.instance().coords_normalized(),
        16,
        scratch,
        &mut sink,
        &mut state,
    ) {
        scratch.clear();
        return Err(error);
    }
    scratch.points = sink.points;
    scratch.tags = sink.tags;
    scratch.ends = sink.ends;
    Ok(OutlineInfo {
        overlap: state.overlap,
        cbox: state.bounds.unwrap_or_default(),
    })
}

pub(super) fn draw(
    outlines: &Outlines<'_, '_, '_>,
    varc: &Varc<'_>,
    glyph: GlyphId,
    scale: Scale,
    scratch: &mut Scratch,
    pen: &mut impl Pen,
) -> Result<OutlineInfo, OutlineError> {
    scratch.clear();
    let mut state = DrawState {
        path: Vec::new(),
        edges_left: 16_384,
        bounds: None,
        overlap: false,
    };
    let mut sink = IntegerSink { scale, pen };
    visit(
        outlines,
        varc,
        glyph,
        Transform::IDENTITY,
        outlines.instance().coords_normalized(),
        64,
        scratch,
        &mut sink,
        &mut state,
    )?;
    Ok(OutlineInfo {
        overlap: state.overlap,
        cbox: state.bounds.unwrap_or_default(),
    })
}

pub(super) fn draw_float(
    outlines: &Outlines<'_, '_, '_>,
    varc: &Varc<'_>,
    glyph: GlyphId,
    scale: HbScale,
    scratch: &mut Scratch,
) -> Result<Vec<HbOutlineCommand>, OutlineError> {
    scratch.clear();
    let mut state = DrawState {
        path: Vec::new(),
        edges_left: 16_384,
        bounds: None,
        overlap: false,
    };
    let mut sink = FloatSink {
        scale,
        commands: Vec::new(),
    };
    visit(
        outlines,
        varc,
        glyph,
        Transform::IDENTITY,
        outlines.instance().coords_normalized(),
        64,
        scratch,
        &mut sink,
        &mut state,
    )?;
    Ok(sink.commands)
}
