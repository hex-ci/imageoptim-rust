//! Standalone `pngquant` CLI binary compiled 100% from vendored sources
//! (`third_party/src/pngquant` + `imagequant` + `libpng` + `zlib` + `lcms2`).

mod ffi;

use std::ffi::{CStr, CString};
use std::io;
use std::os::raw::{c_char, c_uint};
use std::ptr;

use ffi::pngquant_error::*;
use ffi::*;
use imagequant_sys::liq_error::LIQ_OK;
use imagequant_sys::*;
use libc::FILE;

fn unwrap_ptr(opt: Option<&CString>) -> *const c_char {
    opt.map_or(ptr::null(), |c| c.as_ptr())
}

fn print_full_version(fd: &mut dyn io::Write, c_fd: *mut FILE) {
    let _ = writeln!(
        fd,
        "pngquant, {} (Rust), by Kornel Lesinski, Greg Roelofs.",
        env!("CARGO_PKG_VERSION")
    );
    let _ = fd.flush();
    unsafe {
        pngquant_internal_print_config(c_fd);
    }
    let _ = writeln!(fd);
}

fn print_usage(fd: &mut dyn io::Write) {
    let text = unsafe { CStr::from_ptr(PNGQUANT_USAGE) }.to_string_lossy();
    let _ = writeln!(fd, "{text}");
}

fn parse_quality(quality: &str) -> Option<(u8, u8)> {
    let mut parts = quality.splitn(2, '-');
    let left = parts.next().unwrap();
    let right = parts.next();

    Some(match (left, right) {
        (t, Some("")) => (t.parse().ok()?, 100),
        ("", Some(t)) => (0, t.parse().ok()?),
        (t, None) => {
            let target: u8 = t.parse().ok()?;
            ((u16::from(target) * 9 / 10) as u8, target)
        }
        (l, Some(t)) => (l.parse().ok()?, t.parse().ok()?),
    })
}

unsafe extern "C" fn log_callback(_a: &liq_attr, msg: *const c_char, _user: AnySyncSendPtr) {
    let text = unsafe { CStr::from_ptr(msg) }.to_string_lossy();
    eprintln!("{text}");
}

#[derive(Default)]
struct ParsedArgs {
    verbose: bool,
    help: bool,
    force: bool,
    no_force: bool,
    ordered: bool,
    nofs: bool,
    iebug: bool,
    transbug: bool,
    skip_if_larger: bool,
    strip: bool,
    version: bool,
    floyd: Option<String>,
    ext: Option<String>,
    output: Option<String>,
    speed: Option<String>,
    quality: Option<String>,
    posterize: Option<String>,
    map: Option<String>,
    colors: Option<String>,
    free: Vec<String>,
}

fn parse_cli_args(raw: &[String]) -> Result<ParsedArgs, String> {
    let mut out = ParsedArgs::default();
    let mut i = 0usize;
    let mut positional_only = false;

    while i < raw.len() {
        let arg = &raw[i];
        if positional_only || arg == "-" || !arg.starts_with('-') {
            out.free.push(arg.clone());
            i += 1;
            continue;
        }
        if arg == "--" {
            positional_only = true;
            i += 1;
            continue;
        }

        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline_val) = match long.split_once('=') {
                Some((k, v)) => (k, Some(v.to_owned())),
                None => (long, None),
            };
            let mut take_val = |opt_name: &str| -> Result<String, String> {
                if let Some(v) = inline_val.clone() {
                    Ok(v)
                } else if i + 1 < raw.len() {
                    i += 1;
                    Ok(raw[i].clone())
                } else {
                    Err(format!("Argument to option '{opt_name}' missing"))
                }
            };
            match name {
                "verbose" => out.verbose = true,
                "quiet" => out.verbose = false,
                "help" => out.help = true,
                "version" => out.version = true,
                "force" => out.force = true,
                "no-force" => out.no_force = true,
                "ordered" => out.ordered = true,
                "nofs" => out.nofs = true,
                "iebug" => out.iebug = true,
                "transbug" => out.transbug = true,
                "skip-if-larger" => out.skip_if_larger = true,
                "strip" => out.strip = true,
                "floyd" => {
                    out.floyd = Some(inline_val.unwrap_or_else(|| "1.0".to_owned()));
                }
                "ext" => out.ext = Some(take_val("ext")?),
                "output" => out.output = Some(take_val("output")?),
                "speed" => out.speed = Some(take_val("speed")?),
                "quality" => out.quality = Some(take_val("quality")?),
                "posterize" => out.posterize = Some(take_val("posterize")?),
                "map" => out.map = Some(take_val("map")?),
                "colors" => out.colors = Some(take_val("colors")?),
                other => return Err(format!("Unrecognized option: '--{other}'")),
            }
            i += 1;
            continue;
        }

        // Legacy single-dash word aliases accepted by upstream pngquant.
        match arg.as_str() {
            "-fs" | "-floyd" => {
                out.floyd = Some("1.0".to_owned());
                i += 1;
                continue;
            }
            "-nofs" | "-nofloyd" | "-ordered" => {
                out.ordered = true;
                i += 1;
                continue;
            }
            "-force" => {
                out.force = true;
                i += 1;
                continue;
            }
            "-noforce" => {
                out.no_force = true;
                i += 1;
                continue;
            }
            "-verbose" | "-noquiet" => {
                out.verbose = true;
                i += 1;
                continue;
            }
            "-quiet" | "-noverbose" => {
                out.verbose = false;
                i += 1;
                continue;
            }
            "-help" => {
                out.help = true;
                i += 1;
                continue;
            }
            "-version" => {
                out.version = true;
                i += 1;
                continue;
            }
            "-ext" if i + 1 < raw.len() => {
                i += 1;
                out.ext = Some(raw[i].clone());
                i += 1;
                continue;
            }
            "-speed" if i + 1 < raw.len() => {
                i += 1;
                out.speed = Some(raw[i].clone());
                i += 1;
                continue;
            }
            _ => {}
        }

        // Short options cluster (e.g. `-s3`, `-v`, `-o out.png`, `-Q 60-80`).
        let chars: Vec<char> = arg[1..].chars().collect();
        let mut ci = 0usize;
        while ci < chars.len() {
            let ch = chars[ci];
            let mut take_short_val = |flag: char| -> Result<String, String> {
                if ci + 1 < chars.len() {
                    let r: String = chars[ci + 1..].iter().collect();
                    ci = chars.len();
                    Ok(r)
                } else if i + 1 < raw.len() {
                    i += 1;
                    ci = chars.len();
                    Ok(raw[i].clone())
                } else {
                    Err(format!("Argument to option '-{flag}' missing"))
                }
            };
            match ch {
                'v' => out.verbose = true,
                'q' => out.verbose = false,
                'h' => out.help = true,
                'V' => out.version = true,
                'f' => out.force = true,
                's' => {
                    out.speed = Some(take_short_val('s')?);
                    break;
                }
                'Q' => {
                    out.quality = Some(take_short_val('Q')?);
                    break;
                }
                'o' => {
                    out.output = Some(take_short_val('o')?);
                    break;
                }
                other => return Err(format!("Unrecognized option: '-{other}'")),
            }
            ci += 1;
        }
        i += 1;
    }

    Ok(out)
}

fn main() {
    std::process::exit(run() as i32);
}

fn run() -> pngquant_error {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let has_some_explicit_args = !raw.is_empty();
    let mut m = match parse_cli_args(&raw) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("{err}");
            print_usage(&mut io::stderr());
            return MISSING_ARGUMENT;
        }
    };

    let posterize: c_uint = m
        .posterize
        .as_deref()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let floyd: f32 = m
        .floyd
        .as_deref()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1.0);
    if !(0.0..=1.0).contains(&floyd) {
        eprintln!("--floyd argument must be in 0..1 range");
        return INVALID_ARGUMENT;
    }

    let extension = m.ext.as_deref().and_then(|s| CString::new(s).ok());
    let map_file = m.map.as_deref().and_then(|s| CString::new(s).ok());

    let colors: c_uint = if let Some(c) = m
        .colors
        .as_deref()
        .or_else(|| m.free.first().map(String::as_str))
        .and_then(|s| s.parse::<c_uint>().ok())
    {
        if m.colors.is_none() {
            m.free.remove(0);
            if m.free.is_empty() {
                m.free.push("-".to_owned());
            }
        }
        c
    } else {
        0
    };

    let using_stdin = m.free.len() == 1 && m.free.first().map(String::as_str) == Some("-");
    let mut using_stdout = using_stdin;
    let output_file_path = m.output.as_deref().and_then(|s| {
        if s == "-" {
            using_stdout = true;
            None
        } else {
            using_stdout = false;
            CString::new(s).ok()
        }
    });

    let files: Vec<CString> = m
        .free
        .drain(..)
        .filter_map(|s| CString::new(s).ok())
        .collect();
    let file_ptrs: Vec<*const c_char> = files.iter().map(|s| s.as_ptr()).collect();

    let mut options = pngquant_options {
        fixed_palette_image: ptr::null_mut(),
        log_callback: None,
        log_callback_user_info: ptr::null_mut(),
        quality: ptr::null(),
        extension: unwrap_ptr(extension.as_ref()),
        output_file_path: unwrap_ptr(output_file_path.as_ref()),
        map_file: unwrap_ptr(map_file.as_ref()),
        files: file_ptrs.as_ptr(),
        num_files: file_ptrs.len() as c_uint,
        colors,
        speed: 0,
        posterize,
        floyd,
        using_stdin,
        using_stdout,
        force: m.force && !m.no_force,
        fast_compression: false,
        min_quality_limit: false,
        skip_if_larger: m.skip_if_larger,
        strip: m.strip,
        iebug: m.iebug,
        last_index_transparent: false,
        print_help: m.help,
        print_version: m.version,
        missing_arguments: !has_some_explicit_args,
        verbose: m.verbose,
    };

    if m.nofs || m.ordered {
        options.floyd = 0.0;
    }

    if options.print_version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return SUCCESS;
    }

    if options.missing_arguments {
        print_full_version(&mut io::stderr(), unsafe { pngquant_c_stderr() });
        print_usage(&mut io::stderr());
        return MISSING_ARGUMENT;
    }

    if options.print_help {
        print_full_version(&mut io::stdout(), unsafe { pngquant_c_stdout() });
        print_usage(&mut io::stdout());
        return SUCCESS;
    }

    let Some(mut liq) = liq_attr_create() else {
        eprintln!("SSE-capable CPU is required for this build.");
        return WRONG_ARCHITECTURE;
    };
    let liq = &mut *liq;

    if options.iebug {
        #[allow(deprecated)]
        liq_set_min_opacity(liq, 238);
    }

    if options.verbose {
        unsafe {
            liq_set_log_callback(liq, log_callback, Default::default());
        }
        options.log_callback = Some(log_callback);
    }

    if m.transbug {
        liq_set_last_index_transparent(liq, 1);
    }

    if let Some(speed) = m.speed.as_deref() {
        let set_ok = speed
            .parse::<u8>()
            .ok()
            .filter(|s| (1..=11).contains(s))
            .is_some_and(|mut s| {
                if s >= 10 {
                    options.fast_compression = true;
                    if s == 11 {
                        s = 10;
                        options.floyd = 0.0;
                    }
                }
                LIQ_OK == liq_set_speed(liq, s.into())
            });
        if !set_ok {
            eprintln!("Speed should be between 1 (slow) and 11 (fast).");
            return INVALID_ARGUMENT;
        }
    }

    if let Some(q) = m.quality.as_deref() {
        if let Some((limit, target)) = parse_quality(q) {
            options.min_quality_limit = limit > 0;
            if LIQ_OK != liq_set_quality(liq, limit.into(), target.into()) {
                eprintln!("Quality value(s) must be numbers in range 0-100.");
                return INVALID_ARGUMENT;
            }
        } else {
            eprintln!(
                "Quality should be in format min-max where min and max are numbers in range 0-100."
            );
            return INVALID_ARGUMENT;
        }
    }

    if options.colors > 0 && LIQ_OK != liq_set_max_colors(liq, options.colors as _) {
        eprintln!("Number of colors must be between 2 and 256.");
        return INVALID_ARGUMENT;
    }

    if options.posterize > 0 && LIQ_OK != liq_set_min_posterization(liq, options.posterize as _) {
        eprintln!("Posterization should be number of bits in range 0-4.");
        return INVALID_ARGUMENT;
    }

    if !options.extension.is_null() && !options.output_file_path.is_null() {
        eprintln!("--ext and --output options can't be used at the same time");
        return INVALID_ARGUMENT;
    }

    if options.extension.is_null() {
        options.extension = if options.floyd > 0.0 {
            c"-fs8.png".as_ptr()
        } else {
            c"-or8.png".as_ptr()
        };
    }

    if !options.output_file_path.is_null() && options.num_files != 1 {
        eprintln!(
            "  error: Only one input file is allowed when --output is used. This error also happens when filenames with spaces are not in quotes."
        );
        return INVALID_ARGUMENT;
    }

    if options.using_stdout && !options.using_stdin && options.num_files != 1 {
        eprintln!(
            "  error: Only one input file is allowed when using the special output path \"-\" to write to stdout. This error also happens when filenames with spaces are not in quotes."
        );
        return INVALID_ARGUMENT;
    }

    if options.num_files == 0 && !options.using_stdin {
        eprintln!("No input files specified.");
        if options.verbose {
            print_full_version(&mut io::stdout(), unsafe { pngquant_c_stdout() });
        }
        print_usage(&mut io::stderr());
        return MISSING_ARGUMENT;
    }

    unsafe { pngquant_main_internal(&mut options, liq) }
}
