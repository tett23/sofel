//! Inspect a glyph slot loaded through the automatic hinter (root ADR 0085).

use std::ffi::OsString;
use std::io::Write;

use sofel_font::Font;
use sofel_font::read::GlyphId;
use sofel_outline::{
    AutoHintCache, AutoHintShapedCluster, AutoHintShapedGlyph, AutoHintShapingData, HintTarget,
    Outlines, Scratch,
};

/// Command-line usage for `sofel hint`.
pub const HELP: &str = "usage: sofel hint [--gsub] [--face N] [--variations LIST] [--named-instance N] --ppem P --glyph G [--target normal|light|mono|lcd|lcd-v] FILE\n\nLoads one glyph with the automatic hinter and prints the slot's advance, points, tags and contour ends. The default style coverage uses the Unicode cmap only; --gsub also uses GSUB and shaped blue zones.\n";

struct Args {
    face: u32,
    location: crate::variation_options::LocationOptions,
    ppem: u16,
    glyph: u32,
    target: HintTarget,
    gsub: bool,
    file: OsString,
}

fn parse(args: &[OsString]) -> Result<Option<Args>, String> {
    let (mut face, mut ppem, mut glyph, mut file) = (0, None, None, None);
    let mut target = HintTarget::Normal;
    let mut gsub = false;
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
    Ok(Some(Args {
        face,
        location,
        ppem,
        glyph: glyph.ok_or("--glyph is required")?,
        target,
        gsub,
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
    let font = match Font::new(&bytes, args.face) {
        Ok(font) => font,
        Err(error) => return fail(err, error.to_string()),
    };
    let location = args.location.resolve(&font);
    if let Some(error) = location.ft_error() {
        return fail(err, format!("invalid variation location: {error:?}"));
    }
    let outlines = Outlines::new(font.instance(&location));
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
