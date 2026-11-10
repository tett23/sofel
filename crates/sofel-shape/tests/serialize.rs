//! Serialization (ADR 0020, 直列化): every example of HarfBuzz's
//! `test/api/test-buffer.c` (`test_buffer_serialize_deserialize`,
//! `test_buffer_deserialize2`, `test_buffer_serialize_no_advances`),
//! the forms of each flag, and round trips.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]

use sofel_font::GlyphExtents;
use sofel_shape::{
    DeserializeError, EmptyFont, GlyphFlags, GlyphInfo, GlyphPosition, SerializeFlags,
    SerializeFont, SerializeFormat, deserialize_glyphs, deserialize_unicode, glyph_from_string,
    glyph_to_string, serialize_glyphs, serialize_glyphs_range, serialize_invalid,
    serialize_unicode, serialize_unicode_range,
};

use SerializeFormat::{Json, Text};

/// A font with names for glyphs 1 to 5 and boxes for glyphs 1 and 2.
struct TestFont;

const NAMES: [&[u8]; 5] = [b"icon0", b"a\"b\\c", b"x|y", b"nul\0cut", b"uni0041"];

impl SerializeFont for TestFont {
    fn glyph_name(&self, glyph: u32) -> Option<&[u8]> {
        NAMES
            .get(usize::try_from(glyph).ok()?.checked_sub(1)?)
            .copied()
    }
    fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        (1..=2).contains(&glyph).then_some(GlyphExtents {
            x_bearing: 10,
            y_bearing: 700,
            width: 500,
            height: -710 - glyph as i32,
        })
    }
    fn glyph_from_name(&self, name: &[u8]) -> Option<u32> {
        (1u32..)
            .zip(NAMES)
            .find(|(_, n)| *n == name)
            .map(|(g, _)| g)
    }
    fn nominal_glyph(&self, c: u32) -> Option<u32> {
        (c == 0x41).then_some(7)
    }
}

fn info(glyph: u32, cluster: u32) -> GlyphInfo {
    GlyphInfo {
        glyph,
        cluster,
        flags: GlyphFlags::default(),
    }
}

fn pos(x_advance: i32, y_advance: i32, x_offset: i32, y_offset: i32) -> GlyphPosition {
    GlyphPosition {
        x_advance,
        y_advance,
        x_offset,
        y_offset,
    }
}

fn glyphs_string(
    infos: &[GlyphInfo],
    positions: Option<&[GlyphPosition]>,
    font: &dyn SerializeFont,
    format: SerializeFormat,
    flags: SerializeFlags,
) -> String {
    let mut out = Vec::new();
    serialize_glyphs(infos, positions, font, format, flags, &mut out);
    String::from_utf8(out).unwrap()
}

fn unicode_string(
    cps: &[u32],
    clusters: &[u32],
    format: SerializeFormat,
    flags: SerializeFlags,
) -> String {
    let mut out = Vec::new();
    serialize_unicode(cps, clusters, format, flags, &mut out);
    String::from_utf8(out).unwrap()
}

type Glyphs = (
    Result<(), DeserializeError>,
    Vec<GlyphInfo>,
    Vec<GlyphPosition>,
);

fn read_glyphs(s: &str, format: SerializeFormat, font: &dyn SerializeFont) -> Glyphs {
    let (mut infos, mut positions) = (Vec::new(), Vec::new());
    let r = deserialize_glyphs(s.as_bytes(), format, font, &mut infos, &mut positions);
    (r, infos, positions)
}

fn read_unicode(
    s: &str,
    format: SerializeFormat,
) -> (Result<(), DeserializeError>, Vec<(u32, u32)>) {
    let (mut cps, mut clusters) = (Vec::new(), Vec::new());
    let r = deserialize_unicode(s.as_bytes(), format, &mut cps, &mut clusters);
    (r, cps.into_iter().zip(clusters).collect())
}

fn err(consumed: usize) -> Result<(), DeserializeError> {
    Err(DeserializeError { consumed })
}

#[test]
fn serialize_deserialize() {
    // test_buffer_serialize_deserialize: read as Unicode, then written
    // back when the read succeeds without a parsed length.
    for (s, format) in [
        ("<U+0640=0|U+0635=1>", Text),
        ("[{\"u\":1600,\"cl\":0},{\"u\":1589,\"cl\":1}]", Json),
    ] {
        let (r, items) = read_unicode(s, format);
        assert_eq!(r, Ok(()));
        assert_eq!(items, [(0x640, 0), (0x635, 1)]);
        let (cps, clusters): (Vec<u32>, Vec<u32>) = items.into_iter().unzip();
        assert_eq!(
            unicode_string(&cps, &clusters, format, SerializeFlags::DEFAULT),
            s
        );
    }
    // Unfinished: the second item is not added, and the read succeeds
    // having consumed everything (36 bytes).
    let s = "[{\"u\":1600,\"cl\":0},{\"u\":1589,\"cl\":1}";
    assert_eq!(s.len(), 36);
    assert_eq!(read_unicode(s, Json), (Ok(()), vec![(0x640, 0)]));
    // Mixed glyphs and characters fail.
    let (r, items) = read_unicode("[{\"u\":1600,\"cl\":0},{\"g\":1589,\"cl\":1}]", Json);
    assert_eq!((r, items), (err(18), vec![(0x640, 0)]));
    let (r, items) = read_unicode("<U+0640=0|uni0635=1>", Text);
    assert_eq!((r, items), (err(11), vec![(0x640, 0)]));
    // An empty buffer of no content.
    let mut out = Vec::new();
    serialize_invalid(Json, &mut out);
    assert_eq!(out, b"[]");
    out.clear();
    serialize_invalid(Text, &mut out);
    assert_eq!(out, b"!!");
}

#[test]
fn deserialize2() {
    // test_buffer_deserialize2, at full length; the prefixes are compared
    // with HarfBuzz by sofel-conformance.
    let u = |s: &str, f| read_unicode(s, f);
    assert_eq!(
        u("<U+0041=0|U+0042=1>", Text),
        (Ok(()), vec![(65, 0), (66, 1)])
    );
    assert_eq!(
        u("[{\"u\":65,\"cl\":0},{\"u\":66,\"cl\":1}]", Json),
        (Ok(()), vec![(65, 0), (66, 1)])
    );
    assert_eq!(u("<U+0041=0>", Text), (Ok(()), vec![(65, 0)]));
    assert_eq!(u("[{\"u\":65}]", Json), (Ok(()), vec![(65, 0)]));
    assert_eq!(u("<U+0041=0|U+0042=?>", Text), (err(17), vec![(65, 0)]));
    assert_eq!(u("[{\"u\":65},?]", Json), (err(10), vec![(65, 0)]));
    assert_eq!(
        u("<U+0041=0|U+0042=4294967296>", Text),
        (err(10), vec![(65, 0)])
    );
    assert_eq!(
        u("[{\"u\":65},{\"u\":4294967296}]", Json),
        (err(9), vec![(65, 0)])
    );
    assert_eq!(u("", Text), (err(0), vec![]));
    assert_eq!(u("", Json), (err(0), vec![]));

    for font in [&EmptyFont as &dyn SerializeFont, &TestFont] {
        let (r, infos, positions) = read_glyphs("[1=0@2,3+4,5|2=1+6]", Text, font);
        assert_eq!(r, Ok(()));
        assert_eq!(infos, [info(1, 0), info(2, 1)]);
        assert_eq!(positions, [pos(4, 5, 2, 3), pos(6, 0, 0, 0)]);
        let (r, infos, positions) = read_glyphs(
            "[{\"g\":1,\"cl\":0,\"dx\":2,\"dy\":3,\"ax\":4,\"ay\":5},{\"g\":2,\"cl\":1}]",
            Json,
            font,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(infos, [info(1, 0), info(2, 1)]);
        assert_eq!(positions, [pos(4, 5, 2, 3), pos(0, 0, 0, 0)]);
        let (r, infos, _) = read_glyphs("[1=0|2=?]", Text, font);
        assert_eq!((r, infos), (err(7), vec![info(1, 0)]));
        let (r, infos, _) = read_glyphs("[{\"g\":1},?]", Json, font);
        assert_eq!((r, infos), (err(9), vec![info(1, 0)]));
        let (r, infos, _) = read_glyphs("[1=0|2=4294967296]", Text, font);
        assert_eq!((r, infos), (err(5), vec![info(1, 0)]));
        let (r, infos, _) = read_glyphs("[{\"g\":1},{\"g\":2,\"ax\":2147483648}]", Json, font);
        assert_eq!((r, infos), (err(8), vec![info(1, 0)]));
        assert_eq!(read_glyphs("", Text, font).0, err(0));
    }
    // Glyph names need the font.
    let (r, infos, positions) = read_glyphs("[icon0=0+10]", Text, &TestFont);
    assert_eq!(
        (r, infos, positions),
        (Ok(()), vec![info(1, 0)], vec![pos(10, 0, 0, 0)])
    );
    let (r, infos, positions) = read_glyphs("[{\"g\":\"icon0\",\"ax\":10}]", Json, &TestFont);
    assert_eq!(
        (r, infos, positions),
        (Ok(()), vec![info(1, 0)], vec![pos(10, 0, 0, 0)])
    );
    assert_eq!(read_glyphs("[icon0=0+10]", Text, &EmptyFont).0, err(0));
    assert_eq!(
        read_glyphs("[{\"g\":\"icon0\",\"ax\":10}]", Json, &EmptyFont).0,
        err(0)
    );
}

#[test]
fn serialize_no_advances() {
    // Roboto-Regular.ac.ttf shaping "aaa": glyph 1 without a name, 1114
    // wide.
    let infos = [info(1, 0), info(1, 1), info(1, 2)];
    let positions = [pos(1114, 0, 0, 0); 3];
    let flags = SerializeFlags::NO_ADVANCES;
    let text = "[gid1=0|gid1=1@1114,0|gid1=2@2228,0]";
    let json = "[{\"g\":\"gid1\",\"cl\":0,\"dx\":0,\"dy\":0},\
{\"g\":\"gid1\",\"cl\":1,\"dx\":1114,\"dy\":0},\
{\"g\":\"gid1\",\"cl\":2,\"dx\":2228,\"dy\":0}]";
    assert_eq!(
        glyphs_string(&infos, Some(&positions), &EmptyFont, Text, flags),
        text
    );
    assert_eq!(
        glyphs_string(&infos, Some(&positions), &EmptyFont, Json, flags),
        json
    );
    // The test writes into 32 and 64 bytes at a time, as hb-shape does
    // into 32768; the pieces join.
    for (format, whole, size) in [(Text, text, 32), (Json, json, 64)] {
        let (mut out, mut start, mut pieces) = (Vec::new(), 0, 0);
        while start < infos.len() {
            let before = out.len();
            start += serialize_glyphs_range(
                &infos,
                Some(&positions),
                start,
                infos.len(),
                size,
                &EmptyFont,
                format,
                flags,
                &mut out,
            );
            assert!(out.len() - before < size);
            if out.len() == before {
                break;
            }
            pieces += 1;
        }
        assert_eq!(String::from_utf8(out).unwrap(), whole);
        assert!(pieces > 1);
    }
}

#[test]
fn glyph_forms() {
    let mut infos = [info(1, 0), info(2, 3), info(9, 4)];
    infos[1].flags = GlyphFlags(0x3);
    infos[2].flags = GlyphFlags(0xF8 | 0x4);
    let positions = [pos(500, 0, 0, 0), pos(600, -20, 5, -7), pos(0, 1000, 0, 0)];
    let p = Some(&positions[..]);
    let f = TestFont;
    let g = |format, flags| glyphs_string(&infos, p, &f, format, SerializeFlags(flags));
    assert_eq!(
        g(Text, 0),
        "[icon0=0+500|a\"b\\c=3@5,-7+600,-20|gid9=4+0,1000]"
    );
    assert_eq!(g(Text, 1), "[icon0+500|a\"b\\c@5,-7+600,-20|gid9+0,1000]");
    assert_eq!(g(Text, 2), "[icon0=0|a\"b\\c=3|gid9=4]");
    assert_eq!(g(Text, 4), "[1=0+500|2=3@5,-7+600,-20|9=4+0,1000]");
    assert_eq!(
        g(Text, 8),
        "[icon0=0+500<10,700,500,-711>|a\"b\\c=3@5,-7+600,-20<10,700,500,-712>|gid9=4+0,1000]"
    );
    assert_eq!(
        g(Text, 16),
        "[icon0=0+500|a\"b\\c=3@5,-7+600,-20#3|gid9=4+0,1000#4]"
    );
    assert_eq!(g(Text, 32), "[icon0=0|a\"b\\c=3@505,-7|gid9=4@1100,-20]");
    assert_eq!(
        g(Text, 0x3F),
        "[1<10,700,500,-711>|2#3<10,700,500,-712>|9#4]"
    );
    assert_eq!(
        g(Json, 0),
        "[{\"g\":\"icon0\",\"cl\":0,\"dx\":0,\"dy\":0,\"ax\":500,\"ay\":0},\
{\"g\":\"a\\\"b\\\\c\",\"cl\":3,\"dx\":5,\"dy\":-7,\"ax\":600,\"ay\":-20},\
{\"g\":\"gid9\",\"cl\":4,\"dx\":0,\"dy\":0,\"ax\":0,\"ay\":1000}]"
    );
    assert_eq!(
        g(Json, 0x1F),
        "[{\"g\":1,\"xb\":10,\"yb\":700,\"w\":500,\"h\":-711},\
{\"g\":2,\"fl\":3,\"xb\":10,\"yb\":700,\"w\":500,\"h\":-712},\
{\"g\":9,\"fl\":4}]"
    );
    assert_eq!(
        g(Json, 32),
        "[{\"g\":\"icon0\",\"cl\":0,\"dx\":0,\"dy\":0},\
{\"g\":\"a\\\"b\\\\c\",\"cl\":3,\"dx\":505,\"dy\":-7},\
{\"g\":\"gid9\",\"cl\":4,\"dx\":1100,\"dy\":-20}]"
    );
    // Without positions, as with NO_POSITIONS.
    assert_eq!(
        glyphs_string(&infos, None, &f, Text, SerializeFlags::DEFAULT),
        "[icon0=0|a\"b\\c=3|gid9=4]"
    );
    // Nothing for no glyphs.
    assert_eq!(
        glyphs_string(&[], None, &f, Json, SerializeFlags::DEFAULT),
        ""
    );
    // A range in the middle has neither bracket; past the end, nothing.
    let mut out = Vec::new();
    assert_eq!(
        serialize_glyphs_range(
            &infos,
            p,
            1,
            2,
            usize::MAX,
            &f,
            Text,
            SerializeFlags::DEFAULT,
            &mut out
        ),
        1
    );
    assert_eq!(out, b"|a\"b\\c=3@5,-7+600,-20]");
    assert_eq!(
        serialize_glyphs_range(
            &infos,
            p,
            5,
            9,
            usize::MAX,
            &f,
            Text,
            SerializeFlags::DEFAULT,
            &mut out
        ),
        0
    );
    assert_eq!(
        serialize_glyphs_range(
            &infos,
            p,
            2,
            1,
            usize::MAX,
            &f,
            Text,
            SerializeFlags::DEFAULT,
            &mut out
        ),
        0
    );
    // NO_ADVANCES sums from glyph 0 even for a later range, saturating.
    out.clear();
    let big = [
        pos(i32::MAX, 0, 0, 0),
        pos(5, i32::MIN, 3, -1),
        pos(0, 0, 0, 0),
    ];
    serialize_glyphs_range(
        &infos,
        Some(&big),
        1,
        3,
        usize::MAX,
        &f,
        Text,
        SerializeFlags(32 | 1),
        &mut out,
    );
    assert_eq!(out, b"|a\"b\\c@2147483647,-1|gid9@2147483647,-2147483648]");
}

#[test]
fn glyph_strings() {
    let s = |g| {
        let mut out = Vec::new();
        glyph_to_string(&TestFont, g, &mut out);
        out
    };
    assert_eq!(s(1), b"icon0");
    assert_eq!(s(4), b"nul");
    assert_eq!(s(0), b"gid0");
    assert_eq!(s(u32::MAX), b"gid4294967295");
    // Names are cut to 127 bytes.
    struct Long;
    impl SerializeFont for Long {
        fn glyph_name(&self, _: u32) -> Option<&[u8]> {
            Some(&[b'n'; 300])
        }
    }
    let mut out = Vec::new();
    glyph_to_string(&Long, 0, &mut out);
    assert_eq!(out.len(), 127);

    let g = |s: &[u8]| glyph_from_string(&TestFont, s);
    assert_eq!(g(b"icon0"), Some(1));
    // A name of the font comes before the other forms.
    assert_eq!(g(b"uni0041"), Some(5));
    assert_eq!(glyph_from_string(&EmptyFont, b"uni0041"), None);
    assert_eq!(g(b"uni41"), Some(7));
    assert_eq!(g(b"uni0x41"), Some(7));
    assert_eq!(g(b"uni42"), None);
    assert_eq!(g(b"12"), Some(12));
    assert_eq!(g(b" +12"), Some(12));
    assert_eq!(g(b"-1"), Some(u32::MAX));
    assert_eq!(g(b"gid12"), Some(12));
    assert_eq!(g(b"gid"), None);
    assert_eq!(g(b"gid12x"), None);
    assert_eq!(g(b"4294967296"), None);
    assert_eq!(g(b""), None);
}

#[test]
fn unicode_forms() {
    let cps = [0x41, 0x1F600, 0x10FFFF, 0];
    let clusters = [0, 1, 5, 9];
    let d = SerializeFlags::DEFAULT;
    assert_eq!(
        unicode_string(&cps, &clusters, Text, d),
        "<U+0041=0|U+1F600=1|U+10FFFF=5|U+0000=9>"
    );
    assert_eq!(
        unicode_string(&cps, &clusters, Text, SerializeFlags::DEFINED),
        "<U+0041|U+1F600|U+10FFFF|U+0000>"
    );
    assert_eq!(
        unicode_string(&cps, &clusters, Json, d),
        "[{\"u\":65,\"cl\":0},{\"u\":128512,\"cl\":1},{\"u\":1114111,\"cl\":5},{\"u\":0,\"cl\":9}]"
    );
    assert_eq!(
        unicode_string(&cps, &clusters, Json, SerializeFlags::NO_CLUSTERS),
        "[{\"u\":65},{\"u\":128512},{\"u\":1114111},{\"u\":0}]"
    );
    // As many as the shorter slice; ranges as for glyphs.
    assert_eq!(unicode_string(&cps, &clusters[..1], Text, d), "<U+0041=0>");
    let mut out = Vec::new();
    assert_eq!(
        serialize_unicode_range(&cps, &clusters, 3, 4, usize::MAX, Text, d, &mut out),
        1
    );
    assert_eq!(out, b"|U+0000=9>");
}

#[test]
fn format_names() {
    assert_eq!(SerializeFormat::from_name(b"text"), Some(Text));
    assert_eq!(SerializeFormat::from_name(b"JSON"), Some(Json));
    assert_eq!(SerializeFormat::from_name(b"Texts"), Some(Text));
    assert_eq!(SerializeFormat::from_name(b"tex"), None);
    assert_eq!(SerializeFormat::from_name(b""), None);
    assert_eq!((Text.name(), Json.name()), ("text", "json"));
}

#[test]
fn reading_quirks() {
    // A space after an item's `,` or `]` adds it again (HarfBuzz's
    // @add_item on every final state).
    let (r, infos, _) = read_glyphs("[{\"g\":1}, {\"g\":2}]", Json, &EmptyFont);
    assert_eq!(
        (r, infos),
        (Ok(()), vec![info(1, 0), info(1, 0), info(2, 0)])
    );
    let (r, infos, _) = read_glyphs(" [ { \"g\" : 1 , \"cl\" : 3 } ]  ", Json, &EmptyFont);
    assert_eq!((r, infos), (Ok(()), vec![info(1, 3); 3]));
    // Without brackets, and `]` between items.
    let (r, infos, _) = read_glyphs("{\"g\":1}]{\"g\":2},", Json, &EmptyFont);
    assert_eq!((r, infos), (Ok(()), vec![info(1, 0), info(2, 0)]));
    let (r, infos, _) = read_glyphs("1]2|3", Text, &EmptyFont);
    assert_eq!((r, infos), (Ok(()), vec![info(1, 0), info(2, 0)]));
    // An item is added when the next starts: a byte that cannot start
    // one stops before the item is added.
    let (r, infos, _) = read_glyphs("[1|,]", Text, &EmptyFont);
    assert_eq!((r, infos), (err(3), vec![]));
    let (r, items) = read_unicode("<U+41|x", Text);
    assert_eq!((r, items), (err(6), vec![]));
    let (r, items) = read_unicode("<u41>U42>", Text);
    assert_eq!((r, items), (Ok(()), vec![(0x41, 0), (0x42, 0)]));
    // The flags and the box.
    let (r, infos, positions) = read_glyphs("[gid1=0@1,2+3,4#7<1,2,3,4>|uni0041]", Text, &TestFont);
    assert_eq!(r, Ok(()));
    assert_eq!(
        infos,
        [
            GlyphInfo {
                glyph: 1,
                cluster: 0,
                flags: GlyphFlags(7)
            },
            info(5, 0)
        ]
    );
    assert_eq!(positions, [pos(3, 4, 1, 2), pos(0, 0, 0, 0)]);
    // `<` after a name both continues the name and starts the box: the
    // glyph is looked up at the `<`, and again at the end of the name if
    // the box dies before its first `,`.
    let (r, infos, _) = read_glyphs("[icon0<1,2,3,4>|0<-1,0,0,0>]", Text, &TestFont);
    assert_eq!((r, infos), (Ok(()), vec![info(1, 0), info(0, 0)]));
    assert_eq!(read_glyphs("[x<1,2,3,4>]", Text, &TestFont).0, err(0));
    assert_eq!(read_glyphs("[icon0<1>|", Text, &TestFont).0, err(0));
    assert_eq!(read_glyphs("[icon0<01,2,3,4>]", Text, &TestFont).0, err(9));
    // Escapes stay in the name; `\#` is not one.
    let (r, infos, _) = read_glyphs("[x\\|y=1]", Text, &TestFont);
    assert_eq!((r, infos), (err(0), vec![]));
    assert_eq!(read_glyphs("[a\\#b]", Text, &TestFont).0, err(3));
    // A glyph in a Unicode buffer and a character in a glyph buffer.
    assert_eq!(read_glyphs("[{\"u\":1}]", Json, &EmptyFont).0, err(0));
    assert_eq!(read_unicode("[{\"u\":1},{\"g\":\"a\"}]", Json).0, err(8));
    // Leading zeros are not numbers of the grammar.
    assert_eq!(read_glyphs("[1=01]", Text, &EmptyFont).0, err(4));
    assert_eq!(read_glyphs("[{\"g\":01}]", Json, &EmptyFont).0, err(7));
}

#[test]
fn round_trips() {
    let mut infos = vec![
        info(1, 0),
        info(2, 1),
        info(3, 1),
        info(4, 7),
        info(0, 9),
        info(77, 10),
        info(5, 10),
    ];
    for (i, g) in infos.iter_mut().enumerate() {
        g.flags = GlyphFlags(i as u32 % 8);
    }
    let positions: Vec<GlyphPosition> = (0..infos.len() as i32)
        .map(|i| pos(100 * i - 150, -i, i * 7 - 20, 3 - i))
        .collect();
    // The names of glyphs 2 to 4 do not read back ("a\"b\\c" and "x|y"
    // hold delimiters, which HarfBuzz neither escapes in text nor
    // unescapes in JSON, and "nul\0cut" is cut), so with names they are
    // left out.
    let keep = |g: &GlyphInfo| !matches!(g.glyph, 2..=4);
    let named: Vec<GlyphInfo> = infos.iter().copied().filter(keep).collect();
    let named_positions: Vec<GlyphPosition> = infos
        .iter()
        .zip(&positions)
        .filter(|(g, _)| keep(g))
        .map(|(_, p)| *p)
        .collect();
    for format in [Text, Json] {
        for flags in [0, 4, 16, 16 | 4, 8 | 16] {
            let flags = SerializeFlags(flags);
            let sets = if flags.contains(SerializeFlags::NO_GLYPH_NAMES) {
                [(&infos, &positions), (&named, &named_positions)]
            } else {
                [(&named, &named_positions), (&named, &named_positions)]
            };
            for (infos, positions) in sets {
                let mut out = Vec::new();
                serialize_glyphs(infos, Some(positions), &TestFont, format, flags, &mut out);
                let (mut back, mut back_positions) = (Vec::new(), Vec::new());
                let r = deserialize_glyphs(&out, format, &TestFont, &mut back, &mut back_positions);
                assert_eq!(r, Ok(()), "{}", String::from_utf8_lossy(&out));
                let expected: Vec<GlyphInfo> = infos
                    .iter()
                    .map(|g| GlyphInfo {
                        flags: GlyphFlags(if flags.contains(SerializeFlags::GLYPH_FLAGS) {
                            g.flags.0 & 7
                        } else {
                            0
                        }),
                        ..*g
                    })
                    .collect();
                assert_eq!(back, expected, "{}", String::from_utf8_lossy(&out));
                assert_eq!(&back_positions, positions);
            }
        }
    }
    let cps = [0x41, 0x1F600, 0x10FFFF, 0, 0xFFFF_FFFF];
    let clusters = [0, 1, 5, 9, u32::MAX];
    for format in [Text, Json] {
        let mut out = Vec::new();
        serialize_unicode(&cps, &clusters, format, SerializeFlags::DEFAULT, &mut out);
        let (mut c, mut cl) = (Vec::new(), Vec::new());
        assert_eq!(deserialize_unicode(&out, format, &mut c, &mut cl), Ok(()));
        assert_eq!((c.as_slice(), cl.as_slice()), (&cps[..], &clusters[..]));
    }
}
