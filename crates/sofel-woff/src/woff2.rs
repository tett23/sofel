use super::*;

// WOFF2 specification, section 4.1, Known Table Tags.
const TAGS: &[u8] = b"cmapheadhheahmtxmaxpnameOS/2postcvt fpgmglyflocaprepCFF VORGEBDTEBLCgasphdmxkernLTSHPCLTVDMXvheavmtxBASEGDEFGPOSGSUBEBSCJSTFMATHCBDTCBLCCOLRCPALSVG sbixacntavarbdatblocbslncvarfdscfeatfmtxfvargvarhstyjustlcarmortmorxopbdproptrakZapfSilfGlatGlocFeatSill";

fn base128(out: &mut Vec<u8>, n: u32) {
    for shift in [28, 21, 14, 7, 0] {
        if shift == 0 || n >> shift != 0 {
            let byte = u8::try_from((n >> shift) & 127).unwrap_or_default();
            out.push(if shift == 0 { byte } else { byte | 128 });
        }
    }
}

pub(super) fn encode(font: Sfnt<'_>, mut records: Vec<TableRecord>) -> Result<Vec<u8>> {
    records.retain(|r| r.tag != Tag::new(b"DSIG"));
    records.sort_by_key(|r| r.tag);
    let count = u16::try_from(records.len()).map_err(|_| invalid())?;
    let mut total = 12usize.saturating_add(16 * usize::from(count));
    let mut directory = Vec::new();
    let mut stream = Vec::new();
    for r in records {
        let tag = r.tag.to_u32().to_be_bytes();
        let index = TAGS
            .as_chunks::<4>()
            .0
            .iter()
            .position(|t| *t == tag)
            .and_then(|i| u8::try_from(i).ok())
            .unwrap_or(63);
        directory.push(if matches!(&tag, b"glyf" | b"loca") {
            192 | index
        } else {
            index
        });
        if index == 63 {
            directory.extend(tag);
        }
        base128(&mut directory, r.length);
        let data = table(r, font.file())?;
        total = total
            .checked_add(aligned(data.len())?)
            .ok_or_else(invalid)?;
        let start = stream.len();
        stream.extend(data);
        if &tag == b"head" {
            let head = stream.get_mut(start..).ok_or_else(invalid)?;
            *head.get_mut(16).ok_or_else(invalid)? |= 8;
            head.get_mut(8..12).ok_or_else(invalid)?.fill(0);
        }
    }
    let compressed = crate::compress::brotli(&stream);
    let mut out = header(b"wOF2", font.version(), count, size(total)?);
    out.extend(size(compressed.len())?.to_be_bytes());
    out.resize(48, 0);
    out.extend(directory);
    out.extend(compressed);
    out.resize(aligned(out.len())?, 0);
    finish(out)
}
