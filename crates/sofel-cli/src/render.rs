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
usage: sofel-render [--face N] --ppem P (--glyph G | --char U+XXXX) FILE

Renders one glyph without hinting, as FreeType's FT_Render_Glyph with
FT_RENDER_MODE_NORMAL does, and writes a PGM image (P5, 8-bit gray,
black ink on white) to standard output. The bitmap position is in a
comment line: `# left <x> top <y>`.

  --face N      the font in a collection (default 0)
  --ppem P      the size in pixels per em
  --glyph G     the glyph id
  --char U+XXXX the character, mapped through the font's cmap

Exit status:
  0  the image was written
  1  the font, character or glyph cannot be rendered, or the glyph is
     empty
  2  wrong usage
";

struct Args {
    face: u32,
    ppem: u16,
    target: Target,
    file: OsString,
}

enum Target {
    Glyph(u32),
    Char(u32),
}

fn parse(args: &[OsString]) -> Result<Option<Args>, String> {
    let (mut face, mut ppem, mut target, mut file) = (0u32, None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .and_then(|v| v.to_str())
                .map(str::to_string)
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match a.to_str() {
            Some("--help" | "-h") => return Ok(None),
            Some("--face") => face = value("--face")?.parse().map_err(|_| "bad --face")?,
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
        face,
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
    let glyph = match args.target {
        Target::Glyph(g) => GlyphId(g),
        Target::Char(c) => match font.nominal_glyph(c) {
            Some(g) => g,
            None => return fail(err, format!("no glyph for U+{c:04X}")),
        },
    };
    let mut scratch = Scratch::default();
    let mut raster = Rasterizer::new();
    let outlines = Outlines::new(&font);
    let b = match glyph::render(&outlines, glyph, args.ppem, &mut scratch, &mut raster) {
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
