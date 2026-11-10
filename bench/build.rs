//! Links the static FreeType, HarfBuzz and shim libraries built by
//! bench/build-c.sh for the target (ADR 0013).

use std::path::PathBuf;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("..");
    let dir = root.join("target/bench-c").join(&target);
    for lib in ["benchshim", "harfbuzz", "freetype"] {
        let file = dir.join(format!("lib{lib}.a"));
        if !file.exists() {
            panic!(
                "{} is missing; run: bench/build-c.sh {target}",
                file.display()
            );
        }
        println!("cargo:rerun-if-changed={}", file.display());
    }
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static=benchshim");
    println!("cargo:rustc-link-lib=static=harfbuzz");
    println!("cargo:rustc-link-lib=static=freetype");
    // HarfBuzz's C++ runtime (std::mutex and friends) on native targets.
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=dylib=c++");
    } else if target.contains("linux") {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
    // setjmp and longjmp for FreeType in wasm (bench/build-c.sh).
    if dir.join("libsetjmp.a").exists() {
        println!("cargo:rustc-link-lib=static=setjmp");
    }
}
