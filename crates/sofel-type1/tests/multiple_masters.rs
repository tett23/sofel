use sofel_type1::multiple_masters::MultipleMasters;
use sofel_type1::{face::Type1Face, source::Type1Source};

const DISTORTABLE_HEADER: &[u8] = b"%!PS-AdobeFont-1.0\n\
/FontInfo 12 dict dup begin\n\
/BlendDesignPositions [ [0] [1] ] def\n\
/BlendDesignMap [ [[50 0] [999 1]] ] def\n\
/BlendAxisTypes [/Weight] def\n\
end def\n\
/WeightVector [0.631191 0.368809] def\n";

#[test]
fn adr0065_i1_multiple_masters_matches_freetype_2150_axis_and_default() {
    let mm = MultipleMasters::parse(DISTORTABLE_HEADER).unwrap().unwrap();
    assert_eq!(mm.axis_count(), 1);
    assert_eq!(mm.axis(0).unwrap().name(), b"Weight");
    assert_eq!(mm.axis(0).unwrap().minimum(), 50 << 16);
    assert_eq!(mm.axis(0).unwrap().maximum(), 999 << 16);
    assert!(
        (mm.axis(0).unwrap().default() - (400 << 16)).abs() <= 2,
        "{}",
        mm.axis(0).unwrap().default()
    );
    assert_eq!(mm.blend_coordinates(&[50 << 16]).unwrap(), vec![0]);
    assert_eq!(mm.blend_coordinates(&[999 << 16]).unwrap(), vec![1 << 16]);
    assert_eq!(mm.blend_coordinates(&[400 << 16]).unwrap(), vec![24170]);
}

#[test]
fn adr0065_i2_multiple_masters_rejects_missing_or_mismatched_tables() {
    assert!(MultipleMasters::parse(b"/BlendAxisTypes [/Weight] def").is_err());
    assert!(MultipleMasters::parse(b"/BlendAxisTypes [/Weight /Width] def\n/BlendDesignPositions [[0] [1]] def\n/BlendDesignMap [[[0 0] [1 1]]] def\n/WeightVector [0.5 0.5] def").is_err());
    assert!(
        MultipleMasters::parse(b"/FontName /Plain def")
            .unwrap()
            .is_none()
    );
}

#[test]
fn adr0065_i1_multiple_masters_two_axes_default_and_boundaries() {
    let header = b"/BlendAxisTypes [/Weight /Width] def\n\
/BlendDesignPositions [[0 0] [1 0] [0 1] [1 1]] def\n\
/BlendDesignMap [[[100 0] [900 1]] [[50 0] [200 1]]] def\n\
/WeightVector [0.25 0.25 0.25 0.25] def\n";
    let mm = MultipleMasters::parse(header).unwrap().unwrap();
    assert_eq!(mm.axis_count(), 2);
    assert_eq!(mm.axis(0).unwrap().default(), 500 << 16);
    assert_eq!(mm.axis(1).unwrap().default(), 125 << 16);
    assert_eq!(
        mm.blend_coordinates(&[100 << 16, 200 << 16]).unwrap(),
        vec![0, 1 << 16]
    );
    assert_eq!(
        mm.blend_coordinates(&[900 << 16, 50 << 16]).unwrap(),
        vec![1 << 16, 0]
    );
    assert_eq!(
        mm.master_weights(&[100 << 16, 50 << 16]).unwrap(),
        vec![1 << 16, 0, 0, 0]
    );
    assert_eq!(
        mm.master_weights(&[900 << 16, 200 << 16]).unwrap(),
        vec![0, 0, 0, 1 << 16]
    );
    assert_eq!(
        mm.master_weights(&[500 << 16, 125 << 16]).unwrap(),
        vec![1 << 14; 4]
    );
}

#[test]
fn adr0065_i1_type1_face_exposes_multiple_masters_axes() -> Result<(), Box<dyn std::error::Error>> {
    let mut pfa = b"%!PS-AdobeFont-1.0\n/Encoding 256 array\ndup 65/A put\nreadonly def\n".to_vec();
    pfa.extend_from_slice(DISTORTABLE_HEADER);
    pfa.extend_from_slice(b"currentfile eexec\nb8588da5424438407bb191bb43917fe221abd99deb2b4445bd5dc6bc878aed52c60104d4fb73902409cd1f6e6f4d242ad4e3209df6508dce5468e50a62bb81ca1078b9f3529710710c06cdde6e7146c338ce\ncleartomark\n");
    let source = Type1Source::parse_pfa(&pfa, 4096)?;
    let face = Type1Face::new(&source)?;
    assert_eq!(
        face.multiple_masters().unwrap().axis(0).unwrap().default(),
        400 << 16
    );
    Ok(())
}
