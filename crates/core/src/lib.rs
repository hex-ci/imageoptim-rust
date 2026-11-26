//! Core model and optimization pipeline for ImageOptim-Rust.
//!
//! The crate is UI-agnostic: it can be driven by the egui desktop app, the headless CLI, or tests.

// Engine setup mirrors the upstream CLI flags one-for-one; splitting every
// option into an initializer would hide that correspondence.
#![allow(clippy::collapsible_if, clippy::field_reassign_with_default)]

pub mod cache;
pub mod engine;
pub mod filetype;
pub mod optimize;
pub mod settings;
pub mod svgo;
pub mod tools;

pub use filetype::{FileKind, ImageFile};
pub use optimize::{
    JobOutcome, OptimizeResult, SavedFile, optimize_bytes, optimize_bytes_with_progress,
    optimize_path, optimize_path_with_options,
};
pub use settings::Settings;

/// The app version comes from Cargo so the About box can never go stale.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Match upstream `RunLowPriority` for worker threads. macOS exposes per-thread
/// QoS classes; other Unix platforms lower `nice` priority.
pub fn set_current_thread_low_priority() {
    #[cfg(target_os = "macos")]
    unsafe {
        // QOS_CLASS_UTILITY
        unsafe extern "C" {
            fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
        }
        let _ = pthread_set_qos_class_self_np(0x11, 0);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    unsafe {
        let _ = libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}
