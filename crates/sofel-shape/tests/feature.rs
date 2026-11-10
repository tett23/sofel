//! Feature strings (ADR 0020, 機能の文字列): every example of
//! HarfBuzz's `test/api/test-common.c` (`test_types_feature`) and the
//! table of `hb_feature_from_string`'s documentation, plus the cases of
//! the parser's edges.

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use sofel_font::read::Tag;
use sofel_shape::Feature;

fn round(s: &str) -> String {
    let f = Feature::parse(s.as_bytes()).unwrap_or_else(|| panic!("{s:?} does not parse"));
    String::from_utf8(f.format()).unwrap()
}

#[test]
fn test_common_examples() {
    // test_types_feature: the string, then hb_feature_to_string of it.
    let cases = [
        ("abcd", "abcd"),
        ("abcd=1", "abcd"),
        ("+abcd", "abcd"),
        ("abcd=0", "-abcd"),
        ("-abcd", "-abcd"),
        ("abcd=2", "abcd=2"),
        ("+abcd=2", "abcd=2"),
        ("-abcd=2", "abcd=2"),
        ("\"abcd\" on", "abcd"),
        ("\"abcd\" off", "-abcd"),
        ("\"abcd\" 1", "abcd"),
        ("\"abcd\" 0", "-abcd"),
        ("\"abcd\" 2", "abcd=2"),
        ("abcd[0]", "abcd[]"),
        ("abcd[1]", "abcd[1]"),
        ("abcd[1]=1", "abcd[1]"),
        ("abcd[1]=2", "abcd[1]=2"),
        ("abcd[1]=0", "-abcd[1]"),
        ("abcd[]", "abcd"),
        ("abcd[:]", "abcd"),
        ("abcd[1:]", "abcd[1:]"),
        ("abcd[:1]", "abcd[]"),
        ("abcd[1:3]", "abcd[1:3]"),
        ("abcd[1:2]=1", "abcd[1]"),
        ("abcd[1:4]=2", "abcd[1:4]=2"),
    ];
    for (input, output) in cases {
        assert_eq!(round(input), output, "{input:?}");
    }
}

fn feature(tag: &[u8; 4], value: u32, start: u32, end: u32) -> Feature {
    Feature {
        tag: Tag::new(tag),
        value,
        start,
        end,
    }
}

#[test]
fn documented_table() {
    let all = u32::MAX;
    let cases = [
        ("kern", feature(b"kern", 1, 0, all)),
        ("+kern", feature(b"kern", 1, 0, all)),
        ("-kern", feature(b"kern", 0, 0, all)),
        ("kern=0", feature(b"kern", 0, 0, all)),
        ("kern=1", feature(b"kern", 1, 0, all)),
        ("aalt=2", feature(b"aalt", 2, 0, all)),
        ("kern[]", feature(b"kern", 1, 0, all)),
        ("kern[:]", feature(b"kern", 1, 0, all)),
        ("kern[5:]", feature(b"kern", 1, 5, all)),
        ("kern[:5]", feature(b"kern", 1, 0, 5)),
        ("kern[3:5]", feature(b"kern", 1, 3, 5)),
        ("kern[3]", feature(b"kern", 1, 3, 4)),
        ("aalt[3:5]=2", feature(b"aalt", 2, 3, 5)),
        ("'kern' 2", feature(b"kern", 2, 0, all)),
        ("\"kern\" on", feature(b"kern", 1, 0, all)),
    ];
    for (input, expected) in cases {
        assert_eq!(
            Feature::parse(input.as_bytes()),
            Some(expected),
            "{input:?}"
        );
    }
}

#[test]
fn edges() {
    let all = u32::MAX;
    let ok = [
        // Short tags are padded with spaces, and the spaces are dropped
        // again when written.
        ("a", feature(b"a   ", 1, 0, all), "a"),
        ("abc=3", feature(b"abc ", 3, 0, all), "abc=3"),
        // Space around everything; `;` for `:`.
        (
            " \t-kern [ 3 ; 5 ] \n",
            feature(b"kern", 0, 3, 5),
            "-kern[3:5]",
        ),
        ("kern = 2", feature(b"kern", 2, 0, all), "kern=2"),
        ("kern=+2", feature(b"kern", 2, 0, all), "kern=2"),
        ("kern=010", feature(b"kern", 10, 0, all), "kern=10"),
        // Negative numbers wrap, as HarfBuzz parses an int.
        ("kern=-1", feature(b"kern", all, 0, all), "kern=4294967295"),
        ("kern[-1]", feature(b"kern", 1, all, 0), "kern[4294967295]"),
        (
            "kern=2147483647",
            feature(b"kern", 0x7FFF_FFFF, 0, all),
            "kern=2147483647",
        ),
        // CSS's on and off, in any case.
        ("kern OfF", feature(b"kern", 0, 0, all), "-kern"),
        ("kern=ON", feature(b"kern", 1, 0, all), "kern"),
        // A word that is neither is consumed and ignored when there is
        // no `=` (parse_bool moves past the letters even when it fails).
        ("kern xyz", feature(b"kern", 1, 0, all), "kern"),
        ("-kern xyz", feature(b"kern", 0, 0, all), "-kern"),
        // The end of a range without its start.
        ("kern[3:]", feature(b"kern", 1, 3, all), "kern[3:]"),
        ("kern[5:3]", feature(b"kern", 1, 5, 3), "kern[5:3]"),
        ("kern[3:3]", feature(b"kern", 1, 3, 3), "kern[3:3]"),
        ("kern[0:1]", feature(b"kern", 1, 0, 1), "kern[]"),
        // Any bytes but the stops make a tag; a space, `=` and `[` stop
        // a quoted tag too.
        ("k:rn", feature(b"k:rn", 1, 0, all), "k:rn"),
        // Letters after the rest are skipped the same way.
        ("kern[3:5]x", feature(b"kern", 1, 3, 5), "kern[3:5]"),
        ("'kern'x", feature(b"kern", 1, 0, all), "kern"),
    ];
    for (input, expected, written) in ok {
        let f = Feature::parse(input.as_bytes());
        assert_eq!(f, Some(expected), "{input:?}");
        assert_eq!(
            String::from_utf8(expected.format()).unwrap(),
            written,
            "{input:?}"
        );
    }
    let bad = [
        "",
        " ",
        "=",
        "-",
        "abcde",
        "kern\t",
        "kern\t=2",
        "kern=",
        "kern=x",
        "kern=on2",
        "kern=2x",
        "kern=2147483648",
        "kern[",
        "kern[3",
        "kern[x]",
        "kern[3:5]]",
        "kern[3:5]-",
        "kern 2 3",
        "kern==2",
        "'abc'",
        "'abcde'",
        "'kern\"",
        "\"kern'",
        "'kern",
        "''",
        "--kern",
        "+-kern",
        "kern[4294967296]",
        "'k rn'",
        "'k=rn'",
        "'k[rn'",
    ];
    for input in bad {
        assert_eq!(Feature::parse(input.as_bytes()), None, "{input:?}");
    }
}

#[test]
fn bytes_are_kept() {
    // A tag of three bytes that are not ASCII, padded with a space.
    let f = Feature::parse(b"\xC3\xA9t").unwrap();
    assert_eq!(f.tag, Tag::new(b"\xC3\xA9t "));
    assert_eq!(f.format(), b"\xC3\xA9t");
    // A 0 byte ends an unquoted tag (the stop of a missing quote), and
    // the tag of a quoted one stops copying at it; a leading 0 gives the
    // tag 0.
    assert_eq!(Feature::parse(b"ke\0n"), None);
    assert_eq!(Feature::parse(b"'k\0rn'").unwrap().tag, Tag::new(b"k   "));
    assert_eq!(Feature::parse(b"'\0krn'").unwrap().tag, Tag::from_u32(0));
    // Writing appends.
    let mut out = b"x,".to_vec();
    feature(b"liga", 0, 2, 9).write(&mut out);
    assert_eq!(out, b"x,-liga[2:9]");
}
