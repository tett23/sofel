//! `sofel-subset`: subset a font as HarfBuzz 15.1.0's `hb-subset` does
//! (ADR 0022).
//!
//! [`parse`] reads `hb-subset`'s options (`util/hb-subset.cc`) as GLib's
//! option parser does: `--name=value` and `--name value`, short options,
//! `--` and the positional `[FONT-FILE] [TEXT]`. The options change the
//! [`Input`] in command-line order, as HarfBuzz's callbacks do. Glyph names
//! and `--variations` need the font; they are kept unresolved in
//! [`Options`] and applied by [`resolve`]. [`run`] is the command.

use std::cell::RefCell;
use std::ffi::OsString;
use std::io::{BufRead, Read, Write};

use sofel_font::Font;
use sofel_read::Tag;
use sofel_subset::{Flags, INVALID, Input, RangeSet, SetKind, SubsetError, axis_range_from_string};

/// The usage text, printed for `--help`.
pub const HELP: &str = "\
usage: sofel-subset [OPTION...] [FONT-FILE] [TEXT]
       sofel-subset --batch

Subsets a font as HarfBuzz's hb-subset does, with the same options.

Font-face options:
  --font-file=filename          Set font file-name
  -y, --face-index=index        Set face index (default: 0)

Subset glyph-set options (each list option also has the forms
--name+=... to add, --name-=... to subtract, and * for everything):
  -g, --gids=list               Glyph ids or ranges (decimal)
  --gids-file=filename          Read glyph ids or ranges from a file
  --glyphs=list                 Glyph names
  --glyphs-file=filename        Read glyph names from a file
  -t, --text=string             Text
  --text-file=filename          Read text from a file
  -u, --unicodes=list           Unicode code points or ranges (hex)
  --unicodes-file=filename      Read code points or ranges from a file

Subset other options:
  --gid-map=old:new,...         The glyph mapping to use
  --name-IDs=list               The name ids to keep
  --name-languages=list         The name language ids to keep
  --layout-features=list        The layout features to keep
  --layout-scripts=list         The layout scripts to keep
  --drop-tables=list            The tables to drop
  --variations=list             Instantiate a variable font, e.g.
                                wght=400,wdth=drop (alias --instance)

Subset boolean options:
  --keep-everything  --no-hinting  --retain-gids  --desubroutinize
  --downgrade-cff2  --cff-identity-charset  --name-legacy
  --set-overlaps-flag  --notdef-outline  --no-prune-unicode-ranges
  --no-layout-closure  --no-bidi-closure  --glyph-names
  --passthrough-tables  --preprocess  --iftb-requirements
  --retain-num-glyphs  --optimize

Other options:
  -n, --num-iterations=N        Run the subsetter N times (default: 1)
  -o, --output-file=filename    Set output file-name (- for stdout)
  --batch                       Read argument lines separated by ; from
                                standard input; print success or failure
  --version                     Show the version

Exit status:
  0  the subset was written
  1  wrong usage, or the font could not be subset
  2  the font could not be loaded
";

/// What the command line asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Subset the font.
    Subset,
    /// Read argument lines from standard input (`--batch` alone).
    Batch,
    /// Print the usage (`--help`, `--help-all`, `--help-<group>`, `-h`, `-?`).
    Help,
    /// Print the version (`--version`).
    Version,
    /// Print the face loaders (`--list-face-loaders`).
    ListFaceLoaders,
}

/// A `--variations` value for one axis, before the font's `fvar` is known.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AxisValue {
    /// `drop`: pin the axis to its default
    /// (`hb_subset_input_pin_axis_to_default`).
    Drop,
    /// A location or a range (`hb_subset_input_set_axis_range`), as
    /// [`axis_range_from_string`] returns it: `None` is the font's value.
    Range {
        /// The minimum.
        min: Option<f32>,
        /// The default.
        default: Option<f32>,
        /// The maximum.
        max: Option<f32>,
    },
}

/// One `tag=value` of `--variations`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AxisSpec {
    /// The axis, or `None` for `*` (every axis; the value is then
    /// [`AxisValue::Drop`], `hb_subset_input_pin_all_axes_to_default`).
    pub tag: Option<Tag>,
    /// The value.
    pub value: AxisValue,
}

/// The parsed command line.
#[derive(Clone, PartialEq, Debug)]
pub struct Options {
    /// What to do. When not [`Action::Subset`], parsing stopped at the
    /// option that asked for it, as HarfBuzz exits there.
    pub action: Action,
    /// `--font-file`, or the first positional argument.
    pub font_file: Option<String>,
    /// `--face-index` (`-y`), as the unsigned value HarfBuzz stores.
    pub face_index: u32,
    /// `--face-loader` (accepted and ignored: there is one loader).
    pub face_loader: Option<String>,
    /// `--output-file` (`-o`); `-` is standard output.
    pub output_file: Option<String>,
    /// `--output-format` (`-O`), accepted and ignored as in `hb-subset`.
    pub output_format: Option<String>,
    /// The input, with every option that does not need the font applied.
    pub input: Input,
    /// The glyph names of `--glyphs`, `--glyphs+`, `--glyphs-` and
    /// `--glyphs-file`, unresolved. (`--glyphs=*` and the clearing of the
    /// glyph set by `--glyphs=` are applied to [`Options::input`].)
    pub glyph_names: Vec<String>,
    /// The `--variations` and `--instance` values, in order.
    pub instance: Vec<AxisSpec>,
    /// `--preprocess` or `--preprocess-face` (no effect on the output).
    pub preprocess: bool,
    /// `--num-iterations` (`-n`), as the unsigned value HarfBuzz stores.
    pub num_iterations: u32,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            action: Action::Subset,
            font_file: None,
            face_index: 0,
            face_loader: None,
            output_file: None,
            output_format: None,
            input: Input::new(),
            glyph_names: Vec::new(),
            instance: Vec::new(),
            preprocess: false,
            num_iterations: 1,
        }
    }
}

/// The options, as `hb-subset` registers them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opt {
    FontFile,
    FaceIndex,
    FaceLoader,
    ListFaceLoaders,
    Gids,
    GidsFile,
    Glyphs,
    GlyphsFile,
    Text,
    TextFile,
    Unicodes,
    UnicodesFile,
    GidMap,
    NameIds,
    NameLanguages,
    LayoutFeatures,
    LayoutScripts,
    DropTables,
    Variations,
    KeepEverything,
    Flag(Flags),
    Preprocess,
    NumIterations,
    OutputFile,
    OutputFormat,
    Version,
}

/// An option entry: long name, short name (0 for none), GLib group and
/// whether it takes no argument.
struct Entry {
    long: &'static str,
    short: u8,
    group: &'static str,
    opt: Opt,
    no_arg: bool,
}

const fn e(long: &'static str, short: u8, group: &'static str, opt: Opt, no_arg: bool) -> Entry {
    Entry {
        long,
        short,
        group,
        opt,
        no_arg,
    }
}

const MAIN: &str = "";
const FACE: &str = "face";
const GLYPHSET: &str = "subset-glyphset";
const OTHER: &str = "subset-other";
const FLAGS: &str = "subset-flags";
const APP: &str = "subset-app";
const OUTPUT: &str = "output";

/// The GLib groups in the order `hb-subset` adds them.
const GROUPS: [&str; 6] = [FACE, GLYPHSET, OTHER, FLAGS, APP, OUTPUT];

const fn flag(long: &'static str, f: Flags) -> Entry {
    e(long, 0, FLAGS, Opt::Flag(f), true)
}

/// Every entry, grouped as in `hb-subset.cc`.
const ENTRIES: &[Entry] = &[
    e("version", 0, MAIN, Opt::Version, true),
    e("font-file", 0, FACE, Opt::FontFile, false),
    e("face-index", b'y', FACE, Opt::FaceIndex, false),
    e("face-loader", 0, FACE, Opt::FaceLoader, false),
    e("list-face-loaders", 0, FACE, Opt::ListFaceLoaders, true),
    e("gids", b'g', GLYPHSET, Opt::Gids, false),
    e("gids-", 0, GLYPHSET, Opt::Gids, false),
    e("gids+", 0, GLYPHSET, Opt::Gids, false),
    e("gids-file", 0, GLYPHSET, Opt::GidsFile, false),
    e("glyphs", 0, GLYPHSET, Opt::Glyphs, false),
    e("glyphs+", 0, GLYPHSET, Opt::Glyphs, false),
    e("glyphs-", 0, GLYPHSET, Opt::Glyphs, false),
    e("glyphs-file", 0, GLYPHSET, Opt::GlyphsFile, false),
    e("text", b't', GLYPHSET, Opt::Text, false),
    e("text-", 0, GLYPHSET, Opt::Text, false),
    e("text+", 0, GLYPHSET, Opt::Text, false),
    e("text-file", 0, GLYPHSET, Opt::TextFile, false),
    e("unicodes", b'u', GLYPHSET, Opt::Unicodes, false),
    e("unicodes-", 0, GLYPHSET, Opt::Unicodes, false),
    e("unicodes+", 0, GLYPHSET, Opt::Unicodes, false),
    e("unicodes-file", 0, GLYPHSET, Opt::UnicodesFile, false),
    e("gid-map", 0, OTHER, Opt::GidMap, false),
    e("name-IDs", 0, OTHER, Opt::NameIds, false),
    e("name-IDs-", 0, OTHER, Opt::NameIds, false),
    e("name-IDs+", 0, OTHER, Opt::NameIds, false),
    e("name-languages", 0, OTHER, Opt::NameLanguages, false),
    e("name-languages-", 0, OTHER, Opt::NameLanguages, false),
    e("name-languages+", 0, OTHER, Opt::NameLanguages, false),
    e("layout-features", 0, OTHER, Opt::LayoutFeatures, false),
    e("layout-features+", 0, OTHER, Opt::LayoutFeatures, false),
    e("layout-features-", 0, OTHER, Opt::LayoutFeatures, false),
    e("layout-scripts", 0, OTHER, Opt::LayoutScripts, false),
    e("layout-scripts+", 0, OTHER, Opt::LayoutScripts, false),
    e("layout-scripts-", 0, OTHER, Opt::LayoutScripts, false),
    e("drop-tables", 0, OTHER, Opt::DropTables, false),
    e("drop-tables+", 0, OTHER, Opt::DropTables, false),
    e("drop-tables-", 0, OTHER, Opt::DropTables, false),
    e("variations", 0, OTHER, Opt::Variations, false),
    e("instance", 0, OTHER, Opt::Variations, false),
    e("keep-everything", 0, FLAGS, Opt::KeepEverything, true),
    flag("no-hinting", Flags::NO_HINTING),
    flag("retain-gids", Flags::RETAIN_GIDS),
    flag("desubroutinize", Flags::DESUBROUTINIZE),
    flag("downgrade-cff2", Flags::DOWNGRADE_CFF2),
    flag("cff-identity-charset", Flags::CFF_IDENTITY_CHARSET),
    flag("name-legacy", Flags::NAME_LEGACY),
    flag("set-overlaps-flag", Flags::SET_OVERLAPS_FLAG),
    flag("notdef-outline", Flags::NOTDEF_OUTLINE),
    flag("no-prune-unicode-ranges", Flags::NO_PRUNE_UNICODE_RANGES),
    flag("no-layout-closure", Flags::NO_LAYOUT_CLOSURE),
    flag("no-bidi-closure", Flags::NO_BIDI_CLOSURE),
    flag("glyph-names", Flags::GLYPH_NAMES),
    flag("passthrough-tables", Flags::PASSTHROUGH_UNRECOGNIZED),
    e("preprocess-face", 0, FLAGS, Opt::Preprocess, true),
    e("preprocess", 0, FLAGS, Opt::Preprocess, true),
    flag("iftb-requirements", Flags::IFTB_REQUIREMENTS),
    flag("retain-num-glyphs", Flags::RETAIN_NUM_GLYPHS),
    flag("optimize", Flags::OPTIMIZE_IUP_DELTAS),
    e("num-iterations", b'n', APP, Opt::NumIterations, false),
    e("output-file", b'o', OUTPUT, Opt::OutputFile, false),
    e("output-format", b'O', OUTPUT, Opt::OutputFormat, false),
];

/// How a list option changes its set, from the last character of the
/// option name as HarfBuzz's callbacks read it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Replace,
    Add,
    Remove,
}

fn mode_of(option_name: &str) -> Mode {
    match option_name.as_bytes().last() {
        Some(b'-') => Mode::Remove,
        Some(b'+') => Mode::Add,
        _ => Mode::Replace,
    }
}

/// Reads a file named by a `-file` option; `-` is standard input.
pub type ReadFile<'a> = dyn Fn(&str) -> Result<Vec<u8>, String> + 'a;

/// Parses `hb-subset`'s arguments (without the program name), reading
/// the `-file` options' files from the file system and `-` from standard
/// input.
///
/// # Errors
///
/// The message GLib or `hb-subset` would print, e.g.
/// `Unknown option --foo` or `Failed parsing glyph-index at: 'x'`.
pub fn parse(args: &[String]) -> Result<Options, String> {
    parse_with(args, &read_file_or_stdin)
}

fn read_file_or_stdin(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut v = Vec::new();
        std::io::stdin()
            .read_to_end(&mut v)
            .map_err(|e| io_message(&e))?;
        Ok(v)
    } else {
        std::fs::read(path).map_err(|e| io_message(&e))
    }
}

/// An I/O error as `strerror` writes it (without Rust's `(os error N)`).
fn io_message(e: &std::io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error ") {
        Some(i) => s.get(..i).unwrap_or(&s).to_string(),
        None => s,
    }
}

/// [`parse`] with the files read by `read`.
///
/// # Errors
///
/// As [`parse`].
pub fn parse_with(args: &[String], read: &ReadFile<'_>) -> Result<Options, String> {
    if let [only] = args
        && only == "--batch"
    {
        return Ok(Options {
            action: Action::Batch,
            ..Options::default()
        });
    }
    let mut p = Parser {
        opts: Options::default(),
        read,
    };
    let mut i = 0usize;
    let mut stop_parsing = false;
    while let Some(arg) = args.get(i) {
        let bytes = arg.as_bytes();
        let is_option = !stop_parsing && bytes.first() == Some(&b'-') && bytes.len() > 1;
        if !is_option {
            p.remaining(arg)?;
        } else if let Some(long) = arg.strip_prefix("--") {
            if long.is_empty() {
                stop_parsing = true;
            } else if is_help(long) {
                p.opts.action = Action::Help;
                return Ok(p.opts);
            } else {
                i = p.long_option(args, i, long)?;
            }
        } else {
            i = p.short_options(args, i)?;
        }
        if p.opts.action != Action::Subset {
            return Ok(p.opts);
        }
        i = i.saturating_add(1);
    }
    Ok(p.opts)
}

fn is_help(long: &str) -> bool {
    long == "help"
        || long == "help-all"
        || long
            .strip_prefix("help-")
            .is_some_and(|g| GROUPS.contains(&g))
}

struct Parser<'r, 'a> {
    opts: Options,
    read: &'r ReadFile<'a>,
}

impl Parser<'_, '_> {
    /// `collect_rest`: the first positional argument is the font file, the
    /// others are text. GLib names the option `""` for them; HarfBuzz reads
    /// the byte before it, which is a NUL in practice, so the text replaces
    /// the set.
    fn remaining(&mut self, arg: &str) -> Result<(), String> {
        if self.opts.font_file.is_none() {
            self.opts.font_file = Some(arg.to_string());
            return Ok(());
        }
        // `collect_rest` ignores `parse_text`'s result, which never fails.
        self.text(Mode::Replace, arg);
        Ok(())
    }

    /// `parse_long_option` on the main group, the groups, and then the
    /// `--<group>-<option>` aliases. Returns the index of the last argument
    /// used.
    fn long_option(&mut self, args: &[String], i: usize, long: &str) -> Result<usize, String> {
        let direct = core::iter::once(MAIN)
            .chain(GROUPS)
            .find_map(|g| self.match_long(g, long).map(|(en, v)| (en, v, long)));
        let found = direct.or_else(|| {
            let dash = long.find('-').filter(|&d| d > 0)?;
            let prefix = long.get(..dash)?;
            let rest = long.get(dash.saturating_add(1)..)?;
            GROUPS
                .iter()
                .filter(|g| g.as_bytes().get(..dash) == Some(prefix.as_bytes()))
                .find_map(|g| self.match_long(g, rest).map(|(en, v)| (en, v, rest)))
        });
        let Some((entry, inline, _)) = found else {
            return Err(format!(
                "Unknown option {}",
                args.get(i).map_or("", String::as_str)
            ));
        };
        let name = format!("--{}", entry.long);
        if entry.no_arg {
            self.apply(entry.opt, &name, None)?;
            return Ok(i);
        }
        match inline {
            Some(v) => {
                self.apply(entry.opt, &name, Some(v))?;
                Ok(i)
            }
            None => {
                let next = i.saturating_add(1);
                let v = args
                    .get(next)
                    .ok_or_else(|| format!("Missing argument for {name}"))?;
                self.apply(entry.opt, &name, Some(v))?;
                Ok(next)
            }
        }
    }

    /// The entry of `group` that `arg` names, and its `=value`.
    fn match_long<'s>(
        &self,
        group: &str,
        arg: &'s str,
    ) -> Option<(&'static Entry, Option<&'s str>)> {
        ENTRIES
            .iter()
            .filter(|en| en.group == group)
            .find_map(|en| {
                if en.no_arg && arg == en.long {
                    return Some((en, None));
                }
                let rest = arg.strip_prefix(en.long)?;
                if rest.is_empty() {
                    Some((en, None))
                } else {
                    rest.strip_prefix('=').map(|v| (en, Some(v)))
                }
            })
    }

    /// A cluster of short options (`-o file`, `-u 41`). Returns the index
    /// of the last argument used.
    fn short_options(&mut self, args: &[String], i: usize) -> Result<usize, String> {
        let arg = args.get(i).map_or("", String::as_str);
        let mut new_i = i;
        for &c in arg.as_bytes().iter().skip(1) {
            if c == b'?' || c == b'h' {
                self.opts.action = Action::Help;
                return Ok(new_i);
            }
            let Some(entry) = ENTRIES.iter().find(|en| en.short == c && c != 0) else {
                return Err(format!("Unknown option {arg}"));
            };
            let name = format!("-{}", char::from(c));
            if entry.no_arg {
                self.apply(entry.opt, &name, None)?;
                continue;
            }
            if new_i > i {
                return Err(format!("Error parsing option {name}"));
            }
            let next = i.saturating_add(1);
            let v = args
                .get(next)
                .ok_or_else(|| format!("Missing argument for {name}"))?;
            new_i = next;
            self.apply(entry.opt, &name, Some(v))?;
            if self.opts.action != Action::Subset {
                return Ok(new_i);
            }
        }
        Ok(new_i)
    }

    /// Runs the option's GLib storage or `hb-subset` callback.
    fn apply(&mut self, opt: Opt, name: &str, value: Option<&str>) -> Result<(), String> {
        let v = value.unwrap_or("");
        let mode = mode_of(name);
        let generic = || format!("Error parsing option {name}");
        match opt {
            Opt::FontFile => self.opts.font_file = Some(v.to_string()),
            Opt::FaceIndex => self.opts.face_index = parse_int(name, v)?,
            Opt::FaceLoader => self.opts.face_loader = Some(v.to_string()),
            Opt::ListFaceLoaders => self.opts.action = Action::ListFaceLoaders,
            Opt::Version => self.opts.action = Action::Version,
            Opt::OutputFile => self.opts.output_file = Some(v.to_string()),
            Opt::OutputFormat => self.opts.output_format = Some(v.to_string()),
            Opt::NumIterations => self.opts.num_iterations = parse_int(name, v)?,
            Opt::Preprocess => self.opts.preprocess = true,
            Opt::KeepEverything => {
                self.opts.input = core::mem::take(&mut self.opts.input).keep_everything()
            }
            Opt::Flag(f) => {
                let input = core::mem::take(&mut self.opts.input);
                let flags = input.flags() | f;
                self.opts.input = input.with_flags(flags);
            }
            Opt::Gids => self.numbers(SetKind::Glyphs, mode, v, NumberList::Gids)?,
            Opt::Unicodes => self.numbers(SetKind::Unicodes, mode, v, NumberList::Unicodes)?,
            Opt::NameIds => self.numbers(SetKind::NameIds, mode, v, NumberList::NameIds)?,
            Opt::NameLanguages => {
                self.numbers(SetKind::NameLanguages, mode, v, NumberList::NameLanguages)?
            }
            Opt::Text => self.text(mode, v),
            Opt::Glyphs => self.glyphs(mode, v),
            Opt::LayoutFeatures => self.tags(SetKind::LayoutFeatures, mode, v)?,
            Opt::LayoutScripts => self.tags(SetKind::LayoutScripts, mode, v)?,
            Opt::DropTables => self.tags(SetKind::DropTables, mode, v)?,
            Opt::GidMap => self.gid_map(v)?,
            Opt::Variations => {
                let specs = parse_instancing_spec(v)?.ok_or_else(generic)?;
                self.opts.instance.extend(specs);
            }
            Opt::GidsFile => self.file(v, true, |p, line| {
                p.numbers(SetKind::Glyphs, Mode::Add, line, NumberList::Gids)
            })?,
            Opt::UnicodesFile => self.file(v, true, |p, line| {
                p.numbers(SetKind::Unicodes, Mode::Add, line, NumberList::Unicodes)
            })?,
            Opt::GlyphsFile => self.file(v, true, |p, line| {
                p.glyphs(Mode::Add, line);
                Ok(())
            })?,
            Opt::TextFile => self.file(v, false, |p, line| {
                p.text(Mode::Add, line);
                Ok(())
            })?,
        }
        Ok(())
    }

    /// Changes the set of `kind` with `f`, which may fail after changing it
    /// partly, as HarfBuzz's callbacks do.
    fn with_set(
        &mut self,
        kind: SetKind,
        f: impl FnOnce(&mut RangeSet) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut result = Ok(());
        let input = core::mem::take(&mut self.opts.input);
        self.opts.input = input.map_set(kind, |s| result = f(s));
        result
    }

    /// The start of every list callback: a plain option clears the set; `*`
    /// makes it everything, or nothing for the `-` form. Returns whether
    /// the argument was `*`.
    fn begin(s: &mut RangeSet, mode: Mode, arg: &str) -> bool {
        if mode == Mode::Replace {
            s.clear();
        }
        if arg == "*" {
            s.clear();
            if mode != Mode::Remove {
                s.invert();
            }
            return true;
        }
        false
    }

    fn numbers(
        &mut self,
        kind: SetKind,
        mode: Mode,
        arg: &str,
        list: NumberList,
    ) -> Result<(), String> {
        self.with_set(kind, |s| {
            if Self::begin(s, mode, arg) {
                return Ok(());
            }
            parse_numbers(s, mode == Mode::Remove, arg.as_bytes(), list)
        })
    }

    /// `parse_text`.
    fn text(&mut self, mode: Mode, arg: &str) {
        let _ = self.with_set(SetKind::Unicodes, |s| {
            if !Self::begin(s, mode, arg) {
                arg.chars()
                    .for_each(|c| change(s, mode == Mode::Remove, u32::from(c), u32::from(c)));
            }
            Ok(())
        });
    }

    /// `parse_glyphs`: `*` and the clearing are applied; the names are
    /// kept for [`resolve`].
    fn glyphs(&mut self, mode: Mode, arg: &str) {
        let _ = self.with_set(SetKind::Glyphs, |s| {
            Self::begin(s, mode, arg);
            Ok(())
        });
        if arg != "*" {
            let names = arg
                .split([' ', ','])
                .filter(|n| !n.is_empty())
                .map(str::to_string);
            self.opts.glyph_names.extend(names);
        }
    }

    /// `parse_layout_tag_list` and `parse_drop_tables`.
    fn tags(&mut self, kind: SetKind, mode: Mode, arg: &str) -> Result<(), String> {
        self.with_set(kind, |s| {
            if Self::begin(s, mode, arg) {
                return Ok(());
            }
            arg.split([',', ' '])
                .filter(|t| !t.is_empty())
                .try_for_each(|t| {
                    if t.len() > 4 {
                        return Err(format!("Failed parsing table tag at: '{t}'"));
                    }
                    let tag = tag_from_string(t.as_bytes());
                    change(s, mode == Mode::Remove, tag, tag);
                    Ok(())
                })
        })
    }

    /// `parse_glyph_map`.
    fn gid_map(&mut self, arg: &str) -> Result<(), String> {
        let pairs = parse_glyph_map(arg.as_bytes())?;
        let input = core::mem::take(&mut self.opts.input);
        let mut map = input.glyph_mapping().clone();
        map.extend(pairs.iter().copied());
        self.opts.input = input
            .map_set(SetKind::Glyphs, |s| {
                pairs
                    .iter()
                    .for_each(|&(old, _)| change(s, false, old, old))
            })
            .with_glyph_mapping(map);
        Ok(())
    }

    /// `parse_file_for`: each line (without the newline, and before `#`
    /// when comments are allowed) goes to `line` as the `+` form. As in
    /// HarfBuzz, a line that fails stops reading the file without failing
    /// the option.
    fn file(
        &mut self,
        path: &str,
        comments: bool,
        mut line: impl FnMut(&mut Self, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        let data = (self.read)(path).map_err(|e| format!("Failed opening file `{path}': {e}"))?;
        let _ = data.split(|&b| b == b'\n').try_for_each(|l| {
            let l = l.split(|&b| b == 0).next().unwrap_or(&[]);
            let l = if comments {
                l.split(|&b| b == b'#').next().unwrap_or(&[])
            } else {
                l
            };
            line(self, &String::from_utf8_lossy(l))
        });
        Ok(())
    }
}

/// Adds or removes `a..=b` as `hb_set_add_range` and `hb_set_del_range`
/// do, including their refusal of a range ending at [`INVALID`] when
/// adding to the underlying (not inverted) bits.
fn change(s: &mut RangeSet, remove: bool, a: u32, b: u32) {
    if a > b || a == INVALID {
        return;
    }
    // The bits change the other way in an inverted set.
    let adds_bits = remove == s.is_inverted();
    if adds_bits && b == INVALID {
        return;
    }
    if remove {
        s.remove_range(a, b);
    } else {
        s.add_range(a, b);
    }
}

/// `hb_tag_from_string`: up to four bytes, padded with spaces.
fn tag_from_string(s: &[u8]) -> u32 {
    let s = s.split(|&b| b == 0).next().unwrap_or(&[]);
    if s.is_empty() {
        return 0;
    }
    let bytes: [u8; 4] = core::array::from_fn(|i| s.get(i).copied().unwrap_or(b' '));
    u32::from_be_bytes(bytes)
}

/// The list options that take numbers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NumberList {
    /// `parse_gids`: decimal ranges.
    Gids,
    /// `parse_unicodes`: hexadecimal ranges with many delimiters.
    Unicodes,
    /// `parse_nameids`: decimal numbers.
    NameIds,
    /// `parse_name_languages`: decimal numbers.
    NameLanguages,
}

/// The delimiters of `parse_unicodes`.
const UNICODE_DELIMITERS: &[u8] = b"<+->{},;&#\\xXuUnNiI\n\t\x0b\x0c\r ";

/// Parses a number list into `s` as HarfBuzz's callbacks do, changing the
/// set up to the first error.
fn parse_numbers(
    s: &mut RangeSet,
    remove: bool,
    arg: &[u8],
    list: NumberList,
) -> Result<(), String> {
    let (delims, base): (&[u8], u32) = match list {
        NumberList::Unicodes => (UNICODE_DELIMITERS, 16),
        _ => (b", ", 10),
    };
    let what = match list {
        NumberList::Gids => "glyph-index",
        NumberList::Unicodes => "Unicode",
        NumberList::NameIds => "nameID",
        NumberList::NameLanguages => "name-language code",
    };
    let ranges = matches!(list, NumberList::Gids | NumberList::Unicodes);
    let reject_minus = list == NumberList::Gids;
    let fail = |at: &[u8]| {
        format!(
            "Failed parsing {what} at: '{}'",
            String::from_utf8_lossy(at)
        )
    };
    let mut rest = arg;
    loop {
        let skip = rest.iter().take_while(|b| delims.contains(b)).count();
        rest = rest.get(skip..).unwrap_or(&[]);
        if rest.is_empty() {
            return Ok(());
        }
        let first = strtoul(rest, base);
        if (reject_minus && rest.first() == Some(&b'-')) || first.erange || first.end == 0 {
            return Err(fail(rest));
        }
        let start = low32(first.value);
        let after = rest.get(first.end..).unwrap_or(&[]);
        if ranges && after.first() == Some(&b'-') {
            let s2 = after.get(1..).unwrap_or(&[]);
            let second = strtoul(s2, base);
            if s2.first() == Some(&b'-') || second.erange || second.end == 0 {
                return Err(fail(s2));
            }
            let end = low32(second.value);
            if end < start {
                let what = if list == NumberList::Gids {
                    "glyph-index"
                } else {
                    "Unicode"
                };
                return Err(format!("Invalid {what} range {start}-{end}"));
            }
            change(s, remove, start, end);
            rest = s2.get(second.end..).unwrap_or(&[]);
        } else {
            change(s, remove, start, start);
            rest = after;
        }
    }
}

/// `parse_glyph_map`: `old:new` pairs separated by `,` or spaces.
fn parse_glyph_map(arg: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let fail = |at: &[u8]| {
        format!(
            "Failed parsing glyph map at: '{}'",
            String::from_utf8_lossy(at)
        )
    };
    let mut rest = arg;
    let mut out = Vec::new();
    loop {
        let skip = rest.iter().take_while(|b| b", ".contains(b)).count();
        rest = rest.get(skip..).unwrap_or(&[]);
        if rest.is_empty() {
            return Ok(out);
        }
        let old = strtoul(rest, 10);
        if rest.first() == Some(&b'-') || old.erange || old.end == 0 {
            return Err(fail(rest));
        }
        let after = rest.get(old.end..).unwrap_or(&[]);
        if after.first() != Some(&b':') {
            return Err(fail(rest));
        }
        let s2 = after.get(1..).unwrap_or(&[]);
        let new = strtoul(s2, 10);
        if s2.first() == Some(&b'-') || new.erange || new.end == 0 {
            return Err(fail(s2));
        }
        out.push((low32(old.value), low32(new.value)));
        rest = s2.get(new.end..).unwrap_or(&[]);
    }
}

/// The low 32 bits, as C's conversion of `unsigned long` to
/// `hb_codepoint_t`.
fn low32(v: u64) -> u32 {
    u32::try_from(v & u64::from(u32::MAX)).unwrap_or(u32::MAX)
}

/// The result of [`strtoul`].
struct Converted {
    value: u64,
    /// The bytes used; 0 when no number was converted.
    end: usize,
    /// `errno == ERANGE`.
    erange: bool,
}

fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// C's `strtoul` with a 64-bit `unsigned long`, for base 10 or 16: white
/// space, a sign (a minus negates the value modulo 2^64), `0x` for base
/// 16, and digits; overflow gives `ULONG_MAX` and `ERANGE`.
fn strtoul(s: &[u8], base: u32) -> Converted {
    let ws = s.iter().take_while(|&&b| is_c_space(b)).count();
    let t = s.get(ws..).unwrap_or(&[]);
    let (neg, sign_len) = match t.first() {
        Some(b'-') => (true, 1usize),
        Some(b'+') => (false, 1),
        _ => (false, 0),
    };
    let t = t.get(sign_len..).unwrap_or(&[]);
    let digit = |b: u8| char::from(b).to_digit(base);
    let prefix = match t {
        [b'0', b'x' | b'X', d, ..] if base == 16 && digit(*d).is_some() => 2usize,
        _ => 0,
    };
    let digits = t.get(prefix..).unwrap_or(&[]);
    let n = digits.iter().take_while(|&&b| digit(b).is_some()).count();
    if n == 0 {
        return Converted {
            value: 0,
            end: 0,
            erange: false,
        };
    }
    let value = digits.iter().take(n).try_fold(0u64, |acc, &b| {
        acc.checked_mul(u64::from(base))?
            .checked_add(u64::from(digit(b).unwrap_or(0)))
    });
    let end = ws
        .saturating_add(sign_len)
        .saturating_add(prefix)
        .saturating_add(n);
    match value {
        None => Converted {
            value: u64::MAX,
            end,
            erange: true,
        },
        Some(v) => Converted {
            value: if neg { v.wrapping_neg() } else { v },
            end,
            erange: false,
        },
    }
}

/// GLib's `parse_int` (`strtol` with base 0, the whole string, within
/// `gint`), stored in HarfBuzz's `unsigned` field.
fn parse_int(name: &str, arg: &str) -> Result<u32, String> {
    let bad = || format!("Cannot parse integer value \u{201c}{arg}\u{201d} for {name}");
    let range = || format!("Integer value \u{201c}{arg}\u{201d} for {name} out of range");
    let b = arg.as_bytes();
    let ws = b.iter().take_while(|&&c| is_c_space(c)).count();
    let t = b.get(ws..).unwrap_or(&[]);
    let (neg, t) = match t.first() {
        Some(b'-') => (true, t.get(1..).unwrap_or(&[])),
        Some(b'+') => (false, t.get(1..).unwrap_or(&[])),
        _ => (false, t),
    };
    let (base, digits) = match t {
        [b'0', b'x' | b'X', d, ..] if d.is_ascii_hexdigit() => (16u32, t.get(2..).unwrap_or(&[])),
        [b'0', ..] => (8, t),
        _ => (10, t),
    };
    let valid = !digits.is_empty() && digits.iter().all(|&c| char::from(c).is_digit(base));
    if arg.is_empty() || !valid {
        return Err(bad());
    }
    let magnitude = digits.iter().try_fold(0i64, |acc, &c| {
        acc.checked_mul(i64::from(base))?
            .checked_add(i64::from(char::from(c).to_digit(base).unwrap_or(0)))
    });
    let value = magnitude
        .map(|m| if neg { m.saturating_neg() } else { m })
        .ok_or_else(range)?;
    let v = i32::try_from(value).map_err(|_| range())?;
    Ok(u32::from_ne_bytes(v.to_ne_bytes()))
}

/// C's `strtok` over one string, with a delimiter set per call.
struct Strtok<'s> {
    rest: &'s str,
}

impl<'s> Strtok<'s> {
    fn next(&mut self, delims: &[char]) -> Option<&'s str> {
        let start = self.rest.trim_start_matches(delims);
        if start.is_empty() {
            self.rest = start;
            return None;
        }
        match start.find(delims) {
            Some(i) => {
                let (tok, after) = start.split_at(i);
                let mut chars = after.chars();
                chars.next();
                self.rest = chars.as_str();
                Some(tok)
            }
            None => {
                self.rest = "";
                Some(start)
            }
        }
    }
}

/// `parse_instancing_spec`, without the font: the axis specs, `Ok(None)`
/// when HarfBuzz fails without a message (a value
/// `hb_subset_axis_range_from_string` rejects), or the message.
fn parse_instancing_spec(arg: &str) -> Result<Option<Vec<AxisSpec>>, String> {
    let mut tok = Strtok { rest: arg };
    let mut out = Vec::new();
    while let Some(s) = tok.next(&['=']) {
        if s.len() > 4 {
            return Err(format!("Failed parsing axis tag at: '{s}'"));
        }
        if s == "*" {
            match tok.next(&[',', ' ']) {
                Some("drop") => out.push(AxisSpec {
                    tag: None,
                    value: AxisValue::Drop,
                }),
                v => {
                    return Err(format!(
                        "Failed parsing axis position at: '{}'",
                        v.unwrap_or("(null)")
                    ));
                }
            }
            continue;
        }
        let tag = Tag::from_u32(tag_from_string(s.as_bytes()));
        let Some(v) = tok.next(&[',', ' ']) else {
            let name: String = tag.0.iter().map(|&b| char::from(b)).collect();
            return Err(format!("Value not specified for axis: {name}"));
        };
        let value = if v == "drop" {
            AxisValue::Drop
        } else {
            match axis_range_from_string(v) {
                Some((min, default, max)) => AxisValue::Range { min, default, max },
                None => return Ok(None),
            }
        };
        out.push(AxisSpec {
            tag: Some(tag),
            value,
        });
    }
    Ok(Some(out))
}

/// The input with the options that need the font applied: glyph names
/// (`hb_font_glyph_from_string`) and `--variations`
/// (`parse_instancing_spec` with the face).
///
/// # Errors
///
/// HarfBuzz's message for an axis the font does not have (or `*=drop`
/// on a font without axes) and a range with the minimum above the
/// maximum. For now, glyph names (ADR 0022 stage 12) and any
/// `--variations` other than pinning axes to their defaults (deferred
/// by ADR 0022) also give an error.
pub fn resolve(options: &Options, font: &Font<'_>) -> Result<Input, String> {
    if !options.glyph_names.is_empty() {
        return Err("glyph names are not supported yet (ADR 0022 stage 12)".to_string());
    }
    options
        .instance
        .iter()
        .try_fold(options.input.clone(), |input, spec| {
            apply_axis_spec(input, spec, font)
        })
}

/// One `--variations` spec applied to `input` as `parse_instancing_spec`
/// does with the face.
fn apply_axis_spec(input: Input, spec: &AxisSpec, font: &Font<'_>) -> Result<Input, String> {
    let name = |tag: Tag| -> String { tag.0.iter().map(|&b| char::from(b)).collect() };
    let not_in_fvar = |tag: Tag| format!("Cannot pin axis: '{}', not present in fvar", name(tag));
    let Some(tag) = spec.tag else {
        return input
            .with_all_axes_pinned_to_default(font)
            .ok_or_else(|| "Failed pinning all axes to default.".to_string());
    };
    let next = match spec.value {
        AxisValue::Drop => input.with_axis_pinned_to_default(tag, font),
        AxisValue::Range { min, default, max } => {
            input.with_axis_range(tag, min, max, default, font)
        }
    }
    .ok_or_else(|| not_in_fvar(tag))?;
    // Only pinning to the default is implemented; HarfBuzz's set_axis_range
    // gives the same input for a value equal to the default.
    let pinned = next.axis_range(tag).is_some_and(|r| {
        let default = input
            .with_axis_pinned_to_default(tag, font)
            .and_then(|i| i.axis_range(tag));
        Some(r) == default
    });
    if pinned {
        Ok(next)
    } else {
        Err(format!(
            "instancing '{}' other than to its default is not supported yet (deferred by ADR 0022)",
            name(tag)
        ))
    }
}

const PROGRAM: &str = "sofel-subset";

/// How one command line ended.
enum Outcome {
    /// `subset_main_t::operator ()` returned this (0 or 1).
    Returned(i32),
    /// HarfBuzz's `fail` (or `--help`, `--version`) exited the process.
    Exit(i32),
}

/// Runs `sofel-subset` with `args` (without the program name), as
/// `hb-subset`'s `batch_main` does. Returns the exit status.
pub fn run(
    args: &[OsString],
    stdin: &mut dyn BufRead,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let args: Option<Vec<String>> = args
        .iter()
        .map(|a| a.to_str().map(str::to_string))
        .collect();
    let Some(args) = args else {
        let _ = writeln!(err, "{PROGRAM}: arguments must be UTF-8");
        return 1;
    };
    let stdin = RefCell::new(stdin);
    if let [only] = args.as_slice()
        && only == "--batch"
    {
        return batch(&stdin, out, err);
    }
    match run_one(&args, &stdin, out, err) {
        Outcome::Returned(0) => 0,
        Outcome::Returned(code) => {
            let _ = writeln!(
                out,
                "error: Operation failed. Probably a bug. File github issue."
            );
            code
        }
        Outcome::Exit(code) => code,
    }
}

/// `--batch`: each line of standard input is a command line, its
/// arguments separated by `;` (runs of `;` count as one), at most 63 of
/// them; lines are read in pieces of at most 4091 bytes as `fgets` into
/// HarfBuzz's buffer. Prints `success` or `failure` per line. An error
/// that makes `hb-subset` exit (an option error, a font that cannot be
/// loaded) ends the batch with that status, as it ends HarfBuzz's process.
fn batch(stdin: &RefCell<&mut dyn BufRead>, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut ret = 0;
    loop {
        let Some(chunk) = read_chunk(stdin) else {
            return ret;
        };
        let line = chunk.strip_suffix(b"\n").unwrap_or(&chunk);
        let line = line.split(|&b| b == 0).next().unwrap_or(&[]);
        let args = split_batch_line(&String::from_utf8_lossy(line));
        let result = match run_one(&args, stdin, out, err) {
            Outcome::Returned(r) => r,
            Outcome::Exit(code) => return code,
        };
        let _ = out.write_all(if result == 0 {
            b"success\n"
        } else {
            b"failure\n"
        });
        let _ = out.flush();
        ret = ret.max(result);
    }
}

/// One `fgets` into a 4092-byte buffer: up to and including a newline, at
/// most 4091 bytes; `None` at the end of input.
fn read_chunk(stdin: &RefCell<&mut dyn BufRead>) -> Option<Vec<u8>> {
    const MAX: usize = 4091;
    let mut r = stdin.borrow_mut();
    let mut chunk = Vec::new();
    while chunk.len() < MAX {
        let buf = match r.fill_buf() {
            Ok(b) => b,
            Err(_) => break,
        };
        if buf.is_empty() {
            break;
        }
        let room = MAX.saturating_sub(chunk.len());
        let take = buf
            .iter()
            .take(room)
            .position(|&b| b == b'\n')
            .map_or(buf.len().min(room), |i| i.saturating_add(1));
        chunk.extend_from_slice(buf.get(..take).unwrap_or(&[]));
        let newline = chunk.last() == Some(&b'\n');
        r.consume(take);
        if newline {
            break;
        }
    }
    if chunk.is_empty() { None } else { Some(chunk) }
}

/// Splits a batch line as `batch_main` does.
fn split_batch_line(line: &str) -> Vec<String> {
    const MAX_ARGS: usize = 63;
    let mut args = Vec::new();
    let mut rest = line;
    loop {
        match rest
            .find(';')
            .filter(|_| args.len() < MAX_ARGS.saturating_sub(1))
        {
            Some(i) => {
                args.push(rest.get(..i).unwrap_or("").to_string());
                rest = rest
                    .get(i.saturating_add(1)..)
                    .unwrap_or("")
                    .trim_start_matches(';');
            }
            None => {
                args.push(rest.to_string());
                return args;
            }
        }
    }
}

/// `subset_main_t::operator ()` for one command line.
fn run_one(
    args: &[String],
    stdin: &RefCell<&mut dyn BufRead>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Outcome {
    let fail = |err: &mut dyn Write, help: bool, code: i32, msg: &str| {
        let _ = writeln!(err, "{PROGRAM}: {msg}");
        if help {
            let _ = writeln!(err, "Try `{PROGRAM} --help' for more information.");
        }
        Outcome::Exit(code)
    };
    let read_stdin = || {
        let mut v = Vec::new();
        stdin
            .borrow_mut()
            .read_to_end(&mut v)
            .map(|_| v)
            .map_err(|e| io_message(&e))
    };
    let read = |path: &str| {
        if path == "-" {
            read_stdin()
        } else {
            std::fs::read(path).map_err(|e| io_message(&e))
        }
    };
    let options = match parse_with(args, &read) {
        Ok(o) => o,
        Err(msg) => return fail(err, true, 1, &msg),
    };
    match options.action {
        Action::Subset => {}
        Action::Batch => return fail(err, true, 1, "Unknown option --batch"),
        Action::Help => {
            let _ = out.write_all(HELP.as_bytes());
            return Outcome::Exit(0);
        }
        Action::Version => {
            let _ = writeln!(out, "{PROGRAM} (sofel) {}", env!("CARGO_PKG_VERSION"));
            return Outcome::Exit(0);
        }
        Action::ListFaceLoaders => {
            let _ = writeln!(out, "ot");
            return Outcome::Exit(0);
        }
    }
    let Some(path) = options.font_file.as_deref() else {
        return fail(err, true, 1, "No font file set");
    };
    let font_path = if path == "-" { "/dev/stdin" } else { path };
    let Ok(data) = read(path) else {
        return fail(
            err,
            true,
            2,
            &format!("{font_path}: Failed loading font face"),
        );
    };
    let Ok(font) = Font::new(&data, options.face_index) else {
        return fail(
            err,
            true,
            2,
            &format!("{font_path}: Failed loading font face"),
        );
    };
    let Some(output) = options.output_file.as_deref() else {
        return fail(err, true, 1, "No output file was specified");
    };
    let mut file = if output == "-" {
        None
    } else {
        match std::fs::File::create(output) {
            Ok(f) => Some(f),
            Err(e) => {
                let msg = format!("Cannot open output file `{output}': {}", io_message(&e));
                return fail(err, true, 1, &msg);
            }
        }
    };
    let input = match resolve(&options, &font) {
        Ok(i) => i,
        Err(msg) => return fail(err, true, 1, &msg),
    };
    let result =
        (0..options.num_iterations).fold(None, |_, _| Some(sofel_subset::subset(&font, &input)));
    match result {
        Some(Ok(bytes)) => {
            let written = match file.as_mut() {
                Some(f) => f.write_all(&bytes),
                None => out.write_all(&bytes),
            };
            match written {
                Ok(()) => Outcome::Returned(0),
                Err(e) => fail(
                    err,
                    false,
                    1,
                    &format!("Failed to write output: {}", io_message(&e)),
                ),
            }
        }
        Some(Err(SubsetError::NoGlyphs)) => fail(err, false, 1, "Invalid font file."),
        Some(Err(e)) => {
            let _ = writeln!(err, "{PROGRAM}: {e}");
            Outcome::Returned(1)
        }
        None => Outcome::Returned(1),
    }
}
