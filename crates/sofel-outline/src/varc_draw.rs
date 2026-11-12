//! Drawing VARC components through the existing integer pen interface.

use alloc::vec::Vec;

use sofel_font::read::GlyphId;
use sofel_font::read::varc::{TupleValues, Varc};
use sofel_font::var::Location;
use sofel_geom::float::{hb_clamp_to_i32, hb_roundf, hb_roundf32};
use sofel_geom::{Pen, Point, PointF, Transform};

use crate::{CBox, OutlineError, OutlineInfo, Outlines, Scale, Scratch};

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

#[allow(clippy::too_many_arguments)] // The recursive frame keeps location and transform explicit.
fn visit(
    outlines: &Outlines<'_, '_, '_>,
    varc: &Varc<'_>,
    glyph: GlyphId,
    scale: Scale,
    transform: Transform,
    coords: &[i32],
    depth_left: u32,
    scratch: &mut Scratch,
    pen: &mut impl Pen,
    state: &mut DrawState,
) -> Result<(), OutlineError> {
    if glyph.0 >= outlines.font.num_glyphs() {
        return Ok(());
    }
    if let Some(components) = varc.components(glyph) {
        if depth_left == 0 || state.edges_left == 0 || state.path.contains(&glyph.0) {
            return Ok(());
        }
        state.edges_left = state.edges_left.saturating_sub(1);
        state.path.push(glyph.0);
        let record = varc.record(glyph).ok_or(OutlineError::InvalidComposite)?;
        let mut at = 0usize;
        for component in components {
            let component = component.ok_or(OutlineError::InvalidComposite)?;
            let end = at
                .checked_add(component.size)
                .ok_or(OutlineError::InvalidComposite)?;
            let encoded = record.get(at..end).ok_or(OutlineError::InvalidComposite)?;
            at = end;
            if let Some(index) = component.condition_index {
                let Some(condition) = varc.condition_list() else {
                    state.path.pop();
                    return Err(OutlineError::InvalidComposite);
                };
                let store = varc.variation_store();
                let delta = |var_index| {
                    let mut value = [0.0];
                    store
                        .and_then(|store| store.add_delta(var_index, coords, &mut value))
                        .map_or(0.0, |_| value[0])
                };
                match condition.evaluate(index as usize, coords, &delta) {
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
            let result = visit(
                outlines,
                varc,
                component.glyph,
                scale,
                child,
                &child_coords,
                depth_left.saturating_sub(1),
                scratch,
                pen,
                state,
            );
            if let Err(error) = result {
                state.path.pop();
                return Err(error);
            }
        }
        state.path.pop();
        return Ok(());
    }
    let info = if coords == outlines.instance().coords_normalized() {
        outlines.load(glyph, scale, scratch)?
    } else {
        let location = Location::from_normalized_2_14(&outlines.font, coords);
        outlines
            .with_location(&location)
            .load(glyph, scale, scratch)?
    };
    state.overlap |= info.overlap;
    scratch.decompose(
        Point::default(),
        1,
        &mut TransformPen {
            inner: pen,
            transform,
            scale,
            state,
        },
    )
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
    visit(
        outlines,
        varc,
        glyph,
        scale,
        Transform::IDENTITY,
        outlines.instance().coords_normalized(),
        64,
        scratch,
        pen,
        &mut state,
    )?;
    Ok(OutlineInfo {
        overlap: state.overlap,
        cbox: state.bounds.unwrap_or_default(),
    })
}
