//! `sofel-shape`: shape text as HarfBuzz 15.1.0's `hb-shape` does
//! (ADR 0020, `sofel-cli` の `sofel-shape`), with the same options and
//! the same output.
//!
//! The options, their parsing (GLib's `--name=value` and `--name value`
//! forms, C's `strtol` and `sscanf` for numbers), the reading of the
//! text, the cluster renumbering, `--glyphs`, `--verify` and the output
//! follow `util/hb-shape.cc`, `util/shape-options.hh`,
//! `util/font-options.hh`, `util/text-options.hh`,
//! `util/shape-format.hh`, `util/shape-output.hh` and
//! `src/hb-buffer-verify.cc` of HarfBuzz 15.1.0 (MIT).
//!
//! The conformance tests call [`run_with_input`] instead of starting a
//! process.

use std::ffi::OsString;
use std::io::{BufRead, Read, Write};

use crate::variation_options::{parse_named_instance, parse_variations};
use sofel_font::read::GlyphId;
use sofel_font::{
    Font, GlyphExtents, GlyphNames, HbScale,
    var::{Location, Variation},
};
use sofel_outline::{HbExtents, HbOutlineCommand, Scratch};
use sofel_shape::{
    BufferFlags, ClusterLevel, DiffFlags, Direction, Feature, GlyphBuffer, GlyphFlags, GlyphInfo,
    GlyphOutline, GlyphPosition, Language, Scale, Script, SegmentProperties, SerializeFlags,
    SerializeFont, SerializeFormat, ShapePlan, SubFont, UnicodeBuffer, deserialize_glyphs, diff,
    serialize_glyphs_range, serialize_unicode, shape_fallback, shape_fallback_with_subfont,
    shape_with_extents, shape_with_subfont,
};

/// The usage text, printed for `--help` and after a usage error.
pub const HELP: &str = "\
usage: sofel-shape [OPTION...] FONT-FILE [TEXT]

Shapes text with a font as HarfBuzz 15.1.0's hb-shape does and writes
the glyphs in hb-shape's text or JSON form, one line of output per line
of text. Without TEXT, --text, --text-file or --unicodes, the text is
read from standard input.

Text:
  --text=STRING                 the text
  --text-file=FILE              the text, from a file (`-` for stdin)
  -u, --unicodes=HEX...         the text as code points (`U+0041,42`)
  --text-before=STRING          context before each line
  --text-after=STRING           context after each line
  --unicodes-before=HEX...      context before each line, as code points
  --unicodes-after=HEX...       context after each line, as code points
  --single-par                  the text is one paragraph, not lines

Segment:
  --direction=ltr|rtl|ttb|btt   the direction (default: guessed)
  --script=TAG                  the ISO 15924 script (default: guessed)
  --language=BCP47              the language (default: --default-language)
  --default-language=BCP47      the language when --language is absent
                                (default: none; hb-shape reads $LANG)

Features:
  --features=LIST               comma-separated features (`kern,-liga`)

Font:
  -y, --face-index=N            the font in a collection (default 0)
  --font-size=N[,M]|upem        the scale (default: upem, font units)
  --font-ppem=N[,M]             the pixels per em (default 0, none)
  --font-ptem=PT                point size for AAT tracking
  --variations=LIST             comma-separated design coordinates
  --named-instance=N            zero-based named instance
  --font-slant=R                synthetic slant ratio
  --font-bold=X[,Y]             synthetic bold fractions
  --font-grade=X[,Y]            synthetic bold without advance growth
  --font-funcs=NAME             accepted and ignored
  --face-loader=NAME            accepted and ignored

Buffer:
  --bot, --eot                  beginning and end of paragraph
  --preserve-default-ignorables, --remove-default-ignorables
  --invisible-glyph=G           the glyph of default ignorables
  --not-found-glyph=G           the glyph of characters not in the font
  --not-found-variation-selector-glyph=G
  --cluster-level=0|1|2|3       the cluster level (default 0)
  --utf8-clusters               UTF-8 byte offsets as cluster values,
                                not character indices
  --unsafe-to-concat            produce the unsafe-to-concat flag
  --safe-to-insert-tatweel      produce the safe-to-insert-tatweel flag
  --verify                      check the result as HB_BUFFER_FLAG_VERIFY
  --normalize-glyphs            hb_buffer_normalize_glyphs
  --glyphs                      the text is a glyph string to rewrite
  --shaper=ot|fallback, --shapers=LIST
                                the shaper (default: ot)

Output:
  -O, --output-format=text|json
  --no-glyph-names, --no-positions, --no-advances, --no-clusters
  --ned                         no clusters and no advances
  --show-text, --show-unicode, --show-line-num
  --show-flags, --show-extents

Exit status:
  0  the text was shaped
  1  the font or the text file cannot be read
  2  wrong usage, or an option sofel-shape does not support
     (other shapers, ...)
  3  shaping failed (--verify found a problem)
";

/// What a line is shaped with.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shaper {
    Ot,
    Fallback,
}

/// The parsed options (`shape_options_t`, `font_options_t`,
/// `shape_text_options_t`, `shape_format_options_t`).
#[derive(Debug)]
struct Options {
    font_file: Option<OsString>,
    face_index: u32,
    ptem: f32,
    font_size: Option<(f64, f64)>,
    ppem: (i32, i32),
    variations: Vec<Variation>,
    named_instance: Option<u16>,
    slant: f32,
    embolden: (f32, f32),
    embolden_in_place: bool,
    text: Option<Vec<u8>>,
    /// `text_len`: `None` is -1 (`strlen`).
    text_len: Option<usize>,
    text_file: Option<OsString>,
    text_before: Option<Vec<u8>>,
    text_after: Option<Vec<u8>>,
    single_par: bool,
    direction: Option<Vec<u8>>,
    script: Option<Vec<u8>>,
    language: Option<Vec<u8>>,
    default_language: Option<Vec<u8>>,
    features: Vec<Feature>,
    bot: bool,
    eot: bool,
    preserve_default_ignorables: bool,
    remove_default_ignorables: bool,
    invisible_glyph: u32,
    not_found_glyph: u32,
    not_found_variation_selector_glyph: Option<u32>,
    cluster_level: ClusterLevel,
    utf8_clusters: bool,
    unsafe_to_concat: bool,
    safe_to_insert_tatweel: bool,
    verify: bool,
    normalize_glyphs: bool,
    glyphs: bool,
    shaper: Shaper,
    output_format: Option<Vec<u8>>,
    show_glyph_names: bool,
    show_positions: bool,
    show_advances: bool,
    show_clusters: bool,
    show_text: bool,
    show_unicode: bool,
    show_line_num: bool,
    show_extents: bool,
    show_flags: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            font_file: None,
            face_index: 0,
            ptem: 0.,
            font_size: None,
            ppem: (0, 0),
            variations: Vec::new(),
            named_instance: None,
            slant: 0.0,
            embolden: (0.0, 0.0),
            embolden_in_place: false,
            text: None,
            text_len: None,
            text_file: None,
            text_before: None,
            text_after: None,
            single_par: false,
            direction: None,
            script: None,
            language: None,
            default_language: None,
            features: Vec::new(),
            bot: false,
            eot: false,
            preserve_default_ignorables: false,
            remove_default_ignorables: false,
            invisible_glyph: 0,
            not_found_glyph: 0,
            not_found_variation_selector_glyph: None,
            cluster_level: ClusterLevel::MonotoneGraphemes,
            utf8_clusters: false,
            unsafe_to_concat: false,
            safe_to_insert_tatweel: false,
            verify: false,
            normalize_glyphs: false,
            glyphs: false,
            shaper: Shaper::Ot,
            output_format: None,
            show_glyph_names: true,
            show_positions: true,
            show_advances: true,
            show_clusters: true,
            show_text: false,
            show_unicode: false,
            show_line_num: false,
            show_extents: false,
            show_flags: false,
        }
    }
}

type Setter = fn(&mut Options, &[u8]) -> Result<(), String>;

/// How an option is handled.
#[derive(Clone, Copy)]
enum Kind {
    /// Takes no value.
    Flag(fn(&mut Options)),
    /// Takes a value.
    Value(Setter),
    /// hb-shape has it, sofel-shape does not (exit 2).
    Unsupported,
    /// Takes a value and is ignored.
    Ignored,
}

/// The options: long name, short name, kind.
const OPTIONS: &[(&str, Option<u8>, Kind)] = &[
    // Text.
    ("text", None, Kind::Value(set_text)),
    ("text-file", None, Kind::Value(set_text_file)),
    ("unicodes", Some(b'u'), Kind::Value(set_unicodes)),
    ("text-before", None, Kind::Value(set_text_before)),
    ("text-after", None, Kind::Value(set_text_after)),
    ("unicodes-before", None, Kind::Value(set_unicodes_before)),
    ("unicodes-after", None, Kind::Value(set_unicodes_after)),
    ("single-par", None, Kind::Flag(|o| o.single_par = true)),
    // Segment.
    (
        "direction",
        None,
        Kind::Value(|o, v| {
            o.direction = Some(v.to_vec());
            Ok(())
        }),
    ),
    (
        "script",
        None,
        Kind::Value(|o, v| {
            o.script = Some(v.to_vec());
            Ok(())
        }),
    ),
    (
        "language",
        None,
        Kind::Value(|o, v| {
            o.language = Some(v.to_vec());
            Ok(())
        }),
    ),
    (
        "default-language",
        None,
        Kind::Value(|o, v| {
            o.default_language = Some(v.to_vec());
            Ok(())
        }),
    ),
    // Features.
    ("features", None, Kind::Value(set_features)),
    // Font.
    (
        "face-index",
        Some(b'y'),
        Kind::Value(|o, v| {
            o.face_index = u32::from_ne_bytes(glib_int("face-index", v)?.to_ne_bytes());
            Ok(())
        }),
    ),
    ("font-size", None, Kind::Value(set_font_size)),
    ("font-ppem", None, Kind::Value(set_font_ppem)),
    ("font-funcs", None, Kind::Ignored),
    ("face-loader", None, Kind::Ignored),
    // Buffer.
    ("bot", None, Kind::Flag(|o| o.bot = true)),
    ("eot", None, Kind::Flag(|o| o.eot = true)),
    (
        "preserve-default-ignorables",
        None,
        Kind::Flag(|o| o.preserve_default_ignorables = true),
    ),
    (
        "remove-default-ignorables",
        None,
        Kind::Flag(|o| o.remove_default_ignorables = true),
    ),
    (
        "invisible-glyph",
        None,
        Kind::Value(|o, v| {
            o.invisible_glyph = u32::from_ne_bytes(glib_int("invisible-glyph", v)?.to_ne_bytes());
            Ok(())
        }),
    ),
    (
        "not-found-glyph",
        None,
        Kind::Value(|o, v| {
            o.not_found_glyph = u32::from_ne_bytes(glib_int("not-found-glyph", v)?.to_ne_bytes());
            Ok(())
        }),
    ),
    (
        "not-found-variation-selector-glyph",
        None,
        Kind::Value(|o, v| {
            let g = u32::from_ne_bytes(
                glib_int("not-found-variation-selector-glyph", v)?.to_ne_bytes(),
            );
            // HB_CODEPOINT_INVALID is the default, "none".
            o.not_found_variation_selector_glyph = (g != u32::MAX).then_some(g);
            Ok(())
        }),
    ),
    (
        "cluster-level",
        None,
        Kind::Value(|o, v| {
            o.cluster_level = match glib_int("cluster-level", v)? {
                0 => ClusterLevel::MonotoneGraphemes,
                1 => ClusterLevel::MonotoneCharacters,
                2 => ClusterLevel::Characters,
                3 => ClusterLevel::Graphemes,
                n => return Err(format!("unknown cluster level {n}")),
            };
            Ok(())
        }),
    ),
    (
        "utf8-clusters",
        None,
        Kind::Flag(|o| o.utf8_clusters = true),
    ),
    (
        "unsafe-to-concat",
        None,
        Kind::Flag(|o| o.unsafe_to_concat = true),
    ),
    (
        "safe-to-insert-tatweel",
        None,
        Kind::Flag(|o| o.safe_to_insert_tatweel = true),
    ),
    ("verify", None, Kind::Flag(|o| o.verify = true)),
    (
        "normalize-glyphs",
        None,
        Kind::Flag(|o| o.normalize_glyphs = true),
    ),
    ("glyphs", None, Kind::Flag(|o| o.glyphs = true)),
    ("shaper", None, Kind::Value(set_shapers)),
    ("shapers", None, Kind::Value(set_shapers)),
    // Output.
    (
        "output-format",
        Some(b'O'),
        Kind::Value(|o, v| {
            o.output_format = Some(v.to_vec());
            Ok(())
        }),
    ),
    (
        "no-glyph-names",
        None,
        Kind::Flag(|o| o.show_glyph_names = false),
    ),
    (
        "no-positions",
        None,
        Kind::Flag(|o| o.show_positions = false),
    ),
    ("no-advances", None, Kind::Flag(|o| o.show_advances = false)),
    ("no-clusters", None, Kind::Flag(|o| o.show_clusters = false)),
    (
        "ned",
        None,
        Kind::Flag(|o| {
            o.show_clusters = false;
            o.show_advances = false;
        }),
    ),
    ("show-text", None, Kind::Flag(|o| o.show_text = true)),
    ("show-unicode", None, Kind::Flag(|o| o.show_unicode = true)),
    (
        "show-line-num",
        None,
        Kind::Flag(|o| o.show_line_num = true),
    ),
    ("show-flags", None, Kind::Flag(|o| o.show_flags = true)),
    ("show-extents", None, Kind::Flag(|o| o.show_extents = true)),
    // hb-shape's options sofel-shape does not have.
    (
        "variations",
        None,
        Kind::Value(|o, v| {
            let value = std::str::from_utf8(v).map_err(|_| "invalid variations".to_string())?;
            o.variations
                .extend(parse_variations(value).ok_or("invalid variations")?);
            Ok(())
        }),
    ),
    (
        "named-instance",
        None,
        Kind::Value(|o, v| {
            let value = std::str::from_utf8(v).map_err(|_| "invalid named instance".to_string())?;
            o.named_instance = Some(parse_named_instance(value).ok_or("invalid named instance")?);
            Ok(())
        }),
    ),
    ("font-ptem", None, Kind::Value(set_font_ptem)),
    ("font-slant", None, Kind::Value(set_font_slant)),
    ("font-bold", None, Kind::Value(set_font_bold)),
    ("font-grade", None, Kind::Value(set_font_grade)),
    ("ft-load-flags", None, Kind::Unsupported),
    ("sub-font", None, Kind::Unsupported),
    ("font-file", None, Kind::Unsupported),
    ("list-shapers", None, Kind::Unsupported),
    ("list-font-funcs", None, Kind::Unsupported),
    ("list-face-loaders", None, Kind::Unsupported),
    ("num-iterations", Some(b'n'), Kind::Unsupported),
    ("output-file", Some(b'o'), Kind::Unsupported),
    ("trace", Some(b'V'), Kind::Unsupported),
    ("verbose", None, Kind::Unsupported),
    ("justify-to", None, Kind::Unsupported),
    ("batch", None, Kind::Unsupported),
    ("version", None, Kind::Unsupported),
];

/// A usage error: the message, printed with the usage text.
struct Usage(String);

/// Parses the command line as GLib's `g_option_context_parse` does for
/// these options: `--name=value`, `--name value`, `-x value`, `--` and
/// two positional arguments (the font file, then the text). `Ok(None)`
/// for `--help`.
fn parse(args: &[OsString]) -> Result<Option<Options>, Usage> {
    let mut o = Options::default();
    let mut positional = Vec::new();
    let mut it = args.iter();
    let mut only_positional = false;
    while let Some(a) = it.next() {
        let bytes = a.as_encoded_bytes();
        if only_positional || bytes == b"-" || !bytes.starts_with(b"-") {
            positional.push(a.clone());
            continue;
        }
        if bytes == b"--" {
            only_positional = true;
            continue;
        }
        if bytes == b"--help" || bytes == b"-h" {
            return Ok(None);
        }
        let (name, inline) = match bytes.strip_prefix(b"--") {
            Some(long) => match long.iter().position(|&c| c == b'=') {
                Some(i) => (
                    long.get(..i).unwrap_or_default(),
                    Some(long.get(i.saturating_add(1)..).unwrap_or_default()),
                ),
                None => (long, None),
            },
            None => (bytes.get(1..).unwrap_or_default(), None),
        };
        let short = !bytes.starts_with(b"--");
        let entry = OPTIONS.iter().find(|(long, s, _)| {
            if short {
                name.len() == 1 && *s == name.first().copied()
            } else {
                long.as_bytes() == name
            }
        });
        let shown = String::from_utf8_lossy(bytes).into_owned();
        let Some(&(long, _, kind)) = entry else {
            return Err(Usage(format!("unknown option {shown}")));
        };
        let mut value = || -> Result<Vec<u8>, Usage> {
            match inline {
                Some(v) => Ok(v.to_vec()),
                None => it
                    .next()
                    .map(|v| v.as_encoded_bytes().to_vec())
                    .ok_or_else(|| Usage(format!("missing argument for --{long}"))),
            }
        };
        match kind {
            Kind::Flag(f) => {
                if inline.is_some() {
                    return Err(Usage(format!("--{long} takes no value")));
                }
                f(&mut o);
            }
            Kind::Value(f) => {
                let v = value()?;
                f(&mut o, &v).map_err(Usage)?;
            }
            Kind::Ignored => {
                value()?;
            }
            Kind::Unsupported => {
                return Err(Usage(format!("--{long} is not supported")));
            }
        }
    }
    let mut positional = positional.into_iter();
    o.font_file = positional.next();
    if let Some(t) = positional.next() {
        if o.text.is_some() || o.text_file.is_some() {
            return Err(Usage("Too many arguments on the command line".into()));
        }
        o.text = Some(t.as_encoded_bytes().to_vec());
    }
    if positional.next().is_some() {
        return Err(Usage("Too many arguments on the command line".into()));
    }
    if o.font_file.is_none() {
        return Err(Usage("No font file set".into()));
    }
    if o.text.is_some() && o.text_file.is_some() {
        return Err(Usage("Only one of text and text-file can be set".into()));
    }
    Ok(Some(o))
}

/// `parse_text`.
fn set_text(o: &mut Options, v: &[u8]) -> Result<(), String> {
    if o.text.is_some() && v.is_empty() {
        return Ok(());
    }
    if o.text.is_some() && o.text_len == Some(0) {
        o.text = None;
    }
    if o.text.is_some() {
        return Err("Either --text or --unicodes can be provided but not both".into());
    }
    o.text_len = None;
    o.text = Some(v.to_vec());
    Ok(())
}

fn set_text_file(o: &mut Options, v: &[u8]) -> Result<(), String> {
    o.text_file = Some(os_string(v));
    Ok(())
}

/// `parse_unicodes`.
fn set_unicodes(o: &mut Options, v: &[u8]) -> Result<(), String> {
    if o.text.is_some() {
        return Err("Either --text or --unicodes can be provided but not both".into());
    }
    let text = if v == b"*" {
        b"*".to_vec()
    } else {
        encode_unicodes(v)?
    };
    o.text_len = Some(text.len());
    o.text = Some(text);
    Ok(())
}

fn set_text_before(o: &mut Options, v: &[u8]) -> Result<(), String> {
    if o.text_before.is_some() {
        return Err(
            "Either --text-before or --unicodes-before can be provided but not both".into(),
        );
    }
    o.text_before = Some(v.to_vec());
    Ok(())
}

fn set_unicodes_before(o: &mut Options, v: &[u8]) -> Result<(), String> {
    if o.text_before.is_some() {
        return Err(
            "Either --text-before or --unicodes-before can be provided but not both".into(),
        );
    }
    o.text_before = Some(encode_unicodes(v)?);
    Ok(())
}

fn set_text_after(o: &mut Options, v: &[u8]) -> Result<(), String> {
    if o.text_after.is_some() {
        return Err("Either --text-after or --unicodes-after can be provided but not both".into());
    }
    o.text_after = Some(v.to_vec());
    Ok(())
}

fn set_unicodes_after(o: &mut Options, v: &[u8]) -> Result<(), String> {
    if o.text_after.is_some() {
        return Err("Either --text-after or --unicodes-after can be provided but not both".into());
    }
    o.text_after = Some(encode_unicodes(v)?);
    Ok(())
}

/// `parse_features`: the list without surrounding quotes, split at `,`
/// and ` `; items that do not parse are dropped.
fn set_features(o: &mut Options, v: &[u8]) -> Result<(), String> {
    o.features = parse_feature_list(v);
    Ok(())
}

/// Parse a feature list with the same forgiving syntax as `sofel shape`.
pub fn parse_feature_list(v: &[u8]) -> Vec<Feature> {
    let mut s = v;
    if let (Some(&first), Some(&last)) = (s.first(), s.last())
        && first == last
        && (first == b'"' || first == b'\'')
    {
        s = s.get(1..s.len().saturating_sub(1)).unwrap_or_default();
    }
    if s.is_empty() {
        Vec::new()
    } else {
        s.split(|&c| c == b',' || c == b' ')
            .filter(|f| !f.is_empty())
            .filter_map(Feature::parse)
            .collect()
    }
}

/// `parse_shapers`: every name must be a shaper sofel has; the first
/// one shapes, as `ot` always succeeds.
fn set_shapers(o: &mut Options, v: &[u8]) -> Result<(), String> {
    let mut first = None;
    for name in v.split(|&c| c == b',') {
        let s = match name {
            b"ot" => Shaper::Ot,
            b"fallback" => Shaper::Fallback,
            _ => {
                return Err(format!(
                    "Unknown or unsupported shaper: {}",
                    String::from_utf8_lossy(name)
                ));
            }
        };
        first = first.or(Some(s));
    }
    o.shaper = first.unwrap_or(Shaper::Ot);
    Ok(())
}

/// `parse_font_size`: `upem`, or `sscanf ("%lf%*[ ,]%lf")`.
fn set_font_size(o: &mut Options, v: &[u8]) -> Result<(), String> {
    if v == b"upem" {
        o.font_size = None;
        return Ok(());
    }
    let (x, y) = scan_pair(v, scan_double).ok_or_else(|| {
        "--font-size argument should be one or two space-separated numbers".to_string()
    })?;
    o.font_size = Some((x, y.unwrap_or(x)));
    Ok(())
}

/// `parse_font_ppem`: `sscanf ("%d%*[ ,]%d")`.
fn set_font_ptem(o: &mut Options, v: &[u8]) -> Result<(), String> {
    o.ptem = core::str::from_utf8(v)
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .ok_or_else(|| "invalid font point size".to_string())?;
    Ok(())
}

fn set_font_ppem(o: &mut Options, v: &[u8]) -> Result<(), String> {
    let (x, y) = scan_pair(v, scan_int).ok_or_else(|| {
        "--font-ppem argument should be one or two space-separated numbers".to_string()
    })?;
    o.ppem = (x, y.unwrap_or(x));
    Ok(())
}

#[allow(clippy::cast_possible_truncation)] // HarfBuzz parses double then stores float.
fn set_font_slant(o: &mut Options, v: &[u8]) -> Result<(), String> {
    let value = std::str::from_utf8(v)
        .ok()
        .and_then(|text| text.parse::<f64>().ok())
        .ok_or_else(|| "--font-slant argument should be a number".to_string())?;
    o.slant = value as f32;
    Ok(())
}

#[allow(clippy::cast_possible_truncation)] // HarfBuzz parses double then stores float.
fn set_font_embolden(o: &mut Options, v: &[u8], in_place: bool) -> Result<(), String> {
    let (x, y) = scan_pair(v, scan_double).ok_or_else(|| {
        "--font-bold/--font-grade argument should be one or two numbers".to_string()
    })?;
    o.embolden = (x as f32, y.unwrap_or(x) as f32);
    o.embolden_in_place = in_place;
    Ok(())
}

fn set_font_bold(o: &mut Options, v: &[u8]) -> Result<(), String> {
    set_font_embolden(o, v, false)
}

fn set_font_grade(o: &mut Options, v: &[u8]) -> Result<(), String> {
    set_font_embolden(o, v, true)
}

/// `sscanf (s, "%X%*[ ,]%X")` with one conversion `scan`: `None` when
/// the first fails, the second when it is there.
fn scan_pair<T>(s: &[u8], scan: fn(&[u8]) -> Option<(T, usize)>) -> Option<(T, Option<T>)> {
    let (x, n) = scan(s)?;
    let rest = s.get(n..).unwrap_or_default();
    let sep = rest.iter().take_while(|&&c| c == b' ' || c == b',').count();
    if sep == 0 {
        return Some((x, None));
    }
    let y = scan(rest.get(sep..).unwrap_or_default()).map(|(y, _)| y);
    Some((x, y))
}

fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// `%lf`: leading white space, then the longest prefix that is a
/// decimal number (sign, digits, point, exponent) or `inf`, `infinity`,
/// `nan`. Returns the value and the bytes used.
fn scan_double(s: &[u8]) -> Option<(f64, usize)> {
    let ws = s.iter().take_while(|&&c| is_c_space(c)).count();
    let t = s.get(ws..)?;
    let mut i = usize::from(matches!(t.first(), Some(b'+' | b'-')));
    let lower: Vec<u8> = t.iter().map(u8::to_ascii_lowercase).collect();
    for word in [&b"infinity"[..], b"inf", b"nan"] {
        if lower.get(i..).is_some_and(|r| r.starts_with(word)) {
            let end = i.saturating_add(word.len());
            let neg = t.first() == Some(&b'-');
            let v = if word == b"nan" {
                f64::NAN
            } else {
                f64::INFINITY
            };
            return Some((if neg { -v } else { v }, ws.saturating_add(end)));
        }
    }
    let digits = |from: usize| {
        t.get(from..)
            .map_or(0, |r| r.iter().take_while(|c| c.is_ascii_digit()).count())
    };
    let int = digits(i);
    i = i.saturating_add(int);
    let mut frac = 0;
    if t.get(i) == Some(&b'.') {
        frac = digits(i.saturating_add(1));
        if int > 0 || frac > 0 {
            i = i.saturating_add(1).saturating_add(frac);
        }
    }
    if int == 0 && frac == 0 {
        return None;
    }
    if matches!(t.get(i), Some(b'e' | b'E')) {
        let mut j = i.saturating_add(1);
        if matches!(t.get(j), Some(b'+' | b'-')) {
            j = j.saturating_add(1);
        }
        let e = digits(j);
        if e > 0 {
            i = j.saturating_add(e);
        }
    }
    let v: f64 = std::str::from_utf8(t.get(..i)?).ok()?.parse().ok()?;
    Some((v, ws.saturating_add(i)))
}

/// `%d`: leading white space, a sign and decimal digits, wrapping as
/// glibc's `sscanf` does past the range of `int`.
fn scan_int(s: &[u8]) -> Option<(i32, usize)> {
    let ws = s.iter().take_while(|&&c| is_c_space(c)).count();
    let t = s.get(ws..)?;
    let neg = t.first() == Some(&b'-');
    let sign = usize::from(matches!(t.first(), Some(b'+' | b'-')));
    let digits: Vec<u8> = t
        .get(sign..)?
        .iter()
        .copied()
        .take_while(u8::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    let v = digits.iter().fold(0u64, |v, &d| {
        v.wrapping_mul(10)
            .wrapping_add(u64::from(d.wrapping_sub(b'0')))
    });
    let v = if neg { v.wrapping_neg() } else { v };
    let low = u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0);
    Some((
        i32::from_ne_bytes(low.to_ne_bytes()),
        ws.saturating_add(sign).saturating_add(digits.len()),
    ))
}

/// GLib's `G_OPTION_ARG_INT`: `strtol (arg, &end, 0)`, the whole value,
/// within the range of `int`.
fn glib_int(name: &str, v: &[u8]) -> Result<i32, String> {
    let bad = || {
        format!(
            "Cannot parse integer value “{}” for --{name}",
            String::from_utf8_lossy(v)
        )
    };
    let (n, used) = strtol(v, 0).ok_or_else(bad)?;
    if used != v.len() || v.is_empty() {
        return Err(bad());
    }
    i32::try_from(n).map_err(|_| bad())
}

/// C's `strtol` / `strtoul` digits: leading white space, a sign, `0x`
/// for base 16 (base 0 or 16), a leading `0` for base 8 (base 0). The
/// value saturates (`None` when nothing was read). Returns the value
/// and the bytes used.
fn strtol(s: &[u8], base: u32) -> Option<(i128, usize)> {
    let ws = s.iter().take_while(|&&c| is_c_space(c)).count();
    let mut i = ws;
    let neg = s.get(i) == Some(&b'-');
    if matches!(s.get(i), Some(b'+' | b'-')) {
        i = i.saturating_add(1);
    }
    let hex_prefix = s.get(i) == Some(&b'0')
        && matches!(s.get(i.saturating_add(1)), Some(b'x' | b'X'))
        && s.get(i.saturating_add(2))
            .is_some_and(u8::is_ascii_hexdigit);
    let base = match base {
        0 if hex_prefix => 16,
        0 if s.get(i) == Some(&b'0') => 8,
        0 => 10,
        b => b,
    };
    if base == 16 && hex_prefix {
        i = i.saturating_add(2);
    }
    let start = i;
    let mut v: i128 = 0;
    while let Some(d) = s.get(i).and_then(|&c| char::from(c).to_digit(base)) {
        v = v
            .saturating_mul(i128::from(base))
            .saturating_add(i128::from(d));
        i = i.saturating_add(1);
    }
    if i == start {
        return None;
    }
    Some((if neg { v.saturating_neg() } else { v }, i))
}

/// `encode_unicodes`: hex numbers between delimiters, each written as
/// GLib's `g_string_append_unichar` writes it (UTF-8 extended to six
/// bytes, so values that are not characters stay ill-formed for the
/// buffer to replace).
fn encode_unicodes(s: &[u8]) -> Result<Vec<u8>, String> {
    const DELIMITERS: &[u8] = b"<+-|>{},;&#\\xXuUnNiI\n\t\x0b\x0c\r ";
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < s.len() {
        while s.get(i).is_some_and(|c| DELIMITERS.contains(c)) {
            i = i.saturating_add(1);
        }
        if i >= s.len() {
            break;
        }
        let rest = s.get(i..).unwrap_or_default();
        let (v, used) = strtol(rest, 16)
            .filter(|&(v, _)| v.unsigned_abs() <= u128::from(u64::MAX))
            .ok_or_else(|| {
                format!(
                    "Failed parsing Unicode value at: '{}'",
                    String::from_utf8_lossy(rest)
                )
            })?;
        // strtoul negates as unsigned long; hb_codepoint_t keeps 32 bits.
        let low = u32::try_from(v.rem_euclid(1i128 << 32)).unwrap_or(0);
        append_unichar(&mut out, low);
        i = i.saturating_add(used);
    }
    Ok(out)
}

/// `g_unichar_to_utf8`.
fn append_unichar(out: &mut Vec<u8>, c: u32) {
    let (first, len): (u32, u32) = match c {
        0..0x80 => (0, 1),
        0x80..0x800 => (0xC0, 2),
        0x800..0x1_0000 => (0xE0, 3),
        0x1_0000..0x20_0000 => (0xF0, 4),
        0x20_0000..0x400_0000 => (0xF8, 5),
        _ => (0xFC, 6),
    };
    let mut bytes = [0u8; 6];
    let mut c = c;
    for k in (1..len).rev() {
        if let Some(b) = bytes.get_mut(usize::try_from(k).unwrap_or(0)) {
            *b = u8::try_from((c & 0x3F) | 0x80).unwrap_or(0);
        }
        c >>= 6;
    }
    bytes[0] = u8::try_from((c | first) & 0xFF).unwrap_or(0);
    out.extend_from_slice(
        bytes
            .get(..usize::try_from(len).unwrap_or(0))
            .unwrap_or_default(),
    );
}

fn os_string(v: &[u8]) -> OsString {
    // The bytes came from an OsString (as_encoded_bytes) or are UTF-8.
    match std::str::from_utf8(v) {
        Ok(s) => OsString::from(s),
        Err(_) => OsString::from(String::from_utf8_lossy(v).into_owned()),
    }
}

/// The lines of the text as `text_options_t::get_line` gives them: split
/// at `\n` (the whole text with `--single-par`), without the newline, a
/// last empty piece dropped.
fn split_lines(text: &[u8], single_par: bool) -> Vec<&[u8]> {
    if text.is_empty() {
        return Vec::new();
    }
    if single_par {
        return vec![text];
    }
    let mut lines: Vec<&[u8]> = text.split(|&c| c == b'\n').collect();
    if text.last() == Some(&b'\n') {
        lines.pop();
    }
    lines
}

/// The font as `hb-shape` asks HarfBuzz's `hb_font_t`: names (`post`,
/// then CFF), boxes at the scale, glyphs by name and cmap.
struct CliFont<'a, 'b> {
    font: Font<'a>,
    names: Option<GlyphNames<'a, 'b>>,
    extents: HbExtents<'a, 'static, 'static>,
    scale: HbScale,
}

impl SerializeFont for CliFont<'_, '_> {
    fn glyph_name(&self, glyph: u32) -> Option<&[u8]> {
        self.names.as_ref()?.glyph_name(GlyphId(glyph))
    }
    fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        self.extents
            .scaled_glyph_extents(GlyphId(glyph), &self.scale)
    }
    fn glyph_from_name(&self, name: &[u8]) -> Option<u32> {
        self.names.as_ref()?.glyph_from_name(name).map(|g| g.0)
    }
    fn nominal_glyph(&self, c: u32) -> Option<u32> {
        self.font.nominal_glyph_raw(c).map(|g| g.0)
    }
}

struct SyntheticSerializeFont<'a, 'p> {
    base: &'a dyn SerializeFont,
    subfont: &'p SubFont<'p>,
}

impl SerializeFont for SyntheticSerializeFont<'_, '_> {
    fn glyph_name(&self, glyph: u32) -> Option<&[u8]> {
        self.base.glyph_name(glyph)
    }
    fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        self.subfont.glyph_extents(glyph)
    }
    fn glyph_from_name(&self, name: &[u8]) -> Option<u32> {
        self.base.glyph_from_name(name)
    }
    fn nominal_glyph(&self, c: u32) -> Option<u32> {
        self.base.nominal_glyph(c)
    }
}

/// Everything a line is shaped with.
struct Context<'a, 'b, 'c> {
    o: &'c Options,
    font: CliFont<'a, 'b>,
    location: Location<'a>,
    scale: Scale,
    format: Option<SerializeFormat>,
    flags: SerializeFlags,
}

fn synthetic_outline(
    instance: sofel_font::var::Instance<'_, '_, '_, '_>,
    scale: Scale,
    extents: &HbExtents<'_, '_, '_>,
    glyph: u32,
) -> Option<GlyphOutline> {
    let mut outline = GlyphOutline::new();
    let mut emit = |command| match command {
        HbOutlineCommand::MoveTo(p) => outline.move_to(p),
        HbOutlineCommand::LineTo(p) => outline.line_to(p),
        HbOutlineCommand::QuadTo(c, p) => outline.quad_to(c, p),
        HbOutlineCommand::CubicTo(a, b, p) => outline.cubic_to(a, b, p),
        HbOutlineCommand::Close => outline.close(),
    };
    if let Some(path) = extents.cff_glyph_path(GlyphId(glyph), &scale.scale) {
        for command in path {
            emit(command);
        }
    } else {
        let mut scratch = Scratch::default();
        sofel_outline::load(
            instance,
            GlyphId(glyph),
            sofel_outline::Scale::Unscaled,
            &mut scratch,
        )
        .ok()?;
        let upem = f32::from(instance.font().hb_units_per_em());
        scratch
            .hb_decompose(
                (scale.scale.x_scale() as f32) / upem,
                (scale.scale.y_scale() as f32) / upem,
                emit,
            )
            .ok()?;
    }
    Some(outline)
}

/// Runs the command with `args` (without the program name), reading the
/// text from standard input when no other text is given. Returns the
/// exit status.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    run_with_input(args, &mut std::io::stdin().lock(), out, err)
}

/// [`run`] with `input` as standard input.
pub fn run_with_input(
    args: &[OsString],
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let o = match parse(args) {
        Ok(Some(o)) => o,
        Ok(None) => {
            let _ = out.write_all(HELP.as_bytes());
            return 0;
        }
        Err(Usage(e)) => {
            let _ = writeln!(
                err,
                "sofel-shape: {e}\nTry `sofel-shape --help' for more information."
            );
            return 2;
        }
    };
    let format = match &o.output_format {
        None => Some(SerializeFormat::Text),
        Some(f) if f.is_empty() => None,
        Some(f) => match SerializeFormat::from_name(f) {
            Some(f) => Some(f),
            None => {
                let _ = writeln!(
                    err,
                    "sofel-shape: Unknown output format `{}'; supported formats are: text/json",
                    String::from_utf8_lossy(f)
                );
                return 2;
            }
        },
    };
    let fail = |err: &mut dyn Write, msg: String| {
        let _ = writeln!(err, "sofel-shape: {msg}");
        1
    };
    let font_file = o.font_file.clone().unwrap_or_default();
    let data = if font_file == "-" {
        let mut v = Vec::new();
        if let Err(e) = input.read_to_end(&mut v) {
            return fail(err, format!("standard input: {e}"));
        }
        v
    } else {
        match std::fs::read(&font_file) {
            Ok(d) => d,
            Err(e) => return fail(err, format!("{}: {e}", font_file.to_string_lossy())),
        }
    };
    let data = match crate::font_input::unpack(data) {
        Ok(data) => data,
        Err(d) => return fail(err, format!("{} at byte {}", d.kind, d.offset)),
    };
    let font = match Font::new(&data, o.face_index) {
        Ok(f) => f,
        Err(e) => {
            return fail(
                err,
                format!(
                    "{}: Failed loading font face: {e}",
                    font_file.to_string_lossy()
                ),
            );
        }
    };
    let text = match (&o.text, &o.text_file) {
        (Some(t), _) => {
            let len = o
                .text_len
                .unwrap_or_else(|| t.iter().position(|&c| c == 0).unwrap_or(t.len()));
            t.get(..len).unwrap_or_default().to_vec()
        }
        (None, file) => {
            let file = file.clone().unwrap_or_else(|| OsString::from("-"));
            let mut v = Vec::new();
            let r = if file == "-" {
                input.read_to_end(&mut v).map(|_| ())
            } else {
                std::fs::File::open(&file).and_then(|mut f| f.read_to_end(&mut v).map(|_| ()))
            };
            if let Err(e) = r {
                return fail(
                    err,
                    format!("Failed opening text file `{}': {e}", file.to_string_lossy()),
                );
            }
            v
        }
    };
    let upem = i32::from(font.units_per_em());
    let (sx, sy) = o.font_size.map_or((upem, upem), |(x, y)| {
        (font_size_to_scale(x), font_size_to_scale(y))
    });
    let hb_scale = HbScale::for_font(&font, sx, sy).with_ptem(o.ptem);
    let ppem = |v: i32| u32::from_ne_bytes(v.to_ne_bytes());
    let scale = Scale::new(hb_scale, ppem(o.ppem.0), ppem(o.ppem.1));
    let mut names_buf = vec![0u32; font.glyph_names_len()];
    let names = font.glyph_names(&mut names_buf);
    let mut flags = SerializeFlags::DEFAULT;
    for (on, f) in [
        (!o.show_glyph_names, SerializeFlags::NO_GLYPH_NAMES),
        (!o.show_clusters, SerializeFlags::NO_CLUSTERS),
        (!o.show_positions, SerializeFlags::NO_POSITIONS),
        (!o.show_advances, SerializeFlags::NO_ADVANCES),
        (o.show_extents, SerializeFlags::GLYPH_EXTENTS),
        (o.show_flags, SerializeFlags::GLYPH_FLAGS),
    ] {
        if on {
            flags = flags.union(f);
        }
    }
    let location = Location::new(&font, o.named_instance, &o.variations);
    let cx = Context {
        o: &o,
        location,
        font: CliFont {
            font,
            names,
            extents: HbExtents::new(font.default_instance()),
            scale: hb_scale,
        },
        scale,
        format,
        flags,
    };
    let mut failed = false;
    let mut text_out = Vec::new();
    // The plan of the last line, used again while the properties stay
    // (HarfBuzz's plan cache).
    let mut plan = None;
    for (n, line) in (1u32..).zip(split_lines(&text, o.single_par)) {
        text_out.clear();
        if !line_output(&cx, &mut plan, n, line, &mut text_out, err) {
            failed = true;
        }
        if out.write_all(&text_out).is_err() {
            return fail(err, "cannot write the output".into());
        }
    }
    if failed { 3 } else { 0 }
}

/// `(int) scalbnf (size, 0)`: the size as a `float`, truncated.
#[allow(clippy::cast_possible_truncation)]
fn font_size_to_scale(size: f64) -> i32 {
    (size as f32) as i32
}

/// Shapes one line and writes its output (`shape_consumer_t::consume_line`).
/// `false` when shaping failed.
fn line_output<'a>(
    cx: &Context<'a, '_, '_>,
    plan_cache: &mut Option<ShapePlan<'a>>,
    line_no: u32,
    line: &[u8],
    out: &mut Vec<u8>,
    err: &mut dyn Write,
) -> bool {
    let o = cx.o;
    let line_prefix = |out: &mut Vec<u8>| {
        if o.show_line_num {
            out.extend_from_slice(format!("{line_no}: ").as_bytes());
        }
    };
    if o.glyphs {
        let (props, glyphs) = glyphs_input(cx, line);
        show_input(cx, line_no, line, None, Some(&glyphs), out);
        let mut glyphs = glyphs;
        if o.normalize_glyphs {
            glyphs.normalize_glyphs();
        }
        let _ = props;
        line_prefix(out);
        write_glyphs(cx, &glyphs, out);
        out.push(b'\n');
        return true;
    }
    let mut buffer = UnicodeBuffer::new();
    if let Some(before) = &o.text_before {
        buffer.push_utf8(before, before.len(), Some(0));
    }
    buffer.push_utf8(line, 0, None);
    if let Some(after) = &o.text_after {
        buffer.push_utf8(after, 0, Some(0));
    }
    if !o.utf8_clusters {
        buffer.number_clusters();
    }
    let mut bflags = BufferFlags::DEFAULT;
    for (on, f) in [
        (o.bot, BufferFlags::BOT),
        (o.eot, BufferFlags::EOT),
        (o.unsafe_to_concat, BufferFlags::PRODUCE_UNSAFE_TO_CONCAT),
        (
            o.safe_to_insert_tatweel,
            BufferFlags::PRODUCE_SAFE_TO_INSERT_TATWEEL,
        ),
        (
            o.preserve_default_ignorables,
            BufferFlags::PRESERVE_DEFAULT_IGNORABLES,
        ),
        (
            o.remove_default_ignorables,
            BufferFlags::REMOVE_DEFAULT_IGNORABLES,
        ),
    ] {
        if on {
            bflags = bflags.union(f);
        }
    }
    buffer.set_flags(bflags);
    buffer.set_invisible_glyph(o.invisible_glyph);
    buffer.set_not_found_glyph(o.not_found_glyph);
    buffer.set_not_found_variation_selector_glyph(o.not_found_variation_selector_glyph);
    buffer.set_cluster_level(o.cluster_level);
    let props = segment_properties(o).guess(
        &buffer,
        o.default_language.as_deref().and_then(Language::new),
    );
    show_input(cx, line_no, line, Some(&buffer), None, out);
    if buffer.is_empty() {
        line_prefix(out);
        out.push(b'\n');
        return true;
    }
    let instance = cx.font.font.instance(&cx.location);
    let selected = ShapePlan::variations_index_for(instance);
    if plan_cache
        .as_ref()
        .is_none_or(|p| p.key().0 != props || p.key().2 != selected)
    {
        *plan_cache = Some(ShapePlan::new(instance, props.clone(), &o.features));
    }
    let Some(plan) = plan_cache.as_ref() else {
        return false;
    };
    let shape_one = |b: UnicodeBuffer| -> GlyphBuffer {
        match o.shaper {
            _ if o.slant != 0.0 || o.embolden != (0.0, 0.0) => {
                let extents = HbExtents::new(instance);
                let mut subfont = SubFont::new(cx.scale);
                subfont.set_synthetic_slant(o.slant);
                subfont.set_synthetic_bold(o.embolden.0, o.embolden.1, o.embolden_in_place);
                subfont.set_glyph_extents_func(|glyph| {
                    extents.scaled_glyph_extents(GlyphId(glyph), &cx.scale.scale)
                });
                subfont.set_outline_func(|glyph| {
                    synthetic_outline(instance, cx.scale, &extents, glyph)
                });
                match o.shaper {
                    Shaper::Ot => shape_with_subfont(instance, &subfont, plan, b),
                    Shaper::Fallback => shape_fallback_with_subfont(instance, &subfont, &props, b),
                }
            }
            Shaper::Ot => shape_with_extents(instance, &cx.scale, plan, b, &|g| {
                instance
                    .glyf_extents(g)
                    .or_else(|| cx.font.extents.glyph_extents(g))
            }),
            Shaper::Fallback => shape_fallback(instance, &cx.scale, &props, b),
        }
    };
    let text = o.verify.then(|| buffer.clone());
    let mut glyphs = shape_one(buffer);
    if let Some(text) = text
        && let Err(e) = verify(&glyphs, &text, &shape_one)
    {
        let _ = writeln!(err, "sofel-shape: buffer verify error: {e}");
        let mut t = Vec::new();
        let cps: Vec<u32> = text.codepoints().collect();
        let cls: Vec<u32> = text.clusters().collect();
        serialize_unicode(
            &cps,
            &cls,
            SerializeFormat::Text,
            SerializeFlags::NO_CLUSTERS,
            &mut t,
        );
        let _ = writeln!(
            err,
            "sofel-shape: buffer verify error: text was: {}.",
            String::from_utf8_lossy(&t)
        );
        line_prefix(out);
        out.extend_from_slice(b"error: Shaping failed.\n");
        return false;
    }
    if o.normalize_glyphs {
        glyphs.normalize_glyphs();
    }
    line_prefix(out);
    write_glyphs(cx, &glyphs, out);
    out.push(b'\n');
    true
}

/// The segment properties the options give (`setup_buffer`).
fn segment_properties(o: &Options) -> SegmentProperties {
    SegmentProperties {
        direction: o.direction.as_deref().and_then(Direction::from_bytes),
        script: o.script.as_deref().and_then(script_from_string),
        language: o.language.as_deref().and_then(Language::new),
    }
}

/// `hb_script_from_string (s, -1)`: the first four bytes as a tag,
/// padded with spaces.
fn script_from_string(s: &[u8]) -> Option<Script> {
    if s.is_empty() {
        return None;
    }
    let mut tag = [b' '; 4];
    for (t, &c) in tag.iter_mut().zip(s) {
        *t = c;
    }
    Script::from_iso15924(&tag)
}

/// `--show-text` and `--show-unicode` (`serialize_buffer_of_text`).
fn show_input(
    cx: &Context<'_, '_, '_>,
    line_no: u32,
    line: &[u8],
    text: Option<&UnicodeBuffer>,
    glyphs: Option<&GlyphBuffer>,
    out: &mut Vec<u8>,
) {
    let o = cx.o;
    let prefix = |out: &mut Vec<u8>| {
        if o.show_line_num {
            out.extend_from_slice(format!("{line_no}: ").as_bytes());
        }
    };
    if o.show_text {
        prefix(out);
        out.push(b'(');
        out.extend_from_slice(line);
        out.extend_from_slice(b")\n");
    }
    if o.show_unicode {
        prefix(out);
        if let Some(t) = text {
            let cps: Vec<u32> = t.codepoints().collect();
            let cls: Vec<u32> = t.clusters().collect();
            serialize_unicode(
                &cps,
                &cls,
                SerializeFormat::Text,
                SerializeFlags::DEFAULT,
                out,
            );
        }
        if let Some(g) = glyphs {
            serialize_all(cx, g, SerializeFormat::Text, SerializeFlags::DEFAULT, out);
        }
        out.push(b'\n');
    }
}

/// Writes the glyphs in the chosen form.
fn write_glyphs(cx: &Context<'_, '_, '_>, glyphs: &GlyphBuffer, out: &mut Vec<u8>) {
    if let Some(format) = cx.format {
        serialize_all(cx, glyphs, format, cx.flags, out);
    }
}

/// `shape_format_options_t::serialize`: 32768-byte pieces.
fn serialize_all(
    cx: &Context<'_, '_, '_>,
    glyphs: &GlyphBuffer,
    format: SerializeFormat,
    flags: SerializeFlags,
    out: &mut Vec<u8>,
) {
    if cx.o.slant != 0.0 || cx.o.embolden != (0.0, 0.0) {
        let instance = cx.font.font.instance(&cx.location);
        let extents = HbExtents::new(instance);
        let mut subfont = SubFont::new(cx.scale);
        subfont.set_synthetic_slant(cx.o.slant);
        subfont.set_synthetic_bold(cx.o.embolden.0, cx.o.embolden.1, cx.o.embolden_in_place);
        subfont.set_glyph_extents_func(|glyph| {
            extents.scaled_glyph_extents(GlyphId(glyph), &cx.scale.scale)
        });
        subfont.set_outline_func(|glyph| synthetic_outline(instance, cx.scale, &extents, glyph));
        let font = SyntheticSerializeFont {
            base: &cx.font,
            subfont: &subfont,
        };
        serialize_all_with_font(glyphs, format, flags, out, &font);
    } else {
        serialize_all_with_font(glyphs, format, flags, out, &cx.font);
    }
}

fn serialize_all_with_font(
    glyphs: &GlyphBuffer,
    format: SerializeFormat,
    flags: SerializeFlags,
    out: &mut Vec<u8>,
    font: &dyn SerializeFont,
) {
    let infos = glyphs.glyph_infos();
    let mut start = 0;
    while start < infos.len() {
        let n = serialize_glyphs_range(
            infos,
            Some(glyphs.glyph_positions()),
            start,
            infos.len(),
            32768,
            font,
            format,
            flags,
            out,
        );
        if n == 0 {
            break;
        }
        start = start.saturating_add(n);
    }
}

/// `--glyphs`: the line read as a glyph string, positioned with the
/// font's advances when it has none, and scaled to the font size
/// (`populate_buffer` and `shape` with `glyphs`).
fn glyphs_input(cx: &Context<'_, '_, '_>, line: &[u8]) -> (Direction, GlyphBuffer) {
    let o = cx.o;
    let props = segment_properties(o).guess(&UnicodeBuffer::new(), None);
    let mut text = line.to_vec();
    if text.last().is_some_and(|&c| c != b']') {
        text.push(b']');
    }
    let (mut infos, mut positions) = (Vec::new(), Vec::new());
    let _ = deserialize_glyphs(
        &text,
        SerializeFormat::Text,
        &cx.font,
        &mut infos,
        &mut positions,
    );
    let scale_advances = text.contains(&b'+') || text.contains(&b'@');
    if !scale_advances {
        let hb = cx.scale.scale;
        for (i, p) in infos.iter().zip(positions.iter_mut()) {
            let g = GlyphId(i.glyph);
            if props.direction.is_horizontal() {
                p.x_advance = hb.h_advance(&cx.font.font, g);
                p.y_advance = 0;
            } else {
                p.x_advance = 0;
                p.y_advance = hb.v_advance(&cx.font.font, g);
            }
        }
    }
    let upem = i64::from(cx.font.font.units_per_em());
    let (xs, ys) = (cx.scale.scale.x_scale(), cx.scale.scale.y_scale());
    if i64::from(xs) != upem || i64::from(ys) != upem {
        let s = |v: i32, by: i32| -> i32 {
            let r = i64::from(v)
                .wrapping_mul(i64::from(by))
                .checked_div(upem)
                .unwrap_or(0);
            i32::from_ne_bytes(u32::try_from(r & 0xFFFF_FFFF).unwrap_or(0).to_ne_bytes())
        };
        for p in &mut positions {
            p.x_offset = s(p.x_offset, xs);
            p.y_offset = s(p.y_offset, ys);
            if scale_advances {
                p.x_advance = s(p.x_advance, xs);
                p.y_advance = s(p.y_advance, ys);
            }
        }
    }
    (
        props.direction,
        GlyphBuffer::from_parts(props.direction, infos, positions),
    )
}

/// `hb_buffer_t::verify`: the cluster values are monotone, shaping the
/// text in pieces cut where the glyphs are not unsafe to break gives
/// the same glyphs, and (with `--unsafe-to-concat`) shaping the pieces
/// cut where they are not unsafe to concatenate, in two interleaved
/// streams, gives the same glyphs. Each check needs monotone cluster
/// values and is skipped for the other cluster levels.
fn verify(
    glyphs: &GlyphBuffer,
    text: &UnicodeBuffer,
    shape: &dyn Fn(UnicodeBuffer) -> GlyphBuffer,
) -> Result<(), &'static str> {
    let level = text.cluster_level();
    if !matches!(
        level,
        ClusterLevel::MonotoneGraphemes | ClusterLevel::MonotoneCharacters
    ) {
        return Ok(());
    }
    let forward = glyphs.direction().is_forward();
    let infos = glyphs.glyph_infos();
    let monotone = infos.windows(2).all(|w| match w {
        [a, b] => a.cluster == b.cluster || (a.cluster < b.cluster) == forward,
        _ => true,
    });
    if !monotone {
        return Err("clusters are not monotone.");
    }
    let mut result = Ok(());
    if let Err(e) = verify_unsafe_to_break(glyphs, text, shape) {
        result = Err(e);
    }
    if text.flags().contains(BufferFlags::PRODUCE_UNSAFE_TO_CONCAT)
        && let Err(e) = verify_unsafe_to_concat(glyphs, text, shape)
    {
        result = result.and(Err(e));
    }
    result
}

/// The parts of a glyph buffer that are compared.
fn glyph_buffer(
    direction: Direction,
    infos: Vec<GlyphInfo>,
    pos: Vec<GlyphPosition>,
) -> GlyphBuffer {
    GlyphBuffer::from_parts(direction, infos, pos)
}

fn differs(a: &GlyphBuffer, b: &GlyphBuffer) -> bool {
    diff(a, b, u32::MAX, 0).0 & !DiffFlags::GLYPH_FLAGS_MISMATCH.0 != 0
}

/// `buffer_verify_unsafe_to_break`.
fn verify_unsafe_to_break(
    glyphs: &GlyphBuffer,
    text: &UnicodeBuffer,
    shape: &dyn Fn(UnicodeBuffer) -> GlyphBuffer,
) -> Result<(), &'static str> {
    let infos = glyphs.glyph_infos();
    let n = infos.len();
    let chars: Vec<u32> = text.clusters().collect();
    let num_chars = chars.len();
    let forward = glyphs.direction().is_forward();
    let (mut text_start, mut text_end) = if forward {
        (0, 0)
    } else {
        (num_chars, num_chars)
    };
    let (mut r_infos, mut r_pos) = (Vec::new(), Vec::new());
    let cluster = |i: usize| infos.get(i).map_or(0, |g| g.cluster);
    let char_cluster = |i: usize| chars.get(i).copied().unwrap_or(0);
    for end in 1..=n {
        if end < n {
            let at = if forward { end } else { end.saturating_sub(1) };
            let unsafe_here = infos
                .get(at)
                .is_some_and(|g| g.flags.contains(GlyphFlags::UNSAFE_TO_BREAK));
            if cluster(end) == cluster(end.saturating_sub(1)) || unsafe_here {
                continue;
            }
        }
        if end == n {
            if forward {
                text_end = num_chars;
            } else {
                text_start = 0;
            }
        } else if forward {
            let c = cluster(end);
            while text_end < num_chars && char_cluster(text_end) < c {
                text_end = text_end.saturating_add(1);
            }
        } else {
            let c = cluster(end.saturating_sub(1));
            while text_start > 0 && char_cluster(text_start.saturating_sub(1)) >= c {
                text_start = text_start.saturating_sub(1);
            }
        }
        if text_start >= text_end {
            return Err("unsafe-to-break text range is invalid.");
        }
        let mut fragment = text.similar();
        let mut flags = fragment.flags();
        if text_start > 0 {
            flags = BufferFlags(flags.0 & !BufferFlags::BOT.0);
        }
        if text_end < num_chars {
            flags = BufferFlags(flags.0 & !BufferFlags::EOT.0);
        }
        fragment.set_flags(flags);
        fragment.append(text, text_start, text_end);
        let shaped = shape(fragment);
        r_infos.extend_from_slice(shaped.glyph_infos());
        r_pos.extend_from_slice(shaped.glyph_positions());
        if forward {
            text_start = text_end;
        } else {
            text_end = text_start;
        }
    }
    let reconstruction = glyph_buffer(glyphs.direction(), r_infos, r_pos);
    if differs(&reconstruction, glyphs) {
        return Err("unsafe-to-break test failed.");
    }
    Ok(())
}

/// `buffer_verify_unsafe_to_concat`.
fn verify_unsafe_to_concat(
    glyphs: &GlyphBuffer,
    text: &UnicodeBuffer,
    shape: &dyn Fn(UnicodeBuffer) -> GlyphBuffer,
) -> Result<(), &'static str> {
    let forward = glyphs.direction().is_forward();
    let mut infos = glyphs.glyph_infos().to_vec();
    let mut pos = glyphs.glyph_positions().to_vec();
    if !forward {
        infos.reverse();
        pos.reverse();
    }
    let chars: Vec<u32> = text.clusters().collect();
    let num_chars = chars.len();
    let n = infos.len();
    let mut fragments = [text.similar(), text.similar()];
    let mut idx = 0usize;
    let (mut text_start, mut text_end) = (0usize, 0usize);
    let cluster = |i: usize| infos.get(i).map_or(0, |g| g.cluster);
    for end in 1..=n {
        if end < n
            && (cluster(end) == cluster(end.saturating_sub(1))
                || infos
                    .get(end)
                    .is_some_and(|g| g.flags.contains(GlyphFlags::UNSAFE_TO_CONCAT)))
        {
            continue;
        }
        if end == n {
            text_end = num_chars;
        } else {
            let c = cluster(end);
            while text_end < num_chars && chars.get(text_end).is_some_and(|&x| x < c) {
                text_end = text_end.saturating_add(1);
            }
        }
        if let Some(f) = fragments.get_mut(idx) {
            f.append(text, text_start, text_end);
        }
        text_start = text_end;
        idx ^= 1;
    }
    let [f0, f1] = fragments;
    let shaped = [shape(f0), shape(f1)];
    let parts: Vec<(Vec<GlyphInfo>, Vec<GlyphPosition>)> = shaped
        .iter()
        .map(|s| {
            let (mut i, mut p) = (s.glyph_infos().to_vec(), s.glyph_positions().to_vec());
            if !forward {
                i.reverse();
                p.reverse();
            }
            (i, p)
        })
        .collect();
    let (mut r_infos, mut r_pos) = (Vec::new(), Vec::new());
    let mut start = [0usize, 0usize];
    let mut idx = 0usize;
    let len = |k: usize| parts.get(k).map_or(0, |p| p.0.len());
    while start[0] < len(0) || start[1] < len(1) {
        let (fi, fp) = match parts.get(idx) {
            Some(p) => (&p.0, &p.1),
            None => break,
        };
        let s = start.get(idx).copied().unwrap_or(0);
        let mut e = s.saturating_add(1);
        while e < fi.len()
            && (fi.get(e).map(|g| g.cluster) == fi.get(e.saturating_sub(1)).map(|g| g.cluster)
                || fi
                    .get(e)
                    .is_some_and(|g| g.flags.contains(GlyphFlags::UNSAFE_TO_CONCAT)))
        {
            e = e.saturating_add(1);
        }
        let (a, b) = (s.min(fi.len()), e.min(fi.len()));
        r_infos.extend_from_slice(fi.get(a..b).unwrap_or_default());
        r_pos.extend_from_slice(fp.get(a..b).unwrap_or_default());
        if let Some(slot) = start.get_mut(idx) {
            *slot = e;
        }
        idx ^= 1;
    }
    if !forward {
        r_infos.reverse();
        r_pos.reverse();
    }
    let reconstruction = glyph_buffer(glyphs.direction(), r_infos, r_pos);
    if differs(&reconstruction, glyphs) {
        return Err("unsafe-to-concat test failed.");
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn unicodes_are_encoded_as_glib_does() {
        assert_eq!(encode_unicodes(b"U+0041,U+00E9").unwrap(), "Aé".as_bytes());
        assert_eq!(encode_unicodes(b"<41 42>").unwrap(), b"AB");
        assert_eq!(
            encode_unicodes(b"110000").unwrap(),
            [0xF4, 0x90, 0x80, 0x80]
        );
        assert_eq!(encode_unicodes(b"D800").unwrap(), [0xED, 0xA0, 0x80]);
        assert!(encode_unicodes(b"U+zz").is_err());
        assert_eq!(encode_unicodes(b"").unwrap(), b"");
    }

    #[test]
    fn numbers_are_scanned_as_c_does() {
        assert_eq!(scan_pair(b"1000", scan_double), Some((1000.0, None)));
        assert_eq!(
            scan_pair(b"10.5, 20", scan_double),
            Some((10.5, Some(20.0)))
        );
        assert_eq!(scan_pair(b"x", scan_double), None);
        assert_eq!(scan_pair(b"12 13", scan_int), Some((12, Some(13))));
        assert_eq!(glib_int("n", b"0x10"), Ok(16));
        assert_eq!(glib_int("n", b"010"), Ok(8));
        assert!(glib_int("n", b"1x").is_err());
        assert!(glib_int("n", b"").is_err());
    }

    #[test]
    fn lines_are_split_as_get_line_does() {
        assert_eq!(split_lines(b"a\n\nb\n", false), [&b"a"[..], b"", b"b"]);
        assert_eq!(split_lines(b"a\nb", true), [&b"a\nb"[..]]);
        assert!(split_lines(b"", false).is_empty());
    }
}
