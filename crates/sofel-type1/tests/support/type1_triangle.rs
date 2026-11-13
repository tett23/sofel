//! Small Type 1 face accepted by pinned FreeType 2.15.0.
#![allow(dead_code)]

pub fn sample() -> (Vec<u8>, Vec<u8>) {
    with_glyph_program(&[
        139, 248, 136, 13, 239, 239, 21, 239, 139, 5, 139, 239, 5, 39, 39, 5, 9, 14,
    ])
}

pub fn with_glyph_program(program: &[u8]) -> (Vec<u8>, Vec<u8>) {
    with_glyph_name(program, "A")
}

pub fn with_glyph_name(program: &[u8], name: &str) -> (Vec<u8>, Vec<u8>) {
    with_glyph_name_and_blues(program, name, "[]")
}

pub fn with_glyph_name_and_blues(program: &[u8], name: &str, blues: &str) -> (Vec<u8>, Vec<u8>) {
    let header = b"%!PS-AdobeFont-1.0: Test 1.0\n\
11 dict begin\n/FontName /Test def\n/FontType 1 def\n/PaintType 0 def\n\
/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n\
/FontBBox [0 0 200 200] readonly def\n/Encoding 256 array\n\
0 1 255 {1 index exch /.notdef put} for\ndup 65 /A put\n\
readonly def\ncurrentdict end\ncurrentfile eexec\n";
    let header = String::from_utf8_lossy(header)
        .replace("/A put", &format!("/{name} put"))
        .into_bytes();
    let notdef = [139, 248, 136, 13, 14];
    let mut private = b"dup /Private 8 dict dup begin\n\
/RD {string currentfile exch readstring pop} executeonly def\n\
/ND {noaccess def} executeonly def\n/NP {noaccess put} executeonly def\n\
/lenIV -1 def\n/BlueValues "
        .to_vec();
    private.extend_from_slice(blues.as_bytes());
    private
        .extend_from_slice(b" def\n/Subrs 0 array\n/CharStrings 2 dict dup begin\n/.notdef 5 RD ");
    private.extend_from_slice(&notdef);
    private.extend_from_slice(format!(" ND\n/{name} {} RD ", program.len()).as_bytes());
    private.extend_from_slice(program);
    private.extend_from_slice(
        b" ND\nend\nend\nreadonly put\nnoaccess put\n\
dup /FontName get exch definefont pop\nmark currentfile closefile\n",
    );
    let mut state = 55665u16;
    let cipher: Vec<u8> = b"ABCD"
        .iter()
        .chain(&private)
        .map(|plain| {
            let encoded = plain ^ (state >> 8) as u8;
            state = (u16::from(encoded).wrapping_add(state))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            encoded
        })
        .collect();
    let mut pfa = header.clone();
    for chunk in cipher.chunks(32) {
        for byte in chunk {
            pfa.extend_from_slice(format!("{byte:02x}").as_bytes());
        }
        pfa.push(b'\n');
    }
    pfa.extend_from_slice(&[b'0'; 512]);
    pfa.extend_from_slice(b"\ncleartomark\n");
    let mut pfb = Vec::new();
    section(&mut pfb, 1, &header);
    section(&mut pfb, 2, &cipher);
    section(&mut pfb, 1, b"\ncleartomark\n");
    pfb.extend_from_slice(&[0x80, 3]);
    (pfa, pfb)
}

fn section(out: &mut Vec<u8>, kind: u8, bytes: &[u8]) {
    out.extend_from_slice(&[0x80, kind]);
    out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(bytes);
}
