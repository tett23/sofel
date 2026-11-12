//! Premultiplied BGRA compositing, following HarfBuzz hb-raster-image.cc
//! (Old MIT, see THIRD_PARTY_NOTICES.md).
// Channel products fit in u32; from_fn indexes fixed-size channel arrays.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use sofel_geom::CompositeMode;

/// Composite a source pixel onto a destination pixel. The caller supplies
/// square root for the soft-light blend in no_std environments.
pub fn composite_pixel(
    src: [u8; 4],
    dst: [u8; 4],
    mode: CompositeMode,
    sqrt: impl Fn(f32) -> f32,
) -> [u8; 4] {
    use CompositeMode::*;
    let sa = src[3];
    let da = dst[3];
    let factors = match mode {
        Clear => return [0; 4],
        Src => return src,
        Dest => return dst,
        SrcOver => (255, 255 - sa),
        DestOver => (255 - da, 255),
        SrcIn => (da, 0),
        DestIn => (0, sa),
        SrcOut => (255 - da, 0),
        DestOut => (0, 255 - sa),
        SrcAtop => (da, 255 - sa),
        DestAtop => (255 - da, sa),
        Xor => (255 - da, 255 - sa),
        Plus => (255, 255),
        _ => return blend(src, dst, mode, sqrt),
    };
    let scale = |v: u8, a: u8| ((u32::from(v) * u32::from(a) + 255) >> 8) as u8;
    core::array::from_fn(|i| scale(src[i], factors.0).saturating_add(scale(dst[i], factors.1)))
}

fn luminosity([b, g, r]: [f32; 3]) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

fn range([b, g, r]: [f32; 3]) -> (f32, f32) {
    (r.min(g).min(b), r.max(g).max(b))
}

fn set_saturation(c: [f32; 3], target: f32) -> [f32; 3] {
    let (min, max) = range(c);
    if max == min {
        return [0.; 3];
    }
    c.map(|v| {
        if v == max {
            target
        } else {
            (v - min) * target / (max - min)
        }
    })
}

fn set_luminosity(c: [f32; 3], target: f32) -> [f32; 3] {
    let shift = target - luminosity(c);
    let mut c = c.map(|v| v + shift);
    let l = luminosity(c);
    let (min, max) = range(c);
    if min < 0. && l > min {
        c = c.map(|v| l + (v - l) * l / (l - min));
    }
    if max > 1. && max > l {
        c = c.map(|v| l + (v - l) * (1. - l) / (max - l));
    }
    c
}

fn overlay(s: f32, d: f32) -> f32 {
    if d <= 0.5 {
        2. * s * d
    } else {
        1. - 2. * (1. - s) * (1. - d)
    }
}

fn blend(src: [u8; 4], dst: [u8; 4], mode: CompositeMode, sqrt: impl Fn(f32) -> f32) -> [u8; 4] {
    use CompositeMode::*;
    let unpack = |[b, g, r, a]: [u8; 4]| {
        let alpha = f32::from(a) / 255.;
        (
            if a == 0 {
                [0.; 3]
            } else {
                [b, g, r].map(|v| (f32::from(v) / 255.) / alpha)
            },
            alpha,
        )
    };
    let (s, sa) = unpack(src);
    let (d, da) = unpack(dst);
    let saturation = |rgb| {
        let (min, max) = range(rgb);
        max - min
    };
    let mixed = match mode {
        HslHue => set_luminosity(set_saturation(s, saturation(d)), luminosity(d)),
        HslSaturation => set_luminosity(set_saturation(d, saturation(s)), luminosity(d)),
        HslColor => set_luminosity(s, luminosity(d)),
        HslLuminosity => set_luminosity(d, luminosity(s)),
        _ => core::array::from_fn(|i| {
            let (s, d) = (s[i], d[i]);
            match mode {
                Multiply => s * d,
                Screen => s + d - s * d,
                Overlay => overlay(s, d),
                Darken => s.min(d),
                Lighten => s.max(d),
                ColorDodge if d <= 0. => 0.,
                ColorDodge if s >= 1. => 1.,
                ColorDodge => (d / (1. - s)).min(1.),
                ColorBurn if d >= 1. => 1.,
                ColorBurn if s <= 0. => 0.,
                ColorBurn => 1. - ((1. - d) / s).min(1.),
                HardLight => overlay(d, s),
                SoftLight if s <= 0.5 => d - (1. - 2. * s) * d * (1. - d),
                SoftLight => {
                    let curve = if d <= 0.25 {
                        ((16. * d - 12.) * d + 4.) * d
                    } else {
                        sqrt(d)
                    };
                    d + (2. * s - 1.) * (curve - d)
                }
                Difference => (s - d).abs(),
                Exclusion => s + d - 2. * s * d,
                _ => unreachable!(),
            }
        }),
    };
    let byte = |v: f32| (v.clamp(0., 1.) * 255. + 0.5) as u8;
    let [b, g, r] = core::array::from_fn(|i| {
        byte(sa * da * mixed[i] + sa * (1. - da) * s[i] + (1. - sa) * da * d[i])
    });
    [b, g, r, byte(sa + da - sa * da)]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn i2_translucent_composite_modes_match_harfbuzz() {
        // hb-raster-image.cc::composite_pixel(0x80592106, 0xb3093492, mode).
        let expected = [
            [0, 0, 0, 0],
            [6, 33, 89, 128],
            [146, 52, 9, 179],
            [79, 59, 94, 217],
            [148, 62, 36, 217],
            [5, 24, 63, 90],
            [73, 26, 5, 90],
            [2, 10, 27, 38],
            [73, 26, 5, 89],
            [78, 50, 68, 179],
            [75, 36, 32, 128],
            [75, 36, 32, 127],
            [152, 85, 98, 255],
            [149, 78, 95, 217],
            [133, 49, 37, 217],
            [79, 59, 36, 217],
            [148, 62, 93, 217],
            [151, 71, 46, 217],
            [75, 36, 31, 217],
            [81, 49, 69, 217],
            [136, 53, 40, 217],
            [144, 39, 89, 217],
            [145, 72, 92, 217],
            [78, 42, 34, 217],
            [75, 52, 82, 217],
            [140, 62, 39, 217],
            [75, 52, 82, 217],
            [156, 70, 43, 217],
        ];
        for (mode, expected) in (0..28).filter_map(CompositeMode::from_u8).zip(expected) {
            assert_eq!(
                composite_pixel([6, 33, 89, 128], [146, 52, 9, 179], mode, f32::sqrt),
                expected,
                "{mode:?}"
            );
            assert_eq!(
                composite_pixel([0; 4], [0; 4], mode, f32::sqrt),
                [0; 4],
                "{mode:?}"
            );
        }
    }
}
