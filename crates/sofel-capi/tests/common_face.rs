#![allow(clippy::unwrap_used)]

use sofel_capi::*;
use sofel_font::FaceData;

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

#[test]
fn adr0100_i6_c_face_owns_input_and_matches_metrics() {
    let mut data = include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf").to_vec();
    let mut count = 0;
    // SAFETY: pointers reference live buffers of the declared lengths.
    let count_status = unsafe { sofel_face_count(data.as_ptr(), data.len(), &mut count) };
    assert_eq!(count_status, 0);
    assert_eq!(count, 1);
    let mut error = 999;
    // SAFETY: pointers reference live buffers of the declared lengths.
    let face = unsafe { sofel_face_create(data.as_ptr(), data.len(), 0, &mut error) };
    assert!(!face.is_null());
    assert_eq!(error, 0);
    data.fill(0);
    drop(data);
    let mut glyph = 0;
    // SAFETY: face remains live and glyph is writable.
    assert_eq!(unsafe { sofel_face_nominal_glyph(face, 65, &mut glyph) }, 0);
    assert_eq!(glyph, 1);
    let mut advance = 0;
    // SAFETY: face remains live and advance is writable.
    let advance_status = unsafe { sofel_face_h_advance(face, glyph, 8, u32::MAX, &mut advance) };
    assert_eq!(advance_status, 0);
    assert_eq!(advance, 512);
    // SAFETY: face was returned by sofel_face_create and is destroyed once.
    unsafe { sofel_face_destroy(face) };
}

#[test]
fn adr0100_i6_c_render_capacity_preserves_pixels() {
    let data = include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf");
    let mut error = 999;
    // SAFETY: pointers reference live buffers of the declared lengths.
    let face = unsafe { sofel_face_create(data.as_ptr(), data.len(), 0, &mut error) };
    let scratch = sofel_outline_scratch_create();
    let raster = sofel_rasterizer_create();
    let mut pixels = [0x55u8; 64];
    let mut len = 0usize;
    let mut box_ = [0i32; 4];
    // SAFETY: all handles and output buffers are live and disjoint.
    let zero_result = unsafe {
        sofel_face_render_glyph(
            face,
            1,
            8,
            0,
            scratch,
            raster,
            pixels.as_mut_ptr(),
            0,
            &mut len,
            box_.as_mut_ptr(),
        )
    };
    assert_eq!(zero_result, ERROR_BUFFER_TOO_SMALL);
    assert_eq!(len, 64);
    assert!(pixels.iter().all(|p| *p == 0x55));
    // SAFETY: all handles and output buffers are live and disjoint.
    let result = unsafe {
        sofel_face_render_glyph(
            face,
            1,
            8,
            0,
            scratch,
            raster,
            pixels.as_mut_ptr(),
            63,
            &mut len,
            box_.as_mut_ptr(),
        )
    };
    assert_eq!(result, ERROR_BUFFER_TOO_SMALL);
    assert_eq!(len, 64);
    assert!(pixels.iter().all(|p| *p == 0x55));
    // SAFETY: all handles and output buffers are live and disjoint.
    let render_status = unsafe {
        sofel_face_render_glyph(
            face,
            1,
            8,
            0,
            scratch,
            raster,
            pixels.as_mut_ptr(),
            64,
            &mut len,
            box_.as_mut_ptr(),
        )
    };
    assert_eq!(render_status, 0);
    assert_eq!(pixels.get(..8), Some(&[0, 0, 0, 255, 255, 0, 0, 0][..]));
    // SAFETY: each handle is destroyed once.
    unsafe {
        sofel_face_destroy(face);
        sofel_outline_scratch_destroy(scratch);
        sofel_rasterizer_destroy(raster);
    }
}

#[test]
fn adr0100_i3_i6_attached_face_survives_source_and_input_release() {
    let source = include_bytes!("../../sofel-shape/tests/support/type1_triangle.pfa");
    let mut error = 999;
    // SAFETY: the source buffer and error output are live.
    let original = unsafe { sofel_face_create(source.as_ptr(), source.len(), 0, &mut error) };
    assert!(!original.is_null());
    let mut afm =
        b"StartFontMetrics 4.1\nStartKernPairs 1\nKPX A A -80\nEndKernPairs\nEndFontMetrics\n"
            .to_vec();
    // SAFETY: original, the metrics buffer, and error output are live.
    let attached =
        unsafe { sofel_face_with_metrics(original, afm.as_ptr(), afm.len(), 0, &mut error) };
    assert!(!attached.is_null());
    assert_eq!(error, 0);
    afm.fill(0);
    drop(afm);
    // SAFETY: original was returned by create and is destroyed once.
    unsafe { sofel_face_destroy(original) };
    let mut x = 0;
    let mut y = 0;
    // SAFETY: attached remains live; x and y are writable.
    let result = unsafe { sofel_face_kerning(attached, 1, 1, 20, u32::MAX, &mut x, &mut y) };
    assert_eq!(result, 0);
    assert_eq!((x, y), (-102, 0));
    // SAFETY: attached was returned by with_metrics and is destroyed once.
    unsafe { sofel_face_destroy(attached) };
}

#[test]
fn adr0100_i4_i6_c_size_and_overflow_errors() {
    let source = include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf");
    let mut error = 0;
    // SAFETY: the input and error output are live.
    let face = unsafe { sofel_face_create(source.as_ptr(), source.len(), 0, &mut error) };
    assert!(!face.is_null());
    let mut advance = 777;
    for (glyph, ppem, strike) in [(1, 0, 0), (2, 8, 0), (1, 8, 1), (1, u16::MAX, 0)] {
        // SAFETY: the handle and output pointer are live for every call.
        let result = unsafe { sofel_face_h_advance(face, glyph, ppem, strike, &mut advance) };
        assert!(result < 0, "{glyph}/{ppem}/{strike}");
        assert_eq!(advance, 777);
    }
    // SAFETY: the handle is destroyed once.
    unsafe { sofel_face_destroy(face) };

    for (upem, ppem) in [(0u16, 20u16), (1u16, u16::MAX)] {
        let mut tables = build::sample_tables_with_glyf();
        let head = tables.iter_mut().find(|(tag, _)| *tag == b"head").unwrap();
        head.1
            .get_mut(18..20)
            .unwrap()
            .copy_from_slice(&upem.to_be_bytes());
        let bytes = build::sfnt(&tables);
        // SAFETY: the input and error output are live.
        let face = unsafe { sofel_face_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
        assert!(!face.is_null());
        let mut advance = 777;
        // SAFETY: the handle and output pointer are live.
        let result = unsafe { sofel_face_h_advance(face, 0, ppem, u32::MAX, &mut advance) };
        assert!(result < 0);
        assert_eq!(advance, 777);
        // SAFETY: the handle is destroyed once.
        unsafe { sofel_face_destroy(face) };
    }
}

#[test]
fn adr0100_i3_i6_pfm_attachment_survives_release() {
    let source = include_bytes!("../../sofel-shape/tests/support/type1_triangle.pfa");
    let mut error = 0;
    // SAFETY: the input and error output are live.
    let original = unsafe { sofel_face_create(source.as_ptr(), source.len(), 0, &mut error) };
    assert!(!original.is_null());
    let mut pfm = vec![0u8; 141];
    *pfm.get_mut(1).unwrap() = 1;
    pfm.get_mut(2..6)
        .unwrap()
        .copy_from_slice(&141u32.to_le_bytes());
    pfm.get_mut(117..119)
        .unwrap()
        .copy_from_slice(&18u16.to_le_bytes());
    pfm.get_mut(131..135)
        .unwrap()
        .copy_from_slice(&135u32.to_le_bytes());
    pfm.get_mut(135..137)
        .unwrap()
        .copy_from_slice(&1u16.to_le_bytes());
    pfm.get_mut(137..141)
        .unwrap()
        .copy_from_slice(&[b'A', b'A', 0xec, 0xff]);
    // SAFETY: original, the PFM bytes, and error output are live.
    let attached =
        unsafe { sofel_face_with_metrics(original, pfm.as_ptr(), pfm.len(), 1, &mut error) };
    assert!(!attached.is_null());
    pfm.fill(0);
    drop(pfm);
    // SAFETY: original was returned by create and is destroyed once.
    unsafe { sofel_face_destroy(original) };
    let (mut x, mut y) = (0, 0);
    // SAFETY: attached remains live, x and y are writable.
    let result = unsafe { sofel_face_kerning(attached, 1, 1, 20, u32::MAX, &mut x, &mut y) };
    assert_eq!(result, 0);
    assert_eq!((x, y), (-26, 0));
    // SAFETY: attached was returned by with_metrics and is destroyed once.
    unsafe { sofel_face_destroy(attached) };
}

#[test]
fn adr0100_a1_real_legacy_c_and_cli_entries() {
    let base = std::env::var_os("SOFEL_TESTDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata")
        });
    for (name, ppem, glyph) in [
        ("cmr10.pfb", 20u16, 1u32),
        ("spleen-8x16.bdf", 16, 0),
        ("spleen-8x16.pcf", 16, 0),
        ("spleen-8x16.fon", 16, 0),
        ("spleen-8x16.dfont", 16, 0),
        ("spleen-8x16.woff", 16, 2),
        ("spleen-8x16.woff2", 16, 2),
    ] {
        let path = base.join("extra-fonts/fonts").join(name);
        if !path.exists() {
            assert!(
                std::env::var_os("SOFEL_REQUIRE_TESTDATA").is_none(),
                "{} missing",
                path.display()
            );
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let data = FaceData::new(&bytes).unwrap();
        let mut count = 0;
        // SAFETY: source bytes and count output are live.
        // SAFETY: source bytes and count output are live.
        let count_status = unsafe { sofel_face_count(bytes.as_ptr(), bytes.len(), &mut count) };
        assert_eq!(count_status, 0, "{name}");
        assert_eq!(count, data.face_count().unwrap(), "{name}");
        let mut error = 0;
        // SAFETY: source bytes and error output are live.
        let face = unsafe { sofel_face_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
        assert!(!face.is_null(), "{name}: {error}");
        let opened = data.open(0).unwrap();
        // SAFETY: face and count output are live.
        let result = unsafe { sofel_face_num_glyphs(face, &mut count) };
        assert_eq!(result, 0, "{name}");
        assert_eq!(count, opened.as_face().glyph_count(), "{name}");
        let mut advance = 0;
        // SAFETY: face and advance output are live.
        let result = unsafe { sofel_face_h_advance(face, glyph, ppem, u32::MAX, &mut advance) };
        assert_eq!(result, 0, "{name}");
        assert_eq!(
            Ok(advance),
            opened
                .as_face()
                .h_advance(sofel_font::read::GlyphId(glyph), ppem, None),
            "{name}"
        );
        let mut output = Vec::new();
        let mut err = Vec::new();
        let args = [
            "--ppem".into(),
            ppem.to_string().into(),
            "--glyph".into(),
            glyph.to_string().into(),
            path.clone().into_os_string(),
        ];
        assert_eq!(
            sofel_cli::render::run(&args, &mut output, &mut err),
            0,
            "{name}: {}",
            String::from_utf8_lossy(&err)
        );
        assert!(output.starts_with(b"P5\n"), "{name}");
        // SAFETY: face was returned by create and is destroyed once.
        unsafe { sofel_face_destroy(face) };
    }
}

#[test]
fn adr0100_i1_i6_c_sfnt_fixed_strike_metrics_and_pixels() {
    let image = build::Buf::new()
        .bytes(&[2, 3, 1, 2, 4])
        .bytes(&[0xe0, 0xa0])
        .done();
    let sizes = [build::SizeSpec {
        ppem_x: 16,
        ppem_y: 16,
        start_glyph: 1,
        end_glyph: 1,
        subtables: vec![(
            1,
            1,
            build::index_offsets(1, 1, 4, &[0, u32::try_from(image.len()).unwrap()]),
        )],
        ..build::SizeSpec::default()
    }];
    let bytes = build::glyph_font_with(
        &[Vec::new(), Vec::new()],
        false,
        false,
        vec![
            (b"EBLC", build::cblc(2, &sizes)),
            (
                b"EBDT",
                build::Buf::new().u16(2).u16(0).bytes(&image).done(),
            ),
        ],
    );
    let mut error = 0;
    // SAFETY: input and error are live and disjoint.
    let face = unsafe { sofel_face_create(bytes.as_ptr(), bytes.len(), 0, &mut error) };
    assert!(!face.is_null());
    let mut count = 0;
    // SAFETY: face and count output are live.
    assert_eq!(unsafe { sofel_face_strike_count(face, &mut count) }, 0);
    assert_eq!(count, 1);
    let (mut x, mut y) = (0, 0);
    // SAFETY: face and output pointers are live.
    assert_eq!(unsafe { sofel_face_strike(face, 0, &mut x, &mut y) }, 0);
    assert_eq!((x, y), (1024, 1024));
    let mut advance = 0;
    // SAFETY: face and output pointer are live.
    let advance_status = unsafe { sofel_face_h_advance(face, 1, 16, 0, &mut advance) };
    assert_eq!(advance_status, 0);
    assert_eq!(advance, 256);
    let scratch = sofel_outline_scratch_create();
    let raster = sofel_rasterizer_create();
    let mut pixels = [0u8; 6];
    let mut len = 0;
    let mut box_ = [0i32; 4];
    // SAFETY: all handles and output pointers are live and disjoint.
    let status = unsafe {
        sofel_face_render_glyph(
            face,
            1,
            16,
            0,
            scratch,
            raster,
            pixels.as_mut_ptr(),
            pixels.len(),
            &mut len,
            box_.as_mut_ptr(),
        )
    };
    assert_eq!(status, 0);
    assert_eq!(len, 6);
    assert_eq!(pixels, [255, 255, 255, 255, 0, 255]);
    // SAFETY: each handle is destroyed once.
    unsafe {
        sofel_face_destroy(face);
        sofel_outline_scratch_destroy(scratch);
        sofel_rasterizer_destroy(raster);
    }
}
