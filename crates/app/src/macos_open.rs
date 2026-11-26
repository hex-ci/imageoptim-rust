//! Installs a Core AppleEvent handler for "Open With ImageOptim" / Finder drops
//! onto the app icon. Paths are pushed into a Rust queue polled by the UI.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

fn cstr_lossy(s: &str) -> CString {
    if let Ok(c) = CString::new(s) {
        return c;
    }
    let clean: String = s.chars().filter(|&c| c != '\0').collect();
    CString::new(clean).unwrap_or_default()
}

static OPEN_QUEUE: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();
static SHOW_ABOUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn queue() -> &'static Mutex<Vec<PathBuf>> {
    OPEN_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

extern "C" fn on_open(path: *const c_char) {
    if path.is_null() {
        return;
    }
    let path = unsafe { CStr::from_ptr(path) };
    if let Ok(path) = path.to_str() {
        queue()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PathBuf::from(path));
        if let Some(ctx) = CONTEXT.get() {
            ctx.request_repaint();
        }
    }
}

unsafe extern "C" {
    fn imageoptim_install_open_handler(callback: extern "C" fn(*const c_char));
    fn imageoptim_install_about_handler(callback: extern "C" fn());
    fn imageoptim_set_app_menu_titles(
        about: *const c_char,
        services: *const c_char,
        hide: *const c_char,
        hide_others: *const c_char,
        show_all: *const c_char,
        quit: *const c_char,
    );
}

extern "C" fn on_about() {
    SHOW_ABOUT.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

pub fn set_context(ctx: egui::Context) {
    let _ = CONTEXT.set(ctx);
}

pub fn install() {
    unsafe {
        imageoptim_install_open_handler(on_open);
        imageoptim_install_about_handler(on_about);
    }
    install_menu();
    let s = |en: &'static str, cn: &'static str| cstr_lossy(&crate::i18n::t_app(en, cn));
    let about = s("About ImageOptim", "关于 ImageOptim");
    let services = s("Services", "服务");
    let hide = s("Hide ImageOptim", "隐藏 ImageOptim");
    let hide_others = s("Hide Others", "隐藏其他");
    let show_all = s("Show All", "全部显示");
    let quit = s("Quit ImageOptim", "退出 ImageOptim");
    unsafe {
        imageoptim_set_app_menu_titles(
            about.as_ptr(),
            services.as_ptr(),
            hide.as_ptr(),
            hide_others.as_ptr(),
            show_all.as_ptr(),
            quit.as_ptr(),
        );
    }
}

pub fn take_about_request() -> bool {
    SHOW_ABOUT.swap(false, std::sync::atomic::Ordering::Relaxed)
}

pub fn take_paths() -> Vec<PathBuf> {
    queue()
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Native main menu
// ---------------------------------------------------------------------------

static MENU_QUEUE: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn menu_queue() -> &'static Mutex<Vec<String>> {
    MENU_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

extern "C" fn on_menu_action(id: *const c_char) {
    if id.is_null() {
        return;
    }
    if let Ok(id) = unsafe { CStr::from_ptr(id) }.to_str() {
        menu_queue()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(id.to_owned());
        if let Some(ctx) = CONTEXT.get() {
            ctx.request_repaint();
        }
    }
}

/// Mirrors `IOMenuState` in macos/open_files.m.
#[repr(C)]
pub struct MenuState {
    pub rows: i32,
    pub selected: i32,
    pub busy: i32,
    pub againable: i32,
    pub again_optable: i32,
    pub stoppable: i32,
    pub revertable: i32,
    pub any_done: i32,
    pub dataurl_ok: i32,
    pub can_undo: i32,
    pub can_redo: i32,
    pub lossy: i32,
    pub zopfli: i32,
    pub pngout: i32,
    pub oxipng: i32,
    pub advpng: i32,
    pub pngcrush: i32,
    pub jpegoptim: i32,
    pub jpegtran: i32,
    pub gifsicle: i32,
    pub col_original: i32,
    pub col_size: i32,
    pub col_savings: i32,
    pub col_best: i32,
    pub undo_title: *const c_char,
    pub redo_title: *const c_char,
}

unsafe extern "C" {
    fn imageoptim_install_menu(titles_tsv: *const c_char, callback: extern "C" fn(*const c_char));
    fn imageoptim_set_menu_state(state: *const MenuState);
    fn imageoptim_quicklook_toggle(
        paths: *const *const c_char,
        titles: *const *const c_char,
        count: i32,
    );
    fn imageoptim_quicklook_update_if_visible(
        paths: *const *const c_char,
        titles: *const *const c_char,
        count: i32,
    );
    fn imageoptim_set_queue_count(count: i32);
    fn imageoptim_copy_files(paths: *const *const c_char, count: i32);
    fn imageoptim_paste_paths() -> *mut c_char;
    fn imageoptim_browse_files(exts_csv: *const c_char) -> *mut c_char;
    fn imageoptim_free_string(s: *mut c_char);
}

pub fn install_menu() {
    let s = |en: &'static str, zh: &'static str| crate::i18n::t_app(en, zh);
    let entries: [(&str, String); 40] = [
        ("prefs", s("Preferences…", "首选项...")),
        ("webapi", s("Web API…", "Web API…")),
        ("menu_file", s("File", "文件")),
        ("add", s("Add Files…", "添加文件...")),
        ("again", s("Optimize Again", "再次优化")),
        ("again_opt", s("Optimize Optimized", "优化已优化的")),
        ("close", s("Close Window", "关闭窗口")),
        ("stop", s("Stop", "停止")),
        ("revert", s("Revert to Original", "恢复原状")),
        ("quicklook", s("Quick Look", "预览")),
        ("reveal", s("Show in Finder", "在 Finder 中显示")),
        ("menu_edit", s("Edit", "编辑")),
        ("undo", s("Undo", "撤销")),
        ("redo", s("Redo", "重做")),
        ("cut", s("Cut", "剪切")),
        ("copy", s("Copy", "复制")),
        ("dataurl", s("Copy as Data URL", "复制 Data URL")),
        ("paste", s("Paste", "粘贴")),
        ("delete", s("Delete", "删除")),
        ("delete_complete", s("Delete Completed", "删除已完成")),
        ("select_all", s("Select All", "全选")),
        ("menu_tools", s("Tools", "工具")),
        ("quality100", s("Quality: 100%", "质量：100%")),
        ("lossy", s("Lossy minification", "有损压缩")),
        ("menu_window", s("Window", "窗口")),
        ("minimize", s("Minimize", "最小化")),
        ("zoom", s("Zoom", "缩放")),
        ("front", s("Bring All to Front", "前置全部窗口")),
        ("menu_columns", s("Show Columns", "显示的列")),
        ("col_original", s("Original Size", "原始大小")),
        ("col_size", s("Optimized Size", "优化的大小")),
        ("col_savings", s("Savings", "已节省")),
        ("col_best", s("Best tool", "最好的工具")),
        ("menu_help", s("Help", "帮助")),
        ("help", s("ImageOptim Help", "ImageOptim 帮助")),
        ("website", s("ImageOptim Website", "ImageOptim 网站")),
        ("donate", s("Donate", "捐赠")),
        ("source", s("View Source", "查看源码")),
        ("menu_app", "ImageOptim-Rust".to_owned()),
        ("about", s("About ImageOptim", "关于 ImageOptim")),
    ];
    let tsv = entries
        .iter()
        .map(|(k, v)| format!("{k}\t{v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tsv = cstr_lossy(&tsv);
    unsafe { imageoptim_install_menu(tsv.as_ptr(), on_menu_action) };
}

pub fn take_menu_actions() -> Vec<String> {
    menu_queue()
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

pub fn set_menu_state(mut state: MenuState, undo_title: &str, redo_title: &str) {
    let undo = cstr_lossy(undo_title);
    let redo = cstr_lossy(redo_title);
    state.undo_title = undo.as_ptr();
    state.redo_title = redo.as_ptr();
    unsafe { imageoptim_set_menu_state(&state) };
}

fn c_path_and_title_vecs(items: &[(PathBuf, String)]) -> (Vec<CString>, Vec<CString>) {
    let mut paths = Vec::with_capacity(items.len());
    let mut titles = Vec::with_capacity(items.len());
    for (p, t) in items {
        if let Ok(cp) = CString::new(p.as_os_str().as_encoded_bytes()) {
            paths.push(cp);
            titles.push(cstr_lossy(t));
        }
    }
    (paths, titles)
}

pub fn quicklook(items: &[(PathBuf, String)]) {
    let (paths, titles) = c_path_and_title_vecs(items);
    let path_ptrs: Vec<*const c_char> = paths.iter().map(|p| p.as_ptr()).collect();
    let title_ptrs: Vec<*const c_char> = titles.iter().map(|t| t.as_ptr()).collect();
    unsafe {
        imageoptim_quicklook_toggle(
            path_ptrs.as_ptr(),
            title_ptrs.as_ptr(),
            path_ptrs.len() as i32,
        );
    }
}

pub fn quicklook_update_if_visible(items: &[(PathBuf, String)]) {
    let (paths, titles) = c_path_and_title_vecs(items);
    let path_ptrs: Vec<*const c_char> = paths.iter().map(|p| p.as_ptr()).collect();
    let title_ptrs: Vec<*const c_char> = titles.iter().map(|t| t.as_ptr()).collect();
    unsafe {
        imageoptim_quicklook_update_if_visible(
            path_ptrs.as_ptr(),
            title_ptrs.as_ptr(),
            path_ptrs.len() as i32,
        );
    }
}

pub fn set_queue_count(count: usize) {
    unsafe { imageoptim_set_queue_count(count as i32) };
}

pub fn copy_files(paths: &[PathBuf]) {
    let cpaths: Vec<std::ffi::CString> = paths
        .iter()
        .filter_map(|p| std::ffi::CString::new(p.to_string_lossy().as_bytes()).ok())
        .collect();
    let ptrs: Vec<*const c_char> = cpaths.iter().map(|p| p.as_ptr()).collect();
    unsafe { imageoptim_copy_files(ptrs.as_ptr(), ptrs.len() as i32) };
}

pub fn paste_paths() -> Vec<PathBuf> {
    let ptr = unsafe { imageoptim_paste_paths() };
    if ptr.is_null() {
        return Vec::new();
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { imageoptim_free_string(ptr) };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn browse_files(extensions: &[&str]) -> Option<Vec<PathBuf>> {
    let csv = std::ffi::CString::new(extensions.join(",")).ok()?;
    let ptr = unsafe { imageoptim_browse_files(csv.as_ptr()) };
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { imageoptim_free_string(ptr) };
    let out: Vec<PathBuf> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect();
    (!out.is_empty()).then_some(out)
}

/// Whether the app is active (for upstream hidesOnDeactivate windows).
pub fn app_is_active() -> bool {
    unsafe extern "C" {
        fn imageoptim_app_is_active() -> bool;
    }
    unsafe { imageoptim_app_is_active() }
}

/// Moves a file to the Trash, returning its new location (upstream revert source).
pub fn trash_file(path: &std::path::Path) -> Option<std::path::PathBuf> {
    unsafe extern "C" {
        fn imageoptim_trash_file(path: *const c_char, out: *mut c_char, out_len: i32) -> bool;
    }
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut buf = vec![0u8; 4096];
    let ok =
        unsafe { imageoptim_trash_file(c.as_ptr(), buf.as_mut_ptr().cast(), buf.len() as i32) };
    if !ok {
        return None;
    }
    let s = unsafe { CStr::from_ptr(buf.as_ptr().cast()) }
        .to_str()
        .ok()?;
    Some(std::path::PathBuf::from(s))
}

/// Upstream uses NSBeep() for failed revert / no-op Again / missing tools.
pub fn beep() {
    unsafe extern "C" {
        fn imageoptim_beep();
    }
    unsafe { imageoptim_beep() };
}

/// Reveal in Finder via ARC ObjC. The `showfile` crate's macOS path
/// double-autoreleases an NSArray and segfaults at autorelease-pool pop.
pub fn reveal_file(path: &std::path::Path) {
    unsafe extern "C" {
        fn imageoptim_reveal_file(path: *const c_char);
    }
    if let Ok(c) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) {
        unsafe { imageoptim_reveal_file(c.as_ptr()) };
    }
}

/// Initiates a native macOS `NSDraggingSession` so table rows can be dragged
/// directly out to Finder, browsers, editors, Mail, or the Dock.
pub fn begin_file_drag(paths: &[PathBuf]) -> bool {
    unsafe extern "C" {
        fn imageoptim_begin_file_drag(paths: *const *const c_char, count: i32) -> bool;
    }
    let cpaths: Vec<std::ffi::CString> = paths
        .iter()
        .filter_map(|p| std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).ok())
        .collect();
    if cpaths.is_empty() {
        return false;
    }
    let ptrs: Vec<*const c_char> = cpaths.iter().map(|p| p.as_ptr()).collect();
    unsafe { imageoptim_begin_file_drag(ptrs.as_ptr(), ptrs.len() as i32) }
}
