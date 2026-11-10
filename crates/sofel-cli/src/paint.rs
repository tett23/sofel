//! `sofel-paint`: write the paint commands of a color glyph as text
//! (ADR 0023), in the indented form of HarfBuzz's `test/api/test-paint.c`.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::Write;

use sofel_color::{PaintOptions, Painted, Palette, Scratch, paint};
use sofel_outline::font::Font;
use sofel_outline::font::geom::{
    Brush, Color, ColorLine, CompositeMode, Extend, Image, ImageFormat, Painter, Rect, Solid,
    Transform, normalize_color_line, reduce_linear_anchors,
};

/// The usage text, printed for `--help` and after a usage error.
pub const HELP: &str = "\
usage: sofel-paint [--face N] [--palette P] [--foreground RRGGBBAA]
                   [--ppem P] [--exact] (--glyph G | --char U+XXXX) FILE

Writes the paint commands of a color glyph (COLR, SVG, CBDT or sbix) to
standard output, one per line and indented by nesting, as HarfBuzz's
test/api/test-paint.c prints them. Numbers are written as C's %.3g;
with --exact, each f32 is written as its bits in hex (0x%08x).

  --face N          the font in a collection (default 0)
  --palette P       the CPAL palette (default 0)
  --foreground C    the foreground color as hex RRGGBBAA (default 000000ff)
  --ppem P          the pixels per em for bitmap strikes (default 0: the
                    largest)
  --exact           write numbers exactly
  --glyph G         the glyph id
  --char U+XXXX     the character, mapped through the font's cmap

Exit status:
  0  the commands were written
  1  the font cannot be read, or the glyph is not a color glyph
  2  wrong usage
";

/// C's `printf("%.3g", v)` for a finite or infinite `v`.
pub fn format_g3(v: f64) -> String {
    format_g(v, 3)
}

/// C's `printf("%.<precision>g", v)`.
pub fn format_g(v: f64, precision: usize) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() { "-nan" } else { "nan" }.into();
    }
    if v.is_infinite() {
        return if v < 0.0 { "-inf" } else { "inf" }.into();
    }
    let p = precision.max(1);
    if v == 0.0 {
        return if v.is_sign_negative() { "-0" } else { "0" }.into();
    }
    // The exponent after rounding to p significant digits.
    let sci = format!("{:.*e}", p.saturating_sub(1), v);
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let x: i32 = exp.parse().unwrap_or(0);
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    let p_i = i32::try_from(p).unwrap_or(i32::MAX);
    if x < p_i && x >= -4 {
        let decimals = usize::try_from(p_i.saturating_sub(1).saturating_sub(x)).unwrap_or(0);
        strip(&format!("{v:.decimals$}"))
    } else {
        let m = strip(mantissa);
        let sign = if x < 0 { '-' } else { '+' };
        format!("{m}e{sign}{:02}", x.unsigned_abs())
    }
}

/// A [`Painter`] that writes HarfBuzz's test text.
pub struct PaintText {
    /// The text so far.
    pub out: String,
    level: usize,
    exact: bool,
}

impl PaintText {
    /// An empty text; with `exact`, numbers are written as their bits.
    pub fn new(exact: bool) -> PaintText {
        PaintText {
            out: String::new(),
            level: 0,
            exact,
        }
    }

    fn num(&self, v: f32) -> String {
        if self.exact {
            format!("{:#010x}", v.to_bits())
        } else {
            format_g3(f64::from(v))
        }
    }

    fn line(&mut self, s: &str) {
        let _ = writeln!(
            self.out,
            "{:width$}{s}",
            "",
            width = self.level.saturating_mul(2)
        );
    }

    fn color(c: &Color) -> String {
        format!("{} {} {} {}", c.r, c.g, c.b, c.a)
    }

    fn stops(&mut self, extend: Extend, stops: &[sofel_outline::font::geom::ColorStop]) {
        let e = match extend {
            Extend::Pad => 0,
            Extend::Repeat => 1,
            Extend::Reflect => 2,
        };
        self.line(&format!("colors {e}"));
        self.level = self.level.saturating_add(1);
        for s in stops {
            let l = format!("{} {}", self.num(s.offset), Self::color(&s.color.color));
            self.line(&l);
        }
        self.level = self.level.saturating_sub(1);
    }

    fn line_stops(&mut self, line: &ColorLine<'_>) {
        self.stops(line.extend, line.stops);
    }
}

/// `clean_float` of test-paint.c: -0 becomes 0.
fn clean(v: f32) -> f32 {
    if v == 0.0 { 0.0 } else { v }
}

impl Painter for PaintText {
    fn push_transform(&mut self, t: Transform) {
        let v = [t.xx, t.yx, t.xy, t.yy, t.dx, t.dy].map(|v| self.num(clean(v)));
        self.line(&format!("start transform {}", v.join(" ")));
        self.level = self.level.saturating_add(1);
    }
    fn pop_transform(&mut self) {
        self.level = self.level.saturating_sub(1);
        self.line("end transform");
    }
    fn push_clip_glyph(&mut self, glyph: u32) {
        self.line(&format!("start clip glyph {glyph}"));
        self.level = self.level.saturating_add(1);
    }
    fn push_clip_rectangle(&mut self, r: Rect) {
        let l = format!(
            "start clip rectangle {} {} {} {}",
            self.num(r.x_min),
            self.num(r.y_min),
            self.num(r.x_max),
            self.num(r.y_max)
        );
        self.line(&l);
        self.level = self.level.saturating_add(1);
    }
    fn pop_clip(&mut self) {
        self.level = self.level.saturating_sub(1);
        self.line("end clip");
    }
    fn fill(&mut self, brush: &Brush<'_>) {
        match brush {
            Brush::Solid(s) => self.line(&format!("solid {}", Self::color(&s.color))),
            Brush::Linear { line, p0, p1, p2 } => {
                let (a, b) = reduce_linear_anchors(*p0, *p1, *p2);
                let mut stops = line.stops.to_vec();
                let (mn, mx) = normalize_color_line(&mut stops);
                let p = |t: f32| (a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
                let ((x0, y0), (x1, y1)) = (p(mn), p(mx));
                self.line("linear gradient");
                self.level = self.level.saturating_add(1);
                let l0 = format!("p0 {} {}", self.num(x0), self.num(y0));
                let l1 = format!("p1 {} {}", self.num(x1), self.num(y1));
                self.line(&l0);
                self.line(&l1);
                self.stops(line.extend, &stops);
                self.level = self.level.saturating_sub(1);
            }
            Brush::Radial {
                line,
                c0,
                r0,
                c1,
                r1,
            } => {
                self.line("radial gradient");
                self.level = self.level.saturating_add(1);
                let l0 = format!(
                    "p0 {} {} radius {}",
                    self.num(c0.x),
                    self.num(c0.y),
                    self.num(*r0)
                );
                let l1 = format!(
                    "p1 {} {} radius {}",
                    self.num(c1.x),
                    self.num(c1.y),
                    self.num(*r1)
                );
                self.line(&l0);
                self.line(&l1);
                self.line_stops(line);
                self.level = self.level.saturating_sub(1);
            }
            Brush::Sweep {
                line,
                center,
                start,
                end,
            } => {
                self.line("sweep gradient");
                self.level = self.level.saturating_add(1);
                let l0 = format!("center {} {}", self.num(center.x), self.num(center.y));
                let l1 = format!("angles {} {}", self.num(*start), self.num(*end));
                self.line(&l0);
                self.line(&l1);
                self.line_stops(line);
                self.level = self.level.saturating_sub(1);
            }
        }
    }
    fn fill_glyph(&mut self, glyph: u32, color: &Solid) {
        self.line(&format!(
            "fill glyph {glyph} solid {}",
            Self::color(&color.color)
        ));
    }
    fn image(&mut self, image: &Image<'_>) -> bool {
        let tag = match image.format {
            ImageFormat::Png => "png ",
            ImageFormat::Svg { .. } => "svg ",
        };
        let extents = image.extents.map_or("-".to_string(), |e| {
            format!("{} {} {} {}", e.x_bearing, e.y_bearing, e.width, e.height)
        });
        let l = format!(
            "image type {tag} size {} {} slant {} extents {extents}\n",
            image.width,
            image.height,
            self.num(0.0)
        );
        self.line(&l);
        true
    }
    fn push_group(&mut self, _mode: CompositeMode) {
        self.line("push group");
        self.level = self.level.saturating_add(1);
    }
    fn pop_group(&mut self, mode: CompositeMode) {
        self.level = self.level.saturating_sub(1);
        self.line(&format!("pop group mode {}", mode as u8));
    }
    fn color_glyph(&mut self, glyph: u32) -> bool {
        self.line(&format!("paint color glyph {glyph}; acting as failed"));
        false
    }
}

enum Target {
    Glyph(u32),
    Char(u32),
}

struct Args {
    face: u32,
    palette: u16,
    foreground: Color,
    ppem: u16,
    exact: bool,
    target: Target,
    file: OsString,
}

fn parse(args: &[OsString]) -> Result<Option<Args>, String> {
    let mut a = Args {
        face: 0,
        palette: 0,
        foreground: Color::new(0, 0, 0, 255),
        ppem: 0,
        exact: false,
        target: Target::Glyph(0),
        file: OsString::new(),
    };
    let (mut target, mut file) = (None, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .and_then(|v| v.to_str())
                .map(str::to_string)
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.to_str() {
            Some("--help" | "-h") => return Ok(None),
            Some("--exact") => a.exact = true,
            Some("--face") => a.face = value("--face")?.parse().map_err(|_| "bad --face")?,
            Some("--palette") => {
                a.palette = value("--palette")?.parse().map_err(|_| "bad --palette")?;
            }
            Some("--ppem") => a.ppem = value("--ppem")?.parse().map_err(|_| "bad --ppem")?,
            Some("--foreground") => {
                let v = value("--foreground")?;
                let n = (v.len() == 8)
                    .then(|| u32::from_str_radix(&v, 16).ok())
                    .flatten()
                    .ok_or("bad --foreground")?;
                let [r, g, b, al] = n.to_be_bytes();
                a.foreground = Color::new(r, g, b, al);
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
                if file.replace(arg.clone()).is_some() {
                    return Err("give one font file".into());
                }
            }
        }
    }
    a.target = target.ok_or("give --glyph or --char")?;
    a.file = file.ok_or("give a font file")?;
    Ok(Some(a))
}

/// Runs `sofel-paint` with `args` (without the program name); returns
/// the exit status.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let a = match parse(args) {
        Ok(Some(a)) => a,
        Ok(None) => {
            let _ = out.write_all(HELP.as_bytes());
            return 0;
        }
        Err(e) => {
            let _ = writeln!(err, "sofel-paint: {e}\n\n{HELP}");
            return 2;
        }
    };
    let data = match std::fs::read(&a.file) {
        Ok(d) => d,
        Err(e) => {
            let _ = writeln!(err, "sofel-paint: {}: {e}", a.file.to_string_lossy());
            return 1;
        }
    };
    let font = match Font::new(&data, a.face) {
        Ok(f) => f,
        Err(e) => {
            let _ = writeln!(err, "sofel-paint: {e}");
            return 1;
        }
    };
    let glyph = match a.target {
        Target::Glyph(g) => Some(g),
        Target::Char(c) => font.nominal_glyph(c).map(|g| g.0),
    };
    let Some(glyph) = glyph else {
        let _ = writeln!(err, "sofel-paint: the font has no glyph for that character");
        return 1;
    };
    let options = PaintOptions {
        palette: Palette::Font(a.palette),
        foreground: a.foreground,
        ppem: a.ppem,
    };
    let mut text = PaintText::new(a.exact);
    let painted = paint(&font, glyph, &options, &mut Scratch::default(), &mut text);
    if painted == Painted::None {
        let _ = writeln!(err, "sofel-paint: glyph {glyph} is not a color glyph");
        return 1;
    }
    if out.write_all(text.out.as_bytes()).is_err() {
        return 1;
    }
    0
}
