//! An unpinned CFF2 with an explicit `vsindex` and no subroutine calls.

#![allow(clippy::all, clippy::unwrap_used, clippy::arithmetic_side_effects)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;
#[path = "support/fixture_input.rs"]
mod fixture_input;

use build::{Cff2Spec, Cs, cff2, charstring, item_variation_store, otto_font};
use sofel_font::Font;
use sofel_subset::{Flags, Input, SetKind, subset};

fn input_font() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0];
    for value in [16u16, 2, 1, 20, 0, 10] {
        fvar.extend(value.to_be_bytes());
    }
    fvar.extend(*b"wght");
    for value in [100i32, 400, 900] {
        fvar.extend((value * 65536).to_be_bytes());
    }
    fvar.extend([0, 0, 1, 0]);
    let store = item_variation_store(
        1,
        &[vec![(0, 16384, 16384)], vec![(-16384, -16384, 0)]],
        &[vec![0, 1], vec![1]],
    );
    let charstring = charstring(&[Cs::I(0), Cs::Op(15), Cs::I(0), Cs::I(0), Cs::Op(21)]);
    let spec = Cff2Spec {
        charstrings: vec![vec![], charstring],
        fds: vec![(vec![], vec![])],
        vstore: Some(store),
        ..Cff2Spec::default()
    };
    otto_font(2, vec![(b"CFF2", cff2(&spec)), (b"fvar", fvar)])
}

#[test]
fn cff2_flatten_preserves_explicit_vsindex_without_blend() {
    let bytes = input_font();
    fixture_input::export("cff2-flat-vsindex-hb15.otf", &bytes).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let input = Input::new().map_set(SetKind::Glyphs, |set| set.add(1));
    let output = subset(&font, &input).unwrap();
    assert_eq!(output, include_bytes!("data/cff2-flat-vsindex-hb15.otf"),);
}

#[test]
fn cff2_desubroutinize_drops_unused_vsindex() {
    let bytes = input_font();
    fixture_input::export("cff2-flat-vsindex-hb15.otf", &bytes).unwrap();
    let font = Font::new(&bytes, 0).unwrap();
    let input = Input::new()
        .map_set(SetKind::Glyphs, |set| set.add(1))
        .with_flags(Flags::DESUBROUTINIZE);
    let output = subset(&font, &input).unwrap();
    assert_eq!(
        output,
        include_bytes!("data/cff2-flat-vsindex-desubr-hb15.otf"),
    );
}
