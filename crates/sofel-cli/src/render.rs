//! `sofel-render`: render one glyph to a PGM image (ADR 0012).

use std::ffi::OsString;
use std::io::Write;

use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_outline::{Outlines, Scratch};
use sofel_raster::Rasterizer;

use crate::glyph;

/// The usage text, printed for `--help` and after a usage error.
pub const HELP: &str = "\
usage: sofel-render [--face N] --ppem P [--no-bitmap] [--color [--palette N]]
                    (--glyph G | --char U+XXXX) FILE

Renders one glyph without hinting, as FreeType's FT_Render_Glyph with
FT_RENDER_MODE_NORMAL does, and writes a PGM image (P5, 8-bit gray,
black ink on white) to standard output. The bitmap position is in a
comment line: `# left <x> top <y>`.

When the font has a bitmap strike of P pixels per em and the glyph has a
bitmap in it, the bitmap is written instead, as FreeType loads it
(ADR 0023): 1, 2, 4 and 8-bit bitmaps as PGM, 32-bit ones as PAM (P7,
TUPLTYPE RGB_ALPHA, not premultiplied).

With --color, a glyph with COLR version 0 layers is drawn in color as
FreeType's FT_LOAD_COLOR does (ADR 0023) and written as PAM; the color
of palette entry 0xFFFF is white when the palette is marked for dark
backgrounds, else black.

  --variations LIST   design coordinates, e.g. wght=500
  --named-instance N  start at named instance N (zero-based)
  --face N      the font in a collection (default 0)
  --ppem P      the size in pixels per em
  --no-bitmap   draw the outline even when there is a bitmap
                (FT_LOAD_NO_BITMAP)
  --color       draw the COLR version 0 layers in color
  --palette N   the CPAL palette for --color (default 0)
  --glyph G     the glyph id
  --char U+XXXX the character, mapped through the font's cmap

Exit status:
  0  the image was written
  1  the font, character or glyph cannot be rendered, or the glyph is
     empty
  2  wrong usage
";

struct Args {
    location: crate::variation_options::LocationOptions,
    face: u32,
    ppem: u16,
    no_bitmap: bool,
    color: bool,
    palette: u16,
    target: Target,
    file: OsString,
}

enum Target {
    Glyph(u32),
    Char(u32),
}

fn parse(args: &[OsString]) -> Result<Option<Args>, String> {
    let (mut face, mut ppem, mut target, mut file) = (0u32, None, None, None);
    let mut location = crate::variation_options::LocationOptions::default();
    let mut no_bitmap = false;
    let (mut color, mut palette) = (false, 0u16);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .and_then(|v| v.to_str())
                .map(str::to_string)
                .ok_or_else(|| format!("{name} needs a value"))
        };
        if location.consume(a.to_str(), &mut value)? {
            continue;
        }
        match a.to_str() {
            Some("--help" | "-h") => return Ok(None),
            Some("--face") => face = value("--face")?.parse().map_err(|_| "bad --face")?,
            Some("--no-bitmap") => no_bitmap = true,
            Some("--color") => color = true,
            Some("--palette") => {
                palette = value("--palette")?.parse().map_err(|_| "bad --palette")?;
            }
            Some("--ppem") => {
                let p: u16 = value("--ppem")?.parse().map_err(|_| "bad --ppem")?;
                if p == 0 {
                    return Err("--ppem must be positive".into());
                }
                ppem = Some(p);
            }
            Some(o @ ("--glyph" | "--char")) => {
                if target.is_some() {
                    return Err("give one of --glyph and --char".into());
                }
                let v = value(o)?;
                target = Some(if o == "--glyph" {
                    Target::Glyph(v.parse().map_err(|_| "bad --glyph")?)
                } else {
                    let hex = v
                        .strip_prefix("U+")
                        .or_else(|| v.strip_prefix("u+"))
                        .ok_or("bad --char")?;
                    Target::Char(u32::from_str_radix(hex, 16).map_err(|_| "bad --char")?)
                });
            }
            Some(s) if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option {s}"));
            }
            _ => {
                if file.replace(a.clone()).is_some() {
                    return Err("give one file".into());
                }
            }
        }
    }
    Ok(Some(Args {
        location,
        face,
        no_bitmap,
        color,
        palette,
        ppem: ppem.ok_or("--ppem is required")?,
        target: target.ok_or("give --glyph or --char")?,
        file: file.ok_or("no file given")?,
    }))
}

/// Runs the command with `args` (without the program name). Returns the
/// exit status.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let args = match parse(args) {
        Ok(Some(a)) => a,
        Ok(None) => {
            let _ = out.write_all(HELP.as_bytes());
            return 0;
        }
        Err(e) => {
            let _ = writeln!(err, "sofel-render: {e}\n\n{HELP}");
            return 2;
        }
    };
    let fail = |err: &mut dyn Write, msg: String| {
        let _ = writeln!(err, "sofel-render: {msg}");
        1
    };
    let bytes = match std::fs::read(&args.file) {
        Ok(b) => b,
        Err(e) => return fail(err, format!("{}: {e}", args.file.to_string_lossy())),
    };
    let font = match Font::new(&bytes, args.face) {
        Ok(f) => f,
        Err(e) => return fail(err, format!("{}: {e}", args.file.to_string_lossy())),
    };
    let location = args.location.resolve(&font);
    if let Some(error) = location.ft_error() {
        return fail(err, format!("invalid variation location: {error:?}"));
    }
    let glyph = match args.target {
        Target::Glyph(g) => GlyphId(g),
        Target::Char(c) => match font.nominal_glyph(c) {
            Some(g) => g,
            None => return fail(err, format!("no glyph for U+{c:04X}")),
        },
    };
    if !args.no_bitmap
        && location.ft_default()
        && let Some(image) = bitmap_image(&font, glyph.0, args.ppem)
    {
        if out.write_all(&image).is_err() {
            return fail(err, "cannot write the image".into());
        }
        return 0;
    }
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let outlines = Outlines::new(font.instance(&location));
    let rendered = if args.color {
        let options = color_options(&font, args.palette);
        let mut image = sofel_raster::BgraImage::default();
        match glyph::render_color(
            &outlines,
            glyph,
            args.ppem,
            &options,
            &mut scratch,
            &mut raster,
            &mut image,
        ) {
            Ok(glyph::ColorGlyph::Color) => {
                if image.width == 0 || image.rows == 0 {
                    return fail(err, format!("glyph {} is empty", glyph.0));
                }
                let pitch = usize::try_from(image.width).unwrap_or(0).saturating_mul(4);
                let pam = pam_image(
                    image.left,
                    image.top,
                    image.width,
                    image.rows,
                    &image.pixels,
                    pitch,
                );
                if out.write_all(&pam).is_err() {
                    return fail(err, "cannot write the image".into());
                }
                return 0;
            }
            Ok(glyph::ColorGlyph::Gray(b)) => Ok(b),
            Err(e) => Err(e),
        }
    } else {
        glyph::render(&outlines, glyph, args.ppem, &mut scratch, &mut raster)
    };
    let b = match rendered {
        Ok(b) => b,
        Err(e) => return fail(err, format!("glyph {}: {e}", glyph.0)),
    };
    if b.width == 0 || b.rows == 0 {
        return fail(err, format!("glyph {} is empty", glyph.0));
    }
    let mut image = format!(
        "P5\n# left {} top {}\n{} {}\n255\n",
        b.left, b.top, b.width, b.rows
    )
    .into_bytes();
    image.extend(b.pixels.iter().map(|v| !v));
    if out.write_all(&image).is_err() {
        return fail(err, "cannot write the image".into());
    }
    0
}

/// The image of `glyph`'s bitmap in the strike of `ppem` pixels per em
/// (FreeType's `FT_Match_Size`), as PGM or PAM; `None` when the font has
/// no such strike or the glyph no bitmap in it.
pub fn bitmap_image(font: &Font<'_>, glyph: u32, ppem: u16) -> Option<Vec<u8>> {
    use sofel_bitmap::{PixelMode, bitmap_layout, draw_bitmap, match_strike};
    let p = u32::from(ppem) * 64;
    let strike = match_strike(font, p, p)?;
    let layout = bitmap_layout(font, &strike, glyph).ok()?;
    let mut buf = vec![0u8; layout.len()];
    let l = draw_bitmap(font, &strike, glyph, &mut buf).ok()?;
    let (w, h, pitch) = (
        usize::from(l.width),
        usize::from(l.rows),
        usize::try_from(l.pitch).unwrap_or(0),
    );
    let m = l.metrics;
    let head = |kind: &str| {
        format!(
            "{kind}\n# left {} top {}\n",
            m.hori_bearing_x, m.hori_bearing_y
        )
    };
    let row = |y: usize| {
        let start = y.saturating_mul(pitch);
        buf.get(start..start.saturating_add(pitch))
            .unwrap_or_default()
    };
    if l.pixel_mode == PixelMode::Bgra {
        return Some(pam_image(
            i32::from(m.hori_bearing_x),
            i32::from(m.hori_bearing_y),
            u32::from(l.width),
            u32::from(l.rows),
            &buf,
            pitch,
        ));
    }
    let (bits, scale): (usize, u8) = match l.pixel_mode {
        PixelMode::Mono => (1, 255),
        PixelMode::Gray2 => (2, 85),
        PixelMode::Gray4 => (4, 17),
        _ => (8, 1),
    };
    let mask = u8::try_from((1u16 << bits).saturating_sub(1)).unwrap_or(255);
    let mut image = format!("{}{w} {h}\n255\n", head("P5")).into_bytes();
    for y in 0..h {
        let r = row(y);
        for x in 0..w {
            let bit = x.saturating_mul(bits);
            let byte = r.get(bit / 8).copied().unwrap_or(0);
            let shift = 8usize.saturating_sub(bits).saturating_sub(bit % 8);
            let v = (byte >> shift) & mask;
            image.push(!(v.wrapping_mul(scale)));
        }
    }
    Some(image)
}

/// The palette and foreground of `--color`: the font's palette `palette`
/// and white for entry 0xFFFF when that palette is marked for dark
/// backgrounds, else black.
fn color_options(font: &Font<'_>, palette: u16) -> sofel_color::PaintOptions<'static> {
    let dark = sofel_color::palette_flags(font, palette) & 2 != 0;
    let v = if dark { 255 } else { 0 };
    sofel_color::PaintOptions {
        palette: sofel_color::Palette::Font(palette),
        foreground: sofel_font::geom::Color::new(v, v, v, 255),
        ppem: 0,
    }
}

/// A PAM image (P7, `RGB_ALPHA`, not premultiplied) of premultiplied
/// BGRA rows `pitch` bytes apart, with the position in a comment line.
fn pam_image(left: i32, top: i32, width: u32, rows: u32, pixels: &[u8], pitch: usize) -> Vec<u8> {
    let mut image = format!(
        "P7\n# left {left} top {top}\nWIDTH {width}\nHEIGHT {rows}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
    )
    .into_bytes();
    let w = usize::try_from(width).unwrap_or(0);
    for y in 0..usize::try_from(rows).unwrap_or(0) {
        let start = y.saturating_mul(pitch);
        let row = pixels
            .get(start..start.saturating_add(pitch))
            .unwrap_or_default();
        for &[b, g, r, a] in row.as_chunks::<4>().0.iter().take(w) {
            let un = |c: u8| {
                let v = u32::from(c)
                    .saturating_mul(255)
                    .saturating_add(u32::from(a) / 2)
                    .checked_div(u32::from(a))
                    .unwrap_or(0);
                u8::try_from(v.min(255)).unwrap_or(255)
            };
            image.extend([un(r), un(g), un(b), a]);
        }
    }
    image
}
