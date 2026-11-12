//! Fixed CLI observations for color-svg.ttf, face 0, default axes and
//! palette, 48 ppem (sofel ADR 0072 I3 and sofel ADR 0074 in-gradient).
//!
//! Each anchor (x,y) in the source SVG is transformed to font coordinates
//! (120x + 37.5, 950 - 120y), scaled by 48/1024, then floored to select
//! the nearest pixel center in baseline coordinates (right and up).
//! The samples retain the CLI PAM's straight 8-bit RGBA channel order.
//! Source anchors in order: (2,5), (8,5), (5,2), (5,8), (5,5), (6.7,5.6), (2,2).

const POSITIONS: [(i32, i32); 7] = [
    (13, 16),
    (46, 16),
    (29, 33),
    (29, -1),
    (29, 16),
    (39, 13),
    (13, 33),
];
const GLYPH19: [[u8; 4]; 7] = [
    [255, 195, 0, 140],
    [255, 19, 0, 244],
    [255, 198, 0, 245],
    [255, 18, 0, 138],
    [255, 204, 0, 255],
    [255, 70, 57, 183],
    [0, 0, 0, 0],
];
const GLYPH26: [[u8; 4]; 7] = [
    [255, 194, 0, 255],
    [255, 19, 0, 255],
    [255, 198, 0, 255],
    [255, 18, 0, 255],
    [255, 108, 0, 255],
    [255, 56, 0, 104],
    [0, 0, 0, 0],
];

/// Checks the exact pixels written by sofel-render for one reference glyph.
pub fn check_cli_pixels(glyph: u32, pam: &[u8]) -> Result<(), String> {
    let expected = match glyph {
        19 => GLYPH19,
        26 => GLYPH26,
        _ => return Err(format!("no color-svg reference for glyph {glyph}")),
    };
    if !pam.starts_with(b"P7\n") {
        return Err(format!("glyph {glyph}: expected RGBA PAM"));
    }
    let end = b"ENDHDR\n";
    let split = pam
        .windows(end.len())
        .position(|part| part == end)
        .ok_or("missing ENDHDR")?;
    let header = std::str::from_utf8(&pam[..split]).map_err(|_| "invalid PAM header")?;
    if !["DEPTH 4", "MAXVAL 255", "TUPLTYPE RGB_ALPHA"]
        .iter()
        .all(|field| header.lines().any(|line| line == *field))
    {
        return Err("expected 8-bit RGBA PAM".into());
    }
    let dimensions = |field| -> Result<usize, String> {
        header
            .lines()
            .find_map(|l| l.strip_prefix(field))
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| format!("missing PAM {field}"))
    };
    let width = dimensions("WIDTH ")?;
    let rows = dimensions("HEIGHT ")?;
    let origin = header
        .lines()
        .find_map(|l| l.strip_prefix("# left "))
        .ok_or("missing PAM origin")?;
    let (left, top) = origin.split_once(" top ").ok_or("malformed PAM origin")?;
    let left: i32 = left.parse().map_err(|_| "invalid PAM left")?;
    let top: i32 = top.parse().map_err(|_| "invalid PAM top")?;
    let pixels = &pam[split + end.len()..];
    if width.checked_mul(rows).and_then(|n| n.checked_mul(4)) != Some(pixels.len()) {
        return Err("PAM pixel count does not match its dimensions".into());
    }
    for ((x, y), rgba) in POSITIONS.into_iter().zip(expected) {
        let col = usize::try_from(i64::from(x) - i64::from(left))
            .ok()
            .filter(|&col| col < width);
        let row = usize::try_from(i64::from(top) - 1 - i64::from(y))
            .ok()
            .filter(|&row| row < rows);
        let (Some(col), Some(row)) = (col, row) else {
            return Err(format!("glyph {glyph}, pixel ({x},{y}): outside PAM"));
        };
        let at = (row * width + col) * 4;
        let actual = &pixels[at..at + 4];
        if actual != rgba {
            return Err(format!(
                "glyph {glyph}, pixel ({x},{y}): expected RGBA {rgba:?}, got {actual:?}"
            ));
        }
    }
    Ok(())
}
