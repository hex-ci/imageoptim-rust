mod menus;
mod state;
mod table;
mod windows;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use eframe::egui;
use egui::pos2;
use imageoptim_core::cache::ResultsCache;
use imageoptim_core::filetype::FileKind;
use imageoptim_core::optimize::{self, JobOutcome};
use imageoptim_core::settings::Settings;

use self::state::{
    Col, Evt, JobRequest, Preview, Row, RowStatus, ScanSlots, SortKey, UiState, UndoAction,
    UndoEntry, col_from_name, col_name, col_range, valid_window_rect, write_ui_state_to,
};
use self::table::adjust_pair;
use crate::format::{bytes, percent};
use crate::i18n;
use crate::icons::Icons;

pub use self::state::saved_main_window;

pub struct App {
    settings: Settings,
    rows: Vec<Row>,
    selected: BTreeSet<u64>,
    anchor: Option<u64>,
    next_id: u64,

    job_tx: Sender<JobRequest>,
    evt_rx: Receiver<Evt>,
    evt_tx: Sender<Evt>,
    cancel: Arc<std::sync::Mutex<HashMap<u64, Arc<AtomicBool>>>>,
    pending: usize,
    active_scans: usize,
    was_busy: bool,

    overall_avg: bool,

    icons: Icons,
    app_icon: Option<egui::TextureHandle>,

    show_prefs: bool,
    prefs_tab: usize,
    show_about: bool,
    preview: Option<Preview>,

    show_size: bool,
    show_savings: bool,
    show_original_size: bool,
    show_best_tool: bool,
    scan_slots: Arc<ScanSlots>,
    table_width: f32,
    table_columns: usize,
    col_status: f32,
    col_file: f32,
    col_size: f32,
    col_savings: f32,
    col_original: f32,
    col_best: f32,
    col_order: Vec<Col>,
    main_live: Option<egui::Rect>,
    main_inner: Option<egui::Rect>,
    prefs_live: Option<egui::Rect>,
    prefs_inner: Option<egui::Rect>,
    main_saved: Option<[f32; 4]>,
    prefs_saved: Option<[f32; 4]>,
    ui_write_at: Instant,
    last_cursor: egui::CursorIcon,
    qa_steps: Vec<String>,
    qa_i: usize,
    qa_wait: u32,
    qa_done_logged: bool,
    qa_expect: Option<String>,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    quiet: bool,
    sort: Option<(SortKey, bool)>,

    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,

    drop_hover: bool,
    external_drag_started: bool,
    quit_when_done: bool,
    launch_paths: Vec<PathBuf>,

    guetzli_warned: bool,
    guetzli_warn_pending: bool,
    strip_jpeg_before_guetzli: Option<bool>,
    last_settings_json: String,
    ui_state_json: String,
    #[cfg(target_os = "macos")]
    menu_snapshot: Option<(Vec<i32>, String, String)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, launch_paths: Vec<PathBuf>) -> Self {
        #[cfg(target_os = "macos")]
        {
            crate::macos_open::set_context(cc.egui_ctx.clone());
            crate::macos_open::install();
            imageoptim_core::optimize::set_trash_handler(crate::macos_open::trash_file);
        }
        Self::build(&cc.egui_ctx, launch_paths)
    }

    fn build(ctx: &egui::Context, launch_paths: Vec<PathBuf>) -> Self {
        let qa_prefs = std::env::var("IMAGEOPTIM_QA_PREFS").ok();
        let settings = Settings::load();
        let last_settings_json = serde_json::to_string(&settings).unwrap_or_default();
        let icons = Icons::load(ctx);
        let app_icon = load_texture(ctx, "app-icon", include_bytes!("../../assets/app-icon.png"));

        let (job_tx, job_rx) = unbounded::<JobRequest>();
        let (evt_tx, evt_rx) = unbounded::<Evt>();
        let workers = settings.concurrent_files.clamp(1, 8);
        let low_priority = settings.low_priority;
        let scan_slots = Arc::new(ScanSlots::new(settings.concurrent_dirscans.max(1)));
        for _ in 0..workers {
            let rx = job_rx.clone();
            let tx = evt_tx.clone();
            std::thread::spawn(move || {
                if low_priority {
                    imageoptim_core::set_current_thread_low_priority();
                }
                while let Ok(req) = rx.recv() {
                    run_job(req, &tx);
                }
            });
        }
        drop(job_rx);

        let mut app = Self {
            settings,
            rows: Vec::new(),
            selected: BTreeSet::new(),
            anchor: None,
            next_id: 1,
            job_tx,
            evt_rx,
            evt_tx,
            cancel: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending: 0,
            active_scans: 0,
            was_busy: false,
            overall_avg: false,
            icons,
            app_icon,
            show_prefs: qa_prefs.is_some(),
            prefs_tab: qa_prefs
                .as_deref()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            show_about: std::env::var_os("IMAGEOPTIM_QA_ABOUT").is_some(),
            preview: None,
            show_size: true,
            show_savings: true,
            show_original_size: false,
            show_best_tool: false,
            scan_slots,
            table_width: 0.0,
            table_columns: 0,
            col_status: 22.0,
            col_file: 316.0,
            col_size: 85.0,
            col_savings: 85.0,
            col_original: 85.0,
            col_best: 85.0,
            col_order: vec![
                Col::Status,
                Col::File,
                Col::Size,
                Col::Savings,
                Col::Original,
                Col::Best,
            ],
            main_live: None,
            main_inner: None,
            prefs_live: None,
            prefs_inner: None,
            main_saved: None,
            prefs_saved: None,
            ui_write_at: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            last_cursor: egui::CursorIcon::Default,
            qa_steps: std::env::var("IMAGEOPTIM_QA_INPUT")
                .map(|v| v.split(';').map(str::to_owned).collect())
                .unwrap_or_default(),
            qa_i: 0,
            qa_wait: 0,
            qa_done_logged: false,
            qa_expect: std::env::var("IMAGEOPTIM_QA_EXPECT").ok(),
            quiet: std::env::var_os("IMAGEOPTIM_QUIET").is_some(),
            sort: None,
            undo: Vec::new(),
            redo: Vec::new(),
            drop_hover: false,
            external_drag_started: false,
            quit_when_done: !launch_paths.is_empty()
                && std::env::var_os("IMAGEOPTIM_STAY").is_none(),
            launch_paths,
            guetzli_warned: false,
            guetzli_warn_pending: false,
            strip_jpeg_before_guetzli: None,
            last_settings_json,
            ui_state_json: String::new(),
            #[cfg(target_os = "macos")]
            menu_snapshot: None,
        };
        if !cfg!(test)
            && let Ok(text) = std::fs::read_to_string(Settings::config_dir().join("ui-state.json"))
            && let Ok(st) = serde_json::from_str::<UiState>(&text)
        {
            app.show_size = st.show_size;
            app.show_savings = st.show_savings;
            app.show_original_size = st.show_original_size;
            app.show_best_tool = st.show_best_tool;
            app.col_original = st.col_original;
            app.col_best = st.col_best;
            if st.col_status > 0.0 {
                app.col_status = st.col_status;
            }
            if st.col_file > 0.0 {
                app.col_file = st.col_file;
            }
            if st.col_size > 0.0 {
                app.col_size = st.col_size;
            }
            if st.col_savings > 0.0 {
                app.col_savings = st.col_savings;
            }
            let mut order: Vec<Col> = st
                .col_order
                .iter()
                .filter_map(|n| col_from_name(n))
                .collect();
            for c in [
                Col::Status,
                Col::File,
                Col::Size,
                Col::Savings,
                Col::Original,
                Col::Best,
            ] {
                if !order.contains(&c) {
                    order.push(c);
                }
            }
            app.col_order = order;
            app.main_saved = st.main_window;
            app.prefs_saved = st.prefs_window;
            if std::env::var_os("IMAGEOPTIM_QA_INPUT").is_none()
                && let Some([_, _, w, _]) = st.main_window
            {
                app.table_width = w;
            }
        }
        imageoptim_core::cache::prune_scratch(Duration::from_secs(60 * 60 * 24 * 30));
        let paths = std::mem::take(&mut app.launch_paths);
        if !paths.is_empty() {
            app.add_paths(paths);
        }
        app
    }

    pub(super) fn is_busy(&self) -> bool {
        self.pending > 0 || self.active_scans > 0
    }

    fn capture_main_viewport(&mut self, ctx: &egui::Context) {
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            self.main_live = Some(rect);
        }
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.main_inner = Some(rect);
        }
    }

    fn build_ui_state_json(&self) -> String {
        let ui_state = UiState {
            show_size: self.show_size,
            show_savings: self.show_savings,
            show_original_size: self.show_original_size,
            show_best_tool: self.show_best_tool,
            col_original: self.col_original,
            col_best: self.col_best,
            col_order: self
                .col_order
                .iter()
                .map(|c| col_name(*c).to_owned())
                .collect(),
            col_status: self.col_status,
            col_file: self.col_file,
            col_size: self.col_size,
            col_savings: self.col_savings,
            main_window: valid_window_rect(self.main_live, self.main_inner).or(self.main_saved),
            prefs_window: valid_window_rect(self.prefs_live, self.prefs_inner).or(self.prefs_saved),
        };
        serde_json::to_string(&ui_state).unwrap_or_default()
    }

    fn flush_ui_state(&mut self, force: bool, ctx: Option<&egui::Context>) {
        let json = self.build_ui_state_json();
        if json == self.ui_state_json {
            return;
        }
        if force || self.ui_write_at.elapsed() > Duration::from_millis(400) {
            self.ui_state_json = json;
            self.ui_write_at = Instant::now();
            if !cfg!(test) {
                let _ = write_ui_state_to(
                    &Settings::config_dir().join("ui-state.json"),
                    &self.ui_state_json,
                );
            }
        } else if let Some(ctx) = ctx {
            // Schedule one trailing flush after resize/move dragging stops.
            ctx.request_repaint_after(Duration::from_millis(450));
        }
    }

    #[cfg(target_os = "macos")]
    fn queue_count(&self) -> usize {
        self.pending + self.active_scans
    }

    pub(super) fn selected_indices(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| self.selected.contains(&r.id))
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn row_by_id(&self, id: u64) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }

    fn visible_columns(&self) -> Vec<Col> {
        self.col_order
            .iter()
            .copied()
            .filter(|c| match c {
                Col::Status | Col::File => true,
                Col::Size => self.show_size,
                Col::Savings => self.show_savings,
                Col::Original => self.show_original_size,
                Col::Best => self.show_best_tool,
            })
            .collect()
    }

    fn col_width(&self, col: Col) -> f32 {
        match col {
            Col::Status => self.col_status,
            Col::File => self.col_file,
            Col::Size => self.col_size,
            Col::Savings => self.col_savings,
            Col::Original => self.col_original,
            Col::Best => self.col_best,
        }
    }

    fn set_col_width(&mut self, col: Col, w: f32) {
        match col {
            Col::Status => self.col_status = w,
            Col::File => self.col_file = w,
            Col::Size => self.col_size = w,
            Col::Savings => self.col_savings = w,
            Col::Original => self.col_original = w,
            Col::Best => self.col_best = w,
        }
    }

    pub(super) fn undo_menu_title(&self, redo: bool) -> String {
        let stack = if redo { &self.redo } else { &self.undo };
        let base = if redo {
            i18n::t("Redo", "重做")
        } else {
            i18n::t("Undo", "撤销")
        };
        for entry in stack.iter().rev() {
            if let Some(name) = entry.name {
                let zh = if name == "Add" { "添加" } else { "剪切" };
                return format!("{base} {}", i18n::t(name, zh));
            }
        }
        base.into()
    }

    // ----- file input -----------------------------------------------------

    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        let mut files = Vec::new();
        for path in paths {
            let path = path.canonicalize().unwrap_or(path);
            if path.is_dir() {
                let exts: Vec<String> = self
                    .settings
                    .extensions()
                    .into_iter()
                    .map(str::to_owned)
                    .collect();
                let tx = self.evt_tx.clone();
                let slots = self.scan_slots.clone();
                let deterministic = std::env::var_os("IMAGEOPTIM_QA_SORT").is_some();
                self.active_scans += 1;
                std::thread::spawn(move || {
                    let _slot = slots.acquire();
                    let mut batch = Vec::new();
                    let mut flush_limit = 16usize;
                    for entry in walkdir::WalkDir::new(&path).follow_links(false) {
                        let Ok(entry) = entry else { continue };
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        let ext = entry
                            .path()
                            .extension()
                            .map(|e| e.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if exts.contains(&ext) {
                            batch.push(entry.path().to_path_buf());
                            if !deterministic && batch.len() >= flush_limit {
                                let _ = tx.send(Evt::ScanBatch(std::mem::take(&mut batch)));
                                flush_limit = (flush_limit * 4).min(256);
                            }
                        }
                    }
                    if !batch.is_empty() {
                        let _ = tx.send(Evt::ScanBatch(batch));
                    }
                    let _ = tx.send(Evt::ScanFinished);
                });
            } else if path.is_file() {
                files.push(path);
            }
        }
        self.add_files(files);
    }

    fn add_files(&mut self, files: Vec<PathBuf>) {
        let mut files = files;
        if std::env::var_os("IMAGEOPTIM_QA_SORT").is_some() {
            files.sort();
        }
        let mut added = Vec::new();
        for path in files {
            let path = path.canonicalize().unwrap_or(path);
            if let Some(idx) = self.rows.iter().position(|r| r.path == path) {
                if !self.rows[idx].busy {
                    self.enqueue(idx);
                }
                continue;
            }
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let kind = imageoptim_core::ImageFile::read(&path)
                .map(|f| f.kind)
                .unwrap_or(FileKind::Unknown);
            if kind == FileKind::Unknown {
                continue;
            }
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let row = Row {
                id: self.next_id,
                path,
                name,
                kind,
                original_size: meta.len(),
                optimized_size: Some(meta.len()),
                status: RowStatus::Waiting,
                best_tool: None,
                revert: None,
                busy: false,
                done: false,
                failed: false,
                optimized: false,
                has_run_before: false,
            };
            self.next_id += 1;
            self.rows.push(row.clone());
            added.push(row);
            let idx = self.rows.len() - 1;
            self.enqueue(idx);
        }
        if !added.is_empty() {
            self.undo.push(UndoEntry {
                name: Some("Add"),
                action: UndoAction::Added(added),
            });
            self.redo.clear();
            if let Some((key, asc)) = self.sort {
                self.apply_sort(key, asc);
            }
        }
    }

    fn enqueue(&mut self, idx: usize) {
        if self.rows[idx].busy {
            return;
        }
        if let Ok(meta) = std::fs::metadata(&self.rows[idx].path) {
            let disk_len = meta.len();
            let row = &mut self.rows[idx];
            if row.has_run_before && row.optimized_size != Some(disk_len) {
                row.original_size = disk_len;
                row.optimized_size = Some(disk_len);
                row.revert = None;
                row.best_tool = None;
                row.optimized = false;
                row.has_run_before = false;
            } else {
                row.sync_disk_size(disk_len);
            }
        }
        let has_valid_revert = self.rows[idx].revert.as_ref().is_some_and(|p| p.is_file());
        if !has_valid_revert {
            self.rows[idx].revert = None;
        }
        {
            let row = &mut self.rows[idx];
            row.busy = true;
            row.done = false;
            row.failed = false;
            row.status = RowStatus::Waiting;
        }
        let flag = Arc::new(AtomicBool::new(false));
        {
            let row = &self.rows[idx];
            self.cancel
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(row.id, flag.clone());
            let _ = self.job_tx.send(JobRequest {
                id: row.id,
                path: row.path.clone(),
                settings: self.settings.clone(),
                cancel: flag,
                rerun: row.has_run_before,
                need_backup: !has_valid_revert,
            });
        }
        self.pending += 1;
    }

    fn apply_sort(&mut self, key: SortKey, ascending: bool) {
        self.sort = Some((key, ascending));
        self.rows.sort_by(|a, b| {
            let ord = match key {
                SortKey::Status => a.status_order().cmp(&b.status_order()),
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            };
            if ascending { ord } else { ord.reverse() }
        });
    }

    fn apply_reorder(&mut self, from: usize, to: usize) {
        if from < self.rows.len() && to < self.rows.len() && from != to {
            let row = self.rows.remove(from);
            self.rows.insert(to, row);
        }
    }

    fn move_column(&mut self, a: Col, b: Col) {
        if a == b {
            return;
        }
        if let (Some(pa), Some(pb)) = (
            self.col_order.iter().position(|c| *c == a),
            self.col_order.iter().position(|c| *c == b),
        ) {
            let item = self.col_order.remove(pa);
            self.col_order.insert(pb, item);
        }
    }

    fn resize_boundary(&mut self, left: Col, right: Col, d: f32) -> bool {
        match (col_range(left), col_range(right)) {
            (Some(l), Some(r)) => {
                let mut lw = self.col_width(left);
                let mut rw = self.col_width(right);
                let before = lw;
                adjust_pair(&mut lw, &mut rw, d, l, r);
                if lw != before {
                    self.set_col_width(left, lw);
                    self.set_col_width(right, rw);
                    true
                } else {
                    false
                }
            }
            (Some(l), None) => self.resize_single(left, d, l),
            (None, Some(r)) => self.resize_single(right, -d, r),
            _ => false,
        }
    }

    fn resize_single(&mut self, col: Col, d: f32, range: (f32, f32)) -> bool {
        let w = self.col_width(col);
        let nw = (w + d).clamp(range.0, range.1);
        if nw == w {
            return false;
        }
        let applied = nw - w;
        self.set_col_width(col, nw);
        if col != Col::File {
            self.col_file = (self.col_file - applied).clamp(100.0, 1000.0);
        }
        true
    }

    // ----- events ---------------------------------------------------------

    fn handle_events(&mut self) -> bool {
        let mut refresh_preview = false;
        while let Ok(evt) = self.evt_rx.try_recv() {
            match evt {
                Evt::Started(id) => {
                    if let Some(i) = self.row_by_id(id)
                        && self.rows[i].busy
                    {
                        self.rows[i].status = RowStatus::Inspecting;
                    }
                }
                Evt::Tool(id, tool) => {
                    if let Some(i) = self.row_by_id(id)
                        && self.rows[i].busy
                    {
                        self.rows[i].status = RowStatus::Running(tool);
                    }
                }
                Evt::Progress {
                    id,
                    optimized_size,
                    best_tool,
                } => {
                    if let Some(i) = self.row_by_id(id)
                        && self.rows[i].busy
                    {
                        self.rows[i].sync_disk_size(optimized_size);
                        if !best_tool.is_empty() {
                            self.rows[i].best_tool = Some(best_tool);
                        }
                    }
                }
                Evt::Finished(id, outcome) => {
                    self.pending = self.pending.saturating_sub(1);
                    self.cancel
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&id);
                    let Some(i) = self.row_by_id(id) else {
                        continue;
                    };
                    let mut beep_needed = false;
                    let row = &mut self.rows[i];
                    row.busy = false;
                    match outcome {
                        JobOutcome::Optimized(saved) => {
                            row.status = RowStatus::Done;
                            row.done = true;
                            row.optimized = true;
                            row.has_run_before = true;
                            row.optimized_size = Some(saved.optimized_size);
                            row.best_tool = saved.best_tool;
                            if saved.revert.is_some() {
                                row.revert = saved.revert;
                            }
                        }
                        JobOutcome::Noopt { original_size } => {
                            row.status = RowStatus::Noopt;
                            row.done = true;
                            row.has_run_before = true;
                            row.sync_disk_size(original_size);
                        }
                        JobOutcome::Cached => {
                            let disk_len = std::fs::metadata(&row.path)
                                .map(|m| m.len())
                                .unwrap_or_else(|_| {
                                    row.optimized_size.unwrap_or(row.original_size)
                                });
                            row.status = RowStatus::Noopt;
                            row.done = true;
                            row.has_run_before = true;
                            row.sync_disk_size(disk_len);
                        }
                        JobOutcome::Failed(msg) => {
                            beep_needed = msg.ends_with("failed to start");
                            let disk_len = std::fs::metadata(&row.path)
                                .map(|m| m.len())
                                .unwrap_or(row.original_size);
                            row.status = RowStatus::Failed(msg);
                            row.failed = true;
                            row.done = true;
                            row.optimized = false;
                            row.optimized_size = Some(disk_len);
                        }
                        JobOutcome::Cancelled => {
                            let disk_len = std::fs::metadata(&row.path)
                                .map(|m| m.len())
                                .unwrap_or(row.original_size);
                            row.status = RowStatus::Waiting;
                            row.sync_disk_size(disk_len);
                        }
                    }
                    if self.selected.contains(&id) {
                        refresh_preview = true;
                    }
                    if beep_needed {
                        self.beep();
                    }
                }
                Evt::ScanBatch(files) => self.add_files(files),
                Evt::ScanFinished => {
                    self.active_scans = self.active_scans.saturating_sub(1);
                }
            }
        }
        refresh_preview
    }

    // ----- actions --------------------------------------------------------

    pub(super) fn start_again(&mut self, optimized_only: bool) {
        let mut jobs = self.selected_indices();
        if jobs.len() == 1 {
            let row = &self.rows[jobs[0]];
            if row.busy || (optimized_only && !row.optimized) {
                self.selected.clear();
                jobs.clear();
            }
        } else if jobs.is_empty() {
            jobs = (0..self.rows.len()).collect();
        }
        let mut any = false;
        for i in jobs {
            if !self.rows[i].busy && (!optimized_only || self.rows[i].optimized) {
                self.enqueue(i);
                any = true;
            }
        }
        if !any {
            self.beep();
        }
    }

    pub(super) fn shutdown_workers(&self) {
        if let Ok(map) = self.cancel.lock() {
            for flag in map.values() {
                flag.store(true, Ordering::Relaxed);
            }
        }
        imageoptim_core::tools::kill_active_children();
    }

    pub(super) fn stop(&mut self, indices: &[usize]) {
        for &i in indices {
            if self.rows[i].busy && !self.rows[i].done {
                if let Some(flag) = self
                    .cancel
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&self.rows[i].id)
                {
                    flag.store(true, Ordering::Relaxed);
                }
                self.rows[i].status = RowStatus::Waiting;
                self.rows[i].busy = false;
            }
        }
    }

    pub(super) fn stop_selected(&mut self) {
        let indices = self.selected_indices();
        self.stop(&indices);
    }

    pub(super) fn revert_row(&mut self, i: usize) {
        let row = &self.rows[i];
        let Some(backup) = row.revert.clone() else {
            self.beep();
            return;
        };
        if row.busy {
            self.beep();
            return;
        }
        match optimize::revert(&row.path, &backup) {
            Ok(()) => {
                let row = &mut self.rows[i];
                row.status = RowStatus::Reverted;
                row.optimized = false;
                row.done = true;
                row.revert = None;
                row.optimized_size = Some(row.original_size);
                row.best_tool = None;
            }
            Err(_) => self.beep(),
        }
    }

    pub(super) fn revert_selected(&mut self) {
        for i in self.selected_indices() {
            self.revert_row(i);
        }
    }

    pub(super) fn remove_rows(&mut self, indices: &[usize], name: Option<&'static str>) {
        let mut removed = Vec::new();
        let mut indices = indices.to_vec();
        indices.sort_unstable();
        for i in indices.into_iter().rev() {
            if i < self.rows.len() {
                if let Some(flag) = self
                    .cancel
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&self.rows[i].id)
                {
                    flag.store(true, Ordering::Relaxed);
                }
                let row = self.rows.remove(i);
                self.selected.remove(&row.id);
                removed.push((i, row));
            }
        }
        if !removed.is_empty() {
            self.undo.push(UndoEntry {
                name,
                action: UndoAction::Removed(removed),
            });
            self.redo.clear();
        }
    }

    pub(super) fn remove_selected(&mut self) {
        let indices = self.selected_indices();
        self.remove_rows(&indices, None);
    }

    pub(super) fn cut_selected(&mut self, ctx: &egui::Context) {
        self.copy_paths(ctx);
        let indices = self.selected_indices();
        self.remove_rows(&indices, Some("Cut"));
    }

    pub(super) fn clear_complete(&mut self) {
        let indices: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.done)
            .map(|(i, _)| i)
            .collect();
        self.remove_rows(&indices, None);
    }

    pub(super) fn undo(&mut self) {
        if let Some(entry) = self.undo.pop() {
            match &entry.action {
                UndoAction::Added(rows) => {
                    let ids: BTreeSet<u64> = rows.iter().map(|r| r.id).collect();
                    self.rows.retain(|r| !ids.contains(&r.id));
                    self.selected.retain(|id| !ids.contains(id));
                }
                UndoAction::Removed(rows) => {
                    for (idx, row) in rows.iter() {
                        let idx = (*idx).min(self.rows.len());
                        self.rows.insert(idx, row.clone());
                    }
                }
            }
            self.redo.push(entry);
        }
    }

    pub(super) fn redo(&mut self) {
        if let Some(entry) = self.redo.pop() {
            match &entry.action {
                UndoAction::Added(rows) => {
                    for row in rows.iter() {
                        self.rows.push(row.clone());
                    }
                }
                UndoAction::Removed(rows) => {
                    let ids: BTreeSet<u64> = rows.iter().map(|(_, r)| r.id).collect();
                    self.rows.retain(|r| !ids.contains(&r.id));
                }
            }
            self.undo.push(entry);
        }
    }

    pub(super) fn copy_paths(&self, ctx: &egui::Context) {
        let sel = self.selected_indices();
        if sel.is_empty() {
            return;
        }
        #[cfg(target_os = "macos")]
        {
            let _ = ctx;
            let paths: Vec<PathBuf> = sel.into_iter().map(|i| self.rows[i].path.clone()).collect();
            crate::macos_open::copy_files(&paths);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let paths: Vec<String> = sel
                .into_iter()
                .map(|i| self.rows[i].path.to_string_lossy().into_owned())
                .collect();
            ctx.copy_text(paths.join("\n"));
        }
    }

    pub(super) fn copy_data_urls(&self, ctx: &egui::Context) {
        use base64::Engine as _;
        let mut total = 0usize;
        let mut urls = Vec::new();
        for i in self.selected_indices() {
            let row = &self.rows[i];
            if !row.done || row.failed {
                continue;
            }
            let Ok(data) = std::fs::read(&row.path) else {
                continue;
            };
            if data.len() > 100_000 {
                continue;
            }
            total += data.len();
            if total > 1_000_000 {
                break;
            }
            if let Some(mime) = row.kind.mime() {
                urls.push(format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(data)
                ));
            }
        }
        if !urls.is_empty() {
            ctx.copy_text(urls.join("\n"));
        }
    }

    pub(super) fn paste(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        {
            let _ = ctx;
            let paths = crate::macos_open::paste_paths();
            if !paths.is_empty() {
                self.add_paths(paths);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let mut text = String::new();
            ctx.input(|i| {
                for event in &i.events {
                    if let egui::Event::Paste(t) = event {
                        text.push_str(t);
                    }
                }
            });
            let paths: Vec<PathBuf> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(PathBuf::from)
                .collect();
            if !paths.is_empty() {
                self.add_paths(paths);
            }
        }
    }

    pub(super) fn refresh_quicklook_if_visible(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        {
            let _ = ctx;
            let sel = self.selected_indices();
            if !sel.is_empty() {
                let items: Vec<(PathBuf, String)> = sel
                    .iter()
                    .map(|&i| (self.rows[i].path.clone(), self.rows[i].name.clone()))
                    .collect();
                crate::macos_open::quicklook_update_if_visible(&items);
            }
        }
        #[cfg(not(target_os = "macos"))]
        if self.preview.is_some() {
            self.load_in_app_preview(ctx);
        }
    }

    fn move_selection(&mut self, step: isize, extend: bool, ctx: &egui::Context) {
        if self.rows.is_empty() {
            return;
        }
        let sel = self.selected_indices();
        let next = if sel.is_empty() {
            if step >= 0 { 0 } else { self.rows.len() - 1 }
        } else {
            let cur = if step >= 0 {
                sel.last().copied().unwrap_or(0)
            } else {
                sel.first().copied().unwrap_or(0)
            };
            (cur as isize + step).clamp(0, self.rows.len() as isize - 1) as usize
        };
        let id = self.rows[next].id;
        if extend {
            let anchor_idx = self
                .anchor
                .and_then(|a| self.row_by_id(a))
                .or_else(|| sel.first().copied())
                .unwrap_or(next);
            self.anchor = Some(self.rows[anchor_idx].id);
            let (a, b) = if anchor_idx <= next {
                (anchor_idx, next)
            } else {
                (next, anchor_idx)
            };
            self.selected.clear();
            for i in a..=b {
                self.selected.insert(self.rows[i].id);
            }
        } else {
            self.selected.clear();
            self.selected.insert(id);
            self.anchor = Some(id);
        }
        self.refresh_quicklook_if_visible(ctx);
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    fn load_in_app_preview(&mut self, ctx: &egui::Context) {
        let sel = self.selected_indices();
        let Some(&i) = sel.first() else {
            return;
        };
        let row = &self.rows[i];
        let cur_sz = row.optimized_size.unwrap_or(row.original_size);
        let mut info = format!("{} · {}", row.kind.label(), bytes(cur_sz));
        if row.optimized
            && let Some(pct) = row.percent_saved()
        {
            info.push_str(&format!(" (-{})", percent(pct)));
        }
        let mut preview = Preview {
            title: row.name.clone(),
            info,
            texture: None,
            text: None,
            path: row.path.clone(),
        };
        if row.kind == FileKind::Svg {
            preview.text = std::fs::read_to_string(&row.path).ok();
        } else if row.original_size < 80 * 1024 * 1024
            && let Ok(data) = std::fs::read(&row.path)
            && let Ok(img) = image::load_from_memory(&data)
        {
            let (w, h) = (img.width(), img.height());
            preview.info = format!("{w}×{h} · {}", preview.info);
            let rgba = img.to_rgba8();
            let color =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
            preview.texture = Some(ctx.load_texture(
                format!("preview-{}", row.id),
                color,
                egui::TextureOptions::LINEAR,
            ));
        }
        self.preview = Some(preview);
    }

    pub(super) fn preview_selected(&mut self, ctx: &egui::Context) {
        let sel = self.selected_indices();
        if sel.is_empty() {
            return;
        }
        #[cfg(target_os = "macos")]
        {
            let _ = ctx;
            let items: Vec<(PathBuf, String)> = sel
                .iter()
                .map(|&i| (self.rows[i].path.clone(), self.rows[i].name.clone()))
                .collect();
            crate::macos_open::quicklook(&items);
        }
        #[cfg(not(target_os = "macos"))]
        {
            if self
                .preview
                .as_ref()
                .is_some_and(|p| p.path == self.rows[sel[0]].path)
            {
                self.preview = None;
            } else {
                self.load_in_app_preview(ctx);
            }
        }
    }

    pub(super) fn beep(&self) {
        #[cfg(target_os = "macos")]
        crate::macos_open::beep();
    }

    // ----- keyboard -------------------------------------------------------

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let mut actions: Vec<&str> = Vec::new();
        ctx.input_mut(|i| {
            let cmd = egui::Modifiers::COMMAND;
            if i.consume_key(cmd, egui::Key::O) {
                actions.push("add");
            }
            if i.consume_key(cmd, egui::Key::R) {
                actions.push(if i.modifiers.alt {
                    "again_opt"
                } else {
                    "again"
                });
            }
            if i.consume_key(cmd, egui::Key::S) {
                actions.push("stop");
            }
            if i.consume_key(cmd, egui::Key::Comma) {
                actions.push("prefs");
            }
            if i.consume_key(cmd, egui::Key::A) {
                actions.push("select_all");
            }
            if i.consume_key(cmd, egui::Key::Z) {
                actions.push(if i.modifiers.shift { "redo" } else { "undo" });
            }
            if i.consume_key(cmd, egui::Key::C) {
                actions.push(if i.modifiers.alt { "dataurl" } else { "copy" });
            }
            if i.consume_key(cmd, egui::Key::X) {
                actions.push("cut");
            }
            if i.consume_key(cmd, egui::Key::V) {
                actions.push("paste");
            }
            if i.consume_key(cmd, egui::Key::Y) {
                actions.push("preview");
            }
            if i.consume_key(cmd, egui::Key::L) {
                actions.push("lossy");
            }
            if i.consume_key(cmd, egui::Key::Backspace) {
                actions.push(if i.modifiers.alt {
                    "clear_complete"
                } else {
                    "delete"
                });
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::F5) {
                actions.push(if i.modifiers.alt {
                    "again_opt"
                } else {
                    "again"
                });
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
                actions.push("preview");
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
            {
                actions.push("delete");
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                actions.push("close");
            }
            if !i.modifiers.command && i.key_pressed(egui::Key::ArrowUp) {
                actions.push(if i.modifiers.shift { "up_ext" } else { "up" });
            }
            if !i.modifiers.command && i.key_pressed(egui::Key::ArrowDown) {
                actions.push(if i.modifiers.shift {
                    "down_ext"
                } else {
                    "down"
                });
            }
            if !i.modifiers.command && i.key_pressed(egui::Key::Home) {
                actions.push(if i.modifiers.shift {
                    "home_ext"
                } else {
                    "home"
                });
            }
            if !i.modifiers.command && i.key_pressed(egui::Key::End) {
                actions.push(if i.modifiers.shift { "end_ext" } else { "end" });
            }
        });
        for action in actions {
            match action {
                "add" => self.browse_files(),
                "again" => self.start_again(false),
                "again_opt" => self.start_again(true),
                "stop" => self.stop_selected(),
                "prefs" => {
                    self.show_prefs = true;
                    self.prefs_tab = 0;
                }
                "select_all" => {
                    self.selected = self.rows.iter().map(|r| r.id).collect();
                    self.refresh_quicklook_if_visible(ctx);
                }
                "undo" => self.undo(),
                "redo" => self.redo(),
                "copy" => self.copy_paths(ctx),
                "dataurl" => self.copy_data_urls(ctx),
                "cut" => self.cut_selected(ctx),
                "paste" => self.paste(ctx),
                "lossy" => {
                    self.settings.lossy_enabled = !self.settings.lossy_enabled;
                    let _ = self.settings.save();
                }
                "delete" => self.remove_selected(),
                "clear_complete" => self.clear_complete(),
                "preview" => self.preview_selected(ctx),
                "up" => self.move_selection(-1, false, ctx),
                "up_ext" => self.move_selection(-1, true, ctx),
                "down" => self.move_selection(1, false, ctx),
                "down_ext" => self.move_selection(1, true, ctx),
                "home" => self.move_selection(-(self.rows.len() as isize), false, ctx),
                "home_ext" => self.move_selection(-(self.rows.len() as isize), true, ctx),
                "end" => self.move_selection(self.rows.len() as isize, false, ctx),
                "end_ext" => self.move_selection(self.rows.len() as isize, true, ctx),
                "close" => {
                    if self.preview.take().is_none() {
                        self.selected.clear();
                    }
                }
                _ => {}
            }
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn inject_qa_input(&mut self, raw: &mut egui::RawInput) {
        if self.qa_steps.is_empty() || self.qa_i >= self.qa_steps.len() {
            return;
        }
        if self.qa_wait > 0 {
            self.qa_wait -= 1;
            return;
        }
        let step = self.qa_steps[self.qa_i].clone();
        self.qa_i += 1;
        let mut parts = step.splitn(2, ':');
        let kind = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("");
        match kind {
            "wait" => self.qa_wait = arg.parse().unwrap_or(1),
            "move" | "down" | "up" => {
                let mut xy = arg.split(',');
                let x: f32 = xy.next().unwrap_or("0").parse().unwrap_or(0.0);
                let y: f32 = xy.next().unwrap_or("0").parse().unwrap_or(0.0);
                let pos = pos2(x, y);
                if kind == "move" {
                    raw.events.push(egui::Event::PointerMoved(pos));
                } else {
                    raw.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: kind == "down",
                        modifiers: egui::Modifiers::default(),
                    });
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.inject_qa_input(raw_input);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        self.capture_main_viewport(ctx);
        #[cfg(target_os = "macos")]
        if ctx.input(|i| i.viewport().close_requested()) {
            self.flush_ui_state(true, None);
            self.shutdown_workers();
            std::process::exit(0);
        }
        #[cfg(target_os = "macos")]
        {
            if crate::macos_open::take_about_request() {
                self.show_about = true;
            }
            let opened = crate::macos_open::take_paths();
            if !opened.is_empty() {
                self.add_paths(opened);
            }
        }
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.external_drag_started = false;
        }
        if let Some(system) = ctx.input(|i| i.raw.system_theme) {
            ctx.set_theme(system);
        }
        if self.handle_events() {
            self.refresh_quicklook_if_visible(ctx);
        }
        let busy = self.is_busy();
        if busy {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
        if self.was_busy && !busy {
            if self.quit_when_done {
                self.capture_main_viewport(ctx);
                self.flush_ui_state(true, None);
                self.shutdown_workers();
                #[cfg(target_os = "macos")]
                std::process::exit(0);
                #[cfg(not(target_os = "macos"))]
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.settings.bounce_dock {
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                    egui::UserAttentionType::Informational,
                ));
            }
        }
        self.was_busy = busy;

        if !self.qa_steps.is_empty() {
            if self.qa_i < self.qa_steps.len() || !self.qa_done_logged {
                ctx.request_repaint();
            }
            if self.qa_i >= self.qa_steps.len() && !self.qa_done_logged {
                self.qa_done_logged = true;
                let result = format!(
                    "order={:?} file={} rows={:?} cursor={:?}",
                    self.col_order
                        .iter()
                        .map(|c| col_name(*c))
                        .collect::<Vec<_>>(),
                    self.col_file,
                    self.rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
                    self.last_cursor
                );
                eprintln!("QA-RESULT {result}");
                if let Some(expect) = &self.qa_expect {
                    let missing: Vec<&str> = expect
                        .split(';')
                        .filter(|e| !e.is_empty() && !result.contains(e))
                        .collect();
                    if missing.is_empty() {
                        eprintln!("QA-PASS");
                    } else {
                        eprintln!("QA-FAIL missing={missing:?}");
                        std::process::exit(1);
                    }
                }
            }
        }
        self.main_live = ctx.input(|i| i.viewport().outer_rect);
        self.main_inner = ctx.input(|i| i.viewport().inner_rect);

        self.handle_keys(ctx);
        self.handle_drops(ctx);

        #[cfg(target_os = "macos")]
        for action in crate::macos_open::take_menu_actions() {
            self.dispatch_menu_action(&action, ctx);
        }

        #[cfg(not(target_os = "macos"))]
        {
            egui::Panel::top("menu")
                .exact_size(24.0)
                .show(root, |ui| self.menu_bar(ui));
        }

        egui::Panel::bottom("status")
            .exact_size(39.0)
            .show(root, |ui| self.bottom_bar(ui));

        egui::CentralPanel::no_frame().show(root, |ui| {
            self.table_ui(ui);
        });

        #[cfg(target_os = "macos")]
        let app_active = self.quiet || crate::macos_open::app_is_active();
        #[cfg(not(target_os = "macos"))]
        let app_active = true;
        if app_active {
            self.prefs_window(ctx);
            self.about_window(ctx);
        }
        self.preview_window(ctx);

        #[cfg(target_os = "macos")]
        {
            self.sync_menu_state();
            crate::macos_open::set_queue_count(self.queue_count());
        }

        let closing = ctx.input(|i| i.viewport().close_requested());
        if closing {
            self.shutdown_workers();
        }
        self.flush_ui_state(closing, Some(ctx));

        if !self.qa_steps.is_empty() {
            let cur = ctx.output(|o| o.cursor_icon);
            if cur != self.last_cursor {
                eprintln!("QA-CURSOR {cur:?}");
                self.last_cursor = cur;
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_ui_state(true, None);
        self.shutdown_workers();
    }
}

fn run_job(req: JobRequest, tx: &Sender<Evt>) {
    let _ = tx.send(Evt::Started(req.id));
    let cache = ResultsCache::open(&req.settings);
    let id = req.id;
    let mut on_tool = |tool: &str| {
        let _ = tx.send(Evt::Tool(id, tool.to_string()));
    };
    let mut on_progress = |optimized_size: u64, best_tool: &str| {
        let _ = tx.send(Evt::Progress {
            id,
            optimized_size,
            best_tool: best_tool.to_string(),
        });
    };
    let outcome = optimize::optimize_path_with_options(
        &req.path,
        &req.settings,
        &cache,
        &req.cancel,
        &mut on_tool,
        &mut on_progress,
        req.rerun,
        req.need_backup,
    );
    let _ = tx.send(Evt::Finished(id, outcome));
}

fn load_texture(ctx: &egui::Context, name: &str, png_bytes: &[u8]) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(png_bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::build(&egui::Context::default(), Vec::new())
    }

    fn row(name: &str) -> Row {
        Row {
            id: 0,
            path: PathBuf::from(name),
            name: name.to_owned(),
            kind: FileKind::Png,
            original_size: 100,
            optimized_size: Some(100),
            status: RowStatus::Waiting,
            best_tool: None,
            revert: None,
            busy: false,
            done: false,
            failed: false,
            optimized: false,
            has_run_before: false,
        }
    }

    #[test]
    fn row_reorder_moves_and_clamps() {
        let mut a = app();
        a.rows = vec![row("a"), row("b"), row("c")];
        a.apply_reorder(0, 2);
        assert_eq!(
            a.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        a.apply_reorder(9, 9);
        assert_eq!(a.rows.len(), 3);
    }

    #[test]
    fn column_reorder_maps_visible_to_persisted_order() {
        let mut a = app();
        a.move_column(Col::File, Col::Savings);
        assert_eq!(
            a.col_order,
            vec![
                Col::Status,
                Col::Size,
                Col::Savings,
                Col::File,
                Col::Original,
                Col::Best
            ]
        );
        a.move_column(Col::File, Col::File);
        assert_eq!(a.col_order.len(), 6);
    }

    #[test]
    fn resize_respects_upstream_ranges() {
        let mut a = app();
        a.col_file = 214.0;
        assert!(a.resize_boundary(Col::File, Col::Size, 60.0));
        assert_eq!(a.col_file, 274.0);
        assert!(!a.resize_boundary(Col::Size, Col::Savings, 50.0));
        assert!(a.resize_boundary(Col::File, Col::Size, 10_000.0));
        assert_eq!(a.col_file, 1000.0);
        assert!(a.resize_boundary(Col::Original, Col::Best, -100.0));
        assert_eq!(a.col_original, 60.0);
    }

    #[test]
    fn ui_state_roundtrips_window_and_columns() {
        let st = UiState {
            show_size: true,
            show_savings: false,
            show_original_size: true,
            show_best_tool: true,
            col_original: 90.0,
            col_best: 120.0,
            col_order: vec!["status".into(), "file".into(), "best".into()],
            col_status: 22.0,
            col_file: 300.0,
            col_size: 85.0,
            col_savings: 85.0,
            main_window: Some([10.0, 20.0, 700.0, 418.0]),
            prefs_window: Some([1.0, 2.0, 663.0, 376.0]),
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: UiState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.main_window, st.main_window);
        assert_eq!(back.col_order, st.col_order);
        assert_eq!(back.col_file, 300.0);
        let legacy: UiState = serde_json::from_str(
            r#"{"show_size":true,"show_savings":true,"show_original_size":false,"show_best_tool":false,"col_original":85,"col_best":85}"#,
        )
        .unwrap();
        assert!(legacy.col_order.is_empty());
        assert!(legacy.main_window.is_none());
    }

    #[test]
    fn arrow_navigation_and_shift_extend() {
        let ctx = egui::Context::default();
        let mut a = app();
        let mut r1 = row("a");
        r1.id = 1;
        let mut r2 = row("b");
        r2.id = 2;
        let mut r3 = row("c");
        r3.id = 3;
        a.rows = vec![r1, r2, r3];

        a.move_selection(1, false, &ctx);
        assert_eq!(a.selected_indices(), vec![0]);
        a.move_selection(1, false, &ctx);
        assert_eq!(a.selected_indices(), vec![1]);
        a.move_selection(1, true, &ctx);
        assert_eq!(a.selected_indices(), vec![1, 2]);
        a.move_selection(-10, false, &ctx);
        assert_eq!(a.selected_indices(), vec![0]);
    }

    #[test]
    fn single_selected_noopt_row_reruns_on_again() {
        let mut a = app();
        let dir = std::env::temp_dir().join(format!("io-again-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("noopt.png");
        std::fs::write(&file_path, vec![0u8; 512]).unwrap();

        let mut r = row("noopt.png");
        r.id = 7;
        r.path = file_path;
        r.done = true;
        r.optimized = false;
        r.has_run_before = true;
        r.status = RowStatus::Noopt;
        a.rows = vec![r];
        a.selected.insert(7);

        a.start_again(true);
        assert!(a.selected.is_empty());
        assert!(!a.rows[0].busy);

        a.selected.insert(7);
        a.start_again(false);
        assert!(a.selected.contains(&7));
        assert!(a.rows[0].busy);

        a.shutdown_workers();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rerun_noopt_preserves_cumulative_savings_and_original_revert() {
        let mut a = app();
        let dir = std::env::temp_dir().join(format!("io-app-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("img.png");
        let backup_path = dir.join("img.png.orig");
        std::fs::write(&file_path, vec![0u8; 8_000]).unwrap();
        std::fs::write(&backup_path, vec![0u8; 10_000]).unwrap();

        let mut r = row("img.png");
        r.id = 1;
        r.path = file_path.clone();
        r.original_size = 10_000;
        r.optimized_size = Some(8_000);
        r.optimized = true;
        r.done = true;
        r.has_run_before = true;
        r.revert = Some(backup_path.clone());
        r.status = RowStatus::Done;
        a.rows = vec![r];

        a.pending = 1;
        a.rows[0].busy = true;
        let _ = a.evt_tx.send(Evt::Finished(
            1,
            JobOutcome::Noopt {
                original_size: 8_000,
            },
        ));
        a.handle_events();

        assert_eq!(a.rows[0].original_size, 10_000);
        assert_eq!(a.rows[0].optimized_size, Some(8_000));
        assert!(a.rows[0].optimized);
        assert_eq!(a.rows[0].revert.as_deref(), Some(backup_path.as_path()));

        a.shutdown_workers();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ui_state_creates_missing_parent_dir_and_preserves_saved_window_when_minimized() {
        let dir = std::env::temp_dir().join(format!("io-ui-state-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let state_path = dir.join("nested").join("ui-state.json");

        let mut a = app();
        a.main_saved = Some([120.0, 90.0, 820.0, 540.0]);
        // Simulates minimized or pre-layout state: live rects absent, saved rect preserved.
        let json = a.build_ui_state_json();
        write_ui_state_to(&state_path, &json).unwrap();

        let back: UiState = serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap())
            .expect("valid ui-state.json");
        assert_eq!(back.main_window, Some([120.0, 90.0, 820.0, 540.0]));

        // Zero or tiny rects are rejected cleanly.
        let tiny = egui::Rect::from_min_size(pos2(10.0, 20.0), egui::vec2(0.0, 0.0));
        assert_eq!(valid_window_rect(Some(tiny), Some(tiny)), None);
        let valid_outer = egui::Rect::from_min_size(pos2(10.0, 20.0), egui::vec2(700.0, 440.0));
        let valid_inner = egui::Rect::from_min_size(pos2(10.0, 48.0), egui::vec2(700.0, 412.0));
        assert_eq!(
            valid_window_rect(Some(valid_outer), Some(valid_inner)),
            Some([10.0, 20.0, 700.0, 412.0])
        );

        a.shutdown_workers();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
