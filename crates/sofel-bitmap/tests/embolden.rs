//! sofel ADR 0063 I2: FT_Bitmap_Embolden from pinned FreeType 2.15.0.

use sofel_bitmap::{BitmapView, ConvertError, PixelMode, embolden_bitmap};

#[test]
#[allow(clippy::type_complexity)] // A fixed tuple records each FreeType input and exact output.
fn adr0063_i2_bitmap_embolden_matches_freetype() -> Result<(), ConvertError> {
    let cases: &[(
        PixelMode,
        u32,
        u32,
        i32,
        &[u8],
        i32,
        i32,
        u32,
        u32,
        i32,
        &[u8],
    )] = &[
        (
            PixelMode::Gray,
            3,
            2,
            3,
            &[10, 20, 30, 40, 50, 60],
            64,
            64,
            4,
            3,
            4,
            &[
                0x0a, 0x1e, 0x32, 0x1e, 0x2a, 0x5e, 0x7e, 0x3e, 0x28, 0x5a, 0x6e, 0x3c,
            ],
        ),
        (
            PixelMode::Mono,
            8,
            1,
            1,
            &[0x81],
            64,
            0,
            9,
            1,
            2,
            &[0xc1, 0x80],
        ),
        (
            PixelMode::Gray2,
            4,
            1,
            1,
            &[0x1b],
            64,
            0,
            5,
            1,
            5,
            &[0, 1, 3, 3, 3],
        ),
        (
            PixelMode::Gray4,
            2,
            1,
            1,
            &[0x1f],
            64,
            0,
            3,
            1,
            3,
            &[1, 15, 15],
        ),
        (
            PixelMode::Bgra,
            1,
            1,
            4,
            &[0, 0, 255, 255],
            64,
            64,
            1,
            1,
            4,
            &[0, 0, 255, 255],
        ),
    ];
    for &(
        pixel_mode,
        width,
        rows,
        pitch,
        buffer,
        x,
        y,
        want_width,
        want_rows,
        want_pitch,
        want_bytes,
    ) in cases
    {
        let got = embolden_bitmap(
            BitmapView {
                width,
                rows,
                pitch,
                pixel_mode,
                buffer,
            },
            x,
            y,
        )?;
        assert_eq!(
            (got.width, got.rows, got.pitch),
            (want_width, want_rows, want_pitch)
        );
        assert_eq!(got.buffer, want_bytes);
    }
    Ok(())
}
