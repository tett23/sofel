//! FreeType-compatible automatic grid fitting through skrifa's pure Rust
//! autofit implementation. The point mapping preserves FreeType's slot order.

use alloc::vec::Vec;

use skrifa::instance::{NormalizedCoord, Size};
use skrifa::outline::GlyphStyles;
use skrifa::outline::{
    DrawSettings, Engine, HintingInstance, HintingOptions, OutlineGlyphCollection, SmoothMode,
    Target,
};
use skrifa::raw::{FontRef, model::glyph::outline::PathElement};
use sofel_font::read::{GlyphId, Tag, glyf::Glyph, loca::Loca, sfnt::FontFile};

use crate::{
    HintTarget, OutlineError, OutlineInfo, Outlines, Scale, Scratch, auto_adjust, control_box,
};

#[derive(Clone, Copy, Debug)]
enum Map {
    Move(Option<usize>),
    Line(Option<usize>),
    Quad(usize, Option<usize>),
    Close,
}

fn point_map(tags: &[u8], ends: &[usize]) -> Option<Vec<Map>> {
    let mut out = Vec::new();
    let mut first = 0;
    for &last in ends {
        if last < first || last >= tags.len() {
            return None;
        }
        let start_tag = *tags.get(first)? & 3;
        let last_tag = *tags.get(last)? & 3;
        let start = if start_tag == 1 {
            Some(first)
        } else if last_tag == 1 {
            Some(last)
        } else {
            None
        };
        let limit = if start_tag != 1 && last_tag == 1 {
            last.saturating_sub(1)
        } else {
            last
        };
        out.push(Map::Move(start));
        let mut index = if start_tag == 1 {
            first.saturating_add(1)
        } else {
            first
        };
        while index <= limit {
            match *tags.get(index)? & 3 {
                1 => {
                    out.push(Map::Line(Some(index)));
                    index = index.saturating_add(1);
                }
                0 => {
                    let control = index;
                    if index == limit {
                        out.push(Map::Quad(control, start));
                        break;
                    }
                    index = index.saturating_add(1);
                    let on = (*tags.get(index)? & 3 == 1).then_some(index);
                    out.push(Map::Quad(control, on));
                    if on.is_some() {
                        index = index.saturating_add(1);
                    }
                }
                _ => return None,
            }
        }
        out.push(Map::Close);
        first = last.saturating_add(1);
    }
    (first == tags.len()).then_some(out)
}

#[allow(clippy::cast_possible_truncation)] // Hint coordinates are pixel positions in 26.6 units.
fn device(value: f32) -> i32 {
    (value * 64.0) as i32
}

fn assign_point(scratch: &mut Scratch, index: Option<usize>, x: f32, y: f32) -> Option<()> {
    if let Some(index) = index {
        let point = scratch.points.get_mut(index)?;
        point.x = device(x);
        point.y = device(y);
    }
    Some(())
}

fn apply_path(scratch: &mut Scratch, path: &[PathElement]) -> Option<()> {
    let mapping = point_map(&scratch.tags, &scratch.ends)?;
    if mapping.len() != path.len() {
        return None;
    }
    for (map, command) in mapping.into_iter().zip(path) {
        match (map, command) {
            (Map::Move(index), PathElement::MoveTo { x, y })
            | (Map::Line(index), PathElement::LineTo { x, y }) => {
                assign_point(scratch, index, *x, *y)?;
            }
            (Map::Quad(control, end), PathElement::QuadTo { cx0, cy0, x, y }) => {
                assign_point(scratch, Some(control), *cx0, *cy0)?;
                assign_point(scratch, end, *x, *y)?;
            }
            (Map::Close, PathElement::Close) => {}
            _ => return None,
        }
    }
    Some(())
}

fn target(target: HintTarget) -> Target {
    match target {
        HintTarget::Normal => SmoothMode::Normal.into(),
        HintTarget::Light => SmoothMode::Light.into(),
        HintTarget::Mono => Target::Mono,
        HintTarget::Lcd => SmoothMode::Lcd.into(),
        HintTarget::VerticalLcd => SmoothMode::VerticalLcd.into(),
    }
}

fn face_index(font: &sofel_font::Font<'_>) -> Option<u32> {
    let sfnt = font.sfnt();
    match FontFile::new(sfnt.file()).ok()? {
        FontFile::Sfnt(_) => Some(0),
        FontFile::Collection(collection) => (0..collection.num_fonts())
            .find(|&index| collection.offset(index) == u32::try_from(sfnt.start()).ok()),
        FontFile::Dfont(_) => None,
    }
}

fn is_composite(font: &sofel_font::Font<'_>, glyph: GlyphId) -> bool {
    let sfnt = font.sfnt();
    let Some(glyf) = sfnt.table(Tag::new(b"glyf")) else {
        return false;
    };
    let Some(loca_data) = sfnt.table(Tag::new(b"loca")) else {
        return false;
    };
    let Ok(loca) = Loca::new(loca_data, font.head().index_to_loc_format()) else {
        return false;
    };
    let index = glyph.0 as usize;
    let Some(start) = loca.offset(index).map(|v| v as usize) else {
        return false;
    };
    let Some(end) = loca.offset(index.saturating_add(1)).map(|v| v as usize) else {
        return false;
    };
    glyf.get(start..end)
        .and_then(|data| Glyph::new(data).ok())
        .is_some_and(|value| value.number_of_contours() < 0)
}

pub(super) fn load(
    outlines: &Outlines<'_, '_, '_>,
    glyph: GlyphId,
    ppem: u16,
    target_mode: HintTarget,
    scratch: &mut Scratch,
) -> Result<(OutlineInfo, i32), OutlineError> {
    let font = outlines.font();
    let original = outlines.load(glyph, Scale::Unscaled, scratch)?;
    if scratch.points.is_empty() {
        let scale = Scale::pixels(font, ppem, ppem);
        let Scale::Scaled { x, .. } = scale else {
            return Err(OutlineError::InvalidOutline);
        };
        let advance = outlines.instance().ft_h_advance(glyph).map_or(0, |width| {
            sofel_geom::mul_fix(i32::from(width), x.0).wrapping_add(32) & !63
        });
        return Ok((original, advance));
    }
    let sfnt = font.sfnt();
    let index = face_index(font).ok_or(OutlineError::UnsupportedFormat)?;
    let reference =
        FontRef::from_index(sfnt.file(), index).map_err(|_| OutlineError::UnsupportedFormat)?;
    let source = OutlineGlyphCollection::new(&reference);
    let styles = GlyphStyles::new(&source);
    let latin = styles
        .get(skrifa::GlyphId::new(glyph.0))
        .and_then(|style| style.style_class())
        .is_some_and(|style| style.script.name == "Latin");
    if !latin {
        scratch.clear();
        return Err(OutlineError::AutoHintUnsupportedGlyph);
    }
    let needs_up_adjustment = if is_composite(font, glyph) {
        let codepoint = skrifa::charmap::Charmap::new(&reference)
            .mappings()
            .find(|(_, id)| id.to_u32() == glyph.0)
            .map(|(codepoint, _)| codepoint);
        let Some(codepoint) = codepoint else {
            scratch.clear();
            return Err(OutlineError::AutoHintUnsupportedGlyph);
        };
        if auto_adjust::UP_CODEPOINTS
            .binary_search(&codepoint)
            .is_err()
        {
            scratch.clear();
            return Err(OutlineError::AutoHintUnsupportedGlyph);
        }
        true
    } else {
        false
    };
    let coords: Vec<_> = outlines
        .instance()
        .ft_coords()
        .iter()
        .map(|coord| {
            i16::try_from(coord.0.saturating_add(2) >> 2)
                .map(NormalizedCoord::from_bits)
                .map_err(|_| OutlineError::InvalidOutline)
        })
        .collect::<Result<_, _>>()?;
    let options = HintingOptions {
        engine: Engine::Auto(Some(styles)),
        target: target(target_mode),
    };
    let hinter = HintingInstance::new(
        &source,
        Size::new(f32::from(ppem)),
        coords.as_slice(),
        options,
    )
    .map_err(|_| OutlineError::InvalidOutline)?;
    let source_glyph = source
        .get(skrifa::GlyphId::new(glyph.0))
        .ok_or(OutlineError::InvalidGlyphIndex)?;
    let mut path = Vec::new();
    let metrics = source_glyph
        .draw(DrawSettings::hinted(&hinter, false), &mut path)
        .map_err(|_| OutlineError::InvalidOutline)?;
    apply_path(scratch, &path).ok_or(OutlineError::InvalidOutline)?;
    if needs_up_adjustment && !auto_adjust::adjust_up(scratch, ppem) {
        scratch.clear();
        return Err(OutlineError::AutoHintUnsupportedGlyph);
    }
    let advance = metrics.advance_width.map_or(0, device);
    Ok((
        OutlineInfo {
            overlap: original.overlap,
            cbox: control_box(&scratch.points),
        },
        advance,
    ))
}
