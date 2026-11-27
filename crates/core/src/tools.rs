//! Unified adapter for every optimizer in the serial pipeline.
//!
//! Built-in engines call the Rust crates in [`crate::engine`]; tools that have
//! no maintained crate (`advpng`, `pngcrush`, `pngout`, `guetzli`, `svgo`) are
//! driven through their upstream CLI with the same arguments ImageOptim uses.
//! Missing binaries are reported as unavailable and skipped, never fatal.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};

use crate::engine;
use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    PngCrush,
    PngQuant,
    OxiPng,
    PngOut,
    AdvPng,
    Zopfli,
    JpegOptim,
    JpegTran,
    Guetzli,
    Gifsicle,
    Svgo,
    SvgCleaner,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Self::PngCrush => "Pngcrush",
            Self::PngQuant => "pngquant",
            Self::OxiPng => "OxiPNG",
            Self::PngOut => "PNGOUT",
            Self::AdvPng => "AdvPNG",
            Self::Zopfli => "Zopfli",
            Self::JpegOptim => "JpegOptim",
            Self::JpegTran => "Jpegtran",
            Self::Guetzli => "Guetzli",
            Self::Gifsicle => "Gifsicle",
            Self::Svgo => "SVGO",
            Self::SvgCleaner => "Svgcleaner",
        }
    }

    /// Best-tool column name (upstream `toolName:` differs from the status
    /// "Started <worker>" name for jpegtran).
    pub fn best_name(self) -> &'static str {
        match self {
            Self::JpegTran => "MozJPEG",
            _ => self.name(),
        }
    }

    /// Name used in the "%@ failed to start" error (upstream prefs key).
    pub fn start_name(self) -> &'static str {
        match self {
            Self::PngCrush => "PngCrush",
            Self::PngQuant => "PngQuant",
            Self::OxiPng => "OptiPng",
            Self::PngOut => "PngOut",
            Self::AdvPng => "AdvPng",
            Self::JpegTran => "JpegTran",
            Self::Svgo => "Svgo",
            _ => self.name(),
        }
    }

    pub fn is_available(self) -> bool {
        match self {
            Self::OxiPng
            | Self::Zopfli
            | Self::JpegOptim
            | Self::JpegTran
            | Self::SvgCleaner
            | Self::PngQuant
            | Self::Svgo => true,
            Self::Gifsicle => engine::find_gifsicle().is_some(),
            other => find_external(other.binary()).is_some(),
        }
    }

    fn binary(self) -> &'static str {
        match self {
            Self::PngCrush => "pngcrush",
            Self::PngQuant => "pngquant",
            Self::PngOut => "pngout",
            Self::AdvPng => "advpng",
            Self::Guetzli => "guetzli",
            Self::Svgo => "svgo",
            _ => "",
        }
    }
}

/// Whether a bundled/compiled external binary exists for this platform.
pub fn binary_available(binary: &str) -> bool {
    find_external(binary).is_some()
}

fn env_override(binary: &str) -> Option<PathBuf> {
    let key = format!("IMAGEOPTIM_{}", binary.to_ascii_uppercase());
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

pub(crate) fn find_external(binary: &str) -> Option<PathBuf> {
    if let Some(path) = env_override(binary) {
        return Some(path);
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = std::env::var_os("IMAGEOPTIM_TOOLS_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    let file_name = if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_owned()
    };
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        // Workspace binaries (`target/<profile>/pngquant`) & packaged `tools/` dirs:
        dirs.push(dir.to_path_buf());
        dirs.push(dir.join("tools"));
        dirs.push(dir.join("../Resources/tools"));
        // Cargo test runners live in `target/<profile>/deps/<test-bin>` (classic)
        // or `target/<profile>/build/<pkg>/<hash>/out/<test-bin>` (fine-grained build-dir):
        if dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "deps" || n == "out")
        {
            for anc in dir.ancestors().skip(1).take(5) {
                dirs.push(anc.join("tools"));
                dirs.push(anc.to_path_buf());
            }
        } else {
            dirs.push(dir.join("../tools"));
            dirs.push(dir.join(".."));
        }
        // Closed-source PNGOUT is the sole vendor binary in `third_party/bin`.
        if binary == "pngout" {
            let plat = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
            dirs.push(dir.join("../../third_party/bin").join(&plat));
            dirs.push(dir.join("../../third_party/bin"));
            dirs.push(dir.join("../../../third_party/bin").join(&plat));
            dirs.push(dir.join("../../../third_party/bin"));
        }
    }
    if binary != "pngout"
        && let Some(compiled) = option_env!("IMAGEOPTIM_BUILD_TOOLS_DIR")
    {
        let p = PathBuf::from(compiled);
        if p.is_dir() {
            if let Some(profile) = p.parent() {
                dirs.push(profile.to_path_buf());
            }
            dirs.push(p);
        }
    }
    for dir in dirs {
        let candidate = dir.join(&file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run one tool on `data`. Returns `Ok(None)` when the tool is unavailable or
/// deliberately skipped (pngquant exit 98/99). `rerun` selects upstream's
/// alternative Zopfli strategy for files that were already optimized once.
pub fn run(
    tool: Tool,
    data: &[u8],
    settings: &Settings,
    level: u8,
    rerun: bool,
) -> Result<Option<Vec<u8>>> {
    run_cancellable(tool, data, settings, level, rerun, &AtomicBool::new(false))
}

/// Cancellable variant of [`run`]: immediately terminates any spawned external
/// optimizer process within ~20ms when `cancel` becomes `true`.
pub fn run_cancellable(
    tool: Tool,
    data: &[u8],
    settings: &Settings,
    level: u8,
    rerun: bool,
    cancel: &AtomicBool,
) -> Result<Option<Vec<u8>>> {
    if cancel.load(Ordering::Relaxed) || !tool.is_available() {
        return Ok(None);
    }
    let input_apng = crate::filetype::is_apng(data);
    let input_gif_frames = if matches!(tool, Tool::Gifsicle) {
        gif_frame_count(data)
    } else {
        None
    };

    let out = match tool {
        Tool::OxiPng => oxipng(data, settings, level, cancel)?,
        Tool::PngQuant => pngquant(data, settings, level, cancel)?,
        Tool::Zopfli => zopflipng(data, settings, level, rerun, cancel)?,
        Tool::JpegOptim => jpegoptim(data, settings, cancel)?,
        Tool::JpegTran => jpegtran(data, settings.strip_jpeg_metadata, cancel)?,
        Tool::Gifsicle => {
            if settings.lossy_enabled && settings.gif_quality > 30 && settings.gif_quality < 100 {
                engine::gifsicle_data_cancellable(data, settings.gif_quality, false, cancel)?
            } else {
                engine::gifsicle_data_cancellable(data, 100, level > 1, cancel)?
            }
        }
        Tool::SvgCleaner => svgcleaner(data, cancel)?,
        Tool::PngCrush => pngcrush(data, level, settings.strip_png_metadata, cancel)?,
        Tool::PngOut => pngout(data, level, settings.strip_png_metadata, cancel)?,
        Tool::AdvPng => advpng(data, level, cancel)?,
        Tool::Guetzli => {
            let q = if settings.lossy_enabled {
                settings.jpeg_quality
            } else {
                95
            };
            guetzli(data, q, cancel)?
        }
        Tool::Svgo => crate::svgo::optimize(data, settings.lossy_enabled)?,
    };
    if cancel.load(Ordering::Relaxed) || out.is_empty() {
        return Ok(None);
    }
    if out.len() <= 70 {
        bail!("{} produced a truncated file", tool.name());
    }
    // Improvement over upstream: never accept a tool output that does not
    // decode cleanly or silently destroys APNG / animated GIF frames.
    validate_image(&out).with_context(|| format!("{} produced a corrupt image", tool.name()))?;
    if input_apng && !crate::filetype::is_apng(&out) {
        bail!("{} stripped APNG animation frames", tool.name());
    }
    if let Some(want_frames) = input_gif_frames
        && gif_frame_count(&out) != Some(want_frames)
    {
        bail!("{} altered GIF frame count", tool.name());
    }
    Ok(Some(out))
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, e) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *e = c;
    }
    let mut c = 0xffff_ffffu32;
    for b in data {
        c = table[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffff_ffff
}

/// Decode-level integrity check for optimizer outputs.
pub fn validate_image(data: &[u8]) -> Result<()> {
    use crate::filetype::FileKind;
    match FileKind::detect(data, std::path::Path::new("")) {
        FileKind::Png => {
            // Strict chunk CRC walk: the image crate skips some CRC checks.
            if data.len() < 8 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
                bail!("bad png signature");
            }
            let mut i = 8usize;
            let mut seen_iend = false;
            while i + 12 <= data.len() {
                let len =
                    u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
                let typ = &data[i + 4..i + 8];
                if i + 12 + len > data.len() {
                    bail!("truncated png chunk");
                }
                let want = u32::from_be_bytes([
                    data[i + 8 + len],
                    data[i + 9 + len],
                    data[i + 10 + len],
                    data[i + 11 + len],
                ]);
                if crc32(&data[i + 4..i + 8 + len]) != want {
                    bail!("png chunk CRC mismatch");
                }
                if typ == b"IEND" {
                    seen_iend = true;
                    break;
                }
                i += 12 + len;
            }
            if !seen_iend {
                bail!("png missing IEND");
            }
            let img = image::load_from_memory(data).context("decode failed")?;
            let _ = img.to_rgba8();
        }
        FileKind::Jpeg => {
            let img = image::load_from_memory(data).context("decode failed")?;
            let _ = img.to_rgba8();
            if !data.ends_with(&[0xff, 0xd9]) {
                bail!("jpeg missing EOI");
            }
        }
        FileKind::Gif => {
            use image::AnimationDecoder;
            let dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data))
                .context("gif decode failed")?;
            for frame in dec.into_frames() {
                frame.context("gif frame decode failed")?;
            }
        }
        FileKind::Svg => {
            let text = std::str::from_utf8(data).context("svg not utf-8")?;
            svgcleaner::cleaner::parse_data(text, &svgcleaner::ParseOptions::default())
                .map_err(|e| anyhow::anyhow!("svg parse: {e}"))?;
        }
        FileKind::Unknown => bail!("unknown output format"),
    }
    Ok(())
}

/// Returns the decoded frame count of a GIF image.
pub fn gif_frame_count(data: &[u8]) -> Option<usize> {
    use image::AnimationDecoder;
    let dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(data)).ok()?;
    let mut n = 0usize;
    for frame in dec.into_frames() {
        frame.ok()?;
        n += 1;
    }
    Some(n)
}

fn temp_path(ext: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "imageoptim-rust-tool-{}-{}.{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
        ext
    ));
    p
}

static ACTIVE_PIDS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();

fn active_pids() -> &'static Mutex<HashSet<u32>> {
    ACTIVE_PIDS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn kill_pid(pid: u32) {
    #[cfg(unix)]
    unsafe {
        let _ = libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Immediately terminates any running external optimizer child processes
/// (called on app exit so slow tools like Guetzli/Zopfli never orphan).
pub fn kill_active_children() {
    if let Ok(mut pids) = active_pids().lock() {
        for &pid in pids.iter() {
            kill_pid(pid);
        }
        pids.clear();
    }
}

pub(crate) struct ProcessOut {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Spawns an external tool, drains stdout/stderr without pipe deadlock, and
/// checks `cancel` + optional `timeout_secs` every 20ms.
pub(crate) fn run_process_cancellable(
    exe: &Path,
    args: &[OsString],
    cancel: &AtomicBool,
    timeout_secs: Option<u64>,
    sigint_on_timeout: bool,
) -> Result<ProcessOut> {
    use std::io::Read;
    #[cfg(not(unix))]
    let _ = sigint_on_timeout;
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("running {}", exe.display()))?;
    let pid = child.id();
    if let Ok(mut pids) = active_pids().lock() {
        pids.insert(pid);
    }

    let mut out_pipe = child.stdout.take().context("no stdout pipe")?;
    let mut err_pipe = child.stderr.take().context("no stderr pipe")?;
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    let start = std::time::Instant::now();
    let deadline = timeout_secs.map(|s| std::time::Duration::from_secs(s.max(1)));
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Ok(st),
            Ok(None) => {}
            Err(e) => break Err(anyhow::Error::from(e)),
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(anyhow::anyhow!("cancelled"));
        }
        if let Some(limit) = deadline
            && start.elapsed() >= limit
        {
            #[cfg(unix)]
            if sigint_on_timeout {
                unsafe {
                    let _ = libc::kill(pid as libc::pid_t, libc::SIGINT);
                }
                // Give PNGOUT up to 500ms to flush its best output after SIGINT.
                let grace = std::time::Instant::now();
                while grace.elapsed() < std::time::Duration::from_millis(500) {
                    if let Ok(Some(st)) = child.try_wait() {
                        if let Ok(mut pids) = active_pids().lock() {
                            pids.remove(&pid);
                        }
                        return Ok(ProcessOut {
                            status: st,
                            stdout: out_reader.join().unwrap_or_default(),
                            stderr: err_reader.join().unwrap_or_default(),
                        });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
            let _ = child.kill();
            let st = child.wait().with_context(|| "waiting after timeout")?;
            break Ok(st);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };

    if let Ok(mut pids) = active_pids().lock() {
        pids.remove(&pid);
    }
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Ok(ProcessOut {
        status: status?,
        stdout,
        stderr,
    })
}

/// Upstream `timelimitForLevel:` — MIN(8 + level*13, 10 + bytes/1024) seconds.
fn timelimit(level: u8, bytes: usize) -> u64 {
    let max_time = 8 + u64::from(level) * 13;
    let size_time = 10 + (bytes / 1024) as u64;
    max_time.min(size_time)
}

fn pngcrush(data: &[u8], level: u8, strip: bool, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let exe = find_external("pngcrush").context("pngcrush not found")?;
    let input = temp_path("png");
    let output = temp_path("png");
    std::fs::write(&input, data)?;
    let mut args: Vec<OsString> = Vec::new();
    // Upstream: -brute for small PNGs, or at level ≥ 6 unless the file is large.
    if data.len() < 2048 || (level >= 6 && data.len() <= 250_000) {
        args.push("-brute".into());
    }
    if strip {
        args.push("-rem".into());
        args.push("alla".into());
    }
    for arg in ["-nofilecheck", "-bail", "-blacken", "-reduce", "-cc", "--"] {
        args.push(arg.into());
    }
    args.push(input.clone().into_os_string());
    args.push(output.clone().into_os_string());
    let result = run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&output);
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
    let out = result?;
    if !out.status.success() {
        bail!("pngcrush failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    bytes.context("reading pngcrush output")
}

fn pngout(data: &[u8], level: u8, strip: bool, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let exe = find_external("pngout").context("pngout not found")?;
    let input = temp_path("png");
    std::fs::write(&input, data)?;
    let mut args: Vec<OsString> = vec!["-r".into(), "-v".into()];
    let mut mapped = if level == 0 {
        2
    } else if level >= 4 {
        0
    } else {
        1
    };
    // Large PNGs (> 250 KB) use one step faster.
    if data.len() > 250_000 && mapped < 2 {
        mapped += 1;
    }
    if mapped > 0 {
        args.push(format!("-s{mapped}").into());
    }
    if !strip {
        args.push("-k1".into());
    }
    args.push(input.clone().into_os_string());
    args.push("-".into());
    let out = run_process_cancellable(
        &exe,
        &args,
        cancel,
        Some(timelimit(level, data.len())),
        true,
    );
    let _ = std::fs::remove_file(&input);
    let out = out?;
    if out.stdout.len() <= 70 {
        bail!("pngout produced no PNG");
    }
    Ok(out.stdout)
}

fn advpng(data: &[u8], level: u8, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let exe = find_external("advpng").context("advpng not found")?;
    let input = temp_path("png");
    std::fs::write(&input, data)?;
    let level = level.clamp(1, 4);
    let args: Vec<OsString> = vec![
        format!("-{level}").into(),
        "-z".into(),
        "--".into(),
        input.clone().into_os_string(),
    ];
    let out = run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&input);
    let _ = std::fs::remove_file(&input);
    let out = out?;
    if !out.status.success() {
        bail!("advpng failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    bytes.context("reading advpng output")
}

fn guetzli(data: &[u8], quality: u8, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let exe = find_external("guetzli").context("guetzli not found")?;
    // Guetzli consumes PNG (sRGB) and writes JPEG.
    let image = image::load_from_memory(data)?.to_rgb8();
    let input = temp_path("png");
    let output = temp_path("jpg");
    image.save_with_format(&input, image::ImageFormat::Png)?;
    let memlimit = if data.len() < 10 * 1024 { 2000 } else { 6000 };
    let args: Vec<OsString> = vec![
        "--quality".into(),
        quality.max(84).to_string().into(),
        "--memlimit".into(),
        memlimit.to_string().into(),
        input.clone().into_os_string(),
        output.clone().into_os_string(),
    ];
    let out = run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&output);
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
    let out = out?;
    if !out.status.success() {
        bail!("guetzli failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    bytes.context("reading guetzli output")
}

fn jpegtran(data: &[u8], strip: bool, cancel: &AtomicBool) -> Result<Vec<u8>> {
    if find_external("jpegtran").is_none() {
        return engine::jpegtran(data, strip);
    }
    let exe = find_external("jpegtran").context("jpegtran not found")?;
    let input = temp_path("jpg");
    let output = temp_path("jpg");
    std::fs::write(&input, data)?;
    let args: Vec<OsString> = vec![
        "-copy".into(),
        if strip { "none" } else { "all" }.into(),
        "-optimize".into(),
        "-outfile".into(),
        output.clone().into_os_string(),
        input.clone().into_os_string(),
    ];
    let result = run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&output);
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
    let out = result?;
    if !out.status.success() {
        bail!("jpegtran failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    bytes.context("reading jpegtran output")
}

fn jpegoptim(data: &[u8], settings: &Settings, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let quality = if settings.lossy_enabled {
        settings.jpeg_quality
    } else {
        100
    };
    if find_external("jpegoptim").is_none() {
        return engine::jpegoptim(data, quality, settings.strip_jpeg_metadata);
    }
    let exe = find_external("jpegoptim").context("jpegoptim not found")?;
    let input = temp_path("jpg");
    std::fs::write(&input, data)?;
    let mut args: Vec<OsString> = Vec::new();
    if settings.lossy_enabled {
        args.push(format!("-m{quality}").into());
    }
    args.push(
        if settings.strip_jpeg_metadata {
            "--strip-all"
        } else {
            "--strip-none"
        }
        .into(),
    );
    args.push(
        if settings.lossy_enabled {
            "--all-progressive"
        } else {
            "--all-normal"
        }
        .into(),
    );
    args.push("-v".into());
    args.push("--".into());
    args.push(input.clone().into_os_string());
    let out = run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&input);
    let _ = std::fs::remove_file(&input);
    let out = out?;
    if !out.status.success() {
        bail!("jpegoptim failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    bytes.context("reading jpegoptim output")
}

fn oxipng(data: &[u8], settings: &Settings, level: u8, cancel: &AtomicBool) -> Result<Vec<u8>> {
    // Upstream OxiPngWorker: -o MAX(2, MIN(level, 6)).
    let optlevel = level.clamp(2, 6);
    let Some(exe) = env_override("oxipng") else {
        return engine::oxipng(data, optlevel, settings.strip_png_metadata, false, None);
    };
    let input = temp_path("png");
    let output = temp_path("png");
    std::fs::write(&input, data)?;
    let mut args: Vec<OsString> = Vec::new();
    if settings.strip_png_metadata {
        args.push("--strip=safe".into());
    }
    args.push(format!("-o{optlevel}").into());
    for arg in ["-i0", "-a", "--out"] {
        args.push(arg.into());
    }
    args.push(output.clone().into_os_string());
    args.push("--".into());
    args.push(input.clone().into_os_string());
    let result = run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&output);
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
    let out = result?;
    if !out.status.success() {
        bail!("oxipng failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    bytes.context("reading oxipng output")
}

fn zopflipng(
    data: &[u8],
    settings: &Settings,
    level: u8,
    rerun: bool,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    if find_external("zopflipng").is_none() {
        return engine::oxipng(data, level, settings.strip_png_metadata, true, None);
    }
    let exe = find_external("zopflipng").context("zopflipng not found")?;
    let input = temp_path("png");
    let output = temp_path("png");
    std::fs::write(&input, data)?;
    let mut args: Vec<OsString> = Vec::new();
    if !settings.strip_png_metadata {
        args.push("--keepchunks=tEXt,zTXt,iTXt,gAMA,sRGB,iCCP,bKGD,pHYs,sBIT,tIME,oFFs,acTL,fcTL,fdAT,prVW,mkBF,mkTS,mkBS,mkBT".into());
    } else if crate::filetype::is_apng(data) {
        // Fix over upstream: preserve APNG animation chunks even when stripping metadata.
        args.push("--keepchunks=acTL,fcTL,fdAT".into());
    }
    let mut iterations = 3 + 3 * u64::from(level);
    let mut filters = "0pme";
    // Large PNGs (> 250 KB) get fewer iterations and a single filter.
    if data.len() > 250_000 {
        iterations = 5 + iterations / 3;
        filters = "p";
    }
    let mut limit = timelimit(level, data.len()) as f64;
    if rerun {
        // Upstream alternativeStrategy on re-runs.
        limit *= 1.4;
        filters = "bp";
    } else {
        limit *= 0.8;
    }
    args.push(format!("--filters={filters}").into());
    args.push(format!("--iterations={iterations}").into());
    args.push(format!("--timelimit={}", limit as u64).into());
    args.push("--lossy_transparent".into());
    args.push("-y".into());
    args.push(input.clone().into_os_string());
    args.push(output.clone().into_os_string());
    let result = run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&output);
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
    let out = result?;
    if !out.status.success() {
        bail!("zopflipng failed: {}", String::from_utf8_lossy(&out.stdout));
    }
    bytes.context("reading zopflipng output")
}

fn pngquant(data: &[u8], settings: &Settings, level: u8, cancel: &AtomicBool) -> Result<Vec<u8>> {
    // Upstream PngquantWorker: speed = MIN(3, 7 - level).
    let speed = 3u8.min(7u8.saturating_sub(level));
    if find_external("pngquant").is_none() {
        return engine::pngquant_with_options(
            data,
            settings.png_quality,
            speed,
            settings.strip_png_metadata,
        );
    }
    let exe = find_external("pngquant").context("pngquant not found")?;
    let input = temp_path("png");
    let output = temp_path("png");
    std::fs::write(&input, data)?;
    let max = settings.png_quality.saturating_add(20).min(100);
    let mut args: Vec<OsString> = vec![
        "256".into(),
        "--skip-if-larger".into(),
        format!("-s{speed}").into(),
        "--quality".into(),
        format!("{}-{max}", settings.png_quality).into(),
    ];
    if settings.strip_png_metadata {
        args.push("--strip".into());
    }
    args.extend([
        "--force".into(),
        "--output".into(),
        output.clone().into_os_string(),
        input.clone().into_os_string(),
    ]);
    let result = run_process_cancellable(&exe, &args, cancel, None, false);
    let bytes = std::fs::read(&output);
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
    let out = result?;
    // 98/99 mean "skipped: quality too low / compression too poor" — not errors.
    if matches!(out.status.code(), Some(98 | 99)) {
        return Ok(Vec::new());
    }
    if !out.status.success() {
        bail!("pngquant failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let bytes = bytes.context("reading pngquant output")?;
    if bytes.len() <= 70 {
        bail!("pngquant produced no PNG");
    }
    Ok(bytes)
}

fn svgcleaner(data: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
    let Some(exe) = env_override("svgcleaner") else {
        return engine::svgcleaner(data);
    };
    let input = temp_path("svg");
    std::fs::write(&input, data)?;
    let args: Vec<OsString> = vec![
        "--stdout".into(),
        "--".into(),
        input.clone().into_os_string(),
    ];
    let out = run_process_cancellable(&exe, &args, cancel, None, false);
    let _ = std::fs::remove_file(&input);
    let out = out?;
    if !out.status.success() {
        bail!(
            "svgcleaner failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    #[test]
    fn validate_rejects_truncated_and_corrupt_png() {
        let good = std::fs::read("tests/fixtures/unoptimized.png").unwrap();
        assert!(super::validate_image(&good).is_ok());
        let truncated = &good[..good.len() / 2];
        assert!(super::validate_image(truncated).is_err());
        let mut crc = good.clone();
        let n = crc.len();
        crc[n - 8] ^= 0xff; // corrupt IEND CRC
        assert!(super::validate_image(&crc).is_err());
    }
}
