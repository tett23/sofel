//! Slug outline records for C callers (ADR 0068).
use crate::{ERROR_INVALID_ARGUMENT, copy_out, out_array, some, status, variation::input_array, w};
use sofel_gpu::Outline;

#[repr(C)]
pub struct CPathCommand {
    kind: u32,
    coordinates: [f32; 6],
}

/// Encodes a stream of path commands into native RGBA16I records.
///
/// # Safety
/// The input covers `count` initialized commands. The output covers `capacity`
/// records and `len` is writable. Mutable arguments do not alias other arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_gpu_outline(
    commands: *const CPathCommand,
    count: usize,
    x_scale: i32,
    y_scale: i32,
    records: *mut [i16; 4],
    capacity: usize,
    len: *mut usize,
) -> i32 {
    // SAFETY: caller supplies the ranges described above.
    let (commands, records, len) = unsafe {
        (
            input_array(commands, count),
            out_array(records, capacity),
            w(len),
        )
    };
    status((|| {
        let mut outline = Outline::default();
        for cmd in some(commands)? {
            let [x, y, x2, y2, x3, y3] = cmd.coordinates;
            match cmd.kind {
                0 => outline.move_to([x, y]),
                1 => outline.line_to([x, y]),
                2 => outline.quad_to([x, y], [x2, y2]),
                3 => outline.cubic_to([x, y], [x2, y2], [x3, y3]),
                4 => outline.close(),
                _ => return Err(ERROR_INVALID_ARGUMENT),
            }
        }
        let data = outline
            .encode([x_scale, y_scale])
            .map_err(|_| ERROR_INVALID_ARGUMENT)?;
        copy_out(&data, records, len)
    })())
}

/// Copies the requested UTF-8 shader source without a trailing NUL.
///
/// # Safety
/// The output covers `capacity` bytes. `len` is writable and does not alias output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_gpu_shader_source(
    language: u32,
    stage: u32,
    output: *mut u8,
    capacity: usize,
    len: *mut usize,
) -> i32 {
    use sofel_gpu::{ShaderLanguage, ShaderStage, shader_source};
    let language = match language {
        0 => ShaderLanguage::Wgsl,
        1 => ShaderLanguage::Glsl,
        2 => ShaderLanguage::Msl,
        3 => ShaderLanguage::Hlsl,
        _ => return ERROR_INVALID_ARGUMENT,
    };
    let stage = match stage {
        0 => ShaderStage::Vertex,
        1 => ShaderStage::Fragment,
        _ => return ERROR_INVALID_ARGUMENT,
    };
    // SAFETY: caller supplies a writable buffer and a separate length.
    let (out, len) = unsafe { (out_array(output, capacity), w(len)) };
    status(copy_out(
        shader_source(language, stage).as_bytes(),
        out,
        len,
    ))
}

/// Encodes a glyph in font units at the selected variation location.
///
/// # Safety
/// Handles are live. `records` is writable for `capacity` records, and
/// `len` is writable and does not alias any other argument.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sofel_gpu_paint_glyph(
    font: *const crate::SofelFont,
    location: *const crate::SofelLocation,
    glyph: u32,
    palette: u16,
    records: *mut [i16; 4],
    capacity: usize,
    len: *mut usize,
) -> i32 {
    use sofel_font::{HbScale, read::GlyphId};
    use sofel_geom::{ColorRef, Painter, PointF, Solid, Transform};
    use sofel_outline::{HbExtents, HbOutlineCommand as Cmd, Scale};
    // SAFETY: caller supplies the live handles and writable ranges above.
    let (font, location, records, len) = unsafe {
        (
            crate::r(font),
            crate::r(location),
            out_array(records, capacity),
            w(len),
        )
    };
    status((|| {
        let font = some(font)?;
        let loc = location
            .map(|l| &l.location)
            .unwrap_or(sofel_font::var::Location::default_ref());
        let instance = font.font.instance(loc);
        if glyph >= font.font.num_glyphs() {
            return Err(crate::ERROR_INVALID_GLYPH);
        }
        let scale = HbScale::unscaled(&font.font);
        let outlines = crate::outlines_at(font, location);
        let hb = HbExtents::new(instance);
        let mut scratch = sofel_outline::Scratch::default();
        let mut paint =
            sofel_gpu::Paint::new([scale.x_scale(), scale.y_scale()], |gid, t: Transform| {
                let mut path = Outline::default();
                let point = |p: PointF| {
                    let v = t.apply(p);
                    [v.x, v.y]
                };
                let mut emit = |cmd| match cmd {
                    Cmd::MoveTo(p) => path.move_to(point(p)),
                    Cmd::LineTo(p) => path.line_to(point(p)),
                    Cmd::QuadTo(c, p) => path.quad_to(point(c), point(p)),
                    Cmd::CubicTo(a, b, p) => path.cubic_to(point(a), point(b), point(p)),
                    Cmd::Close => path.close(),
                };
                if let Some(commands) = hb.cff_glyph_path(GlyphId(gid), &scale) {
                    for cmd in commands {
                        emit(cmd);
                    }
                } else if outlines
                    .load(GlyphId(gid), Scale::Unscaled, &mut scratch)
                    .is_ok()
                {
                    scratch
                        .hb_decompose(1., 1., emit)
                        .map_err(|_| sofel_gpu::OutOfRange)?;
                }
                Ok(path)
            });
        let options = sofel_color::PaintOptions {
            palette: sofel_color::Palette::Font(palette),
            foreground: sofel_geom::Color::new(0, 0, 0, 255),
            ppem: 0,
        };
        if sofel_color::paint(
            instance,
            glyph,
            &options,
            &mut sofel_color::Scratch::default(),
            &mut paint,
        ) == sofel_color::Painted::None
        {
            paint.fill_glyph(
                glyph,
                &Solid {
                    color: options.foreground,
                    source: ColorRef {
                        index: None,
                        alpha: 1.,
                    },
                },
            );
        }
        let data = paint.encode().map_err(|_| ERROR_INVALID_ARGUMENT)?;
        copy_out(&data, records, len)
    })())
}
