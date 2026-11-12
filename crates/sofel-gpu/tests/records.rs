use sofel_gpu::Outline;

#[test]
fn i2_gpu_records_match_harfbuzz_for_empty_and_touching_contours()
-> Result<(), sofel_gpu::OutOfRange> {
    let mut expected = include_bytes!("data/outline-records.bin").as_slice();
    for case in 0..5 {
        let mut outline = Outline::default();
        if case == 1 || case == 2 {
            outline.move_to([0., 0.]);
            outline.line_to([1., 0.]);
            outline.line_to([0., 1.]);
            outline.close();
        }
        if case == 2 {
            outline.move_to([0., 0.]);
            outline.quad_to([-0.75, 0.25], [-0.25, -1.]);
            outline.close();
        }
        if case == 3 {
            outline.move_to([0.24, 0.24]);
            outline.cubic_to([35., 79.], [60., -21.], [94.8, 1.7]);
            outline.close();
        }
        if case == 4 {
            for _ in 0..34 {
                outline.move_to([0., 0.]);
                outline.line_to([16., 16.]);
                outline.close();
            }
        }
        let records = outline.encode([0, 0])?;
        let (count, rest) = expected
            .split_first_chunk::<4>()
            .ok_or(sofel_gpu::OutOfRange)?;
        expected = rest;
        assert_eq!(
            records.len(),
            u32::from_le_bytes(*count) as usize,
            "case {case}"
        );
        for (index, record) in records.into_iter().enumerate() {
            let (bytes, rest) = expected
                .split_first_chunk::<8>()
                .ok_or(sofel_gpu::OutOfRange)?;
            expected = rest;
            let reference = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .copied()
                .map(i16::from_le_bytes);
            assert!(
                record.into_iter().eq(reference),
                "case {case}, first different record {index}: {record:?}, expected bytes {bytes:?}"
            );
        }
    }
    assert!(expected.is_empty());
    Ok(())
}

#[test]
fn i2_gpu_records_reject_unrepresentable_coordinates_and_offsets() {
    for coordinate in [-8192.25, 8192., f32::NAN] {
        let mut outline = Outline::default();
        outline.move_to([0., 0.]);
        outline.line_to([coordinate, 1.]);
        assert_eq!(outline.encode([0, 0]), Err(sofel_gpu::OutOfRange));
    }
    let mut outline = Outline::default();
    for _ in 0..600 {
        outline.move_to([0., 0.]);
        outline.line_to([10., 10.]);
        outline.close();
    }
    assert_eq!(outline.encode([0, 0]), Err(sofel_gpu::OutOfRange));
}
