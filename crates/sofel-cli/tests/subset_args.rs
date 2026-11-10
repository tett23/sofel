//! `sofel_cli::subset`'s argument parsing against `util/hb-subset.cc`
//! (ADR 0022), and the round trip through `Input::to_args`.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use sofel_cli::subset::{Action, AxisSpec, AxisValue, Options, parse, parse_with, resolve, run};
use sofel_read::Tag;
use sofel_subset::{DEFAULT_DROP_TABLES, Flags, Input, RangeSet, SetKind};

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn ok(a: &[&str]) -> Options {
    parse(&args(a)).unwrap_or_else(|e| panic!("{a:?}: {e}"))
}

fn err(a: &[&str]) -> String {
    match parse(&args(a)) {
        Ok(o) => panic!("{a:?} parsed: {o:?}"),
        Err(e) => e,
    }
}

fn set(o: &Options, kind: SetKind) -> &RangeSet {
    o.input.set(kind)
}

fn members(s: &RangeSet) -> Vec<u32> {
    assert!(!s.is_inverted());
    s.iter().collect()
}

fn tag(t: &[u8; 4]) -> u32 {
    Tag::new(t).to_u32()
}

fn harfbuzz() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata/harfbuzz");
    if p.join("util/hb-subset.cc").exists() {
        Some(p)
    } else {
        eprintln!(
            "skipped: {} is missing (run scripts/fetch-testdata.sh)",
            p.display()
        );
        None
    }
}

/// The fixed arguments of test/subset/run-tests.py.
fn run_tests_args(unicodes: &str) -> Vec<String> {
    args(&[
        "--font-file=font.ttf",
        "--output-file=out.ttf",
        &format!("--unicodes={unicodes}"),
        "--drop-tables+=DSIG,BASE",
        "--drop-tables-=sbix",
    ])
}

#[test]
fn every_profile_parses() {
    let Some(hb) = harfbuzz() else { return };
    let dir = hb.join("test/subset/data/profiles");
    let mut n = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for preprocess in [false, true] {
            let mut a = run_tests_args("41,42,43");
            if preprocess {
                a.push("--preprocess".into());
            }
            a.extend(text.lines().map(str::to_string));
            a.push("--instance=wght=400,wdth=drop".into());
            a.push("--optimize".into());
            let o = parse(&a).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(o.action, Action::Subset);
            assert_eq!(o.preprocess, preprocess);
        }
        n += 1;
    }
    assert!(n > 40, "only {n} profiles");
}

#[test]
fn run_tests_fixed_arguments() {
    let o = parse(&run_tests_args("41,42,43")).unwrap();
    assert_eq!(o.font_file.as_deref(), Some("font.ttf"));
    assert_eq!(o.output_file.as_deref(), Some("out.ttf"));
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x41, 0x42, 0x43]);
    let mut drop: Vec<u32> = DEFAULT_DROP_TABLES.iter().map(|t| tag(t)).collect();
    drop.push(tag(b"BASE"));
    drop.push(tag(b"DSIG"));
    drop.retain(|&t| t != tag(b"sbix"));
    drop.sort();
    drop.dedup();
    assert_eq!(members(set(&o, SetKind::DropTables)), drop);

    // A subset of `*` and of `no-unicodes`, and the `U+` form with the
    // prefix removed by the script.
    let all = parse(&run_tests_args("*")).unwrap();
    assert_eq!(*set(&all, SetKind::Unicodes), RangeSet::all());
    let none = parse(&run_tests_args("")).unwrap();
    assert!(set(&none, SetKind::Unicodes).is_empty());
    let u = parse(&run_tests_args("0041-0043,1F600")).unwrap();
    assert_eq!(
        members(set(&u, SetKind::Unicodes)),
        vec![0x41, 0x42, 0x43, 0x1F600]
    );

    // run-repack-tests.py.
    let r = ok(&[
        "--font-file=f.otf",
        "--output-file=o.otf",
        "--unicodes=0x41,0x42",
        "--drop-tables-=GPOS,GSUB,GDEF",
    ]);
    assert_eq!(members(set(&r, SetKind::Unicodes)), vec![0x41, 0x42]);
    assert!(!set(&r, SetKind::DropTables).contains(tag(b"GPOS")));
}

#[test]
fn unicodes() {
    let o = ok(&["--unicodes=U+0041,U+0043-0045 u+61"]);
    assert_eq!(
        members(set(&o, SetKind::Unicodes)),
        vec![0x41, 0x43, 0x44, 0x45, 0x61]
    );
    let o = ok(&["-u", "41-5A"]);
    assert_eq!(set(&o, SetKind::Unicodes).len(), 26);
    // `\x41`, `&#x41;` and `<41>` are all 0x41: the bytes around are
    // delimiters.
    let o = ok(&["--unicodes=\\x41;&#x42;<43>{44}"]);
    assert_eq!(
        members(set(&o, SetKind::Unicodes)),
        vec![0x41, 0x42, 0x43, 0x44]
    );
    // A plain option replaces; + adds; - removes.
    let o = ok(&[
        "--unicodes=41",
        "--unicodes=42",
        "--unicodes+=43",
        "--unicodes-=42",
    ]);
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x43]);
    // * then -= gives an inverted set.
    let o = ok(&["--unicodes=*", "--unicodes-=41-43"]);
    let s = set(&o, SetKind::Unicodes);
    assert!(s.is_inverted() && !s.contains(0x42) && s.contains(0x44));
    // -=* empties.
    let o = ok(&["--unicodes=41", "--unicodes-=*"]);
    assert!(set(&o, SetKind::Unicodes).is_empty() && !set(&o, SetKind::Unicodes).is_inverted());
    assert_eq!(err(&["--unicodes=5A-41"]), "Invalid Unicode range 90-65");
    assert_eq!(
        err(&["--unicodes=41,zz"]),
        "Failed parsing Unicode at: 'zz'"
    );
    assert_eq!(err(&["--unicodes=41-"]), "Failed parsing Unicode at: ''");
    // strtoul overflow.
    assert_eq!(
        err(&["--unicodes=FFFFFFFFFFFFFFFFF"]),
        "Failed parsing Unicode at: 'FFFFFFFFFFFFFFFFF'"
    );
    // A range to the invalid value is refused by hb_set_add_range.
    let o = ok(&["--unicodes=FFFFFFF0-FFFFFFFF"]);
    assert!(set(&o, SetKind::Unicodes).is_empty());
}

#[test]
fn gids_and_glyphs() {
    let o = ok(&["--gids=1, 3-5", "-g", "7", "--gids+=9"]);
    assert_eq!(members(set(&o, SetKind::Glyphs)), vec![7, 9]);
    let o = ok(&["--gids=1,3-5"]);
    assert_eq!(members(set(&o, SetKind::Glyphs)), vec![1, 3, 4, 5]);
    assert_eq!(err(&["--gids=-1"]), "Failed parsing glyph-index at: '-1'");
    assert_eq!(err(&["--gids=5-3"]), "Invalid glyph-index range 5-3");
    assert_eq!(err(&["--gids=1--3"]), "Failed parsing glyph-index at: '-3'");
    assert_eq!(err(&["--gids=a"]), "Failed parsing glyph-index at: 'a'");
    let o = ok(&["--gids=*", "--gids-=0-9"]);
    assert!(set(&o, SetKind::Glyphs).is_inverted() && !set(&o, SetKind::Glyphs).contains(9));

    // `--glyphs=*` is every glyph without the font.
    let o = ok(&["--gids=4", "--glyphs=*"]);
    assert_eq!(*set(&o, SetKind::Glyphs), RangeSet::all());
    assert!(o.glyph_names.is_empty());
    let o = ok(&["--glyphs-=*"]);
    assert!(set(&o, SetKind::Glyphs).is_empty());
    // Names are kept; the plain form still clears the set.
    let o = ok(&["--gids=4", "--glyphs=a,b  c", "--glyphs+=d"]);
    assert_eq!(o.glyph_names, vec!["a", "b", "c", "d"]);
    assert!(set(&o, SetKind::Glyphs).is_empty());
}

#[test]
fn text_and_positional() {
    let o = ok(&["--text=AB\u{3042}", "--text-=B"]);
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x41, 0x3042]);
    let o = ok(&["-t", "x", "--text+=y"]);
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x78, 0x79]);
    let o = ok(&["--text=*"]);
    assert_eq!(*set(&o, SetKind::Unicodes), RangeSet::all());
    // [FONT-FILE] [TEXT]; later text replaces.
    let o = ok(&["f.ttf", "AB", "--unicodes+=43", "C"]);
    assert_eq!(o.font_file.as_deref(), Some("f.ttf"));
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x43]);
    // With --font-file first, the positional is text; a later --font-file
    // wins over a positional font.
    let o = ok(&["--font-file=a.ttf", "Z"]);
    assert_eq!(o.font_file.as_deref(), Some("a.ttf"));
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x5A]);
    let o = ok(&["a.ttf", "--font-file=b.ttf"]);
    assert_eq!(o.font_file.as_deref(), Some("b.ttf"));
    // `-` is a positional; after `--` everything is.
    let o = ok(&["-", "--", "-x"]);
    assert_eq!(o.font_file.as_deref(), Some("-"));
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x2D, 0x78]);
}

#[test]
fn names() {
    let o = ok(&["--name-IDs=1,2", "--name-IDs+=7", "--name-IDs-=1"]);
    assert_eq!(members(set(&o, SetKind::NameIds)), vec![2, 7]);
    let o = ok(&["--name-IDs=*"]);
    assert!(set(&o, SetKind::NameIds).is_inverted());
    // No ranges: strtoul reads "-6" as a negated number.
    let o = ok(&["--name-IDs=0-6"]);
    assert_eq!(members(set(&o, SetKind::NameIds)), vec![0, 0xFFFF_FFFA]);
    assert_eq!(err(&["--name-IDs=x"]), "Failed parsing nameID at: 'x'");
    // Base 10: "0x" is 0 followed by "x", which fails.
    assert_eq!(
        err(&["--name-languages=1033 0x"]),
        "Failed parsing name-language code at: 'x'"
    );
    let o = ok(&["--name-languages+=1041"]);
    assert_eq!(members(set(&o, SetKind::NameLanguages)), vec![0x409, 1041]);
}

#[test]
fn tags() {
    let o = ok(&[
        "--layout-features=liga,kern",
        "--layout-features+=dlig",
        "--layout-features-=kern",
    ]);
    assert_eq!(
        members(set(&o, SetKind::LayoutFeatures)),
        vec![tag(b"dlig"), tag(b"liga")]
    );
    let o = ok(&["--layout-scripts=DFLT ss"]);
    assert_eq!(
        members(set(&o, SetKind::LayoutScripts)),
        vec![tag(b"DFLT"), tag(b"ss  ")]
    );
    let o = ok(&["--layout-scripts-=*"]);
    assert!(set(&o, SetKind::LayoutScripts).is_empty());
    assert_eq!(
        err(&["--layout-features=toolong"]),
        "Failed parsing table tag at: 'toolong'"
    );
    assert_eq!(
        err(&["--drop-tables=GSUBX"]),
        "Failed parsing table tag at: 'GSUBX'"
    );
    let o = ok(&["--drop-tables=*", "--drop-tables-=glyf"]);
    assert!(set(&o, SetKind::DropTables).is_inverted());
    let o = ok(&["--drop-tables=DSIG", "--drop-tables+=BASE"]);
    assert_eq!(
        members(set(&o, SetKind::DropTables)),
        vec![tag(b"BASE"), tag(b"DSIG")]
    );
}

#[test]
fn gid_map() {
    let o = ok(&["--gids=2", "--gid-map=37:1, 39:2", "--gid-map=37:3"]);
    let map: BTreeMap<u32, u32> = [(37, 3), (39, 2)].into_iter().collect();
    assert_eq!(*o.input.glyph_mapping(), map);
    assert_eq!(members(set(&o, SetKind::Glyphs)), vec![2, 37, 39]);
    assert_eq!(err(&["--gid-map=37"]), "Failed parsing glyph map at: '37'");
    assert_eq!(
        err(&["--gid-map=37:-1"]),
        "Failed parsing glyph map at: '-1'"
    );
}

#[test]
fn flags() {
    let cases: [(&str, Flags); 18] = [
        ("--no-hinting", Flags::NO_HINTING),
        ("--retain-gids", Flags::RETAIN_GIDS),
        ("--desubroutinize", Flags::DESUBROUTINIZE),
        ("--downgrade-cff2", Flags::DOWNGRADE_CFF2),
        ("--cff-identity-charset", Flags::CFF_IDENTITY_CHARSET),
        ("--name-legacy", Flags::NAME_LEGACY),
        ("--set-overlaps-flag", Flags::SET_OVERLAPS_FLAG),
        ("--notdef-outline", Flags::NOTDEF_OUTLINE),
        ("--no-prune-unicode-ranges", Flags::NO_PRUNE_UNICODE_RANGES),
        ("--no-layout-closure", Flags::NO_LAYOUT_CLOSURE),
        ("--no-bidi-closure", Flags::NO_BIDI_CLOSURE),
        ("--glyph-names", Flags::GLYPH_NAMES),
        ("--passthrough-tables", Flags::PASSTHROUGH_UNRECOGNIZED),
        ("--iftb-requirements", Flags::IFTB_REQUIREMENTS),
        ("--retain-num-glyphs", Flags::RETAIN_NUM_GLYPHS),
        ("--optimize", Flags::OPTIMIZE_IUP_DELTAS),
        // GLib passes `=value` to a no-argument callback, which ignores it.
        ("--no-hinting=0", Flags::NO_HINTING),
        ("--optimize=", Flags::OPTIMIZE_IUP_DELTAS),
    ];
    for (a, f) in cases {
        assert_eq!(ok(&[a]).input.flags(), f, "{a}");
    }
    let all: Vec<&str> = cases.iter().map(|c| c.0).collect();
    assert_eq!(ok(&all).input.flags(), Flags::ALL);
    assert!(ok(&["--preprocess"]).preprocess);
    assert!(ok(&["--preprocess-face"]).preprocess);

    // --keep-everything, then options override it in order.
    let o = ok(&["--keep-everything", "--unicodes=41", "--no-hinting"]);
    let k = Input::new().keep_everything();
    assert_eq!(o.input.flags(), k.flags() | Flags::NO_HINTING);
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x41]);
    assert_eq!(*set(&o, SetKind::Glyphs), RangeSet::all());
    let o = ok(&["--drop-tables+=GSUB", "--keep-everything"]);
    assert!(set(&o, SetKind::DropTables).is_empty());
}

#[test]
fn variations() {
    let o = ok(&[
        "--variations=wght=400,wdth=drop",
        "--instance=*=drop",
        "--instance",
        "opsz=8:12 slnt=:-5:",
    ]);
    let r = |min, default, max| AxisValue::Range { min, default, max };
    assert_eq!(
        o.instance,
        vec![
            AxisSpec {
                tag: Some(Tag::new(b"wght")),
                value: r(Some(400.0), Some(400.0), Some(400.0)),
            },
            AxisSpec {
                tag: Some(Tag::new(b"wdth")),
                value: AxisValue::Drop,
            },
            AxisSpec {
                tag: None,
                value: AxisValue::Drop,
            },
            AxisSpec {
                tag: Some(Tag::new(b"opsz")),
                value: r(Some(8.0), None, Some(12.0)),
            },
            AxisSpec {
                tag: Some(Tag::new(b"slnt")),
                value: r(None, Some(-5.0), None),
            },
        ]
    );
    assert_eq!(
        err(&["--variations=width=1"]),
        "Failed parsing axis tag at: 'width'"
    );
    assert_eq!(
        err(&["--variations=wght"]),
        "Value not specified for axis: wght"
    );
    assert_eq!(
        err(&["--variations=wg"]),
        "Value not specified for axis: wg  "
    );
    assert_eq!(
        err(&["--variations=*=1"]),
        "Failed parsing axis position at: '1'"
    );
    assert_eq!(
        err(&["--variations=wght=abc"]),
        "Error parsing option --variations"
    );
    assert_eq!(
        err(&["--instance=wght=abc"]),
        "Error parsing option --instance"
    );
    // strtok leaves the space after a comma on the next tag.
    assert_eq!(
        err(&["--variations=wght=1, wdth=2"]),
        "Failed parsing axis tag at: ' wdth'"
    );
}

#[test]
fn integers_and_strings() {
    let o = ok(&[
        "-n",
        "3",
        "-y",
        "2",
        "-o",
        "x.ttf",
        "--face-loader=ot",
        "-O",
        "ttf",
    ]);
    assert_eq!((o.num_iterations, o.face_index), (3, 2));
    assert_eq!(o.output_file.as_deref(), Some("x.ttf"));
    assert_eq!(o.output_format.as_deref(), Some("ttf"));
    assert_eq!(ok(&["--num-iterations=0x10"]).num_iterations, 16);
    assert_eq!(ok(&["--num-iterations=010"]).num_iterations, 8);
    assert_eq!(ok(&["-n", "-1"]).num_iterations, u32::MAX);
    assert_eq!(
        err(&["-n", "x"]),
        "Cannot parse integer value \u{201c}x\u{201d} for -n"
    );
    assert_eq!(
        err(&["--num-iterations=1 "]),
        "Cannot parse integer value \u{201c}1 \u{201d} for --num-iterations"
    );
    assert_eq!(
        err(&["--num-iterations=4294967296"]),
        "Integer value \u{201c}4294967296\u{201d} for --num-iterations out of range"
    );
    assert_eq!(ok(&[]).num_iterations, 1);
}

#[test]
fn glib_spellings() {
    // `--name value`, `--name=value`, the short forms, and the
    // `--<group>-<option>` aliases.
    assert_eq!(ok(&["--unicodes", "41"]), ok(&["--unicodes=41"]));
    assert_eq!(ok(&["-u", "41"]), ok(&["--unicodes=41"]));
    assert_eq!(ok(&["--unicodes", "-41"]), ok(&["--unicodes=-41"]));
    assert_eq!(ok(&["--face-font-file=f"]).font_file.as_deref(), Some("f"));
    assert_eq!(
        ok(&["--output-output-file=o"]).output_file.as_deref(),
        Some("o")
    );
    assert_eq!(
        err(&["--subset-flags-no-hinting"]),
        "Unknown option --subset-flags-no-hinting"
    );
    assert_eq!(err(&["--unicodes"]), "Missing argument for --unicodes");
    assert_eq!(err(&["-o"]), "Missing argument for -o");
    assert_eq!(err(&["--foo"]), "Unknown option --foo");
    // The value of -u is the next argument, then 4 is unknown.
    assert_eq!(err(&["-u41"]), "Missing argument for -u");
    assert_eq!(err(&["-u41", "x"]), "Unknown option -u41");
    assert_eq!(err(&["-uo", "41", "x"]), "Error parsing option -o");
    assert_eq!(err(&["--batch", "--no-hinting"]), "Unknown option --batch");
    assert_eq!(ok(&["--batch"]).action, Action::Batch);
    for h in ["--help", "--help-all", "--help-subset-flags", "-h", "-?"] {
        assert_eq!(ok(&[h]).action, Action::Help, "{h}");
    }
    // Options are applied in order, so an error after --help is not seen.
    assert_eq!(ok(&["--help", "--foo"]).action, Action::Help);
    assert_eq!(err(&["--helpx"]), "Unknown option --helpx");
    assert_eq!(ok(&["--version", "--foo"]).action, Action::Version);
    assert_eq!(ok(&["--list-face-loaders"]).action, Action::ListFaceLoaders);
}

#[test]
fn files() {
    let read = |path: &str| -> Result<Vec<u8>, String> {
        match path {
            "u" => Ok(b"41 # comment\n42-43\r\n\n".to_vec()),
            "t" => Ok(b"a#\nb".to_vec()),
            "g" => Ok(b"1\nx\n3\n".to_vec()),
            "n" => Ok(b"a b\n# c\n*\n".to_vec()),
            "s" => Ok(b"*".to_vec()),
            _ => Err("No such file or directory".into()),
        }
    };
    let p = |a: &[&str]| parse_with(&args(a), &read);
    // Each line is the + form.
    let o = p(&["--unicodes=40", "--unicodes-file=u"]).unwrap();
    assert_eq!(
        members(set(&o, SetKind::Unicodes)),
        vec![0x40, 0x41, 0x42, 0x43]
    );
    // No comments in text files.
    let o = p(&["--text-file=t"]).unwrap();
    assert_eq!(members(set(&o, SetKind::Unicodes)), vec![0x23, 0x61, 0x62]);
    // A bad line stops the file without failing (HarfBuzz returns true).
    let o = p(&["--gids-file=g"]).unwrap();
    assert_eq!(members(set(&o, SetKind::Glyphs)), vec![1]);
    let o = p(&["--glyphs-file=n"]).unwrap();
    assert_eq!(o.glyph_names, vec!["a", "b"]);
    assert_eq!(*set(&o, SetKind::Glyphs), RangeSet::all());
    let o = p(&["--unicodes-file=s"]).unwrap();
    assert_eq!(*set(&o, SetKind::Unicodes), RangeSet::all());
    assert_eq!(
        p(&["--gids-file=none"]).unwrap_err(),
        "Failed opening file `none': No such file or directory"
    );
}

#[test]
fn resolve_stubs() {
    let data = build::sample_font();
    let font = sofel_font::Font::new(&data, 0).unwrap();
    let o = ok(&["--glyphs=*", "--unicodes=41"]);
    assert_eq!(resolve(&o, &font).unwrap(), o.input);
    assert_eq!(
        resolve(&ok(&["--glyphs=a"]), &font).unwrap_err(),
        "glyph names are not supported yet (ADR 0022 stage 12)"
    );
    // A font without fvar has no axes to pin.
    assert_eq!(
        resolve(&ok(&["--variations=*=drop"]), &font).unwrap_err(),
        "Failed pinning all axes to default."
    );
    assert_eq!(
        resolve(&ok(&["--variations=wght=drop"]), &font).unwrap_err(),
        "Cannot pin axis: 'wght', not present in fvar"
    );
}

#[test]
fn resolve_variations() {
    let Some(hb) = harfbuzz() else { return };
    let data = std::fs::read(hb.join("test/subset/data/fonts/Roboto-Variable.ABC.ttf")).unwrap();
    let font = sofel_font::Font::new(&data, 0).unwrap();
    let point = |v: f32| sofel_subset::AxisRange {
        min: v,
        default: v,
        max: v,
    };
    let axes = |a: &[&str]| resolve(&ok(a), &font).map(|i| i.axes().clone());
    let both = BTreeMap::from([
        (Tag::new(b"wght"), point(400.0)),
        (Tag::new(b"wdth"), point(100.0)),
    ]);
    // `drop`, `*=drop` and values equal to the default all pin to the
    // default (pin_all_at_default.tests).
    assert_eq!(axes(&["--variations=wght=drop,wdth=100"]).unwrap(), both);
    assert_eq!(axes(&["--instance=wght=400,wdth=100.0"]).unwrap(), both);
    assert_eq!(axes(&["--variations=*=drop"]).unwrap(), both);
    assert_eq!(
        axes(&["--variations=wght=400:400:400"]).unwrap(),
        BTreeMap::from([(Tag::new(b"wght"), point(400.0))])
    );
    // Other locations and ranges wait for the instancing ADR.
    for v in ["wght=300", "wght=300:600", "wdth=drop,wght=500"] {
        let e = axes(&[&format!("--variations={v}")]).unwrap_err();
        assert!(e.contains("deferred by ADR 0022"), "{v}: {e}");
    }
    assert_eq!(
        axes(&["--variations=opsz=drop"]).unwrap_err(),
        "Cannot pin axis: 'opsz', not present in fvar"
    );
    assert_eq!(
        axes(&["--variations=wght=500:400"]).unwrap_err(),
        "Cannot pin axis: 'wght', not present in fvar"
    );
}

#[test]
fn to_args_round_trip() {
    let inputs = [
        vec![],
        vec![
            "--unicodes=41-5A,61",
            "--gids=1,3-5",
            "--no-hinting",
            "--retain-gids",
            "--layout-features+=dlig",
            "--layout-scripts=latn,grek",
            "--drop-tables-=DSIG",
            "--name-IDs=1,3",
            "--name-languages=*",
            "--gid-map=3:1,5:2",
        ],
        vec!["--keep-everything"],
        vec![
            "--unicodes=*",
            "--unicodes-=41-43",
            "--glyphs=*",
            "--layout-features-=*",
            "--drop-tables=*",
            "--drop-tables-=glyf,head",
            "--iftb-requirements",
            "--retain-num-glyphs",
            "--optimize",
            "--downgrade-cff2",
        ],
        vec!["--name-languages-=*", "--gids-=*", "--layout-scripts=DFLT"],
    ];
    for a in inputs {
        let input = ok(&a).input;
        let back = input.to_args();
        let again = parse(&back).unwrap_or_else(|e| panic!("{back:?}: {e}"));
        assert_eq!(again.input, input, "{a:?} -> {back:?}");
    }
}

#[test]
fn to_args_format() {
    let o = ok(&[
        "--unicodes=41-5A,61",
        "--gids=1,3-5",
        "--retain-gids",
        "--no-hinting",
        "--layout-scripts=latn,grek",
        "--drop-tables=*",
        "--drop-tables-=DSIG",
        "--name-IDs-=*",
        "--gid-map=5:2,3:1",
    ]);
    assert_eq!(
        o.input.to_args(),
        args(&[
            "--no-hinting",
            "--retain-gids",
            "--unicodes=41-5A,61",
            "--gids=1,3-5",
            "--name-IDs-=*",
            "--layout-scripts=grek,latn",
            "--drop-tables=*",
            "--drop-tables-=DSIG",
            "--gid-map=3:1,5:2",
        ])
    );
    // HarfBuzz writes no option for CFF_IDENTITY_CHARSET, and the copied
    // tables as --passthrough-tables=.
    assert!(ok(&["--cff-identity-charset"]).input.to_args().is_empty());
    let pass = Input::new().map_set(SetKind::NoSubsetTables, |s| s.add(tag(b"BASE")));
    assert_eq!(
        pass.to_args(),
        args(&["--passthrough-tables=BASE,DSIG,VDMX,fpgm,gasp,prep"])
    );
    // Tag bytes other than letters, digits, - and _ are written as _.
    let o = ok(&["--layout-features=a.b,ss"]);
    assert_eq!(o.input.to_args(), args(&["--layout-features=a_b,ss"]));
    // Name ids 0 to 6 are written as a range, which hb-subset's
    // --name-IDs reads as 0 and -6 (a quirk kept from HarfBuzz).
    let o = ok(&["--name-IDs+=9"]);
    assert_eq!(o.input.to_args(), args(&["--name-IDs=0-6,9"]));
    assert_eq!(
        members(set(&ok(&["--name-IDs=0-6,9"]), SetKind::NameIds)),
        vec![0, 9, 0xFFFF_FFFA]
    );
}

fn font_file(name: &str, data: &[u8]) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!("sofel-subset-{name}-{}.ttf", std::process::id()));
    std::fs::write(&p, data).unwrap();
    p
}

fn run_with(a: &[&str], stdin: &str) -> (i32, String, String) {
    let a: Vec<OsString> = a.iter().map(OsString::from).collect();
    let mut input = stdin.as_bytes();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&a, &mut input, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into(),
        String::from_utf8_lossy(&err).into(),
    )
}

#[test]
fn command_errors() {
    let (code, _, err) = run_with(&["--unicodes=41"], "");
    assert_eq!(code, 1);
    assert!(
        err.starts_with("sofel-subset: No font file set\nTry `sofel-subset --help'"),
        "{err}"
    );
    let (code, _, err) = run_with(&["/nonexistent/font.ttf", "-o", "x"], "");
    assert_eq!(code, 2);
    assert!(
        err.starts_with("sofel-subset: /nonexistent/font.ttf: Failed loading font face"),
        "{err}"
    );
    let font = font_file("noout", &build::sample_font());
    let (code, _, err) = run_with(&[font.to_str().unwrap()], "");
    assert_eq!(code, 1);
    assert!(
        err.starts_with("sofel-subset: No output file was specified"),
        "{err}"
    );
    let (code, out, _) = run_with(&["--help"], "");
    assert_eq!(code, 0);
    assert!(out.starts_with("usage: sofel-subset"));
    let (code, _, err) = run_with(&["--glyphs=a", font.to_str().unwrap(), "-o", "-"], "");
    assert_eq!(code, 1);
    assert!(err.contains("glyph names are not supported yet"), "{err}");
    // -n 0 subsets nothing: a failure.
    let (code, out, _) = run_with(&["-n", "0", font.to_str().unwrap(), "-o", "-"], "");
    assert_eq!(code, 1);
    assert_eq!(
        out,
        "error: Operation failed. Probably a bug. File github issue.\n"
    );
    std::fs::remove_file(font).ok();
}

#[test]
fn batch() {
    let font = font_file("batch", &build::sample_font());
    let f = font.to_str().unwrap();
    // Two lines: one that cannot subset (-n 0), one with runs of `;`.
    let lines = format!("--font-file={f};;-o;-;-n;0\n--font-file={f};-n;0;-o;-\n");
    let (code, out, _) = run_with(&["--batch"], &lines);
    assert_eq!(out, "failure\nfailure\n");
    assert_eq!(code, 1);
    // An option error ends the batch, as it ends hb-subset's process.
    let lines = format!("--foo\n--font-file={f};-o;-;-n;0\n");
    let (code, out, err) = run_with(&["--batch"], &lines);
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(
        err.starts_with("sofel-subset: Unknown option --foo"),
        "{err}"
    );
    std::fs::remove_file(font).ok();
}
