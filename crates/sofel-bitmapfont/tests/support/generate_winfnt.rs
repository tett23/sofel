//! Regenerate the hand-built Windows font fixtures (ADR 0088).
use std::{env, fs, path::PathBuf};

fn word(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
fn dword(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn mono() -> Vec<u8> {
    let mut data = vec![0; 160];
    for (offset, value) in [
        (0, 0x200),
        (68, 8),
        (70, 72),
        (72, 72),
        (74, 8),
        (83, 400),
        (86, 8),
        (88, 8),
        (91, 8),
        (93, 8),
        (99, 1),
        (118, 8),
        (120, 122),
    ] {
        word(&mut data, offset, value);
    }
    for (offset, value) in [(2, 160), (105, 130), (113, 122)] {
        dword(&mut data, offset, value);
    }
    data[95..97].copy_from_slice(b"AA");
    data[122..130].copy_from_slice(&[0x18, 0x24, 0x42, 0x7e, 0x42, 0x42, 0x42, 0]);
    data[130..135].copy_from_slice(b"Test\0");
    data
}
fn version3(font: &[u8]) -> Vec<u8> {
    let mut data = vec![0; 192];
    data[..118].copy_from_slice(&font[..118]);
    data[154..162].copy_from_slice(&font[122..130]);
    data[162..].copy_from_slice(&font[130..]);
    word(&mut data, 0, 0x300);
    word(&mut data, 148, 8);
    for (offset, value) in [(2, 192), (105, 162), (113, 154), (150, 154)] {
        dword(&mut data, offset, value);
    }
    data
}
fn ne(font: &[u8]) -> Vec<u8> {
    let mut second = font.to_vec();
    second[95..97].copy_from_slice(b"BB");
    second[122..130].fill(255);
    let mut data = vec![0; 0x240];
    data[..2].copy_from_slice(b"MZ");
    dword(&mut data, 0x3c, 0x40);
    data[0x40..0x42].copy_from_slice(b"NE");
    for (offset, value) in [
        (0x64, 0x40),
        (0x66, 0x64),
        (0x82, 0x8008),
        (0x8a, 0x100),
        (0x8c, 160),
        (0x90, 0x8001),
        (0x96, 0x1a0),
        (0x98, 160),
        (0x9c, 0x8002),
    ] {
        word(&mut data, offset, value);
    }
    dword(&mut data, 0x84, 2);
    data[0x100..0x1a0].copy_from_slice(font);
    data[0x1a0..].copy_from_slice(&second);
    data
}
fn pe(font: &[u8]) -> Vec<u8> {
    let mut data = vec![0; 0x4a0];
    data[..2].copy_from_slice(b"MZ");
    data[0x40..0x44].copy_from_slice(b"PE\0\0");
    data[0x138..0x13d].copy_from_slice(b".rsrc");
    for (offset, value) in [
        (0x44, 0x14c),
        (0x46, 1),
        (0x54, 0xe0),
        (0x58, 0x10b),
        (0x20e, 1),
        (0x226, 1),
        (0x23e, 1),
    ] {
        word(&mut data, offset, value);
    }
    for (offset, value) in [
        (0x3c, 0x40),
        (0xb4, 16),
        (0xc8, 0x1000),
        (0xcc, 0x2a0),
        (0x140, 0x2a0),
        (0x144, 0x1000),
        (0x148, 0x2a0),
        (0x14c, 0x200),
        (0x210, 8),
        (0x214, 0x80000018),
        (0x228, 1),
        (0x22c, 0x80000030),
        (0x240, 0x409),
        (0x244, 0x48),
        (0x248, 0x1200),
        (0x24c, 160),
    ] {
        dword(&mut data, offset, value);
    }
    data[0x400..].copy_from_slice(font);
    data
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(env::args_os().nth(1).ok_or("provide an output directory")?);
    fs::create_dir_all(&dir)?;
    let font = mono();
    for (name, bytes) in [
        ("mono3.fnt", version3(&font)),
        ("two.fon", ne(&font)),
        ("mono-pe.fon", pe(&font)),
        ("mono.fnt", font),
    ] {
        fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}
