//! CFF2 blend scalar columns at one variable location (ADR 0027, ADR 0030).

#![allow(clippy::unwrap_used)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use build::{Cff2Spec, cff2, dict, item_variation_store, otto_font};
use sofel_font::read::cff::FtBlend;
use sofel_font::read::cff::dict::BlendWeights;
use sofel_font::var::{BlendError, Ft, HbF64, Location};
use sofel_font::{Cff, Font};
use std::borrow::Cow;

fn fvar_one_axis() -> Vec<u8> {
    let mut fvar = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 8];
    fvar.extend_from_slice(b"wght");
    for value in [0i32, 0, 0x10000] {
        fvar.extend_from_slice(&value.to_be_bytes());
    }
    fvar.extend_from_slice(&[0, 0, 1, 0]);
    fvar
}

fn font_with_store(store: Option<Vec<u8>>) -> Vec<u8> {
    let table = cff2(&Cff2Spec {
        charstrings: vec![Vec::new()],
        fds: vec![(Vec::new(), Vec::new())],
        vstore: store,
        ..Cff2Spec::default()
    });
    otto_font(1, vec![(b"CFF2", table), (b"fvar", fvar_one_axis())])
}

#[test]
fn adr0030_i06_cff2_preserves_zero_columns_and_order() {
    let store = item_variation_store(
        1,
        &[vec![(0, 0x4000, 0x4000)], vec![(0, 0, 0)]],
        &[vec![0, 1, 0]],
    );
    let bytes = font_with_store(Some(store));
    let font = Font::new(&bytes, 0).unwrap();
    let default = Location::from_design(&font, &[0.0]);
    let max = Location::from_design(&font, &[1.0]);

    let ft = font.instance(&default).blend_scalars::<Ft>(0).unwrap();
    assert_eq!(ft.len(), 3);
    assert_eq!(ft.iter().collect::<Vec<_>>(), vec![0, 0x10000, 0]);
    let cff = Cff::new(&font).unwrap();
    let at = font.instance(&default);
    let adapter = at.ft_blend_weights(&cff);
    let old = FtBlend::new(cff.variation_store(), &[0]);
    assert_eq!(adapter.regions(0), old.regions(0));
    for index in 0..3 {
        assert_eq!(adapter.weight(0, index), old.weight(0, index));
    }

    let at_max = font.instance(&max);
    let adapter_max = at_max.ft_blend_weights(&cff);
    let old_max = FtBlend::new(cff.variation_store(), &[0x10000]);
    assert_eq!(adapter_max.regions(0), old_max.regions(0));
    for index in 0..3 {
        assert_eq!(adapter_max.weight(0, index), old_max.weight(0, index));
    }

    let hb = font.instance(&max).blend_scalars::<HbF64>(0).unwrap();
    assert_eq!(hb.len(), 3);
    assert_eq!(hb.iter().collect::<Vec<_>>(), vec![1.0, 1.0, 1.0]);
}

#[test]
fn adr0030_i06_cff2_distinguishes_empty_columns_from_errors() {
    let empty = item_variation_store(1, &[vec![(0, 0x4000, 0x4000)]], &[vec![]]);
    let bytes = font_with_store(Some(empty));
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[1.0]);
    assert_eq!(
        font.instance(&location)
            .blend_scalars::<Ft>(0)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        font.instance(&location).blend_scalars::<Ft>(1).unwrap_err(),
        BlendError::VsindexOutOfRange
    );

    let bytes = font_with_store(None);
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        font.default_instance().blend_scalars::<Ft>(0).unwrap_err(),
        BlendError::NoStore
    );

    let mismatch = item_variation_store(2, &[vec![(0, 0x4000, 0x4000); 2]], &[vec![0]]);
    let bytes = font_with_store(Some(mismatch));
    let font = Font::new(&bytes, 0).unwrap();
    assert_eq!(
        font.default_instance().blend_scalars::<Ft>(0).unwrap_err(),
        BlendError::AxisCountMismatch
    );
}

#[test]
fn adr0027_i04_private_dict_reparses_for_each_location() {
    use sofel_font::read::cff::dict::op;
    let store = item_variation_store(1, &[vec![(0, 0x4000, 0x4000)]], &[vec![0]]);
    let private = dict(&[(&[10, 4, 1], op::BLEND), (&[], op::STD_HW)]);
    let table = cff2(&Cff2Spec {
        charstrings: vec![Vec::new()],
        fds: vec![(private, Vec::new())],
        vstore: Some(store),
        ..Cff2Spec::default()
    });
    let bytes = otto_font(1, vec![(b"CFF2", table), (b"fvar", fvar_one_axis())]);
    let font = Font::new(&bytes, 0).unwrap();
    let cff = Cff::new(&font).unwrap();
    let default = Location::from_design(&font, &[0.0]);
    let max = Location::from_design(&font, &[1.0]);
    let at_default = font.instance(&default);
    let at_max = font.instance(&max);
    assert_eq!(
        cff.private_dict_with_blend(0, &at_default.ft_blend_weights(&cff))
            .unwrap()
            .std_hw,
        10
    );
    assert_eq!(
        cff.private_dict_with_blend(0, &at_max.ft_blend_weights(&cff))
            .unwrap()
            .std_hw,
        14
    );
    assert_eq!(cff.private_dict(0).unwrap().std_hw, 10);
}

#[test]
fn adr0030_i03_cff2_region_cache_requires_same_source() {
    let store = item_variation_store(1, &[vec![(0, 0x4000, 0x4000)]], &[vec![0]]);
    let bytes = font_with_store(Some(store));
    let font = Font::new(&bytes, 0).unwrap();
    let location = Location::from_design(&font, &[1.0]);
    let cff = Cff::new(&font).unwrap();
    let data = cff.variation_store().unwrap();
    let same_instance = font.instance(&location);
    let same = same_instance.cff2_hb_region_scalars(&data);
    assert!(matches!(same, Cow::Borrowed(_)));

    let copied_bytes = bytes.clone();
    let copied_font = Font::new(&copied_bytes, 0).unwrap();
    let copied_cff = Cff::new(&copied_font).unwrap();
    let copied_data = copied_cff.variation_store().unwrap();
    let other_instance = copied_font.instance(&location);
    let other = other_instance.cff2_hb_region_scalars(&copied_data);
    assert!(matches!(other, Cow::Owned(_)));
    assert_eq!(same.as_ref(), other.as_ref());
}
