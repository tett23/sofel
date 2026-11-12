//! FreeType Harmony LCD rendering with its default RGB stripe geometry.
use super::{
    Bitmap, PixelBox, RasterError, Rasterizer, RenderOptions, control_box, pixel_box_wide,
    too_large,
};
use sofel_geom::Point;

/// The orientation of the three subpixels in a logical pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LcdMode {
    /// RGB channels in adjacent columns.
    Horizontal,
    /// RGB channels in adjacent rows.
    Vertical,
}

impl Rasterizer {
    /// Renders three grayscale coverages displaced according to FreeType's
    /// default Harmony geometry. Width and rows count subpixels in the active
    /// direction. Rows have no padding. The layout is independent of hinting.
    ///
    /// # Errors
    /// Returns an error if the bitmap bounds or allocation exceed the supported range.
    pub fn render_harmony(
        &mut self,
        mode: LcdMode,
        options: RenderOptions,
    ) -> Result<Bitmap<'_>, RasterError> {
        let (x0, y0, x1, y1) = control_box(&self.commands).unwrap_or_default();
        let (pad_x, pad_y) = match mode {
            LcdMode::Horizontal => (21, 0),
            LcdMode::Vertical => (0, 21),
        };
        let (left, top, width, rows) = pixel_box_wide(
            x0.saturating_sub(pad_x),
            y0.saturating_sub(pad_y),
            x1.saturating_add(pad_x),
            y1.saturating_add(pad_y),
        );
        if too_large(left, top, width, rows, options.ppem) {
            return Err(RasterError::TooLarge);
        }
        let b = PixelBox {
            left: i32::try_from(left).map_err(|_| RasterError::TooLarge)?,
            top: i32::try_from(top).map_err(|_| RasterError::TooLarge)?,
            width: u32::try_from(width).map_err(|_| RasterError::TooLarge)?,
            rows: u32::try_from(rows).map_err(|_| RasterError::TooLarge)?,
        };
        let (width, rows) = match mode {
            LcdMode::Horizontal => (b.width.saturating_mul(3), b.rows),
            LcdMode::Vertical => (b.width, b.rows.saturating_mul(3)),
        };
        let len = (width as usize)
            .checked_mul(rows as usize)
            .ok_or(RasterError::TooLarge)?;
        let mut output = core::mem::take(&mut self.lcd_pixels);
        output.clear();
        output.try_reserve(len).map_err(|_| RasterError::TooLarge)?;
        output.resize(len, 0);
        let shift = b.shift();
        if len != 0 {
            for (channel, delta) in [-21, 0, 21].into_iter().enumerate() {
                let shift = match mode {
                    LcdMode::Horizontal => Point {
                        x: shift.x.saturating_sub(delta),
                        ..shift
                    },
                    LcdMode::Vertical => Point {
                        y: shift.y.saturating_add(delta),
                        ..shift
                    },
                };
                let plane = self.render_box(
                    b,
                    shift,
                    Point { x: 1, y: 1 },
                    RenderOptions {
                        overlap: false,
                        ..options
                    },
                )?;
                match mode {
                    LcdMode::Horizontal => {
                        for (dst, src) in
                            output.iter_mut().skip(channel).step_by(3).zip(plane.pixels)
                        {
                            *dst = *src;
                        }
                    }
                    LcdMode::Vertical => {
                        for (dst, src) in output
                            .chunks_exact_mut(width as usize)
                            .skip(channel)
                            .step_by(3)
                            .zip(plane.pixels.chunks_exact(width as usize))
                        {
                            dst.copy_from_slice(src);
                        }
                    }
                }
            }
        }
        self.lcd_pixels = output;
        Ok(Bitmap {
            left: b.left,
            top: b.top,
            width,
            rows,
            pixels: &self.lcd_pixels,
        })
    }
}

impl Rasterizer {
    /// Renders FreeType's filtered LCD configuration. The five weights are
    /// ordered left to right or top to bottom. A zero center disables filtering.
    ///
    /// # Errors
    /// Returns an error when the bitmap or the direct span coordinates exceed
    /// FreeType's supported bounds.
    pub fn render_filtered_lcd(
        &mut self,
        mode: LcdMode,
        weights: [u8; 5],
        options: RenderOptions,
    ) -> Result<Bitmap<'_>, RasterError> {
        let (x0, y0, x1, y1) = control_box(&self.commands).unwrap_or_default();
        let (px, py, scale) = match mode {
            LcdMode::Horizontal => (43, 0, Point { x: 3, y: 1 }),
            LcdMode::Vertical => (0, 43, Point { x: 1, y: 3 }),
        };
        let (left, top, width, rows) = pixel_box_wide(
            x0.saturating_sub(px),
            y0.saturating_sub(py),
            x1.saturating_add(px),
            y1.saturating_add(py),
        );
        if too_large(left, top, width, rows, options.ppem) {
            return Err(RasterError::TooLarge);
        }
        let logical = PixelBox {
            left: i32::try_from(left).map_err(|_| RasterError::TooLarge)?,
            top: i32::try_from(top).map_err(|_| RasterError::TooLarge)?,
            width: u32::try_from(width).map_err(|_| RasterError::TooLarge)?,
            rows: u32::try_from(rows).map_err(|_| RasterError::TooLarge)?,
        };
        let b = PixelBox {
            width: logical.width.saturating_mul(scale.x.cast_unsigned()),
            rows: logical.rows.saturating_mul(scale.y.cast_unsigned()),
            ..logical
        };
        if weights[2] != 0 && b.width > 0x7fff {
            return Err(RasterError::TooLarge);
        }
        let len = (b.width as usize)
            .checked_mul(b.rows as usize)
            .ok_or(RasterError::TooLarge)?;
        self.pixels.clear();
        self.pixels
            .try_reserve(len)
            .map_err(|_| RasterError::TooLarge)?;
        self.render_box(
            b,
            logical.shift(),
            scale,
            RenderOptions {
                overlap: false,
                ..options
            },
        )?;
        if weights[2] != 0 && len != 0 {
            self.lcd_pixels.clear();
            self.lcd_pixels
                .try_reserve(len)
                .map_err(|_| RasterError::TooLarge)?;
            self.lcd_pixels.resize(len, 0);
            let width = b.width as usize;
            let nonzero_width = core::num::NonZeroUsize::new(width).ok_or(RasterError::TooLarge)?;
            for (i, &coverage) in self.pixels.iter().enumerate().filter(|(_, c)| **c != 0) {
                let (row, col) = (i / nonzero_width, i % nonzero_width);
                for (tap, weight) in weights.into_iter().enumerate() {
                    let offset = tap.cast_signed().wrapping_sub(2);
                    let dst = match mode {
                        LcdMode::Horizontal => col
                            .checked_add_signed(offset)
                            .filter(|x| *x < width)
                            .map(|x| row.saturating_mul(width).saturating_add(x)),
                        LcdMode::Vertical => row
                            .checked_add_signed(offset)
                            .filter(|y| *y < b.rows as usize)
                            .map(|y| y.saturating_mul(width).saturating_add(col)),
                    };
                    if let Some(pixel) = dst.and_then(|j| self.lcd_pixels.get_mut(j)) {
                        let value = u16::from(coverage)
                            .wrapping_mul(u16::from(weight))
                            .wrapping_add(85)
                            >> 8;
                        *pixel = pixel.wrapping_add(value.to_le_bytes()[0]);
                    }
                }
            }
            core::mem::swap(&mut self.pixels, &mut self.lcd_pixels);
        }
        Ok(Bitmap {
            left: b.left,
            top: b.top,
            width: b.width,
            rows: b.rows,
            pixels: &self.pixels,
        })
    }
}
