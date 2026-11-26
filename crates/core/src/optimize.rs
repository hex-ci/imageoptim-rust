use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cache::{ResultsCache, is_writable_dir, unique_scratch_path};
use crate::engine;
use crate::filetype::{FileKind, ImageFile};
use crate::settings::Settings;
use crate::tools;

/// Status strings are shared with the UI and are the exact English strings used
/// by the original app so existing translations can be reused.
pub mod status {
    pub const WAITING: &str = "Waiting to be optimized";
    pub const INSPECTING: &str = "Inspecting file";
    pub const NOOPT: &str = "File cannot be optimized any further";
    pub const SAVE_FAILED: &str = "Optimized file could not be saved";
    pub const OPEN_FAILED: &str = "Can't open the file";
    pub const ALL_DISABLED: &str = "All neccessary tools have been disabled in Preferences";
    pub const UNSUPPORTED: &str = "File is neither PNG, GIF nor JPEG";
    pub const REVERTED: &str = "Reverted to original";
}

#[derive(Debug, Clone)]
pub struct ToolStat {
    pub name: String,
    pub old_size: usize,
    pub new_size: usize,
}

/// Upstream `-[Job updateBestToolName:]`: tracks `(oldSize, newSize)` per tool,
/// picks `mostEffective` (highest `oldSize / newSize` ratio) and `smallestFile`
/// (smallest `newSize` below `initial_size`), returning `A` or `A+B` (at most 2 tools).
pub fn format_best_tool(stats: &[ToolStat], initial_size: usize) -> Option<String> {
    let mut best_ratio: f32 = 0.0;
    let mut most_effective: Option<&str> = None;
    let mut smallest_size = initial_size;
    let mut smallest_tool: Option<&str> = None;

    for st in stats {
        if st.new_size == 0 {
            continue;
        }
        let ratio = (st.old_size as f64 / st.new_size as f64) as f32;
        if ratio > best_ratio {
            best_ratio = ratio;
            most_effective = Some(st.name.as_str());
        }
        if st.new_size < smallest_size {
            smallest_size = st.new_size;
            smallest_tool = Some(st.name.as_str());
        }
    }

    match (most_effective, smallest_tool) {
        (Some(a), Some(b)) if a != b => Some(format!("{a}+{b}")),
        (_, Some(b)) => Some(b.to_owned()),
        (Some(a), None) => Some(a.to_owned()),
        (None, None) => None,
    }
}

#[derive(Debug, Default)]
pub struct OptimizeResult {
    pub output: Option<Vec<u8>>,
    pub best_tool: Option<String>,
    pub error: Option<String>,
    pub cached: bool,
    pub output_size: u64,
}

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// macOS installs a handler that moves the original to the Trash (upstream
/// behaviour) and returns the trashed URL; other platforms keep scratch copies.
static TRASH_HANDLER: OnceLock<fn(&Path) -> Option<PathBuf>> = OnceLock::new();

pub fn set_trash_handler(handler: fn(&Path) -> Option<PathBuf>) {
    let _ = TRASH_HANDLER.set(handler);
}

/// Runs the configured engines for one file.
///
/// `on_tool` is called with the display name of each tool just before it runs,
/// matching the original "Started <tool>" status text.
pub fn optimize_bytes(
    data: &[u8],
    kind: FileKind,
    settings: &Settings,
    cancel: &AtomicBool,
    on_tool: &mut dyn FnMut(&str),
    rerun: bool,
) -> OptimizeResult {
    optimize_bytes_with_progress(data, kind, settings, cancel, on_tool, &mut |_, _| {}, rerun)
}

/// Like [`optimize_bytes`], additionally invoking `on_progress(new_size, best_tool)`
/// each time an intermediate candidate reduces the working file size (`wipInput` KVO).
pub fn optimize_bytes_with_progress(
    data: &[u8],
    kind: FileKind,
    settings: &Settings,
    cancel: &AtomicBool,
    on_tool: &mut dyn FnMut(&str),
    on_progress: &mut dyn FnMut(u64, &str),
    rerun: bool,
) -> OptimizeResult {
    let mut result = OptimizeResult::default();
    let mut current = data.to_vec();
    let mut stats: Vec<ToolStat> = Vec::new();
    let record_improvement = |stats: &mut Vec<ToolStat>,
                              current: &mut Vec<u8>,
                              out: Vec<u8>,
                              tool_name: String,
                              on_progress: &mut dyn FnMut(u64, &str)| {
        let old_size = current.len();
        let new_size = out.len();
        *current = out;
        if let Some(existing) = stats.iter_mut().find(|s| s.name == tool_name) {
            existing.old_size = old_size;
            existing.new_size = new_size;
        } else {
            stats.push(ToolStat {
                name: tool_name,
                old_size,
                new_size,
            });
        }
        let best = format_best_tool(stats, data.len()).unwrap_or_default();
        on_progress(new_size as u64, &best);
    };

    macro_rules! cancelled {
        () => {
            if cancel.load(Ordering::Relaxed) {
                return result;
            }
        };
    }
    macro_rules! attempt_named {
        ($tool:expr, $level:expr, $best_name:expr) => {{
            cancelled!();
            if !$tool.is_available() {
                result.error = Some(format!("{} failed to start", $tool.start_name()));
                return result;
            }
            on_tool($tool.name());
            match tools::run_cancellable($tool, &current, settings, $level, rerun, cancel) {
                Ok(Some(out)) if out.len() < current.len() => {
                    record_improvement(&mut stats, &mut current, out, $best_name, on_progress);
                }
                Ok(_) => {}
                Err(e) => {
                    if cancel.load(Ordering::Relaxed) {
                        return result;
                    }
                    log::debug!("{} skipped: {e:#}", $tool.name());
                }
            }
        }};
    }
    macro_rules! attempt {
        ($tool:expr, $level:expr) => {
            attempt_named!($tool, $level, $tool.best_name().to_owned())
        };
    }

    match kind {
        FileKind::Png => {
            // Upstream bumps the level for files that were already optimized.
            let level = settings.level + u8::from(rerun);
            // Fix over upstream: single-frame PNG tools (`pngquant`, `pngcrush`,
            // `pngout`, `advpng`) discard `acTL`/`fcTL`/`fdAT` and silently
            // turn animated PNGs into static single-frame PNGs. Skip them on APNG.
            let apng = crate::filetype::is_apng(data);
            if !apng
                && settings.lossy_enabled
                && settings.png_quality > 30
                && settings.png_quality < 100
            {
                attempt!(tools::Tool::PngQuant, level);
            }
            if !apng && settings.pngcrush_enabled && !(level < 2 && settings.oxipng_enabled) {
                attempt!(tools::Tool::PngCrush, level);
            }
            if settings.oxipng_enabled {
                attempt!(tools::Tool::OxiPng, level);
            }
            if !apng && settings.pngout_enabled && !(level < 4 && settings.zopfli_enabled) {
                attempt!(tools::Tool::PngOut, level);
            }
            if !apng && settings.advpng_enabled && settings.strip_png_metadata {
                attempt!(tools::Tool::AdvPng, level);
            }
            if settings.zopfli_enabled {
                attempt!(tools::Tool::Zopfli, level);
            }
        }
        FileKind::Jpeg => {
            if settings.guetzli_enabled && !rerun && settings.jpeg_quality >= 80 {
                attempt!(tools::Tool::Guetzli, settings.level);
            }
            if settings.jpegoptim_enabled {
                // Lossy passes must save at least 5% to justify the generation
                // loss (upstream JpegoptimWorker).
                cancelled!();
                if !tools::Tool::JpegOptim.is_available() {
                    result.error = Some("JpegOptim failed to start".into());
                    return result;
                }
                on_tool("JpegOptim");
                match tools::run_cancellable(
                    tools::Tool::JpegOptim,
                    &current,
                    settings,
                    settings.level,
                    rerun,
                    cancel,
                ) {
                    Ok(Some(out)) => {
                        let threshold = if settings.lossy_enabled { 95 } else { 100 };
                        if out.len() * 100 < current.len() * threshold {
                            let label = if settings.lossy_enabled {
                                format!("JpegOptim {}%", settings.jpeg_quality)
                            } else {
                                "JpegOptim".into()
                            };
                            record_improvement(&mut stats, &mut current, out, label, on_progress);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if cancel.load(Ordering::Relaxed) {
                            return result;
                        }
                        log::debug!("JpegOptim skipped: {e:#}");
                    }
                }
            }
            if settings.jpegtran_enabled {
                attempt!(tools::Tool::JpegTran, settings.level);
            }
        }
        FileKind::Gif => {
            if !settings.gifsicle_enabled {
                // Nothing to do; the caller reports "all tools disabled".
            } else {
                if !tools::Tool::Gifsicle.is_available() {
                    result.error = Some("Gifsicle failed to start".into());
                    return result;
                }
                let want_frames = crate::tools::gif_frame_count(data);
                if settings.lossy_enabled && settings.gif_quality > 30 && settings.gif_quality < 100
                {
                    cancelled!();
                    on_tool("Gifsicle");
                    match tools::run_cancellable(
                        tools::Tool::Gifsicle,
                        &current,
                        settings,
                        settings.level,
                        rerun,
                        cancel,
                    ) {
                        Ok(Some(out)) => {
                            // Upstream requires ≥ 5% + (100-quality)/2 % gain.
                            let factor = 105 + (100 - u64::from(settings.gif_quality)) / 2;
                            if out.len() as u64 * factor / 100 < current.len() as u64 {
                                record_improvement(
                                    &mut stats,
                                    &mut current,
                                    out,
                                    "Giflossy".into(),
                                    on_progress,
                                );
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            if cancel.load(Ordering::Relaxed) {
                                return result;
                            }
                            log::debug!("Giflossy skipped: {e:#}");
                        }
                    }
                } else {
                    cancelled!();
                    on_tool("Gifsicle");
                    match engine::gifsicle_data_cancellable(&current, 100, false, cancel) {
                        Ok(out)
                            if out.len() > 70
                                && out.len() < current.len()
                                && crate::tools::validate_image(&out).is_ok()
                                && (want_frames.is_none()
                                    || crate::tools::gif_frame_count(&out) == want_frames) =>
                        {
                            record_improvement(
                                &mut stats,
                                &mut current,
                                out,
                                "Gifsicle".into(),
                                on_progress,
                            );
                        }
                        Ok(_) => {}
                        Err(e) => {
                            if cancel.load(Ordering::Relaxed) {
                                return result;
                            }
                            log::debug!("Gifsicle skipped: {e:#}");
                        }
                    }
                    if settings.level > 1 {
                        cancelled!();
                        on_tool("Gifsicle interlaced");
                        match engine::gifsicle_data_cancellable(&current, 100, true, cancel) {
                            Ok(out)
                                if out.len() > 70
                                    && out.len() < current.len()
                                    && crate::tools::validate_image(&out).is_ok()
                                    && (want_frames.is_none()
                                        || crate::tools::gif_frame_count(&out) == want_frames) =>
                            {
                                record_improvement(
                                    &mut stats,
                                    &mut current,
                                    out,
                                    "Gifsicle interlaced".into(),
                                    on_progress,
                                );
                            }
                            Ok(_) => {}
                            Err(e) => {
                                if cancel.load(Ordering::Relaxed) {
                                    return result;
                                }
                                log::debug!("Gifsicle interlaced skipped: {e:#}");
                            }
                        }
                    }
                }
            }
        }
        FileKind::Svg => {
            if settings.svgo_enabled {
                let svgo_label = if settings.lossy_enabled {
                    "SVGO".to_owned()
                } else {
                    "SVGO lite".to_owned()
                };
                attempt_named!(tools::Tool::Svgo, settings.level, svgo_label);
            }
            if settings.svgcleaner_enabled {
                attempt!(tools::Tool::SvgCleaner, settings.level);
            }
        }
        FileKind::Unknown => {
            result.error = Some(status::UNSUPPORTED.into());
            return result;
        }
    }

    if !stats.is_empty() && current.len() < data.len() {
        result.output_size = current.len() as u64;
        result.best_tool = format_best_tool(&stats, data.len());
        result.output = Some(current);
    }
    result
}

#[derive(Debug, Clone)]
pub struct SavedFile {
    pub path: PathBuf,
    pub original_size: u64,
    pub optimized_size: u64,
    pub best_tool: Option<String>,
    pub revert: Option<PathBuf>,
}

#[derive(Debug)]
pub enum JobOutcome {
    Optimized(SavedFile),
    Noopt { original_size: u64 },
    Cached,
    Cancelled,
    Failed(String),
}

pub fn optimize_path(
    path: &Path,
    settings: &Settings,
    cache: &ResultsCache,
    cancel: &AtomicBool,
    on_status: &mut dyn FnMut(&str),
    rerun: bool,
) -> JobOutcome {
    optimize_path_with_options(
        path,
        settings,
        cache,
        cancel,
        on_status,
        &mut |_, _| {},
        rerun,
        true,
    )
}

/// Full pipeline entry point used by the GUI worker pool: emits live intermediate
/// progress (`on_progress`) and skips creating a duplicate revert backup when the
/// row already holds the original backup from an earlier pass (`need_backup = false`).
#[allow(clippy::too_many_arguments)]
pub fn optimize_path_with_options(
    path: &Path,
    settings: &Settings,
    cache: &ResultsCache,
    cancel: &AtomicBool,
    on_status: &mut dyn FnMut(&str),
    on_progress: &mut dyn FnMut(u64, &str),
    rerun: bool,
    need_backup: bool,
) -> JobOutcome {
    let img = match ImageFile::read(path) {
        Ok(f) => f,
        Err(e) => return JobOutcome::Failed(format!("{}: {e}", status::OPEN_FAILED)),
    };
    if img.kind == FileKind::Unknown {
        return JobOutcome::Failed(status::UNSUPPORTED.into());
    }
    let any_enabled = match img.kind {
        FileKind::Png => settings.png_enabled(),
        FileKind::Jpeg => settings.jpeg_enabled(),
        FileKind::Gif => settings.gif_enabled(),
        FileKind::Svg => settings.svg_enabled(),
        FileKind::Unknown => false,
    };
    if !any_enabled {
        return JobOutcome::Failed(status::ALL_DISABLED.into());
    }
    if img.kind == FileKind::Gif && engine::find_gifsicle().is_none() {
        return JobOutcome::Failed("Gifsicle failed to start".into());
    }
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => return JobOutcome::Failed(format!("{}: {e}", status::OPEN_FAILED)),
    };
    if data.len() < 30 {
        return JobOutcome::Failed(status::UNSUPPORTED.into());
    }
    if !is_writable_dir(path) {
        return JobOutcome::Failed(status::SAVE_FAILED.into());
    }

    let hash = cache.file_hash(&data, rerun);
    if cache.contains(&hash) {
        return JobOutcome::Cached;
    }

    on_status(status::INSPECTING);
    let result = optimize_bytes_with_progress(
        &data,
        img.kind,
        settings,
        cancel,
        on_status,
        on_progress,
        rerun,
    );
    if cancel.load(Ordering::Relaxed) {
        return JobOutcome::Cancelled;
    }
    if let Some(err) = result.error {
        return JobOutcome::Failed(err);
    }
    let Some(output) = result.output else {
        if let Err(e) = tools::validate_image(&data) {
            return JobOutcome::Failed(format!("{}: {e}", status::OPEN_FAILED));
        }
        cache.remember_unoptimizable(&hash, img.size);
        return JobOutcome::Noopt {
            original_size: img.size,
        };
    };
    // Upstream refuses to save outputs smaller than 30 bytes (Job.m).
    if output.len() < 30 {
        return JobOutcome::Failed(status::SAVE_FAILED.into());
    }
    // Final integrity gate before touching the original file.
    if let Err(e) = crate::tools::validate_image(&output) {
        log::warn!("refusing corrupt output: {e:#}");
        return JobOutcome::Failed(status::SAVE_FAILED.into());
    }

    let _file_slot = file_slots(settings.concurrent_fileops).acquire();
    match save_output_inner(path, &output, settings, need_backup) {
        Ok(revert) => JobOutcome::Optimized(SavedFile {
            path: path.to_path_buf(),
            original_size: img.size,
            optimized_size: output.len() as u64,
            best_tool: result.best_tool,
            revert,
        }),
        Err(e) => JobOutcome::Failed(format!("{}: {e}", status::SAVE_FAILED)),
    }
}

/// Replaces the file with `bytes`, keeping a revert copy of the original.
///
/// With `preserve_permissions` the existing inode is overwritten in place so
/// hard links, ownership and extended attributes survive; otherwise a temp file
/// is renamed over the original.
pub fn save_output(path: &Path, bytes: &[u8], settings: &Settings) -> Result<Option<PathBuf>> {
    save_output_inner(path, bytes, settings, true)
}

/// Upstream `Job.m`: skip in-place overwrite inside Dropbox folders to avoid
/// triggering Dropbox sync truncation bugs.
fn is_inside_dropbox(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/Dropbox/") || s.contains("\\Dropbox\\") {
        return true;
    }
    path.ancestors()
        .skip(1)
        .any(|dir| dir.join(".dropbox").exists())
}

fn save_output_inner(
    path: &Path,
    bytes: &[u8],
    settings: &Settings,
    need_backup: bool,
) -> Result<Option<PathBuf>> {
    // Resolve symlinks first so temp-file + rename replaces the symlink target
    // rather than destroying the symbolic link itself.
    let resolved = if std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let path = resolved.as_path();
    let original_meta = std::fs::metadata(path)?;
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    // Upstream puts the original in the Trash only when `revertFile` is nil;
    // we trash a *copy* so the live inode (hardlinks, permissions) survives.
    let backup = if need_backup {
        Some(match TRASH_HANDLER.get() {
            Some(h) => {
                let sibling = path.with_file_name(format!(".{file_name}~imageoptim-rust-orig"));
                std::fs::copy(path, &sibling)
                    .with_context(|| format!("backing up {}", path.display()))?;
                match h(&sibling) {
                    Some(trashed) => trashed,
                    None => {
                        let b = unique_scratch_path(&file_name);
                        std::fs::rename(&sibling, &b).or_else(|_| {
                            std::fs::copy(&sibling, &b)?;
                            std::fs::remove_file(&sibling)
                        })?;
                        b
                    }
                }
            }
            None => {
                let b = unique_scratch_path(&file_name);
                std::fs::copy(path, &b)
                    .with_context(|| format!("backing up {}", path.display()))?;
                b
            }
        })
    } else {
        None
    };

    let in_dropbox = is_inside_dropbox(path);
    let result = (|| -> Result<()> {
        if settings.preserve_permissions && !in_dropbox {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(path)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        } else {
            let tmp = path.with_file_name(format!(".{file_name}~imageoptim-rust"));
            std::fs::write(&tmp, bytes)?;
            let _ = std::fs::set_permissions(&tmp, original_meta.permissions());
            #[cfg(unix)]
            copy_xattrs(path, &tmp);
            #[cfg(target_os = "macos")]
            copy_acl(path, &tmp);
            #[cfg(windows)]
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path)?;
        }
        #[cfg(unix)]
        strip_unwanted_xattrs(path);
        if settings.preserve_dates {
            let atime = filetime::FileTime::from_last_access_time(&original_meta);
            let mtime = filetime::FileTime::from_last_modification_time(&original_meta);
            let _ = filetime::set_file_times(path, atime, mtime);
            #[cfg(target_os = "macos")]
            if let Ok(created) = original_meta.created() {
                let _ = set_creation_time(path, created);
            }
        }
        Ok(())
    })();

    match result {
        Ok(()) => Ok(backup),
        Err(e) => {
            // The live file was never touched on failure; drop any new backup.
            if let Some(b) = &backup {
                let _ = std::fs::remove_file(b);
            }
            Err(e)
        }
    }
}

pub fn revert(path: &Path, backup: &Path) -> Result<()> {
    let current = std::fs::metadata(path)?.len();
    let backup_meta = std::fs::metadata(backup)?;
    if current == 0 || backup_meta.len() == 0 {
        anyhow::bail!("invalid revert copy");
    }
    // Copy back (not rename) so the live inode and hardlinks survive.
    std::fs::copy(backup, path)?;
    let _ = std::fs::remove_file(backup);
    let _ = filetime::set_file_mtime(
        path,
        filetime::FileTime::from_last_modification_time(&backup_meta),
    );
    Ok(())
}

/// APFS/HFS+ birthtime cannot be set through `filetime`; use setattrlist.
#[cfg(target_os = "macos")]
fn set_creation_time(path: &Path, time: std::time::SystemTime) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(since) = time.duration_since(std::time::UNIX_EPOCH) else {
        return Ok(());
    };
    let cpath = CString::new(path.as_os_str().as_bytes())?;
    let mut attrs = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_CRTIME,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut ts = libc::timespec {
        tv_sec: since.as_secs() as libc::time_t,
        tv_nsec: since.subsec_nanos() as libc::c_long,
    };
    let rc = unsafe {
        libc::setattrlist(
            cpath.as_ptr(),
            &mut attrs as *mut libc::attrlist as *mut libc::c_void,
            &mut ts as *mut libc::timespec as *mut libc::c_void,
            std::mem::size_of::<libc::timespec>(),
            0,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Upstream `-[Job removeExtendedAttrAtURL:]`: strip Finder icon/resource forks
/// and browser quarantine/origin xattrs from saved images while preserving custom xattrs.
#[cfg(unix)]
const STRIP_XATTRS: &[&str] = &[
    "com.apple.FinderInfo",
    "com.apple.ResourceFork",
    "com.apple.quarantine",
    "com.apple.metadata:kMDItemWhereFroms",
];

#[cfg(unix)]
fn strip_unwanted_xattrs(path: &Path) {
    for name in STRIP_XATTRS {
        let _ = xattr::remove(path, name);
    }
}

#[cfg(unix)]
fn copy_xattrs(src: &Path, dst: &Path) {
    let Ok(names) = xattr::list(src) else {
        return;
    };
    for name in names {
        if STRIP_XATTRS.iter().any(|s| name == std::ffi::OsStr::new(s)) {
            continue;
        }
        if let Ok(Some(value)) = xattr::get(src, &name) {
            let _ = xattr::set(dst, &name, &value);
        }
    }
}

#[cfg(target_os = "macos")]
fn copy_acl(src: &Path, dst: &Path) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn copyfile(
            from: *const libc::c_char,
            to: *const libc::c_char,
            state: *mut libc::c_void,
            flags: libc::c_uint,
        ) -> libc::c_int;
    }
    const COPYFILE_ACL: libc::c_uint = 1;
    if let (Ok(src), Ok(dst)) = (
        CString::new(src.as_os_str().as_bytes()),
        CString::new(dst.as_os_str().as_bytes()),
    ) {
        unsafe {
            let _ = copyfile(
                src.as_ptr(),
                dst.as_ptr(),
                std::ptr::null_mut(),
                COPYFILE_ACL,
            );
        }
    }
}

/// Bounds concurrent save/replace operations (`RunConcurrentFileops`).
struct FileSlots {
    free: std::sync::Mutex<usize>,
    ready: std::sync::Condvar,
}

impl FileSlots {
    fn acquire(&'static self) -> FileSlot {
        let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
        while *free == 0 {
            free = self.ready.wait(free).unwrap_or_else(|e| e.into_inner());
        }
        *free -= 1;
        FileSlot(self)
    }
}

struct FileSlot(&'static FileSlots);

impl Drop for FileSlot {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        self.0.ready.notify_one();
    }
}

fn file_slots(count: usize) -> &'static FileSlots {
    static SLOTS: std::sync::OnceLock<FileSlots> = std::sync::OnceLock::new();
    SLOTS.get_or_init(|| FileSlots {
        free: std::sync::Mutex::new(count.max(1)),
        ready: std::sync::Condvar::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_tool_caps_at_most_effective_plus_smallest() {
        // Single tool
        let stats = vec![ToolStat {
            name: "OxiPNG".into(),
            old_size: 10_000,
            new_size: 6_000,
        }];
        assert_eq!(format_best_tool(&stats, 10_000).as_deref(), Some("OxiPNG"));

        // Four tools improved sequentially: Pngcrush (small), OxiPNG (biggest ratio),
        // AdvPNG (tiny), Zopfli (smallest final size). Upstream displays "OxiPNG+Zopfli".
        let stats = vec![
            ToolStat {
                name: "Pngcrush".into(),
                old_size: 10_000,
                new_size: 9_800,
            },
            ToolStat {
                name: "OxiPNG".into(),
                old_size: 9_800,
                new_size: 5_000,
            },
            ToolStat {
                name: "AdvPNG".into(),
                old_size: 5_000,
                new_size: 4_950,
            },
            ToolStat {
                name: "Zopfli".into(),
                old_size: 4_950,
                new_size: 4_900,
            },
        ];
        assert_eq!(
            format_best_tool(&stats, 10_000).as_deref(),
            Some("OxiPNG+Zopfli")
        );

        // Last tool also has the highest ratio -> single name
        let stats = vec![
            ToolStat {
                name: "Pngcrush".into(),
                old_size: 10_000,
                new_size: 9_900,
            },
            ToolStat {
                name: "Zopfli".into(),
                old_size: 9_900,
                new_size: 5_000,
            },
        ];
        assert_eq!(format_best_tool(&stats, 10_000).as_deref(), Some("Zopfli"));
    }

    #[test]
    fn dropbox_path_detection() {
        assert!(is_inside_dropbox(Path::new("/Users/alice/Dropbox/pic.png")));
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("pic.png");
        assert!(!is_inside_dropbox(&plain));
        std::fs::create_dir_all(dir.path().join(".dropbox")).unwrap();
        let sub = dir.path().join("sub/pic.png");
        assert!(is_inside_dropbox(&sub));
    }
}
