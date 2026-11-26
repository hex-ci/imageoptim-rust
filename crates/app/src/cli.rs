//! Pure headless CLI mode (`--cli` / `--headless`).
//!
//! Upstream ImageOptim opens a GUI window even when invoked with file arguments
//! (requiring a separate third-party `ImageOptim-CLI` Node wrapper for headless
//! scripts, SSH, or CI). Bare `ImageOptim file.png` keeps upstream's GUI+quit
//! behaviour, while `--cli` / `--headless` runs purely on stdout/stderr with
//! zero window server dependency (also powering `ImageOptimize.appex`).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crossbeam_channel::unbounded;
use imageoptim_core::cache::ResultsCache;
use imageoptim_core::optimize::{self, JobOutcome};
use imageoptim_core::settings::Settings;

use crate::format::{bytes, percent, raw_bytes};

pub fn maybe_run_from_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wants_console = args.iter().any(|a| {
        matches!(
            a.as_str(),
            "--help" | "-h" | "--version" | "-V" | "--cli" | "--headless"
        )
    });
    if wants_console {
        #[cfg(windows)]
        {
            unsafe extern "system" {
                fn AttachConsole(dwProcessId: u32) -> i32;
            }
            const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
            unsafe {
                let _ = AttachConsole(ATTACH_PARENT_PROCESS);
            }
        }
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Some(0);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "ImageOptim-Rust {} (compatible with ImageOptim 1.9.3)",
            imageoptim_core::VERSION
        );
        return Some(0);
    }
    if !args.iter().any(|a| a == "--cli" || a == "--headless") {
        return None;
    }

    let mut settings = Settings::load();
    let mut force_lossy = false;
    let mut force_level: Option<u8> = None;
    let mut need_backup = true;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--cli" | "--headless" => {}
            "--no-backup" => need_backup = false,
            "--lossy" => force_lossy = true,
            "--settings" => {
                if let Some(p) = args.get(i + 1) {
                    settings = Settings::load_from(std::path::Path::new(p));
                    i += 1;
                }
            }
            arg if arg.starts_with("--settings=") => {
                let p = &arg["--settings=".len()..];
                settings = Settings::load_from(std::path::Path::new(p));
            }
            "--level" => {
                if let Some(val) = args.get(i + 1).and_then(|v| v.parse::<u8>().ok()) {
                    force_level = Some(val.min(6));
                    i += 1;
                }
            }
            arg if arg.starts_with("--level=") => {
                if let Ok(val) = arg["--level=".len()..].parse::<u8>() {
                    force_level = Some(val.min(6));
                }
            }
            arg if !arg.starts_with('-') => paths.push(PathBuf::from(arg)),
            other => {
                eprintln!("Unknown option: {other}");
                return Some(2);
            }
        }
        i += 1;
    }
    if force_lossy {
        settings.lossy_enabled = true;
    }
    if let Some(lvl) = force_level {
        settings.level = lvl;
    }

    if paths.is_empty() {
        eprintln!("No input files or directories specified. See --help.");
        return Some(2);
    }

    Some(run_headless(paths, settings, need_backup))
}

fn print_help() {
    let bin = if cfg!(target_os = "linux") {
        "imageoptim-rust"
    } else {
        "ImageOptim-Rust"
    };
    println!(
        "\
ImageOptim-Rust {ver} (unofficial cross-platform Rust port inspired by ImageOptim 1.9.3)

USAGE:
    {bin} [PATHS...]               Open GUI, optimize PATHS, and quit when done
    {bin} --cli [OPTIONS] PATHS... Optimize PATHS headlessly without opening a window

OPTIONS:
    --cli, --headless   Run without a GUI window (for scripts, CI, SSH, extensions)
    --lossy             Enable lossy minification for this run
    --level <0..6>      Override optimization speed/level (0=Fast .. 6=Insane)
    --settings <FILE>   Load preferences from an explicit settings.json path
    --no-backup         Overwrite in place without creating a Trash/scratch backup
    -h, --help          Print this help message
    -V, --version       Print version information",
        ver = imageoptim_core::VERSION
    );
}

fn collect_files(paths: Vec<PathBuf>, exts: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            for entry in walkdir::WalkDir::new(&path)
                .follow_links(false)
                .into_iter()
                .flatten()
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                let ext = entry
                    .path()
                    .extension()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if exts.contains(&ext.as_str()) {
                    files.push(entry.into_path());
                }
            }
        } else {
            files.push(path);
        }
    }
    files
}

fn run_headless(paths: Vec<PathBuf>, settings: Settings, need_backup: bool) -> i32 {
    #[cfg(target_os = "macos")]
    {
        imageoptim_core::optimize::set_trash_handler(crate::macos_open::trash_file);
    }
    let exts = settings.extensions();
    let files = collect_files(paths, &exts);
    if files.is_empty() {
        println!("No matching image files found.");
        return 0;
    }

    let total_files = files.len();
    let workers = settings.concurrent_files.clamp(1, 8).min(total_files);
    let (job_tx, job_rx) = unbounded::<PathBuf>();
    let (res_tx, res_rx) = unbounded::<(PathBuf, JobOutcome)>();
    let cancel = Arc::new(AtomicBool::new(false));

    for f in files {
        let _ = job_tx.send(f);
    }
    drop(job_tx);

    for _ in 0..workers {
        let rx = job_rx.clone();
        let tx = res_tx.clone();
        let st = settings.clone();
        let c = cancel.clone();
        std::thread::spawn(move || {
            if st.low_priority {
                imageoptim_core::set_current_thread_low_priority();
            }
            let cache = ResultsCache::open(&st);
            while let Ok(path) = rx.recv() {
                let outcome = optimize::optimize_path_with_options(
                    &path,
                    &st,
                    &cache,
                    &c,
                    &mut |_| {},
                    &mut |_, _| {},
                    false,
                    need_backup,
                );
                let _ = tx.send((path, outcome));
            }
        });
    }
    drop(job_rx);
    drop(res_tx);

    let mut orig_sum = 0u64;
    let mut opt_sum = 0u64;
    let mut failed = 0usize;

    while let Ok((path, outcome)) = res_rx.recv() {
        let disp = path.display();
        match outcome {
            JobOutcome::Optimized(s) => {
                orig_sum += s.original_size;
                opt_sum += s.optimized_size;
                let pct = if s.original_size > 0 {
                    100.0 - 100.0 * s.optimized_size as f64 / s.original_size as f64
                } else {
                    0.0
                };
                let tool = s.best_tool.unwrap_or_default();
                println!(
                    "✓ {disp}: {} -> {} ({} saved, {tool})",
                    raw_bytes(s.original_size),
                    raw_bytes(s.optimized_size),
                    percent(pct)
                );
            }
            JobOutcome::Noopt { original_size } => {
                orig_sum += original_size;
                opt_sum += original_size;
                println!(
                    "- {disp}: {} (cannot be optimized further)",
                    raw_bytes(original_size)
                );
            }
            JobOutcome::Cached => {
                let sz = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                orig_sum += sz;
                opt_sum += sz;
                println!("- {disp}: {} (cached)", raw_bytes(sz));
            }
            JobOutcome::Cancelled => {
                println!("- {disp}: cancelled");
            }
            JobOutcome::Failed(err) => {
                failed += 1;
                eprintln!("✗ {disp}: {err}");
            }
        }
    }

    if orig_sum > 0 && opt_sum <= orig_sum {
        let saved = orig_sum - opt_sum;
        let pct = 100.0 * saved as f64 / orig_sum as f64;
        println!(
            "Total: saved {} out of {} ({}) across {} file(s).",
            bytes(saved),
            bytes(orig_sum),
            percent(pct),
            total_files
        );
    }

    if failed > 0 { 1 } else { 0 }
}
