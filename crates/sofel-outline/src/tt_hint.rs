//! TrueType bytecode execution (sofel ADR 0064).
//! Based on the opcode handlers in FreeType's src/truetype/ttinterp.c.
//! See THIRD_PARTY_NOTICES.md for the FreeType Project License.

// Register banks and axes have fixed sizes. Variable-size zones are checked
// before access; instruction-stack ranges are derived from available operands.
// TrueType register and point-index conversions follow the driver integer widths.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0064_i1_stack_vectors_normalize_and_preserve_zero() {
        let mut stack = alloc::vec![3, 4];
        run(
            &[
                0x0a, 0xb1, 0, 0, 0x0a, 0xb9, 0xff, 0xff, 0, 0, 0x0b, 0x0c, 0x0d,
            ],
            &mut stack,
            4,
        )
        .unwrap();
        assert_eq!(stack, [9830, 13107, -16384, 0]);
    }

    #[test]
    fn adr0064_i1_branches_skip_nested_blocks_and_push_payloads() {
        let mut stack = alloc::vec::Vec::new();
        // Both halves contain opcode-looking payload bytes. The nested
        // ELSE belongs to its own IF and cannot end the outer false arm.
        let program = [
            0xb0, 0, 0x58, 0x40, 3, 0x1b, 0x58, 0x59, 0x58, 0x1b, 0x59, 0x1b, 0xb0, 1, 0x58, 0xb8,
            0xff, 0xfe, 0x1b, 0xb0, 0x59, 0x59, 0x59,
        ];
        run(&program, &mut stack, 4).unwrap();
        assert_eq!(stack, [-2]);
    }

    #[test]
    fn adr0064_i1_truncated_push_and_unmatched_conditional_fail() {
        for code in [
            &[0x40][..],
            &[0x41, 1, 0],
            &[0xb7, 0],
            &[0xb0, 0, 0x58, 0xb0, 0x59],
        ] {
            assert_eq!(
                run(code, &mut alloc::vec::Vec::new(), 8),
                Err(Error::CodeOverflow)
            );
        }
        assert_eq!(
            run(&[0x58], &mut alloc::vec::Vec::new(), 8),
            Err(Error::StackUnderflow)
        );
        assert_eq!(
            run(&[0xb1, 1, 2], &mut alloc::vec::Vec::new(), 1),
            Err(Error::StackOverflow)
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Error {
    CodeOverflow,
    StackUnderflow,
    StackOverflow,
    InvalidOpcode,
    DivideByZero,
    InvalidReference,
    InvalidArgument,
    ExecutionLimit,
}

struct Instruction<'a> {
    opcode: u8,
    immediate: &'a [u8],
    words: bool,
}

fn next<'a>(code: &mut &'a [u8]) -> Result<Instruction<'a>, Error> {
    let (&opcode, rest) = code.split_first().ok_or(Error::CodeOverflow)?;
    let words = opcode == 0x41 || (0xb8..=0xbf).contains(&opcode);
    let (count, rest) = match opcode {
        0x40 | 0x41 => {
            let (&n, rest) = rest.split_first().ok_or(Error::CodeOverflow)?;
            (usize::from(n), rest)
        }
        0xb0..=0xbf => (usize::from(opcode & 7) + 1, rest),
        _ => (0, rest),
    };
    let bytes = if words {
        count.saturating_mul(2)
    } else {
        count
    };
    let (immediate, remaining) = rest.split_at_checked(bytes).ok_or(Error::CodeOverflow)?;
    *code = remaining;
    Ok(Instruction {
        opcode,
        immediate,
        words,
    })
}

#[allow(clippy::arithmetic_side_effects)]
fn skip_block(code: &mut &[u8], stop_at_else: bool) -> Result<(), Error> {
    let mut depth = 0usize;
    loop {
        match next(code)?.opcode {
            0x58 => depth += 1,
            0x1b if depth == 0 && stop_at_else => return Ok(()),
            0x59 if depth == 0 => return Ok(()),
            0x59 => depth -= 1,
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
enum Rounding {
    Grid,
    HalfGrid,
    DoubleGrid,
    Super {
        period: i64,
        phase: i64,
        threshold: i64,
        diagonal: bool,
    },
    Off,
    Up,
    Down,
}

impl Rounding {
    #[allow(clippy::arithmetic_side_effects)]
    fn super_round(selector: i64, diagonal: bool) -> Self {
        let grid = if diagonal { 0x2d41 } else { 0x4000 };
        let period = match selector & 0xc0 {
            0 => grid / 2,
            0x80 => grid * 2,
            _ => grid,
        };
        let phase = period * ((selector >> 4) & 3) / 4;
        let threshold = match selector & 15 {
            0 => period - 1,
            n => (n - 4) * period / 8,
        };
        Self::Super {
            period: period >> 8,
            phase: phase >> 8,
            threshold: threshold >> 8,
            diagonal,
        }
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn apply(&self, distance: i64) -> i64 {
        let magnitude = distance.wrapping_abs();
        let rounded = match self {
            Self::Grid => magnitude.wrapping_add(32) & !63,
            Self::HalfGrid => (magnitude & !63).wrapping_add(32),
            Self::DoubleGrid => magnitude.wrapping_add(16) & !31,
            Self::Super {
                period,
                phase,
                threshold,
                diagonal,
            } => {
                let shifted = magnitude.wrapping_add(threshold - phase);
                let base = if *diagonal {
                    shifted / period * period
                } else {
                    shifted & -period
                };
                let snapped = base.wrapping_add(*phase);
                if snapped < 0 { *phase } else { snapped }
            }
            Self::Off => return distance,
            Self::Up => magnitude.wrapping_add(63) & !63,
            Self::Down => magnitude & !63,
        };
        if distance < 0 {
            -rounded.max(0)
        } else {
            rounded.max(0)
        }
    }
}

/// Target reported to the native TrueType interpreter while loading an outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintTarget {
    /// Normal grayscale target.
    Normal,
    /// Light grayscale target.
    Light,
    /// Monochrome grid fitting.
    Mono,
    /// Horizontal LCD target.
    Lcd,
    /// Vertical LCD target.
    VerticalLcd,
}

#[derive(Clone, Copy)]
struct EngineInfo {
    version: u8,
    target: HintTarget,
    rotated: bool,
    stretched: bool,
    variation: bool,
    symmetric_smoothing: bool,
    tricky: bool,
}

impl EngineInfo {
    #[allow(clippy::arithmetic_side_effects)]
    fn query(self, selector: i64) -> i64 {
        let subpixel = self.version == 40 && self.target != HintTarget::Mono;
        let mut result = if selector & 1 != 0 {
            i64::from(self.version)
        } else {
            0
        };
        for (bit, enabled) in [
            (1, self.rotated),
            (2, self.stretched),
            (3, self.variation),
            (5, self.version == 35 && self.target != HintTarget::Mono),
            (6, subpixel),
            (8, subpixel && self.target == HintTarget::VerticalLcd),
            (10, subpixel),
            (11, subpixel && self.symmetric_smoothing),
            (
                12,
                subpixel && matches!(self.target, HintTarget::Normal | HintTarget::Light),
            ),
        ] {
            if enabled && selector & (1 << bit) != 0 {
                result |= 1 << (bit + 7);
            }
        }
        result
    }
}

#[derive(Clone, Copy, Default)]
struct HintPoint {
    unscaled: [i64; 2],
    original: [i64; 2],
    current: [i64; 2],
    touched: [bool; 2],
    on_curve: bool,
}

fn move_axis(point: &mut HintPoint, axis: usize, value: i64, compatibility: u8) {
    if compatibility == 0 || (axis == 1 && compatibility != 7) {
        point.current[axis] = value;
    }
    point.touched[axis] = true;
}

fn line_vector(a: [i64; 2], b: [i64; 2], perpendicular: bool) -> [i64; 2] {
    if a == b {
        return [16384, 0];
    }
    let dx = a[0].wrapping_sub(b[0]);
    let dy = a[1].wrapping_sub(b[1]);
    let [x, y] = if perpendicular {
        [dy.wrapping_neg(), dx]
    } else {
        [dx, dy]
    };
    let (unit, _) = crate::slot_synth::normalize(sofel_geom::Point {
        x: x as i32,
        y: y as i32,
    });
    [i64::from(unit.x / 4), i64::from(unit.y / 4)]
}

fn project([x, y]: [i64; 2], vector: [i64; 2]) -> i64 {
    let dot = x
        .wrapping_mul(vector[0])
        .wrapping_add(y.wrapping_mul(vector[1]));
    dot.wrapping_add(if dot < 0 { 0x1fff } else { 0x2000 }) >> 14
}

fn projected_distance(a: [i64; 2], b: [i64; 2], vector: [i64; 2]) -> i64 {
    project([a[0].wrapping_sub(b[0]), a[1].wrapping_sub(b[1])], vector)
}

fn intersection(a0: [i64; 2], a1: [i64; 2], b0: [i64; 2], b1: [i64; 2]) -> [i64; 2] {
    use sofel_font::read::cff::ft::mul_div;
    let dx = a1[0].wrapping_sub(a0[0]);
    let dy = a1[1].wrapping_sub(a0[1]);
    let ex = b1[0].wrapping_sub(b0[0]);
    let ey = b1[1].wrapping_sub(b0[1]);
    let cross = |x, y| mul_div(x, ey.wrapping_neg(), 64).wrapping_add(mul_div(y, ex, 64));
    let det = cross(dx, dy);
    let dot = mul_div(dx, ex, 64).wrapping_add(mul_div(dy, ey, 64));
    if det.wrapping_abs().wrapping_mul(19) > dot.wrapping_abs() {
        let numerator = cross(b0[0].wrapping_sub(a0[0]), b0[1].wrapping_sub(a0[1]));
        [
            a0[0].wrapping_add(mul_div(numerator, dx, det)),
            a0[1].wrapping_add(mul_div(numerator, dy, det)),
        ]
    } else {
        [0, 1].map(|axis| {
            a0[axis]
                .wrapping_add(a1[axis])
                .wrapping_add(b0[axis])
                .wrapping_add(b1[axis])
                / 4
        })
    }
}

fn shift_point(point: &mut HintPoint, delta: [i64; 2], freedom: [i64; 2], compatibility: u8) {
    for axis in 0..2 {
        if freedom[axis] != 0 {
            if compatibility == 0 || (axis == 1 && compatibility != 7) {
                point.current[axis] = point.current[axis].wrapping_add(delta[axis]);
            }
            point.touched[axis] = true;
        }
    }
}

// Both vectors contain normalized F2Dot14 components.
#[allow(clippy::arithmetic_side_effects)]
fn movement_factors(projection: [i64; 2], freedom: [i64; 2]) -> [i64; 2] {
    let dot = (projection[0] * freedom[0] + projection[1] * freedom[1] + 0x2000) >> 14;
    if (-0x1554..0x1555).contains(&dot) {
        return [0, 0];
    }
    freedom.map(|component| {
        if dot >= 0x3ffe {
            component * 4
        } else {
            component * 65536 / dot
        }
    })
}

fn move_point(
    point: &mut HintPoint,
    distance: i64,
    projection: [i64; 2],
    freedom: [i64; 2],
    compatibility: u8,
) {
    for (axis, factor) in movement_factors(projection, freedom)
        .into_iter()
        .enumerate()
    {
        if factor != 0 {
            let delta = sofel_font::read::cff::ft::mul_fix(distance, factor);
            move_axis(
                point,
                axis,
                point.current[axis].wrapping_add(delta),
                compatibility,
            );
        }
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn interpolate_contour(points: &mut [HintPoint], axis: usize) {
    let Some(first) = points.iter().position(|p| p.touched[axis]) else {
        return;
    };
    let mut left = first;
    loop {
        let mut right = (left + 1) % points.len();
        while !points[right].touched[axis] {
            right = (right + 1) % points.len();
        }
        let (mut lo, mut hi) = (points[left], points[right]);
        if lo.unscaled[axis] > hi.unscaled[axis] {
            core::mem::swap(&mut lo, &mut hi);
        }
        let mut i = (left + 1) % points.len();
        while i != right {
            let p = &mut points[i];
            let x = p.original[axis];
            p.current[axis] = if left == right {
                p.current[axis].wrapping_add(lo.current[axis].wrapping_sub(lo.original[axis]))
            } else if x <= lo.original[axis] {
                x.wrapping_add(lo.current[axis].wrapping_sub(lo.original[axis]))
            } else if x >= hi.original[axis] {
                x.wrapping_add(hi.current[axis].wrapping_sub(hi.original[axis]))
            } else if lo.current[axis] == hi.current[axis] || lo.unscaled[axis] == hi.unscaled[axis]
            {
                lo.current[axis]
            } else {
                use sofel_font::read::cff::ft::{div_fix, mul_fix};
                let scale = div_fix(
                    hi.current[axis].wrapping_sub(lo.current[axis]),
                    hi.unscaled[axis].wrapping_sub(lo.unscaled[axis]),
                );
                lo.current[axis].wrapping_add(mul_fix(
                    p.unscaled[axis].wrapping_sub(lo.unscaled[axis]),
                    scale,
                ))
            };
            i = (i + 1) % points.len();
        }
        if right == first {
            break;
        }
        left = right;
    }
}

#[derive(Clone)]
pub(crate) struct Interpreter {
    in_prep: bool,
    in_glyph: bool,
    instruction_control: u8,
    contour_ends: alloc::vec::Vec<usize>,
    auto_flip: bool,
    minimum_distance: i64,
    backward_compatibility: u8,
    blend_coords: alloc::vec::Vec<i32>,
    composite: bool,
    loop_count: u16,
    points: [alloc::vec::Vec<HintPoint>; 2],
    twilight_limit: usize,
    reference: [u16; 3],
    delta_base: u16,
    delta_shift: u8,
    zones: [u8; 3],
    control_value_cutin: i64,
    single_width_cutin: i64,
    single_width: i64,
    scan_control: bool,
    scan_type: u16,
    ppem: [u16; 2],
    point_size: i64,
    engine: EngineInfo,
    projection: [i64; 2],
    dual_projection: [i64; 2],
    freedom: [i64; 2],
    instructions: alloc::collections::BTreeMap<u8, alloc::rc::Rc<[u8]>>,
    instruction_def_limit: Option<u16>,
    functions: alloc::collections::BTreeMap<u16, alloc::rc::Rc<[u8]>>,
    rounding: Rounding,
    storage: alloc::vec::Vec<i64>,
    cvt: alloc::vec::Vec<i64>,
    scale: i64,
}

impl Interpreter {
    fn new(max_storage: u16, varied_cvt: &[i32], scale: i64) -> Self {
        Self {
            engine: EngineInfo {
                version: 40,
                target: HintTarget::Normal,
                rotated: false,
                stretched: false,
                variation: false,
                symmetric_smoothing: false,
                tricky: false,
            },
            points: Default::default(),
            twilight_limit: 0,
            reference: [0; 3],
            loop_count: 1,
            backward_compatibility: 0,
            blend_coords: alloc::vec::Vec::new(),
            composite: false,
            auto_flip: true,
            minimum_distance: 64,
            contour_ends: alloc::vec::Vec::new(),
            in_prep: false,
            in_glyph: false,
            instruction_control: 0,
            delta_base: 9,
            delta_shift: 3,
            zones: [1; 3],
            control_value_cutin: 68,
            single_width_cutin: 0,
            single_width: 0,
            scan_control: false,
            scan_type: 0,
            ppem: [0; 2],
            point_size: 0,
            projection: [16384, 0],
            dual_projection: [16384, 0],
            freedom: [16384, 0],
            instructions: alloc::collections::BTreeMap::new(),
            instruction_def_limit: None,
            functions: alloc::collections::BTreeMap::new(),
            rounding: Rounding::Grid,
            scale,
            cvt: varied_cvt
                .iter()
                .map(|v| sofel_font::read::cff::ft::mul_fix(i64::from(v / 64), scale))
                .collect(),
            storage: alloc::vec![0; usize::from(max_storage)],
        }
    }

    pub(super) fn native(
        instance: sofel_font::var::Instance<'_, '_, '_, '_>,
        ppem: u16,
        target: HintTarget,
        version: u8,
    ) -> Result<(Self, usize), super::OutlineError> {
        Self::prepare(
            instance,
            ppem,
            EngineInfo {
                version,
                target,
                rotated: false,
                stretched: false,
                variation: !instance.location().ft_default(),
                symmetric_smoothing: false,
                tricky: false,
            },
        )
        .map_err(|_| super::OutlineError::InvalidOutline)
    }

    pub(super) fn hint_composite(
        &self,
        outline: &mut [super::Point],
        tags: &mut [u8],
        phantoms: &mut [super::Point; 4],
        ends: &[usize],
        code: &[u8],
        limit: usize,
    ) -> Result<(), super::OutlineError> {
        let mut composite = self.clone();
        composite.composite = true;
        let originals: alloc::vec::Vec<_> =
            outline.iter().chain(phantoms.iter()).copied().collect();
        composite.hint_outline(
            originals.into_iter(),
            outline,
            tags,
            phantoms,
            ends,
            code,
            limit,
        )
    }

    #[allow(clippy::too_many_arguments)] // Parallel point and tag buffers share the glyph zone.
    pub(super) fn hint_outline(
        &self,
        unscaled: impl Iterator<Item = super::Point>,
        outline: &mut [super::Point],
        tags: &mut [u8],
        phantoms: &mut [super::Point; 4],
        ends: &[usize],
        code: &[u8],
        limit: usize,
    ) -> Result<(), super::OutlineError> {
        let points = unscaled
            .zip(outline.iter().chain(phantoms.iter()))
            .enumerate()
            .map(|(i, (raw, p))| {
                let scaled = [i64::from(p.x), i64::from(p.y)];
                HintPoint {
                    unscaled: [i64::from(raw.x), i64::from(raw.y)],
                    original: scaled,
                    current: scaled,
                    touched: [false; 2],
                    on_curve: tags.get(i).is_some_and(|tag| tag & 1 != 0),
                }
            })
            .collect();
        let (result, scan_type, compatibility) = self
            .execute_glyph(points, ends, code, limit)
            .map_err(|_| super::OutlineError::InvalidOutline)?;
        for (point, tag) in result.iter().zip(tags.iter_mut()) {
            *tag = (*tag & !0x19)
                | u8::from(point.on_curve)
                | (u8::from(point.touched[0]) << 3)
                | (u8::from(point.touched[1]) << 4);
        }
        if !code.is_empty()
            && let Some(tag) = tags.first_mut()
        {
            *tag |= ((scan_type as u8) << 5) | 4;
        }
        for (point, out) in result.iter().zip(
            outline
                .iter_mut()
                .chain(
                    phantoms
                        .iter_mut()
                        .take(if compatibility == 0 { 4 } else { 0 }),
                ),
        ) {
            out.x = point.current[0] as i32;
            out.y = point.current[1] as i32;
        }
        Ok(())
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn prepare(
        instance: sofel_font::var::Instance<'_, '_, '_, '_>,
        ppem: u16,
        mut engine: EngineInfo,
    ) -> Result<(Self, usize), Error> {
        use sofel_font::read::{CountWidth, Tag, maxp};
        let font = instance.font();
        let (bytes, width) = font
            .table(maxp::TAG_WIDE)
            .map(|b| (b, CountWidth::U24))
            .or_else(|| font.table(maxp::TAG).map(|b| (b, CountWidth::U16)))
            .ok_or(Error::InvalidReference)?;
        let maxima = maxp::Maxp::new(bytes, width)
            .map_err(|_| Error::InvalidReference)?
            .truetype()
            .unwrap_or_default();
        let mut grid_fit = maxima.max_size_of_instructions != 0;
        if engine.version == 40 {
            engine.tricky = super::tt_policy::tricky(font);
            engine.symmetric_smoothing = !grid_fit || ppem > 20;
            if let Some(flags) = font
                .table(Tag::new(b"gasp"))
                .and_then(|data| super::tt_policy::gasp(data, ppem))
            {
                engine.symmetric_smoothing = flags & 8 != 0;
                grid_fit = flags & 4 != 0;
            }
        }
        let cvt = if font.table(Tag::new(b"cvt ")).is_some() {
            instance
                .ft_control_values()
                .ok_or(Error::InvalidReference)?
        } else {
            alloc::vec::Vec::new()
        };
        let scale = sofel_font::read::cff::ft::div_fix(
            i64::from(ppem) * 64,
            i64::from(font.units_per_em()),
        );
        let (mut vm, limit) = Self::prepare_programs(
            maxima,
            &cvt,
            ppem,
            scale,
            engine,
            instance.ft_coords(),
            (
                font.table(Tag::new(b"fpgm")).unwrap_or_default(),
                font.table(Tag::new(b"prep")).unwrap_or_default(),
            ),
        )?;
        if engine.version == 40 && engine.target == HintTarget::Lcd && !engine.tricky && !grid_fit {
            vm.instruction_control |= 1;
        }
        Ok((vm, limit))
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn prepare_programs(
        maxima: sofel_font::read::maxp::TrueTypeMaxima,
        cvt: &[i32],
        ppem: u16,
        scale: i64,
        engine: EngineInfo,
        coords: &[sofel_font::read::Fixed],
        (font_program, prep): (&[u8], &[u8]),
    ) -> Result<(Self, usize), Error> {
        let declared = usize::from(maxima.max_stack_elements);
        let limit = declared + (declared / 2).max(128);
        let twilight_limit = (usize::from(maxima.max_twilight_points) + 4).min(65535);
        let twilight = twilight_limit.min(cvt.len().saturating_mul(2).max(30));
        let mut vm = Self::new(maxima.max_storage, cvt, scale);
        vm.engine = engine;
        vm.twilight_limit = twilight_limit;
        vm.instruction_def_limit = Some(maxima.max_instruction_defs);
        vm.blend_coords
            .extend(coords.iter().map(|coord| coord.0 >> 2));
        vm.points[0].resize(twilight, HintPoint::default());
        let mut font_vm = vm.clone();
        font_vm.cvt.fill(0);
        font_vm.execute(font_program, &mut alloc::vec::Vec::new(), limit)?;
        vm.functions = font_vm.functions;
        vm.instructions = font_vm.instructions;
        vm.ppem = [ppem; 2];
        vm.point_size = i64::from(ppem) * 64;
        vm.in_prep = true;
        vm.execute(prep, &mut alloc::vec::Vec::new(), limit)?;
        vm.in_prep = false;
        Ok((vm, limit))
    }

    pub(super) fn subpixel(&self) -> bool {
        self.engine.version == 40 && self.engine.target != HintTarget::Mono && !self.engine.tricky
    }

    fn native_horizontal(&self) -> bool {
        self.in_glyph
            && self.subpixel()
            && self.backward_compatibility == 0
            && self.freedom[1] == 0
            && self.projection[1] == 0
    }

    pub(super) fn device_width_allowed(&self) -> bool {
        !self.disables_hinting() && (!self.subpixel() || self.instruction_control & 4 != 0)
    }

    pub(super) fn disables_hinting(&self) -> bool {
        self.instruction_control & 1 != 0
    }

    fn execute_glyph(
        &self,
        points: alloc::vec::Vec<HintPoint>,
        contour_ends: &[usize],
        program: &[u8],
        max_stack: usize,
    ) -> Result<(alloc::vec::Vec<HintPoint>, u16, u8), Error> {
        let mut glyph = self.clone();
        glyph.in_glyph = true;
        glyph.instruction_def_limit = None;
        if self.instruction_control & 2 != 0 {
            glyph.minimum_distance = 64;
            glyph.control_value_cutin = 68;
            glyph.single_width_cutin = 0;
            glyph.single_width = 0;
            glyph.delta_base = 9;
            glyph.delta_shift = 3;
            glyph.auto_flip = true;
            glyph.scan_control = false;
            glyph.scan_type = 0;
        }
        if self.subpixel() {
            glyph.backward_compatibility = (self.instruction_control & 4) ^ 4;
        }
        let twilight = points
            .len()
            .saturating_add(self.cvt.len())
            .saturating_mul(2)
            .max(30);
        glyph.points[0].resize(twilight.min(self.twilight_limit), HintPoint::default());
        glyph.points[1] = points;
        let outline_len = contour_ends.last().map_or(0, |end| end.saturating_add(1));
        if let Some([left, right, top, bottom]) = glyph.points[1].get_mut(outline_len..) {
            for (point, axis) in [(left, 0), (right, 0), (top, 1), (bottom, 1)] {
                point.current[axis] = point.current[axis].wrapping_add(32) & !63;
            }
        }
        glyph.contour_ends = contour_ends.to_vec();
        glyph.reference = [0; 3];
        glyph.zones = [1; 3];
        glyph.projection = [16384, 0];
        glyph.dual_projection = glyph.projection;
        glyph.freedom = glyph.projection;
        glyph.loop_count = 1;
        glyph.rounding = Rounding::Grid;
        glyph.execute(program, &mut alloc::vec::Vec::new(), max_stack)?;
        Ok((
            core::mem::take(&mut glyph.points[1]),
            glyph.scan_type,
            glyph.backward_compatibility,
        ))
    }

    fn move_relative(&mut self, op: u8, index: u16, cvt: Option<i64>) {
        let native = self.native_horizontal();
        let cutin = if native {
            self.control_value_cutin >> 4
        } else {
            self.control_value_cutin
        };
        let min_distance = if native {
            self.minimum_distance >> 1
        } else {
            self.minimum_distance
        };
        let base_index = self.reference[0];
        self.reference[1] = base_index;
        self.reference[2] = index;
        if op & 16 != 0 {
            self.reference[0] = index;
        }
        let [base_zone, target_zone, _] = self.zones.map(usize::from);
        let Some(base) = self.points[base_zone].get(usize::from(base_index)).copied() else {
            return;
        };
        let Some(point) = self.points[target_zone].get_mut(usize::from(index)) else {
            return;
        };
        let mut requested = if let Some(entry) = cvt {
            match entry {
                -1 => 0,
                n => match usize::try_from(n).ok().and_then(|i| self.cvt.get(i)) {
                    Some(&value) => value,
                    None => return,
                },
            }
        } else if base_zone == 0 || target_zone == 0 {
            projected_distance(point.original, base.original, self.dual_projection)
        } else {
            sofel_font::read::cff::ft::mul_fix(
                projected_distance(point.unscaled, base.unscaled, self.dual_projection),
                if self.composite { 65536 } else { self.scale },
            )
        };
        if self.single_width_cutin > 0
            && requested.abs_diff(self.single_width) < self.single_width_cutin as u64
        {
            requested = if requested >= 0 {
                self.single_width
            } else {
                self.single_width.wrapping_neg()
            };
        }
        let original = if cvt.is_some() {
            if target_zone == 0 {
                point.original = core::array::from_fn(|axis| {
                    base.original[axis]
                        .wrapping_add(project([requested, 0], [self.freedom[axis], 0]))
                });
                point.current = point.original;
            }
            let measured = projected_distance(point.original, base.original, self.dual_projection);
            if self.auto_flip && (measured ^ requested) < 0 {
                requested = requested.wrapping_neg();
            }
            if op & 4 != 0
                && base_zone == target_zone
                && i128::from(measured.abs_diff(requested)) > i128::from(cutin)
            {
                requested = measured;
            }
            measured
        } else {
            requested
        };
        let mut distance = if op & 4 != 0 && !native {
            self.rounding.apply(requested)
        } else {
            requested
        };
        if op & 8 != 0 {
            distance = if original >= 0 {
                distance.max(min_distance)
            } else {
                distance.min(min_distance.wrapping_neg())
            };
        }
        let current = projected_distance(point.current, base.current, self.projection);
        move_point(
            point,
            distance.wrapping_sub(current),
            self.projection,
            self.freedom,
            self.backward_compatibility,
        );
    }

    fn reference_shift(&self, op: u8) -> Option<(usize, usize, [i64; 2])> {
        let (zone, reference) = if op & 1 == 0 {
            (self.zones[1], self.reference[2])
        } else {
            (self.zones[0], self.reference[1])
        };
        let zone = usize::from(zone);
        let reference = usize::from(reference);
        let base = self.points[zone].get(reference)?;
        let distance = projected_distance(base.current, base.original, self.projection);
        let delta = movement_factors(self.projection, self.freedom)
            .map(|factor| sofel_font::read::cff::ft::mul_fix(distance, factor));
        Some((zone, reference, delta))
    }

    fn interpolate(&mut self, stack: &mut alloc::vec::Vec<i64>) {
        let count = usize::from(core::mem::replace(&mut self.loop_count, 1));
        let Some(first) = stack.len().checked_sub(count) else {
            return;
        };
        let [z0, z1, z2] = self.zones.map(usize::from);
        let base = self.points[z0].get(usize::from(self.reference[1])).copied();
        let end = self.points[z1].get(usize::from(self.reference[2])).copied();
        let twilight = self.zones.contains(&0);
        let original = |p: HintPoint| if twilight { p.original } else { p.unscaled };
        if let Some(base) = base {
            let original_offset =
                |p| projected_distance(original(p), original(base), self.dual_projection);
            let current_offset =
                |p: HintPoint| projected_distance(p.current, base.current, self.projection);
            let old_range = end.map_or(0, original_offset);
            let new_range = end.map_or(0, current_offset);
            for &index in stack[first..].iter().rev() {
                if let Some(p) = self.points[z2].get_mut(index as u32 as usize) {
                    let offset = original_offset(*p);
                    let distance = if old_range == 0 {
                        offset
                    } else {
                        sofel_font::read::cff::ft::mul_div(offset, new_range, old_range)
                    };
                    let delta = distance.wrapping_sub(current_offset(*p));
                    move_point(
                        p,
                        delta,
                        self.projection,
                        self.freedom,
                        self.backward_compatibility,
                    );
                }
            }
        }
        stack.truncate(first);
    }

    fn projected_ppem(&self) -> i64 {
        i64::from(self.ppem[usize::from(self.projection[0] == 0)])
    }

    fn execute(
        &mut self,
        code: &[u8],
        stack: &mut alloc::vec::Vec<i64>,
        max_stack: usize,
    ) -> Result<(), Error> {
        self.execute_frame(code, stack, max_stack, 0, &mut 1_000_000)
    }

    #[allow(clippy::arithmetic_side_effects)]
    fn execute_frame(
        &mut self,
        mut code: &[u8],
        stack: &mut alloc::vec::Vec<i64>,
        max_stack: usize,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<(), Error> {
        let program = code;
        while !code.is_empty() {
            *remaining = remaining.checked_sub(1).ok_or(Error::ExecutionLimit)?;
            let offset = program.len() - code.len();
            let instruction = next(&mut code)?;
            match instruction.opcode {
                op @ 0x00..=0x05 => {
                    let axis = if op & 1 == 0 { [0, 16384] } else { [16384, 0] };
                    if op < 4 {
                        self.projection = axis;
                        self.dual_projection = axis;
                    }
                    if op & 2 == 0 {
                        self.freedom = axis;
                    }
                }
                op @ (0x06..=0x09 | 0x86 | 0x87) => {
                    let second = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    let first = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    let Some((a, b)) = self.points[usize::from(self.zones[1])]
                        .get(usize::from(first))
                        .zip(self.points[usize::from(self.zones[2])].get(usize::from(second)))
                    else {
                        continue;
                    };
                    let dual = matches!(op, 0x86 | 0x87);
                    let perpendicular = op & 1 != 0;
                    let current = line_vector(
                        a.current,
                        b.current,
                        perpendicular && (!dual || a.original != b.original),
                    );
                    if matches!(op, 0x08 | 0x09) {
                        self.freedom = current;
                    } else {
                        self.projection = current;
                        self.dual_projection = if dual {
                            line_vector(a.original, b.original, perpendicular)
                        } else {
                            current
                        };
                    }
                }
                op @ (0x0a | 0x0b) => {
                    let y = stack.pop().ok_or(Error::StackUnderflow)? as i16;
                    let x = stack.pop().ok_or(Error::StackUnderflow)? as i16;
                    let vector = if op == 0x0a {
                        &mut self.projection
                    } else {
                        &mut self.freedom
                    };
                    if x != 0 || y != 0 {
                        let (unit, _) = crate::slot_synth::normalize(sofel_geom::Point {
                            x: i32::from(x),
                            y: i32::from(y),
                        });
                        *vector = [i64::from(unit.x / 4), i64::from(unit.y / 4)];
                    }
                    if op == 0x0a {
                        self.dual_projection = self.projection;
                    }
                }
                op @ (0x0c | 0x0d) => {
                    if max_stack.saturating_sub(stack.len()) < 2 {
                        return Err(Error::StackOverflow);
                    }
                    stack.extend(if op == 0x0c {
                        self.projection
                    } else {
                        self.freedom
                    });
                }
                0x0f => {
                    let start = stack.len().checked_sub(5).ok_or(Error::StackUnderflow)?;
                    let [target, a0, a1, b0, b1] = *stack[start..]
                        .first_chunk::<5>()
                        .ok_or(Error::StackUnderflow)?;
                    stack.truncate(start);
                    let [z0, z1, z2] = self.zones.map(usize::from);
                    let coords = |zone: usize, index: i64| {
                        self.points[zone]
                            .get(usize::from(index as u16))
                            .map(|point| point.current)
                    };
                    if let (Some(a0), Some(a1), Some(b0), Some(b1)) = (
                        coords(z1, a0),
                        coords(z1, a1),
                        coords(z0, b0),
                        coords(z0, b1),
                    ) && let Some(point) = self.points[z2].get_mut(usize::from(target as u16))
                    {
                        point.current = intersection(a0, a1, b0, b1);
                        point.touched = [true; 2];
                    }
                }
                0x0e => self.freedom = self.projection,
                op @ (0x4b | 0x4c) => {
                    let value = if op == 0x4c && self.engine.version == 40 {
                        self.point_size
                    } else {
                        self.projected_ppem()
                    };
                    push(stack, value, max_stack)?;
                }
                op @ 0x10..=0x12 => {
                    self.reference[usize::from(op - 0x10)] =
                        stack.pop().ok_or(Error::StackUnderflow)? as u16;
                }
                op @ 0x13..=0x16 => {
                    let zone = stack.pop().ok_or(Error::StackUnderflow)? as i32;
                    if matches!(zone, 0 | 1) {
                        if op == 0x16 {
                            self.zones.fill(zone as u8);
                        } else {
                            self.zones[usize::from(op - 0x13)] = zone as u8;
                        }
                    }
                }
                op @ 0x1d..=0x1f => {
                    let value = stack.pop().ok_or(Error::StackUnderflow)?;
                    match op {
                        0x1d => self.control_value_cutin = value,
                        0x1e => self.single_width_cutin = value,
                        _ => {
                            self.single_width =
                                sofel_font::read::cff::ft::mul_fix(value, self.scale)
                        }
                    }
                }
                0x5e => self.delta_base = stack.pop().ok_or(Error::StackUnderflow)? as u16,
                0x5f => {
                    let shift = stack.pop().ok_or(Error::StackUnderflow)?;
                    if !(0..=6).contains(&shift) {
                        return Err(Error::InvalidArgument);
                    }
                    self.delta_shift = shift as u8;
                }
                op @ (0x5d | 0x71..=0x75) => {
                    let count = stack.pop().ok_or(Error::StackUnderflow)?;
                    let pairs = usize::try_from(count)
                        .unwrap_or(usize::MAX)
                        .min(stack.len() / 2);
                    let first = stack.len() - 2 * pairs;
                    let band = self.projected_ppem()
                        - i64::from(self.delta_base)
                        - match op {
                            0x71 | 0x74 => 16,
                            0x72 | 0x75 => 32,
                            _ => 0,
                        };
                    for pair in stack[first..].rchunks_exact(2) {
                        let argument = pair[0];
                        if (argument & 0xf0) >> 4 != band {
                            continue;
                        }
                        let step = (argument & 15) - 8;
                        let delta = (step + i64::from(step >= 0)) << (6 - self.delta_shift);
                        if op >= 0x73 {
                            if let Some(value) = usize::try_from(pair[1])
                                .ok()
                                .and_then(|i| self.cvt.get_mut(i))
                            {
                                *value = value.wrapping_add(delta);
                            }
                        } else if let Some(point) = self.points[usize::from(self.zones[0])]
                            .get_mut(usize::from(pair[1] as u16))
                        {
                            let axis = usize::from(self.freedom[0] == 0);
                            let allowed = self.backward_compatibility == 0
                                || (self.backward_compatibility != 7
                                    && ((self.composite && axis == 1) || point.touched[1]));
                            if allowed && self.projection[axis] != 0 {
                                move_axis(
                                    point,
                                    axis,
                                    point.current[axis].wrapping_add(delta),
                                    self.backward_compatibility,
                                );
                            }
                        }
                    }
                    stack.truncate(first);
                }
                op @ (0x32 | 0x33) => {
                    let count = usize::from(core::mem::replace(&mut self.loop_count, 1));
                    let Some(first) = stack.len().checked_sub(count) else {
                        continue;
                    };
                    if let Some((_, _, delta)) = self.reference_shift(op) {
                        for &id in stack[first..].iter().rev() {
                            if let Some(p) =
                                self.points[usize::from(self.zones[2])].get_mut(id as u32 as usize)
                            {
                                shift_point(p, delta, self.freedom, self.backward_compatibility);
                            }
                        }
                    }
                    stack.truncate(first);
                }
                op @ (0x34 | 0x35) => {
                    let contour = usize::from(stack.pop().ok_or(Error::StackUnderflow)? as u16);
                    let target = usize::from(self.zones[2]);
                    let (start, end) = if target == 0 && contour == 0 {
                        (0, self.points[0].len())
                    } else if target == 1 && contour < self.contour_ends.len() {
                        (
                            contour
                                .checked_sub(1)
                                .map_or(0, |i| self.contour_ends[i] + 1),
                            self.contour_ends[contour] + 1,
                        )
                    } else {
                        continue;
                    };
                    let Some((zone, reference, delta)) = self.reference_shift(op) else {
                        continue;
                    };
                    let points = self.points[target]
                        .get_mut(start..end)
                        .ok_or(Error::InvalidReference)?;
                    for (offset, p) in points.iter_mut().enumerate() {
                        if zone != target || offset + start != reference {
                            shift_point(p, delta, self.freedom, self.backward_compatibility);
                        }
                    }
                }
                op @ (0x36 | 0x37) => {
                    let target = stack.pop().ok_or(Error::StackUnderflow)? as i32;
                    if !matches!(target, 0 | 1) {
                        continue;
                    }
                    let target = target as usize;
                    let Some((zone, reference, delta)) = self.reference_shift(op) else {
                        continue;
                    };
                    let points = &mut self.points[target];
                    let limit = if target == 1 {
                        points.len().saturating_sub(4)
                    } else {
                        points.len()
                    };
                    for (i, point) in points.iter_mut().take(limit).enumerate() {
                        if target != zone || i != reference {
                            point.current[0] = point.current[0].wrapping_add(delta[0]);
                            point.current[1] = point.current[1].wrapping_add(delta[1]);
                        }
                    }
                }
                0x38 => {
                    let distance = stack.pop().ok_or(Error::StackUnderflow)?;
                    let count = usize::from(core::mem::replace(&mut self.loop_count, 1));
                    let Some(start) = stack.len().checked_sub(count) else {
                        continue;
                    };
                    let delta = self
                        .freedom
                        .map(|component| project([distance, 0], [component, 0]));
                    let twilight = self.zones.contains(&0);
                    let zone = usize::from(self.zones[2]);
                    for index in stack.drain(start..).rev() {
                        let Some(point) = self.points[zone].get_mut(index as u32 as usize) else {
                            continue;
                        };
                        let compatibility = self.backward_compatibility;
                        if compatibility == 0
                            || twilight
                            || (compatibility != 7
                                && ((self.composite && self.freedom[1] != 0) || point.touched[1]))
                        {
                            shift_point(point, delta, self.freedom, compatibility);
                        }
                    }
                }
                op @ (0x3a | 0x3b) => {
                    let distance = stack.pop().ok_or(Error::StackUnderflow)?;
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    let [z0, z1, _] = self.zones.map(usize::from);
                    let Some(base) = self.points[z0].get(usize::from(self.reference[0])).copied()
                    else {
                        continue;
                    };
                    let Some(p) = self.points[z1].get_mut(usize::from(index)) else {
                        continue;
                    };
                    if z1 == 0 {
                        p.current = base.original;
                        move_point(
                            p,
                            distance,
                            self.projection,
                            self.freedom,
                            self.backward_compatibility,
                        );
                        p.original = p.current;
                    }
                    let current = projected_distance(p.current, base.current, self.projection);
                    move_point(
                        p,
                        distance.wrapping_sub(current),
                        self.projection,
                        self.freedom,
                        self.backward_compatibility,
                    );
                    self.reference[1] = self.reference[0];
                    self.reference[2] = index;
                    if op == 0x3b {
                        self.reference[0] = index;
                    }
                }
                0x27 => {
                    let second = usize::from(stack.pop().ok_or(Error::StackUnderflow)? as u16);
                    let first = usize::from(stack.pop().ok_or(Error::StackUnderflow)? as u16);
                    let [z0, z1, _] = self.zones.map(usize::from);
                    let Some((a, b)) = self.points[z0].get(second).zip(self.points[z1].get(first))
                    else {
                        continue;
                    };
                    let distance = projected_distance(a.current, b.current, self.projection) / 2;
                    move_point(
                        &mut self.points[z1][first],
                        distance,
                        self.projection,
                        self.freedom,
                        self.backward_compatibility,
                    );
                    move_point(
                        &mut self.points[z0][second],
                        distance.wrapping_neg(),
                        self.projection,
                        self.freedom,
                        self.backward_compatibility,
                    );
                }
                0x3c => {
                    let count = usize::from(core::mem::replace(&mut self.loop_count, 1));
                    let Some(start) = stack.len().checked_sub(count) else {
                        continue;
                    };
                    let [z0, z1, _] = self.zones.map(usize::from);
                    for index in stack.drain(start..).rev() {
                        let Some(base) = self.points[z0]
                            .get(usize::from(self.reference[0]))
                            .map(|p| p.current)
                        else {
                            break;
                        };
                        if let Some(point) = self.points[z1].get_mut(usize::from(index as u16)) {
                            let distance = projected_distance(point.current, base, self.projection);
                            move_point(
                                point,
                                distance.wrapping_neg(),
                                self.projection,
                                self.freedom,
                                self.backward_compatibility,
                            );
                        }
                    }
                }
                0x29 => {
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    if let Some(point) =
                        self.points[usize::from(self.zones[0])].get_mut(usize::from(index))
                    {
                        for (touched, freedom) in point.touched.iter_mut().zip(self.freedom) {
                            if freedom != 0 {
                                *touched = false;
                            }
                        }
                    }
                }
                op @ (0x2e | 0x2f) => {
                    let native = self.native_horizontal();
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    let zone = usize::from(self.zones[0]);
                    if let Some(point) = self.points[zone].get_mut(usize::from(index)) {
                        let delta = if op == 0x2f && !native {
                            let current = project(point.current, self.projection);
                            self.rounding.apply(current).wrapping_sub(current)
                        } else {
                            0
                        };
                        move_point(
                            point,
                            delta,
                            self.projection,
                            self.freedom,
                            self.backward_compatibility,
                        );
                        self.reference[..2].fill(index);
                    }
                }
                op @ (0x3e | 0x3f) => {
                    let native = self.native_horizontal();
                    let cutin = if native {
                        self.control_value_cutin >> 4
                    } else {
                        self.control_value_cutin
                    };
                    let cvt = stack.pop().ok_or(Error::StackUnderflow)?;
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    self.reference[..2].fill(index);
                    let zone = usize::from(self.zones[0]);
                    let Some(point) = self.points[zone].get_mut(usize::from(index)) else {
                        continue;
                    };
                    let Some(&value) = usize::try_from(cvt).ok().and_then(|i| self.cvt.get(i))
                    else {
                        continue;
                    };
                    let freedom = usize::from(self.freedom[0] == 0);
                    let projection = usize::from(self.projection[0] == 0);
                    if zone == 0 {
                        point.original = [0; 2];
                        point.original[freedom] = value;
                        point.current = point.original;
                    }
                    let current = point.current[projection];
                    let distance = if op == 0x3e {
                        value
                    } else {
                        let v = if i128::from(value.abs_diff(current)) > i128::from(cutin) {
                            current
                        } else {
                            value
                        };
                        if native { v } else { self.rounding.apply(v) }
                    };
                    if freedom == projection {
                        move_axis(point, freedom, distance, self.backward_compatibility);
                    }
                }
                0x17 => {
                    let count = stack.pop().ok_or(Error::StackUnderflow)?;
                    if count < 0 {
                        return Err(Error::InvalidArgument);
                    }
                    self.loop_count = count.min(65535) as u16;
                }
                0x39 => self.interpolate(stack),
                op @ (0x46 | 0x47) => {
                    let index = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    let zone = &self.points[usize::from(self.zones[2])];
                    *index = usize::try_from(*index)
                        .ok()
                        .and_then(|i| zone.get(i))
                        .map_or(0, |point| {
                            if op == 0x47 {
                                project(point.original, self.dual_projection)
                            } else {
                                project(point.current, self.projection)
                            }
                        });
                }
                op @ (0x49 | 0x4a) => {
                    let second = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    let first = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    let [z0, z1, _] = self.zones.map(usize::from);
                    let pair = self.points[z0]
                        .get(usize::from(*first as u16))
                        .zip(self.points[z1].get(usize::from(second)));
                    *first = pair.map_or(0, |(a, b)| {
                        let (left, right, vector, scale) = if op == 0x49 {
                            (a.current, b.current, self.projection, 65536)
                        } else if z0 == 0 || z1 == 0 {
                            (a.original, b.original, self.dual_projection, 65536)
                        } else {
                            (
                                a.unscaled,
                                b.unscaled,
                                self.dual_projection,
                                if self.composite { 65536 } else { self.scale },
                            )
                        };
                        let delta = [
                            left[0].wrapping_sub(right[0]),
                            left[1].wrapping_sub(right[1]),
                        ];
                        sofel_font::read::cff::ft::mul_fix(project(delta, vector), scale)
                    });
                }
                0x48 => {
                    let coordinate = stack.pop().ok_or(Error::StackUnderflow)?;
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    let zone = usize::from(self.zones[2]);
                    if let Some(point) = self.points[zone].get_mut(usize::from(index)) {
                        let distance =
                            coordinate.wrapping_sub(project(point.current, self.projection));
                        move_point(
                            point,
                            distance,
                            self.projection,
                            self.freedom,
                            self.backward_compatibility,
                        );
                        if zone == 0 {
                            point.original = point.current;
                        }
                    }
                }
                0x1a => self.minimum_distance = stack.pop().ok_or(Error::StackUnderflow)?,
                0x80 => {
                    let count = usize::from(core::mem::replace(&mut self.loop_count, 1));
                    let Some(first) = stack.len().checked_sub(count) else {
                        continue;
                    };
                    *remaining = remaining.checked_sub(count).ok_or(Error::ExecutionLimit)?;
                    for index in stack.drain(first..).rev() {
                        if self.backward_compatibility != 7
                            && let Some(point) = self.points[1].get_mut(usize::from(index as u16))
                        {
                            point.on_curve = !point.on_curve;
                        }
                    }
                }
                op @ (0x81 | 0x82) => {
                    let last = usize::from(stack.pop().ok_or(Error::StackUnderflow)? as u16);
                    let first = usize::from(stack.pop().ok_or(Error::StackUnderflow)? as u16);
                    if self.backward_compatibility != 7
                        && let Some(range) = self.points[1].get_mut(first..=last)
                    {
                        *remaining = remaining
                            .checked_sub(range.len())
                            .ok_or(Error::ExecutionLimit)?;
                        for point in range {
                            point.on_curve = op == 0x81;
                        }
                    }
                }
                0x4d => self.auto_flip = true,
                0x4e => self.auto_flip = false,
                op @ 0xc0..=0xff => {
                    let cvt = if op >= 0xe0 {
                        Some(stack.pop().ok_or(Error::StackUnderflow)?)
                    } else {
                        None
                    };
                    let index = stack.pop().ok_or(Error::StackUnderflow)? as u16;
                    self.move_relative(op, index, cvt);
                }
                op @ (0x30 | 0x31) => {
                    if self.backward_compatibility == 7 {
                        continue;
                    }
                    if self.backward_compatibility != 0 {
                        self.backward_compatibility |= 1 << (op & 1);
                    }
                    let axis = usize::from(op == 0x30);
                    let mut start = 0;
                    for &end in &self.contour_ends {
                        let contour = self.points[1]
                            .get_mut(start..=end)
                            .ok_or(Error::InvalidReference)?;
                        interpolate_contour(contour, axis);
                        start = end + 1;
                    }
                }
                0x8e => {
                    let selector = stack.pop().ok_or(Error::StackUnderflow)?;
                    let value = stack.pop().ok_or(Error::StackUnderflow)?;
                    if self.in_glyph
                        && self.engine.version == 40
                        && selector == 3
                        && (value == 0 || value == 4)
                    {
                        self.backward_compatibility = (value as u8 & 4) ^ 4;
                    }
                    if self.in_prep && (1..=3).contains(&selector) {
                        let flag = 1u8 << (selector - 1);
                        if value == 0 {
                            self.instruction_control &= !flag;
                        } else if value == i64::from(flag) {
                            self.instruction_control |= flag;
                        }
                    }
                }
                0x85 => {
                    let flags = stack.pop().ok_or(Error::StackUnderflow)?;
                    let limit = flags & 255;
                    let ppem = i64::from(self.ppem[0].max(self.ppem[1]));
                    self.scan_control = match limit {
                        0 => false,
                        255 => true,
                        _ if (flags & 0x800 != 0 && ppem > limit)
                            || (flags & 0x1000 != 0 && self.engine.rotated)
                            || (flags & 0x2000 != 0 && self.engine.stretched) =>
                        {
                            false
                        }
                        _ if (flags & 0x100 != 0 && ppem <= limit)
                            || (flags & 0x200 != 0 && self.engine.rotated)
                            || (flags & 0x400 != 0 && self.engine.stretched) =>
                        {
                            true
                        }
                        _ => self.scan_control,
                    };
                }
                0x8d => {
                    let mode = stack.pop().ok_or(Error::StackUnderflow)?;
                    if mode >= 0 {
                        self.scan_type = mode as u16;
                    }
                }
                0x91 if !self.blend_coords.is_empty() => {
                    if self.blend_coords.len() > max_stack.saturating_sub(stack.len()) {
                        return Err(Error::StackOverflow);
                    }
                    stack.extend(self.blend_coords.iter().copied().map(i64::from));
                }
                0x92 if !self.blend_coords.is_empty() => push(stack, 17, max_stack)?,
                0x88 => {
                    let selector = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *selector = self.engine.query(*selector);
                }
                op @ (0x2c | 0x89) => {
                    let id = u16::try_from(stack.pop().ok_or(Error::StackUnderflow)?)
                        .map_err(|_| Error::InvalidReference)?;
                    let instruction_id = if op == 0x89 {
                        let opcode = u8::try_from(id).map_err(|_| Error::InvalidReference)?;
                        let limit = self.instruction_def_limit.ok_or(Error::InvalidOpcode)?;
                        if !self.instructions.contains_key(&opcode)
                            && self.instructions.len() >= usize::from(limit)
                        {
                            return Err(Error::InvalidReference);
                        }
                        Some(opcode)
                    } else {
                        None
                    };
                    let start = code;
                    loop {
                        let size = start.len() - code.len();
                        match next(&mut code)?.opcode {
                            0x2d => {
                                let body = alloc::rc::Rc::from(&start[..size]);
                                if let Some(opcode) = instruction_id {
                                    self.instructions.insert(opcode, body);
                                } else {
                                    self.functions.insert(id, body);
                                }
                                break;
                            }
                            0x2c | 0x89 => return Err(Error::InvalidOpcode),
                            _ => {}
                        }
                    }
                }
                op @ (0x2a | 0x2b) => {
                    let id = u16::try_from(stack.pop().ok_or(Error::StackUnderflow)?)
                        .map_err(|_| Error::InvalidReference)?;
                    let count = if op == 0x2a {
                        stack.pop().ok_or(Error::StackUnderflow)?
                    } else {
                        1
                    };
                    let body = self
                        .functions
                        .get(&id)
                        .cloned()
                        .ok_or(Error::InvalidReference)?;
                    if depth >= 32 {
                        return Err(Error::StackOverflow);
                    }
                    for _ in 0..count {
                        // ENDF consumes an instruction even for an empty body.
                        *remaining = remaining.checked_sub(1).ok_or(Error::ExecutionLimit)?;
                        self.execute_frame(&body, stack, max_stack, depth + 1, remaining)?;
                    }
                }
                0x40 | 0x41 | 0xb0..=0xbf => {
                    let data = instruction.immediate;
                    let count = if instruction.words {
                        data.len() / 2
                    } else {
                        data.len()
                    };
                    if count > max_stack.saturating_sub(stack.len()) {
                        return Err(Error::StackOverflow);
                    }
                    if instruction.words {
                        stack.extend(
                            data.as_chunks::<2>()
                                .0
                                .iter()
                                .map(|v| i64::from(i16::from_be_bytes(*v))),
                        );
                    } else {
                        stack.extend(data.iter().copied().map(i64::from));
                    }
                }
                op @ (0x76 | 0x77) => {
                    let selector = stack.pop().ok_or(Error::StackUnderflow)?;
                    self.rounding = Rounding::super_round(selector, op == 0x77);
                }
                0x18 => self.rounding = Rounding::Grid,
                0x19 => self.rounding = Rounding::HalfGrid,
                0x3d => self.rounding = Rounding::DoubleGrid,
                0x7a => self.rounding = Rounding::Off,
                0x7c => self.rounding = Rounding::Up,
                0x7d => self.rounding = Rounding::Down,
                op @ (0x56..=0x57 | 0x68..=0x6f) => {
                    let value = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *value = match op {
                        0x56 => i64::from(self.rounding.apply(*value) & 64 != 0),
                        0x57 => i64::from(self.rounding.apply(*value) & 64 == 0),
                        0x68..=0x6b if !self.native_horizontal() => self.rounding.apply(*value),
                        _ => *value,
                    };
                }
                op @ (0x42 | 0x44 | 0x70) => {
                    let value = stack.pop().ok_or(Error::StackUnderflow)?;
                    let index = stack.pop().ok_or(Error::StackUnderflow)?;
                    let table = if op == 0x42 {
                        &mut self.storage
                    } else {
                        &mut self.cvt
                    };
                    if let Some(slot) = usize::try_from(index).ok().and_then(|i| table.get_mut(i)) {
                        *slot = if op == 0x70 {
                            sofel_font::read::cff::ft::mul_fix(value, self.scale)
                        } else {
                            value
                        };
                    }
                }
                op @ (0x43 | 0x45) => {
                    let table = if op == 0x43 { &self.storage } else { &self.cvt };
                    let index = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *index = usize::try_from(*index)
                        .ok()
                        .and_then(|i| table.get(i))
                        .copied()
                        .unwrap_or(0);
                }
                0x20 => {
                    let value = *stack.last().ok_or(Error::StackUnderflow)?;
                    push(stack, value, max_stack)?;
                }
                // POP, SANGW, and AA each discard one operand in FreeType.
                0x21 | 0x7e | 0x7f => {
                    stack.pop().ok_or(Error::StackUnderflow)?;
                }
                0x22 => stack.clear(),
                0x23 | 0x8a => {
                    let count = if instruction.opcode == 0x23 { 2 } else { 3 };
                    let start = stack
                        .len()
                        .checked_sub(count)
                        .ok_or(Error::StackUnderflow)?;
                    stack[start..].rotate_left(1);
                }
                0x24 => push(
                    stack,
                    i64::try_from(stack.len()).map_err(|_| Error::StackOverflow)?,
                    max_stack,
                )?,
                0x25 | 0x26 => {
                    let index = stack.pop().ok_or(Error::StackUnderflow)?;
                    let start = usize::try_from(index)
                        .ok()
                        .filter(|&n| n != 0)
                        .and_then(|n| stack.len().checked_sub(n));
                    if instruction.opcode == 0x25 {
                        let value = start.and_then(|i| stack.get(i)).copied().unwrap_or(0);
                        stack.push(value);
                    } else if let Some(i) = start {
                        stack[i..].rotate_left(1);
                    }
                }
                op @ (0x50..=0x55 | 0x5a..=0x5b | 0x60..=0x63 | 0x8b..=0x8c) => {
                    let [.., left, right] = stack.as_mut_slice() else {
                        return Err(Error::StackUnderflow);
                    };
                    *left = binary(op, *left, *right)?;
                    stack.pop();
                }
                op @ (0x5c | 0x64..=0x67) => {
                    let value = stack.last_mut().ok_or(Error::StackUnderflow)?;
                    *value = match op {
                        0x5c => i64::from(*value == 0),
                        0x64 => value.wrapping_abs(),
                        0x65 => value.wrapping_neg(),
                        0x66 => *value & !63,
                        _ => value.wrapping_add(63) & !63,
                    };
                }
                0x58 => {
                    if stack.pop().ok_or(Error::StackUnderflow)? == 0 {
                        skip_block(&mut code, true)?;
                    }
                }
                op @ (0x1c | 0x78 | 0x79) => {
                    let take = if op == 0x1c {
                        true
                    } else {
                        let flag = stack.pop().ok_or(Error::StackUnderflow)?;
                        (flag != 0) == (op == 0x78)
                    };
                    let relative = stack.pop().ok_or(Error::StackUnderflow)?;
                    if take {
                        if relative == 0 && stack.is_empty() {
                            return Err(Error::InvalidArgument);
                        }
                        let target = offset
                            .checked_add_signed(
                                isize::try_from(relative).map_err(|_| Error::InvalidArgument)?,
                            )
                            .ok_or(Error::InvalidArgument)?;
                        if target > program.len() && depth != 0 {
                            return Err(Error::InvalidArgument);
                        }
                        code = program.get(target..).unwrap_or_default();
                    }
                }
                0x1b => skip_block(&mut code, false)?,
                0x59 => {}
                op @ (0x28 | 0x7b | 0x83 | 0x84 | 0x8f..=0xaf) => {
                    let body = self
                        .instructions
                        .get(&op)
                        .cloned()
                        .ok_or(Error::InvalidOpcode)?;
                    if depth >= 32 {
                        return Err(Error::StackOverflow);
                    }
                    *remaining = remaining.checked_sub(1).ok_or(Error::ExecutionLimit)?;
                    self.execute_frame(&body, stack, max_stack, depth + 1, remaining)?;
                }
                _ => return Err(Error::InvalidOpcode),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
fn run(code: &[u8], stack: &mut alloc::vec::Vec<i64>, max_stack: usize) -> Result<(), Error> {
    Interpreter::new(0, &[], 0x10000).execute(code, stack, max_stack)
}

#[test]
fn adr0064_i1_index_and_roll_select_from_the_top_of_the_stack() {
    let mut values = alloc::vec::Vec::new();
    run(
        &[0xb2, 1, 2, 3, 0x8a, 0xb0, 3, 0x25, 0xb0, 3, 0x26, 0x24],
        &mut values,
        8,
    )
    .unwrap();
    assert_eq!(values, [2, 1, 2, 3, 4]);
    run(&[0x22, 0xb0, 0, 0x25, 0xb0, 9, 0x26], &mut values, 8).unwrap();
    assert_eq!(values, [0]);
}

fn push(stack: &mut alloc::vec::Vec<i64>, value: i64, limit: usize) -> Result<(), Error> {
    if stack.len() >= limit {
        return Err(Error::StackOverflow);
    }
    stack.push(value);
    Ok(())
}

#[test]
fn adr0064_i1_arithmetic_rounds_multiply_and_truncates_division() {
    for (op, a, b, expected) in [
        (0x63, -1, 32, -1),
        (0x63, 1, 31, 0),
        (0x62, -5, 3, -106),
        (0x62, 5, -3, -106),
        (0x50, -7, 2, 1),
        (0x5b, 0, -5, 1),
        (0x8b, -20, -4, -4),
    ] {
        let mut stack = alloc::vec![a, b];
        run(&[op], &mut stack, 2).unwrap();
        assert_eq!(stack, [expected]);
    }
    assert_eq!(
        run(&[0x62], &mut alloc::vec![1, 0], 2),
        Err(Error::DivideByZero)
    );
    for (op, input, expected) in [(0x66, -65, -128), (0x67, -65, -64), (0x5c, -1, 0)] {
        let mut stack = alloc::vec![input];
        run(&[op], &mut stack, 1).unwrap();
        assert_eq!(stack, [expected]);
    }
}

fn binary(op: u8, a: i64, b: i64) -> Result<i64, Error> {
    Ok(match op {
        0x50 => i64::from(a < b),
        0x51 => i64::from(a <= b),
        0x52 => i64::from(a > b),
        0x53 => i64::from(a >= b),
        0x54 => i64::from(a == b),
        0x55 => i64::from(a != b),
        0x5a => i64::from(a != 0 && b != 0),
        0x5b => i64::from(a != 0 || b != 0),
        0x60 => a.wrapping_add(b),
        0x61 => a.wrapping_sub(b),
        0x62 | 0x63 => {
            // FreeType computes unsigned magnitudes, then restores the sign.
            let (x, y) = (a.unsigned_abs(), b.unsigned_abs());
            let magnitude = if op == 0x62 {
                x.wrapping_mul(64)
                    .checked_div(y)
                    .ok_or(Error::DivideByZero)?
            } else {
                x.wrapping_mul(y).wrapping_add(32) >> 6
            }
            .cast_signed();
            if (a < 0) != (b < 0) {
                magnitude.wrapping_neg()
            } else {
                magnitude
            }
        }
        0x8b => a.max(b),
        0x8c => a.min(b),
        _ => return Err(Error::InvalidOpcode),
    })
}

#[test]
fn adr0064_i1_rounding_state_controls_negative_distances_and_parity() {
    for (mode, expected) in [
        (0x18, -64),
        (0x19, -96),
        (0x3d, -64),
        (0x7a, -65),
        (0x7c, -128),
        (0x7d, -64),
    ] {
        let mut stack = alloc::vec![-65];
        run(&[mode, 0x68], &mut stack, 1).unwrap();
        assert_eq!(stack, [expected]);
    }
    for (op, expected) in [(0x56, 1), (0x57, 0), (0x6c, -65)] {
        let mut stack = alloc::vec![-65];
        run(&[op], &mut stack, 1).unwrap();
        assert_eq!(stack, [expected]);
    }
}

#[test]
fn adr0064_i1_super_rounding_keeps_phase_and_diagonal_period() {
    for (op, selector, input, expected) in [
        (0x76, 0x54, 0, 16),
        (0x76, 0x54, -81, -80),
        (0x77, 0x44, 89, 45),
        (0x77, 0x44, -90, -90),
    ] {
        let mut stack = alloc::vec![input, selector];
        run(&[op, 0x68], &mut stack, 2).unwrap();
        assert_eq!(stack, [expected]);
    }
}

#[test]
fn adr0064_i1_glyph_storage_writes_are_private_and_bounded() {
    let mut prep = Interpreter::new(2, &[], 0x10000);
    let mut stack = alloc::vec![1, -42];
    prep.execute(&[0x42], &mut stack, 4).unwrap();
    let mut glyph = prep.clone();
    glyph
        .execute(
            &[0xb1, 1, 99, 0x42, 0xb1, 2, 7, 0x42, 0xb0, 2, 0x43],
            &mut stack,
            4,
        )
        .unwrap();
    assert_eq!(stack, [0]);
    prep.execute(&[0xb0, 1, 0x43], &mut stack, 4).unwrap();
    assert_eq!(stack, [0, -42]);
}

#[test]
fn adr0064_i1_control_values_scale_font_units_and_preserve_pixel_units() {
    let mut vm = Interpreter::new(0, &[21 * 64 + 63, -21 * 64 - 63], 0x8000);
    let mut stack = alloc::vec::Vec::new();
    vm.execute(&[0xb0, 0, 0x45, 0xb0, 1, 0x45], &mut stack, 8)
        .unwrap();
    assert_eq!(stack, [11, -11]);
    vm.execute(
        &[
            0xb1, 1, 9, 0x70, 0xb0, 1, 0x45, 0xb1, 0, 9, 0x44, 0xb0, 0, 0x45,
        ],
        &mut stack,
        8,
    )
    .unwrap();
    assert_eq!(stack, [11, -11, 5, 9]);
}

#[test]
fn adr0064_i1_font_functions_share_the_operand_stack_and_bound_recursion() {
    let mut vm = Interpreter::new(0, &[], 0x10000);
    let mut stack = alloc::vec::Vec::new();
    // Function 7 adds 45. Its literal byte is also the ENDF opcode.
    vm.execute(&[0xb0, 7, 0x2c, 0xb0, 0x2d, 0x60, 0x2d], &mut stack, 8)
        .unwrap();
    vm.execute(&[0xb2, 1, 3, 7, 0x2a, 0xb0, 7, 0x2b], &mut stack, 8)
        .unwrap();
    assert_eq!(stack, [181]);
    vm.execute(&[0xb0, 8, 0x2c, 0xb0, 8, 0x2b, 0x2d], &mut stack, 8)
        .unwrap();
    assert_eq!(
        vm.execute(&[0xb0, 8, 0x2b], &mut stack, 8),
        Err(Error::StackOverflow)
    );
}

#[test]
fn adr0064_i1_axis_vectors_can_be_set_independently() {
    let mut stack = alloc::vec::Vec::new();
    run(
        &[0x00, 0x03, 0x0c, 0x0d, 0x0e, 0x0d, 0x04, 0x0d],
        &mut stack,
        8,
    )
    .unwrap();
    assert_eq!(stack, [16384, 0, 0, 16384, 16384, 0, 0, 16384]);
}

#[test]
fn adr0064_i1_getinfo_reports_only_requested_interpreter_features() {
    let mut vm = Interpreter::new(0, &[], 0x10000);
    vm.engine = EngineInfo {
        version: 40,
        target: HintTarget::VerticalLcd,
        rotated: true,
        stretched: false,
        variation: true,
        symmetric_smoothing: true,
        tricky: false,
    };
    for (selector, expected) in [
        (1, 40),
        (2, 256),
        (4, 0),
        (8, 1024),
        (32, 0),
        (64, 8192),
        (256, 32768),
        (1024, 131072),
        (2048, 262144),
        (4096, 0),
    ] {
        let mut stack = alloc::vec![selector];
        vm.execute(&[0x88], &mut stack, 1).unwrap();
        assert_eq!(stack, [expected]);
    }
    vm.engine.version = 35;
    assert_eq!(vm.engine.query(32 | 64 | 4096), 4096);
    vm.engine.target = HintTarget::Mono;
    assert_eq!(vm.engine.query(32 | 64 | 4096), 0);
}

#[test]
fn adr0064_i1_projected_ppem_and_point_size_use_driver_metrics() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.ppem = [12, 18];
    vm.point_size = 9 * 64;
    let mut stack = alloc::vec::Vec::new();
    vm.execute(&[0x4b, 0x00, 0x4b, 0x4c], &mut stack, 4)
        .unwrap();
    vm.engine.version = 35;
    vm.execute(&[0x4c], &mut stack, 4).unwrap();
    assert_eq!(stack, [12, 18, 576, 18]);
}

#[test]
fn adr0064_i1_scan_control_prioritizes_disable_and_remembers_unmatched_settings() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.ppem = [12, 18];
    vm.engine.rotated = true;
    for (selector, expected) in [
        (0x111, false),
        (0x112, true),
        (1, true),
        (0x1201, false),
        (0x20ff, true),
        (0x200, false),
    ] {
        vm.execute(&[0x85], &mut alloc::vec![selector], 1).unwrap();
        assert_eq!(vm.scan_control, expected);
    }
    for value in [0x10005, -1] {
        vm.execute(&[0x8d], &mut alloc::vec![value], 1).unwrap();
        assert_eq!(vm.scan_type, 5);
    }
}

#[test]
fn adr0064_i1_zone_selection_and_cutins_preserve_instruction_units() {
    let mut vm = Interpreter::new(0, &[], 32768);
    let mut stack = alloc::vec::Vec::new();
    vm.execute(
        &[0xb0, 0, 0x16, 0xb0, 1, 0x14, 0xb0, 2, 0x16],
        &mut stack,
        1,
    )
    .unwrap();
    assert_eq!(vm.zones, [0, 1, 0]);
    vm.execute(
        &[0xb0, 17, 0x1d, 0xb0, 19, 0x1e, 0xb0, 21, 0x1f],
        &mut stack,
        1,
    )
    .unwrap();
    assert_eq!(
        (
            vm.control_value_cutin,
            vm.single_width_cutin,
            vm.single_width
        ),
        (17, 19, 11)
    );
}

#[test]
fn adr0064_i1_delta_control_values_obey_ppem_bands_and_signed_steps() {
    for (op, base) in [(0x73, 10), (0x74, 26), (0x75, 42)] {
        for (ppem, expected) in [(base - 1, 0), (base, 4), (base + 15, -4), (base + 16, 0)] {
            let mut vm = Interpreter::new(0, &[0], 65536);
            vm.ppem = [ppem; 2];
            let mut stack = alloc::vec::Vec::new();
            vm.execute(
                &[0xb0, 10, 0x5e, 0xb0, 4, 0x5f, 0xb4, 8, 0, 0xf7, 0, 2, op],
                &mut stack,
                5,
            )
            .unwrap();
            assert!(stack.is_empty());
            assert_eq!(vm.cvt, [expected]);
            assert!(vm.execute(&[0xb0, 7, 0x5f], &mut stack, 5).is_err());
        }
    }
}

#[test]
fn adr0064_i1_miap_sets_original_twilight_coordinates_and_reference_points() {
    let mut vm = Interpreter::new(0, &[70 * 64], 65536);
    vm.points[0].resize(2, HintPoint::default());
    vm.execute(
        &[0xb0, 0, 0x16, 0xb1, 1, 0, 0x3f],
        &mut alloc::vec::Vec::new(),
        2,
    )
    .unwrap();
    assert_eq!(vm.points[0][1].original, [70, 0]);
    assert_eq!(vm.points[0][1].current, [64, 0]);
    assert_eq!(vm.points[0][1].touched, [true, false]);
    assert_eq!(vm.reference, [1, 1, 0]);
}

#[test]
fn adr0064_i1_reference_point_registers_take_unsigned_short_indices() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.execute(&[0x10, 0x11, 0x12], &mut alloc::vec![-1, 7, 9], 3)
        .unwrap();
    assert_eq!(vm.reference, [9, 7, 65535]);
}

#[test]
fn adr0064_i1_ip_interpolates_all_selected_twilight_points() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.zones = [0; 3];
    vm.reference = [0, 0, 1];
    for (original, current) in [(0, 10), (100, 210), (25, 25), (75, 75)] {
        vm.points[0].push(HintPoint {
            original: [original, 0],
            current: [current, 0],
            ..Default::default()
        });
    }
    vm.execute(&[0xb2, 2, 3, 2, 0x17, 0x39], &mut alloc::vec::Vec::new(), 3)
        .unwrap();
    assert_eq!(vm.points[0][2].current, [60, 0]);
    assert_eq!(vm.points[0][3].current, [160, 0]);
    assert_eq!(vm.loop_count, 1);
}

#[test]
fn adr0064_i1_gc_reads_original_and_current_scfs_updates_twilight() {
    for zone in [0, 1] {
        let mut vm = Interpreter::new(0, &[], 65536);
        vm.zones[2] = zone;
        vm.points[usize::from(zone)].push(HintPoint {
            original: [13, 7],
            current: [25, 8],
            ..Default::default()
        });
        let mut stack = alloc::vec::Vec::new();
        vm.execute(
            &[
                0xb0, 0, 0x47, 0xb0, 0, 0x46, 0xb1, 0, 11, 0x48, 0xb0, 0, 0x47, 0xb0, 0, 0x46,
            ],
            &mut stack,
            4,
        )
        .unwrap();
        assert_eq!(stack, [13, 25, if zone == 0 { 11 } else { 13 }, 11]);
    }
}

#[test]
fn adr0064_i1_mdap_rounds_only_when_requested_and_sets_references() {
    for (opcode, expected) in [(0x2e, 37), (0x2f, 64)] {
        let mut vm = Interpreter::new(0, &[], 65536);
        vm.points[1].resize(
            2,
            HintPoint {
                current: [37, 9],
                ..Default::default()
            },
        );
        vm.execute(&[0xb0, 1, opcode], &mut alloc::vec::Vec::new(), 1)
            .unwrap();
        assert_eq!(vm.points[1][1].current, [expected, 9]);
        assert_eq!(vm.points[1][1].touched, [true, false]);
        assert_eq!(vm.reference, [1, 1, 0]);
    }
}

#[test]
fn adr0064_i1_deltap_selects_zone_point_and_obeys_compatibility() {
    for (op, ppem) in [(0x5d, 9), (0x71, 25), (0x72, 41)] {
        let mut vm = Interpreter::new(0, &[], 65536);
        vm.points[1].resize(1, HintPoint::default());
        vm.ppem = [ppem; 2];
        for (compat, composite, expected) in [
            (1, false, 0),
            (1, true, 8),
            (1, false, 16),
            (7, true, 16),
            (0, false, 24),
        ] {
            vm.backward_compatibility = compat;
            vm.composite = composite;
            vm.execute(&[0, 0xb2, 8, 0, 1, op], &mut alloc::vec::Vec::new(), 3)
                .unwrap();
            assert_eq!(vm.points[1][0].current, [0, expected]);
        }
    }
}

#[test]
fn adr0064_i1_mdrp_measures_before_scaling_and_updates_references() {
    for (opcode, expected, rp0) in [(0xc0, 18, 0), (0xc4, 10, 0), (0xd8, 74, 1)] {
        let mut vm = Interpreter::new(0, &[], 32768);
        vm.points[1].push(HintPoint {
            unscaled: [1, 0],
            current: [10, 0],
            ..Default::default()
        });
        vm.points[1].push(HintPoint {
            unscaled: [16, 0],
            current: [100, 0],
            ..Default::default()
        });
        vm.execute(&[0xb0, 1, opcode], &mut alloc::vec::Vec::new(), 1)
            .unwrap();
        assert_eq!(vm.points[1][1].current, [expected, 0]);
        assert_eq!(vm.reference, [rp0, 0, 1]);
    }
}

#[test]
fn adr0064_i1_glyph_starts_with_default_vectors_but_keeps_prepared_cvt() {
    let mut vm = Interpreter::new(0, &[0], 65536);
    vm.engine.target = HintTarget::Mono;
    vm.execute(
        &[0, 0x7a, 0xb1, 0, 38, 0x44, 0xb0, 0, 0x16],
        &mut alloc::vec::Vec::new(),
        2,
    )
    .unwrap();
    let points = alloc::vec![HintPoint::default()];
    let (result, _, _) = vm
        .execute_glyph(points, &[], &[0xb1, 0, 0, 0x3f, 0xb1, 0, 17, 0x44], 2)
        .unwrap();
    assert_eq!(result[0].current, [64, 0]);
    assert_eq!(vm.cvt, [38]);
}

#[test]
fn adr0064_i1_mirp_flips_control_distance_and_observes_cut_in() {
    for (op, flip, index, expected) in [
        (0xe0, 0x4d, 0, -80),
        (0xe0, 0x4e, 0, 80),
        (0xe4, 0x4d, 0, -64),
        (0xe0, 0x4d, -1, 0),
    ] {
        let mut vm = Interpreter::new(0, &[80 * 64], 65536);
        vm.points[1].push(HintPoint::default());
        vm.points[1].push(HintPoint {
            original: [-50, 0],
            ..Default::default()
        });
        vm.execute(&[flip, op], &mut alloc::vec![1, index], 2)
            .unwrap();
        assert_eq!(vm.points[1][1].current, [expected, 0]);
    }
}

#[test]
fn adr0064_i1_iup_wraps_each_contour_and_excludes_phantoms() {
    let vm = Interpreter::new(0, &[], 65536);
    let points = [
        (25, 25, false),
        (0, 10, true),
        (50, 50, false),
        (100, 210, true),
        (120, 120, false),
        (0, 7, true),
        (20, 20, false),
        (18, 18, false),
    ]
    .map(|(x, c, t)| HintPoint {
        original: [x, 0],
        unscaled: [x, 0],
        current: [c, 0],
        touched: [t, false],
        ..Default::default()
    });
    let (out, _, _) = vm
        .execute_glyph(points.to_vec(), &[4, 6], &[0x31], 0)
        .unwrap();
    assert_eq!(
        out.iter()
            .map(|p| p.current[0])
            .collect::<alloc::vec::Vec<_>>(),
        [60, 10, 110, 210, 230, 7, 27, 18]
    );
}

#[test]
fn adr0064_i1_rounds_phantom_coordinates_before_glyph_program() {
    let vm = Interpreter::new(0, &[], 65536);
    let points = alloc::vec![HintPoint{current:[95,-33],original:[95,-33],..Default::default()};5];
    let (result, _, _) = vm.execute_glyph(points, &[0], &[], 0).unwrap();
    assert_eq!(
        result
            .iter()
            .map(|p| p.current)
            .collect::<alloc::vec::Vec<_>>(),
        [[95, -33], [64, -33], [64, -33], [95, -64], [95, -64]]
    );
    assert!(result.iter().all(|p| p.original == [95, -33]));
}

#[test]
fn adr0064_i1_prep_retains_font_functions_and_reinitializes_storage() {
    use sofel_font::read::maxp::TrueTypeMaxima;
    let maxima = TrueTypeMaxima {
        max_storage: 1,
        max_twilight_points: 2,
        ..Default::default()
    };
    let engine = Interpreter::new(0, &[], 0).engine;
    let (vm, limit) = Interpreter::prepare_programs(
        maxima,
        &[4 * 64],
        16,
        65536,
        engine,
        &[],
        (
            &[0xb1, 0, 99, 0x42, 0xb0, 1, 0x2c, 0xb0, 4, 0x60, 0x2d],
            &[0xb1, 0, 0, 0x43, 0xb0, 1, 0x2b, 0x44],
        ),
    )
    .unwrap();
    assert_eq!(vm.cvt, [4]);
    assert_eq!(vm.storage, [0]);
    assert_eq!(vm.points[0].len(), 6);
    assert_eq!(limit, 128);
}

#[test]
fn adr0064_i1_gc_projects_both_coordinates_for_diagonal_vectors() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.points[1].push(HintPoint {
        current: [64, 128],
        original: [-64, -128],
        ..HintPoint::default()
    });
    let mut stack = alloc::vec![3, 4];
    vm.execute(&[0x0a, 0xb0, 0, 0x46, 0xb0, 0, 0x47], &mut stack, 2)
        .unwrap();
    assert_eq!(stack, [141, -141]);
}

#[test]
fn adr0064_i1_scfs_moves_along_freedom_and_marks_each_moved_axis() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.points[1].push(HintPoint::default());
    for (projection, expected, touched) in [
        ([9830, 13107], [60, 80], [true, true]),
        ([16384, 0], [100, 133], [true, true]),
        ([-13107, 9830], [0, 0], [false, false]),
    ] {
        vm.points[1][0] = HintPoint::default();
        vm.projection = projection;
        vm.freedom = [9830, 13107];
        vm.execute(&[0x48], &mut alloc::vec![0, 100], 2).unwrap();
        assert_eq!(vm.points[1][0].current, expected);
        assert_eq!(vm.points[1][0].touched, touched);
    }
}

#[test]
fn adr0064_i1_v40_glyph_waiver_rounds_horizontal_on_the_fine_grid() {
    let mut vm = Interpreter::new(0, &[], 65536);
    // Select x, waive compatibility in this glyph, then ROUND 38 and SCFS point 0.
    let points = alloc::vec![HintPoint::default();5];
    let (result, _, _) = vm
        .execute_glyph(
            points,
            &[0],
            &[1, 0xb1, 4, 3, 0x8e, 0xb1, 0, 38, 0x68, 0x48],
            4,
        )
        .unwrap();
    assert_eq!(result[0].current, [38, 0]);
    vm.engine.target = HintTarget::Mono;
    let (mono, _, _) = vm
        .execute_glyph(
            alloc::vec![HintPoint::default();5],
            &[0],
            &[1, 0xb1, 0, 38, 0x68, 0x48],
            4,
        )
        .unwrap();
    assert_eq!(mono[0].current, [64, 0]);
}

#[test]
fn adr0064_i1_md_uses_current_original_or_unscaled_points_and_selected_zones() {
    let mut vm = Interpreter::new(0, &[], 131072);
    let source = HintPoint {
        current: [10, 20],
        original: [30, 40],
        unscaled: [50, 60],
        ..HintPoint::default()
    };
    vm.points[0].push(source);
    vm.points[1].extend([source, HintPoint::default()]);
    vm.projection = [0, 16384];
    vm.dual_projection = [-16384, 0];
    for (op, zone, index, expected) in [
        (0x49, 1, 1, 20),
        (0x4a, 1, 1, -100),
        (0x4a, 0, 1, -30),
        (0x49, 1, 2, 0),
    ] {
        vm.zones[0] = zone;
        let mut stack = alloc::vec![0, index];
        vm.execute(&[op], &mut stack, 2).unwrap();
        assert_eq!(stack, [expected]);
    }
}

#[test]
fn adr0064_i1_line_vectors_use_zones_rotation_and_original_positions() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.zones = [1, 0, 1];
    vm.points[0].push(HintPoint {
        current: [3, 4],
        original: [0, 4],
        ..HintPoint::default()
    });
    vm.points[1].push(HintPoint::default());
    for (op, expected) in [
        (0x06, [9830, 13107]),
        (0x07, [-13107, 9830]),
        (0x08, [9830, 13107]),
        (0x09, [-13107, 9830]),
        (0x86, [9830, 13107]),
        (0x87, [-13107, 9830]),
    ] {
        vm.execute(&[op], &mut alloc::vec![0, 0], 2).unwrap();
        if matches!(op, 0x08 | 0x09) {
            assert_eq!(vm.freedom, expected);
        } else {
            assert_eq!(vm.projection, expected);
            assert_eq!(
                vm.dual_projection,
                match op {
                    0x86 => [0, 16384],
                    0x87 => [-16384, 0],
                    _ => expected,
                }
            );
        }
    }
    vm.points[0][0].original = [0, 0];
    vm.execute(&[0x87], &mut alloc::vec![0, 0], 2).unwrap();
    assert_eq!(vm.dual_projection, [16384, 0]);
    assert_eq!(vm.projection, [9830, 13107]);
}

#[test]
fn adr0064_i1_alignpts_and_alignrp_move_selected_points_and_reset_loop() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.zones = [0, 1, 1];
    vm.points[0].push(HintPoint {
        current: [15, 5],
        ..HintPoint::default()
    });
    vm.points[1].resize(3, HintPoint::default());
    vm.execute(&[0x27], &mut alloc::vec![0, 0], 2).unwrap();
    assert_eq!(vm.points[0][0].current, [8, 5]);
    assert_eq!(vm.points[1][0].current, [7, 0]);
    vm.loop_count = 3;
    let mut stack = alloc::vec![77, 1, 99, 2];
    vm.execute(&[0x3c], &mut stack, 4).unwrap();
    assert_eq!(vm.loop_count, 1);
    assert_eq!(stack, [77]);
    for point in &vm.points[1][1..] {
        assert_eq!(point.current, [8, 0]);
        assert_eq!(point.touched, [true, false]);
    }
}

#[test]
fn adr0064_i1_ip_projects_original_and_current_ranges_before_moving() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.points[1].extend([
        HintPoint::default(),
        HintPoint {
            unscaled: [0, 100],
            current: [60, 80],
            ..HintPoint::default()
        },
        HintPoint {
            unscaled: [0, 50],
            current: [0, 0],
            ..HintPoint::default()
        },
    ]);
    vm.reference = [0, 0, 1];
    vm.dual_projection = [0, 16384];
    vm.projection = [9830, 13107];
    vm.freedom = vm.projection;
    vm.execute(&[0x39], &mut alloc::vec![2], 1).unwrap();
    assert_eq!(vm.points[1][2].current, [30, 40]);
    assert_eq!(vm.points[1][2].touched, [true, true]);
}

#[test]
fn adr0064_i1_mdap_and_msirp_move_in_the_projected_direction() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.projection = [9830, 13107];
    vm.freedom = vm.projection;
    vm.points[1].push(HintPoint {
        current: [30, 40],
        ..HintPoint::default()
    });
    vm.execute(&[0x2f], &mut alloc::vec![0], 1).unwrap();
    assert_eq!(vm.points[1][0].current, [38, 51]);
    assert_eq!(vm.points[1][0].touched, [true, true]);
    vm.points[0].push(HintPoint::default());
    vm.zones[1] = 0;
    vm.execute(&[0x3b], &mut alloc::vec![0, 100], 2).unwrap();
    assert_eq!(vm.points[0][0].original, [60, 80]);
    assert_eq!(vm.points[0][0].current, [98, 131]);
    assert_eq!(vm.points[0][0].touched, [true, true]);
}

#[test]
fn adr0064_i1_zone_shift_excludes_reference_and_phantoms_and_preserves_tags() {
    let mut vm = Interpreter::new(0, &[], 65536);
    vm.points[1].resize(7, HintPoint::default());
    vm.points[0].resize(2, HintPoint::default());
    vm.points[1][1].current = [30, 40];
    vm.reference[2] = 1;
    vm.projection = [9830, 13107];
    vm.freedom = vm.projection;
    vm.execute(&[0x36], &mut alloc::vec![1], 1).unwrap();
    for (i, p) in vm.points[1].iter().enumerate() {
        assert_eq!(p.current, if i < 3 { [30, 40] } else { [0, 0] });
        assert_eq!(p.touched, [false, false]);
    }
    vm.reference[1] = 1;
    vm.execute(&[0x37], &mut alloc::vec![0], 1).unwrap();
    assert!(
        vm.points[0]
            .iter()
            .all(|p| p.current == [30, 40] && p.touched == [false, false])
    );
}

#[test]
fn adr0064_i1_shp_and_shc_shift_both_axes_with_diagonal_freedom() {
    for op in [0x32, 0x33, 0x34, 0x35] {
        let mut vm = Interpreter::new(0, &[], 65536);
        vm.points[1].resize(3, HintPoint::default());
        vm.points[1][0].current = [30, 40];
        vm.contour_ends.push(2);
        vm.projection = [9830, 13107];
        vm.freedom = vm.projection;
        vm.execute(&[op], &mut alloc::vec![if op < 0x34 { 1 } else { 0 }], 1)
            .unwrap();
        assert_eq!(vm.points[1][1].current, [30, 40]);
        assert_eq!(vm.points[1][1].touched, [true, true]);
        assert_eq!(vm.points[1][0].touched, [false, false]);
        assert_eq!(vm.points[1][2].touched, [op >= 0x34; 2]);
    }
}

#[test]
fn adr0064_i1_shpix_uses_freedom_and_consumes_loop_indices() {
    for (compat, twilight, touched, expected) in [
        (0, false, false, [-60, -80]),
        (4, false, false, [0, 0]),
        (4, false, true, [0, -80]),
        (4, true, false, [0, -80]),
        (7, true, false, [0, 0]),
    ] {
        let mut vm = Interpreter::new(0, &[], 65536);
        vm.points[1].resize(2, HintPoint::default());
        vm.points[1][1].touched[1] = touched;
        vm.zones[0] = u8::from(!twilight);
        vm.backward_compatibility = compat;
        vm.freedom = [9830, 13107];
        vm.loop_count = 2;
        let mut stack = alloc::vec![77, 1, 99, -100];
        vm.execute(&[0x38], &mut stack, 4).unwrap();
        assert_eq!(vm.points[1][1].current, expected);
        assert_eq!(vm.loop_count, 1);
        assert_eq!(stack, [77]);
    }
}

#[test]
fn adr0064_i1_getvariation_pushes_all_axes_within_the_stack_limit() {
    let mut vm = Interpreter::new(0, &[], 65536);
    assert_eq!(
        vm.execute(&[0x91], &mut alloc::vec![], 4),
        Err(Error::InvalidOpcode)
    );
    vm.blend_coords = alloc::vec![-16384, 0, 8192];
    let mut stack = alloc::vec![7];
    assert_eq!(
        vm.execute(&[0x91], &mut stack, 3),
        Err(Error::StackOverflow)
    );
    assert_eq!(stack, [7]);
    vm.execute(&[0x91], &mut stack, 4).unwrap();
    assert_eq!(stack, [7, -16384, 0, 8192]);
}

#[test]
fn adr0064_i1_static_idef_can_be_redefined_in_prep_within_maxp_limit() {
    let maxima = sofel_font::read::maxp::TrueTypeMaxima {
        max_instruction_defs: 1,
        ..Default::default()
    };
    let engine = Interpreter::new(0, &[], 65536).engine;
    let definition = [0xb0, 0x91, 0x89, 0xb0, 2, 0x2d];
    let redefine = [0xb0, 0x91, 0x89, 0xb0, 3, 0x2d];
    let (mut vm, limit) = Interpreter::prepare_programs(
        maxima,
        &[],
        16,
        65536,
        engine,
        &[],
        (&definition, &redefine),
    )
    .unwrap();
    let mut stack = alloc::vec![];
    vm.execute(&[0x91], &mut stack, limit).unwrap();
    assert_eq!(stack, [3]);
    assert!(
        vm.execute_glyph(alloc::vec![], &[], &definition, limit)
            .is_err()
    );
    assert!(
        Interpreter::prepare_programs(
            maxima,
            &[],
            16,
            65536,
            engine,
            &[],
            (&definition, &[0xb0, 0x92, 0x89, 0x2d])
        )
        .is_err()
    );
}

#[test]
fn adr0064_i1_relative_jumps_count_from_opcode_and_obey_execution_limit() {
    for (op, flag, expected) in [
        (0x78, 1, alloc::vec![9]),
        (0x78, 0, alloc::vec![5, 9]),
        (0x79, 0, alloc::vec![9]),
        (0x79, 1, alloc::vec![5, 9]),
    ] {
        let mut values = alloc::vec![];
        run(&[0xb1, 3, flag, op, 0xb0, 5, 0xb0, 9], &mut values, 3).unwrap();
        assert_eq!(values, expected);
    }
    let mut vm = Interpreter::new(0, &[], 65536);
    let mut values = alloc::vec![];
    let mut budget = 7;
    assert_eq!(
        vm.execute_frame(&[0xb8, 0xff, 0xfd, 0x1c], &mut values, 1, 0, &mut budget),
        Err(Error::ExecutionLimit)
    );
    assert_eq!(budget, 0);
    assert!(run(&[0xb0, 0, 0x1c], &mut values, 1).is_err());
}

#[test]
fn adr0064_i1_legacy_angle_instructions_consume_one_operand() {
    let mut vm = Interpreter::new(0, &[], 65536);
    for opcode in [0x7e, 0x7f] {
        let mut stack = alloc::vec![13, 42];
        vm.execute(&[opcode], &mut stack, 2).unwrap();
        assert_eq!(stack, [13]);
        assert_eq!(
            vm.execute(&[opcode], &mut alloc::vec![], 2),
            Err(Error::StackUnderflow)
        );
    }
}
