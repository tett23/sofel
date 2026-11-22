#![allow(clippy::unwrap_used)]

use sofel_capi::*;
use sofel_font::FaceData;
use sofel_glyph_render::render_face;
use sofel_outline::Scratch;
use sofel_raster::Rasterizer;

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;
#[path = "../../sofel-cli/tests/support/pfr.rs"]
#[allow(dead_code)]
mod pfr_fixture;

const BARE_CFF: &[u8] = &[
    1, 0, 4, 4, 0, 1, 1, 1, 2, b'A', 0, 1, 1, 1, 3, 160, 17, 0, 0, 0, 0, 0, 1, 1, 1, 6, 248, 136,
    139, 22, 14,
];
const TEXT_CFF: &[u8] = &[
    1, 0, 4, 4, 0, 1, 1, 1, 2, b'A', 0, 1, 1, 1, 3, 160, 17, 0, 0, 0, 0, 0, 2, 1, 1, 2, 14, 14,
    248, 136, 239, 239, 21, 239, 139, 5, 139, 239, 5, 14,
];

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
fn adr0100_i1_i4_i6_c_fractional_strike_rounding() {
    let source = std::str::from_utf8(include_bytes!("../../sofel-bitmapfont/tests/data/mono.bdf"))
        .unwrap()
        .replace("SIZE 8 72 72", "SIZE 7 82 82");
    let mut error = 0;
    // SAFETY: input and error output are live and disjoint.
    let face = unsafe { sofel_face_create(source.as_ptr(), source.len(), 0, &mut error) };
    assert!(!face.is_null());
    let (mut x, mut y) = (0, 0);
    // SAFETY: face and outputs are live and disjoint.
    let status = unsafe { sofel_face_strike(face, 0, &mut x, &mut y) };
    assert_eq!(status, 0);
    assert_eq!((x, y), (510, 510));
    let mut advance = 0;
    // SAFETY: face and advance output are live.
    let status = unsafe { sofel_face_h_advance(face, 1, 8, 0, &mut advance) };
    assert_eq!(status, 0);
    assert_eq!(advance, 512);
    // SAFETY: face and advance output are live.
    let status = unsafe { sofel_face_h_advance(face, 1, 7, 0, &mut advance) };
    assert!(status < 0);
    // SAFETY: face is destroyed once.
    unsafe { sofel_face_destroy(face) };
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

    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let wrapper = format!(
        "%!PS-TrueTypeFont-1.0\n/FontType 42 def\n/Encoding StandardEncoding def\n/CharStrings 2 dict dup begin\n/.notdef 0 def\n/A 1 def\nend readonly def\n/sfnts [<{hex}>] def\n"
    );
    // SAFETY: wrapper bytes and error output are live and disjoint.
    let face = unsafe { sofel_face_create(wrapper.as_ptr(), wrapper.len(), 0, &mut error) };
    assert!(!face.is_null());
    // SAFETY: face and output pointer are live.
    let advance_status = unsafe { sofel_face_h_advance(face, 1, 16, 0, &mut advance) };
    assert_eq!(advance_status, 0);
    assert_eq!(advance, 256);
    let scratch = sofel_outline_scratch_create();
    let raster = sofel_rasterizer_create();
    pixels.fill(0);
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
    assert_eq!(pixels, [255, 255, 255, 255, 0, 255]);
    // SAFETY: each handle is destroyed once.
    unsafe {
        sofel_face_destroy(face);
        sofel_outline_scratch_destroy(scratch);
        sofel_rasterizer_destroy(raster);
    }
}

#[test]
fn adr0100_a1_synthetic_missing_formats_all_glyphs_match_c_and_cli() {
    let mut cid = b"%!PS-Adobe-3.0 Resource-CIDFont\n\
/CIDFontName /TestCID def\n/CIDFontType 0 def\n\
/Registry (Adobe) def\n/Ordering (Identity) def\n/Supplement 0 def\n\
/CIDCount 1 def\n/FDBytes 1 def\n/GDBytes 2 def\n/CIDMapOffset 0 def\n\
/FDArray 1 array\n%ADOBeginFontDict\n\
dup 0 10 dict begin\n/FontName /TestCID def\n/FontType 1 def\n\
/FontMatrix [0.001 0 0 0.001 0 0] def\n\
/Private 1 dict begin /lenIV -1 def end\n\
end put\n%ADOEndFontDict\ndef\n(Binary) 24 StartData\n"
        .to_vec();
    cid.extend_from_slice(&[
        0, 0, 6, 0, 0, 24, 139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9,
        14,
    ]);
    let mut cef =
        b"OTTO\x00\x02\x00\x20\x00\x01\x00\x00CFF \x00\x00\x00\x00\x00\x00\x00\x2c".to_vec();
    cef.extend_from_slice(&u32::try_from(TEXT_CFF.len()).unwrap().to_be_bytes());
    cef.extend_from_slice(b"cmap\x00\x00\x00\x00\x00\x00\x00\x54\x00\x00\x00\x18");
    cef.extend_from_slice(TEXT_CFF);
    cef.resize(84, 0);
    cef.extend_from_slice(&[
        0, 0, 0, 1, 0, 3, 0, 1, 0, 0, 0, 12, 0, 6, 0, 12, 0, 0, 0, 65, 0, 1, 0, 1,
    ]);
    let samples = [
        ("CFF", BARE_CFF.to_vec()),
        ("CEF", cef),
        ("CID", cid),
        ("PFR", pfr_fixture::triangle()),
        (
            "FNT",
            include_bytes!("../../sofel-bitmapfont/tests/data/mono.fnt").to_vec(),
        ),
        (
            "FON",
            include_bytes!("../../sofel-bitmapfont/tests/data/two.fon").to_vec(),
        ),
    ];
    let mut checked = 0;
    for (name, bytes) in samples {
        let path = std::env::temp_dir().join(format!(
            "sofel-common-face-{}-{}.font",
            std::process::id(),
            name.to_ascii_lowercase()
        ));
        std::fs::write(&path, &bytes).unwrap();
        let data = FaceData::new(&bytes).unwrap();
        let mut count = 0;
        // SAFETY: input and count output are live and disjoint.
        let status = unsafe { sofel_face_count(bytes.as_ptr(), bytes.len(), &mut count) };
        assert_eq!(status, 0, "{name}");
        assert_eq!(count, data.face_count().unwrap(), "{name}");
        for index in 0..count {
            let opened = data.open(index).unwrap();
            let mut error = 0;
            // SAFETY: input and error output are live and disjoint.
            let handle =
                unsafe { sofel_face_create(bytes.as_ptr(), bytes.len(), index, &mut error) };
            assert!(!handle.is_null(), "{name}/{index}: {error}");
            let mut glyph_count = 0;
            // SAFETY: handle and count output are live.
            let status = unsafe { sofel_face_num_glyphs(handle, &mut glyph_count) };
            assert_eq!(status, 0, "{name}/{index}");
            assert_eq!(
                glyph_count,
                opened.as_face().glyph_count(),
                "{name}/{index}"
            );
            let face = opened.as_face();
            let ppem = face
                .strike(0)
                .map_or(20u16, |(x, _)| u16::try_from((x + 32) / 64).unwrap());
            let expected = face.h_advance(sofel_font::read::GlyphId(0), ppem, None);
            let mut advance = 0;
            // SAFETY: handle and advance output are live.
            let status = unsafe { sofel_face_h_advance(handle, 0, ppem, u32::MAX, &mut advance) };
            assert_eq!(status == 0, expected.is_ok(), "{name}/{index}");
            if let Ok(expected) = expected {
                assert_eq!(advance, expected, "{name}/{index}");
            }
            let c_scratch = sofel_outline_scratch_create();
            let c_raster = sofel_rasterizer_create();
            let mut rust_scratch = Scratch::default();
            let mut rust_raster = Rasterizer::new();
            for glyph in 0..glyph_count {
                checked += 1;
                let expected = render_face(
                    face,
                    sofel_font::read::GlyphId(glyph),
                    ppem,
                    None,
                    &mut rust_scratch,
                    &mut rust_raster,
                );
                let mut pixels = vec![
                    0x55;
                    expected
                        .as_ref()
                        .map_or(1, |image| image.pixels.len().max(1))
                ];
                let mut len = 0;
                let mut box_ = [0; 4];
                // SAFETY: handles and output buffers are live and disjoint.
                let status = unsafe {
                    sofel_face_render_glyph(
                        handle,
                        glyph,
                        ppem,
                        u32::MAX,
                        c_scratch,
                        c_raster,
                        pixels.as_mut_ptr(),
                        pixels.len(),
                        &mut len,
                        box_.as_mut_ptr(),
                    )
                };
                assert_eq!(status == 0, expected.is_ok(), "{name}/{index}/{glyph}");
                if let Ok(image) = expected.as_ref() {
                    assert_eq!(len, image.pixels.len(), "{name}/{index}/{glyph}");
                    assert_eq!(
                        pixels.get(..len),
                        Some(image.pixels.as_slice()),
                        "{name}/{index}/{glyph}"
                    );
                    assert_eq!(
                        box_,
                        [
                            image.left,
                            image.top,
                            i32::try_from(image.width).unwrap(),
                            i32::try_from(image.rows).unwrap()
                        ],
                        "{name}/{index}/{glyph}"
                    );
                }
                let mut output = Vec::new();
                let mut stderr = Vec::new();
                let args = [
                    "--ppem".into(),
                    ppem.to_string().into(),
                    "--face".into(),
                    index.to_string().into(),
                    "--glyph".into(),
                    glyph.to_string().into(),
                    path.clone().into_os_string(),
                ];
                let cli_status = sofel_cli::render::run(&args, &mut output, &mut stderr);
                let cli_image = expected
                    .as_ref()
                    .is_ok_and(|image| image.width > 0 && image.rows > 0);
                assert_eq!(
                    cli_status == 0,
                    cli_image,
                    "{name}/{index}/{glyph}: {}",
                    String::from_utf8_lossy(&stderr)
                );
                if cli_image {
                    let image = expected.as_ref().unwrap();
                    let marker = b"\n255\n";
                    let pixel_at = output
                        .windows(marker.len())
                        .position(|w| w == marker)
                        .unwrap()
                        + marker.len();
                    let expected_pixels =
                        image.pixels.iter().map(|value| !value).collect::<Vec<_>>();
                    assert_eq!(
                        output.get(pixel_at..),
                        Some(expected_pixels.as_slice()),
                        "{name}/{index}/{glyph} CLI pixels"
                    );
                }
            }
            // SAFETY: handle is destroyed once.
            unsafe {
                sofel_face_destroy(handle);
                sofel_outline_scratch_destroy(c_scratch);
                sofel_rasterizer_destroy(c_raster);
            }
        }
        std::fs::remove_file(path).unwrap();
    }
    assert_eq!(checked, 12, "synthetic formats must contain glyphs");
}

#[test]
fn adr0100_i6_a1_real_all_glyphs_match_c_and_cli_results() {
    let base = std::env::var_os("SOFEL_TESTDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata")
        });
    let mut checked = 0;
    for (name, ppem) in [
        ("cmr10.pfb", 20u16),
        ("spleen-8x16.bdf", 16),
        ("spleen-8x16.pcf", 16),
        ("spleen-8x16.fon", 16),
        ("spleen-8x16.dfont", 16),
        ("spleen-8x16.woff", 16),
        ("spleen-8x16.woff2", 16),
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
        for index in 0..data.face_count().unwrap() {
            let opened = data.open(index).unwrap();
            let face = opened.as_face();
            let mut error = 0;
            // SAFETY: source bytes and error output are live.
            let handle =
                unsafe { sofel_face_create(bytes.as_ptr(), bytes.len(), index, &mut error) };
            assert!(!handle.is_null(), "{name}/{index}: {error}");
            let c_scratch = sofel_outline_scratch_create();
            let c_raster = sofel_rasterizer_create();
            let mut rust_scratch = Scratch::default();
            let mut rust_raster = Rasterizer::new();
            for glyph in 0..face.glyph_count() {
                checked += 1;
                let id = sofel_font::read::GlyphId(glyph);
                let rust_advance = face.h_advance(id, ppem, None);
                let mut c_advance = 0;
                // SAFETY: handle and output are live.
                let status =
                    unsafe { sofel_face_h_advance(handle, glyph, ppem, u32::MAX, &mut c_advance) };
                assert_eq!(
                    status == 0,
                    rust_advance.is_ok(),
                    "{name}/{index}/{glyph} advance"
                );
                if let Ok(advance) = rust_advance {
                    assert_eq!(c_advance, advance, "{name}/{index}/{glyph} advance");
                }
                let rust_image =
                    render_face(face, id, ppem, None, &mut rust_scratch, &mut rust_raster);
                let mut pixels = vec![
                    0x55u8;
                    rust_image
                        .as_ref()
                        .map_or(1, |image| image.pixels.len().max(1))
                ];
                let mut len = 0;
                let mut box_ = [0i32; 4];
                // SAFETY: handles and output buffers are live and disjoint.
                let status = unsafe {
                    sofel_face_render_glyph(
                        handle,
                        glyph,
                        ppem,
                        u32::MAX,
                        c_scratch,
                        c_raster,
                        pixels.as_mut_ptr(),
                        pixels.len(),
                        &mut len,
                        box_.as_mut_ptr(),
                    )
                };
                assert_eq!(
                    status == 0,
                    rust_image.is_ok(),
                    "{name}/{index}/{glyph} image"
                );
                let mut output = Vec::new();
                let mut stderr = Vec::new();
                let args = [
                    "--ppem".into(),
                    ppem.to_string().into(),
                    "--face".into(),
                    index.to_string().into(),
                    "--glyph".into(),
                    glyph.to_string().into(),
                    path.clone().into_os_string(),
                ];
                let cli_status = sofel_cli::render::run(&args, &mut output, &mut stderr);
                if let Ok(image) = rust_image {
                    assert_eq!(len, image.pixels.len(), "{name}/{index}/{glyph} len");
                    assert_eq!(
                        pixels.get(..len),
                        Some(image.pixels.as_slice()),
                        "{name}/{index}/{glyph} pixels"
                    );
                    assert_eq!(
                        box_,
                        [
                            image.left,
                            image.top,
                            i32::try_from(image.width).unwrap(),
                            i32::try_from(image.rows).unwrap()
                        ],
                        "{name}/{index}/{glyph} box"
                    );
                    if image.width == 0 || image.rows == 0 {
                        assert_ne!(cli_status, 0, "{name}/{index}/{glyph} empty CLI image");
                    } else {
                        assert_eq!(
                            cli_status,
                            0,
                            "{name}/{index}/{glyph}: {}",
                            String::from_utf8_lossy(&stderr)
                        );
                        let marker = b"\n255\n";
                        let pixel_at = output
                            .windows(marker.len())
                            .position(|w| w == marker)
                            .unwrap()
                            + marker.len();
                        let expected = image.pixels.iter().map(|value| !value).collect::<Vec<_>>();
                        assert_eq!(
                            output.get(pixel_at..),
                            Some(expected.as_slice()),
                            "{name}/{index}/{glyph} CLI pixels"
                        );
                    }
                } else {
                    assert_ne!(cli_status, 0, "{name}/{index}/{glyph} CLI error");
                }
            }
            // SAFETY: every handle is destroyed once.
            unsafe {
                sofel_face_destroy(handle);
                sofel_outline_scratch_destroy(c_scratch);
                sofel_rasterizer_destroy(c_raster);
            }
        }
    }
    assert_eq!(checked, 5454);
}
