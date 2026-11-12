//! sofel ADR 0063 I2: FT_Bitmap_Convert on the pinned FreeType 2.15.0.

use sofel_bitmap::{BitmapView, ConvertError, PixelMode, convert_bitmap};

#[test]
fn adr0063_i2_bitmap_convert_unpacks_mono_gray2_gray4() -> Result<(), ConvertError> {
    for (mode, width, source, grays, expected) in [
        (
            PixelMode::Mono,
            9,
            &[0xa5, 0x80][..],
            2,
            &[1, 0, 1, 0, 0, 1, 0, 1, 1][..],
        ),
        (PixelMode::Gray2, 5, &[0x1b, 0xc0], 4, &[0, 1, 2, 3, 3]),
        (PixelMode::Gray4, 3, &[0x1f, 0xa0], 16, &[1, 15, 10]),
    ] {
        let bitmap = convert_bitmap(
            BitmapView {
                width,
                rows: 1,
                pitch: 2,
                pixel_mode: mode,
                buffer: source,
            },
            1,
            0,
        )?;
        assert_eq!(
            (bitmap.width, bitmap.rows, bitmap.pitch, bitmap.num_grays),
            (width, 1, width.cast_signed(), grays)
        );
        assert_eq!(bitmap.buffer, expected);
    }
    Ok(())
}

#[test]
fn adr0063_i2_bitmap_convert_aligns_gray_and_converts_bgra() -> Result<(), ConvertError> {
    let gray = convert_bitmap(
        BitmapView {
            width: 3,
            rows: 1,
            pitch: 3,
            pixel_mode: PixelMode::Gray,
            buffer: &[4, 5, 6],
        },
        4,
        0,
    )?;
    assert_eq!(
        (gray.pitch, gray.num_grays, gray.buffer.as_slice()),
        (4, 256, &[4, 5, 6, 0][..])
    );

    let bgra = convert_bitmap(
        BitmapView {
            width: 2,
            rows: 1,
            pitch: 8,
            pixel_mode: PixelMode::Bgra,
            buffer: &[0, 0, 255, 255, 0, 0, 0, 0],
        },
        1,
        0,
    )?;
    assert_eq!(
        (bgra.pitch, bgra.num_grays, bgra.buffer.as_slice()),
        (2, 256, &[201, 0][..])
    );
    Ok(())
}

#[test]
fn adr0063_i2_bitmap_convert_preserves_negative_flow() -> Result<(), ConvertError> {
    let converted = convert_bitmap(
        BitmapView {
            width: 3,
            rows: 2,
            pitch: -3,
            pixel_mode: PixelMode::Gray,
            buffer: &[4, 5, 6, 1, 2, 3],
        },
        1,
        0,
    )?;
    assert_eq!(
        (converted.pitch, converted.buffer.as_slice()),
        (-3, &[4, 5, 6, 1, 2, 3][..])
    );
    Ok(())
}
