//! `ft-dump --cid`, `--ps-info` and the seeded `--outline` on small
//! CFF and CFF2 fonts built by hand (ADR 0021): the output must be what
//! the fonts say.

#![allow(
    clippy::all,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "../../sofel-read/tests/support/build.rs"]
mod build;

use std::path::PathBuf;
use std::process::Command;

use build::*;
use sofel_conformance::paths;
use sofel_font::read::cff::dict::op;

use Cs::{I, Op};

fn run(mode: &str, name: &str, bytes: &[u8]) -> String {
    let exe = paths::ft_dump();
    paths::require(&exe, "ft-dump").unwrap_or_else(|e| panic!("{e}"));
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let tag: String = mode.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let p = base.join(format!(
        "sofel-ftdump-cff-{name}-{tag}-{}.otf",
        std::process::id()
    ));
    std::fs::write(&p, bytes).unwrap();
    let mut c = Command::new(exe);
    for a in mode.split(' ') {
        c.arg(a);
    }
    let out = c.arg(&p).output().unwrap();
    if std::env::var_os("KEEP").is_none() {
        let _ = std::fs::remove_file(&p);
    }
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

fn glyphs() -> Vec<Vec<u8>> {
    vec![
        charstring(&[Op(14)]),
        charstring(&[I(0), I(0), Op(21), I(500), Op(6), I(700), Op(7), Op(14)]),
    ]
}

fn named() -> Vec<u8> {
    let mut spec = Cff1Spec::new(glyphs());
    spec.charset = Some(vec![0, 0, 34]);
    spec.strings = vec![b"Note".to_vec(), b"/FSType 4 def".to_vec()];
    spec.top = dict(&[
        (&[391], op::NOTICE),
        (&[392], op::POSTSCRIPT),
        (&[1], op::IS_FIXED_PITCH),
    ]);
    otto_font(2, vec![(b"CFF ", cff1(&spec))])
}

fn cid() -> Vec<u8> {
    let mut spec = Cff1Spec::new(glyphs());
    spec.private = None;
    spec.charset = Some(vec![0, 0, 7]);
    spec.strings = vec![b"Adobe".to_vec(), b"Identity".to_vec()];
    spec.cid = Some(CidSpec {
        ros: (391, 392, 3),
        fds: vec![(Vec::new(), Vec::new(), Vec::new())],
        fd_select: vec![0, 0, 0],
    });
    otto_font(2, vec![(b"CFF ", cff1(&spec))])
}

fn cff2_font() -> Vec<u8> {
    otto_font(
        2,
        vec![(
            b"CFF2",
            cff2(&Cff2Spec {
                charstrings: vec![
                    Vec::new(),
                    charstring(&[I(0), I(0), Op(21), I(500), Op(6), I(700), Op(7)]),
                ],
                fds: vec![(Vec::new(), Vec::new())],
                ..Default::default()
            }),
        )],
    )
}

#[test]
fn cid_output() {
    assert_eq!(
        run("--cid", "cid", &cid()),
        "face 0\nis_cid 0 1\nros 0 41646f6265 4964656e74697479 3\ncid 0 0 0\ncid 1 0 7\ncid 2 6 0\nend\n"
    );
    assert_eq!(
        run("--cid", "named", &named()),
        "face 0\nis_cid 0 0\nros 6\ncid 0 6 0\ncid 1 6 0\ncid 2 6 0\nend\n"
    );
    assert_eq!(
        run("--cid", "cff2", &cff2_font()),
        "face 0\nis_cid 0 0\nros 6\ncid 0 6 0\ncid 1 6 0\ncid 2 6 0\nend\n"
    );
}

#[test]
fn ps_info_output() {
    // Notice "Note", isFixedPitch 1, the default underline -100 50, and
    // /FSType 4.
    assert_eq!(
        run("--ps-info", "named", &named()),
        "face 0\nhas_ps_glyph_names 1\nps_font_info 0 - 4e6f7465 - - - 0 1 -100 50\nfs_type_flags 4\nend\n"
    );
    assert_eq!(
        run("--ps-info", "cff2", &cff2_font()),
        "face 0\nhas_ps_glyph_names 0\nps_font_info 6\nfs_type_flags 0\nend\n"
    );
}

#[test]
fn seeded_outline_output() {
    assert_eq!(
        run("--outline unscaled", "named", &named()),
        "face 0\nnum_glyphs 2\nglyph 0\noverlap 0\ncbox 0 0 0 0\nglyph 1\nM 0 0\nL 500 0\nL 500 700\nL 0 0\nZ\noverlap 0\ncbox 0 0 500 700\nend\n"
    );
}
