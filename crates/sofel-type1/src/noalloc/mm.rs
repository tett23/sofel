//! The Multiple Masters dictionaries of a cleartext Type 1 program.
//!
//! Arrays are not stored: a [`Value::Array`] remembers where its elements
//! start and is read again when needed.

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

#[derive(Clone, Copy, Debug)]
pub(crate) enum Value<'a> {
    Number(i32),
    Name(&'a [u8]),
    Array {
        data: &'a [u8],
        start: usize,
        len: usize,
    },
}

/// The elements of an array value.
pub(crate) fn elements<'a>(
    value: Value<'a>,
) -> Result<impl Iterator<Item = Result<Value<'a>, MmError>>, MmError> {
    let Value::Array { data, start, len } = value else {
        return Err(MmError);
    };
    let mut at = start;
    Ok((0..len).map(move |_| read(data, &mut at, 0)))
}

fn len(value: Value<'_>) -> Result<usize, MmError> {
    match value {
        Value::Array { len, .. } => Ok(len),
        _ => Err(MmError),
    }
}

fn number(value: Value<'_>) -> Result<i32, MmError> {
    match value {
        Value::Number(value) => Ok(value),
        _ => Err(MmError),
    }
}

/// The `(design, blend)` points of one axis map.
pub(crate) fn points(
    map: Value<'_>,
) -> Result<impl Iterator<Item = Result<(i32, i32), MmError>>, MmError> {
    Ok(elements(map)?.map(|point| {
        let mut pair = elements(point?)?;
        let design = number(pair.next().ok_or(MmError)??)?;
        let blend = number(pair.next().ok_or(MmError)??)?;
        Ok((design, blend))
    }))
}

/// One design axis found by [`scan`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct AxisInfo<'a> {
    pub(crate) name: &'a [u8],
    pub(crate) map: Value<'a>,
    pub(crate) default: i32,
}

/// Check the MM dictionaries, passing each default weight and then each
/// axis. Returns whether the program is a Multiple Masters font.
pub(crate) fn scan<'a>(
    data: &'a [u8],
    weight: &mut dyn FnMut(i32),
    axis: &mut dyn FnMut(AxisInfo<'a>),
) -> Result<bool, MmError> {
    let types = field(data, b"BlendAxisTypes")?;
    let positions = field(data, b"BlendDesignPositions")?;
    let maps = field(data, b"BlendDesignMap")?;
    let weights = field(data, b"WeightVector")?;
    if types.is_none() && positions.is_none() && maps.is_none() && weights.is_none() {
        return Ok(false);
    }
    let types = types.ok_or(MmError)?;
    let positions = positions.ok_or(MmError)?;
    let maps = maps.ok_or(MmError)?;
    let weights = weights.ok_or(MmError)?;
    let count = len(types)?;
    let designs = len(positions)?;
    if !(1..=4).contains(&count)
        || len(maps)? != count
        || designs != 1 << count
        || len(weights)? != designs
    {
        return Err(MmError);
    }
    for design in elements(positions)? {
        let design = design?;
        if len(design)? != count {
            return Err(MmError);
        }
        for position in elements(design)? {
            number(position?)?;
        }
    }
    for value in elements(weights)? {
        weight(number(value?)?);
    }
    for (index, (name, map)) in elements(types)?.zip(elements(maps)?).enumerate() {
        let Value::Name(name) = name? else {
            return Err(MmError);
        };
        let map = map?;
        if len(map)? < 2 {
            return Err(MmError);
        }
        let mut previous: Option<(i32, i32)> = None;
        for point in elements(map)? {
            if len(point?)? != 2 {
                return Err(MmError);
            }
        }
        for point in points(map)? {
            let point = point?;
            if previous.is_some_and(|left| left.0 >= point.0 || left.1 >= point.1) {
                return Err(MmError);
            }
            previous = Some(point);
        }
        let mut normalized = 0i32;
        for (master, value) in elements(weights)?.enumerate() {
            if master & (1 << index) != 0 {
                normalized = normalized.checked_add(number(value?)?).ok_or(MmError)?;
            }
        }
        let unmapped = unmap(map, normalized)?;
        // FreeType's Type 1 design map stores integral design points.
        let default = (i64::from(unmapped).checked_add(32768).ok_or(MmError)? / 65536)
            .checked_mul(65536)
            .and_then(|n| i32::try_from(n).ok())
            .ok_or(MmError)?;
        axis(AxisInfo { name, map, default });
    }
    Ok(true)
}

pub(crate) fn interpolate(x: i32, x0: i32, x1: i32, y0: i32, y1: i32) -> Result<i32, MmError> {
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

fn unmap(map: Value<'_>, blend: i32) -> Result<i32, MmError> {
    let first = points(map)?.next().ok_or(MmError)??;
    let last = points(map)?.last().ok_or(MmError)??;
    let blend = blend.clamp(first.1, last.1);
    let mut left: Option<(i32, i32)> = None;
    for right in points(map)? {
        let right = right?;
        if let Some(left) = left
            && blend <= right.1
        {
            return interpolate(blend, left.1, right.1, left.0, right.0);
        }
        left = Some(right);
    }
    Ok(last.0)
}

fn field<'a>(data: &'a [u8], key: &[u8]) -> Result<Option<Value<'a>>, MmError> {
    let mut at = 0;
    while let Some(token) = next(data, &mut at)? {
        if token.strip_prefix(b"/") == Some(key) {
            return read(data, &mut at, 0).map(Some);
        }
    }
    Ok(None)
}

fn read<'a>(data: &'a [u8], at: &mut usize, depth: usize) -> Result<Value<'a>, MmError> {
    if depth > 8 {
        return Err(MmError);
    }
    let token = next(data, at)?.ok_or(MmError)?;
    if token == b"[" {
        let start = *at;
        let mut len = 0usize;
        loop {
            let saved = *at;
            let item = next(data, at)?.ok_or(MmError)?;
            if item == b"]" {
                break;
            }
            *at = saved;
            if len >= 256 {
                return Err(MmError);
            }
            read(data, at, depth.checked_add(1).ok_or(MmError)?)?;
            len = len.checked_add(1).ok_or(MmError)?;
        }
        Ok(Value::Array { data, start, len })
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
    let integer: i64 = integer.parse().map_err(|_| MmError)?;
    if fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MmError);
    }
    let scale = 10i64.pow(u32::try_from(fraction.len()).map_err(|_| MmError)?);
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().map_err(|_| MmError)?
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
