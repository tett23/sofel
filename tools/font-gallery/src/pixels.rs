//! PGM/PAM pixels and the exact integer compositing used by ADR 0069.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    pub left: i32,
    pub top: i32,
    pub pgm: bool,
}

/// A signed 26.6 font position; it is not an integer pixel coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fixed26Dot6(pub i32);

#[derive(Clone, Debug)]
pub struct Glyph {
    pub image: Option<Image>,
    pub dx: Fixed26Dot6,
    pub dy: Fixed26Dot6,
    pub ax: Fixed26Dot6,
    pub ay: Fixed26Dot6,
}

fn line<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a str, String> {
    let rest = bytes.get(*cursor..).ok_or("invalid image header")?;
    let end = rest
        .iter()
        .position(|b| *b == b'\n')
        .ok_or("invalid image header")?;
    *cursor += end + 1;
    std::str::from_utf8(&rest[..end]).map_err(|_| "invalid image header".to_string())
}

fn origin(row: &str) -> Option<(i32, i32)> {
    let mut words = row.split_whitespace();
    if words.next() != Some("#") || words.next() != Some("left") {
        return None;
    }
    let left = words.next()?.parse().ok()?;
    if words.next() != Some("top") {
        return None;
    }
    let top = words.next()?.parse().ok()?;
    (words.next().is_none()).then_some((left, top))
}

pub fn parse(bytes: &[u8]) -> Result<Image, String> {
    let mut cursor = 0;
    let magic = line(bytes, &mut cursor)?;
    let (width, height, left, top, pgm, pixels) = match magic {
        "P5" => {
            let mut row = line(bytes, &mut cursor)?;
            let mut place = (0, 0);
            while row.starts_with('#') {
                if let Some(found) = origin(row) {
                    place = found;
                }
                row = line(bytes, &mut cursor)?;
            }
            let mut dims = row.split_whitespace();
            let width: usize = dims
                .next()
                .ok_or("invalid PGM header")?
                .parse()
                .map_err(|_| "invalid PGM header")?;
            let height: usize = dims
                .next()
                .ok_or("invalid PGM header")?
                .parse()
                .map_err(|_| "invalid PGM header")?;
            if dims.next().is_some() || line(bytes, &mut cursor)? != "255" {
                return Err("invalid PGM header".into());
            }
            let size = width
                .checked_mul(height)
                .ok_or("invalid image dimensions")?;
            let raw = bytes.get(cursor..).ok_or("invalid PGM payload")?;
            if raw.len() != size {
                return Err("invalid PGM payload".into());
            }
            let mut rgba = Vec::with_capacity(size.saturating_mul(4));
            for &value in raw {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
            (width, height, place.0, place.1, true, rgba)
        }
        "P7" => {
            let mut width = None;
            let mut height = None;
            let mut depth = None;
            let mut maxval = None;
            let mut tuple = None;
            let mut place = (0, 0);
            loop {
                let row = line(bytes, &mut cursor)?;
                if row == "ENDHDR" {
                    break;
                }
                if let Some(found) = origin(row) {
                    place = found;
                }
                let mut words = row.split_whitespace();
                match words.next() {
                    Some("WIDTH") => width = words.next().and_then(|x| x.parse::<usize>().ok()),
                    Some("HEIGHT") => height = words.next().and_then(|x| x.parse::<usize>().ok()),
                    Some("DEPTH") => depth = words.next(),
                    Some("MAXVAL") => maxval = words.next(),
                    Some("TUPLTYPE") => tuple = words.next(),
                    _ => {}
                }
            }
            let width = width.ok_or("invalid PAM header")?;
            let height = height.ok_or("invalid PAM header")?;
            if depth != Some("4") || maxval != Some("255") || tuple != Some("RGB_ALPHA") {
                return Err("unsupported PAM format".into());
            }
            let size = width
                .checked_mul(height)
                .and_then(|x| x.checked_mul(4))
                .ok_or("invalid image dimensions")?;
            let raw = bytes.get(cursor..).ok_or("invalid PAM payload")?;
            if raw.len() != size {
                return Err("invalid PAM payload".into());
            }
            (width, height, place.0, place.1, false, raw.to_vec())
        }
        _ => return Err("unsupported image format".into()),
    };
    if width == 0 || height == 0 {
        return Err("invalid image dimensions".into());
    }
    Ok(Image {
        width,
        height,
        pixels,
        left,
        top,
        pgm,
    })
}

pub fn blank(width: usize, height: usize) -> Result<Image, String> {
    if width == 0 || height == 0 {
        return Err("invalid canvas dimensions".into());
    }
    let len = width
        .checked_mul(height)
        .and_then(|x| x.checked_mul(4))
        .ok_or("invalid canvas dimensions")?;
    Ok(Image {
        width,
        height,
        pixels: vec![255; len],
        left: 0,
        top: 0,
        pgm: false,
    })
}

pub fn copy(canvas: &mut Image, source: &Image, x: usize, y: usize) -> Result<(), String> {
    if x.checked_add(source.width)
        .is_none_or(|end| end > canvas.width)
        || y.checked_add(source.height)
            .is_none_or(|end| end > canvas.height)
    {
        return Err("copy outside canvas".into());
    }
    let stride = source.width * 4;
    for row in 0..source.height {
        let dst = ((y + row) * canvas.width + x) * 4;
        let src = row * stride;
        canvas.pixels[dst..dst + stride].copy_from_slice(&source.pixels[src..src + stride]);
    }
    Ok(())
}

pub fn blit(canvas: &mut Image, source: &Image, x: i32, y: i32, gray_ink: bool) {
    for sy in 0..source.height {
        let cy = y as i64 + sy as i64;
        if cy < 0 || cy >= canvas.height as i64 {
            continue;
        }
        for sx in 0..source.width {
            let cx = x as i64 + sx as i64;
            if cx < 0 || cx >= canvas.width as i64 {
                continue;
            }
            let si = (sy * source.width + sx) * 4;
            let di = (cy as usize * canvas.width + cx as usize) * 4;
            let (mut sr, mut sg, mut sb, mut sa) = (
                source.pixels[si] as u32,
                source.pixels[si + 1] as u32,
                source.pixels[si + 2] as u32,
                source.pixels[si + 3] as u32,
            );
            if gray_ink {
                sa = 255 - sr;
                sr = 0;
                sg = 0;
                sb = 0;
            }
            if sa == 0 {
                continue;
            }
            let da = canvas.pixels[di + 3] as u32;
            let out_a = sa + (da * (255 - sa) + 127) / 255;
            for (i, s) in [sr, sg, sb].into_iter().enumerate() {
                let d = canvas.pixels[di + i] as u32;
                canvas.pixels[di + i] = ((s * sa * 255 + d * da * (255 - sa) + out_a * 127)
                    / (out_a * 255))
                    .min(255) as u8;
            }
            canvas.pixels[di + 3] = out_a as u8;
        }
    }
}

pub fn compose(glyphs: &[Glyph]) -> Result<Option<Image>, String> {
    let (mut pen_x, mut pen_y) = (0i64, 0i64);
    let mut positioned = Vec::new();
    for glyph in glyphs {
        let x = ((pen_x + i64::from(glyph.dx.0)) as f64 / 64.0).round() as i32;
        let y = -(((pen_y + i64::from(glyph.dy.0)) as f64 / 64.0).round() as i32);
        pen_x += i64::from(glyph.ax.0);
        pen_y += i64::from(glyph.ay.0);
        if let Some(image) = &glyph.image {
            positioned.push((image, x + image.left, y - image.top));
        }
    }
    if positioned.is_empty() {
        return Ok(None);
    }
    let left = positioned.iter().map(|(_, x, _)| *x).min().unwrap();
    let top = positioned.iter().map(|(_, _, y)| *y).min().unwrap();
    let right = positioned
        .iter()
        .map(|(im, x, _)| *x + im.width as i32)
        .max()
        .unwrap();
    let bottom = positioned
        .iter()
        .map(|(im, _, y)| *y + im.height as i32)
        .max()
        .unwrap();
    let mut canvas = blank((right - left) as usize, (bottom - top) as usize)?;
    for (image, x, y) in positioned {
        blit(&mut canvas, image, x - left, y - top, image.pgm);
    }
    canvas.left = left;
    canvas.top = -top;
    Ok(Some(canvas))
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(kind);
    crc_data.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
}

fn zlib_store(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65_535).enumerate() {
        let last = (i + 1) * 65_535 >= raw.len();
        out.push(u8::from(last));
        let n = block.len() as u16;
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(!n).to_le_bytes());
        out.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in raw {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

pub fn png(image: &Image) -> Vec<u8> {
    let mut raw = Vec::with_capacity(image.height * (image.width * 4 + 1));
    for row in 0..image.height {
        raw.push(0);
        let start = row * image.width * 4;
        raw.extend_from_slice(&image.pixels[start..start + image.width * 4]);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(image.width as u32).to_be_bytes());
    ihdr.extend_from_slice(&(image.height as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_store(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}
