#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use sofel_conformance::dump::parse_ft_dump;

#[test]
fn parses_every_line_kind() {
    let text = "\
face 0
num_glyphs 3
head version 65536
head created 0 3000000000
hhea ascender -1
maxp num_glyphs 3
cmap 3 1 4
map 65 1
map 66 2
cmap 0 5 14
advance 0 500
advance 1 -2
end
face 1
error 2
end
";
    let faces = parse_ft_dump(text).unwrap();
    assert_eq!(faces.len(), 2);
    let f = &faces[0];
    assert_eq!(f.num_glyphs, Some(3));
    assert_eq!(
        f.head,
        vec![
            ("version".into(), "65536".into()),
            ("created".into(), "0 3000000000".into())
        ]
    );
    assert_eq!(f.hhea, vec![("ascender".into(), "-1".into())]);
    assert_eq!(f.maxp, vec![("num_glyphs".into(), "3".into())]);
    assert_eq!(f.charmaps.len(), 2);
    assert_eq!(
        (
            f.charmaps[0].platform,
            f.charmaps[0].encoding,
            f.charmaps[0].format
        ),
        (3, 1, 4)
    );
    assert_eq!(f.charmaps[0].maps, vec![(65, 1), (66, 2)]);
    assert!(f.charmaps[1].maps.is_empty());
    assert_eq!(f.advances, vec![(0, Some(500)), (1, Some(-2))]);
    assert_eq!(faces[1].error.as_deref(), Some("2"));
}

#[test]
fn rejects_malformed_output() {
    assert!(parse_ft_dump("head version 1\n").is_err());
    assert!(parse_ft_dump("face 0\nmap 65 1\n").is_err());
    assert!(parse_ft_dump("face 0\nadvance x 1\n").is_err());
    assert!(parse_ft_dump("face 0\nbogus 1\n").is_err());
}
