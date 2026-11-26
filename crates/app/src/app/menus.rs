use eframe::egui;

use super::App;
use super::state::{Row, reveal_in_file_manager, reveal_label};
use super::windows::open_help;
use crate::i18n;

pub(super) const REPO_URL: &str = "https://github.com/hex-ci/imageoptim-rust";

impl App {
    /// Window ▸ Show Columns / header right-click menu.
    pub(super) fn columns_menu(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(
            &mut self.show_original_size,
            i18n::t("Original Size", "原始大小"),
        );
        ui.checkbox(&mut self.show_size, i18n::t("Optimized Size", "优化的大小"));
        ui.checkbox(&mut self.show_savings, i18n::t("Savings", "已节省"));
        ui.checkbox(&mut self.show_best_tool, i18n::t("Best tool", "最好的工具"));
    }

    /// Shared dispatch for the native macOS menu, the in-app menu bar, and
    /// keyboard shortcuts.
    pub(super) fn dispatch_menu_action(&mut self, action: &str, ctx: &egui::Context) {
        match action {
            "add" => self.browse_files(),
            "again" => self.start_again(false),
            "again_opt" => self.start_again(true),
            "stop" => self.stop_selected(),
            "prefs" => {
                self.show_prefs = true;
                self.prefs_tab = 0;
            }
            "about" => self.show_about = true,
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
            "delete" => self.remove_selected(),
            "delete_complete" | "clear_complete" => self.clear_complete(),
            "quicklook" | "preview" => self.preview_selected(ctx),
            "reveal" => {
                for i in self.selected_indices() {
                    reveal_in_file_manager(&self.rows[i].path);
                }
            }
            "revert" => self.revert_selected(),
            "lossy" => {
                self.settings.lossy_enabled = !self.settings.lossy_enabled;
                let _ = self.settings.save();
            }
            "quality100" => {
                self.settings.lossy_enabled = false;
                let _ = self.settings.save();
            }
            "close" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "webapi" => {
                let _ = open::that_detached("https://imageoptim.com/app-api");
            }
            "help" => open_help("index.html"),
            "website" | "source" => {
                let _ = open::that_detached(REPO_URL);
            }
            "donate" => {
                let _ = open::that_detached("https://imageoptim.com/donate.html");
            }
            _ => {
                if let Some(tool) = action.strip_prefix("tool_") {
                    self.toggle_tool(tool);
                } else if let Some(col) = action.strip_prefix("col_") {
                    match col {
                        "original" => self.show_original_size = !self.show_original_size,
                        "size" => self.show_size = !self.show_size,
                        "savings" => self.show_savings = !self.show_savings,
                        "best" => self.show_best_tool = !self.show_best_tool,
                        _ => {}
                    }
                }
            }
        }
    }

    pub(super) fn toggle_tool(&mut self, tool: &str) {
        if tool == "guetzli" {
            let on = !self.settings.guetzli_enabled;
            self.set_guetzli(on);
            return;
        }
        let s = &mut self.settings;
        match tool {
            "zopfli" => s.zopfli_enabled = !s.zopfli_enabled,
            "pngout" => s.pngout_enabled = !s.pngout_enabled,
            "oxipng" => s.oxipng_enabled = !s.oxipng_enabled,
            "advpng" => s.advpng_enabled = !s.advpng_enabled,
            "pngcrush" => s.pngcrush_enabled = !s.pngcrush_enabled,
            "jpegoptim" => s.jpegoptim_enabled = !s.jpegoptim_enabled,
            "jpegtran" => s.jpegtran_enabled = !s.jpegtran_enabled,
            "gifsicle" => s.gifsicle_enabled = !s.gifsicle_enabled,
            "svgo" => s.svgo_enabled = !s.svgo_enabled,
            "svgcleaner" => s.svgcleaner_enabled = !s.svgcleaner_enabled,
            _ => return,
        }
        let _ = self.settings.save();
    }

    /// Upstream Guetzli interlock (`PrefsController.m`): warn once, force JPEG
    /// quality >= 85 and strip-all; restore strip-all when Guetzli is disabled.
    pub(super) fn set_guetzli(&mut self, on: bool) {
        if on == self.settings.guetzli_enabled {
            return;
        }
        if on {
            if !self.guetzli_warned {
                self.guetzli_warn_pending = true;
                self.guetzli_warned = true;
            }
            if self.settings.jpeg_quality < 85 {
                self.settings.jpeg_quality = 85;
            }
            if !self.settings.strip_jpeg_metadata {
                self.strip_jpeg_before_guetzli = Some(false);
                self.settings.strip_jpeg_metadata = true;
            }
        } else if let Some(prev) = self.strip_jpeg_before_guetzli.take() {
            self.settings.strip_jpeg_metadata = prev;
        }
        self.settings.guetzli_enabled = on;
        let _ = self.settings.save();
    }

    #[cfg(target_os = "macos")]
    pub(super) fn sync_menu_state(&mut self) {
        use crate::macos_open::MenuState;
        let sel = self.selected_indices();
        let sel_rows: Vec<&Row> = sel.iter().map(|&i| &self.rows[i]).collect();
        let scope: Vec<&Row> = if sel_rows.is_empty() {
            self.rows.iter().collect()
        } else {
            sel_rows.clone()
        };
        let b = |v: bool| v as i32;
        let state = MenuState {
            rows: self.rows.len() as i32,
            selected: sel.len() as i32,
            busy: b(self.is_busy()),
            againable: b(scope.iter().any(|r| !r.busy)),
            again_optable: b(scope.iter().any(|r| !r.busy && r.optimized)),
            stoppable: b(self.is_busy() && scope.iter().any(|r| r.busy)),
            revertable: b(sel_rows
                .iter()
                .any(|r| r.revert.is_some() && r.done && !r.busy)),
            any_done: b(self.rows.iter().any(|r| r.done)),
            dataurl_ok: b(sel_rows.iter().any(|r| {
                r.done && !r.failed && r.optimized_size.unwrap_or(r.original_size) <= 100_000
            })),
            can_undo: b(!self.undo.is_empty()),
            can_redo: b(!self.redo.is_empty()),
            lossy: b(self.settings.lossy_enabled),
            zopfli: b(self.settings.zopfli_enabled),
            pngout: b(self.settings.pngout_enabled),
            oxipng: b(self.settings.oxipng_enabled),
            advpng: b(self.settings.advpng_enabled),
            pngcrush: b(self.settings.pngcrush_enabled),
            jpegoptim: b(self.settings.jpegoptim_enabled),
            jpegtran: b(self.settings.jpegtran_enabled),
            gifsicle: b(self.settings.gifsicle_enabled),
            col_original: b(self.show_original_size),
            col_size: b(self.show_size),
            col_savings: b(self.show_savings),
            col_best: b(self.show_best_tool),
            undo_title: std::ptr::null(),
            redo_title: std::ptr::null(),
        };
        let sig = vec![
            state.rows,
            state.selected,
            state.busy,
            state.againable,
            state.again_optable,
            state.stoppable,
            state.revertable,
            state.any_done,
            state.dataurl_ok,
            state.can_undo,
            state.can_redo,
            state.lossy,
            state.zopfli,
            state.pngout,
            state.oxipng,
            state.advpng,
            state.pngcrush,
            state.jpegoptim,
            state.jpegtran,
            state.gifsicle,
            state.col_original,
            state.col_size,
            state.col_savings,
            state.col_best,
        ];
        let undo_title = self.undo_menu_title(false);
        let redo_title = self.undo_menu_title(true);
        let snapshot = (sig, undo_title.clone(), redo_title.clone());
        if self.menu_snapshot.as_ref() != Some(&snapshot) {
            self.menu_snapshot = Some(snapshot);
            crate::macos_open::set_menu_state(state, &undo_title, &redo_title);
        }
    }

    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        use egui::MenuBar;
        let sel = self.selected_indices();
        let sel_rows: Vec<&Row> = sel.iter().map(|&i| &self.rows[i]).collect();
        let scope: Vec<&Row> = if sel_rows.is_empty() {
            self.rows.iter().collect()
        } else {
            sel_rows.clone()
        };
        let has_rows = !self.rows.is_empty();
        let has_sel = !sel_rows.is_empty();
        let againable = scope.iter().any(|r| !r.busy);
        let again_optable = scope.iter().any(|r| !r.busy && r.optimized);
        let stoppable = self.is_busy() && scope.iter().any(|r| r.busy);
        let revertable = sel_rows
            .iter()
            .any(|r| r.revert.is_some() && r.done && !r.busy);
        let any_done = self.rows.iter().any(|r| r.done);
        let dataurl_ok = sel_rows
            .iter()
            .any(|r| r.done && !r.failed && r.optimized_size.unwrap_or(r.original_size) <= 100_000);
        let can_undo = !self.undo.is_empty();
        let can_redo = !self.redo.is_empty();

        let mut cmd: Vec<&str> = Vec::new();
        let mut changed = false;
        let mut toggle_guetzli: Option<bool> = None;
        MenuBar::new().ui(ui, |ui| {
            ui.menu_button("ImageOptim-Rust", |ui| {
                if ui
                    .button(i18n::t_app("About ImageOptim", "关于 ImageOptim"))
                    .clicked()
                {
                    cmd.push("about");
                    ui.close();
                }
                if ui.button(i18n::t("Preferences…", "首选项...")).clicked() {
                    cmd.push("prefs");
                    ui.close();
                }
            });
            ui.menu_button(i18n::t("File", "文件"), |ui| {
                if ui.button(i18n::t("Add Files…", "添加文件...")).clicked() {
                    cmd.push("add");
                    ui.close();
                }
                if ui
                    .add_enabled(
                        againable,
                        egui::Button::new(i18n::t("Optimize Again", "再次优化")),
                    )
                    .clicked()
                {
                    cmd.push("again");
                    ui.close();
                }
                if ui
                    .add_enabled(
                        again_optable,
                        egui::Button::new(i18n::t("Optimize Optimized", "优化已优化的")),
                    )
                    .clicked()
                {
                    cmd.push("again_opt");
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(stoppable, egui::Button::new(i18n::t("Stop", "停止")))
                    .clicked()
                {
                    cmd.push("stop");
                    ui.close();
                }
                if ui
                    .add_enabled(
                        revertable,
                        egui::Button::new(i18n::t("Revert to Original", "恢复原状")),
                    )
                    .clicked()
                {
                    cmd.push("revert");
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(has_sel, egui::Button::new(i18n::t("Quick Look", "预览")))
                    .clicked()
                {
                    cmd.push("quicklook");
                    ui.close();
                }
                if ui
                    .add_enabled(has_sel, egui::Button::new(reveal_label()))
                    .clicked()
                {
                    cmd.push("reveal");
                    ui.close();
                }
            });
            ui.menu_button(i18n::t("Edit", "编辑"), |ui| {
                if ui
                    .add_enabled(can_undo, egui::Button::new(self.undo_menu_title(false)))
                    .clicked()
                {
                    cmd.push("undo");
                    ui.close();
                }
                if ui
                    .add_enabled(can_redo, egui::Button::new(self.undo_menu_title(true)))
                    .clicked()
                {
                    cmd.push("redo");
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(has_sel, egui::Button::new(i18n::t("Cut", "剪切")))
                    .clicked()
                {
                    cmd.push("cut");
                    ui.close();
                }
                if ui
                    .add_enabled(has_sel, egui::Button::new(i18n::t("Copy", "复制")))
                    .clicked()
                {
                    cmd.push("copy");
                    ui.close();
                }
                if ui
                    .add_enabled(
                        dataurl_ok,
                        egui::Button::new(i18n::t("Copy as Data URL", "复制 Data URL")),
                    )
                    .clicked()
                {
                    cmd.push("dataurl");
                    ui.close();
                }
                if ui.button(i18n::t("Paste", "粘贴")).clicked() {
                    cmd.push("paste");
                    ui.close();
                }
                if ui
                    .add_enabled(has_sel, egui::Button::new(i18n::t("Delete", "删除")))
                    .clicked()
                {
                    cmd.push("delete");
                    ui.close();
                }
                if ui
                    .add_enabled(
                        any_done,
                        egui::Button::new(i18n::t("Delete Completed", "删除已完成")),
                    )
                    .clicked()
                {
                    cmd.push("delete_complete");
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(has_rows, egui::Button::new(i18n::t("Select All", "全选")))
                    .clicked()
                {
                    cmd.push("select_all");
                    ui.close();
                }
            });
            ui.menu_button(i18n::t("Tools", "工具"), |ui| {
                let mut q100 = !self.settings.lossy_enabled;
                if ui
                    .checkbox(&mut q100, i18n::t("Quality: 100%", "质量：100%"))
                    .changed()
                {
                    self.settings.lossy_enabled = !q100;
                    changed = true;
                }
                changed |= ui
                    .checkbox(
                        &mut self.settings.lossy_enabled,
                        i18n::t("Lossy minification", "有损压缩"),
                    )
                    .changed();
                ui.separator();
                changed |= ui
                    .checkbox(&mut self.settings.zopfli_enabled, "Zopfli")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.pngout_enabled, "PNGOUT")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.oxipng_enabled, "OxiPNG")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.advpng_enabled, "AdvPNG")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.pngcrush_enabled, "PNGCrush")
                    .changed();
                ui.separator();
                changed |= ui
                    .checkbox(&mut self.settings.jpegoptim_enabled, "JPEGOptim")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.jpegtran_enabled, "Jpegtran")
                    .changed();
                let mut guetzli = self.settings.guetzli_enabled;
                if ui.checkbox(&mut guetzli, "Guetzli").changed() {
                    toggle_guetzli = Some(guetzli);
                }
                ui.separator();
                changed |= ui
                    .checkbox(&mut self.settings.gifsicle_enabled, "Gifsicle")
                    .changed();
                ui.separator();
                changed |= ui
                    .checkbox(&mut self.settings.svgo_enabled, "SVGO")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.settings.svgcleaner_enabled, "svgcleaner")
                    .changed();
            });
            ui.menu_button(i18n::t("Window", "窗口"), |ui| {
                ui.menu_button(i18n::t("Show Columns", "显示的列"), |ui| {
                    self.columns_menu(ui);
                });
            });
            ui.menu_button(i18n::t("Help", "帮助"), |ui| {
                if ui
                    .button(i18n::t_app("ImageOptim Help", "ImageOptim 帮助"))
                    .clicked()
                {
                    cmd.push("help");
                    ui.close();
                }
                if ui
                    .button(i18n::t_app("ImageOptim Website", "ImageOptim 网站"))
                    .clicked()
                {
                    cmd.push("website");
                    ui.close();
                }
                if ui.button(i18n::t("Donate", "捐赠")).clicked() {
                    cmd.push("donate");
                    ui.close();
                }
                ui.separator();
                if ui.button(i18n::t("View Source", "查看源码")).clicked() {
                    cmd.push("source");
                    ui.close();
                }
            });
        });
        if let Some(on) = toggle_guetzli {
            self.set_guetzli(on);
        }
        for c in cmd {
            self.dispatch_menu_action(c, ui.ctx());
        }
        if changed && self.settings.save().is_err() {
            log::warn!("could not save settings");
        }
    }
}
