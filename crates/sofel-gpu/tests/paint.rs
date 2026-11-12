use sofel_geom::{Brush, Color, ColorRef, CompositeMode, Painter, Solid};
use sofel_gpu::{OutOfRange, Outline, Paint};

#[test]
fn i2_gpu_records_match_clip_depth_boundaries_and_groups() -> Result<(), OutOfRange> {
    let mut expected = include_bytes!("data/paint-records.bin").as_slice();
    for case in 0..=6 {
        let mut paint = Paint::new([1000, 1000], |_, _| Ok(Outline::default()));
        for _ in 0..if case >= 5 { 1 } else { case } {
            let mut outline = Outline::default();
            if case != 5 {
                outline.move_to([0., 0.]);
                outline.line_to([1., 0.]);
                outline.line_to([0., 1.]);
                outline.close();
            }
            paint.push_clip_path(&outline);
        }
        paint.push_group(CompositeMode::Multiply);
        for (color, index) in [
            (Color::new(234, 73, 11, 128), Some(0)),
            (Color::new(0, 0, 0, 255), None),
        ] {
            paint.fill(&Brush::Solid(Solid {
                color,
                source: ColorRef { index, alpha: 1. },
            }));
        }
        if case == 6 {
            use sofel_geom::{ColorLine, ColorStop, Extend, PointF, Transform};
            let red = Solid {
                color: Color::new(234, 73, 11, 128),
                source: ColorRef {
                    index: Some(0),
                    alpha: 1.,
                },
            };
            let black = Solid {
                color: Color::new(0, 0, 0, 255),
                source: ColorRef {
                    index: None,
                    alpha: 1.,
                },
            };
            let stops = [
                ColorStop {
                    offset: 0.25,
                    color: red,
                },
                ColorStop {
                    offset: 0.75,
                    color: black,
                },
            ];
            let line = ColorLine {
                stops: &stops,
                extend: Extend::Repeat,
            };
            paint.push_transform(Transform {
                xx: 2.,
                dx: 8.,
                dy: 9.,
                ..Transform::IDENTITY
            });
            paint.fill(&Brush::Linear {
                line,
                p0: PointF::new(0., 2.),
                p1: PointF::new(20., 4.),
                p2: PointF::new(0., 12.),
            });
            paint.fill(&Brush::Radial {
                line,
                c0: PointF::new(4., 5.),
                r0: 2.,
                c1: PointF::new(12., 9.),
                r1: 6.,
            });
            paint.fill(&Brush::Sweep {
                line,
                center: PointF::new(4., 5.),
                start: -1.,
                end: 2.,
            });
            paint.pop_transform();
        }
        paint.pop_group(CompositeMode::Multiply);
        let (count, rest) = expected.split_first_chunk::<4>().ok_or(OutOfRange)?;
        expected = rest;
        let count = u32::from_le_bytes(*count);
        if count == u32::MAX {
            assert_eq!(paint.encode(), Err(OutOfRange));
            continue;
        }
        let actual = paint.encode()?;
        assert_eq!(actual.len(), count as usize, "case {case}");
        for (i, texel) in actual.into_iter().enumerate() {
            let (bytes, rest) = expected.split_first_chunk::<8>().ok_or(OutOfRange)?;
            expected = rest;
            assert!(
                texel.into_iter().eq(bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .copied()
                    .map(i16::from_le_bytes)),
                "case {case} record {i} {texel:?} vs {bytes:?}"
            );
        }
    }
    assert!(expected.is_empty());
    Ok(())
}
