//! sofel ADR 0083 and ADR 0084: the derived sample fonts of the gallery.
//!
//! The tests read the fixed fonts under `target/testdata/extra-fonts`
//! (`scripts/fetch-fonts.sh` with both locks). The `#[ignore]`d ones need
//! the generated gallery and the renderer binaries, as in
//! `color_gallery.rs`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use font_gallery::derive::{self, CFF_RANDOM_PROGRAM};
use font_gallery::inputs;
use sofel_font::Font;
use sofel_font::read::cff::dict::TopDict;
use sofel_font::read::cff::{Cff1, Index, Version};
use sofel_font::read::gsub::{self, SubstSubtable};
use sofel_font::read::layout::LayoutTable;
use sofel_font::read::sfnt::Sfnt;
use sofel_font::read::{GlyphId, Tag};
use sofel_geom::{Command, FnPen};
use sofel_outline::{Outlines, Scale, Scratch};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn font_file(name: &str) -> Vec<u8> {
    let path = repo().join("target/testdata/extra-fonts/fonts").join(name);
    fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "missing {}; run scripts/fetch-fonts.sh (with gallery-fonts.lock for Caveat)",
            path.display()
        )
    })
}

fn stix() -> Vec<u8> {
    font_file("stix-math.otf")
}

fn caveat() -> Vec<u8> {
    font_file("caveat.ttf")
}

fn stix_sha() -> String {
    derive::lock_sha256(&repo().join("fonts.lock"), "stix-math").unwrap()
}

fn caveat_sha() -> String {
    derive::lock_sha256(&repo().join("gallery-fonts.lock"), "caveat").unwrap()
}

fn tables(font: &[u8]) -> BTreeMap<Tag, &[u8]> {
    let sfnt = Sfnt::new(font).unwrap();
    sfnt.records()
        .map(|r| (r.tag, r.data(font).unwrap()))
        .collect()
}

/// Rebuilds `font` with `tag` replaced by `data`.
fn with_table(font: &[u8], tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut all = tables(font);
    all.insert(Tag::new(tag), data);
    let list: Vec<(Tag, &[u8])> = all.into_iter().collect();
    sofel_write::build_sfnt(&list).unwrap()
}

/// Compares every table but `skip`, and `head` but `checkSumAdjustment`.
fn assert_other_tables_equal(base: &[u8], derived: &[u8], skip: &[&[u8; 4]]) {
    let (a, b) = (tables(base), tables(derived));
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (tag, data) in &a {
        if skip.iter().any(|s| Tag::new(s) == *tag) {
            continue;
        }
        if *tag == Tag::new(b"head") {
            assert_eq!(data.len(), b[tag].len());
            assert_eq!(data[..8], b[tag][..8], "head");
            assert_eq!(data[12..], b[tag][12..], "head");
        } else {
            assert_eq!(*data, b[tag], "{:?}", std::str::from_utf8(&tag.0));
        }
    }
}

fn r_glyph(font: &[u8]) -> u32 {
    Font::new(font, 0)
        .unwrap()
        .nominal_glyph(u32::from('R'))
        .unwrap()
        .0
}

/// The byte range of `glyph`'s charstring in a CFF table.
fn charstring_range(cff: &[u8], glyph: u32) -> Range<usize> {
    let c = Cff1::new(cff).unwrap();
    let top = TopDict::parse(c.top_dicts().get(0).unwrap(), Version::Cff).unwrap();
    let at = usize::try_from(top.char_strings).unwrap();
    let index = Index::new(&cff[at..], Version::Cff).unwrap();
    let base = at + index.data_offset() - 1;
    base + index.offset(glyph).unwrap() as usize..base + index.offset(glyph + 1).unwrap() as usize
}

/// STIX Two Math with the boundary between `R` and the next glyph moved
/// so that `R`'s charstring is `len` bytes long.
fn stix_with_r_len(len: usize) -> Vec<u8> {
    let base = stix();
    let mut cff = tables(&base)[&Tag::new(b"CFF ")].to_vec();
    let glyph = r_glyph(&base);
    let c = Cff1::new(&cff).unwrap();
    let top = TopDict::parse(c.top_dicts().get(0).unwrap(), Version::Cff).unwrap();
    let at = usize::try_from(top.char_strings).unwrap();
    let index = Index::new(&cff[at..], Version::Cff).unwrap();
    let size = usize::from(index.off_size());
    let value = index.offset(glyph).unwrap() as usize + len;
    let field = at + 3 + (glyph as usize + 1) * size;
    cff[field..field + size].copy_from_slice(&value.to_be_bytes()[8 - size..]);
    with_table(&base, b"CFF ", &cff)
}

/// Name records `(platform, encoding, language, id, text)`.
fn names(font: &[u8]) -> Vec<(u16, u16, u16, u16, String)> {
    let t = tables(font)[&Tag::new(b"name")];
    let word = |at: usize| u16::from_be_bytes([t[at], t[at + 1]]);
    let storage = usize::from(word(4));
    (0..usize::from(word(2)))
        .map(|i| {
            let r = 6 + 12 * i;
            let (len, off) = (usize::from(word(r + 8)), usize::from(word(r + 10)));
            let bytes = &t[storage + off..storage + off + len];
            let text = if word(r) == 1 {
                bytes.iter().map(|&b| char::from(b)).collect()
            } else {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_be_bytes(*c))
                    .collect();
                String::from_utf16(&units).unwrap()
            };
            (word(r), word(r + 2), word(r + 4), word(r + 6), text)
        })
        .collect()
}

fn assert_names(base: &[u8], derived: &[u8], family: &str, postscript: &str) {
    let (a, b) = (names(base), names(derived));
    assert_eq!(a.len(), b.len());
    for (old, new) in a.iter().zip(&b) {
        assert_eq!((old.0, old.1, old.2, old.3), (new.0, new.1, new.2, new.3));
        match new.3 {
            0 | 13 | 14 => assert_eq!(old.4, new.4, "name {}", new.3),
            1 | 4 | 16 => assert_eq!(new.4, family, "name {}", new.3),
            3 | 6 => assert_eq!(new.4, postscript, "name {}", new.3),
            17 => assert_eq!(new.4, "Regular", "name 17"),
            _ => assert_eq!(old.4, new.4, "name {}", new.3),
        }
    }
    assert!(b.iter().any(|n| n.3 == 1));
}

// ADR 0083

#[test]
fn adr0083_i1_only_r_charstring_name_and_checksums_differ() {
    let base = stix();
    let derived = derive::cff_random_sample(&base, &stix_sha()).unwrap();
    assert_other_tables_equal(&base, &derived, &[b"CFF ", b"name"]);
    let (a, b) = (
        tables(&base)[&Tag::new(b"CFF ")],
        tables(&derived)[&Tag::new(b"CFF ")],
    );
    assert_eq!(a.len(), b.len());
    let range = charstring_range(a, r_glyph(&base));
    assert_eq!(range, charstring_range(b, r_glyph(&derived)));
    assert_eq!(a[..range.start], b[..range.start]);
    assert_eq!(a[range.end..], b[range.end..]);
}

#[test]
fn adr0083_i2_r_charstring_is_the_program_padded_with_endchar() {
    let base = stix();
    let derived = derive::cff_random_sample(&base, &stix_sha()).unwrap();
    assert_eq!(r_glyph(&derived), 20);
    let cff = tables(&derived)[&Tag::new(b"CFF ")];
    let r = &cff[charstring_range(cff, 20)];
    assert_eq!(r.len(), 139);
    assert_eq!(
        CFF_RANDOM_PROGRAM,
        [
            12, 23, 28, 3, 0xE8, 12, 24, 12, 23, 28, 3, 0xE8, 12, 24, 21, 28, 1, 0xF4, 139, 5, 139,
            28, 1, 0xF4, 5, 14
        ]
    );
    assert_eq!(r[..26], CFF_RANDOM_PROGRAM);
    assert!(r[26..].iter().all(|&b| b == 14));
}

#[test]
fn adr0083_i2_a_26_byte_charstring_is_replaced_without_padding() {
    let base = stix_with_r_len(26);
    let derived = derive::cff_random_sample(&base, &derive::sha256_hex(&base)).unwrap();
    let cff = tables(&derived)[&Tag::new(b"CFF ")];
    assert_eq!(
        cff[charstring_range(cff, r_glyph(&derived))],
        CFF_RANDOM_PROGRAM
    );
}

fn commands(outlines: &Outlines<'_, '_, '_>, glyph: u32) -> Vec<Command> {
    let mut scratch = Scratch::default();
    let mut out = Vec::new();
    outlines
        .draw(
            GlyphId(glyph),
            Scale::Unscaled,
            &mut scratch,
            &mut FnPen(|c| out.push(c)),
        )
        .unwrap();
    out
}

#[test]
fn adr0083_i3_r_is_drawn_the_same_every_time() {
    let derived = derive::cff_random_sample(&stix(), &stix_sha()).unwrap();
    let font = Font::new(&derived, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    let r = r_glyph(&derived);
    let s = font.nominal_glyph(u32::from('S')).unwrap().0;
    let first = commands(&outlines, r);
    assert!(!commands(&outlines, s).is_empty());
    let second = commands(&outlines, r);
    assert!(!first.is_empty());
    assert_eq!(first, second);
}

/// FreeType's `cff_random` (`src/psaux/psobjs.c`).
fn cff_random(r: u32) -> u32 {
    let r = r ^ (r << 13);
    let r = r ^ (r >> 17);
    r ^ (r << 5)
}

#[test]
fn adr0083_i4_the_start_point_follows_the_seed() {
    // `cf2_escRANDOM` (`src/psaux/psintrp.c`) pushes the 16.16 value
    // `(random & 0xFFFF) + 1` of the current state, then advances the
    // state with `cff_random`; the state starts from the seed at every
    // glyph (initialRandomSeed 0 is taken as 987654321).
    let first = 987_654_321_u32;
    let second = cff_random(first);
    let value = |r: u32| i64::from((r & 0xFFFF) + 1) * 1000;
    // Unhinted, cf2 draws at a scale of 1/64 (0x400 in 16.16, rounded as
    // FT_MulFix) and `cff_builder_add_point` shifts the 16.16 result
    // right by 10: integer font units with no scaling.
    let units = |v: i64| ((v * 0x400 + 0x8000) >> 16) >> 10;
    let expected = Command::MoveTo(sofel_geom::Point {
        x: i32::try_from(units(value(first))).unwrap(),
        y: i32::try_from(units(value(second))).unwrap(),
    });
    let derived = derive::cff_random_sample(&stix(), &stix_sha()).unwrap();
    let font = Font::new(&derived, 0).unwrap();
    let outlines = Outlines::new(font.default_instance());
    assert_eq!(commands(&outlines, r_glyph(&derived))[0], expected);
}

#[test]
fn adr0083_i5_names_drop_tm_math_and_stix() {
    let base = stix();
    let derived = derive::cff_random_sample(&base, &stix_sha()).unwrap();
    assert_names(
        &base,
        &derived,
        "Sofel Random Sample",
        "SofelRandomSample-Regular",
    );
    for n in names(&derived) {
        if [1, 3, 4, 6, 16, 17].contains(&n.3) {
            assert!(!n.4.contains("TM Math") && !n.4.contains("STIX"), "{n:?}");
        }
    }
}

#[test]
fn adr0083_i6_rejects_a_font_with_another_sha256() {
    let mut base = stix();
    let last = base.len() - 1;
    base[last] ^= 1;
    let error = derive::cff_random_sample(&base, &stix_sha()).unwrap_err();
    assert!(error.contains("SHA-256"), "{error}");
}

#[test]
fn adr0083_i6_rejects_a_charstring_shorter_than_the_program() {
    let base = stix_with_r_len(25);
    let error = derive::cff_random_sample(&base, &derive::sha256_hex(&base)).unwrap_err();
    assert!(error.contains("26"), "{error}");
}

#[test]
fn adr0083_i6_failure_writes_no_font() {
    let root = std::env::temp_dir().join(format!("sofel-derived-fail-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let license = root.join("license.txt");
    fs::write(&license, "license").unwrap();
    let out = root.join("out");
    let base = stix_with_r_len(25);
    let font = derive::cff_random_sample(&base, &derive::sha256_hex(&base));
    assert!(derive::write_derived(&out, "font.otf", font, (&license, "license.txt")).is_err());
    assert!(!out.join("font.otf").exists());
    assert!(!out.join("license.txt").exists());
    let _ = fs::remove_dir_all(&root);
}

// ADR 0084

fn letters() -> impl Iterator<Item = char> {
    ('a'..='z').chain('A'..='Z')
}

/// Glyph ids by name, for `[x, x.ss01, x.ss02]` of each letter.
fn alternates(font: &[u8]) -> Vec<Vec<u32>> {
    let f = Font::new(font, 0).unwrap();
    let mut buf = vec![0; f.glyph_names_len()];
    let names = f.glyph_names(&mut buf).unwrap();
    letters()
        .map(|c| {
            let g = f.nominal_glyph(u32::from(c)).unwrap();
            let name = names.glyph_name(g).unwrap().to_vec();
            let mut set = vec![g.0];
            for suffix in [&b".ss01"[..], b".ss02"] {
                let full = [name.as_slice(), suffix].concat();
                set.push(names.glyph_from_name(&full).unwrap().0);
            }
            set
        })
        .collect()
}

#[test]
fn adr0084_i1_only_gsub_name_and_checksums_differ() {
    let base = caveat();
    let derived = derive::rand_sample(&base, &caveat_sha()).unwrap();
    assert_other_tables_equal(&base, &derived, &[b"GSUB", b"name"]);
}

#[test]
fn adr0084_i2_existing_entries_are_kept_and_rand_is_added() {
    let base = caveat();
    let derived = derive::rand_sample(&base, &caveat_sha()).unwrap();
    let (a, b) = (
        LayoutTable::new(tables(&base)[&Tag::new(b"GSUB")]).unwrap(),
        LayoutTable::new(tables(&derived)[&Tag::new(b"GSUB")]).unwrap(),
    );
    assert_eq!(b.version(), 0x0001_0000);
    let (sa, sb) = (
        a.script_list().unwrap().unwrap(),
        b.script_list().unwrap().unwrap(),
    );
    assert_eq!(sa.len(), sb.len());
    let mut changed = 0;
    for i in 0..sa.len() {
        assert_eq!(sa.tag(i), sb.tag(i));
        let (x, y) = (
            sa.script(i).unwrap().unwrap(),
            sb.script(i).unwrap().unwrap(),
        );
        assert_eq!(x.num_lang_sys(), y.num_lang_sys());
        for j in 0..x.num_lang_sys() {
            assert_eq!(x.lang_sys_tag(j), y.lang_sys_tag(j));
            assert_eq!(
                x.lang_sys(j).unwrap().unwrap().data(),
                y.lang_sys(j).unwrap().unwrap().data()
            );
        }
        let tag = sa.tag(i).unwrap();
        if tag == Tag::new(b"DFLT") || tag == Tag::new(b"latn") {
            changed += 1;
            let (old, new) = (
                x.default_lang_sys().unwrap().unwrap(),
                y.default_lang_sys().unwrap().unwrap(),
            );
            assert_eq!(old.required_feature_index(), new.required_feature_index());
            let list = |l: &sofel_font::read::layout::LangSys<'_>| {
                (0..l.num_features())
                    .map(|k| l.feature_index(k).unwrap())
                    .collect::<Vec<_>>()
            };
            let mut expected = list(&old);
            expected.push(13);
            assert_eq!(list(&new), expected);
        } else {
            assert_eq!(x.data(), y.data());
        }
    }
    assert_eq!(changed, 2);
    let (fa, fb) = (
        a.feature_list().unwrap().unwrap(),
        b.feature_list().unwrap().unwrap(),
    );
    assert_eq!((fa.len(), fb.len()), (13, 14));
    for i in 0..13 {
        assert_eq!(fa.tag(i), fb.tag(i));
        assert_eq!(
            fa.feature(i).unwrap().unwrap().data(),
            fb.feature(i).unwrap().unwrap().data()
        );
    }
    assert_eq!(fb.tag(13), Some(Tag::new(b"rand")));
    let rand = fb.feature(13).unwrap().unwrap();
    assert_eq!(rand.feature_params_offset(), 0);
    assert_eq!(
        (0..rand.num_lookups())
            .map(|k| rand.lookup_index(k).unwrap())
            .collect::<Vec<_>>(),
        [30]
    );
    let (la, lb) = (
        a.lookup_list().unwrap().unwrap(),
        b.lookup_list().unwrap().unwrap(),
    );
    assert_eq!((la.len(), lb.len()), (30, 31));
    for i in 0..30 {
        assert_eq!(
            la.lookup(i).unwrap().unwrap().data(),
            lb.lookup(i).unwrap().unwrap().data()
        );
    }
    let lookup = lb.lookup(30).unwrap().unwrap();
    assert_eq!(
        (lookup.lookup_type(), lookup.flag(), lookup.num_subtables()),
        (3, 0, 1)
    );
    let SubstSubtable::Alternate {
        coverage,
        alternate_sets,
    } = gsub::subtable(&lookup, 0).unwrap()
    else {
        panic!("lookup 30 is not an alternate substitution");
    };
    let sets = alternates(&derived);
    assert_eq!(alternate_sets.len(), 52);
    for set in &sets {
        let k = coverage.get(GlyphId(set[0])).unwrap() as usize;
        let got: Vec<u32> = alternate_sets.get(k).unwrap().unwrap().iter().collect();
        assert_eq!(got, *set);
    }
}

fn shape(font: &Path, options: &[&str], text: &str) -> String {
    let mut args: Vec<OsString> = vec![font.into(), "--no-glyph-names".into()];
    args.extend(options.iter().map(OsString::from));
    args.push(text.into());
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = sofel_cli::shape::run_with_input(&args, &mut &b""[..], &mut out, &mut err);
    assert_eq!(status, 0, "{}", String::from_utf8_lossy(&err));
    String::from_utf8(out).unwrap()
}

#[test]
fn adr0084_i3_without_rand_the_derived_font_shapes_as_the_base() {
    let root = std::env::temp_dir().join(format!("sofel-derived-rand-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let derived_path = root.join("derived.ttf");
    let base_path = repo().join("target/testdata/extra-fonts/fonts/caveat.ttf");
    fs::write(
        &derived_path,
        derive::rand_sample(&caveat(), &caveat_sha()).unwrap(),
    )
    .unwrap();
    for text in ["aaaa eeee oooo tttt", ""] {
        assert_eq!(
            shape(&derived_path, &["--features=-rand"], text),
            shape(&base_path, &[], text),
            "{text:?}"
        );
    }
    // `rand` is not in the `CAT ` language system.
    for text in ["aaaa eeee oooo tttt", ""] {
        assert_eq!(
            shape(&derived_path, &["--language=ca"], text),
            shape(&base_path, &["--language=ca"], text),
            "{text:?}"
        );
    }
    // With `rand`, some letter takes an alternate.
    assert_ne!(
        shape(&derived_path, &[], "aaaa eeee oooo tttt"),
        shape(&base_path, &[], "aaaa eeee oooo tttt")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn adr0084_i5_names_are_sofel_rand_sample() {
    let base = caveat();
    let derived = derive::rand_sample(&base, &caveat_sha()).unwrap();
    assert_names(
        &base,
        &derived,
        "Sofel Rand Sample",
        "SofelRandSample-Regular",
    );
}

#[test]
fn adr0084_i6_rejects_a_font_with_another_sha256() {
    let mut base = caveat();
    base[100] ^= 1;
    let error = derive::rand_sample(&base, &caveat_sha()).unwrap_err();
    assert!(error.contains("SHA-256"), "{error}");
}

#[test]
fn adr0084_i6_rejects_a_missing_alternate() {
    let base = caveat();
    let post = tables(&base)[&Tag::new(b"post")].to_vec();
    let at = post
        .windows(7)
        .position(|w| w == b"\x06z.ss02")
        .expect("z.ss02 in post");
    let mut post = post;
    post[at + 6] = b'x';
    let font = with_table(&base, b"post", &post);
    let error = derive::rand_sample(&font, &derive::sha256_hex(&font)).unwrap_err();
    assert!(error.contains("z.ss02"), "{error}");
}

#[test]
fn adr0084_i6_rejects_an_offset_above_65535() {
    // Moves the lists of the original GSUB back by `pad` bytes, so that
    // its own offsets still fit but the new lists' do not.
    let base = caveat();
    let gsub = tables(&base)[&Tag::new(b"GSUB")];
    let word = |at: usize| u16::from_be_bytes([gsub[at], gsub[at + 1]]);
    let last = word(4).max(word(6)).max(word(8));
    let pad = 65535 - last;
    let mut moved = gsub[..4].to_vec();
    for at in [4, 6, 8] {
        moved.extend_from_slice(&(word(at) + pad).to_be_bytes());
    }
    moved.extend(std::iter::repeat_n(0, usize::from(pad)));
    moved.extend_from_slice(&gsub[10..]);
    let font = with_table(&base, b"GSUB", &moved);
    let error = derive::rand_sample(&font, &derive::sha256_hex(&font)).unwrap_err();
    assert!(error.contains("65535"), "{error}");
}

// Both: the gallery cards (A1).

#[test]
fn adr0083_a1_adr0084_a1_derived_cards_are_registered() {
    let repo = repo();
    let samples = inputs::derived_cases(&repo);
    let ids: Vec<_> = samples
        .iter()
        .map(|s| {
            (
                s.id.as_str(),
                s.face,
                s.solo.value(),
                s.text,
                s.ppem,
                s.color,
            )
        })
        .collect();
    assert_eq!(
        ids,
        [
            (
                "derived-cff-random",
                0,
                "U+0052".to_string(),
                "RRRR",
                48,
                false
            ),
            (
                "derived-rand",
                0,
                "U+0061".to_string(),
                "aaaa eeee oooo tttt",
                48,
                false
            ),
        ]
    );
    assert_eq!(
        samples[0].path,
        repo.join("target/derived-fonts/sofel-random-sample.otf")
    );
    assert_eq!(
        samples[1].path,
        repo.join("target/derived-fonts/sofel-rand-sample.ttf")
    );
}

fn gallery() -> (PathBuf, PathBuf, String) {
    let output = PathBuf::from(std::env::var_os("SOFEL_GALLERY_OUTPUT").expect("output path"));
    let binaries =
        PathBuf::from(std::env::var_os("SOFEL_GALLERY_BINARIES").expect("binaries path"));
    let output = if output.is_absolute() {
        output
    } else {
        repo().join(output)
    };
    let html = fs::read_to_string(output.join("index.html")).unwrap();
    (output, binaries, html)
}

fn rendered(id: &str, binaries: &Path) -> inputs::Rendered {
    let cases = inputs::gallery_cases(&repo()).unwrap();
    let sample = cases.iter().find(|s| s.id == id).unwrap();
    inputs::render_case(
        sample,
        &binaries.join("sofel-render"),
        &binaries.join("sofel-shape"),
    )
    .unwrap()
}

#[test]
#[ignore = "requires a generated gallery and renderer binaries"]
fn adr0083_a1_cff_random_card_has_four_equal_r() {
    let (output, binaries, html) = gallery();
    assert!(
        html.contains("<tr><td>derived-cff-random / face 0</td><td>char U+0052</td><td>RRRR</td><td>48</td><td>描画済み</td></tr>"),
        "missing derived-cff-random row"
    );
    let r = rendered("derived-cff-random", &binaries);
    assert_eq!(r.glyph_ids.len(), 4);
    assert!(r.glyph_ids.iter().all(|&g| g == 20));
    let first = r.glyphs[0].image.as_ref().unwrap();
    assert!(first.pixels.iter().any(|&p| p != 255));
    for g in &r.glyphs {
        assert_eq!(g.image.as_ref().unwrap().pixels, first.pixels);
    }
    for entry in fs::read_dir(output).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_lowercase();
        assert!(!name.ends_with(".otf") && !name.ends_with(".ttf"), "{name}");
    }
}

#[test]
#[ignore = "requires a generated gallery and renderer binaries"]
fn adr0084_a1_rand_card_mixes_alternates() {
    let (_, binaries, html) = gallery();
    assert!(
        html.contains("<tr><td>derived-rand / face 0</td><td>char U+0061</td><td>aaaa eeee oooo tttt</td><td>48</td><td>描画済み</td></tr>"),
        "missing derived-rand row"
    );
    let r = rendered("derived-rand", &binaries);
    let derived = fs::read(repo().join("target/derived-fonts/sofel-rand-sample.ttf")).unwrap();
    let sets = alternates(&derived);
    let mixed = r
        .glyph_ids
        .chunks(5)
        .filter(|word| {
            let distinct: std::collections::BTreeSet<_> = word.iter().take(4).collect();
            let set = sets.iter().find(|s| s.contains(&word[0])).unwrap();
            word.iter().take(4).all(|g| set.contains(g)) && distinct.len() >= 2
        })
        .count();
    assert!(mixed >= 1, "{:?}", r.glyph_ids);
}
