//! Compiles the vendored upstream `pngquant` C I/O layer (`pngquant.c`,
//! `rwpng.c`) together with vendored `zlib`, `libpng`, and `LittleCMS 2`
//! (`lcms2`) completely from source, with zero external system library
//! dependencies on macOS, Windows, or Linux.

use std::env;
use std::path::{Path, PathBuf};

fn c_files_in(dir: &Path, exclude_stems: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|ext| ext == "c")
                && !exclude_stems.contains(&p.file_stem().unwrap().to_string_lossy().as_ref())
        })
        .collect();
    files.sort();
    files
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_root = manifest.join("../../third_party/src");
    let zlib_dir = src_root.join("zlib");
    let libpng_dir = src_root.join("libpng16");
    let lcms2_dir = src_root.join("lcms2");
    let pngquant_dir = src_root.join("pngquant");
    let liq_include = pngquant_dir.join("lib/imagequant-sys");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", pngquant_dir.display());
    println!("cargo:rerun-if-changed={}", libpng_dir.display());
    println!("cargo:rerun-if-changed={}", lcms2_dir.display());

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let windows = target_os == "windows";

    // 1. Vendored zlib
    let mut zlib = cc::Build::new();
    zlib.warnings(false).opt_level(2).include(&zlib_dir);
    if !windows {
        zlib.define("HAVE_UNISTD_H", Some("1"));
    }
    for f in c_files_in(
        &zlib_dir,
        &[
            "example", "minigzip", "gzclose", "gzlib", "gzread", "gzwrite",
        ],
    ) {
        zlib.file(f);
    }
    zlib.compile("pq_zlib");

    // 2. Vendored libpng 1.6.44 (with ARM NEON / x86_64 SSE2 intrinsics & full chunk support)
    let mut libpng = cc::Build::new();
    libpng
        .warnings(false)
        .opt_level(2)
        .include(&libpng_dir)
        .include(&zlib_dir);
    for f in c_files_in(&libpng_dir, &["example", "pngtest"]) {
        libpng.file(f);
    }
    if target_arch == "aarch64" {
        libpng.define("PNG_ARM_NEON_OPT", Some("2"));
        for f in c_files_in(&libpng_dir.join("arm"), &[]) {
            libpng.file(f);
        }
    } else if target_arch == "x86_64" {
        libpng.define("PNG_INTEL_SSE_OPT", Some("1"));
        for f in c_files_in(&libpng_dir.join("intel"), &[]) {
            libpng.file(f);
        }
    } else {
        libpng.define("PNG_ARM_NEON_OPT", Some("0"));
        libpng.define("PNG_INTEL_SSE_OPT", Some("0"));
        libpng.define("PNG_POWERPC_VSX_OPT", Some("0"));
        libpng.define("PNG_MIPS_MSA_OPT", Some("0"));
        libpng.define("PNG_LOONGARCH_LSX_OPT", Some("0"));
    }
    libpng.compile("pq_libpng");

    // 3. Vendored LittleCMS 2 (`lcms2`)
    let mut lcms = cc::Build::new();
    lcms.warnings(false)
        .opt_level(2)
        .include(lcms2_dir.join("include"))
        .include(lcms2_dir.join("src"));
    for f in c_files_in(&lcms2_dir.join("src"), &[]) {
        lcms.file(f);
    }
    lcms.compile("pq_lcms2");

    // 4. Vendored pngquant (`rwpng.c` + `pngquant.c`)
    let mut pq = cc::Build::new();
    pq.warnings(false)
        .opt_level(2)
        .define("PNGQUANT_NO_MAIN", Some("1"))
        .define("USE_LCMS", Some("1"))
        .define("NDEBUG", Some("1"))
        .include(&pngquant_dir)
        .include(&liq_include)
        .include(&libpng_dir)
        .include(&zlib_dir)
        .include(lcms2_dir.join("include"));
    if !pq.get_compiler().is_like_msvc() {
        pq.flag("-std=gnu99");
    }
    if target_arch == "x86_64" {
        pq.define("USE_SSE", Some("1"));
    }
    pq.file(pngquant_dir.join("rwpng.c"));
    pq.file(pngquant_dir.join("pngquant.c"));
    pq.compile("pq_core");

    if !windows {
        println!("cargo:rustc-link-lib=m");
    }
}
