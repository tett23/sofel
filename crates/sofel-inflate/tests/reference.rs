//! Wuffs test/data pairs raw, fixed-Huffman, gzip and zlib streams with their original text.
//! Its README identifies gzip and extract-deflate-offsets.go as generators and records
//! the sources of the Shakespeare excerpts and digits of pi. See testdata.lock and Wuffs LICENSE.
#![allow(clippy::unwrap_used)]
use sofel_inflate::{deflate, gzip, zlib};
use std::{fs, path::Path};

#[test]
#[ignore = "requires the Wuffs checkout pinned by testdata.lock"]
fn i1_i3_pinned_reference_streams() {
    let data = std::env::var_os("SOFEL_TESTDATA")
        .map_or_else(
            || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/testdata"),
            Into::into,
        )
        .join("wuffs/test/data");
    for name in ["romeo.txt", "midsummer.txt", "pi.txt"] {
        let expected = fs::read(data.join(name)).unwrap();
        for ext in ["gz", "zlib", "deflate", "fixed-huff.deflate"] {
            if name != "romeo.txt" && ext.ends_with("deflate") {
                continue;
            }
            let file = format!("{name}.{ext}");
            let compressed = fs::read(data.join(&file)).unwrap();
            let decode = match ext {
                "gz" => gzip,
                "zlib" => zlib,
                _ => deflate,
            };
            assert_eq!(
                decode(&compressed, expected.len()).unwrap(),
                expected,
                "{file}"
            );
            assert!(
                decode(&compressed, expected.len().saturating_sub(1)).is_err(),
                "{file}: limit"
            );
        }
    }
}
