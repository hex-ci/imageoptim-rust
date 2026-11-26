//! Compiles the vendored upstream optimizer sources into helper executables
//! shipped inside the app (no user-installed tools). Tools whose sources are
//! absent or fail to compile are skipped; the app then falls back to the
//! prebuilt binaries bundled per platform in `third_party/bin`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_root = manifest.join("../../third_party/src");
    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out
    let profile_dir = out
        .ancestors()
        .nth(3)
        .expect("OUT_DIR layout")
        .to_path_buf();
    let tools_dir = profile_dir.join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    let obj_root = out.join("obj");
    std::fs::create_dir_all(&obj_root).unwrap();

    let target = std::env::var("TARGET").unwrap();
    let windows = target.contains("windows");
    let c_comp = cc::Build::new().get_compiler();
    let cxx_comp = cc::Build::new().cpp(true).get_compiler();
    let tc = Toolchain {
        c: c_comp.path().to_path_buf(),
        cxx: cxx_comp.path().to_path_buf(),
        msvc: c_comp.is_like_msvc(),
    };

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../third_party/src");

    // Shared object sets (compiled once, linked into several tools).
    let zlib = compile_zlib(&src_root, &obj_root, &tc, windows);
    let libpng = compile_libpng(&src_root, &obj_root, &tc, &zlib);
    let mozjpeg = compile_mozjpeg(&src_root, &obj_root, &tc, windows);

    let jobs: Vec<(&str, JobFn)> = vec![
        ("gifsicle", |s, o, t, w, _, _, _| build_gifsicle(s, o, t, w)),
        ("zopflipng", |s, o, t, w, _, _, _| {
            build_zopflipng(s, o, t, w)
        }),
        ("pngcrush", build_pngcrush),
        ("guetzli", build_guetzli),
        ("jpegtran", build_jpegtran),
        ("jpegoptim", build_jpegoptim),
        ("advpng", build_advpng),
    ];
    for (name, job) in jobs {
        let src = if name == "jpegtran" {
            src_root.join("mozjpeg")
        } else {
            src_root.join(name)
        };
        if !src.exists() {
            continue;
        }
        match job(
            &src,
            &obj_root.join(name),
            &tc,
            windows,
            &zlib,
            &libpng,
            &mozjpeg,
        ) {
            Ok(()) => {
                let exe = obj_root.join(name).join(exe_name(name, windows));
                let dest = tools_dir.join(exe_name(name, windows));
                if let Err(e) = std::fs::copy(&exe, &dest) {
                    println!("cargo:warning={name}: install failed: {e}");
                }
            }
            Err(e) => {
                println!("cargo:warning={name} not compiled ({e}); bundled binary will be used")
            }
        }
    }
}

fn exe_name(name: &str, windows: bool) -> String {
    if windows {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

type JobFn =
    fn(&Path, &Path, &Toolchain, bool, &[PathBuf], &[PathBuf], &[PathBuf]) -> Result<(), String>;

struct Toolchain {
    c: PathBuf,
    cxx: PathBuf,
    msvc: bool,
}

impl Toolchain {
    fn compile(
        &self,
        cpp: bool,
        sources: &[PathBuf],
        includes: &[PathBuf],
        defines: &[&str],
        obj_dir: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        self.compile_with(cpp, sources, includes, defines, obj_dir, None)
    }

    fn compile_with(
        &self,
        cpp: bool,
        sources: &[PathBuf],
        includes: &[PathBuf],
        defines: &[&str],
        obj_dir: &Path,
        force_include: Option<&str>,
    ) -> Result<Vec<PathBuf>, String> {
        std::fs::create_dir_all(obj_dir).map_err(|e| e.to_string())?;
        let mut objs = Vec::new();
        for (i, src) in sources.iter().enumerate() {
            let obj = obj_dir.join(format!(
                "{i:03}-{}.{}",
                src.file_stem().unwrap().to_string_lossy(),
                if self.msvc { "obj" } else { "o" }
            ));
            let mut cmd = Command::new(if cpp { &self.cxx } else { &self.c });
            if self.msvc {
                cmd.arg("/nologo").arg("/O2").arg("/c");
                if let Some(fi) = force_include {
                    cmd.arg(format!("/FI{fi}"));
                }
                if cpp {
                    cmd.arg("/std:c++14").arg("/EHsc");
                }
                for inc in includes {
                    cmd.arg(format!("/I{}", inc.display()));
                }
                for d in defines {
                    cmd.arg(format!("/D{d}"));
                }
                cmd.arg(format!("/Fo{}", obj.display()));
                cmd.arg(src);
            } else {
                cmd.arg("-O2").arg("-c");
                if let Some(fi) = force_include {
                    cmd.arg("-include").arg(fi);
                }
                cmd.arg(if cpp { "-std=c++11" } else { "-std=gnu99" });
                for inc in includes {
                    cmd.arg("-I").arg(inc);
                }
                for d in defines {
                    cmd.arg(format!("-D{d}"));
                }
                cmd.arg("-o").arg(&obj).arg(src);
            }
            let out = cmd.output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(format!(
                    "{}: {}",
                    src.display(),
                    String::from_utf8_lossy(&out.stderr)
                        .chars()
                        .take(2000)
                        .collect::<String>()
                ));
            }
            objs.push(obj);
        }
        Ok(objs)
    }

    fn link(&self, cpp: bool, objs: &[PathBuf], exe: &Path, libs: &[&str]) -> Result<(), String> {
        let mut cmd = Command::new(if cpp { &self.cxx } else { &self.c });
        if self.msvc {
            cmd.arg("/nologo").arg(format!("/Fe:{}", exe.display()));
            for o in objs {
                cmd.arg(o);
            }
            cmd.arg("/link");
            for l in libs {
                cmd.arg(format!("{l}.lib"));
            }
        } else {
            cmd.arg("-O2").arg("-o").arg(exe);
            for o in objs {
                cmd.arg(o);
            }
            for l in libs {
                cmd.arg(format!("-l{l}"));
            }
        }
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(2000)
                .collect());
        }
        Ok(())
    }
}

fn c_sources(dir: &Path, exclude: &[&str]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "c")
                && !exclude.contains(&p.file_stem().unwrap().to_string_lossy().as_ref())
        })
        .collect();
    out.sort();
    out
}

fn build_gifsicle(src: &Path, obj: &Path, tc: &Toolchain, windows: bool) -> Result<(), String> {
    let s = src.join("src");
    let cfg_dir = obj.join("cfg");
    std::fs::create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
    let mut includes = vec![s.clone(), src.join("include")];
    if windows {
        std::fs::copy(s.join("win32cfg.h"), cfg_dir.join("config.h")).map_err(|e| e.to_string())?;
        includes.insert(0, cfg_dir.clone());
    } else {
        includes.insert(0, src.to_path_buf());
    }
    // ENABLE_THREADS is unset in both configs: single-threaded, no pthreads.
    // The vendored giflossy tree's nested lcdf submodule predates
    // `palindex_type`; palette indices are a superset of uint8 in giflossy.
    let defines = ["HAVE_CONFIG_H", "palindex_type=uint16_t"];
    // opttemplate.c is #included twice by optimize.c with different macros.
    let sources = c_sources(
        &s,
        &[
            "gifdiff",
            "gifview",
            "giftoc",
            "opttemplate",
            "gifx",
            "ungifwrt",
        ],
    );
    let objs = tc.compile_with(false, &sources, &includes, &defines, obj, Some("stdint.h"))?;
    let libs: Vec<&str> = if windows { vec![] } else { vec!["m"] };
    tc.link(
        false,
        &objs,
        &obj.join(exe_name("gifsicle", windows)),
        &libs,
    )
}

fn build_zopflipng(src: &Path, obj: &Path, tc: &Toolchain, windows: bool) -> Result<(), String> {
    let z = src.join("src/zopfli");
    let p = src.join("src/zopflipng");
    let l = p.join("lodepng");
    let includes = vec![z.clone(), p.clone(), l.clone()];
    let c_objs = tc.compile(
        false,
        &c_sources(&z, &["zopfli_bin"]),
        &includes,
        &[],
        &obj.join("c"),
    )?;
    let mut cc_sources = vec![
        p.join("zopflipng_bin.cc"),
        p.join("zopflipng_lib.cc"),
        l.join("lodepng.cpp"),
        l.join("lodepng_util.cpp"),
    ];
    cc_sources.retain(|p| p.exists());
    let cc_objs = tc.compile(true, &cc_sources, &includes, &[], &obj.join("cc"))?;
    let mut objs = c_objs;
    objs.extend(cc_objs);
    let libs: Vec<&str> = if windows { vec![] } else { vec!["m"] };
    tc.link(
        true,
        &objs,
        &obj.join(exe_name("zopflipng", windows)),
        &libs,
    )
}
fn compile_zlib(src_root: &Path, obj_root: &Path, tc: &Toolchain, windows: bool) -> Vec<PathBuf> {
    let z = src_root.join("zlib");
    if !z.exists() {
        return Vec::new();
    }
    let srcs = c_sources(
        &z,
        &[
            "example", "minigzip", "gzclose", "gzlib", "gzread", "gzwrite",
        ],
    );
    let zdefs: Vec<&str> = if windows {
        vec![]
    } else {
        vec!["HAVE_UNISTD_H"]
    };
    match tc.compile(
        false,
        &srcs,
        std::slice::from_ref(&z),
        &zdefs,
        &obj_root.join("zlib"),
    ) {
        Ok(o) => o,
        Err(e) => {
            println!("cargo:warning=zlib compile failed: {e}");
            Vec::new()
        }
    }
}

fn compile_libpng(
    src_root: &Path,
    obj_root: &Path,
    tc: &Toolchain,
    zlib: &[PathBuf],
) -> Vec<PathBuf> {
    let p = src_root.join("libpng");
    if !p.exists() || zlib.is_empty() {
        return Vec::new();
    }
    let srcs = c_sources(&p, &["example", "pngtest"]);
    let includes = vec![p.clone(), src_root.join("zlib")];
    match tc.compile(false, &srcs, &includes, &[], &obj_root.join("libpng")) {
        Ok(o) => o,
        Err(e) => {
            println!("cargo:warning=libpng compile failed: {e}");
            Vec::new()
        }
    }
}

fn compile_mozjpeg(
    src_root: &Path,
    obj_root: &Path,
    tc: &Toolchain,
    windows: bool,
) -> Vec<PathBuf> {
    let m = src_root.join("mozjpeg");
    if !m.join("jconfig.h").exists() {
        return Vec::new();
    }
    const LIB: &[&str] = &[
        "jaricom",
        "jcapimin",
        "jcapistd",
        "jcarith",
        "jccoefct",
        "jccolor",
        "jcdctmgr",
        "jchuff",
        "jcinit",
        "jcmainct",
        "jcmarker",
        "jcmaster",
        "jcomapi",
        "jcparam",
        "jcphuff",
        "jcprepct",
        "jcsample",
        "jctrans",
        "jdapimin",
        "jdapistd",
        "jdarith",
        "jdatadst",
        "jdatasrc",
        "jdcoefct",
        "jdcolor",
        "jddctmgr",
        "jdhuff",
        "jdinput",
        "jdmainct",
        "jdmarker",
        "jdmaster",
        "jdmerge",
        "jdphuff",
        "jdpostct",
        "jdsample",
        "jdtrans",
        "jerror",
        "jfdctflt",
        "jfdctfst",
        "jfdctint",
        "jidctflt",
        "jidctfst",
        "jidctint",
        "jidctred",
        "jmemmgr",
        "jmemnobs",
        "jquant1",
        "jquant2",
        "jutils",
        "jcicc",
        "jdicc",
        "jcext",
        "jsimd_none",
    ];
    let srcs: Vec<PathBuf> = LIB
        .iter()
        .map(|n| m.join(format!("{n}.c")))
        .filter(|p| p.exists())
        .collect();
    // The vendored jconfigint.h is GCC/Clang-specific; generate an MSVC one.
    let mut mincludes = vec![m.clone()];
    if windows {
        let cfg = obj_root.join("mozjpeg-cfg");
        std::fs::create_dir_all(&cfg).ok();
        std::fs::write(
            cfg.join("jconfigint.h"),
            "#define BUILD \"20231014\"\n#define INLINE __inline\n#define THREAD_LOCAL __declspec(thread)\n#define PACKAGE_NAME \"mozjpeg\"\n#define VERSION \"4.1.5\"\n#define FALLTHROUGH\n",
        )
        .ok();
        mincludes.insert(0, cfg);
    }
    match tc.compile(
        false,
        &srcs,
        &mincludes,
        &["C_ARITH_CODING_SUPPORTED", "D_ARITH_CODING_SUPPORTED"],
        &obj_root.join("mozjpeg"),
    ) {
        Ok(o) => o,
        Err(e) => {
            println!("cargo:warning=mozjpeg compile failed: {e}");
            Vec::new()
        }
    }
}

fn build_pngcrush(
    src: &Path,
    obj: &Path,
    tc: &Toolchain,
    windows: bool,
    zlib: &[PathBuf],
    libpng: &[PathBuf],
    _mj: &[PathBuf],
) -> Result<(), String> {
    if libpng.is_empty() {
        return Err("libpng objects missing".into());
    }
    let includes = vec![
        src.to_path_buf(),
        src.join("../libpng"),
        src.join("../zlib"),
    ];
    let objs = tc.compile(false, &[src.join("pngcrush.c")], &includes, &[], obj)?;
    let mut all = objs;
    all.extend(libpng.iter().cloned());
    all.extend(zlib.iter().cloned());
    tc.link(
        false,
        &all,
        &obj.join(exe_name("pngcrush", windows)),
        &if windows { vec![] } else { vec!["m"] },
    )
}

fn build_guetzli(
    src: &Path,
    obj: &Path,
    tc: &Toolchain,
    windows: bool,
    zlib: &[PathBuf],
    libpng: &[PathBuf],
    _mj: &[PathBuf],
) -> Result<(), String> {
    if libpng.is_empty() {
        return Err("libpng objects missing".into());
    }
    let g = src.join("guetzli");
    let b = src.join("third_party/butteraugli/butteraugli");
    let mut srcs = cc_sources(&g, &["fuzz_target"]);
    srcs.push(b.join("butteraugli.cc"));
    let includes = vec![
        src.to_path_buf(),
        g.clone(),
        src.join("third_party/butteraugli"),
        b.clone(),
        src.join("../libpng"),
        src.join("../zlib"),
    ];
    let objs = tc.compile(true, &srcs, &includes, &[], obj)?;
    let mut all = objs;
    all.extend(libpng.iter().cloned());
    all.extend(zlib.iter().cloned());
    tc.link(
        true,
        &all,
        &obj.join(exe_name("guetzli", windows)),
        &if windows { vec![] } else { vec!["m"] },
    )
}

fn build_jpegtran(
    src: &Path,
    obj: &Path,
    tc: &Toolchain,
    windows: bool,
    _z: &[PathBuf],
    _p: &[PathBuf],
    mozjpeg: &[PathBuf],
) -> Result<(), String> {
    if mozjpeg.is_empty() {
        return Err("mozjpeg objects missing".into());
    }
    let srcs = vec![
        src.join("jpegtran.c"),
        src.join("cdjpeg.c"),
        src.join("rdswitch.c"),
        src.join("transupp.c"),
    ];
    let objs = tc.compile(false, &srcs, &[src.to_path_buf()], &[], obj)?;
    let mut all = objs;
    all.extend(mozjpeg.iter().cloned());
    tc.link(
        false,
        &all,
        &obj.join(exe_name("jpegtran", windows)),
        &if windows { vec![] } else { vec!["m"] },
    )
}

fn build_jpegoptim(
    src: &Path,
    obj: &Path,
    tc: &Toolchain,
    windows: bool,
    _z: &[PathBuf],
    _p: &[PathBuf],
    mozjpeg: &[PathBuf],
) -> Result<(), String> {
    if mozjpeg.is_empty() {
        return Err("mozjpeg objects missing".into());
    }
    let mut srcs = vec![
        src.join("jpegoptim.c"),
        src.join("misc.c"),
        src.join("jpegdest.c"),
    ];
    if windows {
        srcs.push(src.join("getopt.c"));
    }
    let cfg = obj.join("cfg");
    std::fs::create_dir_all(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(
        cfg.join("config.h"),
        "#define HOST_TYPE \"compiled-in\"\n#define HAVE_LABS 1\n#define HAVE_GETOPT_LONG 1\n#define HAVE_STDINT_H 1\n#define HAVE_UNISTD_H 1\n",
    )
    .map_err(|e| e.to_string())?;
    let includes = vec![cfg, src.join("../mozjpeg"), src.to_path_buf()];
    let objs = tc.compile(false, &srcs, &includes, &["HAVE_CONFIG_H"], obj)?;
    let mut all = objs;
    all.extend(mozjpeg.iter().cloned());
    tc.link(
        false,
        &all,
        &obj.join(exe_name("jpegoptim", windows)),
        &if windows { vec![] } else { vec!["m"] },
    )
}

fn build_advpng(
    src: &Path,
    obj: &Path,
    tc: &Toolchain,
    windows: bool,
    zlib: &[PathBuf],
    _p: &[PathBuf],
    _mj: &[PathBuf],
) -> Result<(), String> {
    if zlib.is_empty() {
        return Err("zlib objects missing".into());
    }
    let mut cc_srcs = cc_sources(
        src,
        &[
            "remng", "rezip", "advzip", "advmng", "mngex", "redef", "scroll", "zipsh",
        ],
    );
    cc_srcs.extend(cc_sources(&src.join("7z"), &[]));
    let mut c_srcs: Vec<PathBuf> = ["lib/fz.c", "lib/png.c", "lib/error.c", "lib/snstring.c"]
        .iter()
        .map(|n| src.join(n))
        .collect();
    if windows {
        c_srcs.push(src.join("getopt.c"));
    }
    let cfg = obj.join("cfg");
    std::fs::create_dir_all(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(cfg.join("config.h"), "#define PACKAGE_VERSION \"1.15\"\n#define PACKAGE \"advancecomp\"\n#define VERSION \"1.15\"\n#define HAVE_STDINT_H 1\n#define HAVE_STRING_H 1\n#define HAVE_STDLIB_H 1\n#define HAVE_SNPRINTF 1\n#define HAVE_SYS_STAT_H 1\n#define HAVE_SYS_TYPES_H 1\n#define HAVE_VSNPRINTF 1\n").map_err(|e| e.to_string())?;
    let includes = vec![cfg, src.to_path_buf(), src.join("../zlib")];
    let mut adefs = vec![
        "HAVE_CONFIG_H",
        "USE_ERROR_SILENT",
        "USE_COMPRESS",
        "USE_7Z=1",
        "USE_LZMA=1",
    ];
    if !windows {
        adefs.push("HAVE_UNISTD_H");
        adefs.push("HAVE_UTIME_H");
    }
    let c_objs = tc.compile(false, &c_srcs, &includes, &adefs, &obj.join("c"))?;
    let cc_objs = tc.compile(true, &cc_srcs, &includes, &adefs, &obj.join("cc"))?;
    let mut all = c_objs;
    all.extend(cc_objs);
    all.extend(zlib.iter().cloned());
    tc.link(
        true,
        &all,
        &obj.join(exe_name("advpng", windows)),
        &if windows { vec![] } else { vec!["m"] },
    )
}

fn cc_sources(dir: &Path, exclude: &[&str]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "cc" || e == "cpp")
                && !exclude.contains(&p.file_stem().unwrap().to_string_lossy().as_ref())
        })
        .collect();
    out.sort();
    out
}
