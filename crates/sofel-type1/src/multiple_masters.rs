//! Type 1 Multiple Masters axis metadata and design maps.

use alloc::vec::Vec;
use core::fmt;

/// Invalid or incomplete Multiple Masters dictionaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MmError;

impl fmt::Display for MmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Type 1 Multiple Masters data")
    }
}

impl core::error::Error for MmError {}

#[derive(Clone, Debug)]
enum Value<'a> {
    Number(i32),
    Name(&'a [u8]),
    Array(Vec<Value<'a>>),
}

/// One design axis, with coordinates in signed 16.16 fixed point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Axis<'a> {
    name: &'a [u8],
    map: Vec<(i32, i32)>,
    default: i32,
}

impl Axis<'_> {
    /// PostScript axis name.
    pub fn name(&self) -> &[u8] {
        self.name
    }
    /// Minimum design coordinate.
    pub fn minimum(&self) -> i32 {
        self.map.first().map_or(0, |point| point.0)
    }
    /// Maximum design coordinate.
    pub fn maximum(&self) -> i32 {
        self.map.last().map_or(0, |point| point.0)
    }
    /// Default design coordinate.
    pub fn default(&self) -> i32 {
        self.default
    }

    fn blend(&self, design: i32) -> Result<i32, MmError> {
        let design = design.clamp(self.minimum(), self.maximum());
        for pair in self.map.windows(2) {
            let (&left, &right) = (pair.first().ok_or(MmError)?, pair.get(1).ok_or(MmError)?);
            if design <= right.0 {
                return interpolate(design, left.0, right.0, left.1, right.1);
            }
        }
        self.map.last().map(|point| point.1).ok_or(MmError)
    }
}

/// Validated axis maps and default master weights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipleMasters<'a> {
    axes: Vec<Axis<'a>>,
    default_weights: Vec<i32>,
}

impl<'a> MultipleMasters<'a> {
    /// Parse MM dictionaries in the cleartext Type 1 program. A non-MM font returns `None`.
    pub fn parse(data: &'a [u8]) -> Result<Option<Self>, MmError> {
        let types = field(data, b"BlendAxisTypes")?;
        let positions = field(data, b"BlendDesignPositions")?;
        let maps = field(data, b"BlendDesignMap")?;
        let weights = field(data, b"WeightVector")?;
        if types.is_none() && positions.is_none() && maps.is_none() && weights.is_none() {
            return Ok(None);
        }
        let types = types.ok_or(MmError)?;
        let positions = positions.ok_or(MmError)?;
        let maps = maps.ok_or(MmError)?;
        let weights = weights.ok_or(MmError)?;
        let names = array(&types)?;
        let designs = array(&positions)?;
        let maps = array(&maps)?;
        let weights = array(&weights)?;
        let count = names.len();
        if !(1..=4).contains(&count)
            || maps.len() != count
            || designs.len() != 1 << count
            || weights.len() != designs.len()
        {
            return Err(MmError);
        }
        for design in designs {
            let positions = array(design)?;
            if positions.len() != count {
                return Err(MmError);
            }
            for position in positions {
                number(position)?;
            }
        }
        let mut axes = Vec::with_capacity(count);
        let mut default_weights = Vec::with_capacity(weights.len());
        for weight in weights {
            default_weights.push(number(weight)?);
        }
        for (index, (name, map)) in names.iter().zip(maps).enumerate() {
            let name = name_value(name)?;
            let points = array(map)?;
            if points.len() < 2 {
                return Err(MmError);
            }
            let mut entries = Vec::with_capacity(points.len());
            for point in points {
                let pair = array(point)?;
                if pair.len() != 2 {
                    return Err(MmError);
                }
                entries.push((
                    number(pair.first().ok_or(MmError)?)?,
                    number(pair.get(1).ok_or(MmError)?)?,
                ));
            }
            if entries.windows(2).any(|p| match (p.first(), p.get(1)) {
                (Some(left), Some(right)) => left.0 >= right.0 || left.1 >= right.1,
                _ => true,
            }) {
                return Err(MmError);
            }
            let normalized = default_weights
                .iter()
                .enumerate()
                .filter(|(master, _)| master & (1 << index) != 0)
                .try_fold(0i32, |sum, (_, weight)| {
                    sum.checked_add(*weight).ok_or(MmError)
                })?;
            let unmapped = unmap(&entries, normalized)?;
            // FreeType's Type 1 design map stores integral design points.
            let default = (i64::from(unmapped).checked_add(32768).ok_or(MmError)? / 65536)
                .checked_mul(65536)
                .and_then(|n| i32::try_from(n).ok())
                .ok_or(MmError)?;
            axes.push(Axis {
                name,
                map: entries,
                default,
            });
        }
        Ok(Some(Self {
            axes,
            default_weights,
        }))
    }

    /// Number of design axes.
    pub fn axis_count(&self) -> usize {
        self.axes.len()
    }
    /// An axis by index.
    pub fn axis(&self, index: usize) -> Option<&Axis<'a>> {
        self.axes.get(index)
    }
    /// Default master weights in signed 16.16 fixed point.
    pub fn default_weights(&self) -> &[i32] {
        &self.default_weights
    }
    /// Convert design coordinates to normalized blend coordinates.
    pub fn blend_coordinates(&self, design: &[i32]) -> Result<Vec<i32>, MmError> {
        if design.len() != self.axes.len() {
            return Err(MmError);
        }
        self.axes
            .iter()
            .zip(design)
            .map(|(axis, coordinate)| axis.blend(*coordinate))
            .collect()
    }

    /// Interpolation weights in Type 1 master order for design coordinates.
    pub fn master_weights(&self, design: &[i32]) -> Result<Vec<i32>, MmError> {
        let blends = self.blend_coordinates(design)?;
        let mut weights = Vec::with_capacity(self.default_weights.len());
        for master in 0..self.default_weights.len() {
            let mut weight = 1 << 16;
            for (axis, blend) in blends.iter().enumerate() {
                let factor = if master & (1 << axis) == 0 {
                    (1i32 << 16).checked_sub(*blend).ok_or(MmError)?
                } else {
                    *blend
                };
                if factor <= 0 {
                    weight = 0;
                    break;
                }
                if factor >= 1 << 16 {
                    continue;
                }
                weight = i32::try_from(
                    i64::from(weight)
                        .checked_mul(i64::from(factor))
                        .and_then(|n| n.checked_add(32768))
                        .ok_or(MmError)?
                        >> 16,
                )
                .map_err(|_| MmError)?;
            }
            weights.push(weight);
        }
        Ok(weights)
    }
}

fn array<'a, 'b>(value: &'b Value<'a>) -> Result<&'b [Value<'a>], MmError> {
    match value {
        Value::Array(values) => Ok(values),
        _ => Err(MmError),
    }
}

fn number(value: &Value<'_>) -> Result<i32, MmError> {
    match value {
        Value::Number(value) => Ok(*value),
        _ => Err(MmError),
    }
}

fn name_value<'a>(value: &Value<'a>) -> Result<&'a [u8], MmError> {
    match value {
        Value::Name(name) => Ok(name),
        _ => Err(MmError),
    }
}

fn interpolate(x: i32, x0: i32, x1: i32, y0: i32, y1: i32) -> Result<i32, MmError> {
    let numerator = i64::from(x)
        .checked_sub(i64::from(x0))
        .and_then(|n| n.checked_mul(i64::from(y1).checked_sub(i64::from(y0))?))
        .ok_or(MmError)?;
    let denominator = i64::from(x1).checked_sub(i64::from(x0)).ok_or(MmError)?;
    let delta = numerator
        .checked_add(denominator / 2)
        .ok_or(MmError)?
        .checked_div(denominator)
        .ok_or(MmError)?;
    i32::try_from(i64::from(y0).checked_add(delta).ok_or(MmError)?).map_err(|_| MmError)
}

fn unmap(map: &[(i32, i32)], blend: i32) -> Result<i32, MmError> {
    let blend = blend.clamp(map.first().ok_or(MmError)?.1, map.last().ok_or(MmError)?.1);
    for pair in map.windows(2) {
        let (&left, &right) = (pair.first().ok_or(MmError)?, pair.get(1).ok_or(MmError)?);
        if blend <= right.1 {
            return interpolate(blend, left.1, right.1, left.0, right.0);
        }
    }
    map.last().map(|point| point.0).ok_or(MmError)
}

fn field<'a>(data: &'a [u8], key: &[u8]) -> Result<Option<Value<'a>>, MmError> {
    let mut at = 0;
    while let Some(token) = next(data, &mut at)? {
        if token.strip_prefix(b"/") == Some(key) {
            return value(data, &mut at, 0).map(Some);
        }
    }
    Ok(None)
}

fn value<'a>(data: &'a [u8], at: &mut usize, depth: usize) -> Result<Value<'a>, MmError> {
    if depth > 8 {
        return Err(MmError);
    }
    let token = next(data, at)?.ok_or(MmError)?;
    if token == b"[" {
        let mut values = Vec::new();
        loop {
            let saved = *at;
            let item = next(data, at)?.ok_or(MmError)?;
            if item == b"]" {
                break;
            }
            *at = saved;
            if values.len() >= 256 {
                return Err(MmError);
            }
            values.push(value(data, at, depth.checked_add(1).ok_or(MmError)?)?);
        }
        Ok(Value::Array(values))
    } else if let Some(name) = token.strip_prefix(b"/") {
        Ok(Value::Name(name))
    } else {
        Ok(Value::Number(fixed(token)?))
    }
}

fn next<'a>(data: &'a [u8], at: &mut usize) -> Result<Option<&'a [u8]>, MmError> {
    while let Some(&byte) = data.get(*at) {
        if byte == b'%' {
            while data.get(*at).is_some_and(|b| *b != b'\n') {
                *at = at.checked_add(1).ok_or(MmError)?;
            }
        } else if byte.is_ascii_whitespace() {
            *at = at.checked_add(1).ok_or(MmError)?;
        } else {
            break;
        }
    }
    let Some(&byte) = data.get(*at) else {
        return Ok(None);
    };
    let start = *at;
    if byte == b'[' || byte == b']' {
        *at = at.checked_add(1).ok_or(MmError)?;
    } else {
        while let Some(&byte) = data.get(*at) {
            if byte.is_ascii_whitespace() || matches!(byte, b'[' | b']' | b'%') {
                break;
            }
            *at = at.checked_add(1).ok_or(MmError)?;
        }
    }
    if *at == start {
        return Err(MmError);
    }
    Ok(Some(data.get(start..*at).ok_or(MmError)?))
}

fn fixed(token: &[u8]) -> Result<i32, MmError> {
    let text = core::str::from_utf8(token).map_err(|_| MmError)?;
    let negative = text.starts_with('-');
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let integer = integer.parse::<i64>().map_err(|_| MmError)?;
    if fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MmError);
    }
    let scale = 10i64.pow(u32::try_from(fraction.len()).map_err(|_| MmError)?);
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().map_err(|_| MmError)?
    };
    let raw = integer
        .checked_mul(65536)
        .and_then(|n| {
            fraction
                .checked_mul(65536)
                .and_then(|f| n.checked_add(f.checked_add(scale / 2)?.checked_div(scale)?))
        })
        .ok_or(MmError)?;
    i32::try_from(if negative {
        raw.checked_neg().ok_or(MmError)?
    } else {
        raw
    })
    .map_err(|_| MmError)
}
