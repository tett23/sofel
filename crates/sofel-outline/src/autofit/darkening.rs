//! FreeType 2.15.0 `af_loader_compute_darkening` and light-mode stem darkening.

use sofel_geom::Point;

use super::StyleMetrics;
use super::calc::{div_fix, mul_div, mul_fix};
use super::hints::{HORZ, VERT};
use crate::hint_properties::PostScriptHintProperties;

fn amount(standard_width: i64, ppem: u16, upem: i64, parameters: [i32; 8]) -> i64 {
    let ppem = i64::from(ppem.max(4)) << 16;
    let em_ratio = div_fix(1000 << 16, upem << 16);
    if em_ratio < 655 {
        return 0;
    }
    let stem_width_per_1000 = if standard_width <= 0 {
        75 << 16
    } else {
        mul_fix(standard_width << 16, em_ratio)
    };
    let bits = |value: i64| 31 - (value as u32).leading_zeros();
    let scaled_stem = if bits(stem_width_per_1000) + bits(ppem) >= 46 {
        i64::from(parameters[6]) << 16
    } else {
        mul_fix(stem_width_per_1000, ppem)
    };
    let points = parameters.as_chunks::<2>().0;
    let mut darken = None;
    for pair in points.windows(2) {
        let [x1, y1] = pair[0];
        let [x2, y2] = pair[1];
        if scaled_stem < i64::from(x1) << 16 {
            darken = Some(div_fix(i64::from(y1) << 16, ppem));
            break;
        }
        if scaled_stem < i64::from(x2) << 16 && x2 != x1 {
            let stem = stem_width_per_1000 - div_fix(i64::from(x1) << 16, ppem);
            darken = Some(
                mul_div(stem, i64::from(y2 - y1), i64::from(x2 - x1))
                    + div_fix(i64::from(y1) << 16, ppem),
            );
            break;
        }
    }
    let last = points[3];
    let darken = darken.unwrap_or_else(|| div_fix(i64::from(last[1]) << 16, ppem));
    div_fix(darken, em_ratio)
}

fn rounded_i16(value: i64) -> i32 {
    (((value as u32).wrapping_add(0x8000) >> 16) as u16 as i16).into()
}

/// Embolden the unscaled outline before the automatic hints consume it.
pub(super) fn apply(
    points: &mut [Point],
    ends: &[usize],
    metrics: &StyleMetrics,
    ppem: u16,
    upem: i64,
    scale: i64,
    properties: PostScriptHintProperties,
) {
    if properties.no_stem_darkening || upem == 0 || points.is_empty() {
        return;
    }
    let (horizontal, vertical) = match metrics {
        StyleMetrics::Dummy => return,
        StyleMetrics::Latin(metrics) => (
            metrics.axis[HORZ].standard_width,
            metrics.axis[VERT].standard_width,
        ),
        StyleMetrics::Cjk(metrics) => (
            metrics.axis[HORZ].standard_width,
            metrics.axis[VERT].standard_width,
        ),
    };
    let horizontal_amount = amount(horizontal, ppem, upem, properties.darkening_parameters);
    let vertical_amount = amount(vertical, ppem, upem, properties.darkening_parameters);
    let x = rounded_i16(mul_fix(horizontal_amount, scale));
    let y = rounded_i16(mul_fix(vertical_amount, scale));
    if crate::slot_synth::embolden_outline(points, ends, x, y).is_err() {
        return;
    }
    let down = div_fix((upem << 16) - vertical_amount - (8 << 16), upem << 16);
    for point in points {
        point.y = mul_fix(i64::from(point.y), down) as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0086_i2_roboto_stem_darkening_factors_match_freetype() {
        let parameters = PostScriptHintProperties::default().darkening_parameters;
        let upem = 2048;
        let scale = div_fix(9 << 6, upem);
        let x = rounded_i16(mul_fix(amount(185, 9, upem, parameters), scale));
        let y_amount = amount(152, 9, upem, parameters);
        let y = rounded_i16(mul_fix(y_amount, scale));
        let down = div_fix((upem << 16) - y_amount - (8 << 16), upem << 16);
        assert_eq!((x, y, down), (21, 23, 62673));
    }
}
