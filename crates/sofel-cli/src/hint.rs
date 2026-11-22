//! Inspect a glyph slot after the selected load engine (root ADR 0064 and 0085).

use std::ffi::OsString;
use std::io::Write;

use sofel_font::read::GlyphId;
use sofel_font::{CidFace, Font, Type1Face, Type1Source};
use sofel_geom::{Command, Fixed, FnPen, Point, div_fix, mul_fix};
use sofel_outline::{
    AutoHintCache, AutoHintShapedCluster, AutoHintShapedGlyph, AutoHintShapingData, HintMode,
    HintTarget, Outlines, Scale, Scratch, draw_cid, draw_cid_native, draw_type1, draw_type1_native,
};

/// Command-line usage for `sofel hint`.
pub const HELP: &str = "usage: sofel hint [--engine auto|native|none] [--gsub] [--face N] [--variations LIST] [--named-instance N] --ppem P --glyph G [--target normal|light|mono|lcd|lcd-v] FILE\n\nLoads one glyph and prints the slot's advance, points, tags and contour ends. The default engine is auto. With auto, style coverage uses the Unicode cmap only; --gsub also uses GSUB and shaped blue zones.\n";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Engine {
    Auto,
    Native,
    None,
}

struct Args {
    face: u32,
    location: crate::variation_options::LocationOptions,
    ppem: u16,
    glyph: u32,
    target: HintTarget,
    gsub: bool,
    engine: Engine,
    file: OsString,
}

fn parse(args: &[OsString]) -> Result<Option<Args>, String> {
    let (mut face, mut ppem, mut glyph, mut file) = (0, None, None, None);
    let mut target = HintTarget::Normal;
    let mut gsub = false;
    let mut engine = Engine::Auto;
    let mut location = crate::variation_options::LocationOptions::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| -> Result<String, String> {
            it.next()
                .and_then(|item| item.to_str())
                .map(str::to_owned)
                .ok_or_else(|| format!("{name} needs a value"))
        };
        if location.consume(arg.to_str(), &mut value)? {
            continue;
        }
        let (name, inline) = arg
            .to_str()
            .map(|s| s.split_once('=').map_or((s, None), |(a, b)| (a, Some(b))))
            .unwrap_or(("", None));
        let mut next = |name: &str| {
            inline
                .map(str::to_owned)
                .map(Ok)
                .unwrap_or_else(|| value(name))
        };
        match name {
            "--help" | "-h" => return Ok(None),
            "--gsub" => gsub = true,
            "--engine" => {
                engine = match next(name)?.as_str() {
                    "auto" => Engine::Auto,
                    "native" => Engine::Native,
                    "none" => Engine::None,
                    _ => return Err("bad --engine".into()),
                }
            }
            "--face" => face = next(name)?.parse().map_err(|_| "bad --face")?,
            "--ppem" => ppem = Some(next(name)?.parse().map_err(|_| "bad --ppem")?),
            "--glyph" => glyph = Some(next(name)?.parse().map_err(|_| "bad --glyph")?),
            "--target" => {
                target = match next(name)?.as_str() {
                    "normal" => HintTarget::Normal,
                    "light" => HintTarget::Light,
                    "mono" => HintTarget::Mono,
                    "lcd" => HintTarget::Lcd,
                    "lcd-v" => HintTarget::VerticalLcd,
                    _ => return Err("bad --target".into()),
                }
            }
            _ if name.starts_with('-') => return Err(format!("unknown option {name}")),
            _ => {
                if file.replace(arg.clone()).is_some() {
                    return Err("give one file".into());
                }
            }
        }
    }
    let ppem = ppem.ok_or("--ppem is required")?;
    if ppem == 0 {
        return Err("--ppem must be positive".into());
    }
    if gsub && engine != Engine::Auto {
        return Err("--gsub requires --engine auto".into());
    }
    Ok(Some(Args {
        face,
        location,
        ppem,
        glyph: glyph.ok_or("--glyph is required")?,
        target,
        gsub,
        engine,
        file: file.ok_or("no file given")?,
    }))
}

/// Builds a cache with GSUB coverage and shaped auto-hinter data.
pub fn gsub_cache(outlines: &Outlines<'_, '_, '_>) -> Result<AutoHintCache, String> {
    let specs = AutoHintCache::style_specs();
    let requests: Vec<_> = specs
        .iter()
        .map(|style| sofel_shape::AutoHintStyleRequest {
            index: style.index,
            script: style.script,
            ranges: style.ranges,
            nonbase_ranges: style.nonbase_ranges,
            feature: style.feature,
            blue_strings: &style.blue_strings,
            standard: style.standard,
        })
        .collect();
    let (glyph_styles, clusters) = sofel_shape::auto_hint_shaping_data(
        outlines.instance(),
        &|glyph| outlines.glyph_extents(glyph),
        &requests,
        86,
        57,
    );
    let reverse = sofel_shape::auto_hint_reverse_entries(
        outlines.font(),
        &requests,
        &AutoHintCache::adjustment_codepoints(),
    );
    let data = AutoHintShapingData {
        glyph_styles,
        reverse,
        clusters: clusters
            .into_iter()
            .map(|cluster| AutoHintShapedCluster {
                style: cluster.style,
                text: cluster.text,
                glyphs: cluster
                    .glyphs
                    .into_iter()
                    .map(|glyph| AutoHintShapedGlyph {
                        glyph: glyph.glyph,
                        x_advance: glyph.x_advance,
                        y_offset: glyph.y_offset,
                    })
                    .collect(),
            })
            .collect(),
    };
    AutoHintCache::new(outlines)
        .with_shaping_data(data)
        .ok_or("invalid GSUB style count".into())
}

fn postscript_slot(commands: &[Command], advance: i32, out: &mut dyn Write) -> i32 {
    let mut points: Vec<(Point, u8)> = Vec::new();
    let mut ends = Vec::new();
    let mut contour_start = 0;
    for &command in commands {
        match command {
            Command::MoveTo(point) => {
                contour_start = points.len();
                points.push((point, 1));
            }
            Command::LineTo(point) => points.push((point, 1)),
            Command::QuadTo(control, point) => {
                points.push((control, 0));
                points.push((point, 1));
            }
            Command::CubicTo(first, second, point) => {
                points.push((first, 2));
                points.push((second, 2));
                points.push((point, 1));
            }
            Command::Close => {
                if points.len() > contour_start.saturating_add(1)
                    && points.last() == points.get(contour_start)
                {
                    points.pop();
                }
                if let Some(end) = points.len().checked_sub(1) {
                    ends.push(end);
                }
            }
        }
    }
    if writeln!(out, "advance {advance} 0").is_err() {
        return 1;
    }
    for (point, tag) in points {
        if writeln!(out, "point {} {} {tag}", point.x, point.y).is_err() {
            return 1;
        }
    }
    for end in ends {
        if writeln!(out, "end {end}").is_err() {
            return 1;
        }
    }
    0
}

fn postscript_hint(
    args: &Args,
    bytes: &[u8],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Option<i32> {
    if let Ok(face) = CidFace::parse(bytes, 67_108_864) {
        if args.face != 0 {
            let _ = writeln!(err, "sofel hint: CID face index out of range");
            return Some(1);
        }
        let factor = Fixed(div_fix(
            i32::from(args.ppem).saturating_mul(64),
            i32::from(face.units_per_em()),
        ));
        let scale = Scale::Scaled {
            x: factor,
            y: factor,
        };
        let mut commands = Vec::new();
        let glyph = usize::try_from(args.glyph).unwrap_or(usize::MAX);
        let loaded = match args.engine {
            Engine::Native => draw_cid_native(
                &face,
                glyph,
                scale,
                args.target,
                &mut FnPen(|c| commands.push(c)),
            )
            .map(|(_, advance)| advance),
            Engine::None => {
                if face.glyph_is_empty(glyph) == Some(true) {
                    Ok(0)
                } else {
                    draw_cid(&face, glyph, scale, &mut FnPen(|c| commands.push(c))).map(|_| {
                        face.glyph_metrics(glyph)
                            .map_or(0, |metrics| mul_fix(metrics.advance_x, factor.0))
                    })
                }
            }
            Engine::Auto => Err(sofel_outline::OutlineError::AutoHintUnsupported),
        };
        return Some(match loaded {
            Ok(advance) => postscript_slot(&commands, advance, out),
            Err(error) => {
                let _ = writeln!(err, "sofel hint: {error:?}");
                1
            }
        });
    }
    let source = if Type1Source::has_text_header(bytes) {
        Type1Source::parse_pfa(bytes, 67_108_864)
    } else {
        Type1Source::parse_pfb(bytes, 67_108_864)
    };
    let Ok(source) = source else {
        return None;
    };
    let Ok(face) = Type1Face::new(&source) else {
        let _ = writeln!(err, "sofel hint: invalid Type 1 face");
        return Some(1);
    };
    if args.face != 0 {
        let _ = writeln!(err, "sofel hint: Type 1 face index out of range");
        return Some(1);
    }
    let factor = Fixed(div_fix(i32::from(args.ppem).saturating_mul(64), 1000));
    let scale = Scale::Scaled {
        x: factor,
        y: factor,
    };
    let mut commands = Vec::new();
    let glyph = usize::try_from(args.glyph).unwrap_or(usize::MAX);
    let loaded = match args.engine {
        Engine::Native => draw_type1_native(
            &face,
            glyph,
            scale,
            args.target,
            &mut FnPen(|c| commands.push(c)),
        )
        .map(|(_, advance)| advance),
        Engine::None => {
            draw_type1(&face, glyph, scale, &mut FnPen(|c| commands.push(c))).map(|_| {
                face.glyph_metrics(glyph)
                    .map_or(0, |metrics| mul_fix(metrics.advance_x, factor.0))
            })
        }
        Engine::Auto => Err(sofel_outline::OutlineError::AutoHintUnsupported),
    };
    Some(match loaded {
        Ok(advance) => postscript_slot(&commands, advance, out),
        Err(error) => {
            let _ = writeln!(err, "sofel hint: {error:?}");
            1
        }
    })
}

/// Runs `sofel hint`, returning a process exit status.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let args = match parse(args) {
        Ok(Some(args)) => args,
        Ok(None) => {
            let _ = out.write_all(HELP.as_bytes());
            return 0;
        }
        Err(message) => {
            let _ = writeln!(err, "sofel hint: {message}\n{HELP}");
            return 2;
        }
    };
    let fail = |err: &mut dyn Write, message: String| {
        let _ = writeln!(err, "sofel hint: {message}");
        1
    };
    let bytes = match std::fs::read(&args.file) {
        Ok(bytes) => bytes,
        Err(error) => return fail(err, error.to_string()),
    };
    let bytes = match crate::font_input::unpack(bytes) {
        Ok(bytes) => bytes,
        Err(error) => return fail(err, format!("{} at byte {}", error.kind, error.offset)),
    };
    if let Some(status) = postscript_hint(&args, &bytes, out, err) {
        return status;
    }
    let font = match Font::new(&bytes, args.face) {
        Ok(font) => font,
        Err(error) => return fail(err, error.to_string()),
    };
    let location = args.location.resolve(&font);
    if let Some(error) = location.ft_error() {
        return fail(err, format!("invalid variation location: {error:?}"));
    }
    let outlines = Outlines::new(font.instance(&location));
    if args.engine != Engine::Auto {
        let mode = match args.engine {
            Engine::Native => HintMode::NativeV40,
            Engine::None => HintMode::None,
            Engine::Auto => unreachable!(),
        };
        let mut scratch = Scratch::default();
        let (_, advance) = match outlines.load_with_hint(
            GlyphId(args.glyph),
            args.ppem,
            mode,
            args.target,
            &mut scratch,
        ) {
            Ok(result) => result,
            Err(error) => return fail(err, format!("{error:?}")),
        };
        return write_slot(out, advance, &scratch);
    }
    let mut cache = if args.gsub {
        match gsub_cache(&outlines) {
            Ok(cache) => cache,
            Err(message) => return fail(err, message),
        }
    } else {
        AutoHintCache::new(&outlines)
    };
    let mut scratch = Scratch::default();
    let (_, advance) = match outlines.load_auto_hinted(
        GlyphId(args.glyph),
        args.ppem,
        args.target,
        &mut cache,
        &mut scratch,
    ) {
        Ok(result) => result,
        Err(error) => return fail(err, format!("{error:?}")),
    };
    write_slot(out, advance, &scratch)
}

fn write_slot(out: &mut dyn Write, advance: i32, scratch: &Scratch) -> i32 {
    if writeln!(out, "advance {advance} 0").is_err() {
        return 1;
    }
    for (point, tag) in scratch.points().iter().zip(scratch.point_tags()) {
        if writeln!(out, "point {} {} {tag}", point.x, point.y).is_err() {
            return 1;
        }
    }
    for end in scratch.contour_ends() {
        if writeln!(out, "end {end}").is_err() {
            return 1;
        }
    }
    0
}
