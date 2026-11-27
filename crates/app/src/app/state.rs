use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};

use imageoptim_core::filetype::FileKind;
use imageoptim_core::optimize::JobOutcome;
use imageoptim_core::settings::Settings;

use crate::i18n;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum RowStatus {
    Waiting,
    Inspecting,
    Running(String),
    Done,
    Noopt,
    Reverted,
    Failed(String),
}

#[derive(Debug, Clone)]
pub(super) struct Row {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,
    pub kind: FileKind,
    pub original_size: u64,
    pub optimized_size: Option<u64>,
    pub status: RowStatus,
    pub best_tool: Option<String>,
    pub revert: Option<PathBuf>,
    pub busy: bool,
    pub done: bool,
    pub failed: bool,
    pub optimized: bool,
    pub has_run_before: bool,
}

impl Row {
    pub fn percent_saved(&self) -> Option<f64> {
        let opt = self.optimized_size?;
        if self.original_size == 0 {
            return None;
        }
        Some((100.0 - 100.0 * opt as f64 / self.original_size as f64).max(0.0))
    }

    pub fn status_text(&self) -> String {
        match &self.status {
            RowStatus::Waiting => i18n::t("Waiting to be optimized", "等待被优化").into(),
            RowStatus::Inspecting => i18n::t("Inspecting file", "正在检查文件").into(),
            RowStatus::Running(tool) => i18n::t("Started {}", "开始 {}").replacen("{}", tool, 1),
            RowStatus::Done => match &self.best_tool {
                Some(tool) => i18n::t("Optimized successfully with {}", "优化成功于 {}")
                    .replacen("{}", tool, 1),
                None => i18n::t("Optimized successfully", "优化成功").into(),
            },
            RowStatus::Noopt => i18n::t(
                "File cannot be optimized any further",
                "文件不能被进一步优化",
            )
            .into(),
            RowStatus::Reverted => i18n::t("Reverted to original", "已恢复原状").into(),
            RowStatus::Failed(msg) => localize_error(msg),
        }
    }

    /// Upstream `statusOrder` sort values (`Job.m`).
    pub fn status_order(&self) -> u8 {
        match self.status {
            RowStatus::Waiting => 0,
            RowStatus::Inspecting => 3,
            RowStatus::Running(_) => 4,
            RowStatus::Noopt => 5,
            RowStatus::Reverted => 6,
            RowStatus::Done => 7,
            RowStatus::Failed(_) => 9,
        }
    }

    pub fn stop_icon(&self) -> &'static str {
        match self.status {
            RowStatus::Waiting => "wait",
            RowStatus::Inspecting | RowStatus::Running(_) => "progress",
            RowStatus::Done => "ok",
            RowStatus::Noopt | RowStatus::Reverted => "noopt",
            RowStatus::Failed(_) => "err",
        }
    }

    pub fn sync_disk_size(&mut self, disk_size: u64) {
        self.optimized_size = Some(disk_size);
        self.optimized = disk_size < self.original_size;
    }
}

/// Row error tooltips use the upstream localized strings; details stay in the log.
pub(super) fn localize_error(msg: &str) -> String {
    if msg == "File is neither PNG, GIF nor JPEG"
        || msg.starts_with("File is neither PNG, GIF nor JPEG:")
    {
        if i18n::is_chinese() {
            return "文件不是 PNG、JPEG、GIF 或 SVG 格式".into();
        }
        let translated = i18n::t("File is neither PNG, GIF nor JPEG", "");
        if translated != "File is neither PNG, GIF nor JPEG" && !translated.is_empty() {
            return translated.into();
        }
        return "File is neither PNG, JPEG, GIF nor SVG".into();
    }
    const KEYS: [(&str, &str); 3] = [
        ("Can't open the file", "无法打开该文件"),
        ("Optimized file could not be saved", "优化的文件未能保存"),
        (
            "All neccessary tools have been disabled in Preferences",
            "在首选项中已禁用所有必要工具",
        ),
    ];
    for (en, zh) in KEYS {
        if msg == en || msg.starts_with(&format!("{en}:")) {
            return i18n::t(en, zh).into();
        }
    }
    if let Some(tool) = msg.strip_suffix(" failed to start") {
        return i18n::t("{} failed to start", "{} 启动失败").replacen("{}", tool, 1);
    }
    msg.to_owned()
}

pub(super) fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        i18n::t("Show in Finder", "在 Finder 中显示")
    } else if cfg!(target_os = "windows") {
        if i18n::is_chinese() {
            "在资源管理器中显示"
        } else {
            "Show in Explorer"
        }
    } else if i18n::is_chinese() {
        "在文件管理器中显示"
    } else {
        "Show in File Manager"
    }
}

pub(super) struct JobRequest {
    pub id: u64,
    pub path: PathBuf,
    pub settings: Settings,
    pub cancel: Arc<AtomicBool>,
    pub rerun: bool,
    pub need_backup: bool,
}

#[derive(Debug)]
pub(super) enum Evt {
    Started(u64),
    Tool(u64, String),
    Progress {
        id: u64,
        optimized_size: u64,
        best_tool: String,
    },
    Finished(u64, JobOutcome),
    ScanBatch(Vec<PathBuf>),
    ScanFinished,
}

#[derive(Clone)]
pub(super) enum UndoAction {
    Added(Vec<Row>),
    Removed(Vec<(usize, Row)>),
}

/// Upstream registers an undo action name for Add/Cut only; plain Delete keeps
/// the previous menu title.
#[derive(Clone)]
pub(super) struct UndoEntry {
    pub name: Option<&'static str>,
    pub action: UndoAction,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SortKey {
    Status,
    Name,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Col {
    Status,
    File,
    Size,
    Savings,
    Original,
    Best,
}

/// Persisted like `NSTableView` "Files" autosave (visibility, order, widths,
/// and window rectangles).
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct UiState {
    pub show_size: bool,
    pub show_savings: bool,
    pub show_original_size: bool,
    pub show_best_tool: bool,
    pub col_original: f32,
    pub col_best: f32,
    #[serde(default)]
    pub col_order: Vec<String>,
    #[serde(default)]
    pub col_status: f32,
    #[serde(default)]
    pub col_file: f32,
    #[serde(default)]
    pub col_size: f32,
    #[serde(default)]
    pub col_savings: f32,
    #[serde(default)]
    pub main_window: Option<[f32; 4]>,
    #[serde(default)]
    pub prefs_window: Option<[f32; 4]>,
}

pub(super) fn reveal_in_file_manager(path: &std::path::Path) {
    log::info!("reveal {}", path.display());
    #[cfg(target_os = "macos")]
    crate::macos_open::reveal_file(path);
    #[cfg(not(target_os = "macos"))]
    showfile::show_path_in_file_manager(path);
}

pub(super) fn col_from_name(name: &str) -> Option<Col> {
    match name {
        "status" => Some(Col::Status),
        "file" => Some(Col::File),
        "size" => Some(Col::Size),
        "savings" => Some(Col::Savings),
        "original" => Some(Col::Original),
        "best" => Some(Col::Best),
        _ => None,
    }
}

pub(super) fn col_name(col: Col) -> &'static str {
    match col {
        Col::Status => "status",
        Col::File => "file",
        Col::Size => "size",
        Col::Savings => "savings",
        Col::Original => "original",
        Col::Best => "best",
    }
}

/// Extracts a valid `[x, y, width, height]` window rect from egui viewport bounds.
pub(super) fn valid_window_rect(
    outer: Option<eframe::egui::Rect>,
    inner: Option<eframe::egui::Rect>,
) -> Option<[f32; 4]> {
    let (outer, inner) = (outer?, inner?);
    let [x, y, w, h] = [outer.min.x, outer.min.y, inner.width(), inner.height()];
    (x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite() && w >= 100.0 && h >= 80.0)
        .then_some([x, y, w, h])
}

/// Persists `ui-state.json`, creating parent configuration directories if absent.
pub(super) fn write_ui_state_to(path: &std::path::Path, json: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)
}

/// Reads the saved main-window frame for the initial viewport (`main.rs`).
pub fn saved_main_window() -> Option<[f32; 4]> {
    let text = std::fs::read_to_string(Settings::config_dir().join("ui-state.json")).ok()?;
    let [x, y, w, h] = serde_json::from_str::<UiState>(&text).ok()?.main_window?;
    (x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite() && w >= 100.0 && h >= 80.0)
        .then_some([x, y, w, h])
}

pub(super) fn col_range(col: Col) -> Option<(f32, f32)> {
    match col {
        Col::File => Some((100.0, 1000.0)),
        Col::Original => Some((60.0, 120.0)),
        Col::Best => Some((40.0, 250.0)),
        _ => None,
    }
}

pub(super) struct Preview {
    pub title: String,
    pub info: String,
    pub texture: Option<eframe::egui::TextureHandle>,
    pub text: Option<String>,
    pub path: PathBuf,
}

pub(super) enum RowAction {
    Select(usize, eframe::egui::Modifiers),
    SelectContext(usize),
    Reveal(PathBuf),
    Reorder(usize, usize),
    BeginExternalDrag(usize),
    QuickLook,
    Again,
    Stop(usize),
    Revert(usize),
    Copy,
    CopyDataUrl,
    Remove(usize),
}

/// Counting semaphore for directory scans; keeps `RunConcurrentDirscans`
/// bounded without pulling in an async runtime.
pub(super) struct ScanSlots {
    free: Mutex<usize>,
    ready: Condvar,
}

impl ScanSlots {
    pub fn new(count: usize) -> Self {
        Self {
            free: Mutex::new(count.max(1)),
            ready: Condvar::new(),
        }
    }

    pub fn acquire(self: &Arc<Self>) -> ScanSlot {
        let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
        while *free == 0 {
            free = self.ready.wait(free).unwrap_or_else(|e| e.into_inner());
        }
        *free -= 1;
        ScanSlot(self.clone())
    }
}

pub(super) struct ScanSlot(Arc<ScanSlots>);

impl Drop for ScanSlot {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        self.0.ready.notify_one();
    }
}
