use font_gallery::{inputs, pixels, sheet};

#[test]
fn i1_rust_gallery_parity_pixels() {
    let pgm = b"P5\n# left -2 top 3\n2 1\n255\n\x00\x80";
    let image = pixels::parse(pgm).unwrap();
    assert_eq!(
        (image.left, image.top, image.width, image.height),
        (-2, 3, 2, 1)
    );
    assert_eq!(image.pixels, [0, 0, 0, 255, 128, 128, 128, 255]);
    let card = sheet::card(Some(&image), None, &[]).unwrap();
    assert_eq!((card.width, card.height), (490, 200));
}

#[test]
fn i1_rust_gallery_parity_faces() {
    let root = std::env::temp_dir().join(format!("sofel-gallery-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("fonts")).unwrap();
    std::fs::write(
        root.join("fonts/one.ttf"),
        [b"OTTO".as_slice(), &[0; 8]].concat(),
    )
    .unwrap();
    std::fs::write(
        root.join("fonts/many.ttc"),
        [b"ttcf\0\x01\0\0".as_slice(), &[0, 0, 0, 2]].concat(),
    )
    .unwrap();
    std::fs::write(
        root.join("fonts.tsv"),
        "source-serif\tsource-serif\t-\tfonts/one.ttf\nsource-han\tsource-han\t-\tfonts/many.ttc\n",
    )
    .unwrap();
    let cases = inputs::cases(&root, &root.join("fonts.tsv"), &root, false).unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|c| (c.id.as_str(), c.face))
            .collect::<Vec<_>>(),
        [("source-serif", 0), ("source-han", 0), ("source-han", 1)]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn i1_rust_gallery_parity_composition_and_sheet() {
    let gray = pixels::parse(b"P5\n# left 0 top 1\n2 1\n255\n\x00\xff").unwrap();
    let ink = pixels::compose(&[
        pixels::Glyph {
            image: Some(gray.clone()),
            dx: 0,
            dy: 0,
            ax: 0,
            ay: 0,
        },
        pixels::Glyph {
            image: Some(gray),
            dx: 64,
            dy: 0,
            ax: 0,
            ay: 0,
        },
    ])
    .unwrap()
    .unwrap();
    assert_eq!((ink.width, ink.height), (3, 1));
    assert_eq!(ink.pixels, [0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255]);
    let card = sheet::card(Some(&ink), None, &[]).unwrap();
    let full = sheet::layout(&[card.clone(), card]).unwrap();
    assert_eq!((full.width, full.height), (980, 200));
    assert_eq!(full.pixels.len(), 980 * 200 * 4);
    assert_eq!(&pixels::png(&full)[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn i1_rust_gallery_parity_alpha_and_exact_copy() {
    let rgba = pixels::parse(b"P7\n# left 1 top 4\nWIDTH 1\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n\xf0\x0c\x50\x80").unwrap();
    assert_eq!((rgba.left, rgba.top, rgba.width, rgba.height), (1, 4, 1, 1));
    let mut canvas = pixels::blank(2, 1).unwrap();
    pixels::copy(&mut canvas, &rgba, 1, 0).unwrap();
    assert_eq!(&canvas.pixels[4..8], &[240, 12, 80, 128]);
    assert!(pixels::parse(b"P5\n1 1\n255\n").is_err());
}

#[test]
fn i2_rust_gallery_publish_rejects_bad_inputs() {
    let root = std::env::temp_dir().join(format!("sofel-gallery-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let manifest = root.join("fonts.tsv");
    assert!(inputs::cases(&root, &manifest, &root, false).is_err());
    std::fs::write(&manifest, "# empty\n").unwrap();
    assert_eq!(
        inputs::cases(&root, &manifest, &root, false).unwrap_err(),
        "empty font manifest"
    );
    std::fs::remove_dir_all(root).unwrap();
}
