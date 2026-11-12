//! SVG numbers and affine transforms, in SVG matrix order.
use super::IDENTITY;

pub(crate) fn whitespace(s: &str) -> &str {
    s.trim_start_matches([' ', '\t', '\r', '\n'])
}

pub(crate) fn separator(s: &str) -> &str {
    let s = whitespace(s);
    s.strip_prefix(',').map_or(s, whitespace)
}

// A sign can begin the next number without whitespace. A sign immediately
// after an exponent marker belongs to this number instead.
pub(crate) fn take_number(input: &mut &str) -> Option<f32> {
    let s = *input;
    let mut rest = s.strip_prefix(['-', '+']).unwrap_or(s);
    let integer = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    let has_integer = integer.len() != rest.len();
    rest = integer;
    if let Some(after_dot) = rest.strip_prefix('.') {
        rest = after_dot.trim_start_matches(|c: char| c.is_ascii_digit());
        if !has_integer && rest.len() == after_dot.len() {
            return None;
        }
    } else if !has_integer {
        return None;
    }
    if let Some(exp) = rest.strip_prefix(['e', 'E']) {
        let exp = exp.strip_prefix(['-', '+']).unwrap_or(exp);
        rest = exp.trim_start_matches(|c: char| c.is_ascii_digit());
        if exp.len() == rest.len() {
            return None;
        }
    }
    let n = s
        .get(..s.len().checked_sub(rest.len())?)?
        .parse::<f32>()
        .ok()?;
    if !n.is_finite() {
        return None;
    }
    *input = rest;
    Some(n)
}

pub(crate) fn compose(parent: [f32; 6], local: [f32; 6]) -> [f32; 6] {
    let [a, b, c, d, e, f] = parent;
    let [g, h, i, j, k, l] = local;
    [
        a * g + c * h,
        b * g + d * h,
        a * i + c * j,
        b * i + d * j,
        a * k + c * l + e,
        b * k + d * l + f,
    ]
}

fn rotate(degrees: f32, x: f32, y: f32) -> [f32; 6] {
    let angle = degrees % 360.0;
    let (sin, cos) = match angle {
        0.0 => (0.0, 1.0),
        90.0 | -270.0 => (1.0, 0.0),
        180.0 | -180.0 => (0.0, -1.0),
        270.0 | -90.0 => (-1.0, 0.0),
        _ => sofel_geom::sin_cos(angle.to_radians()),
    };
    [
        cos,
        sin,
        -sin,
        cos,
        x - cos * x + sin * y,
        y - sin * x - cos * y,
    ]
}

fn operation(name: &str, mut args: &str) -> Option<[f32; 6]> {
    let mut values = [0.0; 6];
    let mut filled = 0;
    args = whitespace(args);
    while !args.is_empty() {
        *values.get_mut(filled)? = take_number(&mut args)?;
        filled = filled.checked_add(1)?;
        let next = separator(args);
        if next.is_empty() && whitespace(args).starts_with(',') {
            return None;
        }
        args = next;
    }
    match (name, values.get(..filled)?) {
        ("matrix", [a, b, c, d, e, f]) => Some([*a, *b, *c, *d, *e, *f]),
        ("translate", [x]) => Some([1., 0., 0., 1., *x, 0.]),
        ("translate", [x, y]) => Some([1., 0., 0., 1., *x, *y]),
        ("scale", [x]) => Some([*x, 0., 0., *x, 0., 0.]),
        ("scale", [x, y]) => Some([*x, 0., 0., *y, 0., 0.]),
        ("rotate", [a]) => Some(rotate(*a, 0., 0.)),
        ("rotate", [a, x, y]) => Some(rotate(*a, *x, *y)),
        ("skewX", [a]) => Some([1., 0., sofel_geom::tan((a % 180.).to_radians()), 1., 0., 0.]),
        ("skewY", [a]) => Some([1., sofel_geom::tan((a % 180.).to_radians()), 0., 1., 0., 0.]),
        _ => None,
    }
}

pub(crate) fn transform(value: Option<&str>) -> [f32; 6] {
    let mut text = whitespace(value.unwrap_or(""));
    let mut result = IDENTITY;
    while !text.is_empty() {
        let Some((name, args)) = text.split_once('(') else {
            return IDENTITY;
        };
        let Some((args, rest)) = args.split_once(')') else {
            return IDENTITY;
        };
        let Some(next) = operation(name.trim_end(), args) else {
            return IDENTITY;
        };
        result = compose(result, next);
        if !result.iter().all(|v| v.is_finite()) {
            return IDENTITY;
        }
        text = separator(rest);
    }
    result
}
