//! PFR design-unit outlines. The control arrays, instruction formats and
//! compound transforms follow FreeType's src/pfr/pfrgload.c (FTL).

use alloc::vec::Vec;
use sofel_font::PfrFace;
use sofel_font::geom::{Command, Pen, Point, mul_fix};
use sofel_font::read::{GlyphId, ReadError, be};

struct Bytes<'a>(&'a [u8]);
impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ReadError> {
        let (value, tail) = self.0.split_at_checked(n).ok_or(ReadError::OutOfBounds)?;
        self.0 = tail;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, ReadError> {
        self.take(1)?.first().copied().ok_or(ReadError::OutOfBounds)
    }
    fn signed(&mut self) -> Result<i32, ReadError> {
        Ok(i32::from(i8::from_be_bytes([self.byte()?])))
    }
    fn word(&mut self) -> Result<i32, ReadError> {
        be::i16_at(self.take(2)?, 0)
            .map(i32::from)
            .ok_or(ReadError::OutOfBounds)
    }
    fn extras(&mut self) -> Result<(), ReadError> {
        for _ in 0..self.byte()? {
            let n = self.byte()?;
            self.byte()?;
            self.take(usize::from(n))?;
        }
        Ok(())
    }
    fn coordinate(
        &mut self,
        format: u16,
        previous: i32,
        controls: &[i32],
    ) -> Result<i32, ReadError> {
        match format & 3 {
            0 => controls
                .get(usize::from(self.byte()?))
                .copied()
                .ok_or(ReadError::OutOfBounds),
            1 => self.word(),
            2 => previous
                .checked_add(self.signed()?)
                .ok_or(ReadError::InvalidValue),
            _ => Ok(previous),
        }
    }
    fn delta(&mut self, format: u8) -> Result<i32, ReadError> {
        match format & 3 {
            1 => self.word(),
            2 => self.signed(),
            _ => Ok(0),
        }
    }
}

/// Emits the unscaled outline of a PFR glyph to a pen after the entire glyph
/// is decoded.
pub fn draw_pfr(face: &PfrFace<'_>, glyph: GlyphId, pen: &mut impl Pen) -> Result<(), ReadError> {
    let mut commands = Vec::new();
    let mut components = 0;
    decode(
        face.glyph_program(glyph).ok_or(ReadError::OutOfBounds)?,
        face.programs(),
        &mut components,
        &mut commands,
    )?;
    for cmd in commands {
        cmd.apply(pen);
    }
    Ok(())
}

fn decode(
    data: &[u8],
    gps: &[u8],
    components: &mut u8,
    commands: &mut Vec<Command>,
) -> Result<(), ReadError> {
    let mut bytes = Bytes(data);
    let flags = bytes.byte()?;
    if flags & 0x80 == 0 {
        return simple(&mut bytes, flags, commands);
    }
    let count = flags & 63;
    *components = components
        .checked_add(count)
        .filter(|&n| n <= 64)
        .ok_or(ReadError::InvalidValue)?;
    if flags & 64 != 0 {
        bytes.extras()?;
    }
    for _ in 0..count {
        let format = bytes.byte()?;
        let sx = if format & 16 != 0 {
            bytes.word()? << 4
        } else {
            65536
        };
        let sy = if format & 32 != 0 {
            bytes.word()? << 4
        } else {
            65536
        };
        let dx = bytes.delta(format)?;
        let dy = bytes.delta(format >> 2)?;
        let size = if format & 64 != 0 {
            usize::from(be::u16_at(bytes.take(2)?, 0).ok_or(ReadError::OutOfBounds)?)
        } else {
            usize::from(bytes.byte()?)
        };
        let offset = if format & 128 != 0 {
            be::to_usize(be::u24_at(bytes.take(3)?, 0).ok_or(ReadError::OutOfBounds)?)
        } else {
            usize::from(be::u16_at(bytes.take(2)?, 0).ok_or(ReadError::OutOfBounds)?)
        };
        let start = commands.len();
        decode(
            be::slice_at(gps, offset, size).ok_or(ReadError::OutOfBounds)?,
            gps,
            components,
            commands,
        )?;
        let transform = |p: Point| -> Result<Point, ReadError> {
            Ok(Point {
                x: mul_fix(p.x, sx)
                    .checked_add(dx)
                    .ok_or(ReadError::InvalidValue)?,
                y: mul_fix(p.y, sy)
                    .checked_add(dy)
                    .ok_or(ReadError::InvalidValue)?,
            })
        };
        for c in commands.get_mut(start..).ok_or(ReadError::OutOfBounds)? {
            *c = match *c {
                Command::MoveTo(p) => Command::MoveTo(transform(p)?),
                Command::LineTo(p) => Command::LineTo(transform(p)?),
                Command::CubicTo(a, b, p) => {
                    Command::CubicTo(transform(a)?, transform(b)?, transform(p)?)
                }
                other => other,
            };
        }
    }
    Ok(())
}

fn close(start: Option<Point>, last: Point, commands: &mut Vec<Command>) {
    if let Some(p) = start {
        if last != p {
            commands.push(Command::LineTo(p));
        }
        commands.push(Command::Close);
    }
}

fn simple(bytes: &mut Bytes<'_>, flags: u8, commands: &mut Vec<Command>) -> Result<(), ReadError> {
    let (nx, ny) = if flags & 4 != 0 {
        let packed = bytes.byte()?;
        (packed & 15, packed >> 4)
    } else {
        (
            if flags & 2 != 0 { bytes.byte()? } else { 0 },
            if flags & 1 != 0 { bytes.byte()? } else { 0 },
        )
    };
    let count = usize::from(nx).saturating_add(usize::from(ny));
    let mut controls = Vec::with_capacity(count);
    let mut value = 0i32;
    let mut mask = 0;
    for i in 0..count {
        if i % 8 == 0 {
            mask = bytes.byte()?;
        }
        value = if mask & 1 != 0 {
            bytes.word()?
        } else {
            value.saturating_add(i32::from(bytes.byte()?))
        };
        controls.push(value);
        mask >>= 1;
    }
    let (x, y) = controls
        .split_at_checked(usize::from(nx))
        .ok_or(ReadError::OutOfBounds)?;
    if flags & 8 != 0 {
        bytes.extras()?;
    }
    let mut start = None;
    let mut pos = Point::default();
    loop {
        let op = bytes.byte()?;
        let kind = op >> 4;
        let mut fmt = u16::from(op & 15);
        if kind == 0 {
            close(start, pos, commands);
            return Ok(());
        }
        if kind != 4 && kind != 5 && start.is_none() {
            return Err(ReadError::InvalidValue);
        }
        if kind == 2 || kind == 3 {
            let coordinate = if kind == 2 { x } else { y }
                .get(usize::from(op & 15))
                .copied()
                .ok_or(ReadError::OutOfBounds)?;
            if kind == 2 {
                pos.x = coordinate;
            } else {
                pos.y = coordinate;
            }
            commands.push(Command::LineTo(pos));
            continue;
        }
        let prior = pos;
        if kind == 6 {
            fmt = 0xb8e;
        } else if kind == 7 {
            fmt = 0xe2b;
        }
        let mut points = [Point::default(); 3];
        let is_curve = kind >= 6;
        for (i, p) in points
            .iter_mut()
            .take(if is_curve { 3 } else { 1 })
            .enumerate()
        {
            pos = Point {
                x: bytes.coordinate(fmt, pos.x, x)?,
                y: bytes.coordinate(fmt >> 2, pos.y, y)?,
            };
            *p = pos;
            fmt = if i == 0 && kind >= 8 {
                u16::from(bytes.byte()?)
            } else {
                fmt >> 4
            };
        }
        if kind == 4 || kind == 5 {
            close(start, prior, commands);
            start = Some(pos);
            commands.push(Command::MoveTo(pos));
        } else if is_curve {
            let [a, b, c] = points;
            commands.push(Command::CubicTo(a, b, c));
        } else {
            commands.push(Command::LineTo(pos));
        }
    }
}
