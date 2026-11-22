//! Shared gray rendering for common faces (root ADR 0100).

use alloc::vec::Vec;

use sofel_font::{Face, FaceError, read::GlyphId};
use sofel_outline::{Outlines, Scratch};
use sofel_raster::{Bitmap, Rasterizer};

use crate::{RenderError, render, render_bare_cff, render_cid, render_pfr, render_type1};

/// An owned 8-bit coverage image from an outline or fixed strike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceImage {
    /// Horizontal bearing in pixels.
    pub left: i32,
    /// Vertical top bearing in pixels.
    pub top: i32,
    /// Number of pixel columns.
    pub width: u32,
    /// Number of pixel rows.
    pub rows: u32,
    /// Row-major coverage values, zero for background and 255 for full ink.
    pub pixels: Vec<u8>,
}

impl From<Bitmap<'_>> for FaceImage {
    fn from(value: Bitmap<'_>) -> Self {
        Self {
            left: value.left,
            top: value.top,
            width: value.width,
            rows: value.rows,
            pixels: value.pixels.to_vec(),
        }
    }
}

/// Why a common face could not produce an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceRenderError {
    /// Invalid size, glyph or strike.
    Face(FaceError),
    /// Outline or raster failure.
    Render(RenderError),
    /// A selected bitmap could not be decoded.
    Bitmap,
}

impl From<FaceError> for FaceRenderError {
    fn from(value: FaceError) -> Self {
        Self::Face(value)
    }
}

impl From<RenderError> for FaceRenderError {
    fn from(value: RenderError) -> Self {
        Self::Render(value)
    }
}

/// Renders a glyph through the common outline or exact fixed-strike path.
pub fn render_face(
    face: Face<'_>,
    glyph: GlyphId,
    ppem: u16,
    strike: Option<u32>,
    scratch: &mut Scratch,
    raster: &mut Rasterizer,
) -> Result<FaceImage, FaceRenderError> {
    face.h_advance(glyph, ppem, strike)?;
    match face {
        Face::Sfnt(font) => {
            let outlines = Outlines::new(font.default_instance());
            Ok(render(&outlines, glyph, ppem, scratch, raster)?.into())
        }
        Face::BareCff(font) => Ok(render_bare_cff(font, glyph, ppem, scratch, raster)?.into()),
        Face::Type1(font) => Ok(render_type1(
            font,
            usize::try_from(glyph.0).map_err(|_| FaceError::Overflow)?,
            ppem,
            raster,
        )?
        .into()),
        Face::Cid(font) => Ok(render_cid(
            font,
            usize::try_from(glyph.0).map_err(|_| FaceError::Overflow)?,
            ppem,
            raster,
        )?
        .into()),
        Face::Type42(font) => {
            let embedded = font.embedded_glyph(glyph).ok_or(FaceError::InvalidGlyph)?;
            let inner = font.inner_font();
            let outlines = Outlines::new(inner.default_instance());
            Ok(render(&outlines, embedded, ppem, scratch, raster)?.into())
        }
        Face::Pfr(font) => {
            let bitmap_only = !font.inner().has_outlines();
            let strike = strike.or_else(|| {
                bitmap_only
                    .then(|| {
                        (0..font.strike_count()).find(|&index| {
                            font.strike(index).is_some_and(|(x, y)| {
                                let ppem = u32::from(ppem);
                                (x.saturating_add(32) / 64, y.saturating_add(32) / 64)
                                    == (ppem, ppem)
                            })
                        })
                    })
                    .flatten()
            });
            if let Some(index) = strike {
                let (x, y) = font.strike(index).ok_or(FaceError::InvalidStrike)?;
                let bitmap = sofel_bitmap::pfr_bitmap(
                    font,
                    glyph,
                    u16::try_from(x.saturating_add(32) / 64).map_err(|_| FaceError::Overflow)?,
                    u16::try_from(y.saturating_add(32) / 64).map_err(|_| FaceError::Overflow)?,
                )
                .map_err(|_| FaceRenderError::Bitmap)?;
                if let Some(bitmap) = bitmap {
                    return unpack_bitmap(
                        bitmap.left,
                        bitmap.top,
                        bitmap.width,
                        bitmap.height,
                        1,
                        &bitmap.pixels,
                    );
                }
            }
            Ok(render_pfr(font, glyph, ppem, raster)?.into())
        }
        Face::Bitmap(font) => {
            let bitmap = font.glyph(glyph).ok_or(FaceError::InvalidGlyph)?;
            unpack_bitmap(
                i32::from(bitmap.left),
                i32::from(bitmap.top),
                bitmap.width,
                bitmap.height,
                font.bits_per_pixel(),
                &bitmap.bitmap,
            )
        }
    }
}

fn unpack_bitmap(
    left: i32,
    top: i32,
    width: u16,
    height: u16,
    bits: u8,
    packed: &[u8],
) -> Result<FaceImage, FaceRenderError> {
    if !matches!(bits, 1 | 2 | 4 | 8) {
        return Err(FaceRenderError::Bitmap);
    }
    let width_usize = usize::from(width);
    let height_usize = usize::from(height);
    let bits = usize::from(bits);
    let pitch = width_usize
        .checked_mul(bits)
        .and_then(|n| n.checked_add(7))
        .ok_or(FaceRenderError::Bitmap)?
        / 8;
    let len = pitch
        .checked_mul(height_usize)
        .ok_or(FaceRenderError::Bitmap)?;
    let packed = packed.get(..len).ok_or(FaceRenderError::Bitmap)?;
    if pitch == 0 {
        return Ok(FaceImage {
            left,
            top,
            width: 0,
            rows: u32::from(height),
            pixels: Vec::new(),
        });
    }
    let mask =
        u8::try_from((1u16 << bits).saturating_sub(1)).map_err(|_| FaceRenderError::Bitmap)?;
    let scale = match bits {
        1 => 255,
        2 => 85,
        4 => 17,
        _ => 1,
    };
    let pixel_count = width_usize
        .checked_mul(height_usize)
        .ok_or(FaceRenderError::Bitmap)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(pixel_count)
        .map_err(|_| FaceRenderError::Bitmap)?;
    for row in packed.chunks_exact(pitch) {
        for x in 0..width_usize {
            let bit = x.checked_mul(bits).ok_or(FaceRenderError::Bitmap)?;
            let byte = row.get(bit / 8).copied().ok_or(FaceRenderError::Bitmap)?;
            let shift = 8usize.saturating_sub(bits).saturating_sub(bit % 8);
            pixels.push(((byte >> shift) & mask).saturating_mul(scale));
        }
    }
    Ok(FaceImage {
        left,
        top,
        width: u32::from(width),
        rows: u32::from(height),
        pixels,
    })
}
