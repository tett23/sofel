use super::*;

pub(super) struct Header {
    pub width: u32,
    pub height: u32,
    pub depth: u8,
    pub color: u8,
    premultiply: fn(u8, u8) -> u8,
    channels: usize,
    interlaced: bool,
    pub output_bytes: usize,
}
impl Header {
    pub(super) fn parse(data: &[u8], limit: usize, premultiply: fn(u8, u8) -> u8) -> Result<Self> {
        let &[a, b, c, d, e, f, g, h, depth, color, 0, 0, interlace] = data else {
            return Err(Error::Invalid);
        };
        let width = u32::from_be_bytes([a, b, c, d]);
        let height = u32::from_be_bytes([e, f, g, h]);
        if width == 0
            || height == 0
            || width > i32::MAX as u32
            || height > i32::MAX as u32
            || interlace > 1
        {
            return Err(Error::Invalid);
        }
        let channels = match (color, depth) {
            (0, 1 | 2 | 4 | 8 | 16) | (3, 1 | 2 | 4 | 8) => 1,
            (2, 8 | 16) => 3,
            (4, 8 | 16) => 2,
            (6, 8 | 16) => 4,
            _ => return Err(Error::Invalid),
        };
        let count = (width as usize)
            .checked_mul(height as usize)
            .filter(|&n| n <= limit)
            .ok_or(Error::Limit)?;
        let output_bytes = count.checked_mul(4).ok_or(Error::Limit)?;
        Ok(Self {
            premultiply,
            width,
            height,
            depth,
            color,
            channels,
            interlaced: interlace == 1,
            output_bytes,
        })
    }
    pub(super) fn passes(&self) -> Result<Vec<Pass>> {
        let steps: &[(usize, usize, usize, usize)] = if self.interlaced {
            &[
                (0, 0, 8, 8),
                (4, 0, 8, 8),
                (0, 4, 4, 8),
                (2, 0, 4, 4),
                (0, 2, 2, 4),
                (1, 0, 2, 2),
                (0, 1, 1, 2),
            ]
        } else {
            &[(0, 0, 1, 1)]
        };
        let mut passes = Vec::new();
        for &(x, y, dx, dy) in steps {
            let width = (self.width as usize).saturating_sub(x).div_ceil(dx);
            let height = (self.height as usize).saturating_sub(y).div_ceil(dy);
            if width == 0 || height == 0 {
                continue;
            }
            let bpp = self.channels.saturating_mul(usize::from(self.depth));
            let row_bytes = width.checked_mul(bpp).ok_or(Error::Limit)?.div_ceil(8);
            passes.push(Pass {
                x,
                y,
                dx,
                dy,
                width,
                height,
                row_bytes,
                stride: bpp.div_ceil(8),
            });
        }
        Ok(passes)
    }
    fn rgba(&self, s: [u16; 4], palette: &[u8], transparent: &[u8]) -> Result<[u8; 4]> {
        let to_byte = |n: u16| -> u8 {
            let v = match self.depth {
                16 => n >> 8,
                8 => n,
                1 => n.saturating_mul(255),
                2 => n.saturating_mul(85),
                _ => n.saturating_mul(17),
            };
            u8::try_from(v).unwrap_or_default()
        };
        let [r, g, b, a] = s;
        let rgba = match self.color {
            0 => [
                to_byte(r),
                to_byte(r),
                to_byte(r),
                if transparent == r.to_be_bytes() {
                    0
                } else {
                    255
                },
            ],
            2 => [
                to_byte(r),
                to_byte(g),
                to_byte(b),
                if transparent == [r.to_be_bytes(), g.to_be_bytes(), b.to_be_bytes()].as_flattened()
                {
                    0
                } else {
                    255
                },
            ],
            3 => {
                let i = usize::from(r);
                let &[r, g, b] = palette.as_chunks::<3>().0.get(i).ok_or(Error::Invalid)?;
                [r, g, b, transparent.get(i).copied().unwrap_or(255)]
            }
            4 => [to_byte(r), to_byte(r), to_byte(r), to_byte(g)],
            6 => [to_byte(r), to_byte(g), to_byte(b), to_byte(a)],
            _ => return Err(Error::Invalid),
        };
        let [r, g, b, a] = rgba;
        let premul = |c| (self.premultiply)(c, a);
        Ok([premul(b), premul(g), premul(r), a])
    }
}

pub(super) struct Pass {
    x: usize,
    y: usize,
    dx: usize,
    dy: usize,
    width: usize,
    height: usize,
    row_bytes: usize,
    stride: usize,
}
impl Pass {
    pub(super) fn bytes(&self) -> Option<usize> {
        self.row_bytes.checked_add(1)?.checked_mul(self.height)
    }
    pub(super) fn draw(
        &self,
        h: &Header,
        data: &[u8],
        palette: &[u8],
        transparent: &[u8],
        out: &mut [u8],
    ) -> Result<()> {
        let pitch = self.row_bytes.checked_add(1).ok_or(Error::Limit)?;
        let mut previous = alloc::vec![0u8;self.row_bytes];
        let mut current = previous.clone();
        for (y, line) in data.chunks_exact(pitch).enumerate() {
            let (&filter, filtered) = line.split_first().ok_or(Error::Invalid)?;
            for (i, &value) in filtered.iter().enumerate() {
                let left = i.checked_sub(self.stride);
                let a = left.and_then(|i| current.get(i)).copied().unwrap_or(0);
                let b = previous.get(i).copied().ok_or(Error::Invalid)?;
                let c = left.and_then(|i| previous.get(i)).copied().unwrap_or(0);
                let predictor = match filter {
                    0 => 0,
                    1 => a,
                    2 => b,
                    3 => u8::midpoint(a, b),
                    4 => paeth(a, b, c),
                    _ => return Err(Error::Invalid),
                };
                *current.get_mut(i).ok_or(Error::Invalid)? = value.wrapping_add(predictor);
            }
            for x in 0..self.width {
                let mut samples = [0u16; 4];
                for (channel, sample) in samples.iter_mut().take(h.channels).enumerate() {
                    let n = x.saturating_mul(h.channels).saturating_add(channel);
                    let bit = n.saturating_mul(usize::from(h.depth));
                    let bytes = current.get(bit / 8..).ok_or(Error::Invalid)?;
                    *sample = match h.depth {
                        16 => u16::from_be_bytes(*bytes.first_chunk::<2>().ok_or(Error::Invalid)?),
                        depth => {
                            let shift = 8usize
                                .saturating_sub(usize::from(depth))
                                .saturating_sub(bit % 8);
                            (u16::from(*bytes.first().ok_or(Error::Invalid)?) >> shift)
                                & (1u16 << depth).saturating_sub(1)
                        }
                    };
                }
                let row = self.y.saturating_add(y.saturating_mul(self.dy));
                let column = self.x.saturating_add(x.saturating_mul(self.dx));
                let offset = row
                    .saturating_mul(h.width as usize)
                    .saturating_add(column)
                    .saturating_mul(4);
                out.get_mut(offset..)
                    .and_then(|p| p.first_chunk_mut::<4>())
                    .ok_or(Error::Invalid)?
                    .copy_from_slice(&h.rgba(samples, palette, transparent)?);
            }
            core::mem::swap(&mut current, &mut previous);
        }
        Ok(())
    }
}
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i32::from(a)
        .saturating_add(i32::from(b))
        .saturating_sub(i32::from(c));
    let da = p.abs_diff(i32::from(a));
    let db = p.abs_diff(i32::from(b));
    let dc = p.abs_diff(i32::from(c));
    if da <= db && da <= dc {
        a
    } else if db <= dc {
        b
    } else {
        c
    }
}
