//! The CID-keyed Type 1 header, Font DICTs and data section.

use core::fmt;

use crate::noalloc::Problem;

/// Invalid CID-keyed Type 1 header, map, or data bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CidError {
    offset: usize,
}

impl CidError {
    pub(crate) const fn invalid() -> Self {
        Self { offset: 0 }
    }

    pub(crate) const fn at(offset: usize) -> Self {
        Self { offset }
    }

    /// Byte position in the original file where reading failed.
    pub const fn offset(self) -> usize {
        self.offset
    }
}

impl fmt::Display for CidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid CID-keyed Type 1 font")
    }
}

impl core::error::Error for CidError {}

/// Font DICT transform, normalized to 16.16 font units as FreeType does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CidMatrix {
    /// Horizontal contribution from x.
    pub xx: i32,
    /// Vertical contribution from x.
    pub yx: i32,
    /// Horizontal contribution from y.
    pub xy: i32,
    /// Vertical contribution from y.
    pub yy: i32,
    /// Horizontal translation in font units.
    pub dx: i32,
    /// Vertical translation in font units.
    pub dy: i32,
}

/// The values of one Font DICT.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FontDict {
    pub(crate) len_iv: i32,
    pub(crate) subr_count: usize,
    pub(crate) subr_map_offset: usize,
    pub(crate) sd_bytes: usize,
    pub(crate) matrix: CidMatrix,
}

/// The header values and the data section found by [`scan`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Header<'a> {
    /// The PostScript text before `StartData`.
    pub(crate) header: &'a [u8],
    /// The binary data section, when it is not hexadecimal.
    pub(crate) raw: Option<&'a [u8]>,
    pub(crate) cid_count: usize,
    pub(crate) cidmap_offset: usize,
    pub(crate) fd_bytes: usize,
    pub(crate) gd_bytes: usize,
    pub(crate) units_per_em: u16,
}

/// Check the header, the Font DICTs and the bounds of the data section,
/// passing each decoded hexadecimal data byte to `byte` and each Font
/// DICT to `dict`.
pub(crate) fn scan<'a>(
    source: &'a [u8],
    max_output: usize,
    byte: &mut dyn FnMut(u8),
    visit_dict: &mut dyn FnMut(FontDict),
) -> Result<Header<'a>, CidError> {
    if !source.starts_with(b"%!PS-Adobe-3.0 Resource-CIDFont") {
        return Err(CidError::invalid());
    }
    let (start, hex) = find_start_data(source).ok_or(CidError::invalid())?;
    let header = source.get(..start).ok_or(CidError::invalid())?;
    let data_start = start
        .checked_add(b"StartData".len())
        .and_then(|value| value.checked_add(1))
        .ok_or(CidError::invalid())?;
    let raw = source.get(data_start..).ok_or(CidError::invalid())?;
    let data_len = if hex {
        decode_hex(raw, max_output, data_start, byte)?
    } else {
        if raw.len() > max_output {
            return Err(CidError::at(data_start.saturating_add(max_output)));
        }
        raw.len()
    };
    if field_usize(header, b"CIDFontType")? != 0 {
        return Err(CidError::invalid());
    }
    let cid_count = field_usize(header, b"CIDCount")?;
    let cidmap_offset = field_usize(header, b"CIDMapOffset")?;
    let fd_bytes = field_usize(header, b"FDBytes")?;
    let gd_bytes = field_usize(header, b"GDBytes")?;
    let dict_count = field_usize(header, b"FDArray")?;
    if cid_count == 0 || dict_count == 0 || fd_bytes > 4 || !(1..=4).contains(&gd_bytes) {
        return Err(CidError::invalid());
    }
    let entry = fd_bytes.checked_add(gd_bytes).ok_or(CidError::invalid())?;
    let map_len = cid_count
        .checked_add(1)
        .and_then(|count| count.checked_mul(entry))
        .ok_or(CidError::invalid())?;
    let map_end = cidmap_offset
        .checked_add(map_len)
        .ok_or(CidError::invalid())?;
    if map_end > data_len {
        return Err(CidError::at(source.len()));
    }
    let mut dict_total = 0usize;
    let mut units_per_em = 1000;
    let marker = b"%ADOBeginFontDict";
    let mut rest = header;
    while let Some(pos) = find_bytes(rest, marker) {
        rest = rest
            .get(pos.checked_add(marker.len()).ok_or(CidError::invalid())?..)
            .ok_or(CidError::invalid())?;
        let dict_base = header.len().saturating_sub(rest.len());
        let end = find_bytes(rest, marker).unwrap_or(rest.len());
        let dict = rest.get(..end).ok_or(CidError::invalid())?;
        let locate = |error: CidError| CidError::at(dict_base.saturating_add(error.offset()));
        let subr_count = optional_field_usize(dict, b"SubrCount")
            .map_err(locate)?
            .unwrap_or(0);
        let subr_map_offset = optional_field_usize(dict, b"SubrMapOffset")
            .map_err(locate)?
            .unwrap_or(0);
        let sd_bytes = optional_field_usize(dict, b"SDBytes")
            .map_err(locate)?
            .unwrap_or(0);
        if subr_count > 0 {
            let map_end = subr_count
                .checked_add(1)
                .and_then(|count| count.checked_mul(sd_bytes))
                .and_then(|size| subr_map_offset.checked_add(size))
                .ok_or(CidError::invalid())?;
            if !(1..=4).contains(&sd_bytes) {
                return Err(CidError::invalid());
            }
            if map_end > data_len {
                return Err(CidError::at(source.len()));
            }
        }
        let (matrix, current_upem) = parse_matrix(dict).map_err(|_| {
            CidError::at(dict_base.saturating_add(find_bytes(dict, b"/FontMatrix").unwrap_or(0)))
        })?;
        dict_total = dict_total.saturating_add(1);
        visit_dict(FontDict {
            len_iv: optional_field_i32(dict, b"lenIV")
                .map_err(locate)?
                .unwrap_or(4),
            subr_count,
            subr_map_offset,
            sd_bytes,
            matrix,
        });
        if let Some(current_upem) = current_upem {
            units_per_em = current_upem;
        }
        rest = rest.get(end..).ok_or(CidError::invalid())?;
    }
    if dict_total != dict_count {
        return Err(CidError::invalid());
    }
    Ok(Header {
        header,
        raw: (!hex).then_some(raw),
        cid_count,
        cidmap_offset,
        fd_bytes,
        gd_bytes,
        units_per_em,
    })
}

/// Validate a CID-keyed Type 1 font: its header, Font DICTs, CIDMap and
/// subroutine map bounds, and its binary or hexadecimal data section.
pub fn validate(source: &[u8], max_output: usize, report: &mut dyn FnMut(Problem)) {
    if let Err(error) = scan(source, max_output, &mut |_| {}, &mut |_| {}) {
        report(Problem {
            offset: error.offset(),
            reason: "invalid CID-keyed Type 1 font",
        });
    }
}

fn find_bytes(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

fn find_start_data(source: &[u8]) -> Option<(usize, bool)> {
    let mut hex_arguments = [false; 2];
    let mut at = 0usize;
    while let Some(&byte) = source.get(at) {
        let token = at;
        if byte == b'%' {
            while source
                .get(at)
                .is_some_and(|value| !matches!(value, b'\r' | b'\n'))
            {
                at = at.checked_add(1)?;
            }
            continue;
        } else if byte == b'(' {
            let mut depth = 1usize;
            at = at.checked_add(1)?;
            while depth != 0 {
                let &value = source.get(at)?;
                if value == b'\\' {
                    at = at.checked_add(2)?;
                    continue;
                }
                if value == b'(' {
                    depth = depth.checked_add(1)?;
                }
                if value == b')' {
                    depth = depth.checked_sub(1)?;
                }
                at = at.checked_add(1)?;
            }
        } else if byte == b'/' {
            at = at.checked_add(1)?;
            while source.get(at).is_some_and(|&value| word_byte(value)) {
                at = at.checked_add(1)?;
            }
        } else if word_byte(byte) {
            let start = at;
            while source.get(at).is_some_and(|&value| word_byte(value)) {
                at = at.checked_add(1)?;
            }
            if source.get(start..at) == Some(&b"StartData"[..]) {
                return Some((start, hex_arguments[0]));
            }
        } else {
            at = at.checked_add(1)?;
        }
        if !byte.is_ascii_whitespace() {
            hex_arguments = [hex_arguments[1], source.get(token..at) == Some(b"(Hex)")];
        }
    }
    None
}

fn word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn field_usize(data: &[u8], name: &[u8]) -> Result<usize, CidError> {
    let (value, at) = field_i32(data, name)?;
    usize::try_from(value).map_err(|_| CidError::at(at))
}

fn optional_field_usize(data: &[u8], name: &[u8]) -> Result<Option<usize>, CidError> {
    optional_field_i32(data, name)?
        .map(|value| usize::try_from(value).map_err(|_| CidError::invalid()))
        .transpose()
}

fn optional_field_i32(data: &[u8], name: &[u8]) -> Result<Option<i32>, CidError> {
    if find_key(data, name).is_none() {
        return Ok(None);
    }
    field_i32(data, name).map(|(value, _)| Some(value))
}

/// The first position of `/name` in `data`.
fn find_key(data: &[u8], name: &[u8]) -> Option<usize> {
    let len = name.len().checked_add(1)?;
    data.windows(len)
        .position(|w| w.first() == Some(&b'/') && w.get(1..) == Some(name))
}

fn field_i32(data: &[u8], name: &[u8]) -> Result<(i32, usize), CidError> {
    let at = find_key(data, name).ok_or(CidError::at(data.len()))?;
    let after_key = at
        .checked_add(name.len())
        .and_then(|n| n.checked_add(1))
        .ok_or(CidError::at(at))?;
    let untrimmed = data.get(after_key..).ok_or(CidError::at(after_key))?;
    if !untrimmed.first().is_some_and(u8::is_ascii_whitespace) {
        return Err(CidError::at(after_key));
    }
    let rest = untrimmed.trim_ascii_start();
    let value_at = after_key.saturating_add(untrimmed.len().saturating_sub(rest.len()));
    let end = rest
        .iter()
        .position(|b| !b.is_ascii_digit() && *b != b'-')
        .unwrap_or(rest.len());
    let text = core::str::from_utf8(rest.get(..end).ok_or(CidError::at(value_at))?)
        .map_err(|_| CidError::at(value_at))?;
    let value: Result<i32, _> = text.parse();
    value
        .map(|value| (value, value_at))
        .map_err(|_| CidError::at(value_at))
}

pub(crate) fn read_be(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .fold(0usize, |value, &byte| (value << 8) | usize::from(byte))
}

fn decode_hex(
    raw: &[u8],
    max_output: usize,
    data_start: usize,
    sink: &mut dyn FnMut(u8),
) -> Result<usize, CidError> {
    let mut len = 0usize;
    let mut high = None;
    for (index, &byte) in raw.iter().enumerate() {
        if byte == b'>' {
            if let Some(hi) = high.take() {
                if len >= max_output {
                    return Err(CidError::at(data_start.saturating_add(index)));
                }
                sink(hi << 4);
                len = len.saturating_add(1);
            }
            break;
        }
        if byte == 0 || byte.is_ascii_whitespace() {
            continue;
        }
        let offset = data_start.saturating_add(index);
        let digit = u8::try_from((byte as char).to_digit(16).ok_or(CidError::at(offset))?)
            .map_err(|_| CidError::at(offset))?;
        if let Some(hi) = high.take() {
            if len >= max_output {
                return Err(CidError::at(offset));
            }
            sink((hi << 4) | digit);
            len = len.saturating_add(1);
        } else {
            high = Some(digit);
        }
    }
    if high.is_some() {
        return Err(CidError::at(data_start.saturating_add(raw.len())));
    }
    Ok(len)
}

pub(crate) fn parse_matrix(dict: &[u8]) -> Result<(CidMatrix, Option<u16>), CidError> {
    let Some(at) = find_bytes(dict, b"/FontMatrix") else {
        return Ok((
            CidMatrix {
                xx: 65536,
                yx: 0,
                xy: 0,
                yy: 65536,
                dx: 0,
                dy: 0,
            },
            None,
        ));
    };
    let after = at
        .checked_add(b"/FontMatrix".len())
        .ok_or(CidError::invalid())?;
    let data = dict
        .get(after..)
        .ok_or(CidError::invalid())?
        .trim_ascii_start();
    let data = data.strip_prefix(b"[").ok_or(CidError::invalid())?;
    let end = data
        .iter()
        .position(|&byte| byte == b']')
        .ok_or(CidError::invalid())?;
    let mut values: [&[u8]; 6] = [&[]; 6];
    let mut count = 0usize;
    for part in data
        .get(..end)
        .ok_or(CidError::invalid())?
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
    {
        if let Some(slot) = values.get_mut(count) {
            *slot = part;
        }
        count = count.saturating_add(1);
    }
    if count != 6 {
        return Err(CidError::invalid());
    }
    let mut fixed = [0i32; 6];
    for (target, value) in fixed.iter_mut().zip(values) {
        let text = core::str::from_utf8(value).map_err(|_| CidError::invalid())?;
        let number: f64 = text.parse().map_err(|_| CidError::invalid())?;
        let scaled = number * 1000.0 * 65536.0;
        if !scaled.is_finite() || scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
            return Err(CidError::invalid());
        }
        // The explicit range check above makes this rounded float conversion bounded.
        #[allow(clippy::cast_possible_truncation)]
        let rounded = (scaled + if scaled < 0.0 { -0.5 } else { 0.5 }) as i32;
        *target = rounded;
    }
    let scale = fixed[3].unsigned_abs();
    if scale == 0 {
        return Ok((
            CidMatrix {
                xx: 0,
                yx: 0,
                xy: 0,
                yy: 0,
                dx: 0,
                dy: 0,
            },
            None,
        ));
    }
    let upem = if scale == 65536 {
        None
    } else {
        Some(
            u16::try_from(
                (1000u64 << 16)
                    .saturating_add(u64::from(scale / 2))
                    .checked_div(u64::from(scale))
                    .ok_or(CidError::invalid())?,
            )
            .map_err(|_| CidError::invalid())?,
        )
    };
    let normalize = |value: i32| -> Result<i32, CidError> {
        let numerator = i64::from(value).saturating_mul(65536);
        i32::try_from(
            numerator
                .checked_div(i64::from(scale))
                .ok_or(CidError::invalid())?,
        )
        .map_err(|_| CidError::invalid())
    };
    let matrix = CidMatrix {
        xx: normalize(fixed[0])?,
        yx: normalize(fixed[1])?,
        xy: normalize(fixed[2])?,
        yy: normalize(fixed[3])?,
        dx: normalize(fixed[4])? >> 16,
        dy: normalize(fixed[5])? >> 16,
    };
    Ok((matrix, upem))
}
