//! FreeType TrueType driver policies for native hinting (sofel ADR 0064).
//! The known face names and table signatures are from ttobjs.c, FreeType Project License.
use sofel_font::{
    Font,
    read::{Tag, validate::checksum},
};

const FAMILIES: &[&str] = &[
    "cpop",
    "DFGirl-W6-WIN-BF",
    "DFGothic-EB",
    "DFGyoSho-Lt",
    "DFHei",
    "DFHSGothic-W5",
    "DFHSMincho-W3",
    "DFHSMincho-W7",
    "DFKaiSho-SB",
    "DFKaiShu",
    "DFKai-SB",
    "DFMing",
    "DLC",
    "HuaTianKaiTi?",
    "HuaTianSongTi?",
    "Ming(for ISO10646)",
    "MingLiU",
    "MingMedium",
    "PMingLiU",
    "MingLi43",
];
#[rustfmt::skip]
const SIGNATURES: &[[(u32,usize);3]] = &[
    [(0x05BCF058, 0x000002E4), (0x28233BF1, 0x000087C4), (0xA344A1EA, 0x000001E1)],
    [(0x05BCF058, 0x000002E4), (0x28233BF1, 0x000087C4), (0xA344A1EB, 0x000001E1)],
    [(0x12C3EBB2, 0x00000350), (0xB680EE64, 0x000087A7), (0xCE939563, 0x00000758)],
    [(0x11E5EAD4, 0x00000350), (0xCE5956E9, 0x0000BC85), (0x8272F416, 0x00000045)],
    [(0x1257EB46, 0x00000350), (0xF699D160, 0x0000715F), (0xD222F568, 0x000003BC)],
    [(0x1262EB4E, 0x00000350), (0xE86A5D64, 0x00007940), (0x7850F729, 0x000005FF)],
    [(0x122DEB0A, 0x00000350), (0x3D16328A, 0x0000859B), (0xA93FC33B, 0x000002CB)],
    [(0x125FEB26, 0x00000350), (0xA5ACC982, 0x00007EE1), (0x90999196, 0x0000041F)],
    [(0x11E5EAD4, 0x00000350), (0x5A30CA3B, 0x00009063), (0x13A42602, 0x0000007E)],
    [(0x11E5EAD4, 0x00000350), (0xA6E78C01, 0x00008998), (0x13A42602, 0x0000007E)],
    [(0x11E5EAD4, 0x00000360), (0x9DB282B2, 0x0000C06E), (0x53E6D7CA, 0x00000082)],
    [(0x1243EB18, 0x00000350), (0xBA0A8C30, 0x000074AD), (0xF3D83409, 0x0000037B)],
    [(0x07DCF546, 0x00000308), (0x40FE7C90, 0x00008E2A), (0x608174B5, 0x0000007A)],
    [(0xEB891238, 0x00000308), (0xD2E4DCD4, 0x0000676F), (0x8EA5F293, 0x000003B8)],
    [(0xFFFBFFFC, 0x00000008), (0x9C9E48B8, 0x0000BEA2), (0x70020112, 0x00000008)],
    [(0xFFFBFFFC, 0x00000008), (0x0A5A0483, 0x00017C39), (0x70020112, 0x00000008)],
    [(0x00000000, 0x00000000), (0x40C92555, 0x000000E5), (0xA39B58E3, 0x0000117C)],
    [(0x00000000, 0x00000000), (0x33C41652, 0x000000E5), (0x26D6C52A, 0x00000F6A)],
    [(0x00000000, 0x00000000), (0x6DB1651D, 0x0000019D), (0x6C6E4B03, 0x00002492)],
    [(0x00000000, 0x00000000), (0x40C92555, 0x000000E5), (0xDE51FAD0, 0x0000117C)],
    [(0x00000000, 0x00000000), (0x85E47664, 0x000000E5), (0xA6C62831, 0x00001CAA)],
    [(0x00000000, 0x00000000), (0x2D891CFD, 0x0000019D), (0xA0604633, 0x00001DE8)],
    [(0x00000000, 0x00000000), (0x40AA774C, 0x000001CB), (0x9B5CAA96, 0x00001F9A)],
    [(0x00000000, 0x00000000), (0x0D3DE9CB, 0x00000141), (0xD4127766, 0x00002280)],
    [(0x00000000, 0x00000000), (0x4A692698, 0x000001F0), (0x340D4346, 0x00001FCA)],
    [(0x00000000, 0x00000000), (0xCD34C604, 0x00000166), (0x6CF31046, 0x000022B0)],
    [(0x00000000, 0x00000000), (0x5DA75315, 0x0000019D), (0x40745A5F, 0x000022E0)],
    [(0x00000000, 0x00000000), (0xF055FC48, 0x000001C2), (0x3900DED3, 0x00001E18)],
    [(0x00170003, 0x00000060), (0xDBB4306E, 0x000058AA), (0xD643482A, 0x00000035)],
    [(0x1269EB58, 0x00000350), (0x5CD5957A, 0x00006A4E), (0xF758323A, 0x00000380)],
    [(0x122FEB0B, 0x00000350), (0x7F10919A, 0x000070A9), (0x7CD7E7B7, 0x0000025C)],
];

pub(super) fn tricky(font: &Font<'_>) -> bool {
    let wws = font
        .os2()
        .is_some_and(|table| table.fs_selection() & 256 != 0);
    let ids: &[u16] = if wws { &[16, 1] } else { &[21, 16, 1] };
    for &id in ids {
        let len = font.name_utf8(id, None, &mut []).total;
        if len == 0 {
            continue;
        }
        let mut family = alloc::vec![0;len.saturating_add(1)];
        let written = font.name_utf8(id, None, &mut family).written;
        if family
            .get(..written)
            .and_then(|b| core::str::from_utf8(b).ok())
            .is_some_and(|name| FAMILIES.iter().any(|family| name.contains(family)))
        {
            return true;
        }
        break;
    }
    let tags = [Tag::new(b"cvt "), Tag::new(b"fpgm"), Tag::new(b"prep")];
    let tables = tags.map(|t| font.table(t).unwrap_or_default());
    let lengths = tables.map(<[u8]>::len);
    let mut sums = None;
    SIGNATURES.iter().any(|id| {
        if lengths != id.map(|(_, len)| len) {
            return false;
        }
        let actual =
            sums.get_or_insert_with(|| tags.map(|tag| checksum(tag, font.table(tag))));
        *actual == id.map(|(sum, _)| sum)
    })
}

pub(super) fn gasp(data: &[u8], ppem: u16) -> Option<u16> {
    let ([vh, vl, ch, cl], ranges) = data.split_first_chunk::<4>()?;
    if u16::from_be_bytes([*vh, *vl]) != 1 {
        return None;
    }
    let count = usize::from(u16::from_be_bytes([*ch, *cl]));
    let records = ranges.as_chunks::<4>().0.get(..count)?;
    records.iter().find_map(|&[hi, lo, fh, fl]| {
        (ppem <= u16::from_be_bytes([hi, lo])).then(|| u16::from_be_bytes([fh, fl]))
    })
}

pub(super) fn device_width(font: &Font<'_>, glyph: u32, ppem: u16) -> Option<i32> {
    use sofel_font::read::post;
    if font
        .table(post::TAG)
        .and_then(|b| post::Post::new(b).ok())
        .is_some_and(|p| p.is_fixed_pitch() != 0)
    {
        return None;
    }
    let (header, records) = font.table(Tag::new(b"hdmx"))?.split_first_chunk::<8>()?;
    let [_, _, ch, cl, a, b, c, d] = *header;
    let count = usize::from(u16::from_be_bytes([ch, cl]));
    let mut size = u32::from_be_bytes([a, b, c, d]);
    if size >= 0xffff0000 {
        size &= 0xffff;
    }
    if !(1..=255).contains(&count) || size != font.num_glyphs().checked_add(5)? & !3 {
        return None;
    }
    let record = records
        .chunks_exact(usize::try_from(size).ok()?)
        .take(count)
        .find(|r| r.first().copied().map(u16::from) == Some(ppem))?;
    Some(i32::from(*record.get(usize::try_from(glyph).ok()?.checked_add(2)?)?) << 6)
}
