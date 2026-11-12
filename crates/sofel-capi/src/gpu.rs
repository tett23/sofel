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
