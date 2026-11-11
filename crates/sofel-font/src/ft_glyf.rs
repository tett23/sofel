//! FreeType's unscaled glyf phantom-point path for advance queries (ADR 0048).

use sofel_read::{GlyphId, glyf};

use crate::var::Instance;
use crate::var::gvar::{FtScratch, Point};

const MAX_DEPTH: usize = 64;

pub(crate) fn metrics_source_glyph(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
) -> Option<GlyphId> {
    metrics_source_at(instance, glyph, 0)
}

fn metrics_source_at(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    depth: usize,
) -> Option<GlyphId> {
    if depth >= MAX_DEPTH {
        return None;
    }
    let glyf = instance.font().glyf?;
    if glyph.0 >= glyf.num_glyphs() {
        return None;
    }
    let data = glyf.glyph(glyph);
    if data.contours >= 0 {
        return Some(glyph);
    }
    let source = glyf
        .components(&data)
        .filter(|(flags, _)| flags & glyf::USE_MY_METRICS != 0)
        .map(|(_, child)| child)
        .last();
    source.map_or(Some(glyph), |child| {
        metrics_source_at(instance, GlyphId(child), depth.saturating_add(1))
    })
}

/// Reused storage for FreeType advance queries that require gvar.
pub struct FtAdvanceScratch {
    gvar: FtScratch,
    stack: [u32; MAX_DEPTH],
    depth: usize,
}

impl Default for FtAdvanceScratch {
    fn default() -> Self {
        Self {
            gvar: FtScratch::default(),
            stack: [0; MAX_DEPTH],
            depth: 0,
        }
    }
}

/// Values returned by the first no-scale `FT_Get_Advances` call on a fresh face.
pub(crate) fn advances(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scratch: &mut FtAdvanceScratch,
) -> Option<(Option<u16>, Option<u16>)> {
    let phantoms = phantoms_at(instance, glyph, scratch)?;
    let horizontal = phantoms[1].x.wrapping_sub(phantoms[0].x);
    let vertical = phantoms[2].y.wrapping_sub(phantoms[3].y);
    Some((
        u16::try_from(horizontal).ok(),
        u16::try_from(vertical.max(0)).ok(),
    ))
}

fn phantoms_at(
    instance: Instance<'_, '_, '_, '_>,
    glyph: GlyphId,
    scratch: &mut FtAdvanceScratch,
) -> Option<[Point<i32>; 4]> {
    if scratch.depth >= MAX_DEPTH
        || scratch
            .stack
            .get(..scratch.depth)
            .is_some_and(|stack| stack.contains(&glyph.0))
    {
        return None;
    }
    let font = instance.font();
    let glyf = font.glyf?;
    if glyph.0 >= glyf.num_glyphs() {
        return None;
    }
    let data = glyf.glyph(glyph);
    let parsed = (!data.bytes.is_empty())
        .then(|| glyf::Glyph::new(data.bytes).ok())
        .flatten();
    let count = if data.contours > 0 {
        usize::try_from(parsed?.simple().ok()?.num_points()).ok()?
    } else if data.contours < 0 {
        parsed?.components(glyf.extended()).count()
    } else {
        0
    };
    let left = i32::from(data.x_min).wrapping_sub(i32::from(instance.ft_left_side_bearing(glyph)));
    let right = left.wrapping_add(i32::from(instance.ft_h_advance_before_gvar(glyph)?));
    let top = i32::from(data.y_max)
        .wrapping_add(i32::from(instance.ft_top_side_bearing(glyph, data.y_max)?));
    let bottom = top.wrapping_sub(i32::from(instance.ft_v_advance_before_gvar(glyph)?));
    let bases = [
        Point {
            x: left,
            y: 0,
            end: false,
        },
        Point {
            x: right,
            y: 0,
            end: false,
        },
        Point {
            x: 0,
            y: top,
            end: false,
        },
        Point {
            x: 0,
            y: bottom,
            end: false,
        },
    ];
    let originals = bases.map(|point| Point {
        x: point.x.wrapping_shl(16),
        y: point.y.wrapping_shl(16),
        end: false,
    });
    let mut phantoms = originals;
    instance
        .apply_gvar_ft_phantoms(glyph.0, count, &mut phantoms, &mut scratch.gvar)
        .ok()?;
    if instance.ft_hvar_supported() {
        phantoms[..2].copy_from_slice(&originals[..2]);
    }
    if instance.ft_vvar_supported() {
        phantoms[2..].copy_from_slice(&originals[2..]);
    }
    for ((phantom, original), base) in phantoms.iter_mut().zip(originals).zip(bases) {
        let dx = phantom.x.wrapping_sub(original.x);
        let dy = phantom.y.wrapping_sub(original.y);
        phantom.x = base.x.wrapping_add(dx.wrapping_add(0x8000) >> 16);
        phantom.y = base.y.wrapping_add(dy.wrapping_add(0x8000) >> 16);
    }
    if data.contours < 0 {
        let mut components = parsed?.components(glyf.extended());
        *scratch.stack.get_mut(scratch.depth)? = glyph.0;
        scratch.depth = scratch.depth.saturating_add(1);
        let result = components.try_fold(phantoms, |current, component| {
            let component = component.ok()?;
            if component.flags & glyf::USE_MY_METRICS != 0 {
                phantoms_at(instance, component.glyph, scratch)
            } else {
                Some(current)
            }
        });
        scratch.depth = scratch.depth.saturating_sub(1);
        phantoms = result?;
    }
    Some(phantoms)
}
