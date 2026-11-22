use sofel_outline::{HintProperties, PostScriptHintProperties};

#[test]
fn adr0086_i4_invalid_darkening_parameters_are_rejected() {
    let defaults = HintProperties::default();
    assert!(defaults.validate().is_ok());
    for parameters in [
        [-1, 0, 0, 0, 0, 0, 0, 0],
        [0, 501, 0, 0, 0, 0, 0, 0],
        [2, 0, 1, 0, 2, 0, 3, 0],
    ] {
        let properties = HintProperties {
            autofitter: PostScriptHintProperties {
                darkening_parameters: parameters,
                ..defaults.autofitter
            },
            ..defaults
        };
        assert!(properties.validate().is_err());
    }
    let equal_x = PostScriptHintProperties {
        darkening_parameters: [0, 500, 0, 500, 0, 500, 0, 500],
        ..defaults.autofitter
    };
    assert!(
        HintProperties {
            autofitter: equal_x,
            ..defaults
        }
        .validate()
        .is_ok()
    );
}
