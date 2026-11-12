#![allow(clippy::unwrap_used)]
use sofel_geom::{Command, FnPen, Point};
use sofel_svg::parse_shapes;

fn commands(d: &str, transform: &str) -> Vec<Command> {
    let document = format!(
        r##"<svg><defs><linearGradient id="g"><stop stop-color="red"/></linearGradient></defs><g id="glyph1" transform="{transform}"><path d="{d}" fill="url(#g)"/></g></svg>"##
    );
    let scene = parse_shapes(document.as_bytes(), 1).unwrap();
    let mut out = Vec::new();
    for shape in scene.shapes {
        shape.draw(1.0, &mut FnPen(|cmd| out.push(cmd)));
    }
    out
}

#[test]
fn adr0066_i3_path_repeated_relative_coordinates_and_ordered_transforms() {
    let line = commands("M1,2 3,2 v2 h-2Z", "translate(5 7) scale(2 3)");
    let pt = |x, y| Point { x, y };
    assert_eq!(
        line,
        [
            Command::MoveTo(pt(448, -832)),
            Command::LineTo(pt(704, -832)),
            Command::LineTo(pt(704, -1216)),
            Command::LineTo(pt(448, -1216)),
            Command::LineTo(pt(448, -832)),
            Command::Close
        ]
    );
    assert_ne!(
        line,
        commands("M1,2 3,2 v2 h-2Z", "scale(2 3) translate(5 7)")
    );
}

#[test]
fn adr0066_i3_path_reflects_bezier_controls_and_reads_adjacent_numbers() {
    let compact = commands("M.5-1q.5 2 2 0t2 0c1-2 2-2 3 0s2 2 3 0z", "");
    assert!(!compact.is_empty());
    assert_eq!(
        compact,
        commands(
            "M.5 -1 Q1 1 2.5 -1 Q4 -3 4.5 -1 C5.5 -3 6.5 -3 7.5 -1 C8.5 1 9.5 1 10.5 -1 Z",
            ""
        )
    );
    assert_eq!(
        commands("m1e1 2e0 l1 0z", "rotate(90 10 2)"),
        commands("M10 2 V3 Z", "")
    );
}

#[test]
fn adr0066_i3_rejects_incomplete_paths_and_nonfinite_coordinates() {
    for d in ["", "L1 2", "M1", "M0 0 Q1 1", "M0 0 L1e99 1", "M0 0 Z 5"] {
        assert!(commands(d, "").is_empty(), "{d}");
    }
}

#[test]
fn adr0066_i3_elliptical_arcs_preserve_sweep_endpoints_and_degenerate_cases() {
    let positive = commands("M0 0 A4 4 0 0 1 4 4Z", "");
    let negative = commands("M0 0 A4 4 0 0 0 4 4Z", "");
    assert!(!positive.is_empty());
    assert_ne!(positive, negative);
    assert_eq!(positive, commands("M0 0 a4 4 0 014 4Z", ""));
    let curves = |cmds: &[Command]| {
        cmds.iter()
            .filter(|cmd| matches!(cmd, Command::CubicTo(..)))
            .count()
    };
    assert_eq!(curves(&positive), 1);
    assert_eq!(curves(&commands("M0 0 A4 4 0 1 1 4 4Z", "")), 3);
    assert_eq!(
        commands("M0 0 A0 4 0 0 1 4 4Z", ""),
        commands("M0 0 L4 4Z", "")
    );
    assert_eq!(
        commands("M0 0 A4 4 0 1 1 0 0 L4 4Z", ""),
        commands("M0 0 L4 4Z", "")
    );
}

#[test]
fn adr0066_i3_rotated_arcs_fit_small_radii_and_transform_centers() {
    for flags in ["0 0", "0 1", "1 0", "1 1"] {
        let path = commands(&format!("M0 0 A1 2 37 {flags} 30 40Z"), "");
        let end = Point { x: 1920, y: -2560 };
        assert!(
            path.iter()
                .any(|cmd| matches!(cmd, Command::CubicTo(_,_,p) if *p == end))
        );
    }
    assert_eq!(
        commands("M0 0L0 1Z", "skewX(45)"),
        commands("M0 0L1 1Z", "")
    );
    assert_eq!(
        commands("M0 0L1 0Z", "skewY(45)"),
        commands("M0 0L1 1Z", "")
    );
}
