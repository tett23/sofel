//! Non-default font locations in sofel and both reference libraries (ADR 0027).

use super::{Bench, Compare, FtFace, HbFont};
use sofel_font::{
    Font,
    read::Tag,
    var::{Location, Variation},
};
use std::os::raw::c_long;
use std::path::Path;

const FONTS: [(&str, &str); 3] = [
    ("RobotoFlex-Variable", "RobotoFlex-Variable.ttf"),
    ("SourceSerifVariable-Roman", "SourceSerifVariable-Roman.ttf"),
    ("AdobeVFPrototype", "AdobeVFPrototype.otf"),
];

pub(super) fn items(bench: &mut Bench, data: &Path) {
    let weight = Variation {
        tag: Tag::new(b"wght"),
        value: 500.0,
    };
    let item = "location wght=500";
    for (name, file) in FONTS {
        if !bench.wanted(name, item) {
            continue;
        }
        let bytes = std::fs::read(data.join("harfbuzz/test/subset/data/fonts").join(file)).unwrap();
        let font = Font::new(&bytes, 0).unwrap();
        let ft = FtFace::open(&bytes).unwrap();
        let hb = HbFont::open(&bytes).unwrap();
        let make_location = || Location::new(&font, None, &[weight]);
        let location = make_location();
        let mut design: Vec<c_long> = location
            .ft_design_coords()
            .iter()
            .map(|v| c_long::from(v.0))
            .collect();
        assert!(ft.set_design(&mut design));
        hb.set_weight(weight.value);
        let mut normalized = vec![0; design.len()];
        assert!(ft.normalized(&mut normalized));
        assert_eq!(
            normalized,
            location
                .ft_coords()
                .iter()
                .map(|v| c_long::from(v.0))
                .collect::<Vec<_>>(),
            "{name} FreeType normalized coordinates"
        );
        assert_eq!(
            hb.normalized(),
            location.coords_normalized(),
            "{name} HarfBuzz normalized coordinates"
        );
        bench.item(
            name,
            item.into(),
            &mut || u64::from(std::hint::black_box(make_location()).ft_error().is_none()),
            Some((&mut || u64::from(ft.set_design(&mut design)), Compare::Yes)),
            Some((
                &mut || {
                    hb.set_weight(weight.value);
                    1
                },
                Compare::Yes,
            )),
        );
    }
}
