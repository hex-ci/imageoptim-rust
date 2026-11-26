//! Persistent preferences.
//!
//! Field names deliberately mirror the `NSUserDefaults` keys used by the
//! original ImageOptim so an existing macOS preferences plist can be imported
//! without a translation table.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(rename = "LossyEnabled")]
    pub lossy_enabled: bool,
    #[serde(rename = "JpegOptimMaxQuality")]
    pub jpeg_quality: u8,
    #[serde(rename = "PngMinQuality")]
    pub png_quality: u8,
    #[serde(rename = "GifQuality")]
    pub gif_quality: u8,

    #[serde(rename = "AdvPngEnabled")]
    pub advpng_enabled: bool,
    /// Reused by every optimizer as the global "Optimization level" (0..=6).
    #[serde(rename = "AdvPngLevel")]
    pub level: u8,

    #[serde(rename = "PngCrush2Enabled")]
    pub pngcrush_enabled: bool,
    #[serde(rename = "PngOutEnabled")]
    pub pngout_enabled: bool,
    #[serde(rename = "OptiPngEnabled")]
    pub oxipng_enabled: bool,
    #[serde(rename = "ZopfliEnabled")]
    pub zopfli_enabled: bool,
    #[serde(rename = "PngOutRemoveChunks")]
    pub strip_png_metadata: bool,

    #[serde(rename = "JpegOptimEnabled")]
    pub jpegoptim_enabled: bool,
    #[serde(rename = "JpegTranEnabled")]
    pub jpegtran_enabled: bool,
    #[serde(rename = "JpegTranStripAll")]
    pub strip_jpeg_metadata: bool,

    #[serde(rename = "GifsicleEnabled")]
    pub gifsicle_enabled: bool,
    #[serde(rename = "SvgoEnabled")]
    pub svgo_enabled: bool,
    #[serde(rename = "SvgcleanerEnabled")]
    pub svgcleaner_enabled: bool,
    #[serde(rename = "GuetzliEnabled")]
    pub guetzli_enabled: bool,

    #[serde(rename = "PreservePermissions")]
    pub preserve_permissions: bool,
    #[serde(rename = "PreserveDates")]
    pub preserve_dates: bool,
    #[serde(rename = "RunLowPriority")]
    pub low_priority: bool,
    #[serde(rename = "BounceDock")]
    pub bounce_dock: bool,

    #[serde(rename = "RunConcurrentFiles")]
    pub concurrent_files: usize,
    #[serde(rename = "RunConcurrentDirscans")]
    pub concurrent_dirscans: usize,
    #[serde(rename = "RunConcurrentFileops")]
    pub concurrent_fileops: usize,
}

impl Default for Settings {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Self {
            lossy_enabled: false,
            jpeg_quality: 80,
            png_quality: 80,
            gif_quality: 80,

            advpng_enabled: true,
            level: 4,
            pngcrush_enabled: false,
            pngout_enabled: cpus > 4 && crate::tools::binary_available("pngout"),
            oxipng_enabled: true,
            zopfli_enabled: true,
            strip_png_metadata: true,

            jpegoptim_enabled: true,
            jpegtran_enabled: true,
            strip_jpeg_metadata: true,

            gifsicle_enabled: true,
            svgo_enabled: false,
            svgcleaner_enabled: true,
            guetzli_enabled: false,

            preserve_permissions: true,
            preserve_dates: false,
            low_priority: true,
            bounce_dock: true,

            concurrent_files: cpus,
            concurrent_dirscans: ((cpus as f64) / 3.9).ceil() as usize,
            concurrent_fileops: 2,
        }
    }
}

/// Canonical per-user project directories (`io.github.hex-ci.ImageOptim-Rust` on macOS,
/// `imageoptim-rust` on Linux/XDG, `hex-ci\ImageOptim-Rust` on Windows).
pub(crate) fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("io.github", "hex-ci", "ImageOptim-Rust")
}

impl Settings {
    pub fn config_dir() -> PathBuf {
        if let Some(custom) = std::env::var_os("IMAGEOPTIM_RUST_CONFIG_DIR")
            .or_else(|| std::env::var_os("IMAGEOPTIM_CONFIG_DIR"))
        {
            return PathBuf::from(custom);
        }
        #[cfg(target_os = "macos")]
        if let Some(user_dirs) = directories::UserDirs::new() {
            let group = user_dirs
                .home_dir()
                .join("Library/Group Containers/group.io.github.hex-ci.imageoptim-rust");
            if group.is_dir() {
                return group;
            }
        }
        project_dirs()
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("settings.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::config_path())
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&Self::config_path())
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, data)
    }

    pub fn png_enabled(&self) -> bool {
        self.pngcrush_enabled
            || self.pngout_enabled
            || self.oxipng_enabled
            || self.advpng_enabled
            || self.zopfli_enabled
            || (self.lossy_enabled && self.png_quality > 30 && self.png_quality < 100)
    }

    pub fn jpeg_enabled(&self) -> bool {
        self.jpegoptim_enabled || self.jpegtran_enabled || self.guetzli_enabled
    }

    pub fn gif_enabled(&self) -> bool {
        self.gifsicle_enabled
    }

    pub fn svg_enabled(&self) -> bool {
        self.svgo_enabled || self.svgcleaner_enabled
    }

    /// Extensions accepted from drag & drop / the open panel, mirroring
    /// `-[FilesController extensions]`.
    pub fn extensions(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.png_enabled() {
            out.extend(["png", "PNG"]);
        }
        if self.jpeg_enabled() {
            out.extend(["jpg", "JPG", "jpeg", "JPEG"]);
        }
        if self.gif_enabled() {
            out.extend(["gif", "GIF"]);
        }
        if self.svg_enabled() {
            out.push("svg");
        }
        out
    }

    pub fn format_summary(&self) -> Option<String> {
        if !self.lossy_enabled {
            return None;
        }
        let mut parts = Vec::new();
        if (1..100).contains(&self.jpeg_quality) {
            parts.push(format!("JPEG {}%", self.jpeg_quality));
        }
        if (1..100).contains(&self.png_quality) {
            parts.push(format!("PNG {}%", self.png_quality));
        }
        if (1..100).contains(&self.gif_quality) {
            parts.push(format!("GIF {}%", self.gif_quality));
        }
        (!parts.is_empty()).then(|| parts.join(", "))
    }
}
